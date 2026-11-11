use super::cache::sha256_hex;
use super::types::NormalizedImageFormat;
use image::{DynamicImage, GenericImageView, imageops};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024; // 8 MB limit
pub const MAX_DIMENSION: u32 = 4096;
pub const MIN_DIMENSION: u32 = 10;
pub const TARGET_MAX_SIDE: u32 = 1024;
pub const NORMALIZED_IMAGE_FORMAT: NormalizedImageFormat = NormalizedImageFormat::Jpeg;

/// Result of downloading, normalizing, and storing artwork locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedArtwork {
    pub local_path: PathBuf,
    pub relative_path: String,
    pub content_hash: String,
    pub bytes: Vec<u8>,
    pub format: NormalizedImageFormat,
}

/// Normalizes image according to specifications:
///
/// - Rejects malformed / out-of-bound dimensions
/// - Resizes to fit within 1024x1024 preserving original aspect ratio
/// - Converts to RGB
/// - Encodes as JPEG
/// - Computes SHA-256 hash of normalized bytes
pub fn normalize_image(img: DynamicImage) -> Result<(Vec<u8>, String), String> {
    normalize_image_with_side(img, TARGET_MAX_SIDE)
}

/// Normalizes image with a configurable target side constraint.
pub fn normalize_image_with_side(
    img: DynamicImage,
    target_side: u32,
) -> Result<(Vec<u8>, String), String> {
    let (bytes, hash, _format) = normalize_image_with_format(img, target_side)?;
    Ok((bytes, hash))
}

/// Normalizes image with a configurable target side constraint and returns the encoded format.
pub fn normalize_image_with_format(
    img: DynamicImage,
    target_side: u32,
) -> Result<(Vec<u8>, String, NormalizedImageFormat), String> {
    let (w, h) = img.dimensions();
    if w < MIN_DIMENSION || h < MIN_DIMENSION {
        return Err(format!("Image dimensions ({w}x{h}) are too small"));
    }
    if w > MAX_DIMENSION || h > MAX_DIMENSION {
        return Err(format!(
            "Image dimensions ({w}x{h}) exceed maximum allowed ({MAX_DIMENSION})"
        ));
    }

    // Resize to fit within target_side x target_side while preserving aspect ratio (do not stretch)
    let resized = if w > target_side || h > target_side {
        img.resize(target_side, target_side, imageops::FilterType::Triangle)
    } else {
        img
    };

    // Convert to RGB
    let rgb = resized.to_rgb8();

    // Encode to normalized format
    let format = NORMALIZED_IMAGE_FORMAT;
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            format.to_image_format(),
        )
        .map_err(|e| format!("Failed to encode artwork to JPEG: {e}"))?;

    let hash = sha256_hex(&encoded);
    Ok((encoded, hash, format))
}

/// Persistent HTTP downloader with connection pooling for artwork fetching and normalization.
pub struct ArtworkDownloader {
    client: reqwest::blocking::Client,
}

