#[derive(Clone, Debug)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub stream_url: String,
    pub art_url: String,
    pub spotify_id: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackSource {
    SearchResult { index: usize },
    Queue { entry_id: u64 },
    History,
    Current,
}
