use presenceforge::activity::Activity;
use presenceforge::{ActivityBuilder, ActivityType, StatusDisplayType};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ANSIMUZ_GITHUB_URL: &str = "https://github.com/greenbugx/ANSImuz";

pub const DEFAULT_LARGE_IMAGE: &str = "logo";
pub const DEFAULT_LARGE_TEXT: &str = "ANSImuz";
pub const DEFAULT_SMALL_IMAGE: &str = "logo";
pub const DEFAULT_SMALL_TEXT: &str = "ANSImuz";

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub is_paused: bool,
    pub provider: Option<String>,
    pub public_track_url: Option<String>,
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
}

impl PlaybackInfo {
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            album: None,
            position: None,
            duration: None,
            is_paused: false,
            provider: None,
            public_track_url: None,
            large_image: None,
            large_text: None,
            small_image: None,
            small_text: None,
        }
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    pub fn with_position(mut self, position: Duration) -> Self {
        self.position = Some(position);
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn with_paused(mut self, is_paused: bool) -> Self {
        self.is_paused = is_paused;
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn with_public_track_url(mut self, url: impl Into<String>) -> Self {
        self.public_track_url = Some(url.into());
        self
    }

    pub fn with_large_image(mut self, image: impl Into<String>) -> Self {
        self.large_image = Some(image.into());
        self
    }

    pub fn with_large_text(mut self, text: impl Into<String>) -> Self {
        self.large_text = Some(text.into());
        self
    }

    pub fn with_small_image(mut self, image: impl Into<String>) -> Self {
        self.small_image = Some(image.into());
        self
    }

    pub fn with_small_text(mut self, text: impl Into<String>) -> Self {
        self.small_text = Some(text.into());
        self
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn is_clean_track_id(id: &str) -> bool {
    !id.is_empty()
        && !id
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '&' | '=' | ':'))
}

fn is_valid_button_url(url: &str) -> bool {
    let trimmed = url.trim();
    let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.is_empty()
        && !rest.starts_with('/')
        && !trimmed.chars().any(char::is_whitespace)
        && trimmed.chars().count() <= 512
        && trimmed.len() <= 512
}

fn button_label_for_provider(provider: Option<&str>) -> &'static str {
    match provider.map(|p| p.trim().to_ascii_lowercase()).as_deref() {
        Some("ytmusic" | "youtube") => "Listen on YouTube Music",
        Some("spotify") => "Listen on Spotify",
        _ => "Open Track",
    }
}

/// Derive a safe public track URL for Discord activity buttons from a track.
///
/// Never exposes temporary or internal `mpv` stream URLs.
pub fn public_track_url_from_track(track: &crate::music::Track) -> Option<String> {
    let provider = track.provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "ytmusic" | "youtube" => {
            if let Some(id) = non_empty(track.track_id.as_deref())
                && is_clean_track_id(id)
            {
                return Some(format!("https://music.youtube.com/watch?v={id}"));
            }

            let stream_url = non_empty(track.stream_url.as_deref())?;
            for prefix in [
                "https://music.youtube.com/watch?v=",
                "http://music.youtube.com/watch?v=",
                "https://www.youtube.com/watch?v=",
                "http://www.youtube.com/watch?v=",
                "https://youtube.com/watch?v=",
                "http://youtube.com/watch?v=",
            ] {
                if let Some(rest) = stream_url.strip_prefix(prefix) {
                    let video_id = rest.split(['&', '#']).next().unwrap_or("");
                    if is_clean_track_id(video_id) {
                        return Some(format!("https://music.youtube.com/watch?v={video_id}"));
                    }
                }
            }
            None
        }
        "spotify" => {
            let raw_id = non_empty(track.spotify_id.as_deref())
                .or_else(|| non_empty(track.track_id.as_deref()))?;
            if let Some(track_part) = raw_id
                .strip_prefix("https://open.spotify.com/track/")
                .or_else(|| raw_id.strip_prefix("http://open.spotify.com/track/"))
            {
                let clean_id = track_part.split(['?', '#']).next().unwrap_or("");
                if is_clean_track_id(clean_id) {
                    return Some(format!("https://open.spotify.com/track/{clean_id}"));
                }
                return None;
            }
            let id = raw_id.strip_prefix("spotify:track:").unwrap_or(raw_id);
            if is_clean_track_id(id) {
                return Some(format!("https://open.spotify.com/track/{id}"));
            }
            None
        }
        _ => None,
    }
}

