pub mod cache;
pub mod downloader;
pub mod manager;
pub mod types;
pub mod uploader;

pub use cache::ArtworkCache;
pub use downloader::{DownloadedArtwork, download_and_normalize, normalize_image};
pub use manager::ArtworkManager;
pub use types::{ArtworkRequest, ArtworkResult, CacheIndex, CacheRecord, HostingProvider};
pub use uploader::{UploadResult, upload_artwork, upload_to_catbox, upload_to_litterbox};

