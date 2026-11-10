use super::types::HostingProvider;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub const LITTERBOX_API_URL: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
pub const CATBOX_API_URL: &str = "https://catbox.moe/user/api.php";
pub const LITTERBOX_EXPIRY_SECS: u64 = 24 * 3600; // 24 hours

static BOUNDARY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates a unique multipart boundary string without external dependencies.
#[must_use]
pub fn generate_boundary() -> String {
    let count = BOUNDARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = crate::discord::artwork::cache::current_unix_timestamp();
    format!("----ANSImuzBoundary{}_{}_{}", std::process::id(), now, count)
}

/// Result of uploading artwork to a hosting provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadResult {
    pub hosted_url: String,
    pub host: HostingProvider,
    pub expires_at: Option<u64>,
}

/// Constructs a standard `multipart/form-data` payload in memory.
#[must_use]
pub fn build_multipart_payload(
    boundary: &str,
    fields: &[(&str, &str)],
    file_field: &str,
    filename: &str,
    mime_type: &str,
    file_bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();

    for (key, val) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{val}\r\n")
                .as_bytes(),
        );
    }

    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{file_field}\"; filename=\"{filename}\"\r\nContent-Type: {mime_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    body
}

/// Attempts to upload artwork bytes to Litterbox.
pub fn upload_to_litterbox(bytes: &[u8], filename: &str) -> Result<UploadResult, String> {
    let boundary = generate_boundary();
    let body = build_multipart_payload(
        &boundary,
        &[("reqtype", "fileupload"), ("time", "24h")],
        "fileToUpload",
        filename,
        "image/jpeg",
        bytes,
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Litterbox HTTP client initialization failed: {e}"))?;

    let response = client
        .post(LITTERBOX_API_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .map_err(|e| format!("Litterbox upload request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Litterbox returned status {}", response.status()));
    }

    let hosted_url = response
        .text()
        .map_err(|e| format!("Failed reading Litterbox response: {e}"))?
        .trim()
        .to_string();

    if !hosted_url.starts_with("https://") {
        return Err(format!("Litterbox returned invalid URL: {hosted_url}"));
    }

    let now = crate::discord::artwork::cache::current_unix_timestamp();
    Ok(UploadResult {
        hosted_url,
        host: HostingProvider::Litterbox,
        expires_at: Some(now + LITTERBOX_EXPIRY_SECS),
    })
}

/// Attempts to upload artwork bytes to Catbox.
pub fn upload_to_catbox(bytes: &[u8], filename: &str) -> Result<UploadResult, String> {
    let boundary = generate_boundary();
    let body = build_multipart_payload(
        &boundary,
        &[("reqtype", "fileupload")],
        "fileToUpload",
        filename,
        "image/jpeg",
        bytes,
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Catbox HTTP client initialization failed: {e}"))?;

    let response = client
        .post(CATBOX_API_URL)
        .header(
            reqwest::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(body)
        .send()
        .map_err(|e| format!("Catbox upload request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Catbox returned status {}", response.status()));
    }

    let hosted_url = response
        .text()
        .map_err(|e| format!("Failed reading Catbox response: {e}"))?
        .trim()
        .to_string();

    if !hosted_url.starts_with("https://") {
        return Err(format!("Catbox returned invalid URL: {hosted_url}"));
    }

    Ok(UploadResult {
        hosted_url,
        host: HostingProvider::Catbox,
        expires_at: None,
    })
}

/// Uploads artwork bytes with Litterbox-first fallback to Catbox strategy.
pub fn upload_artwork(bytes: &[u8], filename: &str) -> Result<UploadResult, String> {
    // Try Litterbox
    match upload_to_litterbox(bytes, filename) {
        Ok(res) => Ok(res),
        Err(litterbox_err) => {
            // Fallback to Catbox
            upload_to_catbox(bytes, filename)
                .map_err(|catbox_err| format!("Upload failed on all providers: Litterbox ({litterbox_err}); Catbox ({catbox_err})"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_body_construction() {
        let boundary = "test_boundary";
        let body = build_multipart_payload(
            boundary,
            &[("reqtype", "fileupload"), ("time", "24h")],
            "fileToUpload",
            "art.jpg",
            "image/jpeg",
            b"test-bytes",
        );

        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("--test_boundary\r\n"));
        assert!(body_str.contains("name=\"reqtype\"\r\n\r\nfileupload\r\n"));
        assert!(body_str.contains("filename=\"art.jpg\"\r\nContent-Type: image/jpeg\r\n\r\ntest-bytes\r\n"));
        assert!(body_str.ends_with("--test_boundary--\r\n"));
    }
}