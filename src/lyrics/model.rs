use crate::lyrics::types::{SyncType, ValidationError};
use serde::{Deserialize, Serialize};

/// An individual synchronized syllable within a word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LyricSyllable {
    pub text: String,
    #[serde(default)]
    pub start_ms: Option<u64>,
    #[serde(default)]
    pub end_ms: Option<u64>,
}

impl LyricSyllable {
    pub fn new(text: impl Into<String>, start_ms: Option<u64>, end_ms: Option<u64>) -> Self {
        Self {
            text: text.into(),
            start_ms,
            end_ms,
        }
    }
}

/// A single word within a lyric line, optionally containing syllable-level timing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LyricWord {
    pub text: String,
    #[serde(default)]
    pub start_ms: Option<u64>,
    #[serde(default)]
    pub end_ms: Option<u64>,
    #[serde(default)]
    pub syllables: Vec<LyricSyllable>,
}

impl LyricWord {
    pub fn new(text: impl Into<String>, start_ms: Option<u64>, end_ms: Option<u64>) -> Self {
        Self {
            text: text.into(),
            start_ms,
            end_ms,
            syllables: Vec::new(),
        }
    }

    pub fn with_syllables(
        text: impl Into<String>,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
        syllables: Vec<LyricSyllable>,
    ) -> Self {
        Self {
            text: text.into(),
            start_ms,
            end_ms,
            syllables,
        }
    }

    /// Returns the active syllable at the given millisecond position, if available.
    pub fn syllable_at_ms(&self, ms: u64) -> Option<(usize, &LyricSyllable)> {
        if self.syllables.is_empty() {
            return None;
        }

        for (idx, syl) in self.syllables.iter().enumerate() {
            if let Some(start) = syl.start_ms {
                let end = syl.end_ms.unwrap_or(u64::MAX);
                if ms >= start && ms < end {
                    return Some((idx, syl));
                }
            }
        }
        None
    }
}

/// A line of lyrics containing optional line timestamps and a list of words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LyricLine {
    #[serde(default)]
    pub start_ms: Option<u64>,
    #[serde(default)]
    pub end_ms: Option<u64>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub words: Vec<LyricWord>,
}

impl LyricLine {
    pub fn new(text: impl Into<String>, start_ms: Option<u64>, end_ms: Option<u64>) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.into(),
            words: Vec::new(),
        }
    }

    pub fn with_words(
        text: impl Into<String>,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
        words: Vec<LyricWord>,
    ) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.into(),
            words,
        }
    }

    /// Returns the textual representation of this line.
    pub fn display_text(&self) -> &str {
        &self.text
    }

    /// Finds the active word at a given timestamp in milliseconds.
    pub fn word_at_ms(&self, ms: u64) -> Option<(usize, &LyricWord)> {
        if self.words.is_empty() {
            return None;
        }

        for (idx, word) in self.words.iter().enumerate() {
            if let Some(start) = word.start_ms {
                let end = word.end_ms.unwrap_or(u64::MAX);
                if ms >= start && ms < end {
                    return Some((idx, word));
                }
            }
        }
        None
    }
}

/// Canonical representation of song lyrics in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Lyrics {
    #[serde(default, alias = "provider")]
    pub source: Option<String>,
    #[serde(default)]
    pub sync_type: SyncType,
    #[serde(default, alias = "track_title")]
    pub title: Option<String>,
    #[serde(default, alias = "artist_name")]
    pub artist: Option<String>,
    #[serde(default)]
    pub isrc: Option<String>,
    #[serde(default)]
    pub lines: Vec<LyricLine>,
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

impl Lyrics {
    /// Constructs an empty lyrics instance.
    pub fn empty() -> Self {
        Self {
            source: None,
            sync_type: SyncType::Unsynced,
            title: None,
            artist: None,
            isrc: None,
            lines: Vec::new(),
            unavailable_reason: None,
        }
    }

