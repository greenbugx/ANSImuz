use super::cache::{ArtworkCache, current_unix_timestamp, sha256_hex};
use super::downloader::ArtworkDownloader;
use super::types::{ArtworkError, ArtworkRequest, ArtworkResult, CacheRecord, HostingProvider};
use super::uploader::{ArtworkUploader, CatboxUploader, LitterboxUploader};
use crate::config::DiscordArtworkConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const DEFAULT_FAILURE_COOLDOWN: Duration = Duration::from_secs(60);
pub const DEFAULT_CATBOX_COOLDOWN: Duration = Duration::from_secs(60);

/// Derives a stable artwork identity key from an ArtworkRequest.
#[must_use]
pub fn artwork_identity(req: &ArtworkRequest) -> String {
    if let Some(ref id) = req.track_id {
        format!("{}:{}", req.provider, id)
    } else {
        format!("url:{}", sha256_hex(req.source_url.trim().as_bytes()))
    }
}

/// The main entry point coordinating caching, downloading, and hosting uploads.
pub struct ArtworkManager {
    cache: ArtworkCache,
    downloader: ArtworkDownloader,
    catbox: CatboxUploader,
    litterbox: LitterboxUploader,
    config: DiscordArtworkConfig,
    failure_cooldowns: HashMap<String, Instant>,
    failure_cooldown_duration: Duration,
    catbox_failure: Option<Instant>,
    catbox_cooldown_duration: Duration,
}

impl ArtworkManager {
    /// Creates a new artwork manager with default settings.
    #[must_use]
    pub fn new(custom_cache_dir: Option<PathBuf>) -> Self {
        Self::with_config(custom_cache_dir, DiscordArtworkConfig::default())
    }

    /// Creates a new artwork manager with custom configuration.
    #[must_use]
    pub fn with_config(
        custom_cache_dir: Option<PathBuf>,
        config: DiscordArtworkConfig,
    ) -> Self {
        let max_cache_bytes = config.cache_max_bytes();
        let cache = ArtworkCache::load_with_capacity(custom_cache_dir, max_cache_bytes);
        let litterbox = LitterboxUploader::with_expiry(&config.litterbox_expiry)
            .unwrap_or_default();
        Self {
            cache,
            downloader: ArtworkDownloader::default(),
            catbox: CatboxUploader::default(),
            litterbox,
            config,
            failure_cooldowns: HashMap::new(),
            failure_cooldown_duration: DEFAULT_FAILURE_COOLDOWN,
            catbox_failure: None,
            catbox_cooldown_duration: DEFAULT_CATBOX_COOLDOWN,
        }
    }

    /// Access configuration.
    #[must_use]
    pub fn config(&self) -> &DiscordArtworkConfig {
        &self.config
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

        // Execute download, normalization, and upload workflow
        let outcome = self.execute_resolve(&request, now);

        match &outcome {
            Ok(_) => self.clear_failure(&identity),
            Err(_) => self.record_failure(&identity),
        }

        outcome
    }

    fn upload_to_host(
        &mut self,
        host: &str,
        bytes: &[u8],
    ) -> Result<(String, HostingProvider, Option<u64>), ArtworkError> {
        let uploaded = match host {
            "catbox" => {
                if self.is_catbox_cooling_down() {
                    return Err(ArtworkError::CooldownActive);
                }
                let result = ArtworkUploader::upload(&self.catbox, bytes, "art.jpg");
                match &result {
                    Ok(_) => self.clear_catbox_failure(),
                    Err(_) => self.record_catbox_failure(),
                }
                result?
            }
            "litterbox" => ArtworkUploader::upload(&self.litterbox, bytes, "art.jpg")?,
            other => return Err(ArtworkError::UploadFailed(format!("Unsupported host: {other}"))),
        };
        Ok((uploaded.url, uploaded.provider, uploaded.expires_at))
    }

