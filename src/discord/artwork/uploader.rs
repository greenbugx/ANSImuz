use super::cache::current_unix_timestamp;
use super::types::{ArtworkError, HostingProvider, UploadedArtwork};
use crate::config::LitterboxExpiry;
use reqwest::StatusCode;
use reqwest::blocking::multipart::{Form, Part};
use serde::Deserialize;
use std::io::Read;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::time::Duration;

pub const IMGDB_API_URL: &str = "https://imgdb.io/api/v1/upload?ttl=86400";
/// Conservative 24-hour expiration estimate used only when Imgdb omits `expires` despite `ttl=86400`.
pub const IMGDB_DEFAULT_TTL_SECS: u64 = 86_400;
pub const CATBOX_API_URL: &str = "https://catbox.moe/user/api.php";
pub const LITTERBOX_API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
pub const LITTERBOX_RETENTION_HOURS: u64 = 72;
pub const LITTERBOX_RETENTION_SECS: u64 = LITTERBOX_RETENTION_HOURS * 3600;

const MAX_IMGDB_JSON_BYTES: usize = 64 * 1024;
const MAX_ERROR_BODY_BYTES: usize = 256;

const ACCEPTED_IMAGE_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/gif",
    "image/avif",
];

const NON_IMAGE_EXTENSIONS: &[&str] = &[
    "html", "htm", "xhtml", "php", "asp", "aspx", "jsp", "cgi", "json", "xml", "txt", "js", "css",
    "svg", "mp3", "mp4", "webm", "wav", "ogg", "flac", "zip", "pdf",
];

#[derive(Debug, Deserialize)]
struct ImgdbUploadResponse {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    id: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default, rename = "type")]
    content_type: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    expires: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
}

fn read_bounded_bytes(mut reader: impl Read, max_bytes: usize) -> Result<Vec<u8>, std::io::Error> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let remaining = max_bytes.saturating_sub(out.len());
        if remaining == 0 {
            // Check whether extra bytes exist beyond ceiling
            let extra = reader.read(&mut buf[..1])?;
            if extra > 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Response body exceeded {max_bytes} byte limit"),
                ));
            }
            break;
        }
        let chunk_limit = remaining.min(buf.len());
        let n = reader.read(&mut buf[..chunk_limit])?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

fn read_bounded_error_snippet(mut reader: impl Read) -> String {
    let mut buf = vec![0u8; MAX_ERROR_BODY_BYTES];
    let n = reader.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    let snippet: String = String::from_utf8_lossy(&buf)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    snippet.trim().to_string()
}

fn is_accepted_image_mime(mime: &str) -> bool {
    let essence = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    ACCEPTED_IMAGE_MIME_TYPES.contains(&essence.as_str())
}

fn detect_image_mime_and_filename(
    image_bytes: &[u8],
    filename: &str,
) -> Result<(&'static str, String), ArtworkError> {
    let detected = match image::guess_format(image_bytes) {
        Ok(image::ImageFormat::Jpeg) => Some(("image/jpeg", "jpg")),
        Ok(image::ImageFormat::Png) => Some(("image/png", "png")),
        Ok(image::ImageFormat::WebP) => Some(("image/webp", "webp")),
        Ok(image::ImageFormat::Gif) => Some(("image/gif", "gif")),
        Ok(image::ImageFormat::Avif) => Some(("image/avif", "avif")),
        Ok(other) => {
            return Err(ArtworkError::UploadFailed(format!(
                "Imgdb: unsupported normalized image format ({other:?})"
            )));
        }
        Err(_) => match Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("jpg" | "jpeg") => Some(("image/jpeg", "jpg")),
            Some("png") => Some(("image/png", "png")),
            Some("webp") => Some(("image/webp", "webp")),
            Some("gif") => Some(("image/gif", "gif")),
            Some("avif") => Some(("image/avif", "avif")),
            _ => None,
        },
    };

    let (mime, ext) = detected.ok_or_else(|| {
        ArtworkError::UploadFailed("Imgdb: unrecognized image format".to_string())
    })?;

    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("artwork");
    Ok((mime, format!("{stem}.{ext}")))
}

fn is_disallowed_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    let is_cgnat = o[0] == 100 && (o[1] & 0b1100_0000) == 64;
    let is_benchmark = o[0] == 198 && (o[1] & 0xfe) == 18;
    let is_reserved_e = o[0] >= 240;
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0
        || is_cgnat
        || is_benchmark
        || is_reserved_e
}

fn is_disallowed_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_disallowed_ipv4(v4);
    }
    let s = ip.segments();
    let is_unique_local = (s[0] & 0xfe00) == 0xfc00;
    let is_unicast_link_local = (s[0] & 0xffc0) == 0xfe80;
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_unique_local
        || is_unicast_link_local
}

