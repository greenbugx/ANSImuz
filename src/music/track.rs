use pyo3::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub provider: String,
    pub track_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub stream_url: Option<String>,
    pub art_url: Option<String>,
    pub spotify_id: Option<String>,
    pub artists: Vec<String>,
    pub duration_ms: Option<u64>,
}

impl Track {
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        let artist_str = artist.into();
        Self {
            provider: "ytmusic".to_string(),
            track_id: None,
            title: title.into(),
            artist: artist_str.clone(),
            album: None,
            stream_url: None,
            art_url: None,
            spotify_id: None,
            artists: vec![artist_str],
            duration_ms: None,
        }
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = provider.into();
        self
    }

    pub fn with_track_id(mut self, id: impl Into<String>) -> Self {
        let s = id.into();
        if self.provider == "spotify" || self.spotify_id.is_none() {
            self.spotify_id = Some(s.clone());
        }
        self.track_id = Some(s);
        self
    }

    pub fn with_stream_url(mut self, url: impl Into<String>) -> Self {
        let u = url.into();
        self.stream_url = if u.trim().is_empty() { None } else { Some(u) };
        self
    }

    pub fn with_art_url(mut self, url: impl Into<String>) -> Self {
        let u = url.into();
        self.art_url = if u.trim().is_empty() { None } else { Some(u) };
        self
    }

    pub fn with_spotify_id(mut self, id: impl Into<String>) -> Self {
        let id_str = id.into();
        self.spotify_id = Some(id_str.clone());
        if self.track_id.is_none() {
            self.track_id = Some(id_str);
        }
        self
    }

    pub fn with_artists(mut self, artists: Vec<String>) -> Self {
        self.artists = artists;
        self
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        let a = album.into();
        self.album = if a.trim().is_empty() { None } else { Some(a) };
        self
    }

    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn has_stream_url(&self) -> bool {
        self.stream_url
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
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
        // If both tracks have matching provider and track IDs, they match
        if !self.provider.is_empty()
            && self.provider == other.provider
            && let (Some(a), Some(b)) = (&self.track_id, &other.track_id)
        {
            return a == b;
        }

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

        let provider = item
            .get_item("provider")
            .ok()
            .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            .or_else(|| {
                item.get_item("source")
                    .ok()
                    .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "ytmusic".to_string());

        let track_id = item
            .get_item("track_id")
            .ok()
            .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            .or_else(|| {
                item.get_item("spotify_id")
                    .ok()
                    .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            })
            .or_else(|| {
                item.get_item("video_id")
                    .ok()
                    .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            })
            .filter(|s| !s.trim().is_empty());

        let album = item
            .get_item("album")
            .ok()
            .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            .filter(|s| !s.trim().is_empty());

        let stream_url = item
            .get_item("stream_url")
            .ok()
            .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            .filter(|s| !s.trim().is_empty());

        let art_url = item
            .get_item("art_url")
            .ok()
            .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            .filter(|s| !s.trim().is_empty());

        let spotify_id = item
            .get_item("spotify_id")
            .ok()
            .and_then(|v| v.extract::<Option<String>>().ok().flatten())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                if provider == "spotify" {
                    track_id.clone()
                } else {
                    None
                }
            });

        let artists = item
            .get_item("artists")
            .ok()
            .and_then(|v| v.extract::<Vec<String>>().ok())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec![artist.clone()]);

        let duration_ms = item
            .get_item("duration_ms")
            .ok()
            .and_then(|v| v.extract::<Option<u64>>().ok().flatten());

        Ok(Self {
            provider,
            track_id,
            title,
            artist,
            album,
            stream_url,
            art_url,
            spotify_id,
            artists,
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
            .with_provider("ytmusic")
            .with_track_id("vid123")
            .with_stream_url("https://music.youtube.com/watch?v=123")
            .with_art_url("https://img.youtube.com/vi/123/hqdefault.jpg")
            .with_spotify_id("sp_abc")
            .with_album("Greatest Hits")
            .with_duration_ms(240_000);

        assert_eq!(t.provider, "ytmusic");
        assert_eq!(t.track_id.as_deref(), Some("vid123"));
        assert_eq!(t.display_title(), "Song Title");
        assert_eq!(t.display_artist(), "Artist Name");
        assert!(t.has_stream_url());
        assert!(t.is_resolved());
        assert_eq!(t.album.as_deref(), Some("Greatest Hits"));
        assert_eq!(
            t.stream_url.as_deref(),
            Some("https://music.youtube.com/watch?v=123")
        );
        assert_eq!(
            t.art_url.as_deref(),
            Some("https://img.youtube.com/vi/123/hqdefault.jpg")
        );
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
    fn test_same_provider_track_id_match() {
        let t1 = Track::new("Song A", "Artist A")
            .with_provider("ytmusic")
            .with_track_id("yt_id_1");
        let t2 = Track::new("Song B", "Artist B")
            .with_provider("ytmusic")
            .with_track_id("yt_id_1");

        assert!(t1.same_identity(&t2));
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

    #[test]
    fn test_from_py_ytmusic_with_and_without_art() {
        use pyo3::types::PyDict;

        Python::attach(|py| {
            // Track with art and album
            let dict = PyDict::new(py);
            dict.set_item("provider", "ytmusic").unwrap();
            dict.set_item("track_id", "vid_123").unwrap();
            dict.set_item("title", "Night Falls").unwrap();
            dict.set_item("artist", "Jody Wisternoff, Booka Shade")
                .unwrap();
            dict.set_item("album", "Night Falls Album").unwrap();
            dict.set_item("art_url", "https://img.youtube.com/vi/vid_123/maxresdefault.jpg")
                .unwrap();
            dict.set_item("stream_url", "https://youtube.com/watch?v=vid_123")
                .unwrap();

            let track = Track::from_py(&dict.into_any()).unwrap();
            assert_eq!(track.provider, "ytmusic");
            assert_eq!(track.track_id.as_deref(), Some("vid_123"));
            assert_eq!(track.title, "Night Falls");
            assert_eq!(track.artist, "Jody Wisternoff, Booka Shade");
            assert_eq!(track.album.as_deref(), Some("Night Falls Album"));
            assert_eq!(
                track.art_url.as_deref(),
                Some("https://img.youtube.com/vi/vid_123/maxresdefault.jpg")
            );
            assert_eq!(
                track.stream_url.as_deref(),
                Some("https://youtube.com/watch?v=vid_123")
            );

            // Track without artwork and without album
            let dict_no_art = PyDict::new(py);
            dict_no_art.set_item("provider", "ytmusic").unwrap();
            dict_no_art.set_item("track_id", "vid_456").unwrap();
            dict_no_art.set_item("title", "No Art Track").unwrap();
            dict_no_art.set_item("artist", "Solo Artist").unwrap();
            dict_no_art.set_item("album", py.None()).unwrap();
            dict_no_art.set_item("art_url", py.None()).unwrap();
            dict_no_art
                .set_item("stream_url", "https://youtube.com/watch?v=vid_456")
                .unwrap();

            let track_no_art = Track::from_py(&dict_no_art.into_any()).unwrap();
            assert_eq!(track_no_art.provider, "ytmusic");
            assert_eq!(track_no_art.track_id.as_deref(), Some("vid_456"));
            assert_eq!(track_no_art.album, None);
            assert_eq!(track_no_art.art_url, None);
            assert_eq!(
                track_no_art.stream_url.as_deref(),
                Some("https://youtube.com/watch?v=vid_456")
            );
        });
    }

    #[test]
    fn test_from_py_spotify_with_and_without_art() {
        use pyo3::types::PyDict;

        Python::attach(|py| {
            // Spotify track with art, no stream_url yet
            let dict = PyDict::new(py);
            dict.set_item("provider", "spotify").unwrap();
            dict.set_item("track_id", "sp_track_1").unwrap();
            dict.set_item("title", "Bohemian Rhapsody").unwrap();
            dict.set_item("artist", "Queen").unwrap();
            dict.set_item("album", "A Night at the Opera").unwrap();
            dict.set_item("art_url", "https://i.scdn.co/image/ab67616d0000b273")
                .unwrap();
            dict.set_item("stream_url", py.None()).unwrap();

            let track = Track::from_py(&dict.into_any()).unwrap();
            assert_eq!(track.provider, "spotify");
            assert_eq!(track.track_id.as_deref(), Some("sp_track_1"));
            assert_eq!(track.spotify_id.as_deref(), Some("sp_track_1"));
            assert_eq!(track.title, "Bohemian Rhapsody");
            assert_eq!(track.artist, "Queen");
            assert_eq!(track.album.as_deref(), Some("A Night at the Opera"));
            assert_eq!(
                track.art_url.as_deref(),
                Some("https://i.scdn.co/image/ab67616d0000b273")
            );
            assert_eq!(track.stream_url, None);
            assert!(!track.has_stream_url());

            // Spotify track without art
            let dict_no_art = PyDict::new(py);
            dict_no_art.set_item("provider", "spotify").unwrap();
            dict_no_art.set_item("track_id", "sp_track_2").unwrap();
            dict_no_art.set_item("title", "Obscure Track").unwrap();
            dict_no_art.set_item("artist", "Obscure Artist").unwrap();
            dict_no_art.set_item("album", py.None()).unwrap();
            dict_no_art.set_item("art_url", py.None()).unwrap();
            dict_no_art.set_item("stream_url", py.None()).unwrap();

            let track_no_art = Track::from_py(&dict_no_art.into_any()).unwrap();
            assert_eq!(track_no_art.provider, "spotify");
            assert_eq!(track_no_art.track_id.as_deref(), Some("sp_track_2"));
            assert_eq!(track_no_art.spotify_id.as_deref(), Some("sp_track_2"));
            assert_eq!(track_no_art.art_url, None);
            assert_eq!(track_no_art.stream_url, None);
        });
    }
}
