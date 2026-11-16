use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::lyrics::model::Lyrics;
use crate::lyrics::types::SyncType;
use crate::music::track::Track;

/// Default maximum size for the persistent lyrics cache (50 MB).
pub const DEFAULT_MAX_LYRICS_CACHE_BYTES: u64 = 50 * 1024 * 1024;

/// Duration tolerance in milliseconds for matching recordings of the same song (5 seconds).
/// Recordings exceeding this difference (live vs studio versions) are treated as distinct.
pub const DURATION_TOLERANCE_MS: u64 = 5000;

static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GLOBAL_CACHE: Mutex<Option<LyricsCache>> = Mutex::new(None);

/// Global test lock to prevent parallel test interference when accessing disk caches.
#[cfg(test)]
pub static LYRICS_CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Computes hex-encoded SHA-256 hash of byte slices.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    format!("{result:x}")
}

/// Returns the current Unix timestamp in seconds.
#[must_use]
pub fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Normalizes key text by lowercasing, trimming, and collapsing whitespace.
#[must_use]
pub fn normalize_key_text(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Generates a primary stable cache key for a track.
#[must_use]
pub fn make_primary_key(track: &Track) -> String {
    if track.provider == "spotify"
        && let Some(ref spotify_id) = track.spotify_id
    {
        let s = spotify_id.trim();
        if !s.is_empty() {
            return format!("spotify:{s}");
        }
    }
    if let Some(ref track_id) = track.track_id {
        let tid = track_id.trim();
        let provider = track.provider.trim();
        if !tid.is_empty() && !provider.is_empty() {
            return format!("{provider}:{tid}");
        }
    }
    if let Some(ref spotify_id) = track.spotify_id {
        let s = spotify_id.trim();
        if !s.is_empty() {
            return format!("spotify:{s}");
        }
    }
    let norm_artist = normalize_key_text(&track.artist);
    let norm_title = normalize_key_text(&track.title);
    if let Some(dur) = track.duration_ms {
        format!("norm:{norm_artist}:{norm_title}:{}s", dur / 1000)
    } else {
        format!("norm:{norm_artist}:{norm_title}")
    }
}

/// Generates a filesystem-safe filename for a cache entry based on SHA-256 of its key.
#[must_use]
pub fn entry_file_name(primary_key: &str) -> String {
    format!("{}.json", sha256_hex(primary_key.as_bytes()))
}

/// Atomically writes bytes to a file via a temporary `.part` file and rename.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_id = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("cache");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("part");
    let temp_path =
        path.with_file_name(format!("{stem}.tmp-{}-{temp_id}.{ext}", std::process::id()));

    if let Err(e) = fs::write(&temp_path, bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(e);
    }

    if let Err(e) = fs::rename(&temp_path, path) {
        if e.kind() == std::io::ErrorKind::AlreadyExists
            || e.kind() == std::io::ErrorKind::PermissionDenied
        {
            let _ = fs::remove_file(path);
            if let Err(retry_err) = fs::rename(&temp_path, path) {
                let _ = fs::remove_file(&temp_path);
                return Err(retry_err);
            }
        } else {
            let _ = fs::remove_file(&temp_path);
            return Err(e);
        }
    }

    Ok(())
}

/// Metadata record for a single cached lyrics entry in `index.json`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LyricsIndexEntry {
    pub key: String,
    pub file: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub spotify_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub track_id: Option<String>,
    pub sync_type: SyncType,
    #[serde(default)]
    pub source: Option<String>,
    pub cached_at: u64,
    pub last_accessed_at: u64,
    pub size_bytes: u64,
}

fn default_index_version() -> u32 {
    1
}

/// Persistent root index stored at `$XDG_CACHE_HOME/ansimuz/lyrics/index.json`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LyricsIndex {
    #[serde(default = "default_index_version")]
    pub version: u32,
    #[serde(default)]
    pub entries: HashMap<String, LyricsIndexEntry>,
}

impl Default for LyricsIndex {
    fn default() -> Self {
        Self {
            version: default_index_version(),
            entries: HashMap::new(),
        }
    }
}

/// Persistent disk cache managing lyrics metadata in `index.json`
/// and serialized normalized payloads in `entries/`.
#[derive(Debug)]
pub struct LyricsCache {
    base_dir: PathBuf,
    entries_dir: PathBuf,
    index_path: PathBuf,
    index: LyricsIndex,
    max_cache_bytes: u64,
}

