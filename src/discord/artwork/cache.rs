use super::types::{ArtworkRequest, ArtworkResult, CacheIndex, CacheRecord, HostingProvider};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_CACHE_BYTES: u64 = 250 * 1024 * 1024; // 250 MB ceiling

/// Returns the default base directory for Discord artwork cache.
#[must_use]
pub fn default_cache_dir() -> PathBuf {
    crate::config::cache_base_dir().join("discord").join("artwork")
}

/// Compute hex-encoded SHA-256 hash of bytes.
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

/// Primary lookup key for provider + track ID.
#[must_use]
pub fn make_primary_key(provider: &str, track_id: Option<&str>) -> Option<String> {
    track_id.map(|id| format!("{provider}:{id}"))
}

/// Fallback lookup key based on normalized source URL SHA-256 hash.
#[must_use]
pub fn make_url_key(source_url: &str) -> String {
    let hash = sha256_hex(source_url.trim().as_bytes());
    format!("url:{hash}")
}

/// Validates whether a cache record's hosted URL is still active.
#[must_use]
pub fn is_record_valid(record: &CacheRecord, now_secs: u64) -> bool {
    match record.host {
        HostingProvider::Litterbox => {
            // For Litterbox entries, expires_at must be mandatory
            match record.expires_at {
                Some(expires) => expires > now_secs,
                None => false,
            }
        }
        HostingProvider::Catbox => {
            // Catbox links are permanent unless an expiry is explicitly specified
            match record.expires_at {
                Some(expires) => expires > now_secs,
                None => true,
            }
        }
    }
}

/// Persistent artwork cache managing `index.json` and the `images/` directory.
#[derive(Debug)]
pub struct ArtworkCache {
    base_dir: PathBuf,
    images_dir: PathBuf,
    index_path: PathBuf,
    index: CacheIndex,
    content_hash_index: HashMap<String, String>,
    max_cache_bytes: u64,
}

impl ArtworkCache {
    /// Initialize or load the artwork cache with a specific storage ceiling (default 250 MB).
    pub fn load_with_capacity(custom_base_dir: Option<PathBuf>, max_cache_bytes: u64) -> Self {
        let base_dir = custom_base_dir.unwrap_or_else(default_cache_dir);
        let images_dir = base_dir.join("images");
        let index_path = base_dir.join("index.json");

        let _ = fs::create_dir_all(&images_dir);

        let index = if index_path.exists() {
            fs::read_to_string(&index_path)
                .ok()
                .and_then(|data| serde_json::from_str::<CacheIndex>(&data).ok())
                .unwrap_or_default()
        } else {
            CacheIndex::default()
        };

        let mut cache = Self {
            base_dir,
            images_dir,
            index_path,
            index,
            content_hash_index: HashMap::new(),
            max_cache_bytes,
        };

        // Validate persistent cache and rebuild lookup maps on startup
        let now = current_unix_timestamp();
        cache.validate_and_rebuild_indexes(now);
        let _ = cache.prune_lru();

        cache
    }

    /// Initialize or load the artwork cache from the given directory (or default).
    pub fn load(custom_base_dir: Option<PathBuf>) -> Self {
        Self::load_with_capacity(custom_base_dir, DEFAULT_MAX_CACHE_BYTES)
    }

    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    #[must_use]
    pub fn images_dir(&self) -> &Path {
        &self.images_dir
    }

    #[must_use]
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    #[must_use]
    pub fn entries(&self) -> &HashMap<String, CacheRecord> {
        &self.index.entries
    }

    #[must_use]
    pub fn content_hash_index(&self) -> &HashMap<String, String> {
        &self.content_hash_index
    }

    /// On-startup validation:
    /// 1. Read index.json (already deserialized in `load`).
    /// 2. Check that referenced local files exist.
    /// 3. Remove invalid local entries.
    /// 4. Check expiry timestamps for Litterbox (purge expired).
    /// 5. Retain valid Catbox mappings.
    /// 6. Rebuild in-memory lookup maps (content_hash index).
    pub fn validate_and_rebuild_indexes(&mut self, now_secs: u64) -> usize {
        let mut removed = 0;
        let mut valid_entries = HashMap::new();
        self.content_hash_index.clear();

        for (key, record) in self.index.entries.drain() {
            let local_path = self.base_dir.join(&record.local_file);

            // 2 & 3. Check that referenced local file exists
            if !local_path.is_file() {
                removed += 1;
                continue;
            }

            // 4 & 5. Check expiry timestamps for Litterbox; retain valid Catbox mappings
            if !is_record_valid(&record, now_secs) {
                let _ = fs::remove_file(&local_path);
                removed += 1;
                continue;
            }

            // 6. Rebuild in-memory content hash index
            self.content_hash_index
                .insert(record.content_hash.clone(), key.clone());
            valid_entries.insert(key, record);
        }

        self.index.entries = valid_entries;

        if removed > 0 {
            let _ = self.save_index();
        }

        removed
    }

