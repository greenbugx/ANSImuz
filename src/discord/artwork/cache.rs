use super::types::{ArtworkRequest, ArtworkResult, CacheIndex, CacheRecord, HostingProvider};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_CACHE_BYTES: u64 = 250 * 1024 * 1024; // 250 MB ceiling

/// Returns the default base directory for Discord artwork cache.
#[must_use]
pub fn default_cache_dir() -> PathBuf {
    crate::config::cache_base_dir()
        .join("discord")
        .join("artwork")
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

/// Validates whether a cache record is locally valid according to its known metadata.
///
/// Litterbox entries always carry an explicit expiration timestamp (`expires_at`).
/// Catbox entries have no known expiry (`expires_at: None`) in the local cache model,
/// though anonymous uploads can still expire remotely after prolonged inactivity.
/// Callers should use [`ArtworkCache::invalidate`] or [`ArtworkCache::invalidate_hosted_url`]
/// to recover if remote loading fails.
#[must_use]
pub fn is_record_valid(record: &CacheRecord, now_secs: u64) -> bool {
    match record.host {
        HostingProvider::Litterbox => {
            // For Litterbox entries, an explicit expires_at timestamp is mandatory.
            match record.expires_at {
                Some(expires) => expires > now_secs,
                None => false,
            }
        }
        HostingProvider::Catbox => {
            // Catbox entries have no known expiry unless explicitly annotated
            // this reflects local cache state and does not guarantee permanent retention.
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
        if let Err(err) = cache.prune_lru() {
            crate::discord::log_discord(&format!("[Artwork] Startup LRU prune failed: {err}"));
        }

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

    fn rebuild_content_hash_index(&mut self) {
        self.content_hash_index.clear();
        for (key, record) in &self.index.entries {
            self.content_hash_index
                .insert(record.content_hash.clone(), key.clone());
        }
    }

    fn normalize_rel(path: &str) -> String {
        path.replace('\\', "/")
    }

    fn clean_orphaned_images(&self, include_part_files: bool) -> u64 {
        if !self.images_dir.exists() {
            return 0;
        }
        let referenced: HashSet<String> = self
            .index
            .entries
            .values()
            .map(|r| Self::normalize_rel(&r.local_file))
            .collect();

        let Ok(read_dir) = fs::read_dir(&self.images_dir) else {
            return 0;
        };

        let mut pruned_bytes = 0u64;
        for entry in read_dir.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !include_part_files && path.extension().and_then(|e| e.to_str()) == Some("part") {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&self.base_dir) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if !referenced.contains(&rel_str) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if fs::remove_file(&path).is_ok() {
                    pruned_bytes += size;
                }
            }
        }
        pruned_bytes
    }

    /// On-startup validation:
    /// 1. Read index.json (already deserialized in `load`).
    /// 2. Check that referenced local files exist.
    /// 3. Remove invalid local entries.
    /// 4. Check local expiry metadata (purge expired Litterbox entries without deleting shared files).
    /// 5. Retain Catbox mappings with no known expiry.
    /// 6. Rebuild in-memory lookup maps (content_hash index) and remove orphaned files.
    pub fn validate_and_rebuild_indexes(&mut self, now_secs: u64) -> usize {
        let mut removed = 0;
        let mut dirty = false;
        let mut valid_entries = HashMap::new();
        let mut expired_rel_files = Vec::new();
        self.content_hash_index.clear();

        for (key, mut record) in self.index.entries.drain() {
            let local_path = self.base_dir.join(&record.local_file);

            // Check that referenced local file exists
            if !local_path.is_file() {
                removed += 1;
                continue;
            }

            // Check expiry timestamps for Litterbox
            // retain Catbox entries with no known expiry
            if !is_record_valid(&record, now_secs) {
                expired_rel_files.push(Self::normalize_rel(&record.local_file));
                removed += 1;
                continue;
            }

            if record.last_accessed == 0 {
                record.last_accessed = record.uploaded_at;
                dirty = true;
            }

            // Rebuild in-memory content hash index
            self.content_hash_index
                .insert(record.content_hash.clone(), key.clone());
            valid_entries.insert(key, record);
        }

        self.index.entries = valid_entries;

        let referenced_files: HashSet<String> = self
            .index
            .entries
            .values()
            .map(|r| Self::normalize_rel(&r.local_file))
            .collect();

        // Only delete expired image files if no remaining valid entry still references them
        for rel in expired_rel_files {
            if !referenced_files.contains(&rel) {
                let _ = fs::remove_file(self.base_dir.join(&rel));
            }
        }

        // Clean any leftover orphaned files (including stale .part files) at startup
        self.clean_orphaned_images(true);

        if (removed > 0 || dirty)
            && let Err(err) = self.save_index()
        {
            crate::discord::log_discord(&format!(
                "[Artwork] Failed to save validated index: {err}"
            ));
        }

        removed
    }

    /// Storage management: LRU cleanup policy ordered by index `last_accessed` timestamps
    /// with a configurable cache-size ceiling (e.g. 250 MB).
    /// Cleans up orphaned files first and protects shared images still referenced by newer entries.
    pub fn prune_lru(&mut self) -> std::io::Result<u64> {
        if !self.images_dir.exists() {
            return Ok(0);
        }

        // Compute effective recency per local_file as the max last_accessed across all referencing entries.
        // This guarantees a shared image cannot be deleted prematurely while a more recent entry uses it.
        let mut file_recency: HashMap<String, u64> = HashMap::new();
        for record in self.index.entries.values() {
            let recency = record.last_accessed.max(record.uploaded_at);
            let rel = Self::normalize_rel(&record.local_file);
            file_recency
                .entry(rel)
                .and_modify(|cur| *cur = (*cur).max(recency))
                .or_insert(recency);
        }

        let mut files = Vec::new();
        let mut total_size = 0u64;
        let mut bytes_pruned = 0u64;

        for entry in fs::read_dir(&self.images_dir)?.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let size = meta.len();

            let Ok(rel) = path.strip_prefix(&self.base_dir) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            if let Some(&recency) = file_recency.get(&rel_str) {
                total_size += size;
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push((path, rel_str, size, recency, mtime));
            } else {
                // Skip active temporary writes
                // remove orphaned image files with no index entry
                if path.extension().and_then(|e| e.to_str()) != Some("part")
                    && fs::remove_file(&path).is_ok()
                {
                    bytes_pruned += size;
                }
            }
        }

        if total_size <= self.max_cache_bytes {
            return Ok(bytes_pruned);
        }

        // Sort by least recently accessed first (using filesystem mtime as deterministic tie-breaker)
        files.sort_by_key(|(_, _, _, recency, mtime)| (*recency, *mtime));

        let mut pruned_rel_paths = HashSet::new();

        for (path, rel_str, size, _, _) in files {
            if total_size <= self.max_cache_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total_size = total_size.saturating_sub(size);
                bytes_pruned += size;
                pruned_rel_paths.insert(rel_str);
            }
        }

        // Remove all index entries pointing to pruned files and rebuild content_hash_index
        if !pruned_rel_paths.is_empty() {
            self.index.entries.retain(|_, record| {
                !pruned_rel_paths.contains(&Self::normalize_rel(&record.local_file))
            });
            self.rebuild_content_hash_index();
            if let Err(err) = self.save_index() {
                crate::discord::log_discord(&format!(
                    "[Artwork] Failed to save pruned index: {err}"
                ));
            }
        }

        Ok(bytes_pruned)
    }

    /// Cache lookup implementing the exact lookup order:
    /// 1. Find the provider + track ID entry.
    /// 2. If not found, check the source URL hash.
    /// 3. Check that the local file exists.
    /// 4. Check whether the hosted URL is locally valid according to its known metadata.
    /// 5. Update `last_accessed` recency timestamp in the index on hit.
    /// 6. Return the cached result if valid, or `None` to re-resolve.
    pub fn lookup(&mut self, req: &ArtworkRequest, now_secs: u64) -> Option<ArtworkResult> {
        let pkey = make_primary_key(&req.provider, req.track_id.as_deref());
        let ukey = make_url_key(&req.source_url);

        let mut matched_key = None;

        // 1. Find provider + track ID entry
        if let Some(ref key) = pkey
            && self.index.entries.contains_key(key)
        {
            matched_key = Some(key.clone());
        }

        // 2. If not found, check source URL hash
        if matched_key.is_none() {
            if self.index.entries.contains_key(&ukey) {
                matched_key = Some(ukey.clone());
            } else {
                // Also search entries by matching source_url directly
                matched_key = self
                    .index
                    .entries
                    .iter()
                    .find(|(_, r)| r.source_url == req.source_url)
                    .map(|(k, _)| k.clone());
            }
        }

        let key = matched_key?;
        let record = self.index.entries.get(&key)?.clone();

        // 3. Check that the local file exists
        let full_local_path = self.base_dir.join(&record.local_file);
        if !full_local_path.is_file() {
            return None;
        }

        // 4. Check whether the hosted URL is locally valid according to its known metadata
        if !is_record_valid(&record, now_secs) {
            return None;
        }

        // 5. Update last_accessed recency timestamp for the hit keys
        let mut updated = false;
        if let Some(ref k) = pkey
            && let Some(entry) = self.index.entries.get_mut(k)
            && entry.last_accessed < now_secs
        {
            entry.last_accessed = now_secs;
            updated = true;
        }
        if let Some(entry) = self.index.entries.get_mut(&ukey)
            && entry.last_accessed < now_secs
        {
            entry.last_accessed = now_secs;
            updated = true;
        }
        if let Some(entry) = self.index.entries.get_mut(&key)
            && entry.last_accessed < now_secs
        {
            entry.last_accessed = now_secs;
            updated = true;
        }

        if updated && let Err(err) = self.save_index() {
            crate::discord::log_discord(&format!(
                "[Artwork] Failed to persist cache last_accessed timestamp: {err}"
            ));
        }

        // 6. Return the cached result if valid
        Some(ArtworkResult {
            source_url: req.source_url.clone(),
            hosted_url: record.hosted_url,
            local_path: full_local_path,
            content_hash: record.content_hash,
        })
    }

    /// Invalidate cached entries matching a request (e.g. if a remote image fails to load).
    /// Removes unreferenced local files only when no other cache entry still references them.
    pub fn invalidate(&mut self, req: &ArtworkRequest) -> bool {
        let pkey = make_primary_key(&req.provider, req.track_id.as_deref());
        let ukey = make_url_key(&req.source_url);

        let mut removed_any = false;
        self.index.entries.retain(|k, r| {
            let matches_pkey = pkey.as_deref() == Some(k.as_str());
            let matches_ukey = k == &ukey;
            let matches_url = r.source_url == req.source_url;
            if matches_pkey || matches_ukey || matches_url {
                removed_any = true;
                false
            } else {
                true
            }
        });

        if removed_any {
            self.rebuild_content_hash_index();
            self.clean_orphaned_images(false);
            if let Err(err) = self.save_index() {
                crate::discord::log_discord(&format!(
                    "[Artwork] Failed to save index after invalidation: {err}"
                ));
            }
        }

        removed_any
    }

    /// Invalidate all cache entries pointing to a specific remote hosted URL
    /// (for recovery if Discord or a CDN rejects an expired or purged URL).
    pub fn invalidate_hosted_url(&mut self, hosted_url: &str) -> usize {
        let before = self.index.entries.len();
        self.index.entries.retain(|_, r| r.hosted_url != hosted_url);
        let removed = before.saturating_sub(self.index.entries.len());

        if removed > 0 {
            self.rebuild_content_hash_index();
            self.clean_orphaned_images(false);
            if let Err(err) = self.save_index() {
                crate::discord::log_discord(&format!(
                    "[Artwork] Failed to save index after hosted URL invalidation: {err}"
                ));
            }
        }

        removed
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
    pub fn insert_and_save(
        &mut self,
        req: &ArtworkRequest,
        mut record: CacheRecord,
    ) -> std::io::Result<()> {
        if record.last_accessed == 0 {
            record.last_accessed = if record.uploaded_at > 0 {
                record.uploaded_at
            } else {
                current_unix_timestamp()
            };
        }

        let primary_key = make_primary_key(&req.provider, req.track_id.as_deref());
        let url_key = make_url_key(&req.source_url);

        let mut needs_hash_rebuild = false;
        let mut replaced_local_files = false;

        if let Some(ref pkey) = primary_key
            && let Some(prev) = self.index.entries.get(pkey).cloned()
        {
            needs_hash_rebuild = true;
            if Self::normalize_rel(&prev.local_file) != Self::normalize_rel(&record.local_file) {
                replaced_local_files = true;
            }
            let prev_trimmed = prev.source_url.trim();
            if prev_trimmed != req.source_url.trim() {
                let old_url_key = make_url_key(prev_trimmed);
                if old_url_key != url_key {
                    let still_used = self.index.entries.iter().any(|(k, r)| {
                        k != pkey && k != &old_url_key && r.source_url.trim() == prev_trimmed
                    });
                    if !still_used {
                        self.index.entries.remove(&old_url_key);
                    }
                }
            }
        }

        if let Some(prev_url) = self.index.entries.get(&url_key) {
            needs_hash_rebuild = true;
            if Self::normalize_rel(&prev_url.local_file) != Self::normalize_rel(&record.local_file)
            {
                replaced_local_files = true;
            }
        }

        let content_hash = record.content_hash.clone();

        if let Some(key) = primary_key {
            self.content_hash_index
                .insert(content_hash.clone(), key.clone());
            self.index.entries.insert(key, record.clone());
        }

        self.content_hash_index
            .insert(content_hash, url_key.clone());
        self.index.entries.insert(url_key, record);

        if needs_hash_rebuild {
            self.rebuild_content_hash_index();
        }
        if replaced_local_files {
            self.clean_orphaned_images(false);
        }

        self.save_index()?;
        if let Err(err) = self.prune_lru() {
            crate::discord::log_discord(&format!("[Artwork] Failed to prune LRU cache: {err}"));
        }
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

        let req = ArtworkRequest::new(
            "ytmusic",
            Some("vid123".into()),
            "https://example.com/art.jpg",
        );

        // Litterbox valid record
        let record = CacheRecord {
            source_url: req.source_url.clone(),
            local_file: local_rel.to_string(),
            hosted_url: "https://litterbox.catbox.moe/art.jpg".to_string(),
            host: HostingProvider::Litterbox,
            content_hash: "hash123".to_string(),
            uploaded_at: now,
            expires_at: Some(now + 3600),
            last_accessed: now,
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
        index.entries.insert(
            "ytmusic:valid_cat".into(),
            CacheRecord {
                source_url: "https://example.com/cat.jpg".into(),
                local_file: "images/cat.jpg".into(),
                hosted_url: "https://files.catbox.moe/cat.jpg".into(),
                host: HostingProvider::Catbox,
                content_hash: "cat_hash".into(),
                uploaded_at: now - 1000,
                expires_at: None,
                last_accessed: now - 1000,
            },
        );

        index.entries.insert(
            "ytmusic:missing_file".into(),
            CacheRecord {
                source_url: "https://example.com/missing.jpg".into(),
                local_file: "images/missing.jpg".into(),
                hosted_url: "https://files.catbox.moe/missing.jpg".into(),
                host: HostingProvider::Catbox,
                content_hash: "missing_hash".into(),
                uploaded_at: now - 1000,
                expires_at: None,
                last_accessed: now - 1000,
            },
        );

        index.entries.insert(
            "ytmusic:expired_lit".into(),
            CacheRecord {
                source_url: "https://example.com/lit.jpg".into(),
                local_file: "images/lit.jpg".into(),
                hosted_url: "https://litterbox.catbox.moe/lit.jpg".into(),
                host: HostingProvider::Litterbox,
                content_hash: "lit_hash".into(),
                uploaded_at: now - 10_000,
                expires_at: Some(now - 100), // Expired!
                last_accessed: now - 10_000,
            },
        );

        fs::write(
            temp_dir.join("index.json"),
            serde_json::to_string_pretty(&index).unwrap(),
        )
        .unwrap();

        // Load cache which triggers startup validation
        let cache = ArtworkCache::load(Some(temp_dir.clone()));

        // Valid Catbox retained (no known expiry)
        assert!(cache.entries().contains_key("ytmusic:valid_cat"));
        assert!(cache.entries()["ytmusic:valid_cat"].has_no_known_expiry());
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
        let req1 = ArtworkRequest::new("ytmusic", Some("1".into()), "https://example.com/1.jpg");
        cache
            .insert_and_save(
                &req1,
                CacheRecord {
                    source_url: "https://example.com/1.jpg".into(),
                    local_file: "images/file1.jpg".into(),
                    hosted_url: "https://files.catbox.moe/1.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash1".into(),
                    uploaded_at: 100,
                    expires_at: None,
                    last_accessed: 100,
                },
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&file2, vec![0u8; 80]).unwrap();
        let req2 = ArtworkRequest::new("ytmusic", Some("2".into()), "https://example.com/2.jpg");
        cache
            .insert_and_save(
                &req2,
                CacheRecord {
                    source_url: "https://example.com/2.jpg".into(),
                    local_file: "images/file2.jpg".into(),
                    hosted_url: "https://files.catbox.moe/2.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash2".into(),
                    uploaded_at: 200,
                    expires_at: None,
                    last_accessed: 200,
                },
            )
            .unwrap();

        // Total was 160 bytes > 100 bytes ceiling -> oldest file1 should be pruned
        assert!(!file1.exists());
        assert!(file2.exists());
        assert!(!cache.entries().contains_key("ytmusic:1"));
        assert!(cache.entries().contains_key("ytmusic:2"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lru_cache_hit_updates_recency_and_preserves_hot_item() {
        let temp_dir = std::env::temp_dir().join("ansimuz_test_lru_recency");
        let _ = fs::remove_dir_all(&temp_dir);

        // Ceiling 180 bytes (fits two 80-byte files, evicts on third)
        let mut cache = ArtworkCache::load_with_capacity(Some(temp_dir.clone()), 180);

        let file1 = cache.images_dir().join("file1.jpg");
        let file2 = cache.images_dir().join("file2.jpg");
        let file3 = cache.images_dir().join("file3.jpg");

        fs::write(&file1, vec![0u8; 80]).unwrap();
        let req1 = ArtworkRequest::new("ytmusic", Some("1".into()), "https://example.com/1.jpg");
        cache
            .insert_and_save(
                &req1,
                CacheRecord {
                    source_url: req1.source_url.clone(),
                    local_file: "images/file1.jpg".into(),
                    hosted_url: "https://files.catbox.moe/1.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash1".into(),
                    uploaded_at: 100,
                    expires_at: None,
                    last_accessed: 100,
                },
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&file2, vec![0u8; 80]).unwrap();
        let req2 = ArtworkRequest::new("ytmusic", Some("2".into()), "https://example.com/2.jpg");
        cache
            .insert_and_save(
                &req2,
                CacheRecord {
                    source_url: req2.source_url.clone(),
                    local_file: "images/file2.jpg".into(),
                    hosted_url: "https://files.catbox.moe/2.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash2".into(),
                    uploaded_at: 200,
                    expires_at: None,
                    last_accessed: 200,
                },
            )
            .unwrap();

        // Hit req1 at t=300: file1 still has an OLDER filesystem mtime than file2,
        // but its last_accessed timestamp is now newer (300 > 200).
        assert!(cache.lookup(&req1, 300).is_some());
        assert_eq!(cache.entries()["ytmusic:1"].last_accessed, 300);

        // Insert file3 -> total 240B > 180B ceiling -> file2 (last_accessed=200) must be evicted!
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&file3, vec![0u8; 80]).unwrap();
        let req3 = ArtworkRequest::new("ytmusic", Some("3".into()), "https://example.com/3.jpg");
        cache
            .insert_and_save(
                &req3,
                CacheRecord {
                    source_url: req3.source_url.clone(),
                    local_file: "images/file3.jpg".into(),
                    hosted_url: "https://files.catbox.moe/3.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash3".into(),
                    uploaded_at: 400,
                    expires_at: None,
                    last_accessed: 400,
                },
            )
            .unwrap();

        assert!(
            file1.exists(),
            "file1 was recently accessed and must survive LRU"
        );
        assert!(
            !file2.exists(),
            "file2 was least recently accessed and must be evicted"
        );
        assert!(file3.exists());
        assert!(cache.entries().contains_key("ytmusic:1"));
        assert!(!cache.entries().contains_key("ytmusic:2"));
        assert!(cache.entries().contains_key("ytmusic:3"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lru_prune_protects_shared_image_and_cleans_orphans() {
        let temp_dir = std::env::temp_dir().join("ansimuz_test_lru_shared_orphan");
        let _ = fs::remove_dir_all(&temp_dir);

        // Ceiling 100 bytes (fits one 80-byte file)
        let mut cache = ArtworkCache::load_with_capacity(Some(temp_dir.clone()), 100);

        let shared = cache.images_dir().join("shared.jpg");
        let other = cache.images_dir().join("other.jpg");
        let orphan = cache.images_dir().join("orphan.jpg");

        fs::write(&shared, vec![0u8; 80]).unwrap();
        let req_a = ArtworkRequest::new("ytmusic", Some("a".into()), "https://example.com/a.jpg");
        cache
            .insert_and_save(
                &req_a,
                CacheRecord {
                    source_url: req_a.source_url.clone(),
                    local_file: "images/shared.jpg".into(),
                    hosted_url: "https://files.catbox.moe/shared.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash_shared".into(),
                    uploaded_at: 100,
                    expires_at: None,
                    last_accessed: 100,
                },
            )
            .unwrap();

        // Hit req_a later? Keep req_a stale (100), add req_b referencing same shared file at 500
        let req_b = ArtworkRequest::new("ytmusic", Some("b".into()), "https://example.com/b.jpg");
        cache
            .insert_and_save(
                &req_b,
                CacheRecord {
                    source_url: req_b.source_url.clone(),
                    local_file: "images/shared.jpg".into(),
                    hosted_url: "https://files.catbox.moe/shared.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash_shared".into(),
                    uploaded_at: 500,
                    expires_at: None,
                    last_accessed: 500,
                },
            )
            .unwrap();

        // Insert another unique file with recency 200 (older than shared file's max recency 500)
        // plus an unindexed orphan file on disk.
        fs::write(&orphan, vec![0u8; 50]).unwrap();
        fs::write(&other, vec![0u8; 80]).unwrap();
        let req_c = ArtworkRequest::new("ytmusic", Some("c".into()), "https://example.com/c.jpg");
        cache
            .insert_and_save(
                &req_c,
                CacheRecord {
                    source_url: req_c.source_url.clone(),
                    local_file: "images/other.jpg".into(),
                    hosted_url: "https://files.catbox.moe/other.jpg".into(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash_other".into(),
                    uploaded_at: 200,
                    expires_at: None,
                    last_accessed: 200,
                },
            )
            .unwrap();

        assert!(
            !orphan.exists(),
            "Orphaned file must be removed during pruning"
        );
        assert!(!other.exists(), "Older unshared file must be evicted");
        assert!(
            shared.exists(),
            "Shared file kept alive by recent entry req_b must remain"
        );
        assert!(cache.entries().contains_key("ytmusic:b"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn startup_validation_protects_shared_file_when_expired_sibling_removed() {
        let temp_dir = std::env::temp_dir().join("ansimuz_test_val_shared");
        let _ = fs::remove_dir_all(&temp_dir);
        let images_dir = temp_dir.join("images");
        fs::create_dir_all(&images_dir).unwrap();

        let shared_file = images_dir.join("shared.jpg");
        fs::write(&shared_file, b"shared-bytes").unwrap();
        let now = 200_000u64;

        let mut index = CacheIndex::default();
        index.entries.insert(
            "ytmusic:expired_lit".into(),
            CacheRecord {
                source_url: "https://example.com/1.jpg".into(),
                local_file: "images/shared.jpg".into(),
                hosted_url: "https://litterbox.catbox.moe/exp.jpg".into(),
                host: HostingProvider::Litterbox,
                content_hash: "shared_hash".into(),
                uploaded_at: now - 10_000,
                expires_at: Some(now - 10),
                last_accessed: now - 10_000,
            },
        );
        index.entries.insert(
            "ytmusic:valid_cat".into(),
            CacheRecord {
                source_url: "https://example.com/2.jpg".into(),
                local_file: "images/shared.jpg".into(),
                hosted_url: "https://files.catbox.moe/keep.jpg".into(),
                host: HostingProvider::Catbox,
                content_hash: "shared_hash".into(),
                uploaded_at: now - 500,
                expires_at: None,
                last_accessed: 0, // Test backfill from uploaded_at
            },
        );

        fs::write(
            temp_dir.join("index.json"),
            serde_json::to_string_pretty(&index).unwrap(),
        )
        .unwrap();

        let cache = ArtworkCache::load(Some(temp_dir.clone()));
        assert!(!cache.entries().contains_key("ytmusic:expired_lit"));
        assert!(cache.entries().contains_key("ytmusic:valid_cat"));
        assert_eq!(
            cache.entries()["ytmusic:valid_cat"].last_accessed,
            now - 500
        );
        assert!(
            shared_file.exists(),
            "Shared image must not be deleted when expired sibling is purged"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn cache_invalidate_recovers_stale_catbox_entry() {
        let mut cache = create_test_cache("invalidate");
        let now = 100_000u64;

        let local_rel = "images/cat_stale.jpg";
        let local_abs = cache.base_dir().join(local_rel);
        fs::create_dir_all(local_abs.parent().unwrap()).unwrap();
        fs::write(&local_abs, b"image-data").unwrap();

        let req = ArtworkRequest::new("ytmusic", Some("v1".into()), "https://example.com/v1.jpg");
        cache
            .insert_and_save(
                &req,
                CacheRecord {
                    source_url: req.source_url.clone(),
                    local_file: local_rel.to_string(),
                    hosted_url: "https://files.catbox.moe/stale.jpg".to_string(),
                    host: HostingProvider::Catbox,
                    content_hash: "h_stale".to_string(),
                    uploaded_at: now,
                    expires_at: None,
                    last_accessed: now,
                },
            )
            .unwrap();

        assert!(cache.lookup(&req, now).is_some());
        assert!(cache.invalidate(&req));
        assert!(cache.lookup(&req, now).is_none());
        assert!(cache.find_reusable_upload("h_stale", now).is_none());

        let _ = fs::remove_dir_all(cache.base_dir());
    }

    #[test]
    fn insert_update_removes_stale_url_duplicate_and_refreshes_hash_index() {
        let mut cache = create_test_cache("update_dedup");
        let now = 100_000u64;

        let file1 = cache.images_dir().join("old.jpg");
        let file2 = cache.images_dir().join("new.jpg");
        fs::write(&file1, b"old-bytes").unwrap();

        let url1 = "https://example.com/old.jpg";
        let url2 = "https://example.com/new.jpg";

        let req_v1 = ArtworkRequest::new("spotify", Some("track_1".into()), url1);
        cache
            .insert_and_save(
                &req_v1,
                CacheRecord {
                    source_url: url1.to_string(),
                    local_file: "images/old.jpg".to_string(),
                    hosted_url: "https://files.catbox.moe/old.jpg".to_string(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash_old".to_string(),
                    uploaded_at: now,
                    expires_at: None,
                    last_accessed: now,
                },
            )
            .unwrap();

        let old_url_key = make_url_key(url1);
        assert!(cache.entries().contains_key("spotify:track_1"));
        assert!(cache.entries().contains_key(&old_url_key));

        // Update same track to new artwork URL/file/hash
        fs::write(&file2, b"new-bytes").unwrap();
        let req_v2 = ArtworkRequest::new("spotify", Some("track_1".into()), url2);
        cache
            .insert_and_save(
                &req_v2,
                CacheRecord {
                    source_url: url2.to_string(),
                    local_file: "images/new.jpg".to_string(),
                    hosted_url: "https://files.catbox.moe/new.jpg".to_string(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash_new".to_string(),
                    uploaded_at: now + 10,
                    expires_at: None,
                    last_accessed: now + 10,
                },
            )
            .unwrap();

        let new_url_key = make_url_key(url2);
        assert!(
            !cache.entries().contains_key(&old_url_key),
            "stale URL key must be removed"
        );
        assert!(cache.entries().contains_key(&new_url_key));
        assert_eq!(
            cache.entries()["spotify:track_1"].hosted_url,
            "https://files.catbox.moe/new.jpg"
        );
        assert!(
            !cache.content_hash_index().contains_key("hash_old"),
            "stale hash must be cleared"
        );
        assert!(cache.content_hash_index().contains_key("hash_new"));
        assert!(
            !file1.exists(),
            "unreferenced replaced file must be removed"
        );
        assert!(file2.exists());

        // When another track shares a URL, updating one track must keep the URL key alive for the sibling
        let shared_file = cache.images_dir().join("shared.jpg");
        fs::write(&shared_file, b"shared-bytes").unwrap();
        let req_a = ArtworkRequest::new("spotify", Some("track_a".into()), url1);
        let req_b = ArtworkRequest::new("spotify", Some("track_b".into()), url1);
        let shared_rec = CacheRecord {
            source_url: url1.to_string(),
            local_file: "images/shared.jpg".to_string(),
            hosted_url: "https://files.catbox.moe/shared.jpg".to_string(),
            host: HostingProvider::Catbox,
            content_hash: "hash_shared".to_string(),
            uploaded_at: now + 20,
            expires_at: None,
            last_accessed: now + 20,
        };
        cache.insert_and_save(&req_a, shared_rec.clone()).unwrap();
        cache.insert_and_save(&req_b, shared_rec).unwrap();

        let req_a_new = ArtworkRequest::new("spotify", Some("track_a".into()), url2);
        cache
            .insert_and_save(
                &req_a_new,
                CacheRecord {
                    source_url: url2.to_string(),
                    local_file: "images/new.jpg".to_string(),
                    hosted_url: "https://files.catbox.moe/new.jpg".to_string(),
                    host: HostingProvider::Catbox,
                    content_hash: "hash_new".to_string(),
                    uploaded_at: now + 30,
                    expires_at: None,
                    last_accessed: now + 30,
                },
            )
            .unwrap();

        assert!(
            cache.entries().contains_key(&old_url_key),
            "shared URL key still used by track_b must stay"
        );
        assert!(shared_file.exists());

        let _ = fs::remove_dir_all(cache.base_dir());
    }
}
