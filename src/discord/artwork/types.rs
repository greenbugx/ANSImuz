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

