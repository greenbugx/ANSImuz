use super::cache::{ArtworkCache, current_unix_timestamp, sha256_hex};
use super::downloader::download_and_normalize;
use super::types::{ArtworkError, ArtworkRequest, ArtworkResult, CacheRecord, HostingProvider};
use super::uploader::{CatboxUploader, LitterboxUploader};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_FAILURE_COOLDOWN: Duration = Duration::from_secs(60);
pub const DEFAULT_CATBOX_COOLDOWN: Duration = Duration::from_secs(60);

type InFlightState = Arc<(Mutex<Option<Result<ArtworkResult, ArtworkError>>>, Condvar)>;

/// Derives a stable artwork identity key from an ArtworkRequest.
#[must_use]
pub fn artwork_identity(req: &ArtworkRequest) -> String {
    if let Some(ref id) = req.track_id {
        format!("{}:{}", req.provider, id)
    } else {
        format!("url:{}", sha256_hex(req.source_url.trim().as_bytes()))
    }
}

/// The main entry point coordinating caching, in-flight deduplication, and hosting uploads.
pub struct ArtworkManager {
    cache: ArtworkCache,
    catbox: CatboxUploader,
    litterbox: LitterboxUploader,
    in_flight: Arc<(Mutex<HashMap<String, InFlightState>>, Condvar)>,
    failure_cooldowns: HashMap<String, Instant>,
    failure_cooldown_duration: Duration,
    catbox_failure: Option<Instant>,
    catbox_cooldown_duration: Duration,
}

impl ArtworkManager {
    /// Creates a new artwork manager with default or custom cache directory.
    #[must_use]
    pub fn new(custom_cache_dir: Option<PathBuf>) -> Self {
        let cache = ArtworkCache::load(custom_cache_dir);
        Self {
            cache,
            catbox: CatboxUploader::default(),
            litterbox: LitterboxUploader::default(),
            in_flight: Arc::new((Mutex::new(HashMap::new()), Condvar::new())),
            failure_cooldowns: HashMap::new(),
            failure_cooldown_duration: DEFAULT_FAILURE_COOLDOWN,
            catbox_failure: None,
            catbox_cooldown_duration: DEFAULT_CATBOX_COOLDOWN,
        }
    }

    /// Access inner cache.
    #[must_use]
    pub fn cache(&self) -> &ArtworkCache {
        &self.cache
    }

    /// Access mutable reference to inner cache.
    pub fn cache_mut(&mut self) -> &mut ArtworkCache {
        &mut self.cache
    }

    /// Check if an artwork identity is cooling down after a recent failure.
    #[must_use]
    pub fn is_cooling_down(&self, identity: &str) -> bool {
        if let Some(last_fail) = self.failure_cooldowns.get(identity) {
            last_fail.elapsed() < self.failure_cooldown_duration
        } else {
            false
        }
    }

    /// Check if Catbox uploads are cooling down after a failure.
    #[must_use]
    pub fn is_catbox_cooling_down(&self) -> bool {
        if let Some(last_fail) = self.catbox_failure {
            last_fail.elapsed() < self.catbox_cooldown_duration
        } else {
            false
        }
    }

    fn record_catbox_failure(&mut self) {
        self.catbox_failure = Some(Instant::now());
    }

    fn clear_catbox_failure(&mut self) {
        self.catbox_failure = None;
    }

    fn record_failure(&mut self, identity: &str) {
        self.failure_cooldowns.insert(identity.to_string(), Instant::now());
    }

    fn clear_failure(&mut self, identity: &str) {
        self.failure_cooldowns.remove(identity);
    }

    /// Resolves artwork executing the full 9-step workflow:
    /// 1. Check cache.
    /// 2. Download original artwork.
    /// 3. Normalize image.
    /// 4. Calculate content hash.
    /// 5. Check for duplicate content (content-hash index).
    /// 6. Upload to Catbox.
    /// 7. Fall back to Litterbox.
    /// 8. Persist cache entry.
    /// 9. Return hosted artwork.
    pub fn resolve(
        &mut self,
        request: ArtworkRequest,
    ) -> Result<ArtworkResult, ArtworkError> {
        let identity = artwork_identity(&request);
        let now = current_unix_timestamp();

        // Check cache
        if let Some(hit) = self.cache.lookup(&request, now) {
            self.clear_failure(&identity);
            return Ok(hit);
        }

        // Retry cooldown: prevent repeatedly hammering network on every position update
        if self.is_cooling_down(&identity) {
            return Err(ArtworkError::CooldownActive);
        }

        // In-flight request deduplication
        let pending = {
            let (map_lock, _) = &*self.in_flight;
            let mut map = map_lock.lock().unwrap();

            if let Some(existing) = map.get(&identity).cloned() {
                drop(map);
                // A request for this artwork identity is already in flight!
                // Wait for the in-flight operation to complete.
                let (res_lock, res_cvar) = &*existing;
                let mut guard = res_lock.lock().unwrap();
                while guard.is_none() {
                    guard = res_cvar.wait(guard).unwrap();
                }
                return guard.clone().unwrap();
            }

            let new_pending = Arc::new((Mutex::new(None), Condvar::new()));
            map.insert(identity.clone(), Arc::clone(&new_pending));
            new_pending
        };

        // Execute download, normalization, and upload workflow
        let outcome = self.execute_resolve(&request, now);

        // Notify all waiters on this artwork identity and clean up in-flight map
        {
            let (res_lock, res_cvar) = &*pending;
            let mut guard = res_lock.lock().unwrap();
            *guard = Some(outcome.clone());
            res_cvar.notify_all();

            let (map_lock, _) = &*self.in_flight;
            let mut map = map_lock.lock().unwrap();
            map.remove(&identity);
        }

        match &outcome {
            Ok(_) => self.clear_failure(&identity),
            Err(_) => self.record_failure(&identity),
        }

        outcome
    }

