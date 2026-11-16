use serde::{Deserialize, Serialize};
use std::fmt;

/// The precision of synchronization for the lyrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    /// Unsynchronized lyrics (plain text without timestamps).
    #[default]
    #[serde(alias = "none")]
    Unsynced,
    /// Line-level synchronized lyrics (each line has a timestamp).
    Line,
    /// Word-level synchronized lyrics (each word within a line has timing).
    Word,
    /// Syllable-level synchronized lyrics (individual syllables are timed).
    Syllable,
}

impl SyncType {
    /// Returns true if this sync type contains timing information.
    #[inline]
    pub fn is_timed(&self) -> bool {
        matches!(self, Self::Line | Self::Word | Self::Syllable)
    }

    /// Returns true if this sync type contains word- or syllable-level timing.
    #[inline]
    pub fn has_word_timing(&self) -> bool {
        matches!(self, Self::Word | Self::Syllable)
    }

    /// Returns the canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsynced => "unsynced",
            Self::Line => "line",
            Self::Word => "word",
            Self::Syllable => "syllable",
        }
    }
}

impl fmt::Display for SyncType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Errors detected when validating timestamp consistency in a lyrics model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// A line has a start_ms greater than its end_ms.
    InvalidLineRange {
        line_index: usize,
        start_ms: u64,
        end_ms: u64,
    },
    /// A word has a start_ms greater than its end_ms.
    InvalidWordRange {
        line_index: usize,
        word_index: usize,
        start_ms: u64,
        end_ms: u64,
    },
    /// A syllable has a start_ms greater than its end_ms.
    InvalidSyllableRange {
        line_index: usize,
        word_index: usize,
        syllable_index: usize,
        start_ms: u64,
        end_ms: u64,
    },
    /// Consecutive lines have non-monotonic start timestamps.
    LinesOutOfOrder {
        line_index: usize,
        prev_start_ms: u64,
        curr_start_ms: u64,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLineRange {
                line_index,
                start_ms,
                end_ms,
            } => {
                write!(
                    f,
                    "Line {line_index} has invalid range: start_ms ({start_ms}) > end_ms ({end_ms})"
                )
            }
            Self::InvalidWordRange {
                line_index,
                word_index,
                start_ms,
                end_ms,
            } => {
                write!(
                    f,
                    "Line {line_index}, word {word_index} has invalid range: start_ms ({start_ms}) > end_ms ({end_ms})"
                )
            }
            Self::InvalidSyllableRange {
                line_index,
                word_index,
                syllable_index,
                start_ms,
                end_ms,
            } => {
                write!(
                    f,
                    "Line {line_index}, word {word_index}, syllable {syllable_index} has invalid range: start_ms ({start_ms}) > end_ms ({end_ms})"
                )
            }
            Self::LinesOutOfOrder {
                line_index,
                prev_start_ms,
                curr_start_ms,
            } => {
                write!(
                    f,
                    "Line {line_index} start_ms ({curr_start_ms}) is earlier than previous line start_ms ({prev_start_ms})"
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