impl LyricsCache {
    /// Loads or initializes the lyrics cache with a custom or default directory.
    pub fn try_load(
        custom_base_dir: Option<PathBuf>,
        max_cache_bytes: u64,
    ) -> std::io::Result<Self> {
        let base_dir = custom_base_dir.unwrap_or_else(crate::config::lyrics_cache_dir);
        let entries_dir = base_dir.join("entries");
        let index_path = base_dir.join("index.json");

        fs::create_dir_all(&entries_dir)?;

        let (index, corrupt) = match fs::read_to_string(&index_path) {
            Ok(data) => match serde_json::from_str::<LyricsIndex>(&data) {
                Ok(parsed) => (parsed, false),
                Err(_) => (LyricsIndex::default(), true),
            },
            Err(_) => (LyricsIndex::default(), false),
        };

        let cache = Self {
            base_dir,
            entries_dir,
            index_path,
            index,
            max_cache_bytes,
        };

        if corrupt {
            let _ = cache.save_index();
        }

        Ok(cache)
    }

    /// Returns a reference to the base directory of the lyrics cache.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Serializes and atomically commits `index.json` to disk.
    pub fn save_index(&self) -> std::io::Result<()> {
        let json_bytes = serde_json::to_vec_pretty(&self.index)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_atomic(&self.index_path, &json_bytes)
    }

    /// Finds a matching entry key for a given track, taking into account
    /// exact IDs, normalized title/artist, and recording duration to disambiguate
    /// duplicate recordings (such as live vs studio versions).
    pub fn find_entry_key(&self, track: &Track) -> Option<String> {
        // Direct primary key match
        let primary = make_primary_key(track);
        if self.index.entries.contains_key(&primary) {
            return Some(primary);
        }

        // Exact spotify_id match
        if let Some(ref sid) = track.spotify_id {
            let s_trim = sid.trim();
            for (key, entry) in &self.index.entries {
                if entry.spotify_id.as_deref() == Some(s_trim) {
                    return Some(key.clone());
                }
            }
        }

        // Exact provider + track_id match
        if let Some(ref tid) = track.track_id {
            let tid_trim = tid.trim();
            let p_trim = track.provider.trim();
            for (key, entry) in &self.index.entries {
                if entry.provider.as_deref() == Some(p_trim)
                    && entry.track_id.as_deref() == Some(tid_trim)
                {
                    return Some(key.clone());
                }
            }
        }

        // Normalized title + artist match with recording duration disambiguation
        let norm_artist = normalize_key_text(&track.artist);
        let norm_title = normalize_key_text(&track.title);

        let mut fallback_candidate = None;
        for (key, entry) in &self.index.entries {
            let e_artist = normalize_key_text(&entry.artist);
            let e_title = normalize_key_text(&entry.title);

            if e_artist == norm_artist && e_title == norm_title {
                match (track.duration_ms, entry.duration_ms) {
                    (Some(t_dur), Some(e_dur)) => {
                        let diff = (t_dur as i64 - e_dur as i64).unsigned_abs();
                        if diff <= DURATION_TOLERANCE_MS {
                            return Some(key.clone());
                        }
                        // Duration mismatch indicates a different recording (live version)
                    }
                    _ => {
                        if fallback_candidate.is_none() {
                            fallback_candidate = Some(key.clone());
                        }
                    }
                }
            }
        }

        fallback_candidate
    }

    /// Loads and validates cached lyrics for a track, handling corruption transparently.
    pub fn get(&mut self, track: &Track) -> Option<Lyrics> {
        let key = self.find_entry_key(track)?;
        let entry = self.index.entries.get(&key)?;
        let file_path = self.entries_dir.join(&entry.file);

        let bytes = match fs::read(&file_path) {
            Ok(b) => b,
            Err(_) => {
                // Missing entry file -> purge stale index entry
                self.index.entries.remove(&key);
                let _ = self.save_index();
                return None;
            }
        };

        let lyrics: Lyrics = match serde_json::from_slice(&bytes) {
            Ok(l) => l,
            Err(_) => {
                // Corrupted JSON payload -> purge file and index entry
                let _ = fs::remove_file(&file_path);
                self.index.entries.remove(&key);
                let _ = self.save_index();
                return None;
            }
        };

        if lyrics.validate().is_err() {
            // Corrupted/inconsistent timestamp timing -> purge
            let _ = fs::remove_file(&file_path);
            self.index.entries.remove(&key);
            let _ = self.save_index();
            return None;
        }

        // Cache hit: update LRU access timestamp
        if let Some(entry_mut) = self.index.entries.get_mut(&key) {
            entry_mut.last_accessed_at = current_unix_timestamp();
            let _ = self.save_index();
        }

        Some(lyrics)
    }