    fn execute_resolve(
        &mut self,
        request: &ArtworkRequest,
        now: u64,
    ) -> Result<ArtworkResult, ArtworkError> {
        // Download original artwork, normalize image, and calculate content hash
        let downloaded = download_and_normalize(&request.source_url, self.cache.images_dir())
            .map_err(ArtworkError::DownloadFailed)?;

        // Check for duplicate content using in-memory content hash index
        // Tracks sharing the same cover don't cause duplicate uploads!
        let (hosted_url, host, expires_at) = if let Some(existing) =
            self.cache.find_reusable_upload(&downloaded.content_hash, now)
        {
            (existing.hosted_url.clone(), existing.host, existing.expires_at)
        } else {
            // Upload to Catbox with fallback to Litterbox (72h)
            let (url, provider, exp) = if !self.is_catbox_cooling_down() {
                match self.catbox.upload(downloaded.bytes.clone(), "art.jpg") {
                    Ok(url) => {
                        self.clear_catbox_failure();
                        (url, HostingProvider::Catbox, None)
                    }
                    Err(e) => {
                        self.record_catbox_failure();
                        crate::discord::log_discord(&format!(
                            "Warning: Catbox upload failed ({e}), falling back to Litterbox (72h)"
                        ));
                        let up = self
                            .litterbox
                            .upload(downloaded.bytes.clone(), "art.jpg")
                            .map_err(|lit_err| {
                                ArtworkError::UploadFailed(format!(
                                    "Both Catbox ({e}) and Litterbox ({lit_err}) uploads failed"
                                ))
                            })?;
                        (up.url, up.provider, up.expires_at)
                    }
                }
            } else {
                let up = self
                    .litterbox
                    .upload(downloaded.bytes.clone(), "art.jpg")
                    .map_err(|lit_err| {
                        ArtworkError::UploadFailed(format!(
                            "Litterbox upload failed (Catbox in cooldown): {lit_err}"
                        ))
                    })?;
                (up.url, up.provider, up.expires_at)
            };
            (url, provider, exp)
        };

        // Persist cache entry atomically
        let record = CacheRecord {
            source_url: request.source_url.clone(),
            local_file: downloaded.relative_path,
            hosted_url: hosted_url.clone(),
            host,
            content_hash: downloaded.content_hash.clone(),
            uploaded_at: now,
            expires_at,
        };
        let _ = self.cache.insert_and_save(request, record);

        // Return hosted artwork
        Ok(ArtworkResult {
            source_url: request.source_url.clone(),
            hosted_url,
            local_path: downloaded.local_path,
            content_hash: downloaded.content_hash,
        })
    }

    /// Resolves an asset key for Discord Rich Presence according to fallback policy:
    ///
    /// 1. Catbox hosted URL if successful.
    /// 2. Litterbox (72h) hosted URL if Catbox failed.
    /// 3. Configured ANSImuz default asset ("logo") if both uploads fail or artwork is unavailable.
    #[must_use]
    pub fn resolve_asset_key(&mut self, request: Option<&ArtworkRequest>) -> String {
        let Some(req) = request else {
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

    #[test]
    fn manager_fallback_to_default_asset_on_none_or_error() {
        let mut manager = ArtworkManager::new(None);
        assert_eq!(manager.resolve_asset_key(None), crate::discord::activity::DEFAULT_LARGE_IMAGE);
    }

    #[test]
    fn manager_cooldown_and_identity_generation() {
        let req = ArtworkRequest::new("ytmusic", Some("vid123".into()), "https://example.com/art.jpg");
        let id = artwork_identity(&req);
        assert_eq!(id, "ytmusic:vid123");

        let req_no_id = ArtworkRequest::new("ytmusic", None, "https://example.com/art.jpg");
        let id_url = artwork_identity(&req_no_id);
        assert!(id_url.starts_with("url:"));

        let mut manager = ArtworkManager::new(None);
        assert!(!manager.is_cooling_down(&id));
        manager.record_failure(&id);
        assert!(manager.is_cooling_down(&id));
        manager.clear_failure(&id);
        assert!(!manager.is_cooling_down(&id));
    }
}
