pub mod cache;
pub mod downloader;
pub mod manager;
pub mod types;
pub mod uploader;

pub use cache::ArtworkCache;
pub use downloader::{DownloadedArtwork, download_and_normalize, normalize_image};
pub use manager::ArtworkManager;
pub use types::{
    ArtworkError, ArtworkRequest, ArtworkResult, CacheIndex, CacheRecord, HostingProvider,
    UploadedArtwork,
};
pub use uploader::{
    ArtworkUploader, CatboxUploader, FallbackUploader, LitterboxUploader,
};
