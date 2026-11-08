use pyo3::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        let artist_str = artist.into();
        Self {
            title: title.into(),
            artist: artist_str.clone(),
            stream_url: String::new(),
            art_url: String::new(),
            spotify_id: None,
            artists: vec![artist_str],
            album: None,
            duration_ms: None,
        }
    }

    pub fn with_stream_url(mut self, url: impl Into<String>) -> Self {
        self.stream_url = url.into();
        self
    }

    pub fn with_art_url(mut self, url: impl Into<String>) -> Self {
        self.art_url = url.into();
        self
    }

    pub fn with_spotify_id(mut self, id: impl Into<String>) -> Self {
        self.spotify_id = Some(id.into());
        self
    }

    pub fn with_artists(mut self, artists: Vec<String>) -> Self {
        self.artists = artists;
        self
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn has_stream_url(&self) -> bool {
        !self.stream_url.trim().is_empty()
    }

    pub fn is_resolved(&self) -> bool {
        self.has_stream_url()
    }

    pub fn display_artist(&self) -> &str {
        let trimmed = self.artist.trim();
        if !trimmed.is_empty() {
            trimmed
        } else if let Some(first) = self.artists.first() {
            let t = first.trim();
            if !t.is_empty() { t } else { "Unknown Artist" }
        } else {
            "Unknown Artist"
        }
    }

    pub fn display_title(&self) -> &str {
        let trimmed = self.title.trim();
        if trimmed.is_empty() {
            "Unknown Track"
        } else {
            trimmed
        }
    }

    pub fn cache_key(&self) -> String {
        format!(
            "{}—{}",
            self.title.to_lowercase().trim(),
            self.artist.to_lowercase().trim()
        )
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        // If both tracks have Spotify IDs, they match if and only if the IDs match.
        // Differing Spotify IDs denote distinct releases/recordings even if titles match.
        if let (Some(a), Some(b)) = (&self.spotify_id, &other.spotify_id) {
            return a == b;
        }

        // If both tracks have non-empty stream URLs, they match if and only if the URLs match.
        if self.has_stream_url() && other.has_stream_url() {
            return self.stream_url == other.stream_url;
        }

        // Fallback: normalized title and artist comparison (cross-provider matching)
        self.title.trim().eq_ignore_ascii_case(other.title.trim())
            && self.artist.trim().eq_ignore_ascii_case(other.artist.trim())
    }

    pub fn is_same(&self, other: &Self) -> bool {
        self.same_identity(other)
    }

    pub fn matches_identity(&self, spotify_id: Option<&str>, title: &str, artist: &str) -> bool {
        if let (Some(a), Some(b)) = (&self.spotify_id, spotify_id) {
            return a.as_str() == b;
        }
        self.title.trim().eq_ignore_ascii_case(title.trim())
            && self.artist.trim().eq_ignore_ascii_case(artist.trim())
    }

    pub fn from_py<'py>(item: &Bound<'py, PyAny>) -> PyResult<Self> {
        let title = item.get_item("title")?.extract::<String>()?;
        let artist = item.get_item("artist")?.extract::<String>()?;
        let stream_url = item
            .get_item("stream_url")
            .and_then(|v| v.extract::<String>())
            .unwrap_or_default();
        let art_url = item
            .get_item("art_url")
            .and_then(|v| v.extract::<String>())
            .unwrap_or_default();

        let spotify_id = item
            .get_item("spotify_id")
            .ok()
            .and_then(|v| v.extract::<String>().ok())
            .filter(|s| !s.is_empty());

        let artists = item
            .get_item("artists")
            .ok()
            .and_then(|v| v.extract::<Vec<String>>().ok())
            .unwrap_or_else(|| vec![artist.clone()]);

        let album = item
            .get_item("album")
            .ok()
            .and_then(|v| v.extract::<String>().ok())
            .filter(|s| !s.is_empty());

        let duration_ms = item
            .get_item("duration_ms")
            .ok()
            .and_then(|v| v.extract::<u64>().ok());

        Ok(Self {
            title,
            artist,
            stream_url,
            art_url,
            spotify_id,
            artists,
            album,
            duration_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackSource {
    SearchResult { index: usize },
    Queue { entry_id: u64 },
    History,
    Current,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_builder_and_display() {
        let t = Track::new("  Song Title  ", "  Artist Name  ")
            .with_stream_url("https://music.youtube.com/watch?v=123")
            .with_spotify_id("sp_abc")
            .with_album("Greatest Hits")
            .with_duration_ms(240_000);

        assert_eq!(t.display_title(), "Song Title");
        assert_eq!(t.display_artist(), "Artist Name");
        assert!(t.has_stream_url());
        assert!(t.is_resolved());
        assert_eq!(t.cache_key(), "song title—artist name");
    }

    #[test]
    fn test_differing_spotify_ids_do_not_match_even_with_same_metadata() {
        let t1 = Track::new("Yesterday", "The Beatles").with_spotify_id("sp_original");
        let t2 = Track::new("Yesterday", "The Beatles").with_spotify_id("sp_remaster_2009");

        assert!(
            !t1.same_identity(&t2),
            "Tracks with different Spotify IDs must never be considered the same identity"
        );
        assert!(!t1.is_same(&t2));
    }

    #[test]
    fn test_same_spotify_ids_match() {
        let t1 = Track::new("Song", "Artist").with_spotify_id("sp_same");
        let t2 = Track::new("Song", "Artist").with_spotify_id("sp_same");

        assert!(t1.same_identity(&t2));
        assert!(t1.is_same(&t2));
    }

    #[test]
    fn test_differing_stream_urls_do_not_match() {
        let t1 = Track::new("Song", "Artist").with_stream_url("http://yt/1");
        let t2 = Track::new("Song", "Artist").with_stream_url("http://yt/2");

        assert!(
            !t1.same_identity(&t2),
            "Different non-empty stream URLs indicate different audio streams"
        );
    }

    #[test]
    fn test_cross_provider_fallback_matches_by_normalized_metadata() {
        let t1 = Track::new("  My Song  ", "  My Artist  ").with_spotify_id("sp_only");
        let t2 = Track::new("MY SONG", "my artist").with_stream_url("http://yt/resolved");

        assert!(
            t1.same_identity(&t2),
            "Cross-provider tracks with matching metadata must match identity"
        );
    }

    #[test]
    fn test_matches_identity_helper() {
        let t = Track::new("Title", "Artist").with_spotify_id("sp_123");
        assert!(t.matches_identity(Some("sp_123"), "Other Title", "Other Artist"));
        assert!(!t.matches_identity(Some("sp_456"), "Title", "Artist"));
        assert!(t.matches_identity(None, "title", "artist"));
    }
}
