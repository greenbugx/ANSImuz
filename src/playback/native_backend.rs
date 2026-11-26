use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::events::PlayerEvent;
use super::mpv::{PrefetchResult, audio_cache_dir, audio_hash};
use super::native_adapter::Translator;
use super::native_mpv::NativeMpv;
use super::player::AudioPlayer;

// Dumps an audio stream to disk cache using a secondary native libmpv instance
pub(crate) fn dump_stream_native(
    url: &str,
    dumping: Arc<Mutex<HashSet<String>>>,
    cancel: Option<Arc<AtomicBool>>,
) -> PrefetchResult {
    let epoch = crate::config::current_cache_epoch();
    if url.trim().is_empty() {
        return PrefetchResult::Failed("Empty URL".into());
    }

    if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
        return PrefetchResult::Cancelled;
    }

    let final_path = audio_cache_dir().join(format!("{}.audio", audio_hash(url)));
    if final_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return PrefetchResult::AlreadyCached(final_path);
    }

    {
        let mut set = match dumping.lock() {
            Ok(set) => set,
            Err(_) => return PrefetchResult::Failed("Dumping lock poisoned".into()),
        };

        if set.contains(url) {
            return PrefetchResult::Cancelled;
        }
        set.insert(url.to_string());
    }

    struct DumpingGuard {
        url: String,
        dumping: Arc<Mutex<HashSet<String>>>,
    }

    impl Drop for DumpingGuard {
        fn drop(&mut self) {
            if let Ok(mut set) = self.dumping.lock() {
                set.remove(&self.url);
            }
        }
    }

    let _dump_guard = DumpingGuard {
        url: url.to_string(),
        dumping: Arc::clone(&dumping),
    };

    if let Err(e) = std::fs::create_dir_all(audio_cache_dir()) {
        return PrefetchResult::Failed(format!("Cannot create audio cache directory: {e}"));
    }

    let part = audio_cache_dir().join(format!("{}.part", audio_hash(url)));
    if part.exists() {
        let _ = std::fs::remove_file(&part);
    }

    struct PartGuard {
        path: std::path::PathBuf,
        success: bool,
    }

    impl Drop for PartGuard {
        fn drop(&mut self) {
            if !self.success && self.path.exists() {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    let mut part_guard = PartGuard {
        path: part.clone(),
        success: false,
    };

    if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
        return PrefetchResult::Cancelled;
    }

    let cancel_ref = cancel.as_deref();
    let dump_res = NativeMpv::dump_stream(url, &part, cancel_ref);

    match dump_res {
        Ok(()) => {
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return PrefetchResult::Cancelled;
            }
            if crate::config::current_cache_epoch() != epoch {
                return PrefetchResult::Cancelled;
            }

            let file_size = match part.metadata() {
                Ok(meta) => meta.len(),
                Err(e) => {
                    return PrefetchResult::Failed(format!(
                        "Failed to read stream dump metadata: {e}"
                    ));
                }
            };

            if file_size < super::mpv::MIN_DUMP_BYTES {
                return PrefetchResult::Failed(format!(
                    "Prefetched audio file too small: {file_size} bytes (minimum is {} bytes)",
                    super::mpv::MIN_DUMP_BYTES
                ));
            }

            if file_size > super::mpv::MAX_DUMP_BYTES {
                return PrefetchResult::Failed(format!(
                    "Prefetched audio file too large: {file_size} bytes (maximum is {} bytes)",
                    super::mpv::MAX_DUMP_BYTES
                ));
            }

            if let Err(e) = std::fs::rename(&part, &final_path) {
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    || e.kind() == std::io::ErrorKind::PermissionDenied
                {
                    let _ = std::fs::remove_file(&final_path);
                    if let Err(retry_e) = std::fs::rename(&part, &final_path) {
                        return PrefetchResult::Failed(format!(
                            "Failed to rename .part to final cache file: {retry_e}"
                        ));
                    }
                } else {
                    return PrefetchResult::Failed(format!(
                        "Failed to rename .part to final cache file: {e}"
                    ));
                }
            }

            part_guard.success = true;
            let _ = crate::config::prune_cache_dir(
                &audio_cache_dir(),
                crate::config::MAX_CACHE_DIR_BYTES,
            );
            PrefetchResult::Cached(final_path)
        }
        Err(err) => {
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
                || err.contains("cancelled")
            {
                PrefetchResult::Cancelled
            } else {
                PrefetchResult::Failed(err)
            }
        }
    }
}

