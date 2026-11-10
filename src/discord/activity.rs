use presenceforge::ActivityBuilder;
use presenceforge::activity::Activity;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_LARGE_IMAGE: &str = "logo";
pub const DEFAULT_LARGE_TEXT: &str = "ANSImuz";

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackInfo {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub is_paused: bool,
    pub stream_url: Option<String>,
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
            stream_url: None,
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

    pub fn with_stream_url(mut self, url: impl Into<String>) -> Self {
        self.stream_url = Some(url.into());
        self
    }
}

fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    const ELLIPSIS: &str = "…";
    let target = max_bytes.saturating_sub(ELLIPSIS.len());
    let mut end = 0;
    for (idx, _) in s.char_indices() {
        if idx <= target {
            end = idx;
        } else {
            break;
        }
    }
    let mut result = s[..end].to_string();
    result.push_str(ELLIPSIS);
    result
}

pub fn build_activity(info: &PlaybackInfo) -> Activity {
    let mut builder = ActivityBuilder::new();

    // Discord max length: 128 characters
    // Details: track title
    let details = if info.title.trim().is_empty() {
        "Unknown Track".to_string()
    } else {
        truncate_bytes(info.title.trim(), 128)
    };
    builder = builder.details(details);

    // State: artist or artist • album
    let state_text = match &info.album {
        Some(album) if !album.trim().is_empty() => {
            format!("{} • {}", info.artist.trim(), album.trim())
        }
        _ if !info.artist.trim().is_empty() => info.artist.trim().to_string(),
        _ => "Unknown Artist".to_string(),
    };
    builder = builder.state(truncate_bytes(&state_text, 128));

    // Assets: ANSImuz logo and playback status indicator
    builder = builder
        .large_image(DEFAULT_LARGE_IMAGE)
        .large_text(DEFAULT_LARGE_TEXT);

    if info.is_paused {
        builder = builder.small_image("pause").small_text("Paused");
    } else {
        builder = builder.small_image("play").small_text("Playing");

        // Timestamps
        // applied only when actively playing
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

        if let Some(dur) = info.duration
            && dur > Duration::ZERO
        {
            let end_ts = start_ts + dur.as_secs();
            if end_ts > now {
                builder = builder.end_timestamp(end_ts as i64);
            }
        }
    }

    // Optional stream button if a valid web link is provided
    if let Some(url) = &info.stream_url {
        let trimmed = url.trim();
        if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && trimmed.len() <= 512
        {
            builder = builder.button("Listen", trimmed);
        }
    }

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
            .with_stream_url("https://music.youtube.com/watch?v=mock");

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());

        assert_eq!(activity.details.as_deref(), Some("Bohemian Rhapsody"));
        assert_eq!(
            activity.state.as_deref(),
            Some("Queen • A Night at the Opera")
        );
        let assets = activity.assets.as_ref().expect("assets should exist");
        assert_eq!(assets.large_image.as_deref(), Some(DEFAULT_LARGE_IMAGE));
        assert_eq!(assets.small_image.as_deref(), Some("play"));

        let timestamps = activity
            .timestamps
            .as_ref()
            .expect("timestamps should exist");
        assert!(timestamps.start.is_some());
        assert!(timestamps.end.is_some());

        let buttons = activity.buttons.as_ref().expect("buttons should exist");
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].label, "Listen");
    }

    #[test]
    fn test_build_activity_paused() {
        let info = PlaybackInfo::new("Song", "Artist").with_paused(true);

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());

        let assets = activity.assets.as_ref().expect("assets should exist");
        assert_eq!(assets.small_image.as_deref(), Some("pause"));
        assert_eq!(assets.small_text.as_deref(), Some("Paused"));

        // When paused, end timestamp should not be set
        if let Some(ts) = activity.timestamps.as_ref() {
            assert!(ts.end.is_none());
        }
    }

    #[test]
    fn test_build_activity_truncation() {
        let very_long_title = "a".repeat(200);
        let very_long_artist = "b".repeat(200);
        let info = PlaybackInfo::new(very_long_title, very_long_artist);

        let activity = build_activity(&info);

        assert!(activity.details.as_ref().unwrap().len() <= 128);
        assert!(activity.state.as_ref().unwrap().len() <= 128);
        assert!(activity.validate().is_ok());
    }
}