impl From<&crate::music::Track> for PlaybackInfo {
    fn from(track: &crate::music::Track) -> Self {
        let mut info = Self::new(&track.title, &track.artist);
        if let Some(provider) = non_empty(Some(track.provider.as_str())) {
            info = info.with_provider(provider);
        }
        if let Some(url) = public_track_url_from_track(track) {
            info = info.with_public_track_url(url);
        }
        if let Some(album) = &track.album {
            info = info.with_album(album);
        }
        if let Some(dur_ms) = track.duration_ms {
            info = info.with_duration(Duration::from_millis(dur_ms));
        }
        info
    }
}

fn truncate_activity_text(s: &str, max_chars: usize, max_bytes: usize) -> String {
    if s.chars().count() <= max_chars && s.len() <= max_bytes {
        return s.to_string();
    }

    const ELLIPSIS: char = '…';
    let ellipsis_bytes = ELLIPSIS.len_utf8();
    let char_budget = max_chars.saturating_sub(1);
    let byte_budget = max_bytes.saturating_sub(ellipsis_bytes);

    let mut result = String::new();
    for (count, ch) in s.chars().enumerate() {
        if count >= char_budget || result.len() + ch.len_utf8() > byte_budget {
            break;
        }
        result.push(ch);
    }
    result.push(ELLIPSIS);
    result
}