    /// Stores normalized lyrics in the cache, updates `index.json`, and prunes LRU if needed.
    pub fn store(&mut self, track: &Track, lyrics: &Lyrics) -> std::io::Result<()> {
        let primary_key = make_primary_key(track);
        let file_name = entry_file_name(&primary_key);
        let file_path = self.entries_dir.join(&file_name);

        let json_bytes = serde_json::to_vec_pretty(lyrics)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        write_atomic(&file_path, &json_bytes)?;

        let now = current_unix_timestamp();
        let size_bytes = json_bytes.len() as u64;

        let entry = LyricsIndexEntry {
            key: primary_key.clone(),
            file: file_name,
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_ms: track.duration_ms,
            isrc: lyrics.isrc.clone(),
            spotify_id: track.spotify_id.clone(),
            provider: Some(track.provider.clone()),
            track_id: track.track_id.clone(),
            sync_type: lyrics.sync_type,
            source: lyrics.source.clone(),
            cached_at: now,
            last_accessed_at: now,
            size_bytes,
        };

        self.index.entries.insert(primary_key, entry);
        self.save_index()?;
        let _ = self.prune();
        Ok(())
    }

    /// Invalidates and removes cached lyrics for a specific track.
    pub fn invalidate(&mut self, track: &Track) -> bool {
        if let Some(key) = self.find_entry_key(track)
            && let Some(entry) = self.index.entries.remove(&key)
        {
            let file_path = self.entries_dir.join(&entry.file);
            let _ = fs::remove_file(file_path);
            let _ = self.save_index();
            return true;
        }
        false
    }

    /// Completely clears all cached lyrics entries and resets the index.
    pub fn clear(&mut self) -> std::io::Result<()> {
        if self.entries_dir.exists() {
            let _ = fs::remove_dir_all(&self.entries_dir);
            let _ = fs::create_dir_all(&self.entries_dir);
        }
        self.index.entries.clear();
        self.save_index()
    }

    /// Enforces `max_cache_bytes` by deleting least-recently-accessed entries.
    pub fn prune(&mut self) -> std::io::Result<u64> {
        let mut total_size: u64 = self.index.entries.values().map(|e| e.size_bytes).sum();
        if total_size <= self.max_cache_bytes {
            return Ok(0);
        }

        let mut keys_by_access: Vec<(String, u64, u64)> = self
            .index
            .entries
            .iter()
            .map(|(k, e)| (k.clone(), e.last_accessed_at, e.size_bytes))
            .collect();
        keys_by_access.sort_by_key(|(_, accessed, _)| *accessed);

        let mut bytes_pruned = 0;
        let mut removed_keys = Vec::new();

        for (k, _, size) in keys_by_access {
            if total_size <= self.max_cache_bytes {
                break;
            }
            if let Some(entry) = self.index.entries.get(&k) {
                let file_path = self.entries_dir.join(&entry.file);
                let _ = fs::remove_file(file_path);
            }
            total_size = total_size.saturating_sub(size);
            bytes_pruned += size;
            removed_keys.push(k);
        }

        for k in removed_keys {
            self.index.entries.remove(&k);
        }

        self.save_index()?;
        Ok(bytes_pruned)
    }

    /// Returns the number of cached entries.
    pub fn len(&self) -> usize {
        self.index.entries.len()
    }

    /// Returns true if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.index.entries.is_empty()
    }

    /// Returns total size of cached entry files in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.index.entries.values().map(|e| e.size_bytes).sum()
    }
}

/// Retrieves cached lyrics for a track from the global cache singleton, if present and valid.
pub fn get_lyrics_from_cache(track: &Track) -> Option<Lyrics> {
    let mut lock = GLOBAL_CACHE.lock().ok()?;
    if lock.is_none() {
        *lock = LyricsCache::try_load(None, DEFAULT_MAX_LYRICS_CACHE_BYTES).ok();
    }
    lock.as_mut()?.get(track)
}