    /// Storage management: LRU cleanup policy with a configurable cache-size ceiling (e.g. 250 MB).
    pub fn prune_lru(&mut self) -> std::io::Result<u64> {
        if !self.images_dir.exists() {
            return Ok(0);
        }

        let mut files = Vec::new();
        let mut total_size = 0u64;

        for entry in fs::read_dir(&self.images_dir)?.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file()
                && let Ok(meta) = entry.metadata()
            {
                let size = meta.len();
                total_size += size;
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push((path, size, mtime));
            }
        }

        if total_size <= self.max_cache_bytes {
            return Ok(0);
        }

        // Sort oldest modified first
        files.sort_by_key(|(_, _, mtime)| *mtime);

        let mut bytes_pruned = 0u64;
        let mut pruned_rel_paths = Vec::new();

        for (path, size, _) in files {
            if total_size <= self.max_cache_bytes {
                break;
            }
            if let Ok(rel) = path.strip_prefix(&self.base_dir) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                pruned_rel_paths.push(rel_str);
            }
            if fs::remove_file(&path).is_ok() {
                total_size = total_size.saturating_sub(size);
                bytes_pruned += size;
            }
        }

        // Clean up corresponding index entries
        if !pruned_rel_paths.is_empty() {
            self.index.entries.retain(|_, record| {
                let keep = !pruned_rel_paths.iter().any(|p| p == &record.local_file);
                if !keep {
                    self.content_hash_index.remove(&record.content_hash);
                }
                keep
            });
            let _ = self.save_index();
        }

        Ok(bytes_pruned)
    }

    /// Cache lookup implementing the exact lookup order:
    /// 1. Find the provider + track ID entry.
    /// 2. If not found, check the source URL hash.
    /// 3. Check that the local file exists.
    /// 4. Check whether the hosted URL is still valid according to its hosting provider.
    /// 5. Return the cached result if valid.
    /// 6. Otherwise, resolve the artwork again (returns None).
    pub fn lookup(&self, req: &ArtworkRequest, now_secs: u64) -> Option<ArtworkResult> {
        let mut candidate = None;

        // 1. Find provider + track ID entry
        if let Some(key) = make_primary_key(&req.provider, req.track_id.as_deref())
            && let Some(record) = self.index.entries.get(&key)
        {
            candidate = Some(record);
        }

        // 2. If not found, check source URL hash
        if candidate.is_none() {
            let url_key = make_url_key(&req.source_url);
            if let Some(record) = self.index.entries.get(&url_key) {
                candidate = Some(record);
            } else {
                // Also search entries by matching source_url directly
                candidate = self.index.entries.values().find(|r| r.source_url == req.source_url);
            }
        }

        let record = candidate?;

        // 3. Check that the local file exists
        let full_local_path = self.base_dir.join(&record.local_file);
        if !full_local_path.is_file() {
            return None;
        }

        // 4. Check whether the hosted URL is still valid according to its hosting provider
        if !is_record_valid(record, now_secs) {
            return None;
        }

        // 5. Return the cached result if valid
        Some(ArtworkResult {
            source_url: req.source_url.clone(),
            hosted_url: record.hosted_url.clone(),
            local_path: full_local_path,
            content_hash: record.content_hash.clone(),
        })
    }

    /// Check if another cached track with identical image content hash already has a valid hosted URL.
    /// Reuses that hosted URL to avoid duplicate third-party uploads.
    pub fn find_reusable_upload(&self, content_hash: &str, now_secs: u64) -> Option<&CacheRecord> {
        // Fast O(1) in-memory index check
        if let Some(key) = self.content_hash_index.get(content_hash)
            && let Some(record) = self.index.entries.get(key)
            && is_record_valid(record, now_secs)
            && self.base_dir.join(&record.local_file).is_file()
        {
            return Some(record);
        }

        // Fallback scan
        self.index.entries.values().find(|record| {
            record.content_hash == content_hash
                && is_record_valid(record, now_secs)
                && self.base_dir.join(&record.local_file).is_file()
        })
    }

    /// Insert a record for a request and atomically persist `index.json`.
    pub fn insert_and_save(&mut self, req: &ArtworkRequest, record: CacheRecord) -> std::io::Result<()> {
        let content_hash = record.content_hash.clone();

        if let Some(key) = make_primary_key(&req.provider, req.track_id.as_deref()) {
            self.content_hash_index.insert(content_hash.clone(), key.clone());
            self.index.entries.insert(key, record.clone());
        }

        let url_key = make_url_key(&req.source_url);
        self.content_hash_index.insert(content_hash, url_key.clone());
        self.index.entries.insert(url_key, record);

        self.save_index()?;
        let _ = self.prune_lru();
        Ok(())
    }

    /// Write index updates atomically:
    /// writes a temporary JSON file, flushes it, then renames it over the existing index.
    pub fn save_index(&self) -> std::io::Result<()> {
        if let Some(parent) = self.index_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json_bytes = serde_json::to_vec_pretty(&self.index)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let temp_path = self.base_dir.join(format!(
            "index.tmp-{}-{}.part",
            std::process::id(),
            current_unix_timestamp()
        ));

        // Write and flush
        fs::write(&temp_path, &json_bytes)?;

        // Rename atomically over index.json
        if let Err(e) = fs::rename(&temp_path, &self.index_path) {
            if e.kind() == std::io::ErrorKind::AlreadyExists
                || e.kind() == std::io::ErrorKind::PermissionDenied
            {
                let _ = fs::remove_file(&self.index_path);
                if let Err(retry_err) = fs::rename(&temp_path, &self.index_path) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cache(dir_name: &str) -> ArtworkCache {
        let temp_dir = std::env::temp_dir().join(format!("ansimuz_test_cache_{dir_name}"));
        let _ = fs::remove_dir_all(&temp_dir);
        ArtworkCache::load(Some(temp_dir))
    }

    #[test]
    fn cache_lookup_flow_and_expiration() {
        let mut cache = create_test_cache("flow");
        let now = 100_000u64;

        let local_rel = "images/test.jpg";
        let local_abs = cache.base_dir().join(local_rel);
        fs::create_dir_all(local_abs.parent().unwrap()).unwrap();
        fs::write(&local_abs, b"image-data").unwrap();

        let req = ArtworkRequest::new("ytmusic", Some("vid123".into()), "https://example.com/art.jpg");

        // Litterbox valid record
        let record = CacheRecord {
            source_url: req.source_url.clone(),
            local_file: local_rel.to_string(),
            hosted_url: "https://litterbox.catbox.moe/art.jpg".to_string(),
            host: HostingProvider::Litterbox,
            content_hash: "hash123".to_string(),
            uploaded_at: now,
            expires_at: Some(now + 3600),
        };

        cache.insert_and_save(&req, record).unwrap();

        // Primary key hit
        let hit = cache.lookup(&req, now).expect("expected cache hit");
        assert_eq!(hit.hosted_url, "https://litterbox.catbox.moe/art.jpg");
        assert_eq!(hit.local_path, local_abs);

        // Fallback URL key hit (without track_id)
        let req_no_id = ArtworkRequest::new("ytmusic", None, "https://example.com/art.jpg");
        let hit_url = cache.lookup(&req_no_id, now).expect("expected url key hit");
        assert_eq!(hit_url.hosted_url, "https://litterbox.catbox.moe/art.jpg");

        // Litterbox expired -> lookup returns None
        assert!(cache.lookup(&req, now + 3601).is_none());

        // Missing local file -> lookup returns None
        fs::remove_file(&local_abs).unwrap();
        assert!(cache.lookup(&req, now).is_none());

        // Cleanup
        let _ = fs::remove_dir_all(cache.base_dir());
    }

    #[test]
    fn startup_validation_purges_missing_local_files_and_expired_litterbox() {
        let temp_dir = std::env::temp_dir().join("ansimuz_test_validation");
        let _ = fs::remove_dir_all(&temp_dir);

        let now = 200_000u64;

        // Setup manually an index with 3 entries:
        //
        // 1. Valid Catbox with local file
        // 2. Catbox with missing local file
        // 3. Expired Litterbox with local file
        let images_dir = temp_dir.join("images");
        fs::create_dir_all(&images_dir).unwrap();

        let valid_cat_file = images_dir.join("cat.jpg");
        fs::write(&valid_cat_file, b"cat-bytes").unwrap();

        let expired_lit_file = images_dir.join("lit.jpg");
        fs::write(&expired_lit_file, b"lit-bytes").unwrap();

        let mut index = CacheIndex::default();
        index.entries.insert("ytmusic:valid_cat".into(), CacheRecord {
            source_url: "https://example.com/cat.jpg".into(),
            local_file: "images/cat.jpg".into(),
            hosted_url: "https://files.catbox.moe/cat.jpg".into(),
            host: HostingProvider::Catbox,
            content_hash: "cat_hash".into(),
            uploaded_at: now - 1000,
            expires_at: None,
        });

        index.entries.insert("ytmusic:missing_file".into(), CacheRecord {
            source_url: "https://example.com/missing.jpg".into(),
            local_file: "images/missing.jpg".into(),
            hosted_url: "https://files.catbox.moe/missing.jpg".into(),
            host: HostingProvider::Catbox,
            content_hash: "missing_hash".into(),
            uploaded_at: now - 1000,
            expires_at: None,
        });

        index.entries.insert("ytmusic:expired_lit".into(), CacheRecord {
            source_url: "https://example.com/lit.jpg".into(),
            local_file: "images/lit.jpg".into(),
            hosted_url: "https://litterbox.catbox.moe/lit.jpg".into(),
            host: HostingProvider::Litterbox,
            content_hash: "lit_hash".into(),
            uploaded_at: now - 10_000,
            expires_at: Some(now - 100), // Expired!
        });

        fs::write(temp_dir.join("index.json"), serde_json::to_string_pretty(&index).unwrap()).unwrap();

        // Load cache which triggers startup validation
        let cache = ArtworkCache::load(Some(temp_dir.clone()));

        // Valid Catbox retained
        assert!(cache.entries().contains_key("ytmusic:valid_cat"));
        assert!(cache.content_hash_index().contains_key("cat_hash"));

        // Missing file entry purged
        assert!(!cache.entries().contains_key("ytmusic:missing_file"));

        // Expired Litterbox entry purged
        assert!(!cache.entries().contains_key("ytmusic:expired_lit"));
        assert!(!expired_lit_file.exists()); // Expired file removed from disk

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lru_cache_prune_enforces_ceiling() {
        let temp_dir = std::env::temp_dir().join("ansimuz_test_lru");
        let _ = fs::remove_dir_all(&temp_dir);

        // Ceiling 100 bytes
        let mut cache = ArtworkCache::load_with_capacity(Some(temp_dir.clone()), 100);

        let file1 = cache.images_dir().join("file1.jpg");
        let file2 = cache.images_dir().join("file2.jpg");

        fs::write(&file1, vec![0u8; 80]).unwrap();
        // Ensure distinct mtime
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&file2, vec![0u8; 80]).unwrap();

        let req1 = ArtworkRequest::new("ytmusic", Some("1".into()), "https://example.com/1.jpg");
        cache.insert_and_save(&req1, CacheRecord {
            source_url: "https://example.com/1.jpg".into(),
            local_file: "images/file1.jpg".into(),
            hosted_url: "https://files.catbox.moe/1.jpg".into(),
            host: HostingProvider::Catbox,
            content_hash: "hash1".into(),
            uploaded_at: 100,
            expires_at: None,
        }).unwrap();

        let req2 = ArtworkRequest::new("ytmusic", Some("2".into()), "https://example.com/2.jpg");
        cache.insert_and_save(&req2, CacheRecord {
            source_url: "https://example.com/2.jpg".into(),
            local_file: "images/file2.jpg".into(),
            hosted_url: "https://files.catbox.moe/2.jpg".into(),
            host: HostingProvider::Catbox,
            content_hash: "hash2".into(),
            uploaded_at: 200,
            expires_at: None,
        }).unwrap();

        // Total was 160 bytes > 100 bytes ceiling -> oldest file1 should be pruned
        assert!(!file1.exists());
        assert!(file2.exists());
        assert!(!cache.entries().contains_key("ytmusic:1"));
        assert!(cache.entries().contains_key("ytmusic:2"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
