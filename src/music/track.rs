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

impl Track {
    pub fn is_same(&self, other: &Track) -> bool {
        if let (Some(a), Some(b)) = (&self.spotify_id, &other.spotify_id)
            && a == b
        {
            return true;
        }
        if !self.stream_url.is_empty() && self.stream_url == other.stream_url {
            return true;
        }
        self.title.trim().eq_ignore_ascii_case(other.title.trim())
            && self.artist.trim().eq_ignore_ascii_case(other.artist.trim())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackSource {
    SearchResult { index: usize },
    Queue { entry_id: u64 },
    History,
    Current,
}
