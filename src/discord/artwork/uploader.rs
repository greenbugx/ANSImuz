use super::cache::current_unix_timestamp;
use super::types::{ArtworkError, HostingProvider, UploadedArtwork};
use reqwest::blocking::multipart::{Form, Part};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const CATBOX_API_URL: &str = "https://catbox.moe/user/api.php";
pub const LITTERBOX_API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
pub const LITTERBOX_RETENTION_HOURS: u64 = 72;
pub const LITTERBOX_RETENTION_SECS: u64 = LITTERBOX_RETENTION_HOURS * 3600;
pub const DEFAULT_RETRY_COOLDOWN: Duration = Duration::from_secs(60);

/// Common interface for artwork hosting services.
pub trait ArtworkUploader: Send + Sync {
    fn upload(
        &self,
        image_bytes: Vec<u8>,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError>;
}

/// Catbox image uploader implementing anonymous file uploads.
pub struct CatboxUploader {
    client: reqwest::blocking::Client,
}

impl CatboxUploader {
    /// Builds Catbox uploader client with connection and request timeouts.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client = reqwest::blocking::Client::builder()
            .http1_only()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }

    /// Uploads normalized image bytes to Catbox, returning the hosted HTTPS URL.
    pub fn upload(
        &self,
        image_bytes: Vec<u8>,
        filename: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Build multipart part with filename and MIME type
        let part = Part::bytes(image_bytes)
            .file_name(filename.to_string())
            .mime_str("image/jpeg")?;

        // Build multipart form (anonymous upload: omit userhash)
        let form = Form::new()
            .text("reqtype", "fileupload")
            .part("fileToUpload", part);

        // Send POST request
        let response = self
            .client
            .post(CATBOX_API_URL)
            .multipart(form)
            .send()?;

        // Check HTTP status code
        if !response.status().is_success() {
            return Err(format!("Catbox returned HTTP status {}", response.status()).into());
        }

        // Trim response body and validate that it's a valid HTTPS URL
        let body = response.text()?.trim().to_string();
        if !body.starts_with("https://") {
            return Err(format!("Catbox returned invalid URL: {body}").into());
        }

        Ok(body)
    }
}

impl Default for CatboxUploader {
    fn default() -> Self {
        Self::new().expect("failed to initialize Catbox HTTP client")
    }
}

impl ArtworkUploader for CatboxUploader {
    fn upload(
        &self,
        image_bytes: Vec<u8>,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        let url = self
            .upload(image_bytes, filename)
            .map_err(|e| ArtworkError::UploadFailed(format!("Catbox: {e}")))?;

        Ok(UploadedArtwork {
            url,
            provider: HostingProvider::Catbox,
            expires_at: None,
        })
    }
}

/// Litterbox image uploader for temporary hosting fallback (72h retention).
pub struct LitterboxUploader {
    client: reqwest::blocking::Client,
    time: &'static str,
    duration_secs: u64,
}

impl LitterboxUploader {
    /// Builds Litterbox uploader client configured for 72h retention fallback.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client = reqwest::blocking::Client::builder()
            .http1_only()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            time: "72h",
            duration_secs: LITTERBOX_RETENTION_SECS,
        })
    }

    /// Uploads normalized image bytes to Litterbox with 72h retention.
    pub fn upload(
        &self,
        image_bytes: Vec<u8>,
        filename: &str,
    ) -> Result<UploadedArtwork, Box<dyn std::error::Error>> {
        let part = Part::bytes(image_bytes)
            .file_name(filename.to_string())
            .mime_str("image/jpeg")?;

        let form = Form::new()
            .text("reqtype", "fileupload")
            .text("time", self.time)
            .part("fileToUpload", part);

        let response = self
            .client
            .post(LITTERBOX_API_URL)
            .multipart(form)
            .send()?;

        if !response.status().is_success() {
            return Err(format!("Litterbox returned HTTP status {}", response.status()).into());
        }

        let body = response.text()?.trim().to_string();
        if !body.starts_with("https://") {
            return Err(format!("Litterbox returned invalid URL: {body}").into());
        }

        let now = current_unix_timestamp();
        Ok(UploadedArtwork {
            url: body,
            provider: HostingProvider::Litterbox,
            expires_at: Some(now + self.duration_secs),
        })
    }
}

