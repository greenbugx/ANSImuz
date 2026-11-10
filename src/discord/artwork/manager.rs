use super::cache::{ArtworkCache, current_unix_timestamp};
use super::downloader::download_and_normalize;
use super::types::{ArtworkRequest, ArtworkResult, CacheRecord};
use super::uploader::upload_artwork;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

/// Coordinates caching, deduplicating, downloading, and uploading album artwork for Discord RPC.
#[derive(Clone)]
pub struct ArtworkManager {
    cache: Arc<Mutex<ArtworkCache>>,
    in_flight: Arc<(Mutex<HashSet<String>>, Condvar)>,
}

impl ArtworkManager {
    /// Creates a new artwork manager with default or custom cache directory.
    #[must_use]
    pub fn new(custom_cache_dir: Option<PathBuf>) -> Self {
        let cache = ArtworkCache::load(custom_cache_dir);
        Self {
            cache: Arc::new(Mutex::new(cache)),
            in_flight: Arc::new((Mutex::new(HashSet::new()), Condvar::new())),
        }
    }

    /// Access inner cache mutex.
    #[must_use]
    pub fn cache(&self) -> &Arc<Mutex<ArtworkCache>> {
        &self.cache
    }

    /// Resolves artwork for the given request:
    ///
    /// 1. Checks cache for existing valid entry.
    /// 2. If in-flight for the same source URL, waits for completion and re-checks cache.
    /// 3. Downloads and normalizes image locally.
    /// 4. Checks if another track with identical content hash already has a valid upload.
    /// 5. Uploads to hosting provider (Litterbox/Catbox) if not already hosted.
    /// 6. Updates persistent cache and returns `ArtworkResult`.
    pub fn resolve(&self, req: ArtworkRequest) -> Result<ArtworkResult, String> {
        let now = current_unix_timestamp();

        // 1. Check existing persistent cache
        if let Ok(cache) = self.cache.lock()
            && let Some(hit) = cache.lookup(&req, now)
        {
            return Ok(hit);
        }

        // 2. In-flight request deduplication
        let (lock, cvar) = &*self.in_flight;
        let mut in_flight_guard = lock.lock().unwrap();

        while in_flight_guard.contains(&req.source_url) {
            in_flight_guard = cvar.wait(in_flight_guard).unwrap();

            // After waking up, re-check cache to see if the in-flight request resolved it
            if let Ok(cache) = self.cache.lock()
                && let Some(hit) = cache.lookup(&req, current_unix_timestamp())
            {
                return Ok(hit);
            }
        }

        // Mark as in-flight
        in_flight_guard.insert(req.source_url.clone());
        drop(in_flight_guard);

        // Ensure we unregister from in_flight even on panic or error
        let result = self.execute_resolution(&req);

        let mut in_flight_guard = lock.lock().unwrap();
        in_flight_guard.remove(&req.source_url);
        cvar.notify_all();

        result
    }

    fn execute_resolution(&self, req: &ArtworkRequest) -> Result<ArtworkResult, String> {
        let images_dir = {
            let cache = self.cache.lock().unwrap();
            cache.images_dir().to_path_buf()
        };

        // 3. Download and normalize
        let downloaded = download_and_normalize(&req.source_url, &images_dir)?;
        let now = current_unix_timestamp();

        // 4. Content hash reuse check:
        // Different tracks sharing identical artwork can reuse one hosted image!
        let (hosted_url, host, expires_at) = {
            let cache = self.cache.lock().unwrap();
            if let Some(existing) = cache.find_reusable_upload(&downloaded.content_hash, now) {
                (existing.hosted_url.clone(), existing.host, existing.expires_at)
            } else {
                drop(cache);
                // 5. Upload to hosting provider
                let upload = upload_artwork(&downloaded.bytes, "art.jpg")?;
                (upload.hosted_url, upload.host, upload.expires_at)
            }
        };

        // 6. Update cache and save index atomically
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
}

