use super::cache::current_unix_timestamp;
use super::types::{
    ArtworkError, ArtworkInitComponent, HostingProvider, NormalizedImageFormat, UploadedArtwork,
};
use crate::config::LitterboxExpiry;
use reqwest::StatusCode;
use reqwest::blocking::multipart::{Form, Part};
use serde::Deserialize;
use std::io::Read;
use std::net::{Ipv4Addr, Ipv6Addr};
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

fn parse_ascii_u64(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<u64>().ok()
}

fn is_valid_byte_zero_content_range(raw: &str) -> bool {
    let Some((unit, spec)) = raw.trim().split_once(char::is_whitespace) else {
        return false;
    };
    if !unit.eq_ignore_ascii_case("bytes") {
        return false;
    }
    let Some((range_part, total_part)) = spec.trim().split_once('/') else {
        return false;
    };
    let Some((start_str, end_str)) = range_part.trim().split_once('-') else {
        return false;
    };
    let (Some(start), Some(end)) = (parse_ascii_u64(start_str), parse_ascii_u64(end_str)) else {
        return false;
    };
    if start != 0 || start > end {
        return false;
    }
    let total_trimmed = total_part.trim();
    if total_trimmed == "*" {
        return true;
    }
    let Some(total) = parse_ascii_u64(total_trimmed) else {
        return false;
    };
    total > 0 && end < total
}

/// Parses an HTTP `Retry-After` header as integer delta-seconds.
///
/// Limitation: This crate has no date/time dependency, so only integer delta-seconds values
/// are supported. HTTP-date `Retry-After` values (as well as non-numeric or negative values)
/// return `None` and fall back to the artwork manager's default provider cooldown.
fn parse_retry_after_secs(header: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    header
        .and_then(|v| v.to_str().ok())
        .and_then(parse_ascii_u64)
}

fn format_retry_after_suffix(
    header: Option<&reqwest::header::HeaderValue>,
) -> (Option<u64>, String) {
    let retry_after_secs = parse_retry_after_secs(header);
    let suffix = if let Some(secs) = retry_after_secs {
        format!(" (Retry-After: {secs}s)")
    } else if let Some(raw) = header
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let sanitized: String = raw
            .chars()
            .take(64)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        format!(" (unparsed Retry-After: '{sanitized}'; using default cooldown)")
    } else {
        String::new()
    };
    (retry_after_secs, suffix)
}

fn classify_send_error(provider_label: &str, err: reqwest::Error) -> ArtworkError {
    if err.is_timeout() {
        ArtworkError::UploadFailed(format!("{provider_label} request timed out: {err}"))
    } else {
        ArtworkError::UploadFailed(format!("{provider_label} request failed: {err}"))
    }
}

fn classify_http_failure(
    provider: HostingProvider,
    provider_label: &str,
    response: reqwest::blocking::Response,
) -> ArtworkError {
    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let (retry_after_secs, suffix) =
            format_retry_after_suffix(response.headers().get(reqwest::header::RETRY_AFTER));
        return ArtworkError::RateLimited {
            provider,
            retry_after_secs,
            message: format!("{provider_label} returned HTTP 429 Too Many Requests{suffix}"),
        };
    }

    let snippet = read_bounded_error_snippet(response);
    let detail = if snippet.is_empty() {
        String::new()
    } else {
        format!(": {snippet}")
    };

    if matches!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE | StatusCode::UNSUPPORTED_MEDIA_TYPE
    ) {
        return ArtworkError::UnsupportedInput(format!(
            "{provider_label} rejected artwork input with HTTP {status}{detail}"
        ));
    }

    if status == StatusCode::REQUEST_TIMEOUT || status.is_server_error() {
        return ArtworkError::UploadFailed(format!(
            "{provider_label} returned HTTP {status}{detail}"
        ));
    }

    ArtworkError::InvalidResponse(format!("{provider_label} returned HTTP {status}{detail}"))
}

