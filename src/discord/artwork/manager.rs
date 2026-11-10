use super::cache::{ArtworkCache, current_unix_timestamp};
use super::downloader::download_and_normalize;
use super::types::{ArtworkError, ArtworkRequest, ArtworkResult, CacheRecord};
use super::uploader::{ArtworkUploader, FallbackUploader};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_FAILURE_COOLDOWN: Duration = Duration::from_secs(60);

/// Coordinates caching, deduplicating, downloading, and uploading album artwork for Discord RPC.
#[derive(Clone)]
pub struct ArtworkManager {
    cache: Arc<Mutex<ArtworkCache>>,
    uploader: Arc<dyn ArtworkUploader>,
    in_flight: Arc<(Mutex<HashSet<String>>, Condvar)>,
    failure_cooldowns: Arc<Mutex<HashMap<String, Instant>>>,
    failure_cooldown_duration: Duration,
}

impl ArtworkManager {
    /// Creates a new artwork manager with default FallbackUploader (Catbox -> Litterbox 72h).
    #[must_use]
    pub fn new(custom_cache_dir: Option<PathBuf>) -> Self {
        Self::with_uploader(custom_cache_dir, Arc::new(FallbackUploader::new()))
    }

    /// Creates a new artwork manager with a custom uploader implementation (useful for tests).
    pub fn with_uploader(custom_cache_dir: Option<PathBuf>, uploader: Arc<dyn ArtworkUploader>) -> Self {
        let cache = ArtworkCache::load(custom_cache_dir);
        Self {
            cache: Arc::new(Mutex::new(cache)),
            uploader,
            in_flight: Arc::new((Mutex::new(HashSet::new()), Condvar::new())),
            failure_cooldowns: Arc::new(Mutex::new(HashMap::new())),
            failure_cooldown_duration: DEFAULT_FAILURE_COOLDOWN,
        }
    }

    /// Access inner cache mutex.
    #[must_use]
    pub fn cache(&self) -> &Arc<Mutex<ArtworkCache>> {
        &self.cache
    }

    /// Check if a request's source URL is currently cooling down after a recent failure.
    #[must_use]
    pub fn is_cooling_down(&self, source_url: &str) -> bool {
        if let Ok(guard) = self.failure_cooldowns.lock()
            && let Some(last_fail) = guard.get(source_url)
        {
            last_fail.elapsed() < self.failure_cooldown_duration
        } else {
            false
        }
    }

    fn record_failure(&self, source_url: &str) {
        if let Ok(mut guard) = self.failure_cooldowns.lock() {
            guard.insert(source_url.to_string(), Instant::now());
        }
    }

    fn clear_failure(&self, source_url: &str) {
        if let Ok(mut guard) = self.failure_cooldowns.lock() {
            guard.remove(source_url);
        }
    }

    /// Resolves artwork following the exact policy:
    ///
    /// 1. Checks cache for existing valid entry (validating Litterbox expiry).
    /// 2. If recent failure cooldown is active, aborts early without repeating network calls.
    /// 3. If in-flight for the same source URL, waits for completion and re-checks cache.
    /// 4. Downloads and normalizes image locally.
    /// 5. Checks if another track with identical content hash already has a valid upload.
    /// 6. Uploads via hosting policy: Catbox -> Litterbox (72h).
    /// 7. Updates persistent cache and returns `ArtworkResult`.
    pub fn resolve(&self, req: ArtworkRequest) -> Result<ArtworkResult, ArtworkError> {
        let now = current_unix_timestamp();

        // 1. Check existing persistent cache
        if let Ok(cache) = self.cache.lock()
            && let Some(hit) = cache.lookup(&req, now)
        {
            self.clear_failure(&req.source_url);
            return Ok(hit);
        }

        // 2. Check retry cooldown: don't repeatedly retry failed uploads on every position update
        if self.is_cooling_down(&req.source_url) {
            return Err(ArtworkError::CooldownActive);
        }

        // 3. In-flight request deduplication
        let (lock, cvar) = &*self.in_flight;
        let mut in_flight_guard = lock.lock().unwrap();

        while in_flight_guard.contains(&req.source_url) {
            in_flight_guard = cvar.wait(in_flight_guard).unwrap();

            // After waking up, re-check cache to see if the in-flight request resolved it
            if let Ok(cache) = self.cache.lock()
                && let Some(hit) = cache.lookup(&req, current_unix_timestamp())
            {
                self.clear_failure(&req.source_url);
                return Ok(hit);
            }
        }

        // Mark as in-flight
        in_flight_guard.insert(req.source_url.clone());
        drop(in_flight_guard);

        // Execute resolution workflow
        let result = self.execute_resolution(&req);

        let mut in_flight_guard = lock.lock().unwrap();
        in_flight_guard.remove(&req.source_url);
        cvar.notify_all();

        match &result {
            Ok(_) => self.clear_failure(&req.source_url),
            Err(_) => self.record_failure(&req.source_url),
        }

        result
    }

