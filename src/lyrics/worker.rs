use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

use crate::lyrics::model::Lyrics;
use crate::lyrics::provider::fetch_lyrics;
use crate::lyrics::types::{LyricsError, LyricsStatus};
use crate::music::track::Track;

/// Commands sent to the lyrics worker loop.
#[derive(Debug)]
pub enum LyricsCommand {
    /// Fetch lyrics for a track. If `force` is false, duplicate requests for the same track are ignored.
    Fetch { track: Track, force: bool },
    /// Clear lyrics state and invalidate active fetch operations.
    Clear,
    /// Result received from an asynchronous background fetch thread.
    InternalCompleted {
        generation: u64,
        track: Track,
        result: Result<Option<Lyrics>, LyricsError>,
    },
    /// Terminate worker loop.
    Shutdown,
}

/// Events emitted by the lyrics worker over the response channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsEvent {
    /// Indicates lyrics retrieval has begun for the specified track and generation.
    Loading { generation: u64, track: Track },
    /// Lyrics were successfully fetched, parsed, and validated.
    Success {
        generation: u64,
        track: Track,
        lyrics: Box<Lyrics>,
    },
    /// Track was queried but no lyrics were found on the provider.
    NotFound { generation: u64, track: Track },
    /// Lyrics fetch or processing failed with an error.
    Failed {
        generation: u64,
        track: Track,
        error: String,
    },
    /// Lyrics state was cleared to idle.
    Cleared,
}

impl LyricsEvent {
    /// Returns the generation ID associated with this event, if any.
    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::Loading { generation, .. }
            | Self::Success { generation, .. }
            | Self::NotFound { generation, .. }
            | Self::Failed { generation, .. } => Some(*generation),
            Self::Cleared => None,
        }
    }

    /// Returns a reference to the contained `Track` if this event is associated with a track.
    pub fn track(&self) -> Option<&Track> {
        match self {
            Self::Loading { track, .. }
            | Self::Success { track, .. }
            | Self::NotFound { track, .. }
            | Self::Failed { track, .. } => Some(track),
            Self::Cleared => None,
        }
    }

    /// Convenience accessor for the track title, if any.
    pub fn title(&self) -> Option<&str> {
        self.track().map(|t| t.title.as_str())
    }

    /// Convenience accessor for the track artist, if any.
    pub fn artist(&self) -> Option<&str> {
        self.track().map(|t| t.artist.as_str())
    }

    /// Maps this event to its corresponding `LyricsStatus`.
    pub fn status(&self) -> LyricsStatus {
        match self {
            Self::Cleared => LyricsStatus::Idle,
            Self::Loading { .. } => LyricsStatus::Loading,
            Self::Success { .. } => LyricsStatus::Available,
            Self::NotFound { .. } => LyricsStatus::NotFound,
            Self::Failed { error, .. } => LyricsStatus::Failed(error.clone()),
        }
    }

    /// Returns a reference to the contained `Lyrics` if this is a `Success` event.
    pub fn lyrics(&self) -> Option<&Lyrics> {
        match self {
            Self::Success { lyrics, .. } => Some(lyrics),
            _ => None,
        }
    }
}

/// High-level synchronized state for lyrics in the UI or application.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LyricsState {
    pub status: LyricsStatus,
    pub generation: u64,
    pub current_track: Option<Track>,
    pub lyrics: Option<Lyrics>,
}

impl LyricsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the state according to an incoming `LyricsEvent`.
    pub fn apply_event(&mut self, event: LyricsEvent) {
        match event {
            LyricsEvent::Loading { generation, track } => {
                if generation >= self.generation {
                    self.generation = generation;
                    self.status = LyricsStatus::Loading;
                    self.current_track = Some(track);
                    self.lyrics = None;
                }
            }
            LyricsEvent::Success {
                generation,
                track,
                lyrics,
            } => {
                if generation >= self.generation {
                    self.generation = generation;
                    self.status = LyricsStatus::Available;
                    self.current_track = Some(track);
                    self.lyrics = Some(*lyrics);
                }
            }
            LyricsEvent::NotFound { generation, track } => {
                if generation >= self.generation {
                    self.generation = generation;
                    self.status = LyricsStatus::NotFound;
                    self.current_track = Some(track);
                    self.lyrics = None;
                }
            }
            LyricsEvent::Failed {
                generation,
                track,
                error,
            } => {
                if generation >= self.generation {
                    self.generation = generation;
                    self.status = LyricsStatus::Failed(error);
                    self.current_track = Some(track);
                    self.lyrics = None;
                }
            }
            LyricsEvent::Cleared => {
                self.status = LyricsStatus::Idle;
                self.current_track = None;
                self.lyrics = None;
            }
        }
    }
}