fn detect_upload_format(
    provider_label: &str,
    image_bytes: &[u8],
) -> Result<NormalizedImageFormat, ArtworkError> {
    NormalizedImageFormat::from_bytes(image_bytes).map_err(|err| match err {
        ArtworkError::UnsupportedInput(msg) => {
            ArtworkError::UnsupportedInput(format!("{provider_label}: {msg}"))
        }
        other => other,
    })
}

#[derive(Clone, Copy)]
struct MultipartImage<'a> {
    bytes: &'a [u8],
    format: NormalizedImageFormat,
    filename: &'a str,
}

fn build_multipart_image_part(
    provider_label: &str,
    image: MultipartImage<'_>,
) -> Result<Part, ArtworkError> {
    if image.bytes.is_empty() {
        return Err(ArtworkError::UnsupportedInput(format!(
            "{provider_label}: cannot upload empty image"
        )));
    }

    let effective_filename = image.format.multipart_filename(image.filename);
    Part::bytes(image.bytes.to_vec())
        .file_name(effective_filename)
        .mime_str(image.format.mime_type())
        .map_err(|e| {
            ArtworkError::UnsupportedInput(format!("{provider_label}: invalid MIME type: {e}"))
        })
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

fn validate_public_host(authority: &str, label: &str) -> Result<(), String> {
    if authority.is_empty() || authority.contains('@') {
        return Err(format!("{label} has an invalid authority"));
    }

    let host = if let Some(after_open) = authority.strip_prefix('[') {
        let Some((ipv6_str, remainder)) = after_open.split_once(']') else {
            return Err(format!("{label} has a malformed IPv6 host"));
        };
        if !remainder.is_empty() {
            let Some(port_str) = remainder.strip_prefix(':') else {
                return Err(format!("{label} has a malformed host port"));
            };
            let port: u16 = port_str
                .parse()
                .map_err(|_| format!("{label} has an invalid port"))?;
            if port == 0 {
                return Err(format!("{label} cannot use port 0"));
            }
        }
        let ipv6: Ipv6Addr = ipv6_str
            .parse()
            .map_err(|_| format!("{label} has an invalid IPv6 host"))?;
        if is_disallowed_ipv6(ipv6) {
            return Err(format!("{label} points to a non-public IPv6 destination"));
        }
        return Ok(());
    } else if let Some((h, port_str)) = authority.split_once(':') {
        if h.is_empty() || port_str.contains(':') {
            return Err(format!("{label} has a malformed host or port"));
        }
        let port: u16 = port_str
            .parse()
            .map_err(|_| format!("{label} has an invalid port"))?;
        if port == 0 {
            return Err(format!("{label} cannot use port 0"));
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
        return Err(format!("{label} has a local/private host: {host}"));
    }

    if let Ok(ipv4) = lower.parse::<Ipv4Addr>() {
        if is_disallowed_ipv4(ipv4) {
            return Err(format!("{label} points to a private/non-public IP: {host}"));
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
        return Err(format!("{label} has a malformed host: {host}"));
    }

    let tld = labels.last().copied().unwrap_or("");
    if tld.len() < 2 || !tld.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(format!("{label} has an invalid TLD: {host}"));
    }

    Ok(())
}

fn parse_bounded_url_no_credentials(raw_url: &str, label: &str) -> Result<reqwest::Url, String> {
    let url = raw_url.trim();
    if url.is_empty() {
        return Err(format!("{label} is empty"));
    }
    if url.len() > 2048 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("{label} has malformed URL characters: {url}"));
    }

    let parsed =
        reqwest::Url::parse(url).map_err(|e| format!("{label} is invalid ({e}): {url}"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!("{label} must not embed credentials: {url}"));
    }
    if parsed.path().trim_end_matches('/').is_empty() {
        return Err(format!("{label} is missing a resource path: {url}"));
    }

    Ok(parsed)
}

