use presenceforge::{ActivityBuilder, ActivityType};
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
    let mut builder = ActivityBuilder::new().activity_type(ActivityType::Listening);

    // Discord max length: 128 characters
    // Details: track title
    let details = if info.title.trim().is_empty() {
        "Unknown Track".to_string()
    } else {
        truncate_activity_text(info.title.trim(), 128, 128)
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
    builder = builder.state(truncate_activity_text(&state_text, 128, 128));

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
        assert_eq!(activity.r#type, Some(ActivityType::Listening));

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
        let info = PlaybackInfo::new("Song", "Artist")
            .with_position(Duration::from_secs(45))
            .with_duration(Duration::from_secs(200))
            .with_paused(true);

        let activity = build_activity(&info);
        assert!(activity.validate().is_ok());
        assert_eq!(activity.r#type, Some(ActivityType::Listening));

        let assets = activity.assets.as_ref().expect("assets should exist");
        assert_eq!(assets.small_image.as_deref(), Some("pause"));
        assert_eq!(assets.small_text.as_deref(), Some("Paused"));

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
        if let Some(ts) = activity.timestamps {
            assert!(
                ts.end.is_none(),
                "Finished track must not advertise end timestamp"
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
        assert_eq!(activity.r#type, Some(ActivityType::Listening));
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
            let activity = ActivityBuilder::new()
                .activity_type(activity_type)
                .build();

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
        let activity: Activity =
            serde_json::from_str(r#"{"type": 2}"#).unwrap();

        assert_eq!(activity.r#type, Some(ActivityType::Listening));
    }
}