fn validate_public_host(authority: &str) -> Result<(), ArtworkError> {
    if authority.is_empty() || authority.contains('@') {
        return Err(ArtworkError::InvalidResponse(
            "Imgdb returned URL with invalid authority".to_string(),
        ));
    }

    let host = if let Some(after_open) = authority.strip_prefix('[') {
        let Some((ipv6_str, remainder)) = after_open.split_once(']') else {
            return Err(ArtworkError::InvalidResponse(
                "Imgdb returned URL with malformed IPv6 host".to_string(),
            ));
        };
        if !remainder.is_empty() {
            let Some(port_str) = remainder.strip_prefix(':') else {
                return Err(ArtworkError::InvalidResponse(
                    "Imgdb returned URL with malformed host port".to_string(),
                ));
            };
            let port: u16 = port_str.parse().map_err(|_| {
                ArtworkError::InvalidResponse("Imgdb returned URL with invalid port".to_string())
            })?;
            if port == 0 {
                return Err(ArtworkError::InvalidResponse(
                    "Imgdb returned URL with port 0".to_string(),
                ));
            }
        }
        let ipv6: Ipv6Addr = ipv6_str.parse().map_err(|_| {
            ArtworkError::InvalidResponse("Imgdb returned URL with invalid IPv6 host".to_string())
        })?;
        if is_disallowed_ipv6(ipv6) {
            return Err(ArtworkError::InvalidResponse(
                "Imgdb returned URL pointing to non-public IPv6 destination".to_string(),
            ));
        }
        return Ok(());
    } else if let Some((h, port_str)) = authority.split_once(':') {
        if h.is_empty() || port_str.contains(':') {
            return Err(ArtworkError::InvalidResponse(
                "Imgdb returned URL with malformed host or port".to_string(),
            ));
        }
        let port: u16 = port_str.parse().map_err(|_| {
            ArtworkError::InvalidResponse("Imgdb returned URL with invalid port".to_string())
        })?;
        if port == 0 {
            return Err(ArtworkError::InvalidResponse(
                "Imgdb returned URL with port 0".to_string(),
            ));
        }
        h
    } else {
        authority
    };

    let lower = host.to_ascii_lowercase();
    if lower.is_empty()
        || lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower.ends_with(".arpa")
        || lower.ends_with(".lan")
        || lower.ends_with(".home")
    {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb returned URL with local/private host: {host}"
        )));
    }

    if let Ok(ipv4) = lower.parse::<Ipv4Addr>() {
        if is_disallowed_ipv4(ipv4) {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned URL pointing to private/non-public IP: {host}"
            )));
        }
        return Ok(());
    }

    let labels: Vec<&str> = lower.split('.').collect();
    if labels.len() < 2
        || labels.iter().any(|l| {
            l.is_empty()
                || l.len() > 63
                || l.starts_with('-')
                || l.ends_with('-')
                || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb returned URL with malformed host: {host}"
        )));
    }

    let tld = labels.last().copied().unwrap_or("");
    if tld.len() < 2 || !tld.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb returned URL with invalid TLD: {host}"
        )));
    }

    Ok(())
}

fn validate_imgdb_returned_url(raw_url: &str) -> Result<(), ArtworkError> {
    let url = raw_url.trim();
    if url.is_empty() {
        return Err(ArtworkError::InvalidResponse(
            "Imgdb returned an empty URL".to_string(),
        ));
    }
    if url.len() > 2048 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ArtworkError::InvalidResponse(
            "Imgdb returned malformed URL characters".to_string(),
        ));
    }

    let Some(rest) = url.strip_prefix("https://") else {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb URL must use HTTPS: {url}"
        )));
    };

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    validate_public_host(authority)?;

    let tail = &rest[authority_end..];
    let path_only = tail.split(['?', '#']).next().unwrap_or("");
    let trimmed_path = path_only.trim_end_matches('/');
    if trimmed_path.is_empty() {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb URL is missing an image resource path: {url}"
        )));
    }

    let lower_path = trimmed_path.to_ascii_lowercase();
    if lower_path == "/api" || lower_path.starts_with("/api/") {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb returned metadata/API endpoint instead of direct image URL: {url}"
        )));
    }

    let last_segment = lower_path.rsplit('/').next().unwrap_or("");
    if last_segment.is_empty() {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb URL does not point to an image resource: {url}"
        )));
    }
    if let Some((_, ext)) = last_segment.rsplit_once('.')
        && NON_IMAGE_EXTENSIONS.contains(&ext)
    {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb URL points to a non-image page/resource (.{ext}): {url}"
        )));
    }

    Ok(())
}

/// Imgdb image uploader implementing anonymous 24-hour TTL uploads (`https://imgdb.io/api/v1/upload?ttl=86400`).
pub struct ImgdbUploader {
    client: reqwest::blocking::Client,
    upload_url: String,
    verify_returned_url: bool,
}