fn parse_and_validate_https_public_url(raw_url: &str, label: &str) -> Result<reqwest::Url, String> {
    let parsed = parse_bounded_url_no_credentials(raw_url, label)?;
    if parsed.scheme() != "https" {
        return Err(format!("{label} must use HTTPS: {}", raw_url.trim()));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| format!("{label} is missing a host: {}", raw_url.trim()))?;
    let authority = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    validate_public_host(&authority, label)?;

    Ok(parsed)
}

fn validate_imgdb_upload_url(raw_url: &str) -> Result<reqwest::Url, ArtworkError> {
    let parsed = parse_and_validate_https_public_url(raw_url, "Imgdb upload URL")
        .map_err(ArtworkError::InvalidUrl)?;
    if parsed.fragment().is_some() {
        return Err(ArtworkError::InvalidUrl(format!(
            "Imgdb upload URL must not contain a fragment: {parsed}"
        )));
    }
    Ok(parsed)
}

#[cfg(test)]
#[allow(dead_code)]
fn validate_local_mock_upload_url(raw_url: &str) -> Result<reqwest::Url, ArtworkError> {
    let parsed = parse_bounded_url_no_credentials(raw_url, "Imgdb test upload URL")
        .map_err(ArtworkError::InvalidUrl)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ArtworkError::InvalidUrl(format!(
            "Imgdb test upload URL must use HTTP/HTTPS: {}",
            raw_url.trim()
        )));
    }
    if !matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
        return Err(ArtworkError::InvalidUrl(format!(
            "Imgdb test upload URL must target a local loopback host: {}",
            raw_url.trim()
        )));
    }
    if parsed.port() == Some(0) || parsed.fragment().is_some() {
        return Err(ArtworkError::InvalidUrl(format!(
            "Imgdb test upload URL has invalid port or fragment: {}",
            raw_url.trim()
        )));
    }
    Ok(parsed)
}

fn validate_imgdb_returned_url(raw_url: &str) -> Result<reqwest::Url, ArtworkError> {
    let parsed = parse_and_validate_https_public_url(raw_url, "Imgdb URL")
        .map_err(ArtworkError::InvalidResponse)?;

    let lower_path = parsed.path().trim_end_matches('/').to_ascii_lowercase();
    if lower_path == "/api" || lower_path.starts_with("/api/") {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb returned metadata/API endpoint instead of direct image URL: {raw_url}"
        )));
    }

    let last_segment = lower_path.rsplit('/').next().unwrap_or("");
    if last_segment.is_empty() {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb URL does not point to an image resource: {raw_url}"
        )));
    }
    if let Some((_, ext)) = last_segment.rsplit_once('.')
        && NON_IMAGE_EXTENSIONS.contains(&ext)
    {
        return Err(ArtworkError::InvalidResponse(format!(
            "Imgdb URL points to a non-image page/resource (.{ext}): {raw_url}"
        )));
    }

    Ok(parsed)
}

/// Imgdb image uploader implementing anonymous 24-hour TTL uploads (`https://imgdb.io/api/v1/upload?ttl=86400`).
pub struct ImgdbUploader {
    client: reqwest::blocking::Client,
    upload_url: reqwest::Url,
    verify_returned_url: bool,
}

impl ImgdbUploader {
    /// Builds an Imgdb uploader with a pooled HTTPS-only HTTP/1.1 client, no redirects, and timeouts.
    ///
    /// Upload response fields (`url`, `kind`, `type`, `size`, `expires`) are always validated.
    /// Extra remote URL HEAD/GET verification is disabled by default to avoid extra round trips
    /// and false fallback on transient CDN failures; use [`Self::with_verify_returned_url`] to opt in.
    pub fn new() -> Result<Self, ArtworkError> {
        let upload_url = validate_imgdb_upload_url(IMGDB_API_URL).map_err(|e| {
            ArtworkError::InitializationFailed {
                component: ArtworkInitComponent::ImgdbUploader,
                message: e.to_string(),
            }
        })?;
        let client = super::build_http_client(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(reqwest::redirect::Policy::none()),
        )
        .map_err(|e| ArtworkError::InitializationFailed {
            component: ArtworkInitComponent::ImgdbUploader,
            message: e.to_string(),
        })?;
        Ok(Self {
            client,
            upload_url,
            verify_returned_url: false,
        })
    }

