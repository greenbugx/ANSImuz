use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Discord Rich Presence artwork resolution request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtworkRequest {
    pub provider: String,
    pub track_id: Option<String>,
    pub source_url: String,
}

impl ArtworkRequest {
    #[must_use]
    pub fn new(provider: impl Into<String>, track_id: Option<String>, source_url: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            track_id,
            source_url: source_url.into(),
        }
    }
}

/// Discord Rich Presence artwork resolution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtworkResult {
    pub source_url: String,
    pub hosted_url: String,
    pub local_path: PathBuf,
    pub content_hash: String,
}

/// Supported third-party image hosting providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostingProvider {
    Catbox,
    Litterbox,
}

impl fmt::Display for HostingProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catbox => write!(f, "catbox"),
            Self::Litterbox => write!(f, "litterbox"),
        }
    }
}

/// Metadata record stored in persistent `index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheRecord {
    pub source_url: String,
    pub local_file: String,
    pub hosted_url: String,
    pub host: HostingProvider,
    pub content_hash: String,
    pub uploaded_at: u64,
    pub expires_at: Option<u64>,
}

/// Root index of cached Discord Rich Presence artwork mappings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheIndex {
    #[serde(default)]
    pub entries: std::collections::HashMap<String, CacheRecord>,
}

/// Result of an upload operation from an artwork uploader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadedArtwork {
    pub url: String,
    pub provider: HostingProvider,
    pub expires_at: Option<u64>,
}

/// Errors occurring during artwork fetching, normalization, caching, or uploading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtworkError {
    InvalidUrl(String),
    DownloadFailed(String),
    NormalizationFailed(String),
    UploadFailed(String),
    InvalidResponse(String),
    CooldownActive,
    Io(String),
}

impl fmt::Display for ArtworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(msg) => write!(f, "Invalid artwork URL: {msg}"),
            Self::DownloadFailed(msg) => write!(f, "Artwork download failed: {msg}"),
            Self::NormalizationFailed(msg) => write!(f, "Artwork normalization failed: {msg}"),
            Self::UploadFailed(msg) => write!(f, "Artwork upload failed: {msg}"),
            Self::InvalidResponse(msg) => write!(f, "Invalid artwork response: {msg}"),
            Self::CooldownActive => write!(f, "Artwork retry cooldown is active"),
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for ArtworkError {}


