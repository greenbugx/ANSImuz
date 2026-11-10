use crate::discord::activity::{PlaybackInfo, build_activity};
use crate::discord::client::DiscordClient;
use presenceforge::activity::Activity;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const INITIAL_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RECONNECT_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
enum StoredPresence {
    Playback(PlaybackInfo),
    Custom(Activity),
}

impl StoredPresence {
    fn to_activity(&self) -> Activity {
        match self {
            Self::Playback(info) => build_activity(info),
            Self::Custom(activity) => activity.clone(),
        }
    }
}

#[derive(Debug)]
pub enum DiscordCommand {
    UpdatePlayback(PlaybackInfo),
    SetActivity(Activity),
    ClearActivity,
    Reconnect,
    Shutdown,
}

pub struct DiscordWorker {
    sender: Sender<DiscordCommand>,
    thread_handle: Option<JoinHandle<()>>,
}

impl DiscordWorker {
    pub fn spawn(client_id: impl Into<String>) -> Self {
        let (sender, receiver) = channel();
        let client_id = client_id.into();

        let thread_handle = thread::Builder::new()
            .name("discord-rpc-worker".to_string())
            .spawn(move || {
                run_worker_loop(&client_id, receiver);
            })
            .expect("Failed to spawn Discord RPC worker thread");

        Self {
            sender,
            thread_handle: Some(thread_handle),
        }
    }

    pub fn sender(&self) -> &Sender<DiscordCommand> {
        &self.sender
    }

    pub fn update_playback(&self, info: PlaybackInfo) {
        let _ = self.sender.send(DiscordCommand::UpdatePlayback(info));
    }

    pub fn set_activity(&self, activity: Activity) {
        let _ = self.sender.send(DiscordCommand::SetActivity(activity));
    }

    pub fn clear_activity(&self) {
        let _ = self.sender.send(DiscordCommand::ClearActivity);
    }

    pub fn reconnect(&self) {
        let _ = self.sender.send(DiscordCommand::Reconnect);
    }

    pub fn shutdown(&mut self) {
        let _ = self.sender.send(DiscordCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DiscordWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_worker_loop(client_id: &str, receiver: Receiver<DiscordCommand>) {
    let mut client = DiscordClient::new(client_id);
    let mut connected = client.connect().is_ok();
    let mut current_presence: Option<StoredPresence> = None;

    let mut current_backoff = INITIAL_RECONNECT_INTERVAL;
    let mut next_reconnect_at: Option<Instant> = if connected {
        None
    } else {
        Some(Instant::now() + current_backoff)
    };

    loop {
        // Calculate timeout until next scheduled reconnect attempt
        let timeout = if !connected {
            if let Some(next_at) = next_reconnect_at {
                let now = Instant::now();
                if now >= next_at {
                    Duration::ZERO
                } else {
                    next_at - now
                }
            } else {
                Duration::ZERO
            }
        } else {
            // Idle wait when connected
            Duration::from_secs(10)
        };

        match receiver.recv_timeout(timeout) {
            Ok(cmd) => match cmd {
                DiscordCommand::UpdatePlayback(info) => {
                    let presence = StoredPresence::Playback(info);
                    apply_presence(
                        &mut client,
                        &mut connected,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        &presence,
                    );
                    current_presence = Some(presence);
                }
                DiscordCommand::SetActivity(activity) => {
                    let presence = StoredPresence::Custom(activity);
                    apply_presence(
                        &mut client,
                        &mut connected,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        &presence,
                    );
                    current_presence = Some(presence);
                }
                DiscordCommand::ClearActivity => {
                    current_presence = None;
                    if connected && client.clear_activity().is_err() {
                        mark_disconnected(
                            &mut client,
                            &mut connected,
                            &mut next_reconnect_at,
                            &mut current_backoff,
                        );
                    }
                }
                DiscordCommand::Reconnect => {
                    client.disconnect();
                    connected = false;
                    current_backoff = INITIAL_RECONNECT_INTERVAL;
                    next_reconnect_at = None;
                    try_reconnect(
                        &mut client,
                        &mut connected,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        current_presence.as_ref(),
                    );
                }
                DiscordCommand::Shutdown => {
                    if connected {
                        let _ = client.clear_activity();
                    }
                    client.disconnect();
                    break;
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                // Scheduled reconnect retry when disconnected
                if !connected {
                    try_reconnect(
                        &mut client,
                        &mut connected,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        current_presence.as_ref(),
                    );
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if connected {
                    let _ = client.clear_activity();
                }
                client.disconnect();
                break;
            }
        }
    }
}

fn apply_presence(
    client: &mut DiscordClient,
    connected: &mut bool,
    next_reconnect_at: &mut Option<Instant>,
    current_backoff: &mut Duration,
    presence: &StoredPresence,
) {
    if !*connected {
        let now = Instant::now();
        let ready_for_reconnect = next_reconnect_at.is_none_or(|next_at| now >= next_at);
        if ready_for_reconnect {
            try_reconnect(
                client,
                connected,
                next_reconnect_at,
                current_backoff,
                Some(presence),
            );
            return;
        }
    }

    if *connected {
        let activity = presence.to_activity();
        if client.set_activity(&activity).is_err() {
            mark_disconnected(client, connected, next_reconnect_at, current_backoff);
        }
    }
}

fn try_reconnect(
    client: &mut DiscordClient,
    connected: &mut bool,
    next_reconnect_at: &mut Option<Instant>,
    current_backoff: &mut Duration,
    presence: Option<&StoredPresence>,
) {
    client.disconnect();
    if client.connect().is_ok() {
        *connected = true;
        *current_backoff = INITIAL_RECONNECT_INTERVAL;
        *next_reconnect_at = None;

        if let Some(p) = presence {
            let activity = p.to_activity();
            if client.set_activity(&activity).is_err() {
                mark_disconnected(client, connected, next_reconnect_at, current_backoff);
            }
        }
    } else {
        mark_disconnected(client, connected, next_reconnect_at, current_backoff);
    }
}

fn mark_disconnected(
    client: &mut DiscordClient,
    connected: &mut bool,
    next_reconnect_at: &mut Option<Instant>,
    current_backoff: &mut Duration,
) {
    client.disconnect();
    *connected = false;
    *next_reconnect_at = Some(Instant::now() + *current_backoff);
    *current_backoff = (*current_backoff * 2).min(MAX_RECONNECT_INTERVAL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_lifecycle_and_channel_drain() {
        let mut worker = DiscordWorker::spawn("dummy_id");
        let track = PlaybackInfo::new("Title", "Artist");
        worker.update_playback(track);
        worker.clear_activity();
        worker.reconnect();
        worker.shutdown();
    }
}