/// Dedicated worker managing lyrics requests on background threads,
/// completely decoupled from audio playback.
pub struct LyricsWorker {
    command_tx: Sender<LyricsCommand>,
    shutdown_flag: Arc<AtomicBool>,
    active_generation: Arc<AtomicU64>,
    worker_handle: Option<JoinHandle<()>>,
}

impl LyricsWorker {
    /// Spawns a new lyrics worker with its own event response channel.
    pub fn spawn() -> (Self, Receiver<LyricsEvent>) {
        let (tx, rx) = channel();
        let worker = Self::spawn_with_sender(tx).expect("spawn lyrics worker");
        (worker, rx)
    }

    /// Spawns a new lyrics worker delivering events to the provided sender.
    pub fn spawn_with_sender(event_tx: Sender<LyricsEvent>) -> std::io::Result<Self> {
        Self::spawn_with_fetcher(event_tx, fetch_lyrics)
    }

    /// Spawns a worker with a custom fetcher (useful for unit testing without live network/backend).
    pub fn spawn_with_fetcher<F>(event_tx: Sender<LyricsEvent>, fetcher: F) -> std::io::Result<Self>
    where
        F: Fn(&Track) -> Result<Option<Lyrics>, LyricsError> + Send + Sync + 'static,
    {
        let (cmd_tx, cmd_rx) = channel();
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown_flag);
        let active_generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&active_generation);
        let worker_cmd_tx = cmd_tx.clone();
        let fetcher = Arc::new(fetcher);

        let worker_handle = thread::Builder::new()
            .name("lyrics-worker".to_string())
            .spawn(move || {
                run_lyrics_worker(
                    cmd_rx,
                    worker_cmd_tx,
                    event_tx,
                    worker_shutdown,
                    worker_generation,
                    fetcher,
                );
            })?;

        Ok(Self {
            command_tx: cmd_tx,
            shutdown_flag,
            active_generation,
            worker_handle: Some(worker_handle),
        })
    }

    /// Requests lyrics for a track. If this is the same track as the currently active
    /// or pending request, it is treated as a duplicate and ignored.
    pub fn fetch(&self, track: Track) {
        let _ = self.command_tx.send(LyricsCommand::Fetch {
            track,
            force: false,
        });
    }

    /// Requests lyrics for a track, bypassing duplicate detection.
    pub fn refresh(&self, track: Track) {
        let _ = self
            .command_tx
            .send(LyricsCommand::Fetch { track, force: true });
    }

    /// Clears the current lyrics state and invalidates any in-flight fetches.
    pub fn clear(&self) {
        let _ = self.command_tx.send(LyricsCommand::Clear);
    }

    /// Returns the currently active generation ID.
    pub fn generation(&self) -> u64 {
        self.active_generation.load(Ordering::SeqCst)
    }

    /// Shuts down the worker thread gracefully and idempotently.
    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        let _ = self.command_tx.send(LyricsCommand::Shutdown);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LyricsWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct FetchTask {
    generation: u64,
    track: Track,
    cancel_token: Arc<AtomicBool>,
}

#[derive(Default)]
struct FetchQueueState {
    pending: Option<FetchTask>,
    shutdown: bool,
}

type FetchQueue = Arc<(std::sync::Mutex<FetchQueueState>, std::sync::Condvar)>;