pub fn build_activity(info: &PlaybackInfo) -> Activity {
    let mut builder = ActivityBuilder::new()
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::Details);

    // Discord max length: 128 characters
    // Details: track title
    let details = if info.title.trim().is_empty() {
        "Unknown Track".to_string()
    } else {
        truncate_activity_text(info.title.trim(), 128, 128)
    };
    builder = builder.details(details);

    // State: artist names
    let state_text = if info.artist.trim().is_empty() {
        "Unknown Artist".to_string()
    } else {
        truncate_activity_text(info.artist.trim(), 128, 128)
    };
    builder = builder.state(state_text);

    // Assets:
    // large_image: hosted album artwork URL or fallback asset key ("logo")
    let large_image = info
        .large_image
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_LARGE_IMAGE);

    // large_text: album name (or fallback to ANSImuz)
    let large_text = info
        .large_text
        .as_deref()
        .or(info.album.as_deref())
        .filter(|s| !s.trim().is_empty())
        .map(|s| truncate_activity_text(s.trim(), 128, 128))
        .unwrap_or_else(|| DEFAULT_LARGE_TEXT.to_string());

    // small_image: ANSImuz application asset key ("logo")
    let small_image = info
        .small_image
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_SMALL_IMAGE);

    // small_text: ANSImuz
    let small_text = info
        .small_text
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_SMALL_TEXT);

    builder = builder
        .large_image(large_image)
        .large_text(large_text)
        .small_image(small_image)
        .small_text(small_text);

    // Timestamps and playback status logic
    if !info.is_paused {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let start_ts = if let Some(pos) = info.position {
            now.saturating_sub(pos.as_secs())
        } else {
            now
        };
        builder = builder.start_timestamp(start_ts);

        let is_finished = match (info.position, info.duration) {
            (Some(pos), Some(dur)) => pos >= dur,
            _ => false,
        };

        if !is_finished
            && let Some(dur) = info.duration
            && dur > Duration::ZERO
            && let Some(end_ts) = start_ts.checked_add(dur.as_secs())
            && end_ts > now
            && let Ok(end_i64) = i64::try_from(end_ts)
        {
            builder = builder.end_timestamp(end_i64);
        }
    }

    // Provider-aware public track link (only when a valid public URL is provided)
    if let Some(url) = info.public_track_url.as_deref().map(str::trim)
        && is_valid_button_url(url)
    {
        let label = button_label_for_provider(info.provider.as_deref());
        builder = builder.button(label, url);
    }

    // Secondary repository button
    builder = builder.button("Get ANSImuz", ANSIMUZ_GITHUB_URL);

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_activity_playing() {
        let info = PlaybackInfo::new("Bohemian Rhapsody", "Queen")
            .with_album("A Night at the Opera")
            .with_position(Duration::from_secs(30))
            .with_duration(Duration::from_secs(354))
            .with_provider("ytmusic")
            .with_public_track_url("https://music.youtube.com/watch?v=mock");

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());
        assert_eq!(activity.r#type, Some(ActivityType::Listening));
        assert_eq!(
            activity.status_display_type,
            Some(StatusDisplayType::Details)
        );

        let serialized = serde_json::to_value(&activity).unwrap();
        assert_eq!(serialized["type"], 2);
        assert_eq!(serialized["status_display_type"], 2);
        assert_eq!(serialized["details"], "Bohemian Rhapsody");

        assert_eq!(activity.details.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(activity.state.as_deref(), Some("Queen"));
        let assets = activity.assets.as_ref().expect("assets should exist");
        assert_eq!(assets.large_image.as_deref(), Some(DEFAULT_LARGE_IMAGE));
        assert_eq!(assets.large_text.as_deref(), Some("A Night at the Opera"));
        assert_eq!(assets.small_image.as_deref(), Some(DEFAULT_SMALL_IMAGE));
        assert_eq!(assets.small_text.as_deref(), Some(DEFAULT_SMALL_TEXT));

        let timestamps = activity
            .timestamps
            .as_ref()
            .expect("timestamps should exist");
        assert!(timestamps.start.is_some());
        assert!(timestamps.end.is_some());

        let buttons = activity.buttons.as_ref().expect("buttons should exist");
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0].label, "Listen on YouTube Music");
        assert_eq!(buttons[0].url, "https://music.youtube.com/watch?v=mock");
        assert_eq!(buttons[1].label, "Get ANSImuz");
        assert_eq!(buttons[1].url, ANSIMUZ_GITHUB_URL);
        activity.validate().expect("activity should validate");
    }

    #[test]
    fn test_build_activity_with_hosted_artwork() {
        let info = PlaybackInfo::new("Night Falls", "Booka Shade")
            .with_album("Movements")
            .with_large_image("https://files.catbox.moe/abc123.jpg");

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());
        let assets = activity.assets.as_ref().expect("assets should exist");
        assert_eq!(
            assets.large_image.as_deref(),
            Some("https://files.catbox.moe/abc123.jpg")
        );
        assert_eq!(assets.large_text.as_deref(), Some("Movements"));
        assert_eq!(assets.small_image.as_deref(), Some(DEFAULT_SMALL_IMAGE));
        assert_eq!(assets.small_text.as_deref(), Some(DEFAULT_SMALL_TEXT));
    }

    #[test]
    fn test_build_activity_paused() {
        let info = PlaybackInfo::new("Song", "Artist")
            .with_position(Duration::from_secs(45))
            .with_duration(Duration::from_secs(200))
            .with_paused(true);

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());
        assert_eq!(activity.r#type, Some(ActivityType::Listening));

        let assets = activity.assets.as_ref().expect("assets should exist");
        assert_eq!(assets.small_image.as_deref(), Some(DEFAULT_SMALL_IMAGE));
        assert_eq!(assets.small_text.as_deref(), Some(DEFAULT_SMALL_TEXT));

        // When paused, neither start nor end timestamps should be advertised
        if let Some(ts) = activity.timestamps.as_ref() {
            assert!(
                ts.start.is_none(),
                "Paused track must not advertise start timestamp"
            );
            assert!(
                ts.end.is_none(),
                "Paused track must not advertise end timestamp"
            );
        }
    }

    #[test]
    fn test_build_activity_resumed_recalculates_timestamps_and_preserves_artwork() {
        let mut info = PlaybackInfo::new("Song", "Artist")
            .with_position(Duration::from_secs(45))
            .with_duration(Duration::from_secs(200))
            .with_large_image("https://files.catbox.moe/abc123.jpg")
            .with_paused(true);

        // Paused state: timestamps removed/omitted, artwork preserved
        let paused_activity = build_activity(&info);
        assert_eq!(
            paused_activity
                .assets
                .as_ref()
                .unwrap()
                .large_image
                .as_deref(),
            Some("https://files.catbox.moe/abc123.jpg")
        );
        if let Some(ts) = paused_activity.timestamps.as_ref() {
            assert!(ts.start.is_none());
            assert!(ts.end.is_none());
        }

        // Resumed state: timestamps recalculated with start and end, artwork preserved
        info.is_paused = false;
        info.position = Some(Duration::from_secs(50));
        let resumed_activity = build_activity(&info);
        assert_eq!(
            resumed_activity
                .assets
                .as_ref()
                .unwrap()
                .large_image
                .as_deref(),
            Some("https://files.catbox.moe/abc123.jpg")
        );
        let ts = resumed_activity
            .timestamps
            .as_ref()
            .expect("timestamps exist on resumed");
        assert!(ts.start.is_some());
        assert!(ts.end.is_some());
    }

    #[test]
    fn test_build_activity_truncation() {
        let very_long_title = "a".repeat(200);
        let very_long_artist = "b".repeat(200);
        let info = PlaybackInfo::new(very_long_title, very_long_artist);

        let activity = build_activity(&info);

        assert!(activity.details.as_ref().unwrap().chars().count() <= 128);
        assert!(activity.details.as_ref().unwrap().len() <= 128);
        assert!(activity.state.as_ref().unwrap().chars().count() <= 128);
        assert!(activity.state.as_ref().unwrap().len() <= 128);
        assert!(activity.validate().is_ok());
    }

    #[test]
    fn test_build_activity_multibyte_truncation() {
        let multibyte_title = "日本語タイトル".repeat(30); // 30 * 7 = 210 chars, 630 bytes
        let multibyte_artist = "アーティスト名".repeat(30);
        let info = PlaybackInfo::new(multibyte_title, multibyte_artist);

        let activity = build_activity(&info);

        assert!(activity.details.as_ref().unwrap().chars().count() <= 128);
        assert!(activity.details.as_ref().unwrap().len() <= 128);
        assert!(activity.state.as_ref().unwrap().chars().count() <= 128);
        assert!(activity.state.as_ref().unwrap().len() <= 128);
        assert!(activity.validate().is_ok());
    }

    #[test]
    fn test_build_activity_finished_track() {
        let info = PlaybackInfo::new("Song", "Artist")
            .with_position(Duration::from_secs(200))
            .with_duration(Duration::from_secs(200));

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());

        if let Some(ts) = activity.timestamps.as_ref() {
            assert!(
                ts.end.is_none(),
                "Finished track should not emit an end timestamp"
            );
        }
    }

    #[test]
    fn test_build_activity_overflow_protection() {
        let info = PlaybackInfo::new("Song", "Artist")
            .with_position(Duration::from_secs(0))
            .with_duration(Duration::from_secs(u64::MAX));

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());

        if let Some(ts) = activity.timestamps.as_ref() {
            assert!(
                ts.end.is_none(),
                "Overflowing end duration should not emit end timestamp"
            );
        }
    }

    #[test]
    fn activity_type_serializes_correctly() {
        let activity = ActivityBuilder::new()
            .activity_type(ActivityType::Listening)
            .build();

        let json = serde_json::to_value(&activity).unwrap();

        assert_eq!(json["type"], 2);
    }

    #[test]
    fn all_activity_types_serialize_correctly() {
        let cases = [
            (ActivityType::Playing, 0),
            (ActivityType::Listening, 2),
            (ActivityType::Watching, 3),
            (ActivityType::Competing, 5),
        ];

        for (activity_type, expected) in cases {
            let activity = ActivityBuilder::new().activity_type(activity_type).build();

            let json = serde_json::to_value(activity).unwrap();

            assert_eq!(json["type"], expected);
        }
    }

    #[test]
    fn default_activity_omits_type() {
        let activity = Activity::default();
        let json = serde_json::to_value(activity).unwrap();

        assert!(json.get("type").is_none());
    }

    #[test]
    fn activity_type_deserializes_from_json() {
        let activity: Activity = serde_json::from_str(r#"{"type": 2}"#).unwrap();

        assert_eq!(activity.r#type, Some(ActivityType::Listening));
    }

    #[test]
    fn status_display_type_serializes_correctly() {
        for (display_type, expected) in [
            (StatusDisplayType::Name, 0),
            (StatusDisplayType::State, 1),
            (StatusDisplayType::Details, 2),
        ] {
            let activity = ActivityBuilder::new()
                .status_display_type(display_type)
                .build();
            let json = serde_json::to_value(activity).unwrap();
            assert_eq!(json["status_display_type"], expected);
        }

        let default_json = serde_json::to_value(Activity::default()).unwrap();
        assert!(default_json.get("status_display_type").is_none());
    }

    #[test]
    fn test_playback_info_and_artwork_request_from_track() {
        let track = crate::music::Track::new("Track Title", "Artist Name")
            .with_provider("spotify")
            .with_track_id("sp_123")
            .with_album("Album Name")
            .with_art_url("https://i.scdn.co/image/abc")
            .with_duration_ms(185_000);

        let info = PlaybackInfo::from(&track);
        assert_eq!(info.title, "Track Title");
        assert_eq!(info.artist, "Artist Name");
        assert_eq!(info.album.as_deref(), Some("Album Name"));
        assert_eq!(info.duration, Some(Duration::from_millis(185_000)));
        assert_eq!(info.provider.as_deref(), Some("spotify"));
        assert_eq!(
            info.public_track_url.as_deref(),
            Some("https://open.spotify.com/track/sp_123")
        );

        let artwork = crate::discord::artwork::ArtworkRequest::from_track(&track)
            .expect("artwork request should be extracted");
        assert_eq!(artwork.provider, "spotify");
        assert_eq!(artwork.track_id.as_deref(), Some("sp_123"));
        assert_eq!(artwork.source_url, "https://i.scdn.co/image/abc");

        let no_art = crate::music::Track::new("No Art", "Artist");
        assert!(crate::discord::artwork::ArtworkRequest::from_track(&no_art).is_none());
    }

    #[test]
    fn test_public_track_url_ytmusic_from_id_and_watch_url() {
        let mut from_id = crate::music::Track::new("Track", "Artist")
            .with_provider("ytmusic")
            .with_track_id("vid_full");
        from_id.stream_url =
            Some("https://rr1---sn-test.googlevideo.com/videoplayback?id=1".into());

        let info = PlaybackInfo::from(&from_id);
        assert_eq!(
            info.public_track_url.as_deref(),
            Some("https://music.youtube.com/watch?v=vid_full")
        );

        let activity = build_activity(&info);
        activity
            .validate()
            .expect("ytmusic activity should validate");
        let buttons = activity.buttons.as_ref().expect("buttons should exist");
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0].label, "Listen on YouTube Music");
        assert_eq!(buttons[0].url, "https://music.youtube.com/watch?v=vid_full");
        assert_eq!(buttons[1].label, "Get ANSImuz");
        assert_eq!(buttons[1].url, ANSIMUZ_GITHUB_URL);

        let mut from_watch_url =
            crate::music::Track::new("Track", "Artist").with_provider("ytmusic");
        from_watch_url.stream_url =
            Some("https://youtube.com/watch?v=vid_watch&feature=share".into());
        assert_eq!(
            public_track_url_from_track(&from_watch_url).as_deref(),
            Some("https://music.youtube.com/watch?v=vid_watch")
        );

        for internal_url in [
            "https://rr1---sn-test.googlevideo.com/videoplayback?id=1",
            "http://127.0.0.1:8080/audio.webm",
            "/tmp/song.mp3",
        ] {
            let mut internal = crate::music::Track::new("Track", "Artist").with_provider("ytmusic");
            internal.stream_url = Some(internal_url.into());
            assert_eq!(public_track_url_from_track(&internal), None);
        }
    }

    #[test]
    fn test_public_track_url_spotify_never_leaks_youtube_stream_url() {
        let mut track = crate::music::Track::new("Track", "Artist")
            .with_provider("spotify")
            .with_track_id("sp_track_fallback");
        track.spotify_id = Some("6rqhFgbbKwnb9MLmUQDhG6".into());
        track.stream_url = Some("https://music.youtube.com/watch?v=yt_matched".into());

        let info = PlaybackInfo::from(&track);
        assert_eq!(
            info.public_track_url.as_deref(),
            Some("https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6")
        );

        let activity = build_activity(&info);
        activity
            .validate()
            .expect("spotify activity should validate");
        let buttons = activity.buttons.as_ref().expect("buttons should exist");
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0].label, "Listen on Spotify");
        assert_eq!(
            buttons[0].url,
            "https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6"
        );
        assert_eq!(buttons[1].label, "Get ANSImuz");
        assert_eq!(buttons[1].url, ANSIMUZ_GITHUB_URL);

        let mut missing_id = crate::music::Track::new("Track", "Artist").with_provider("spotify");
        missing_id.spotify_id = Some("   ".into());
        missing_id.stream_url = Some("https://music.youtube.com/watch?v=yt_matched".into());
        assert_eq!(public_track_url_from_track(&missing_id), None);
    }

    #[test]
    fn test_build_activity_other_provider_and_invalid_urls() {
        let custom = PlaybackInfo::new("Track", "Artist")
            .with_provider("soundcloud")
            .with_public_track_url("https://example.com/tracks/1");
        let custom_activity = build_activity(&custom);
        custom_activity
            .validate()
            .expect("custom provider activity should validate");
        let custom_buttons = custom_activity
            .buttons
            .as_ref()
            .expect("buttons should exist");
        assert_eq!(custom_buttons.len(), 2);
        assert_eq!(custom_buttons[0].label, "Open Track");
        assert_eq!(custom_buttons[0].url, "https://example.com/tracks/1");
        assert_eq!(custom_buttons[1].label, "Get ANSImuz");
        assert_eq!(custom_buttons[1].url, ANSIMUZ_GITHUB_URL);

        let mut unknown = crate::music::Track::new("Local Song", "Artist")
            .with_provider("local")
            .with_track_id("local_1");
        unknown.stream_url = Some("http://127.0.0.1:9000/song.flac".into());
        let unknown_info = PlaybackInfo::from(&unknown);
        assert_eq!(unknown_info.public_track_url, None);
        let unknown_activity = build_activity(&unknown_info);
        unknown_activity
            .validate()
            .expect("fallback single-button activity should validate");
        let unknown_buttons = unknown_activity
            .buttons
            .as_ref()
            .expect("Get ANSImuz button should still be present");
        assert_eq!(unknown_buttons.len(), 1);
        assert_eq!(unknown_buttons[0].label, "Get ANSImuz");
        assert_eq!(unknown_buttons[0].url, ANSIMUZ_GITHUB_URL);

        let too_long_url = format!("https://example.com/{}", "a".repeat(500));
        for bad_url in [
            "ftp://example.com/track",
            "https://example.com/with space",
            "https://",
            "/tmp/file.mp3",
            too_long_url.as_str(),
        ] {
            let info = PlaybackInfo::new("Track", "Artist")
                .with_provider("ytmusic")
                .with_public_track_url(bad_url);
            let activity = build_activity(&info);
            activity
                .validate()
                .expect("activity with invalid track URL omitted should validate");
            let buttons = activity.buttons.as_ref().expect("Get ANSImuz should exist");
            assert_eq!(buttons.len(), 1);
            assert_eq!(buttons[0].label, "Get ANSImuz");
        }
    }
}
