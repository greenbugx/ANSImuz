use crate::discord::activity::{PlaybackInfo, build_activity};
use crate::discord::artwork::worker::{ArtworkEvent, ArtworkWorker};
use crate::discord::artwork::types::{ArtworkError, ArtworkRequest, ArtworkResult};
use crate::discord::client::DiscordClient;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const INITIAL_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RECONNECT_INTERVAL: Duration = Duration::from_secs(60);

pub(crate) fn log_discord(msg: &str) {
    let log_dir = crate::config::cache_base_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("discord_rpc.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "[{secs}] {msg}");
    }
}

#[derive(Debug)]
pub enum DiscordCommand {
    TrackChanged(PlaybackInfo),
    PositionChanged(Duration),
    Paused,
    Resumed(Duration),
    Clear,
    Shutdown,
    #[doc(hidden)]
    ArtworkResolved {
        generation: u64,
        result: Result<ArtworkResult, ArtworkError>,
    },
}

pub struct DiscordWorker {
    sender: Sender<DiscordCommand>,
    connected: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl DiscordWorker {
    pub fn spawn(client_id: impl Into<String>) -> std::io::Result<Self> {
        let (sender, receiver) = channel();
        let client_id = client_id.into();
        let connected = Arc::new(AtomicBool::new(false));
        let connected_flag = Arc::clone(&connected);
        let internal_sender = sender.clone();

        let thread_handle = thread::Builder::new()
            .name("discord-rpc-worker".to_string())
            .spawn(move || {
                run_worker_loop(&client_id, receiver, connected_flag, internal_sender);
            })?;

        Ok(Self {
            sender,
            connected,
            thread_handle: Some(thread_handle),
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn sender(&self) -> &Sender<DiscordCommand> {
        &self.sender
    }

    pub fn track_changed(&self, info: PlaybackInfo) {
        let _ = self.sender.send(DiscordCommand::TrackChanged(info));
    }

    pub fn position_changed(&self, pos: Duration) {
        let _ = self.sender.send(DiscordCommand::PositionChanged(pos));
    }

    pub fn paused(&self) {
        let _ = self.sender.send(DiscordCommand::Paused);
    }

    pub fn resumed(&self, pos: Duration) {
        let _ = self.sender.send(DiscordCommand::Resumed(pos));
    }

    pub fn clear(&self) {
        let _ = self.sender.send(DiscordCommand::Clear);
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

fn run_worker_loop(
    client_id: &str,
    receiver: Receiver<DiscordCommand>,
    connected_flag: Arc<AtomicBool>,
    internal_sender: Sender<DiscordCommand>,
) {
    let mut client = DiscordClient::new(client_id);
    let mut connected = match client.connect() {
        Ok(()) => {
            log_discord("[Discord RPC] Connected");
            true
        }
        Err(err) => {
            log_discord(&format!("[Discord RPC] Initial connection failed: {err}"));
            false
        }
    };
    connected_flag.store(connected, Ordering::Relaxed);
    let mut current_playback: Option<PlaybackInfo> = None;
    let mut current_generation: u64 = 0;

    // Dedicated artwork worker keeping network operations off the playback path
    let (art_tx, art_rx) = channel::<ArtworkEvent>();
    let artwork_worker = match ArtworkWorker::spawn(art_tx) {
        Ok(w) => {
            let tx_clone = internal_sender;
            let _ = thread::Builder::new()
                .name("discord-artwork-forwarder".to_string())
                .spawn(move || {
                    while let Ok(ArtworkEvent::Resolved { generation, result }) = art_rx.recv() {
                        let _ = tx_clone.send(DiscordCommand::ArtworkResolved { generation, result });
                    }
                });
            Some(w)
        }
        Err(e) => {
            log_discord(&format!("[Discord RPC] Failed to spawn artwork worker: {e}"));
            None
        }
    };

    let mut current_backoff = INITIAL_RECONNECT_INTERVAL;
    let mut next_reconnect_at: Option<Instant> = if connected {
        None
    } else {
        Some(Instant::now() + current_backoff)
    };

    loop {
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
            Duration::from_secs(10)
        };

        match receiver.recv_timeout(timeout) {
            Ok(cmd) => match cmd {
                DiscordCommand::TrackChanged(info) => {
                    // Generation tracking
                    current_generation += 1;
                    let generation = current_generation;

                    log_discord(&format!(
                        "[Discord RPC] Track changed (gen {generation}): {} — {}",
                        info.title, info.artist
                    ));

                    // Publish initial activity immediately using default fallback asset ("logo")
                    apply_playback(
                        &mut client,
                        &mut connected,
                        &connected_flag,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        &info,
                    );

                    // Request artwork resolution in parallel without blocking
                    if let Some(ref art_url) = info.art_url
                        && !art_url.trim().is_empty()
                        && let Some(ref aw) = artwork_worker
                    {
                        let req = ArtworkRequest::new(
                            info.provider.clone().unwrap_or_else(|| "unknown".to_string()),
                            info.track_id.clone(),
                            art_url.clone(),
                        );
                        aw.resolve(generation, req);
                    }

                    current_playback = Some(info);
                }
                DiscordCommand::ArtworkResolved { generation, result } => {
                    // Only apply resolved artwork if generation matches currently playing track
                    if generation != current_generation {
                        log_discord(&format!(
                            "[Discord RPC] Ignoring stale artwork for gen {generation} (current: {current_generation})"
                        ));
                        continue;
                    }

                    match result {
                        Ok(art_res) => {
                            log_discord(&format!(
                                "[Discord RPC] Artwork resolved for gen {generation}: {}",
                                art_res.hosted_url
                            ));
                            if let Some(info) = &mut current_playback {
                                // Update the activity's large_image with the hosted URL and re-publish
                                info.large_image = Some(art_res.hosted_url);
                                apply_playback(
                                    &mut client,
                                    &mut connected,
                                    &connected_flag,
                                    &mut next_reconnect_at,
                                    &mut current_backoff,
                                    info,
                                );
                            }
                        }
                        Err(e) => {
                            log_discord(&format!(
                                "[Discord RPC] Artwork resolution failed for gen {generation}: {e}; keeping default asset"
                            ));
                        }
                    }
                }
                DiscordCommand::PositionChanged(pos) => {
                    if let Some(info) = &mut current_playback {
                        let old_pos = info.position.unwrap_or(Duration::ZERO);
                        info.position = Some(pos);
                        // Discord automatically client-side interpolates start/end timestamps.
                        // Only refresh activity if a seek caused a significant jump, i.e. greater than 2s
                        let seeked = pos.abs_diff(old_pos) > Duration::from_secs(2);
                        if seeked && !info.is_paused {
                            apply_playback(
                                &mut client,
                                &mut connected,
                                &connected_flag,
                                &mut next_reconnect_at,
                                &mut current_backoff,
                                info,
                            );
                        }
                    }
                }
                DiscordCommand::Paused => {
                    if let Some(info) = &mut current_playback
                        && !info.is_paused
                    {
                        info.is_paused = true;
                        apply_playback(
                            &mut client,
                            &mut connected,
                            &connected_flag,
                            &mut next_reconnect_at,
                            &mut current_backoff,
                            info,
                        );
                    }
                }
                DiscordCommand::Resumed(pos) => {
                    if let Some(info) = &mut current_playback {
                        info.is_paused = false;
                        if pos > Duration::ZERO {
                            info.position = Some(pos);
                        }
                        apply_playback(
                            &mut client,
                            &mut connected,
                            &connected_flag,
                            &mut next_reconnect_at,
                            &mut current_backoff,
                            info,
                        );
                    }
                }
                DiscordCommand::Clear => {
                    current_generation += 1;
                    current_playback = None;
                    if connected && client.clear_activity().is_err() {
                        mark_disconnected(
                            &mut client,
                            &mut connected,
                            &connected_flag,
                            &mut next_reconnect_at,
                            &mut current_backoff,
                        );
                    }
                }
                DiscordCommand::Shutdown => {
                    if let Some(mut aw) = artwork_worker {
                        aw.shutdown();
                    }
                    if connected {
                        let _ = client.clear_activity();
                    }
                    client.disconnect();
                    connected_flag.store(false, Ordering::Relaxed);
                    break;
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                if !connected {
                    try_reconnect(
                        &mut client,
                        &mut connected,
                        &connected_flag,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        current_playback.as_ref(),
                    );
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if connected {
                    let _ = client.clear_activity();
                }
                client.disconnect();
                connected_flag.store(false, Ordering::Relaxed);
                break;
            }
        }
    }
}

fn apply_playback(
    client: &mut DiscordClient,
    connected: &mut bool,
    connected_flag: &Arc<AtomicBool>,
    next_reconnect_at: &mut Option<Instant>,
    current_backoff: &mut Duration,
    playback: &PlaybackInfo,
) {
    if !*connected {
        return;
    }

    let activity = build_activity(playback);
    if let Err(err) = client.set_activity(&activity) {
        log_discord(&format!("[Discord RPC] Failed to set activity: {err}"));
        mark_disconnected(
            client,
            connected,
            connected_flag,
            next_reconnect_at,
            current_backoff,
        );
    }
}

fn try_reconnect(
    client: &mut DiscordClient,
    connected: &mut bool,
    connected_flag: &Arc<AtomicBool>,
    next_reconnect_at: &mut Option<Instant>,
    current_backoff: &mut Duration,
    playback: Option<&PlaybackInfo>,
) {
    log_discord("[Discord RPC] Attempting reconnection...");
    match client.connect() {
        Ok(()) => {
            log_discord("[Discord RPC] Reconnected successfully");
            *connected = true;
            connected_flag.store(true, Ordering::Relaxed);
            *next_reconnect_at = None;
            *current_backoff = INITIAL_RECONNECT_INTERVAL;

            if let Some(info) = playback {
                let activity = build_activity(info);
                if let Err(err) = client.set_activity(&activity) {
                    log_discord(&format!(
                        "[Discord RPC] Failed to restore activity on reconnect: {err}"
                    ));
                    mark_disconnected(
                        client,
                        connected,
                        connected_flag,
                        next_reconnect_at,
                        current_backoff,
                    );
                }
            }
        }
        Err(err) => {
            log_discord(&format!("[Discord RPC] Reconnection failed: {err}"));
            mark_disconnected(
                client,
                connected,
                connected_flag,
                next_reconnect_at,
                current_backoff,
            );
        }
    }
}

fn mark_disconnected(
    client: &mut DiscordClient,
    connected: &mut bool,
    connected_flag: &Arc<AtomicBool>,
    next_reconnect_at: &mut Option<Instant>,
    current_backoff: &mut Duration,
) {
    client.disconnect();
    *connected = false;
    connected_flag.store(false, Ordering::Relaxed);
    *next_reconnect_at = Some(Instant::now() + *current_backoff);
    *current_backoff = (*current_backoff * 2).min(MAX_RECONNECT_INTERVAL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_lifecycle_and_channel_drain() {
        let mut worker = DiscordWorker::spawn("dummy_id").expect("spawn worker");
        assert!(!worker.is_connected());
        let track = PlaybackInfo::new("Title", "Artist")
            .with_provider("ytmusic")
            .with_track_id("vid1")
            .with_art_url("https://example.com/art.jpg");
        worker.track_changed(track);
        worker.position_changed(Duration::from_secs(10));
        worker.paused();
        worker.resumed(Duration::from_secs(10));
        worker.clear();
        worker.shutdown();
    }

    #[test]
    fn test_worker_seek_and_pause_resume_commands() {
        let mut worker = DiscordWorker::spawn("dummy_id").expect("spawn worker");
        let track = PlaybackInfo::new("Title", "Artist")
            .with_position(Duration::from_secs(10))
            .with_duration(Duration::from_secs(300));
        worker.track_changed(track);

        // Small incremental tick (<= 2s should not trigger redundant seek)
        worker.position_changed(Duration::from_secs(11));

        // Forward seek (> 2s)
        worker.position_changed(Duration::from_secs(50));

        // Backward seek (> 2s)
        worker.position_changed(Duration::from_secs(20));

        // Pause
        worker.paused();

        // Position event while paused
        worker.position_changed(Duration::from_secs(20));

        // Resume at position
        worker.resumed(Duration::from_secs(20));

        worker.clear();
        worker.shutdown();
    }
}
