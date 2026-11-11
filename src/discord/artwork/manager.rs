use super::cache::{ArtworkCache, current_unix_timestamp, sha256_hex};
use super::downloader::ArtworkDownloader;
use super::types::{
    ArtworkError, ArtworkInitComponent, ArtworkRequest, ArtworkResult, CacheRecord, HostConfig,
    HostingProvider,
};
use super::uploader::{ArtworkUploader, CatboxUploader, ImgdbUploader, LitterboxUploader};
use crate::config::DiscordArtworkConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const DEFAULT_FAILURE_COOLDOWN: Duration = Duration::from_secs(60);
pub const DEFAULT_CATBOX_COOLDOWN: Duration = Duration::from_secs(60);
pub const DEFAULT_PROVIDER_COOLDOWN: Duration = Duration::from_secs(60);
const MAX_RETRY_AFTER_COOLDOWN: Duration = Duration::from_secs(3600);

fn provider_display_title(provider: HostingProvider) -> &'static str {
    match provider {
        HostingProvider::Imgdb => "Imgdb",
        HostingProvider::Catbox => "Catbox",
        HostingProvider::Litterbox => "Litterbox",
    }
}

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
    imgdb: Option<Box<dyn ArtworkUploader>>,
    catbox: Box<dyn ArtworkUploader>,
    litterbox: Box<dyn ArtworkUploader>,
    config: DiscordArtworkConfig,
    failure_cooldowns: HashMap<String, Instant>,
    failure_cooldown_duration: Duration,
    provider_cooldowns: HashMap<HostingProvider, Instant>,
    catbox_cooldown_duration: Duration,
}

impl ArtworkManager {
    /// Creates a new artwork manager with default settings.
    pub fn new(custom_cache_dir: Option<PathBuf>) -> Result<Self, ArtworkError> {
        Self::with_config(custom_cache_dir, DiscordArtworkConfig::default())
    }

    /// Creates a new artwork manager with custom configuration, preserving component context on errors.
    pub fn with_config(
        custom_cache_dir: Option<PathBuf>,
        config: DiscordArtworkConfig,
    ) -> Result<Self, ArtworkError> {
        let max_cache_bytes = config.cache_max_bytes();
        let cache = ArtworkCache::try_load_with_capacity(custom_cache_dir, max_cache_bytes)
            .map_err(|e| ArtworkError::InitializationFailed {
                component: ArtworkInitComponent::Cache,
                message: e.to_string(),
            })?;
        let downloader =
            ArtworkDownloader::new().map_err(|e| ArtworkError::InitializationFailed {
                component: ArtworkInitComponent::Downloader,
                message: e.to_string(),
            })?;
        let imgdb: Option<Box<dyn ArtworkUploader>> = match ImgdbUploader::new() {
            Ok(uploader) => Some(Box::new(uploader)),
            Err(e) => {
                crate::discord::log_discord(&format!("[Artwork] {e}"));
                None
            }
        };
        let catbox = CatboxUploader::new().map_err(|e| ArtworkError::InitializationFailed {
            component: ArtworkInitComponent::CatboxUploader,
            message: e.to_string(),
        })?;
        let litterbox = LitterboxUploader::with_expiry(config.litterbox_expiry).map_err(|e| {
            ArtworkError::InitializationFailed {
                component: ArtworkInitComponent::LitterboxUploader,
                message: e.to_string(),
            }
        })?;
        Ok(Self::from_parts(
            cache,
            downloader,
            imgdb,
            Box::new(catbox),
            Box::new(litterbox),
            config,
        ))
    }

