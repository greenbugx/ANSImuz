use super::types::{ArtworkRequest, ArtworkResult, CacheIndex, CacheRecord, HostingProvider};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
}

impl ArtworkCache {
    /// Initialize or load the artwork cache from the given directory (or default).
    pub fn load(custom_base_dir: Option<PathBuf>) -> Self {
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

        Self {
            base_dir,
            images_dir,
            index_path,
            index,
        }
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
        self.index.entries.values().find(|record| {
            record.content_hash == content_hash
                && is_record_valid(record, now_secs)
                && self.base_dir.join(&record.local_file).is_file()
        })
    }

    /// Insert a record for a request and atomically persist `index.json`.
    pub fn insert_and_save(&mut self, req: &ArtworkRequest, record: CacheRecord) -> std::io::Result<()> {
        if let Some(key) = make_primary_key(&req.provider, req.track_id.as_deref()) {
            self.index.entries.insert(key, record.clone());
        }

        let url_key = make_url_key(&req.source_url);
        self.index.entries.insert(url_key, record);

        self.save_index()
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
    fn catbox_record_validity() {
        let record = CacheRecord {
            source_url: "https://example.com/art.jpg".into(),
            local_file: "images/cat.jpg".into(),
            hosted_url: "https://files.catbox.moe/abc.jpg".into(),
            host: HostingProvider::Catbox,
            content_hash: "chash".into(),
            uploaded_at: 1000,
            expires_at: None,
        };

        assert!(is_record_valid(&record, 999_999));
    }
}