    fn execute_resolve(
        &mut self,
        request: &ArtworkRequest,
        now: u64,
    ) -> Result<ArtworkResult, ArtworkError> {
        // Download original artwork, normalize image, and calculate content hash with config limits
        let max_download_bytes = self.config.max_download_bytes();
        let target_side = self.config.image_size;
        let downloaded = self
            .downloader
            .download_and_normalize_with_limits(
                &request.source_url,
                self.cache.images_dir(),
                max_download_bytes,
                target_side,
            )
            .map_err(ArtworkError::DownloadFailed)?;

        // Check for duplicate content using in-memory content hash index
        // Tracks sharing the same cover don't cause duplicate uploads!
        let (hosted_url, host, expires_at) = if let Some(existing) =
            self.cache.find_reusable_upload(&downloaded.content_hash, now)
        {
            (existing.hosted_url.clone(), existing.host, existing.expires_at)
        } else {
            let primary = self.config.primary_host.trim().to_lowercase();
            let fallback = self.config.fallback_host.trim().to_lowercase();

            let upload_res = match self.upload_to_host(&primary, &downloaded.bytes) {
                Ok(res) => Ok(res),
                Err(primary_err) => {
                    if !fallback.is_empty() && fallback != primary && fallback != "none" {
                        crate::discord::log_discord(&format!(
                            "Warning: {primary} upload failed ({primary_err}), falling back to {fallback}"
                        ));
                        self.upload_to_host(&fallback, &downloaded.bytes)
                            .map_err(|fb_err| {
                                ArtworkError::UploadFailed(format!(
                                    "Both {primary} ({primary_err}) and {fallback} ({fb_err}) uploads failed"
                                ))
                            })
                    } else {
                        Err(primary_err)
                    }
                }
            };
            upload_res?
        };

        // Persist cache entry atomically without failing artwork publication if index save errors
        let record = CacheRecord {
            source_url: request.source_url.clone(),
            local_file: downloaded.relative_path,
            hosted_url: hosted_url.clone(),
            host,
            content_hash: downloaded.content_hash.clone(),
            uploaded_at: now,
            expires_at,
            last_accessed: now,
        };
        if let Err(err) = self.cache.insert_and_save(request, record) {
            crate::discord::log_discord(&format!(
                "[Artwork] Failed to persist cache entry: {err}"
            ));
        }

        // Return hosted artwork
        Ok(ArtworkResult {
            source_url: request.source_url.clone(),
            hosted_url,
            local_path: downloaded.local_path,
            content_hash: downloaded.content_hash,
        })
    }

    /// Invalidates cached entries matching `request` so future lookups re-resolve remotely.
    pub fn invalidate(&mut self, request: &ArtworkRequest) -> bool {
        let identity = artwork_identity(request);
        self.clear_failure(&identity);
        self.cache.invalidate(request)
    }

    /// Invalidates all cached entries pointing to a specific remote hosted URL.
    pub fn invalidate_hosted_url(&mut self, hosted_url: &str) -> usize {
        self.cache.invalidate_hosted_url(hosted_url)
    }

    /// Invalidates any existing cache mapping (including its stale remote URL) and re-resolves freshly.
    pub fn refresh(
        &mut self,
        request: ArtworkRequest,
    ) -> Result<ArtworkResult, ArtworkError> {
        let now = current_unix_timestamp();
        let identity = artwork_identity(&request);
        self.clear_failure(&identity);

        if let Some(hit) = self.cache.lookup(&request, now) {
            self.cache.invalidate_hosted_url(&hit.hosted_url);
        }
        self.cache.invalidate(&request);

        self.resolve(request)
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

    #[test]
    fn manager_catbox_cooldown_tracking() {
        let mut manager = ArtworkManager::new(None);
        assert!(!manager.is_catbox_cooling_down());
        manager.record_catbox_failure();
        assert!(manager.is_catbox_cooling_down());
        manager.clear_catbox_failure();
        assert!(!manager.is_catbox_cooling_down());

        manager.catbox_cooldown_duration = Duration::from_millis(10);
        manager.record_catbox_failure();
        assert!(manager.is_catbox_cooling_down());
        std::thread::sleep(Duration::from_millis(25));
        assert!(!manager.is_catbox_cooling_down());
    }
}
