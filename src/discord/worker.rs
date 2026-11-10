use crate::discord::activity::{PlaybackInfo, build_activity};
use crate::discord::client::DiscordClient;
use presenceforge::activity::Activity;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

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

    while let Ok(cmd) = receiver.recv() {
        match cmd {
            DiscordCommand::UpdatePlayback(info) => {
                let activity = build_activity(&info);
                ensure_connected_and_set(&mut client, &mut connected, &activity);
            }
            DiscordCommand::SetActivity(activity) => {
                ensure_connected_and_set(&mut client, &mut connected, &activity);
            }
            DiscordCommand::ClearActivity => {
                if connected {
                    let _ = client.clear_activity();
                }
            }
            DiscordCommand::Reconnect => {
                client.disconnect();
                connected = client.connect().is_ok();
            }
            DiscordCommand::Shutdown => {
                if connected {
                    let _ = client.clear_activity();
                }
                client.disconnect();
                break;
            }
        }
    }
}

fn ensure_connected_and_set(client: &mut DiscordClient, connected: &mut bool, activity: &Activity) {
    if !*connected {
        *connected = client.connect().is_ok();
    }
    if *connected && client.set_activity(activity).is_err() {
        // If connection dropped or failed
        // then mark disconnected for retry on next command
        *connected = false;
    }
}