// Resolves playback target by checking disk cache first then delegating to mpv
pub(crate) fn resolve_playback_target(url: &str) -> Result<String, String> {
    if let Some(path) = AudioPlayer::cached_audio(url) {
        Ok(path.to_string_lossy().into_owned())
    } else {
        Ok(url.to_string())
    }
}

// Native libmpv backend implementing the playback interface
pub struct NativeBackend {
    mpv: Option<NativeMpv>,
    translator: Translator,
    playing: bool,
    generation: u64,
    last_error: Option<String>,
    events: Option<Sender<PlayerEvent>>,
    dumping: Arc<Mutex<HashSet<String>>>,
    cancel_prefetch: Arc<AtomicBool>,
    prefetch_results: Arc<Mutex<HashMap<String, PrefetchResult>>>,
    is_mock: bool,
    mock_playing: bool,
    mock_fail: bool,
}

impl NativeBackend {
    pub fn new() -> Result<Self, String> {
        let mpv = NativeMpv::new().map_err(|e| format!("Failed to initialize native mpv: {e}"))?;
        Ok(Self {
            mpv: Some(mpv),
            translator: Translator::new(),
            playing: false,
            generation: 0,
            last_error: None,
            events: None,
            dumping: Arc::new(Mutex::new(HashSet::new())),
            cancel_prefetch: Arc::new(AtomicBool::new(false)),
            prefetch_results: Arc::new(Mutex::new(HashMap::new())),
            is_mock: false,
            mock_playing: false,
            mock_fail: false,
        })
    }

    pub fn new_mock() -> Self {
        Self {
            mpv: None,
            translator: Translator::new(),
            playing: false,
            generation: 0,
            last_error: None,
            events: None,
            dumping: Arc::new(Mutex::new(HashSet::new())),
            cancel_prefetch: Arc::new(AtomicBool::new(false)),
            prefetch_results: Arc::new(Mutex::new(HashMap::new())),
            is_mock: true,
            mock_playing: false,
            mock_fail: false,
        }
    }

    pub fn set_mock_fail(&mut self, fail: bool) {
        self.mock_fail = fail;
        if fail {
            self.last_error = Some("Failed to launch mpv: process failed to start".into());
        }
    }

    pub fn is_mock(&self) -> bool {
        self.is_mock
    }