/// Stores normalized lyrics for a track in the global cache singleton.
pub fn store_lyrics_in_cache(track: &Track, lyrics: &Lyrics) {
    if let Ok(mut lock) = GLOBAL_CACHE.lock() {
        if lock.is_none() {
            *lock = LyricsCache::try_load(None, DEFAULT_MAX_LYRICS_CACHE_BYTES).ok();
        }
        if let Some(cache) = lock.as_mut() {
            let _ = cache.store(track, lyrics);
        }
    }
}

/// Invalidates cached lyrics for a track in the global cache singleton.
pub fn invalidate_track_in_cache(track: &Track) {
    if let Ok(mut lock) = GLOBAL_CACHE.lock()
        && let Some(cache) = lock.as_mut()
    {
        let _ = cache.invalidate(track);
    }
}

/// Clears the in-memory global lyrics cache singleton reference.
pub fn clear_memory_cache() {
    if let Ok(mut lock) = GLOBAL_CACHE.lock() {
        *lock = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::LyricLine;

    fn make_test_lyrics(title: &str, artist: &str) -> Lyrics {
        Lyrics {
            source: Some("lrc.red".to_string()),
            sync_type: SyncType::Line,
            title: Some(title.to_string()),
            artist: Some(artist.to_string()),
            isrc: Some("US1234567890".to_string()),
            lines: vec![
                LyricLine::new("First line of song", Some(1000), Some(3000)),
                LyricLine::new("Second line of song", Some(3500), Some(6000)),
            ],
            unavailable_reason: None,
        }
    }

    #[test]
    fn test_stable_cache_key_generation() {
        let t1 = Track::new("Blinding Lights", "The Weeknd")
            .with_provider("spotify")
            .with_spotify_id("0VjIjW4GlUZAMYd2vXMi3b");
        assert_eq!(make_primary_key(&t1), "spotify:0VjIjW4GlUZAMYd2vXMi3b");

        let t2 = Track::new("Song", "Artist")
            .with_provider("ytmusic")
            .with_track_id("dQw4w9WgXcQ");
        assert_eq!(make_primary_key(&t2), "ytmusic:dQw4w9WgXcQ");

        let mut t3 = Track::new("  Hotel California  ", "  Eagles  ");
        t3.duration_ms = Some(390_000);
        assert_eq!(make_primary_key(&t3), "norm:eagles:hotel california:390s");

        let t4 = Track::new("Plain Song", "Plain Artist");
        assert_eq!(make_primary_key(&t4), "norm:plain artist:plain song");
    }

    #[test]
    fn test_store_and_load_cached_lyrics() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-store-{}",
            current_unix_timestamp()
        ));
        let mut cache = LyricsCache::try_load(Some(temp_dir.clone()), 10 * 1024 * 1024).unwrap();

        let track = Track::new("Starboy", "The Weeknd").with_track_id("starboy-yt");
        let lyrics = make_test_lyrics("Starboy", "The Weeknd");

        // Cache miss initially
        assert!(cache.get(&track).is_none());

        // Store
        cache.store(&track, &lyrics).expect("Store must succeed");
        assert_eq!(cache.len(), 1);

        // Cache hit
        let loaded = cache.get(&track).expect("Must find cached lyrics");
        assert_eq!(loaded.title.as_deref(), Some("Starboy"));
        assert_eq!(loaded.lines.len(), 2);
        assert_eq!(loaded.source.as_deref(), Some("lrc.red"));

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_disambiguate_duplicate_recordings_by_duration() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-dup-{}",
            current_unix_timestamp()
        ));
        let mut cache = LyricsCache::try_load(Some(temp_dir.clone()), 10 * 1024 * 1024).unwrap();

        // Studio version (6:22 = 382s)
        let mut studio_track = Track::new("Comfortably Numb", "Pink Floyd");
        studio_track.duration_ms = Some(382_000);
        let studio_lyrics = make_test_lyrics("Comfortably Numb", "Pink Floyd");
        cache
            .store(&studio_track, &studio_lyrics)
            .expect("store studio");

        // Live version (9:31 = 571s) -> distinct recording duration
        let mut live_track = Track::new("Comfortably Numb", "Pink Floyd");
        live_track.duration_ms = Some(571_000);

        // Looking up live version should NOT match studio version!
        assert!(
            cache.get(&live_track).is_none(),
            "Live recording with 9:31 duration must not falsely match 6:22 studio recording"
        );

        // Remaster with 2s difference (384s) -> within 5s tolerance
        let mut remaster_track = Track::new("Comfortably Numb", "Pink Floyd");
        remaster_track.duration_ms = Some(384_000);
        assert!(
            cache.get(&remaster_track).is_some(),
            "Remaster within tolerance should match same recording"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_handle_corrupted_entry_file() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-corrupt-{}",
            current_unix_timestamp()
        ));
        let mut cache = LyricsCache::try_load(Some(temp_dir.clone()), 10 * 1024 * 1024).unwrap();

        let track = Track::new("Corrupt Me", "Artist").with_track_id("corrupt-123");
        let lyrics = make_test_lyrics("Corrupt Me", "Artist");
        cache.store(&track, &lyrics).unwrap();

        // Corrupt the entry file on disk
        let key = make_primary_key(&track);
        let file_name = entry_file_name(&key);
        let entry_path = cache.entries_dir.join(&file_name);
        fs::write(&entry_path, b"INVALID_GARBAGE_JSON{{{").unwrap();

        // get() should handle corrupted JSON, delete it, remove from index, and return None
        assert!(cache.get(&track).is_none());
        assert!(!entry_path.exists(), "Corrupted entry file must be purged");
        assert_eq!(cache.len(), 0, "Index must be cleaned of corrupted entry");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_handle_invalid_lyrics_data() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-invalid-{}",
            current_unix_timestamp()
        ));
        let mut cache = LyricsCache::try_load(Some(temp_dir.clone()), 10 * 1024 * 1024).unwrap();

        let track = Track::new("Invalid Timestamps", "Artist").with_track_id("invalid-123");
        // Create invalid lyrics with end_ms < start_ms
        let invalid_lyrics = Lyrics {
            source: Some("lrc.red".to_string()),
            sync_type: SyncType::Line,
            title: Some("Invalid Timestamps".to_string()),
            artist: Some("Artist".to_string()),
            isrc: None,
            lines: vec![LyricLine::new("Invalid line", Some(5000), Some(2000))],
            unavailable_reason: None,
        };
        cache.store(&track, &invalid_lyrics).unwrap();

        // Validation should fail on get(), purging corrupted entry
        assert!(cache.get(&track).is_none());
        assert_eq!(cache.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_corrupt_index_recovery() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-corrupt-index-{}",
            current_unix_timestamp()
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("index.json"), b"{CORRUPT_INDEX_DATA]").unwrap();

        // Loading corrupt index should gracefully reset without error
        let cache = LyricsCache::try_load(Some(temp_dir.clone()), 10 * 1024 * 1024);
        assert!(cache.is_ok());
        let cache = cache.unwrap();
        assert_eq!(cache.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_invalidation_and_clear() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-invalidate-{}",
            current_unix_timestamp()
        ));
        let mut cache = LyricsCache::try_load(Some(temp_dir.clone()), 10 * 1024 * 1024).unwrap();

        let track1 = Track::new("Song One", "Artist One").with_track_id("t1");
        let track2 = Track::new("Song Two", "Artist Two").with_track_id("t2");
        cache
            .store(&track1, &make_test_lyrics("Song One", "Artist One"))
            .unwrap();
        cache
            .store(&track2, &make_test_lyrics("Song Two", "Artist Two"))
            .unwrap();
        assert_eq!(cache.len(), 2);

        // Invalidate track1
        assert!(cache.invalidate(&track1));
        assert!(cache.get(&track1).is_none());
        assert!(cache.get(&track2).is_some());
        assert_eq!(cache.len(), 1);

        // Clear all
        cache.clear().expect("clear must succeed");
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&track2).is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lru_pruning() {
        let _guard = LYRICS_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let temp_dir = std::env::temp_dir().join(format!(
            "ansimuz-lyrics-test-lru-{}",
            current_unix_timestamp()
        ));
        // Capacity limited to 1000 bytes
        let mut cache = LyricsCache::try_load(Some(temp_dir.clone()), 1000).unwrap();

        let t1 = Track::new("Track 1", "Artist").with_track_id("t1");
        let t2 = Track::new("Track 2", "Artist").with_track_id("t2");
        let t3 = Track::new("Track 3", "Artist").with_track_id("t3");

        cache
            .store(&t1, &make_test_lyrics("Track 1", "Artist"))
            .unwrap();
        cache
            .store(&t2, &make_test_lyrics("Track 2", "Artist"))
            .unwrap();
        cache
            .store(&t3, &make_test_lyrics("Track 3", "Artist"))
            .unwrap();

        // Total size should be kept within capacity
        assert!(cache.total_bytes() <= 1000);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