    /// Private test constructor accepting only validated local loopback mock server URLs.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(super) fn with_test_upload_url(upload_url: &str) -> Result<Self, ArtworkError> {
        let upload_url = validate_local_mock_upload_url(upload_url)?;
        let client = super::build_http_client_with_https(
            Duration::from_secs(10),
            Duration::from_secs(30),
            Some(reqwest::redirect::Policy::none()),
            false,
        )
        .map_err(|e| ArtworkError::InitializationFailed {
            component: ArtworkInitComponent::ImgdbUploader,
            message: e.to_string(),
        })?;
        Ok(Self {
            client,
            upload_url,
            verify_returned_url: false,
        })
    }

    /// Enables or disables optional HEAD/GET verification of the returned Imgdb image URL.
    #[must_use]
    pub const fn with_verify_returned_url(mut self, verify: bool) -> Self {
        self.verify_returned_url = verify;
        self
    }

    fn verify_returned_image_url(&self, url: &reqwest::Url) -> Result<(), ArtworkError> {
        let (response, used_get) = match self.client.head(url.clone()).send() {
            Ok(resp)
                if matches!(
                    resp.status(),
                    StatusCode::FORBIDDEN
                        | StatusCode::METHOD_NOT_ALLOWED
                        | StatusCode::NOT_IMPLEMENTED
                ) =>
            {
                // Request only the first byte so the server does not transmit the full image body.
                (
                    self.client
                        .get(url.clone())
                        .header(reqwest::header::RANGE, "bytes=0-0")
                        .send(),
                    true,
                )
            }
            other => (other, false),
        };
        let response = response.map_err(|e| {
            ArtworkError::InvalidResponse(format!("Imgdb returned URL verification failed: {e}"))
        })?;

        let status = response.status();
        let headers = response.headers();

        if used_get {
            match status {
                StatusCode::OK => {}
                StatusCode::PARTIAL_CONTENT => {
                    let content_range = headers
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    if !is_valid_byte_zero_content_range(content_range) {
                        return Err(ArtworkError::InvalidResponse(format!(
                            "Imgdb returned URL GET verification gave HTTP 206 with invalid Content-Range: '{}'",
                            content_range.trim()
                        )));
                    }
                }
                _ => {
                    return Err(ArtworkError::InvalidResponse(format!(
                        "Imgdb returned URL GET verification gave HTTP {status}"
                    )));
                }
            }
        } else if status != StatusCode::OK {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned URL HEAD verification gave HTTP {status}"
            )));
        }

        let content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .trim();
        if !NormalizedImageFormat::is_accepted_mime(content_type) {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned URL served unsupported Content-Type: '{content_type}'"
            )));
        }

        if let Some(len_val) = headers.get(reqwest::header::CONTENT_LENGTH) {
            let parsed_len = len_val.to_str().ok().and_then(parse_ascii_u64);
            if parsed_len.is_none_or(|n| n == 0) {
                return Err(ArtworkError::InvalidResponse(
                    "Imgdb returned URL served invalid or zero Content-Length".to_string(),
                ));
            }
        }

        // Only headers are required
        // drop the streaming response without consuming the body.
        Ok(())
    }

    /// Uploads image bytes anonymously to Imgdb with `ttl=86400`, detecting format from the payload.
    pub fn upload(
        &self,
        image_bytes: &[u8],
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        let format = detect_upload_format("Imgdb", image_bytes)?;
        self.upload_with_format(image_bytes, format, filename)
    }

    /// Uploads normalized image bytes of known format anonymously to Imgdb with `ttl=86400`.
    pub fn upload_with_format(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        let part = build_multipart_image_part(
            "Imgdb",
            MultipartImage {
                bytes: image_bytes,
                format,
                filename,
            },
        )?;
        let form = Form::new().part("file", part);

        let response = self
            .client
            .post(self.upload_url.clone())
            .multipart(form)
            .send()
            .map_err(|e| classify_send_error("Imgdb", e))?;

        if !response.status().is_success() {
            return Err(classify_http_failure(
                HostingProvider::Imgdb,
                "Imgdb",
                response,
            ));
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

        let validated_url = validate_imgdb_returned_url(&url)?;

        if let Some(kind) = parsed.kind.as_deref()
            && !kind.trim().eq_ignore_ascii_case("image")
        {
            return Err(ArtworkError::InvalidResponse(format!(
                "Imgdb returned unsupported upload kind: '{}'",
                kind.trim()
            )));
        }

        if let Some(ct) = parsed.content_type.as_deref()
            && !NormalizedImageFormat::is_accepted_mime(ct)
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
            self.verify_returned_image_url(&validated_url)?;
        }

        Ok(UploadedArtwork {
            url,
            provider: HostingProvider::Imgdb,
            expires_at: Some(expires_at),
        })
    }
}

