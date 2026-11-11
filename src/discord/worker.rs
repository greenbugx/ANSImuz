use crate::config::DiscordConfig;
use crate::discord::activity::{PlaybackInfo, build_activity};
use crate::discord::artwork::types::{ArtworkError, ArtworkRequest, ArtworkResult};
use crate::discord::artwork::worker::{ArtworkEvent, ArtworkWorker};
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
enum DiscordCommand {
    TrackChanged {
        playback: Box<PlaybackInfo>,
        artwork: Option<ArtworkRequest>,
    },
    PositionChanged {
        position: Duration,
        duration: Option<Duration>,
    },
    Paused,
    Resumed(Duration),
    Clear,
    Shutdown,
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
        Self::spawn_with_config(client_id, DiscordConfig::default())
    }

    pub fn spawn_with_config(
        client_id: impl Into<String>,
        config: DiscordConfig,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = channel();
        let client_id = client_id.into();
        let connected = Arc::new(AtomicBool::new(false));
        let connected_flag = Arc::clone(&connected);
        let internal_sender = sender.clone();

        let thread_handle = thread::Builder::new()
            .name("discord-rpc-worker".to_string())
            .spawn(move || {
                run_worker_loop(
                    &client_id,
                    receiver,
                    connected_flag,
                    internal_sender,
                    config,
                );
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

    pub fn track_changed(&self, playback: PlaybackInfo, artwork: Option<ArtworkRequest>) {
        let _ = self.sender.send(DiscordCommand::TrackChanged {
            playback: Box::new(playback),
            artwork,
        });
    }

    pub fn position_changed(&self, pos: Duration) {
        let _ = self.sender.send(DiscordCommand::PositionChanged {
            position: pos,
            duration: None,
        });
    }

    pub fn playback_progress(&self, pos: Duration, duration: Option<Duration>) {
        let _ = self.sender.send(DiscordCommand::PositionChanged {
            position: pos,
            duration,
        });
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
    config: DiscordConfig,
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
    let mut artwork_worker = if config.artwork_enabled {
        let tx_clone = internal_sender;
        match ArtworkWorker::spawn_with_forwarder(
            config.artwork.clone(),
            move |event| match event {
                ArtworkEvent::Resolved { generation, result } => {
                    let _ = tx_clone.send(DiscordCommand::ArtworkResolved { generation, result });
                }
            },
        ) {
            Ok(w) => Some(w),
            Err(e) => {
                log_discord(&format!(
                    "[Discord RPC] Failed to spawn artwork worker: {e}"
                ));
                None
            }
        }
    } else {
        log_discord("[Discord RPC] Artwork resolution disabled by configuration");
        None
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
                DiscordCommand::TrackChanged { playback, artwork } => {
                    // Generation tracking
                    current_generation += 1;
                    let generation = current_generation;

                    log_discord(&format!(
                        "[Discord RPC] Track changed (gen {generation}): {} — {}",
                        playback.title, playback.artist
                    ));

                    // Publish initial activity immediately using default fallback asset ("logo")
                    apply_playback(
                        &mut client,
                        &mut connected,
                        &connected_flag,
                        &mut next_reconnect_at,
                        &mut current_backoff,
                        &playback,
                    );

                    // Request artwork resolution in parallel without blocking if enabled
                    if config.artwork_enabled
                        && let Some(req) = artwork
                        && !req.source_url.trim().is_empty()
                        && let Some(ref aw) = artwork_worker
                    {
                        aw.resolve(generation, req);
                    }

                    current_playback = Some(*playback);
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
                DiscordCommand::PositionChanged {
                    position: pos,
                    duration: maybe_dur,
                } => {
                    if let Some(info) = &mut current_playback {
                        let old_pos = info.position.unwrap_or(Duration::ZERO);
                        info.position = Some(pos);
                        let duration_updated = if info.duration.is_none() && maybe_dur.is_some() {
                            info.duration = maybe_dur;
                            true
                        } else {
                            false
                        };
                        // Discord automatically client-side interpolates start/end timestamps.
                        // Refresh activity if duration was newly discovered or if a seek caused a jump > 2s
                        let seeked = pos.abs_diff(old_pos) > Duration::from_secs(2);
                        if (duration_updated || seeked) && !info.is_paused {
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
                    if let Some(mut aw) = artwork_worker.take() {
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
                if let Some(mut aw) = artwork_worker.take() {
                    aw.shutdown();
                }
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
        let track = PlaybackInfo::new("Title", "Artist");
        let artwork = ArtworkRequest::new(
            "ytmusic",
            Some("vid1".into()),
            "https://example.com/art.jpg",
        );
        worker.track_changed(track, Some(artwork));
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
        worker.track_changed(track, None);

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

    #[test]
    fn test_worker_discovers_duration_dynamically() {
        let mut worker = DiscordWorker::spawn("dummy_id").expect("spawn worker");
        // Start track with unknown duration
        let track = PlaybackInfo::new("Stream Title", "Stream Artist");
        assert!(track.duration.is_none());
        worker.track_changed(track, None);

        // First position update delivers newly discovered duration from MPV
        worker.playback_progress(Duration::from_secs(1), Some(Duration::from_secs(210)));

        worker.clear();
        worker.shutdown();
    }

    #[test]
    fn test_step17_playback_state_consistency_lifecycle() {
        let mut worker = DiscordWorker::spawn("dummy_id").expect("spawn worker");

        // Track changed: publish new title and artist, trigger asynchronous artwork resolve
        let track =
            PlaybackInfo::new("Track A", "Artist A").with_duration(Duration::from_secs(180));
        let artwork = ArtworkRequest::new(
            "spotify",
            Some("track_1".into()),
            "https://example.com/art.jpg",
        );
        worker.track_changed(track, Some(artwork));

        // Position changed: update timestamps, never trigger artwork upload
        worker.playback_progress(Duration::from_secs(5), Some(Duration::from_secs(180)));

        // Paused: freeze/remove timestamps, keep existing artwork
        worker.paused();

        // Resumed: recalculate timestamps, keep existing artwork
        worker.resumed(Duration::from_secs(5));

        // Track finished / Playback stopped: clear activity, keep cache entry
        worker.clear();

        worker.shutdown();
    }
}
