use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{LazyLock, Mutex},
    thread,
};

use image::{DynamicImage, imageops::FilterType};

use super::decode::{MAX_IMAGE_BYTES, decode_artwork};

pub fn cache_dir() -> PathBuf {
    crate::config::artwork_cache_dir()
}

pub fn clear_memory_cache() {
    if let Ok(mut guard) = art_cache().lock() {
        guard.clear();
    }
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

    let path = cache_path(url);

    if path.exists() {
        if let Ok(bytes) = fs::read(&path)
            && let Ok(image) = decode_artwork(&bytes)
        {
            return Ok(image);
        }
        let _ = fs::remove_file(&path);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
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

    let bytes = response
        .bytes()
        .map_err(|e| format!("Cannot read artwork: {e}"))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Artwork image is too large".into());
    }

    let image = decode_artwork(&bytes)?;

    let _ = fs::create_dir_all(cache_dir());
    let _ = fs::write(path, &bytes);

    Ok(image)
}

fn thumbnail_fallback(url: &str) -> Option<String> {
    (url.contains("img.youtube.com") && url.contains("maxresdefault"))
        .then(|| url.replace("maxresdefault", "hqdefault"))
}

type ArtCache = Mutex<HashMap<(String, u32), Result<DynamicImage, String>>>;

pub fn art_cache() -> &'static ArtCache {
    static CACHE: LazyLock<ArtCache> = LazyLock::new(|| Mutex::new(HashMap::new()));
    &CACHE
}

pub fn request_art(url: &str, side: u32) -> Result<DynamicImage, String> {
    let key = (url.to_string(), side);

    if let Ok(guard) = art_cache().lock()
        && let Some(image) = guard.get(&key)
    {
        return image.clone();
    }

    if let Ok(mut guard) = art_cache().lock() {
        guard.insert(key.clone(), Err("loading".into()));
    }

    let owned_url = url.to_string();

    thread::spawn(move || {
        let result = load_image(&owned_url).map(|image| {
            if side == 0 {
                image
            } else {
                image.resize_exact(side, side, FilterType::Triangle)
            }
        });

        if let Ok(mut guard) = art_cache().lock() {
            guard.insert(key, result);
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