impl ArtworkDownloader {
    /// Initializes an artwork downloader with a pooled HTTP/1.1 client, redirect limit, and timeouts.
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = super::build_http_client(
            Duration::from_secs(8),
            Duration::from_secs(15),
            Some(reqwest::redirect::Policy::limited(5)),
        )?;
        Ok(Self { client })
    }

    /// Download artwork from an HTTPS URL and normalize it to the cache `images/` directory.
    pub fn download_and_normalize(
        &self,
        url: &str,
        images_dir: &Path,
    ) -> Result<DownloadedArtwork, String> {
        self.download_and_normalize_with_limits(url, images_dir, MAX_IMAGE_BYTES, TARGET_MAX_SIDE)
    }

    /// Download artwork with configurable download size ceiling and image size.
    pub fn download_and_normalize_with_limits(
        &self,
        url: &str,
        images_dir: &Path,
        max_download_bytes: usize,
        target_side: u32,
    ) -> Result<DownloadedArtwork, String> {
        // Accept HTTPS artwork URLs only
        if !url.starts_with("https://") {
            return Err("Artwork URL must use the HTTPS scheme".to_string());
        }

        // Send GET request using pooled client
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        // Check HTTP status code
        if !response.status().is_success() {
            return Err(format!(
                "HTTP request failed with status: {}",
                response.status()
            ));
        }

        // Check Content-Type header if present
        if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            let ct = content_type.to_str().unwrap_or_default().to_lowercase();
            if !ct.starts_with("image/")
                && !ct.starts_with("application/octet-stream")
                && !ct.is_empty()
            {
                return Err(format!("Response is not an image (Content-Type: {ct})"));
            }
        }

        // Stream chunks with size limit
        let initial_cap = response
            .content_length()
            .map(|len| (len as usize).min(max_download_bytes))
            .unwrap_or(0);
        let mut raw_bytes = Vec::with_capacity(initial_cap);
        let mut buffer = [0u8; 8192];
        let mut reader = response;

        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| format!("Failed reading response stream: {e}"))?;
            if n == 0 {
                break;
            }
            if raw_bytes.len() + n > max_download_bytes {
                return Err(format!(
                    "Artwork download exceeded maximum allowed size ({max_download_bytes} bytes)"
                ));
            }
            raw_bytes.extend_from_slice(&buffer[..n]);
        }

        // Decode using image crate
        let img_reader = image::ImageReader::new(std::io::Cursor::new(&raw_bytes))
            .with_guessed_format()
            .map_err(|e| format!("Failed to guess artwork format: {e}"))?;

        let decoded = img_reader
            .decode()
            .map_err(|e| format!("Failed to decode artwork image: {e}"))?;

        // Normalize (RGB, resize <= target_side maintaining aspect ratio, encode, SHA-256)
        let (normalized_bytes, content_hash, format) =
            normalize_image_with_format(decoded, target_side)?;

        // Write to unique temporary file, then rename to hash-based filename
        fs::create_dir_all(images_dir)
            .map_err(|e| format!("Failed creating images directory: {e}"))?;

        let file_name = format!("{content_hash}.{}", format.extension());
        let final_path = images_dir.join(&file_name);
        let relative_path = format!("images/{file_name}");

        if !final_path.exists() {
            let temp_filename = format!(
                "{content_hash}.tmp-{}-{}.part",
                std::process::id(),
                crate::discord::artwork::cache::current_unix_timestamp()
            );
            let temp_path = images_dir.join(temp_filename);

            fs::write(&temp_path, &normalized_bytes)
                .map_err(|e| format!("Failed writing temp image file: {e}"))?;

            if let Err(e) = fs::rename(&temp_path, &final_path) {
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    || e.kind() == std::io::ErrorKind::PermissionDenied
                {
                    let _ = fs::remove_file(&final_path);
                    if let Err(retry_err) = fs::rename(&temp_path, &final_path) {
                        let _ = fs::remove_file(&temp_path);
                        return Err(format!("Failed renaming temp image file: {retry_err}"));
                    }
                } else {
                    let _ = fs::remove_file(&temp_path);
                    return Err(format!("Failed renaming temp image file: {e}"));
                }
            }
        }

        Ok(DownloadedArtwork {
            local_path: final_path,
            relative_path,
            content_hash,
            bytes: normalized_bytes,
            format,
        })
    }
}

/// Download artwork from an HTTPS URL and normalize it to the cache `images/` directory.
pub fn download_and_normalize(url: &str, images_dir: &Path) -> Result<DownloadedArtwork, String> {
    download_and_normalize_with_limits(url, images_dir, MAX_IMAGE_BYTES, TARGET_MAX_SIDE)
}

/// Download artwork with configurable download size ceiling and image size.
pub fn download_and_normalize_with_limits(
    url: &str,
    images_dir: &Path,
    max_download_bytes: usize,
    target_side: u32,
) -> Result<DownloadedArtwork, String> {
    ArtworkDownloader::new()
        .map_err(|e| format!("Failed to initialize HTTP client: {e}"))?
        .download_and_normalize_with_limits(url, images_dir, max_download_bytes, target_side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_image_preserves_aspect_ratio_and_resizes() {
        // Create 2000x1000 image
        // 2:1 aspect ratio
        let img = image::RgbaImage::new(2000, 1000);
        let dyn_img = DynamicImage::ImageRgba8(img);

        let (bytes, hash) = normalize_image(dyn_img).expect("normalization should succeed");
        assert!(!bytes.is_empty());
        assert_eq!(hash.len(), 64); // SHA-256 hex string length

        // Decode back and check dimensions
        let decoded = image::load_from_memory(&bytes).expect("should decode back");
        let (w, h) = decoded.dimensions();
        assert!(w <= 1024 && h <= 1024);
        assert_eq!(w, 1024);
        assert_eq!(h, 512); // Exactly 2:1 aspect ratio preserved!
    }

    #[test]
    fn test_normalize_image_with_custom_side() {
        let img = image::RgbaImage::new(2000, 1000);
        let dyn_img = DynamicImage::ImageRgba8(img);

        let (bytes, _hash) = normalize_image_with_side(dyn_img, 512).expect("normalization");
        let decoded = image::load_from_memory(&bytes).expect("decode");
        let (w, h) = decoded.dimensions();
        assert_eq!(w, 512);
        assert_eq!(h, 256);
    }

    #[test]
    fn non_https_url_rejected() {
        let temp_dir = std::env::temp_dir();
        let downloader = ArtworkDownloader::new().expect("downloader init");
        let res = downloader.download_and_normalize("http://example.com/art.jpg", &temp_dir);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("HTTPS"));
    }
}