    pub fn record_error(&mut self, error: String) {
        self.last_error = Some(error);
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    pub fn prefetch_later(&self, url: &str, delay_secs: u64) {
        if self.is_mock || url.trim().is_empty() || AudioPlayer::cached_audio(url).is_some() {
            return;
        }

        let dumping = Arc::clone(&self.dumping);
        let results = Arc::clone(&self.prefetch_results);
        let cancel = Arc::clone(&self.cancel_prefetch);
        let owned = url.to_string();

        thread::spawn(move || {
            for _ in 0..(delay_secs * 10) {
                if cancel.load(Ordering::Relaxed) {
                    if let Ok(mut lock) = results.lock() {
                        lock.insert(owned, PrefetchResult::Cancelled);
                    }
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }

            if cancel.load(Ordering::Relaxed) {
                if let Ok(mut lock) = results.lock() {
                    lock.insert(owned, PrefetchResult::Cancelled);
                }
                return;
            }

            let res = dump_stream_native(&owned, dumping, Some(cancel));
            if let Ok(mut lock) = results.lock() {
                if lock.len() >= 20
                    && !lock.contains_key(&owned)
                    && let Some(first_key) = lock.keys().next().cloned()
                {
                    lock.remove(&first_key);
                }
                lock.insert(owned, res);
            }
        });
    }

    pub fn prefetch_result(&self, url: &str) -> Option<PrefetchResult> {
        self.prefetch_results.lock().ok()?.get(url).cloned()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_playing(&mut self) -> bool {
        if self.is_mock {
            return self.mock_playing && !self.translator.is_paused();
        }
        self.playing && !self.translator.is_paused()
    }

    pub fn error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn play(
        &mut self,
        url: &str,
        volume: u8,
        events: Sender<PlayerEvent>,
    ) -> Result<(), String> {
        if self.is_mock {
            self.mock_playing = false;
            self.last_error = None;
            self.generation += 1;
            if self.mock_fail {
                self.mock_playing = false;
                self.last_error = Some("Failed to launch mpv: process failed to start".into());
                return Err("Failed to launch mpv: process failed to start".into());
            }
            self.mock_playing = true;
            self.events = Some(events);
            return Ok(());
        }

        self.last_error = None;
        self.cancel_prefetch.store(true, Ordering::SeqCst);
        self.cancel_prefetch = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        let generation = self.generation;

        let target = resolve_playback_target(url)?;

        if let Some(ref mut mpv) = self.mpv {
            mpv.set_pause(false)?;
            mpv.set_volume(volume)?;
            let load_id = mpv.loadfile(&target)?;
            self.translator.begin(load_id, generation);
            self.playing = true;
            self.events = Some(events);
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn poll_events(&mut self) -> Result<Vec<PlayerEvent>, String> {
        let mut emitted = Vec::new();
        if let Some(ref mut mpv) = self.mpv {
            while let Ok(Some(ev)) = mpv.poll_event() {
                let desc = |code| mpv.error_message(code);
                if let Some(player_event) = self.translator.translate(ev, &desc) {
                    match &player_event {
                        PlayerEvent::Ended { .. } => {
                            self.playing = false;
                        }
                        PlayerEvent::Failed { message, .. } => {
                            self.playing = false;
                            self.last_error = Some(message.clone());
                        }
                        _ => {}
                    }
                    if let Some(ref tx) = self.events {
                        let _ = tx.send(player_event.clone());
                    }
                    emitted.push(player_event);
                }
            }
        }
        Ok(emitted)
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        if self.is_mock {
            let target = !self.translator.is_paused();
            self.translator.set_paused(target);
            return Ok(());
        }
        if let Some(ref mut mpv) = self.mpv {
            let target = !self.translator.is_paused();
            mpv.set_pause(target)?;
            self.translator.set_paused(target);
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        if self.is_mock {
            return Ok(());
        }
        if let Some(ref mut mpv) = self.mpv {
            mpv.set_volume(volume)?;
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn stop(&mut self) {
        if self.is_mock {
            self.mock_playing = false;
            self.translator.set_paused(false);
            return;
        }
        self.cancel_prefetch.store(true, Ordering::SeqCst);
        if let Some(ref mut mpv) = self.mpv {
            let _ = mpv.set_pause(false);
            let _ = mpv.stop();
        }
        self.translator.set_paused(false);
        self.playing = false;
        self.generation += 1;
    }
}

impl Default for NativeBackend {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self::new_mock())
    }
}

impl Drop for NativeBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_playback_target_direct_url_preserved() {
        let direct = "https://example.com/audio.mp3";
        let target = resolve_playback_target(direct).expect("resolve direct");
        assert_eq!(target, direct);
    }

    #[test]
    fn test_resolve_playback_target_local_path_preserved() {
        let local_path = "native/mpv/music1.mp3";
        let target = resolve_playback_target(local_path).expect("resolve local");
        assert_eq!(target, local_path);
    }

    #[test]
    fn test_native_backend_mock_lifecycle_and_prefetch() {
        let mut backend = NativeBackend::new_mock();
        assert_eq!(backend.generation(), 0);
        assert!(!backend.is_playing());

        let (tx, _rx) = std::sync::mpsc::channel();
        let res = backend.play("native/mpv/music1.mp3", 85, tx);
        assert!(res.is_ok());
        assert!(backend.is_playing());
        assert_eq!(backend.generation(), 1);

        backend.prefetch_later("https://example.com/track", 1);
        backend.stop();
        assert!(!backend.is_playing());
        assert_eq!(backend.generation(), 1);
    }

    #[test]
    fn test_native_backend_thread_send() {
        fn assert_send<T: Send>() {}
        assert_send::<NativeBackend>();

        let backend = NativeBackend::new_mock();
        let handle = std::thread::spawn(move || {
            let mut b = backend;
            assert_eq!(b.generation(), 0);
            b.stop();
        });
        handle.join().expect("join native backend thread");
    }
}