    /// Constructs an unavailable lyrics placeholder with a reason.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            source: None,
            sync_type: SyncType::Unsynced,
            title: None,
            artist: None,
            isrc: None,
            lines: Vec::new(),
            unavailable_reason: Some(reason.into()),
        }
    }

    /// Constructs unsynchronized lyrics from plain text lines.
    pub fn from_plain_text(
        title: Option<String>,
        artist: Option<String>,
        plain_text: &str,
    ) -> Self {
        let lines = plain_text
            .lines()
            .map(|raw| {
                let trimmed = raw.trim();
                let words = trimmed
                    .split_whitespace()
                    .map(|tok| LyricWord::new(tok, None, None))
                    .collect();
                LyricLine::with_words(trimmed, None, None, words)
            })
            .collect();

        Self {
            source: None,
            sync_type: SyncType::Unsynced,
            title,
            artist,
            isrc: None,
            lines,
            unavailable_reason: None,
        }
    }

    /// Returns true if this lyrics object contains no lines.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Returns true if lyrics are available (not marked unavailable and non-empty).
    #[inline]
    pub fn is_available(&self) -> bool {
        self.unavailable_reason.is_none() && !self.is_empty()
    }

    /// Returns true if the lyrics contain synchronized timing.
    #[inline]
    pub fn is_timed(&self) -> bool {
        self.sync_type.is_timed() && self.lines.iter().any(|l| l.start_ms.is_some())
    }

    /// Generates full plain text representation by joining line texts with newlines.
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.display_text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Finds the active line index and reference at a given millisecond timestamp.
    pub fn line_at_ms(&self, ms: u64) -> Option<(usize, &LyricLine)> {
        if self.lines.is_empty() {
            return None;
        }

        let mut candidate = None;
        for (idx, line) in self.lines.iter().enumerate() {
            if let Some(start) = line.start_ms {
                if start <= ms {
                    // Check if current line hasn't expired yet
                    if let Some(end) = line.end_ms
                        && ms < end
                    {
                        return Some((idx, line));
                    }
                    candidate = Some((idx, line));
                } else if candidate.is_some() {
                    // Passed beyond candidate line
                    break;
                }
            }
        }
        candidate
    }

    /// Validates timestamps throughout lines, words, and syllables for consistency.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut prev_line_start: Option<u64> = None;

        for (line_idx, line) in self.lines.iter().enumerate() {
            if let (Some(start), Some(end)) = (line.start_ms, line.end_ms)
                && start > end
            {
                return Err(ValidationError::InvalidLineRange {
                    line_index: line_idx,
                    start_ms: start,
                    end_ms: end,
                });
            }

            if let Some(curr_start) = line.start_ms {
                if let Some(prev_start) = prev_line_start
                    && curr_start < prev_start
                {
                    return Err(ValidationError::LinesOutOfOrder {
                        line_index: line_idx,
                        prev_start_ms: prev_start,
                        curr_start_ms: curr_start,
                    });
                }
                prev_line_start = Some(curr_start);
            }

            for (word_idx, word) in line.words.iter().enumerate() {
                if let (Some(w_start), Some(w_end)) = (word.start_ms, word.end_ms)
                    && w_start > w_end
                {
                    return Err(ValidationError::InvalidWordRange {
                        line_index: line_idx,
                        word_index: word_idx,
                        start_ms: w_start,
                        end_ms: w_end,
                    });
                }

                for (syl_idx, syl) in word.syllables.iter().enumerate() {
                    if let (Some(s_start), Some(s_end)) = (syl.start_ms, syl.end_ms)
                        && s_start > s_end
                    {
                        return Err(ValidationError::InvalidSyllableRange {
                            line_index: line_idx,
                            word_index: word_idx,
                            syllable_index: syl_idx,
                            start_ms: s_start,
                            end_ms: s_end,
                        });
                    }
                }
            }
        }

        Ok(())
    }
}