fn cancel_active_and_pending(
    queue: &FetchQueue,
    current_cancel_token: &mut Option<Arc<AtomicBool>>,
) {
    if let Some(token) = current_cancel_token.take() {
        token.store(true, Ordering::SeqCst);
    }
    let mut guard = queue.0.lock().unwrap();
    if let Some(prev) = guard.pending.take() {
        prev.cancel_token.store(true, Ordering::SeqCst);
    }
}

fn run_lyrics_worker<F>(
    cmd_rx: Receiver<LyricsCommand>,
    cmd_tx: Sender<LyricsCommand>,
    event_tx: Sender<LyricsEvent>,
    shutdown_flag: Arc<AtomicBool>,
    active_generation: Arc<AtomicU64>,
    fetcher: Arc<F>,
) where
    F: Fn(&Track) -> Result<Option<Lyrics>, LyricsError> + Send + Sync + 'static,
{
    let mut current_generation: u64 = 0;
    let mut last_track: Option<Track> = None;
    // True when the most-recent completed fetch for `last_track` returned an
    // error.  Allows the deduplication guard to pass a retry through for the
    // same track without requiring `force = true`.
    let mut last_track_failed: bool = false;
    let mut current_cancel_token: Option<Arc<AtomicBool>> = None;

    let task_queue: FetchQueue = Arc::new((
        std::sync::Mutex::new(FetchQueueState::default()),
        std::sync::Condvar::new(),
    ));

    let fetcher_clone = Arc::clone(&fetcher);
    let fetch_queue_clone = Arc::clone(&task_queue);
    let fetch_cmd_tx = cmd_tx.clone();
    let fetch_active_gen = Arc::clone(&active_generation);
    let fetch_shutdown = Arc::clone(&shutdown_flag);

    let fetch_handle = thread::Builder::new()
        .name("lyrics-fetch-worker".to_string())
        .spawn(move || {
            loop {
                let task = {
                    let mut guard = fetch_queue_clone.0.lock().unwrap();
                    while !guard.shutdown && guard.pending.is_none() {
                        guard = fetch_queue_clone.1.wait(guard).unwrap();
                    }
                    if guard.shutdown {
                        break;
                    }
                    guard.pending.take().unwrap()
                };

                // Check cancellation before calling fetcher
                if task.cancel_token.load(Ordering::SeqCst)
                    || fetch_active_gen.load(Ordering::Relaxed) != task.generation
                    || fetch_shutdown.load(Ordering::SeqCst)
                {
                    continue;
                }

                crate::lyrics::provider::CURRENT_CANCEL_TOKEN.with(|cell| {
                    *cell.borrow_mut() = Some(Arc::clone(&task.cancel_token));
                });

                let result = fetcher_clone(&task.track);

                crate::lyrics::provider::CURRENT_CANCEL_TOKEN.with(|cell| {
                    *cell.borrow_mut() = None;
                });

                // Check cancellation after fetcher returns
                if task.cancel_token.load(Ordering::SeqCst)
                    || fetch_active_gen.load(Ordering::Relaxed) != task.generation
                    || fetch_shutdown.load(Ordering::SeqCst)
                {
                    continue;
                }

                let _ = fetch_cmd_tx.send(LyricsCommand::InternalCompleted {
                    generation: task.generation,
                    track: task.track,
                    result,
                });
            }
        });

    while let Ok(cmd) = cmd_rx.recv() {
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }

        match cmd {
            LyricsCommand::Fetch { track, force } => {
                // Prevent duplicate requests for the same track, unless the
                // previous attempt for that track ended in a transient failure
                // (in which case a retry must be allowed through).
                if !force
                    && !last_track_failed
                    && let Some(ref last) = last_track
                    && last.same_identity(&track)
                {
                    continue;
                }

                // A new fetch supersedes any prior failure for this track.
                last_track_failed = false;

                // If forcing refresh, invalidate track from cache first.
                // Otherwise check cache for instant loading.
                if force {
                    crate::lyrics::cache::invalidate_track_in_cache(&track);
                } else if let Some(cached) = crate::lyrics::cache::get_lyrics_from_cache(&track) {
                    current_generation = current_generation.wrapping_add(1);
                    let generation = current_generation;
                    active_generation.store(generation, Ordering::SeqCst);
                    last_track = Some(track.clone());

                    // Invalidate in-flight background fetch
                    cancel_active_and_pending(&task_queue, &mut current_cancel_token);

                    let _ = event_tx.send(LyricsEvent::Success {
                        generation,
                        track,
                        lyrics: Box::new(cached),
                    });
                    continue;
                }

                // Advance generation and track active identity
                current_generation = current_generation.wrapping_add(1);
                let generation = current_generation;
                active_generation.store(generation, Ordering::SeqCst);
                last_track = Some(track.clone());

                // Return loading state
                let _ = event_tx.send(LyricsEvent::Loading {
                    generation,
                    track: track.clone(),
                });

                // Cancel any prior in-flight fetch and enqueue new task
                let cancel_token = Arc::new(AtomicBool::new(false));
                cancel_active_and_pending(&task_queue, &mut current_cancel_token);
                current_cancel_token = Some(Arc::clone(&cancel_token));

                {
                    let mut guard = task_queue.0.lock().unwrap();
                    guard.pending = Some(FetchTask {
                        generation,
                        track,
                        cancel_token,
                    });
                    task_queue.1.notify_one();
                }
            }

            LyricsCommand::InternalCompleted {
                generation,
                track,
                result,
            } => {
                // Discard stale generation
                if generation != current_generation {
                    continue;
                }

                // Verify track matches current track
                if let Some(ref current) = last_track {
                    if !current.same_identity(&track) {
                        continue;
                    }
                } else {
                    continue;
                }

                // Return success, unavailable (NotFound), and error states
                match result {
                    Ok(Some(lyrics)) => {
                        last_track_failed = false;
                        crate::lyrics::cache::store_lyrics_in_cache(&track, &lyrics);
                        let _ = event_tx.send(LyricsEvent::Success {
                            generation,
                            track,
                            lyrics: Box::new(lyrics),
                        });
                    }
                    Ok(None) => {
                        last_track_failed = false;
                        let _ = event_tx.send(LyricsEvent::NotFound { generation, track });
                    }
                    Err(err) => {
                        // Mark this track as failed so the next `fetch` call
                        // for the same track is treated as a retry rather than
                        // a duplicate.
                        last_track_failed = true;
                        let _ = event_tx.send(LyricsEvent::Failed {
                            generation,
                            track,
                            error: err.to_string(),
                        });
                    }
                }
            }

            LyricsCommand::Clear => {
                current_generation = current_generation.wrapping_add(1);
                active_generation.store(current_generation, Ordering::SeqCst);
                last_track = None;
                last_track_failed = false;
                cancel_active_and_pending(&task_queue, &mut current_cancel_token);
                let _ = event_tx.send(LyricsEvent::Cleared);
            }

            LyricsCommand::Shutdown => {
                break;
            }
        }
    }

    // Tear down fetch worker and wait for clean termination
    cancel_active_and_pending(&task_queue, &mut current_cancel_token);
    {
        let mut guard = task_queue.0.lock().unwrap();
        guard.shutdown = true;
        task_queue.1.notify_all();
    }
    if let Ok(handle) = fetch_handle {
        let _ = handle.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_lyrics_worker_retry_allowed_after_failed_fetch() {
        let (event_tx, event_rx) = channel();
        let track = Track::new("RetryTrack_Failed_Test", "Artist");
        crate::lyrics::cache::invalidate_track_in_cache(&track);

        // Fetcher fails on the first call, succeeds on the second.
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);
        let worker = LyricsWorker::spawn_with_fetcher(event_tx, move |_t| {
            let n = call_count_clone.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(LyricsError::Network("transient failure".to_string()))
            } else {
                Ok(Some(Lyrics::from_plain_text(
                    Some("RetryTrack".to_string()),
                    Some("Artist".to_string()),
                    "Lyrics on retry",
                )))
            }
        })
        .expect("spawn worker");

        let mut state = LyricsState::new();

        // First fetch — should trigger a network error.
        worker.fetch(track.clone());
        let ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev); // Loading
        let ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev); // Failed
        assert!(
            matches!(state.status, LyricsStatus::Failed(_)),
            "Expected Failed after first fetch, got {:?}",
            state.status
        );

        // Second fetch for the same track — must NOT be deduplicated even
        // though `last_track` still points to this track.
        worker.fetch(track.clone());
        let ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev); // Loading
        assert_eq!(
            state.status,
            LyricsStatus::Loading,
            "Retry must trigger a new Loading event"
        );
        let ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev); // Available
        assert_eq!(
            state.status,
            LyricsStatus::Available,
            "Retry must succeed after transient failure"
        );
    }

    #[test]
    fn test_lyrics_worker_single_track_success() {
        let (event_tx, event_rx) = channel();
        let track = Track::new("Creep_Single_Test", "Radiohead");
        crate::lyrics::cache::invalidate_track_in_cache(&track);

        let worker = LyricsWorker::spawn_with_fetcher(event_tx, |_t| {
            Ok(Some(Lyrics::from_plain_text(
                Some("Creep_Single_Test".to_string()),
                Some("Radiohead".to_string()),
                "I'm a creep",
            )))
        })
        .expect("spawn worker");

        let mut state = LyricsState::new();
        assert_eq!(state.status, LyricsStatus::Idle);

        worker.fetch(track.clone());

        // Loading event
        let ev1 = event_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("receive loading event");
        state.apply_event(ev1.clone());
        assert_eq!(state.status, LyricsStatus::Loading);
        assert_eq!(ev1.generation(), Some(1));
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("Creep_Single_Test")
        );

        // Success event
        let ev2 = event_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("receive success event");
        state.apply_event(ev2);
        assert_eq!(state.status, LyricsStatus::Available);
        assert!(state.lyrics.is_some());
        assert_eq!(
            state.lyrics.as_ref().unwrap().title.as_deref(),
            Some("Creep_Single_Test")
        );
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("Creep_Single_Test")
        );
    }

    #[test]
    fn test_lyrics_worker_prevent_duplicate_requests() {
        let (event_tx, event_rx) = channel();
        let track = Track::new("Karma_Police_Dup_Test", "Radiohead");
        crate::lyrics::cache::invalidate_track_in_cache(&track);

        let worker = LyricsWorker::spawn_with_fetcher(event_tx, |_t| {
            Ok(Some(Lyrics::from_plain_text(
                Some("Karma_Police_Dup_Test".to_string()),
                Some("Radiohead".to_string()),
                "Karma police, arrest this man",
            )))
        })
        .expect("spawn worker");

        worker.fetch(track.clone());

        // Consume Loading and Success
        let _ = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let _ = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();

        // Duplicate fetch for same track
        worker.fetch(track.clone());
        let res = event_rx.recv_timeout(Duration::from_millis(100));
        assert!(
            res.is_err(),
            "Duplicate fetch must be ignored without sending new events"
        );

        // Forced refresh should re-fetch
        worker.refresh(track.clone());
        let refresh_ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            refresh_ev,
            LyricsEvent::Loading { generation: 2, .. }
        ));
    }

    #[test]
    fn test_lyrics_worker_rapid_track_changes_reject_stale() {
        let (event_tx, event_rx) = channel();

        // Simulate artificial delays: Track 1 is slow, Track 2 is slow, Track 3 is fast
        let worker = LyricsWorker::spawn_with_fetcher(event_tx, |t| {
            if t.title == "Song 1" {
                thread::sleep(Duration::from_millis(150));
            } else if t.title == "Song 2" {
                thread::sleep(Duration::from_millis(100));
            } else {
                thread::sleep(Duration::from_millis(10));
            }

            Ok(Some(Lyrics::from_plain_text(
                Some(t.title.clone()),
                Some(t.artist.clone()),
                &format!("Lyrics for {}", t.title),
            )))
        })
        .expect("spawn worker");

        let track1 = Track::new("Song 1", "Artist");
        let track2 = Track::new("Song 2", "Artist");
        let track3 = Track::new("Song 3", "Artist");

        let mut state = LyricsState::new();

        // Rapid switching: 1 -> 2 -> 3
        worker.fetch(track1);
        worker.fetch(track2);
        worker.fetch(track3);

        // Collect all events emitted within 500ms
        let mut final_success = None;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            if let Ok(event) = event_rx.recv_timeout(Duration::from_millis(50)) {
                if let LyricsEvent::Success { track, .. } = &event {
                    final_success = Some(track.title.clone());
                }
                state.apply_event(event);
            }
        }

        // Only Song 3's lyrics should ever have been accepted as Success
        assert_eq!(
            final_success,
            Some("Song 3".to_string()),
            "Old lyrics from Song 1 or 2 must NEVER appear"
        );
        assert_eq!(state.status, LyricsStatus::Available);
        assert_eq!(
            state.lyrics.as_ref().unwrap().title.as_deref(),
            Some("Song 3")
        );
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("Song 3")
        );
    }

    #[test]
    fn test_lyrics_worker_not_found_and_failed_states() {
        let (event_tx, event_rx) = channel();

        let worker = LyricsWorker::spawn_with_fetcher(event_tx, |t| {
            if t.title == "NotFoundTrack" {
                Ok(None)
            } else {
                Err(LyricsError::Network("Connection dropped".to_string()))
            }
        })
        .expect("spawn worker");

        let mut state = LyricsState::new();

        // NotFound track
        worker.fetch(Track::new("NotFoundTrack", "Artist"));
        let ev_load1 = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev_load1);
        assert_eq!(state.status, LyricsStatus::Loading);
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("NotFoundTrack")
        );

        let ev_not_found = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev_not_found);
        assert_eq!(state.status, LyricsStatus::NotFound);
        assert!(state.lyrics.is_none());
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("NotFoundTrack")
        );

        // Error track
        worker.fetch(Track::new("FailingTrack", "Artist"));
        let ev_load2 = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev_load2);
        assert_eq!(state.status, LyricsStatus::Loading);
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("FailingTrack")
        );

        let ev_failed = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(ev_failed);
        assert!(matches!(state.status, LyricsStatus::Failed(_)));
        assert!(state.lyrics.is_none());
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("FailingTrack")
        );
    }

    #[test]
    fn test_lyrics_worker_clear_invalidates_in_flight() {
        let (event_tx, event_rx) = channel();

        let worker = LyricsWorker::spawn_with_fetcher(event_tx, |_t| {
            thread::sleep(Duration::from_millis(100));
            Ok(Some(Lyrics::from_plain_text(
                Some("Song".to_string()),
                Some("Artist".to_string()),
                "Some words",
            )))
        })
        .expect("spawn worker");

        let mut state = LyricsState::new();
        worker.fetch(Track::new("Song", "Artist"));

        // Wait for Loading
        let load_ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(load_ev);
        assert_eq!(state.status, LyricsStatus::Loading);
        assert_eq!(
            state.current_track.as_ref().map(|t| t.title.as_str()),
            Some("Song")
        );

        // Clear playback
        worker.clear();
        let clear_ev = event_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        state.apply_event(clear_ev);
        assert_eq!(state.status, LyricsStatus::Idle);
        assert_eq!(state.current_track, None);

        // Sleep to let in-flight finish
        thread::sleep(Duration::from_millis(150));

        // In-flight result should have been discarded
        let lingering = event_rx.recv_timeout(Duration::from_millis(50));
        assert!(
            lingering.is_err(),
            "In-flight result after clear must be discarded"
        );
        assert_eq!(state.status, LyricsStatus::Idle);
    }

    #[test]
    fn test_lyrics_worker_shutdown_is_idempotent() {
        let (event_tx, _event_rx) = channel();
        let mut worker = LyricsWorker::spawn_with_sender(event_tx).expect("spawn worker");

        worker.shutdown();
        assert!(worker.worker_handle.is_none());

        // Calling shutdown again must not panic
        worker.shutdown();
        assert!(worker.worker_handle.is_none());
    }
}