impl ArtworkUploader for ImgdbUploader {
    fn upload(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        self.upload_with_format(image_bytes, format, filename)
    }
}

/// Common interface for artwork hosting services.
pub trait ArtworkUploader: Send + Sync {
    fn upload(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError>;
}

fn upload_catbox_family(
    client: &reqwest::blocking::Client,
    provider: HostingProvider,
    retention_time: Option<&str>,
    image: MultipartImage<'_>,
) -> Result<String, ArtworkError> {
    let (endpoint, provider_label) = match provider {
        HostingProvider::Catbox => (CATBOX_API_URL, "Catbox"),
        HostingProvider::Litterbox => (LITTERBOX_API_URL, "Litterbox"),
        HostingProvider::Imgdb => {
            return Err(ArtworkError::UnsupportedInput(
                "Invalid catbox-family provider: imgdb".to_string(),
            ));
        }
    };
    let part = build_multipart_image_part(provider_label, image)?;

    let mut form = Form::new().text("reqtype", "fileupload");
    if let Some(time_str) = retention_time {
        form = form.text("time", time_str.to_string());
    }
    form = form.part("fileToUpload", part);

    let response = client
        .post(endpoint)
        .multipart(form)
        .send()
        .map_err(|e| classify_send_error(provider_label, e))?;

    if !response.status().is_success() {
        return Err(classify_http_failure(provider, provider_label, response));
    }

    let body = response
        .text()
        .map_err(|e| {
            ArtworkError::InvalidResponse(format!("{provider_label} response read failed: {e}"))
        })?
        .trim()
        .to_string();
    if !body.starts_with("https://") {
        return Err(ArtworkError::InvalidResponse(format!(
            "{provider_label} returned invalid URL: {body}"
        )));
    }

    Ok(body)
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

    fn upload_with_format(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        let url = upload_catbox_family(
            &self.client,
            HostingProvider::Catbox,
            None,
            MultipartImage {
                bytes: image_bytes,
                format,
                filename,
            },
        )?;
        Ok(UploadedArtwork {
            url,
            provider: HostingProvider::Catbox,
            expires_at: None,
        })
    }

    /// Uploads image bytes to Catbox, detecting format from the payload and returning the HTTPS URL.
    pub fn upload(
        &self,
        image_bytes: &[u8],
        filename: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let format = detect_upload_format("Catbox", image_bytes)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        self.upload_with_format(image_bytes, format, filename)
            .map(|uploaded| uploaded.url)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl ArtworkUploader for CatboxUploader {
    fn upload(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        self.upload_with_format(image_bytes, format, filename)
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

    fn upload_with_format(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        let url = upload_catbox_family(
            &self.client,
            HostingProvider::Litterbox,
            Some(self.time_str()),
            MultipartImage {
                bytes: image_bytes,
                format,
                filename,
            },
        )?;
        let now = current_unix_timestamp();
        Ok(UploadedArtwork {
            url,
            provider: HostingProvider::Litterbox,
            expires_at: Some(now + self.duration_secs()),
        })
    }

    /// Uploads image bytes to Litterbox with configured retention, detecting format from the payload.
    pub fn upload(
        &self,
        image_bytes: &[u8],
        filename: &str,
    ) -> Result<UploadedArtwork, Box<dyn std::error::Error>> {
        let format = detect_upload_format("Litterbox", image_bytes)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        self.upload_with_format(image_bytes, format, filename)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl ArtworkUploader for LitterboxUploader {
    fn upload(
        &self,
        image_bytes: &[u8],
        format: NormalizedImageFormat,
        filename: &str,
    ) -> Result<UploadedArtwork, ArtworkError> {
        self.upload_with_format(image_bytes, format, filename)
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

    #[test]
    fn test_retry_after_header_parsing() {
        let make_hdr = |s: &str| reqwest::header::HeaderValue::from_str(s).ok();

        assert_eq!(parse_retry_after_secs(make_hdr("120").as_ref()), Some(120));
        assert_eq!(parse_retry_after_secs(make_hdr("0").as_ref()), Some(0));
        assert_eq!(
            parse_retry_after_secs(make_hdr("86400").as_ref()),
            Some(86400)
        );
        assert_eq!(parse_retry_after_secs(make_hdr("invalid").as_ref()), None);
        assert_eq!(parse_retry_after_secs(make_hdr("-5").as_ref()), None);
        assert_eq!(
            parse_retry_after_secs(make_hdr("Wed, 21 Oct 2026 07:28:00 GMT").as_ref()),
            None
        );
        assert_eq!(parse_retry_after_secs(None), None);
    }

    #[test]
    fn test_imgdb_url_validation_and_rejected_destinations() {
        // Valid HTTPS public image URL
        assert!(validate_imgdb_returned_url("https://i.imgdb.io/images/art123.jpg").is_ok());

        // Disallowed / private / non-HTTPS destinations
        assert!(validate_imgdb_returned_url("http://i.imgdb.io/images/art123.jpg").is_err());
        assert!(validate_imgdb_returned_url("https://localhost/art.jpg").is_err());
        assert!(validate_imgdb_returned_url("https://127.0.0.1/art.jpg").is_err());
        assert!(validate_imgdb_returned_url("https://192.168.1.1/art.jpg").is_err());
        assert!(validate_imgdb_returned_url("https://10.0.0.1/art.jpg").is_err());
        assert!(validate_imgdb_returned_url("https://169.254.169.254/art.jpg").is_err());
        assert!(validate_imgdb_returned_url("https://user:pass@imgdb.io/art.jpg").is_err());

        // API or non-image resources
        assert!(validate_imgdb_returned_url("https://imgdb.io/api/v1/upload").is_err());
        assert!(validate_imgdb_returned_url("https://imgdb.io/images/index.html").is_err());
        assert!(validate_imgdb_returned_url("https://imgdb.io/upload.php").is_err());
    }

    #[test]
    fn test_multipart_build_and_empty_image_rejection() {
        // Empty bytes rejected as UnsupportedInput
        let empty_res = build_multipart_image_part(
            "Catbox",
            MultipartImage {
                bytes: b"",
                format: NormalizedImageFormat::Jpeg,
                filename: "cover.jpg",
            },
        );
        assert!(matches!(empty_res, Err(ArtworkError::UnsupportedInput(_))));

        // Valid image part built with correct format and canonical filename extension
        let part_res = build_multipart_image_part(
            "Catbox",
            MultipartImage {
                bytes: b"valid-bytes",
                format: NormalizedImageFormat::Png,
                filename: "original.jpg",
            },
        );
        assert!(part_res.is_ok());
    }
}