impl ImgdbUploader {
    /// Builds an Imgdb uploader with a pooled HTTPS-only HTTP/1.1 client, no redirects, and timeouts.
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = super::build_http_client(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(reqwest::redirect::Policy::none()),
        )?;
        Ok(Self {
            client,
            upload_url: IMGDB_API_URL.to_string(),
            verify_returned_url: true,
        })
    }

    fn verify_returned_image_url(&self, url: &str) -> Result<(), ArtworkError> {
        let response = match self.client.head(url).send() {
            Ok(resp)
                if matches!(
                    resp.status(),
                    StatusCode::FORBIDDEN
                        | StatusCode::METHOD_NOT_ALLOWED
                        | StatusCode::NOT_IMPLEMENTED
                ) =>
            {
                self.client.get(url).send()
            }
            other => other,
        }
        .map_err(|e| {
            ArtworkError::InvalidResponse(format!("Imgdb returned URL verification failed: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned URL verification gave HTTP {}",
                response.status()
            )));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .trim();
        if !is_accepted_image_mime(content_type) {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned URL served unsupported Content-Type: '{content_type}'"
            )));
        }

        Ok(())
    }

    /// Uploads normalized image bytes anonymously to Imgdb with `ttl=86400` and validates the response.
    pub fn upload(
        &self,
        image_bytes: &[u8],
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        if image_bytes.is_empty() {
            return Err(ArtworkError::UploadFailed(
                "Imgdb: cannot upload empty image".to_string(),
            ));
        }

        let (mime_type, effective_filename) =
            detect_image_mime_and_filename(image_bytes, filename)?;
        let part = Part::bytes(image_bytes.to_vec())
            .file_name(effective_filename)
            .mime_str(mime_type)
            .map_err(|e| ArtworkError::UploadFailed(format!("Imgdb: invalid MIME type: {e}")))?;

        let form = Form::new().part("file", part);

        let response = self
            .client
            .post(&self.upload_url)
            .multipart(form)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    ArtworkError::UploadFailed(format!("Imgdb request timed out: {e}"))
                } else {
                    ArtworkError::UploadFailed(format!("Imgdb request failed: {e}"))
                }
            })?;

        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after_secs = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok());
            let suffix = retry_after_secs
                .map(|secs| format!(" (Retry-After: {secs}s)"))
                .unwrap_or_default();
            return Err(ArtworkError::RateLimited {
                provider: HostingProvider::Imgdb,
                retry_after_secs,
                message: format!("Imgdb returned HTTP 429 Too Many Requests{suffix}"),
            });
        }

        if !status.is_success() {
            let snippet = read_bounded_error_snippet(response);
            let detail = if snippet.is_empty() {
                String::new()
            } else {
                format!(": {snippet}")
            };
            return Err(ArtworkError::UploadFailed(format!(
                "Imgdb returned HTTP {status}{detail}"
            )));
        }

        let raw_body = read_bounded_bytes(response, MAX_IMGDB_JSON_BYTES).map_err(|e| {
            ArtworkError::InvalidResponse(format!("Imgdb response read failed: {e}"))
        })?;
        let parsed: ImgdbUploadResponse = serde_json::from_slice(&raw_body).map_err(|e| {
            ArtworkError::InvalidResponse(format!("Imgdb returned invalid JSON: {e}"))
        })?;

        let url = parsed
            .url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .ok_or_else(|| {
                ArtworkError::InvalidResponse(
                    "Imgdb response missing non-empty 'url' field".to_string(),
                )
            })?
            .to_string();

        validate_imgdb_returned_url(&url)?;

        if let Some(kind) = parsed.kind.as_deref()
            && !kind.trim().eq_ignore_ascii_case("image")
        {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned unsupported upload kind: '{}'",
                kind.trim()
            )));
        }

        if let Some(ct) = parsed.content_type.as_deref()
            && !is_accepted_image_mime(ct)
        {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned unsupported image MIME type: '{}'",
                ct.trim()
            )));
        }

        if parsed.size == Some(0) {
            return Err(ArtworkError::InvalidResponse(
                "Imgdb reported zero-byte image upload".to_string(),
            ));
        }

        let now = current_unix_timestamp();
        let expires_at = match parsed.expires {
            Some(ts) if ts > now => ts,
            Some(ts) => {
                return Err(ArtworkError::InvalidResponse(format!(
                    "Imgdb returned non-future expiration timestamp ({ts} <= {now})"
                )));
            }
            // Fallback estimate when the server omits `expires` despite `ttl=86400`:
            // conservatively estimate 24 hours from successful upload completion.
            None => now.saturating_add(IMGDB_DEFAULT_TTL_SECS),
        };

        if self.verify_returned_url {
            self.verify_returned_image_url(&url)?;
        }

        Ok(UploadedArtwork {
            url,
            provider: HostingProvider::Imgdb,
            expires_at: Some(expires_at),
        })
    }
}

impl ArtworkUploader for ImgdbUploader {
    fn upload(&self, image_bytes: &[u8], filename: &str) -> Result<UploadedArtwork, ArtworkError> {
        self.upload(image_bytes, filename)
    }
}

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