impl Default for LitterboxUploader {
    fn default() -> Self {
        Self::new().expect("failed to initialize Litterbox HTTP client")
    }
}

impl ArtworkUploader for LitterboxUploader {
    fn upload(
        &self,
        image_bytes: Vec<u8>,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        self.upload(image_bytes, filename)
            .map_err(|e| ArtworkError::UploadFailed(format!("Litterbox: {e}")))
    }
}

/// Fallback uploader executing the policy: Catbox -> Litterbox (72h) with retry cooldown.
pub struct FallbackUploader {
    catbox: CatboxUploader,
    litterbox: LitterboxUploader,
    catbox_failure: Mutex<Option<Instant>>,
    retry_cooldown: Duration,
}

impl FallbackUploader {
    pub fn new() -> Self {
        Self {
            catbox: CatboxUploader::default(),
            litterbox: LitterboxUploader::default(),
            catbox_failure: Mutex::new(None),
            retry_cooldown: DEFAULT_RETRY_COOLDOWN,
        }
    }

    pub fn with_cooldown(retry_cooldown: Duration) -> Self {
        Self {
            catbox: CatboxUploader::default(),
            litterbox: LitterboxUploader::default(),
            catbox_failure: Mutex::new(None),
            retry_cooldown,
        }
    }

    /// Check whether Catbox is currently cooling down from a previous failure.
    pub fn is_catbox_cooling_down(&self) -> bool {
        if let Ok(guard) = self.catbox_failure.lock()
            && let Some(last_fail) = *guard
        {
            last_fail.elapsed() < self.retry_cooldown
        } else {
            false
        }
    }

    fn record_catbox_failure(&self) {
        if let Ok(mut guard) = self.catbox_failure.lock() {
            *guard = Some(Instant::now());
        }
    }

    fn clear_catbox_failure(&self) {
        if let Ok(mut guard) = self.catbox_failure.lock() {
            *guard = None;
        }
    }
}

impl Default for FallbackUploader {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtworkUploader for FallbackUploader {
    fn upload(
        &self,
        image_bytes: Vec<u8>,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        // 1. Try Catbox if not cooling down
        if !self.is_catbox_cooling_down() {
            match self.catbox.upload(image_bytes.clone(), filename) {
                Ok(url) => {
                    self.clear_catbox_failure();
                    return Ok(UploadedArtwork {
                        url,
                        provider: HostingProvider::Catbox,
                        expires_at: None,
                    });
                }
                Err(e) => {
                    self.record_catbox_failure();
                    crate::discord::log_discord(&format!(
                        "Warning: Catbox upload failed ({e}), falling back to Litterbox (72h)"
                    ));
                }
            }
        }

        // 2. Fallback to Litterbox (72h)
        self.litterbox
            .upload(image_bytes, filename)
            .map_err(|e| ArtworkError::UploadFailed(format!("Litterbox: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catbox_uploader_init_succeeds() {
        assert!(CatboxUploader::new().is_ok());
    }

    #[test]
    fn litterbox_uploader_init_succeeds() {
        assert!(LitterboxUploader::new().is_ok());
    }

    #[test]
    #[ignore]
    fn test_live_catbox_upload() {
        let uploader = CatboxUploader::new().unwrap();
        let img = image::DynamicImage::new_rgb8(10, 10);
        let mut jpeg = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        let res = uploader.upload(jpeg, "test.jpg");
        println!("Result: {:?}", res);
        assert!(res.is_ok());
    }

    #[test]
    fn fallback_uploader_cooldown_tracking() {
        let uploader = FallbackUploader::with_cooldown(Duration::from_millis(50));
        assert!(!uploader.is_catbox_cooling_down());

        uploader.record_catbox_failure();
        assert!(uploader.is_catbox_cooling_down());

        std::thread::sleep(Duration::from_millis(60));
        assert!(!uploader.is_catbox_cooling_down());

        uploader.record_catbox_failure();
        uploader.clear_catbox_failure();
        assert!(!uploader.is_catbox_cooling_down());
    }
}