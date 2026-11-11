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
    pub fn new(
        provider: impl Into<String>,
        track_id: Option<String>,
        source_url: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            track_id,
            source_url: source_url.into(),
        }
    }

    /// Extracts an artwork resolution request from a track when a non-empty artwork URL is present.
    #[must_use]
    pub fn from_track(track: &crate::music::Track) -> Option<Self> {
        let art_url = track
            .art_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())?;
        Some(Self::new(
            track.provider.clone(),
            track.track_id.clone(),
            art_url.to_string(),
        ))
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

/// Configured artwork hosting target parsed from configuration strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostConfig {
    Disabled,
    Provider(HostingProvider),
}

impl HostConfig {
    /// Parses a host configuration value
    /// ("imgdb", "catbox", "litterbox", "none", or empty).
    pub fn parse(value: &str) -> Result<Self, ArtworkError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "imgdb" => Ok(Self::Provider(HostingProvider::Imgdb)),
            "catbox" => Ok(Self::Provider(HostingProvider::Catbox)),
            "litterbox" => Ok(Self::Provider(HostingProvider::Litterbox)),
            "" | "none" => Ok(Self::Disabled),
            other => Err(ArtworkError::UploadFailed(format!(
                "Unsupported host: {other}"
            ))),
        }
    }

    /// Returns the concrete hosting provider when enabled.
    #[must_use]
    pub fn as_provider(self) -> Option<HostingProvider> {
        match self {
            Self::Disabled => None,
            Self::Provider(provider) => Some(provider),
        }
    }
}

impl fmt::Display for HostConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "none"),
            Self::Provider(provider) => provider.fmt(f),
        }
    }
}

/// Supported third-party image hosting providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostingProvider {
    Imgdb,
    Catbox,
    Litterbox,
}

impl fmt::Display for HostingProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Imgdb => write!(f, "imgdb"),
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
    #[serde(default)]
    pub last_accessed: u64,
}

impl CacheRecord {
    /// Returns true if this record has no known expiration timestamp (e.g., anonymous Catbox upload).
    ///
    /// Note that having no known expiry does not guarantee permanent remote retention.
    #[must_use]
    pub fn has_no_known_expiry(&self) -> bool {
        self.expires_at.is_none() && self.host == HostingProvider::Catbox
    }
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

/// Subcomponent that failed during artwork manager initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkInitComponent {
    Cache,
    Downloader,
    ImgdbUploader,
    CatboxUploader,
    LitterboxUploader,
}

impl fmt::Display for ArtworkInitComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cache => write!(f, "cache"),
            Self::Downloader => write!(f, "downloader"),
            Self::ImgdbUploader => write!(f, "imgdb uploader"),
            Self::CatboxUploader => write!(f, "catbox uploader"),
            Self::LitterboxUploader => write!(f, "litterbox uploader"),
        }
    }
}

/// Errors occurring during artwork fetching, normalization, caching, or uploading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtworkError {
    InitializationFailed {
        component: ArtworkInitComponent,
        message: String,
    },
    InvalidUrl(String),
    DownloadFailed(String),
    NormalizationFailed(String),
    UploadFailed(String),
    InvalidResponse(String),
    /// Provider rejected the upload with HTTP 429 Too Many Requests.
    ///
    /// `retry_after_secs` is populated only when the server supplies an integer delta-seconds
    /// `Retry-After` header (HTTP-date values are unsupported and yield `None`).
    RateLimited {
        provider: HostingProvider,
        retry_after_secs: Option<u64>,
        message: String,
    },
    CooldownActive,
    Io(String),
}

impl fmt::Display for ArtworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializationFailed { component, message } => {
                write!(f, "Artwork {component} initialization failed: {message}")
            }
            Self::InvalidUrl(msg) => write!(f, "Invalid artwork URL: {msg}"),
            Self::DownloadFailed(msg) => write!(f, "Artwork download failed: {msg}"),
            Self::NormalizationFailed(msg) => write!(f, "Artwork normalization failed: {msg}"),
            Self::UploadFailed(msg) => write!(f, "Artwork upload failed: {msg}"),
            Self::InvalidResponse(msg) => write!(f, "Invalid artwork response: {msg}"),
            Self::RateLimited { message, .. } => write!(f, "Artwork host rate-limited: {message}"),
            Self::CooldownActive => write!(f, "Artwork retry cooldown is active"),
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for ArtworkError {}