    pub(crate) fn from_parts(
        cache: ArtworkCache,
        downloader: ArtworkDownloader,
        imgdb: Option<Box<dyn ArtworkUploader>>,
        catbox: Box<dyn ArtworkUploader>,
        litterbox: Box<dyn ArtworkUploader>,
        config: DiscordArtworkConfig,
    ) -> Self {
        Self {
            cache,
            downloader,
            imgdb,
            catbox,
            litterbox,
            config,
            failure_cooldowns: HashMap::new(),
            failure_cooldown_duration: DEFAULT_FAILURE_COOLDOWN,
            provider_cooldowns: HashMap::new(),
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

    fn default_cooldown_for_provider(&self, provider: HostingProvider) -> Duration {
        match provider {
            HostingProvider::Imgdb | HostingProvider::Litterbox => DEFAULT_PROVIDER_COOLDOWN,
            HostingProvider::Catbox => self.catbox_cooldown_duration,
        }
    }

    /// Check if a hosting provider is cooling down after a failure or rate limit.
    #[must_use]
    pub fn is_provider_cooling_down(&self, provider: HostingProvider) -> bool {
        self.provider_cooldowns
            .get(&provider)
            .is_some_and(|until| Instant::now() < *until)
    }

    fn record_provider_failure(
        &mut self,
        provider: HostingProvider,
        retry_after_secs: Option<u64>,
    ) {
        // Note: `retry_after_secs` only captures integer delta-seconds `Retry-After` headers
        // HTTP-date `Retry-After` values produce `None` and intentionally use the default cooldown.
        let duration = match retry_after_secs {
            Some(0) => return,
            Some(secs) => Duration::from_secs(secs).min(MAX_RETRY_AFTER_COOLDOWN),
            None => self.default_cooldown_for_provider(provider),
        };
        if !duration.is_zero() {
            self.provider_cooldowns
                .insert(provider, Instant::now() + duration);
        }
    }

    fn clear_provider_failure(&mut self, provider: HostingProvider) {
        self.provider_cooldowns.remove(&provider);
    }

    /// Check if Catbox uploads are cooling down after a failure.
    #[must_use]
    pub fn is_catbox_cooling_down(&self) -> bool {
        self.is_provider_cooling_down(HostingProvider::Catbox)
    }

    #[cfg(test)]
    fn record_catbox_failure(&mut self) {
        self.record_provider_failure(HostingProvider::Catbox, None);
    }

    #[cfg(test)]
    fn clear_catbox_failure(&mut self) {
        self.clear_provider_failure(HostingProvider::Catbox);
    }

    fn record_failure(&mut self, identity: &str) {
        self.failure_cooldowns
            .insert(identity.to_string(), Instant::now());
    }

    fn clear_failure(&mut self, identity: &str) {
        self.failure_cooldowns.remove(identity);
    }

    /// Resolves artwork executing the full workflow:
    /// 1. Check cache.
    /// 2. Download original artwork.
    /// 3. Normalize image.
    /// 4. Calculate content hash.
    /// 5. Check for duplicate content (content-hash index).
    /// 6. Upload to Imgdb (primary, 24h TTL).
    /// 7. Fall back to Catbox.
    /// 8. Fall back to Litterbox.
    /// 9. Persist cache entry.
    /// 10. Return hosted artwork.
    pub fn resolve(&mut self, request: ArtworkRequest) -> Result<ArtworkResult, ArtworkError> {
        let identity = artwork_identity(&request);
        let now = current_unix_timestamp();

        // Check cache
        if let Some(hit) = self.cache.lookup(&request, now) {
            crate::discord::log_discord(&format!(
                "[Artwork] Reusing cached artwork ({})",
                hit.hosted_url
            ));
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

    fn resolve_provider_chain(&self) -> Result<Vec<HostingProvider>, ArtworkError> {
        let primary = HostConfig::parse(&self.config.primary_host)
            .map_err(|err| {
                ArtworkError::UploadFailed(format!("Invalid primary host configuration ({err})"))
            })?
            .as_provider()
            .ok_or_else(|| {
                ArtworkError::UploadFailed("Primary artwork host is disabled".to_string())
            })?;

        let fallback_cfg = match HostConfig::parse(&self.config.fallback_host) {
            Ok(cfg) => Some(cfg),
            Err(fb_err) => {
                crate::discord::log_discord(&format!(
                    "[Artwork] Invalid fallback host configuration '{}' ({fb_err}); using default fallback behavior",
                    self.config.fallback_host.trim()
                ));
                None
            }
        };

        let mut chain = Vec::with_capacity(3);
        match primary {
            HostingProvider::Imgdb => {
                chain.push(HostingProvider::Imgdb);
                // Preserve Catbox -> Litterbox fallback chain unless fallbacks are explicitly disabled.
                // Do not let a legacy/malformed `fallback_host` skip the default Imgdb fallback chain.
                if !matches!(fallback_cfg, Some(HostConfig::Disabled)) {
                    chain.push(HostingProvider::Catbox);
                    chain.push(HostingProvider::Litterbox);
                }
            }
            HostingProvider::Catbox => {
                chain.push(HostingProvider::Catbox);
                if !matches!(fallback_cfg, Some(HostConfig::Disabled)) {
                    let fallback = match fallback_cfg {
                        Some(HostConfig::Provider(p)) if p != HostingProvider::Catbox => p,
                        _ => HostingProvider::Litterbox,
                    };
                    chain.push(fallback);
                }
            }
            HostingProvider::Litterbox => {
                chain.push(HostingProvider::Litterbox);
                if let Some(HostConfig::Provider(fallback)) = fallback_cfg
                    && fallback != HostingProvider::Litterbox
                {
                    chain.push(fallback);
                }
            }
        }
        Ok(chain)
    }

    fn upload_to_host(
        &mut self,
        host: HostingProvider,
        bytes: &[u8],
    ) -> Result<(String, HostingProvider, Option<u64>), ArtworkError> {
        if self.is_provider_cooling_down(host) {
            return Err(ArtworkError::CooldownActive);
        }

        let uploader: &dyn ArtworkUploader = match host {
            HostingProvider::Imgdb => {
                self.imgdb
                    .as_deref()
                    .ok_or_else(|| ArtworkError::InitializationFailed {
                        component: ArtworkInitComponent::ImgdbUploader,
                        message: "Imgdb uploader unavailable".to_string(),
                    })?
            }
            HostingProvider::Catbox => self.catbox.as_ref(),
            HostingProvider::Litterbox => self.litterbox.as_ref(),
        };

        let result = uploader.upload(bytes, "art.jpg");
        match &result {
            Ok(_) => self.clear_provider_failure(host),
            Err(err) => {
                let retry_after = match err {
                    ArtworkError::RateLimited {
                        retry_after_secs, ..
                    } => *retry_after_secs,
                    _ => None,
                };
                self.record_provider_failure(host, retry_after);
            }
        }

        let uploaded = result?;
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
        let (hosted_url, host, expires_at) = if let Some(existing) = self
            .cache
            .find_reusable_upload(&downloaded.content_hash, now)
        {
            crate::discord::log_discord(&format!(
                "[Artwork] Reusing cached artwork (shared content hash via {})",
                existing.host
            ));
            (
                existing.hosted_url.clone(),
                existing.host,
                existing.expires_at,
            )
        } else {
            let chain = self.resolve_provider_chain()?;
            let mut provider_errors = Vec::with_capacity(chain.len());
            let mut upload_success = None;

            for (idx, provider) in chain.iter().copied().enumerate() {
                if idx > 0 {
                    crate::discord::log_discord(&format!(
                        "[Artwork] Falling back to {}",
                        provider_display_title(provider)
                    ));
                }
                if provider == HostingProvider::Imgdb && !self.is_provider_cooling_down(provider) {
                    crate::discord::log_discord("[Artwork] Imgdb upload started");
                }

                match self.upload_to_host(provider, &downloaded.bytes) {
                    Ok(res) => {
                        crate::discord::log_discord(&format!(
                            "[Artwork] {} upload succeeded ({})",
                            provider_display_title(provider),
                            res.0
                        ));
                        upload_success = Some(res);
                        break;
                    }
                    Err(err) => {
                        crate::discord::log_discord(&format!(
                            "[Artwork] {} upload failed: {err}",
                            provider_display_title(provider)
                        ));
                        provider_errors.push((provider, err));
                    }
                }
            }

            let Some(res) = upload_success else {
                let combined = provider_errors
                    .iter()
                    .map(|(provider, err)| format!("{provider}: {err}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                crate::discord::log_discord(&format!("[Artwork] All providers failed: {combined}"));
                return Err(ArtworkError::UploadFailed(format!(
                    "All artwork hosting providers failed ({combined})"
                )));
            };
            res
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
            crate::discord::log_discord(&format!("[Artwork] Failed to persist cache entry: {err}"));
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
    pub fn refresh(&mut self, request: ArtworkRequest) -> Result<ArtworkResult, ArtworkError> {
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
    /// 1. Imgdb (24h TTL) hosted URL if successful.
    /// 2. Catbox hosted URL if Imgdb failed.
    /// 3. Litterbox (configurable TTL) hosted URL if Catbox failed.
    /// 4. Configured ANSImuz default asset ("logo") if all uploads fail or artwork is unavailable.
    #[must_use]
    pub fn resolve_asset_key(&mut self, request: Option<&ArtworkRequest>) -> String {
        let Some(req) = request else {
            return crate::discord::activity::DEFAULT_LARGE_IMAGE.to_string();
        };

        match self.resolve(req.clone()) {
            Ok(result) => result.hosted_url,
            Err(err) => {
                crate::discord::log_discord(&format!(
                    "[Artwork] Failed to resolve artwork asset key (using default): {err}"
                ));
                crate::discord::activity::DEFAULT_LARGE_IMAGE.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_fallback_to_default_asset_on_none_or_error() {
        let mut manager = ArtworkManager::new(None).expect("artwork manager init");
        assert_eq!(
            manager.resolve_asset_key(None),
            crate::discord::activity::DEFAULT_LARGE_IMAGE
        );
    }

    #[test]
    fn manager_cooldown_and_identity_generation() {
        let req = ArtworkRequest::new(
            "ytmusic",
            Some("vid123".into()),
            "https://example.com/art.jpg",
        );
        let id = artwork_identity(&req);
        assert_eq!(id, "ytmusic:vid123");

        let req_no_id = ArtworkRequest::new("ytmusic", None, "https://example.com/art.jpg");
        let id_url = artwork_identity(&req_no_id);
        assert!(id_url.starts_with("url:"));

        let mut manager = ArtworkManager::new(None).expect("artwork manager init");
        assert!(!manager.is_cooling_down(&id));
        manager.record_failure(&id);
        assert!(manager.is_cooling_down(&id));
        manager.clear_failure(&id);
        assert!(!manager.is_cooling_down(&id));
    }

    #[test]
    fn manager_catbox_cooldown_tracking() {
        let mut manager = ArtworkManager::new(None).expect("artwork manager init");
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

    #[test]
    fn manager_host_config_parsing() {
        assert_eq!(
            HostConfig::parse(" ImgDb "),
            Ok(HostConfig::Provider(HostingProvider::Imgdb))
        );
        assert_eq!(
            HostConfig::parse(" CatBox "),
            Ok(HostConfig::Provider(HostingProvider::Catbox))
        );
        assert_eq!(
            HostConfig::parse("litterbox"),
            Ok(HostConfig::Provider(HostingProvider::Litterbox))
        );
        assert_eq!(HostConfig::parse("none"), Ok(HostConfig::Disabled));
        assert_eq!(HostConfig::parse(""), Ok(HostConfig::Disabled));
        assert!(HostConfig::parse("imgur").is_err());
    }

    #[test]
    fn manager_with_config_preserves_cache_initialization_error_context() {
        let blocking_file = std::env::temp_dir().join("ansimuz_test_mgr_invalid_cache_file");
        std::fs::write(&blocking_file, b"not-a-directory").unwrap();

        let res = ArtworkManager::with_config(
            Some(blocking_file.clone()),
            DiscordArtworkConfig::default(),
        );
        let _ = std::fs::remove_file(&blocking_file);

        let err = match res {
            Ok(_) => panic!("expected cache initialization failure when base path is a file"),
            Err(e) => e,
        };
        assert!(matches!(
            err,
            ArtworkError::InitializationFailed {
                component: ArtworkInitComponent::Cache,
                ..
            }
        ));
    }
}
