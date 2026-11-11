pub mod cache;
pub mod downloader;
pub mod manager;
pub mod types;
pub mod uploader;
pub mod worker;

use std::time::Duration;

const ARTWORK_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Builds a shared HTTP/1.1 blocking client with consistent user-agent, HTTPS enforcement, and timeouts.
pub(super) fn build_http_client_with_https(
    connect_timeout: Duration,
    timeout: Duration,
    redirect_policy: Option<reqwest::redirect::Policy>,
    https_only: bool,
) -> Result<reqwest::blocking::Client, reqwest::Error> {
    let mut builder = reqwest::blocking::Client::builder()
        .http1_only()
        .https_only(https_only)
        .user_agent(ARTWORK_USER_AGENT)
        .connect_timeout(connect_timeout)
        .timeout(timeout);
    if let Some(policy) = redirect_policy {
        builder = builder.redirect(policy);
    }
    builder.build()
}

/// Builds a shared HTTPS-only HTTP/1.1 blocking client with consistent user-agent and timeouts.
pub(super) fn build_http_client(
    connect_timeout: Duration,
    timeout: Duration,
    redirect_policy: Option<reqwest::redirect::Policy>,
) -> Result<reqwest::blocking::Client, reqwest::Error> {
    build_http_client_with_https(connect_timeout, timeout, redirect_policy, true)
}

pub use cache::ArtworkCache;
pub use downloader::{
    ArtworkDownloader, DownloadedArtwork, download_and_normalize, normalize_image,
};
pub use manager::ArtworkManager;
pub use types::{
    ArtworkError, ArtworkInitComponent, ArtworkRequest, ArtworkResult, CacheIndex, CacheRecord,
    HostConfig, HostingProvider, NormalizedImageFormat, UploadedArtwork,
};
pub use uploader::{ArtworkUploader, CatboxUploader, ImgdbUploader, LitterboxUploader};
pub use worker::{ArtworkCommand, ArtworkEvent, ArtworkWorker};
