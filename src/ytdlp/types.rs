use std::time::SystemTime;

/// Direct audio stream URL and metadata resolved from yt-dlp
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStream {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub expires_at: Option<SystemTime>,
    pub ext: Option<String>,
    pub acodec: Option<String>,
    pub abr: Option<f64>,
    pub duration_s: Option<f64>,
    pub video_id: Option<String>,
    pub title: Option<String>,
    pub ytdlp_version: String,
    pub js_runtime: String,
}

/// Snapshot of the embedded yt-dlp resolver and active JS runtime
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolverStatus {
    pub version: String,
    pub bundle_age: Option<String>,
    pub js_runtime: String,
    pub js_runtime_path: Option<String>,
}

/// Abstraction over YouTube stream URL extraction for testing without Python
pub trait StreamResolver: Send + Sync {
    fn resolve(&self, watch_url: &str, background: bool) -> Result<ResolvedStream, String>;
}
