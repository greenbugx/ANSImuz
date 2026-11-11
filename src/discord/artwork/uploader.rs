use super::cache::current_unix_timestamp;
use super::types::{ArtworkError, HostingProvider, UploadedArtwork};
use crate::config::LitterboxExpiry;
use reqwest::blocking::multipart::{Form, Part};
use std::time::Duration;

pub const CATBOX_API_URL: &str = "https://catbox.moe/user/api.php";
pub const LITTERBOX_API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
pub const LITTERBOX_RETENTION_HOURS: u64 = 72;
pub const LITTERBOX_RETENTION_SECS: u64 = LITTERBOX_RETENTION_HOURS * 3600;

/// Common interface for artwork hosting services.
pub trait ArtworkUploader: Send + Sync {
    fn upload(&self, image_bytes: &[u8], filename: &str) -> Result<UploadedArtwork, ArtworkError>;
}

/// Catbox image uploader implementing anonymous file uploads.
pub struct CatboxUploader {
    client: reqwest::blocking::Client,
}

impl CatboxUploader {
    /// Builds Catbox uploader client with connection and request timeouts.
    pub fn new() -> Result<Self, reqwest::Error> {
        let client =
            super::build_http_client(Duration::from_secs(10), Duration::from_secs(30), None)?;
        Ok(Self { client })
    }

    /// Uploads normalized image bytes to Catbox, returning the hosted HTTPS URL.
    pub fn upload(
        &self,
        image_bytes: &[u8],
        filename: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Build multipart part with filename and MIME type
        let part = Part::bytes(image_bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str("image/jpeg")?;

        // Build multipart form (anonymous upload: omit userhash)
        let form = Form::new()
            .text("reqtype", "fileupload")
            .part("fileToUpload", part);

        // Send POST request
        let response = self.client.post(CATBOX_API_URL).multipart(form).send()?;

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

impl ArtworkUploader for CatboxUploader {
    fn upload(&self, image_bytes: &[u8], filename: &str) -> Result<UploadedArtwork, ArtworkError> {
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

/// Litterbox image uploader for temporary hosting (configurable retention up to 72h).
pub struct LitterboxUploader {
    client: reqwest::blocking::Client,
    expiry: LitterboxExpiry,
}

impl LitterboxUploader {
    /// Builds Litterbox uploader client configured for 72h retention fallback.
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_expiry(LitterboxExpiry::default())
    }

    /// Builds Litterbox uploader client with validated retention expiry.
    pub fn with_expiry(expiry: LitterboxExpiry) -> Result<Self, reqwest::Error> {
        let client =
            super::build_http_client(Duration::from_secs(10), Duration::from_secs(30), None)?;

        Ok(Self { client, expiry })
    }

    /// Access configured retention duration in seconds.
    #[must_use]
    pub const fn duration_secs(&self) -> u64 {
        self.expiry.duration_seconds()
    }

    /// Access configured retention time string.
    #[must_use]
    pub const fn time_str(&self) -> &'static str {
        self.expiry.as_str()
    }

    /// Uploads normalized image bytes to Litterbox with configured retention.
    pub fn upload(
        &self,
        image_bytes: &[u8],
        filename: &str,
    ) -> Result<UploadedArtwork, Box<dyn std::error::Error>> {
        let part = Part::bytes(image_bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str("image/jpeg")?;

        let form = Form::new()
            .text("reqtype", "fileupload")
            .text("time", self.time_str().to_string())
            .part("fileToUpload", part);

        let response = self.client.post(LITTERBOX_API_URL).multipart(form).send()?;

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
            expires_at: Some(now + self.duration_secs()),
        })
    }
}

impl ArtworkUploader for LitterboxUploader {
    fn upload(&self, image_bytes: &[u8], filename: &str) -> Result<UploadedArtwork, ArtworkError> {
        self.upload(image_bytes, filename)
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
    fn test_litterbox_uploader_with_expiry() {
        let u1 = LitterboxUploader::with_expiry(LitterboxExpiry::OneHour).unwrap();
        assert_eq!(u1.time_str(), "1h");
        assert_eq!(u1.duration_secs(), 3600);

        let u12 = LitterboxUploader::with_expiry(LitterboxExpiry::TwelveHours).unwrap();
        assert_eq!(u12.time_str(), "12h");
        assert_eq!(u12.duration_secs(), 12 * 3600);

        let u2 = LitterboxUploader::with_expiry(LitterboxExpiry::TwentyFourHours).unwrap();
        assert_eq!(u2.time_str(), "24h");
        assert_eq!(u2.duration_secs(), 24 * 3600);

        let u3 = LitterboxUploader::with_expiry(LitterboxExpiry::SeventyTwoHours).unwrap();
        assert_eq!(u3.time_str(), "72h");
        assert_eq!(u3.duration_secs(), 72 * 3600);
    }

    #[test]
    #[ignore]
    fn test_live_catbox_upload() {
        let uploader = CatboxUploader::new().unwrap();
        let img = image::DynamicImage::new_rgb8(10, 10);
        let mut jpeg = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let res = uploader.upload(&jpeg, "test.jpg");
        println!("Result: {:?}", res);
        assert!(res.is_ok());
    }
}