    fn execute_resolution(&self, req: &ArtworkRequest) -> Result<ArtworkResult, ArtworkError> {
        let images_dir = {
            let cache = self.cache.lock().unwrap();
            cache.images_dir().to_path_buf()
        };

        // 4. Download and normalize
        let downloaded = download_and_normalize(&req.source_url, &images_dir)
            .map_err(ArtworkError::DownloadFailed)?;
        let now = current_unix_timestamp();

        // 5. Content hash reuse check:
        // Different tracks sharing identical artwork can reuse one hosted image!
        let (hosted_url, host, expires_at) = {
            let cache = self.cache.lock().unwrap();
            if let Some(existing) = cache.find_reusable_upload(&downloaded.content_hash, now) {
                (existing.hosted_url.clone(), existing.host, existing.expires_at)
            } else {
                drop(cache);
                // 6. Upload via hosting policy (Catbox -> Litterbox 72h)
                let upload = self.uploader.upload(downloaded.bytes, "art.jpg")?;
                (upload.url, upload.provider, upload.expires_at)
            }
        };

        // 7. Update cache and save index atomically
        let record = CacheRecord {
            source_url: req.source_url.clone(),
            local_file: downloaded.relative_path,
            hosted_url: hosted_url.clone(),
            host,
            content_hash: downloaded.content_hash.clone(),
            uploaded_at: now,
            expires_at,
        };

        let mut cache = self.cache.lock().unwrap();
        let _ = cache.insert_and_save(req, record);

        Ok(ArtworkResult {
            source_url: req.source_url.clone(),
            hosted_url,
            local_path: downloaded.local_path,
            content_hash: downloaded.content_hash,
        })
    }

    /// Resolves an asset key for Discord Rich Presence according to Step 9 fallback policy:
    ///
    /// 1. Catbox hosted URL if successful.
    /// 2. Litterbox (72h) hosted URL if Catbox failed.
    /// 3. Configured ANSImuz default asset ("logo") if both uploads fail or artwork is unavailable.
    #[must_use]
    pub fn resolve_asset_key(&self, req: Option<&ArtworkRequest>) -> String {
        let Some(req) = req else {
            return crate::discord::activity::DEFAULT_LARGE_IMAGE.to_string();
        };

        match self.resolve(req.clone()) {
            Ok(result) => result.hosted_url,
            Err(_) => crate::discord::activity::DEFAULT_LARGE_IMAGE.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discord::artwork::types::HostingProvider;

    struct MockUploader {
        fail_catbox: bool,
    }

    impl ArtworkUploader for MockUploader {
        fn upload(&self, _bytes: Vec<u8>, _filename: &str) -> Result<crate::discord::artwork::types::UploadedArtwork, ArtworkError> {
            if self.fail_catbox {
                Ok(crate::discord::artwork::types::UploadedArtwork {
                    url: "https://litterbox.catbox.moe/fallback.jpg".to_string(),
                    provider: HostingProvider::Litterbox,
                    expires_at: Some(current_unix_timestamp() + 72 * 3600),
                })
            } else {
                Ok(crate::discord::artwork::types::UploadedArtwork {
                    url: "https://files.catbox.moe/success.jpg".to_string(),
                    provider: HostingProvider::Catbox,
                    expires_at: None,
                })
            }
        }
    }

    #[test]
    fn manager_fallback_to_default_asset_on_none_or_error() {
        let manager = ArtworkManager::new(None);
        // None request returns DEFAULT_LARGE_IMAGE ("logo")
        assert_eq!(manager.resolve_asset_key(None), crate::discord::activity::DEFAULT_LARGE_IMAGE);
    }

    #[test]
    fn mock_uploader_integration_and_cooldown() {
        let temp_dir = std::env::temp_dir().join("ansimuz_test_mgr");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let uploader = Arc::new(MockUploader { fail_catbox: false });
        let manager = ArtworkManager::with_uploader(Some(temp_dir.clone()), uploader);

        assert!(!manager.is_cooling_down("https://example.com/art.jpg"));
        manager.record_failure("https://example.com/art.jpg");
        assert!(manager.is_cooling_down("https://example.com/art.jpg"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
