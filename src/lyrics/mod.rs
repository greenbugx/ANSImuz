pub mod model;
pub mod provider;
pub mod synchronizer;
pub mod types;
pub mod worker;

pub use model::{LyricLine, LyricSyllable, LyricWord, Lyrics};
pub use provider::{
    LyricsResponse, classify_lyrics_error, fetch_lyrics, spawn_fetch_lyrics, track_to_py_dict,
};
pub use synchronizer::{LyricsSyncPoint, LyricsSynchronizer};
pub use types::{LyricsError, LyricsStatus, SyncType, ValidationError};
pub use worker::{LyricsCommand, LyricsEvent, LyricsState, LyricsWorker};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_target_schema() {
        let json = r#"{
            "source": "lrc.red",
            "sync_type": "word",
            "title": "Example Song",
            "artist": "Example Artist",
            "isrc": "EXAMPLE12345",
            "lines": [
                {
                    "start_ms": 12000,
                    "end_ms": 15000,
                    "text": "Hello world",
                    "words": [
                        {
                            "text": "Hello",
                            "start_ms": 12000,
                            "end_ms": 12500,
                            "syllables": []
                        },
                        {
                            "text": "world",
                            "start_ms": 12600,
                            "end_ms": 14000,
                            "syllables": []
                        }
                    ]
                }
            ]
        }"#;

        let lyrics: Lyrics = serde_json::from_str(json).expect("Must deserialize target schema");
        assert_eq!(lyrics.source.as_deref(), Some("lrc.red"));
        assert_eq!(lyrics.sync_type, SyncType::Word);
        assert_eq!(lyrics.title.as_deref(), Some("Example Song"));
        assert_eq!(lyrics.artist.as_deref(), Some("Example Artist"));
        assert_eq!(lyrics.isrc.as_deref(), Some("EXAMPLE12345"));
        assert_eq!(lyrics.lines.len(), 1);

        let line = &lyrics.lines[0];
        assert_eq!(line.start_ms, Some(12000));
        assert_eq!(line.end_ms, Some(15000));
        assert_eq!(line.display_text(), "Hello world");
        assert_eq!(line.words.len(), 2);

        assert_eq!(line.words[0].text, "Hello");
        assert_eq!(line.words[0].start_ms, Some(12000));
        assert_eq!(line.words[0].end_ms, Some(12500));
        assert!(line.words[0].syllables.is_empty());

        assert_eq!(line.words[1].text, "world");
        assert_eq!(line.words[1].start_ms, Some(12600));
        assert_eq!(line.words[1].end_ms, Some(14000));
        assert!(line.words[1].syllables.is_empty());

        assert!(lyrics.validate().is_ok());
    }

    #[test]
    fn test_deserialize_syllable_level_timing() {
        let json = r#"{
            "source": "lrc.red",
            "sync_type": "syllable",
            "title": "Bohemian Rhapsody",
            "artist": "Queen",
            "isrc": "GBUM71029604",
            "lines": [
                {
                    "start_ms": 3725,
                    "end_ms": 6373,
                    "text": "Is this just fantasy?",
                    "words": [
                        { "text": "Is", "start_ms": 3725, "end_ms": 4140, "syllables": [] },
                        { "text": "this", "start_ms": 4140, "end_ms": 4526, "syllables": [] },
                        { "text": "just", "start_ms": 4526, "end_ms": 4891, "syllables": [] },
                        {
                            "text": "fantasy?",
                            "start_ms": 4891,
                            "end_ms": 6373,
                            "syllables": [
                                { "text": "fan", "start_ms": 4891, "end_ms": 5274 },
                                { "text": "ta", "start_ms": 5274, "end_ms": 5527 },
                                { "text": "sy?", "start_ms": 5527, "end_ms": 6373 }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let lyrics: Lyrics = serde_json::from_str(json).expect("Must deserialize syllable timing");
        assert_eq!(lyrics.sync_type, SyncType::Syllable);
        assert!(lyrics.is_timed());

        let line = &lyrics.lines[0];
        assert_eq!(line.words.len(), 4);
        let fantasy = &line.words[3];
        assert_eq!(fantasy.text, "fantasy?");
        assert_eq!(fantasy.syllables.len(), 3);
        assert_eq!(fantasy.syllables[0].text, "fan");
        assert_eq!(fantasy.syllables[0].start_ms, Some(4891));
        assert_eq!(fantasy.syllables[0].end_ms, Some(5274));

        // Syllable lookup
        let syl = fantasy.syllable_at_ms(5000);
        assert!(syl.is_some());
        assert_eq!(syl.unwrap().1.text, "fan");

        let syl2 = fantasy.syllable_at_ms(5400);
        assert!(syl2.is_some());
        assert_eq!(syl2.unwrap().1.text, "ta");

        assert!(lyrics.validate().is_ok());
    }

    #[test]
    fn test_deserialize_line_level_sync() {
        let json = r#"{
            "source": "lrc.red",
            "sync_type": "line",
            "title": "Creep",
            "artist": "Radiohead",
            "lines": [
                {
                    "start_ms": 19760,
                    "end_ms": 26500,
                    "text": "When you were here before",
                    "words": [
                        { "text": "When", "start_ms": null, "end_ms": null, "syllables": [] },
                        { "text": "you", "start_ms": null, "end_ms": null, "syllables": [] }
                    ]
                }
            ]
        }"#;

        let lyrics: Lyrics = serde_json::from_str(json).expect("Must deserialize line sync");
        assert_eq!(lyrics.sync_type, SyncType::Line);
        assert!(lyrics.is_timed());
        assert!(!lyrics.sync_type.has_word_timing());
        assert_eq!(lyrics.lines[0].start_ms, Some(19760));
        assert_eq!(lyrics.lines[0].end_ms, Some(26500));
        assert!(lyrics.validate().is_ok());
    }

    #[test]
    fn test_deserialize_unsynced_lyrics() {
        let json = r#"{
            "source": "lrc.red",
            "sync_type": "none",
            "title": "Plain Track",
            "lines": [
                {
                    "start_ms": null,
                    "end_ms": null,
                    "text": "First plain line",
                    "words": []
                },
                {
                    "start_ms": null,
                    "end_ms": null,
                    "text": "Second plain line",
                    "words": []
                }
            ]
        }"#;

        let lyrics: Lyrics = serde_json::from_str(json).expect("Must deserialize unsynced lyrics");
        assert_eq!(lyrics.sync_type, SyncType::Unsynced);
        assert!(!lyrics.is_timed());
        assert_eq!(lyrics.plain_text(), "First plain line\nSecond plain line");
        assert!(lyrics.validate().is_ok());
    }

    #[test]
    fn test_empty_and_unavailable_lyrics() {
        let empty = Lyrics::empty();
        assert!(empty.is_empty());
        assert!(!empty.is_available());
        assert_eq!(empty.sync_type, SyncType::Unsynced);

        let unavail = Lyrics::unavailable("Song is instrumental");
        assert!(unavail.is_empty());
        assert!(!unavail.is_available());
        assert_eq!(
            unavail.unavailable_reason.as_deref(),
            Some("Song is instrumental")
        );
    }

    #[test]
    fn test_from_plain_text_constructor() {
        let text = "Line one\nLine two";
        let lyrics =
            Lyrics::from_plain_text(Some("Song".to_string()), Some("Artist".to_string()), text);
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].text, "Line one");
        assert_eq!(lyrics.lines[0].words.len(), 2);
        assert_eq!(lyrics.lines[0].words[0].text, "Line");
        assert_eq!(lyrics.sync_type, SyncType::Unsynced);
        assert!(lyrics.is_available());
    }

    #[test]
    fn test_line_and_word_lookup_at_timestamp() {
        let mut line1 = LyricLine::new("Line 1", Some(1000), Some(3000));
        line1.words = vec![
            LyricWord::new("Word1", Some(1000), Some(2000)),
            LyricWord::new("Word2", Some(2000), Some(3000)),
        ];

        let line2 = LyricLine::new("Line 2", Some(3000), Some(5000));

        let lyrics = Lyrics {
            source: Some("test".to_string()),
            sync_type: SyncType::Word,
            title: Some("Title".to_string()),
            artist: Some("Artist".to_string()),
            isrc: None,
            lines: vec![line1, line2],
            unavailable_reason: None,
        };

        // Before playback starts
        assert!(lyrics.line_at_ms(500).is_none());

        // At 1500ms -> Line 1, Word 1
        let (idx, line) = lyrics.line_at_ms(1500).expect("Line 1 expected");
        assert_eq!(idx, 0);
        assert_eq!(line.text, "Line 1");
        let (w_idx, word) = line.word_at_ms(1500).expect("Word 1 expected");
        assert_eq!(w_idx, 0);
        assert_eq!(word.text, "Word1");

        // At 2500ms -> Line 1, Word 2
        let (_, word2) = line.word_at_ms(2500).expect("Word 2 expected");
        assert_eq!(word2.text, "Word2");

        // At 3500ms -> Line 2
        let (idx2, line_two) = lyrics.line_at_ms(3500).expect("Line 2 expected");
        assert_eq!(idx2, 1);
        assert_eq!(line_two.text, "Line 2");
    }

    #[test]
    fn test_validation_errors() {
        // Invalid line range
        let bad_line = Lyrics {
            lines: vec![LyricLine::new("Reversed", Some(5000), Some(2000))],
            ..Lyrics::empty()
        };
        match bad_line.validate() {
            Err(ValidationError::InvalidLineRange {
                line_index,
                start_ms,
                end_ms,
            }) => {
                assert_eq!(line_index, 0);
                assert_eq!(start_ms, 5000);
                assert_eq!(end_ms, 2000);
            }
            other => panic!("Expected InvalidLineRange, got {other:?}"),
        }

        // Invalid word range
        let bad_word = Lyrics {
            lines: vec![LyricLine::with_words(
                "Test",
                Some(1000),
                Some(5000),
                vec![LyricWord::new("Bad", Some(3000), Some(1500))],
            )],
            ..Lyrics::empty()
        };
        match bad_word.validate() {
            Err(ValidationError::InvalidWordRange {
                line_index,
                word_index,
                start_ms,
                end_ms,
            }) => {
                assert_eq!(line_index, 0);
                assert_eq!(word_index, 0);
                assert_eq!(start_ms, 3000);
                assert_eq!(end_ms, 1500);
            }
            other => panic!("Expected InvalidWordRange, got {other:?}"),
        }

        // Invalid syllable range
        let bad_syl = Lyrics {
            lines: vec![LyricLine::with_words(
                "Test",
                Some(1000),
                Some(5000),
                vec![LyricWord::with_syllables(
                    "Word",
                    Some(1000),
                    Some(4000),
                    vec![LyricSyllable::new("Syl", Some(2500), Some(1200))],
                )],
            )],
            ..Lyrics::empty()
        };
        match bad_syl.validate() {
            Err(ValidationError::InvalidSyllableRange {
                line_index,
                word_index,
                syllable_index,
                start_ms,
                end_ms,
            }) => {
                assert_eq!(line_index, 0);
                assert_eq!(word_index, 0);
                assert_eq!(syllable_index, 0);
                assert_eq!(start_ms, 2500);
                assert_eq!(end_ms, 1200);
            }
            other => panic!("Expected InvalidSyllableRange, got {other:?}"),
        }

        // Lines out of order
        let out_of_order = Lyrics {
            lines: vec![
                LyricLine::new("Line 1", Some(4000), Some(6000)),
                LyricLine::new("Line 2", Some(2000), Some(3000)),
            ],
            ..Lyrics::empty()
        };
        match out_of_order.validate() {
            Err(ValidationError::LinesOutOfOrder {
                line_index,
                prev_start_ms,
                curr_start_ms,
            }) => {
                assert_eq!(line_index, 1);
                assert_eq!(prev_start_ms, 4000);
                assert_eq!(curr_start_ms, 2000);
            }
            other => panic!("Expected LinesOutOfOrder, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_lyrics_error() {
        assert_eq!(
            classify_lyrics_error("Rate limit exceeded: HTTP 429 quota"),
            LyricsError::RateLimit("Rate limit exceeded: HTTP 429 quota".to_string())
        );
        assert_eq!(
            classify_lyrics_error("Network connection error: timed out"),
            LyricsError::Network("Network connection error: timed out".to_string())
        );
        assert_eq!(
            classify_lyrics_error("Track not found on provider."),
            LyricsError::NotFound("Track not found on provider.".to_string())
        );
        assert_eq!(
            classify_lyrics_error("Resource 404"),
            LyricsError::NotFound("Resource 404".to_string())
        );
        assert_eq!(
            classify_lyrics_error("Unexpected syntax error in script"),
            LyricsError::Backend("Unexpected syntax error in script".to_string())
        );
    }

    #[test]
    fn test_lyrics_response_generation_and_track_validation() {
        use crate::music::track::Track;

        let track1 = Track::new("Creep", "Radiohead")
            .with_album("Pablo Honey")
            .with_duration_ms(238000);
        let track2 = Track::new("Karma Police", "Radiohead");

        let response = LyricsResponse {
            generation: 42,
            track_title: "Creep".to_string(),
            artist: "Radiohead".to_string(),
            result: Ok(Some(Lyrics::from_plain_text(
                Some("Creep".to_string()),
                Some("Radiohead".to_string()),
                "I'm a creep",
            ))),
        };

        // Matching generation and track identity
        assert!(response.is_valid_for(42, &track1));
        assert!(!response.should_reject(42, Some(&track1)));

        // Case-insensitive and trimmed match
        let response_fuzzy = LyricsResponse {
            generation: 42,
            track_title: "  creep  ".to_string(),
            artist: "RADIOHEAD ".to_string(),
            result: Ok(None),
        };
        assert!(response_fuzzy.is_valid_for(42, &track1));
        assert!(!response_fuzzy.should_reject(42, Some(&track1)));

        // Stale generation must be rejected
        assert!(!response.is_valid_for(41, &track1));
        assert!(response.should_reject(41, Some(&track1)));
        assert!(!response.is_valid_for(43, &track1));
        assert!(response.should_reject(43, Some(&track1)));

        // Changed track title or artist must be rejected
        assert!(!response.is_valid_for(42, &track2));
        assert!(response.should_reject(42, Some(&track2)));

        // Stopped playback (no current track) must be rejected
        assert!(response.should_reject(42, None));
    }

    #[test]
    fn test_track_to_py_dict_conversion() {
        use crate::music::track::Track;
        use pyo3::prelude::*;
        use pyo3::types::PyDictMethods;

        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        Python::attach(|py| {
            let track = Track::new("Creep", "Radiohead")
                .with_album("Pablo Honey")
                .with_duration_ms(238000)
                .with_spotify_id("spotify:track:123")
                .with_stream_url("https://example.com/audio.mp3")
                .with_art_url("https://example.com/art.jpg")
                .with_artists(vec!["Radiohead".to_string(), "Thom Yorke".to_string()]);

            let dict = track_to_py_dict(py, &track).expect("track_to_py_dict succeeds");
            assert_eq!(
                dict.get_item("title")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "Creep"
            );
            assert_eq!(
                dict.get_item("artist")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "Radiohead"
            );
            assert_eq!(
                dict.get_item("album")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "Pablo Honey"
            );
            assert_eq!(
                dict.get_item("duration_ms")
                    .unwrap()
                    .unwrap()
                    .extract::<u64>()
                    .unwrap(),
                238000
            );
            assert_eq!(
                dict.get_item("spotify_id")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "spotify:track:123"
            );
            assert_eq!(
                dict.get_item("stream_url")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "https://example.com/audio.mp3"
            );
            assert_eq!(
                dict.get_item("art_url")
                    .unwrap()
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                "https://example.com/art.jpg"
            );
            let artists = dict
                .get_item("artists")
                .unwrap()
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap();
            assert_eq!(artists, vec!["Radiohead", "Thom Yorke"]);
        });
    }

    #[test]
    fn test_spawn_fetch_lyrics_channel_delivery() {
        use crate::music::track::Track;
        use std::sync::mpsc;
        use std::time::Duration;

        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let track = Track::new("NonExistentTrack_XYZ_12345", "UnknownArtist_99999");

        let handle = spawn_fetch_lyrics(track, 7, tx);
        let resp = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("Must receive LyricsResponse on channel");

        assert_eq!(resp.generation, 7);
        assert_eq!(resp.track_title, "NonExistentTrack_XYZ_12345");
        assert_eq!(resp.artist, "UnknownArtist_99999");
        // Unknown track without internet or not on lrc.red should resolve to Ok(None) or Err(Network/NotFound)
        match resp.result {
            Ok(lyrics) => assert!(lyrics.is_none()),
            Err(e) => assert!(matches!(
                e,
                LyricsError::NotFound(_) | LyricsError::Network(_) | LyricsError::Backend(_)
            )),
        }

        let _ = handle.join();
    }
}
