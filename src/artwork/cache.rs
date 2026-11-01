use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::{Duration, Instant},
};

use image::{DynamicImage, imageops::FilterType};

use super::decode::{MAX_IMAGE_BYTES, decode_artwork};

const RETRY_COOLDOWN: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub enum CacheEntry {
    Loading,
    Ready(Arc<DynamicImage>),
    Failed { error: String, failed_at: Instant },
}

type ArtCache = Mutex<HashMap<(String, u32), CacheEntry>>;

pub fn art_cache() -> &'static ArtCache {
    static CACHE: LazyLock<ArtCache> = LazyLock::new(|| Mutex::new(HashMap::new()));
    &CACHE
}

pub fn cache_dir() -> PathBuf {
    crate::config::artwork_cache_dir()
}

pub fn clear_memory_cache() {
    let mut guard = match art_cache().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.clear();
}

pub fn cache_path(url: &str) -> PathBuf {
    let hash = url.bytes().fold(2166136261u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16777619)
    });

    cache_dir().join(format!("{hash:08x}.img"))
}

pub fn load_image(url: &str) -> Result<DynamicImage, String> {
    if url.trim().is_empty() {
        return Err("No artwork URL".into());
    }

    let final_path = cache_path(url);

    if final_path.exists() {
        if let Ok(bytes) = fs::read(&final_path)
            && let Ok(image) = decode_artwork(&bytes)
        {
            return Ok(image);
        }
        let _ = fs::remove_file(&final_path);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("ANSIMUZ/0.1")
        .build()
        .map_err(|e| e.to_string())?;

    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("Artwork download failed: {e}"))?;

    if !response.status().is_success()
        && let Some(fallback_url) = thumbnail_fallback(url)
    {
        response = client
            .get(&fallback_url)
            .send()
            .map_err(|e| format!("Artwork download failed: {e}"))?;
    }

    let response = response
        .error_for_status()
        .map_err(|e| format!("Artwork HTTP error: {e}"))?;

    if response
        .content_length()
        .is_some_and(|size| size > MAX_IMAGE_BYTES as u64)
    {
        return Err("Artwork image is too large".into());
    }

    // Enforce download limits while streaming chunks, not after downloading everything
    let initial_cap = response
        .content_length()
        .map(|len| (len as usize).min(MAX_IMAGE_BYTES))
        .unwrap_or(0);
    let mut bytes = Vec::with_capacity(initial_cap);
    let mut buffer = [0u8; 8192];
    let mut reader = response;

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("Cannot read artwork: {e}"))?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > MAX_IMAGE_BYTES {
            return Err("Artwork image is too large".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
    }

    let image = decode_artwork(&bytes)?;

    // Centralized atomic cache write
    let file_stem = final_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("art");
    let _ = crate::config::write_cache_atomic(&cache_dir(), file_stem, "img", &bytes);
    Ok(image)
}

fn thumbnail_fallback(url: &str) -> Option<String> {
    (url.contains("img.youtube.com") && url.contains("maxresdefault"))
        .then(|| url.replace("maxresdefault", "hqdefault"))
}

pub fn request_art(url: &str, side: u32) -> Result<Arc<DynamicImage>, String> {
    if url.trim().is_empty() {
        return Err("No artwork URL".into());
    }

    let key = (url.to_string(), side);
    let mut guard = match art_cache().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(entry) = guard.get(&key) {
        match entry {
            CacheEntry::Ready(img) => return Ok(Arc::clone(img)),
            CacheEntry::Loading => return Err("loading".into()),
            CacheEntry::Failed { error, failed_at } => {
                if failed_at.elapsed() < RETRY_COOLDOWN {
                    return Err(error.clone());
                }
                // Cooldown elapsed: proceed to retry
            }
        }
    }

    // Atomically insert Loading so duplicate requests don't start multiple downloads
    guard.insert(key.clone(), CacheEntry::Loading);
    drop(guard);

    let owned_url = url.to_string();

    thread::spawn(move || {
        let result = load_image(&owned_url).map(|image| {
            if side == 0 {
                Arc::new(image)
            } else {
                Arc::new(image.resize_exact(side, side, FilterType::Triangle))
            }
        });

        let mut guard = match art_cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match result {
            Ok(arc_image) => {
                guard.insert(key, CacheEntry::Ready(arc_image));
            }
            Err(error) => {
                // Do not cache permanently; record failed_at so retries are allowed after cooldown
                guard.insert(
                    key,
                    CacheEntry::Failed {
                        error,
                        failed_at: Instant::now(),
                    },
                );
            }
        }
    });

    Err("loading".into())
}

pub fn prefetch(url: &str) {
    if url.trim().is_empty() {
        return;
    }

    let _ = request_art(url, 64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arc_cached_image_shares_allocation() {
        let dummy = DynamicImage::new_rgb8(200, 200);
        let arc = Arc::new(dummy);
        let entry = CacheEntry::Ready(Arc::clone(&arc));

        assert_eq!(Arc::strong_count(&arc), 2);

        if let CacheEntry::Ready(img) = entry {
            assert_eq!(Arc::strong_count(&img), 2);
            let cloned = Arc::clone(&img);
            assert_eq!(Arc::strong_count(&cloned), 3);
        } else {
            panic!("Expected Ready entry");
        }
    }

    #[test]
    fn test_failed_cache_entry_retries_after_cooldown() {
        let entry_fresh = CacheEntry::Failed {
            error: "Network timeout".into(),
            failed_at: Instant::now(),
        };
        if let CacheEntry::Failed { failed_at, .. } = entry_fresh {
            assert!(failed_at.elapsed() < RETRY_COOLDOWN);
        }

        // Simulated expired cooldown
        let entry_expired = CacheEntry::Failed {
            error: "Old failure".into(),
            failed_at: Instant::now() - Duration::from_secs(10),
        };
        if let CacheEntry::Failed { failed_at, .. } = entry_expired {
            assert!(failed_at.elapsed() >= RETRY_COOLDOWN);
        }
    }
}
