use crate::lyrics::types::{SyncType, ValidationError};
use serde::{Deserialize, Serialize};

/// Calculates normalized progression (0.0 to 1.0) of `ms` within `[start_ms, end_ms]`.
#[inline]
pub fn time_window_progress(ms: u64, start_ms: u64, end_ms: u64) -> f32 {
    if ms <= start_ms {
        0.0
    } else if ms >= end_ms {
        1.0
    } else {
        let span = (end_ms - start_ms).max(1);
        ((ms - start_ms) as f32 / span as f32).clamp(0.0, 1.0)
    }
}

/// Checks whether `ms` falls within the half-open interval `[start_ms, end_ms)`.
/// If `end_ms` is None, any timestamp `>= start_ms` is considered in-window.
#[inline]
pub fn is_in_time_window(ms: u64, start_ms: u64, end_ms: Option<u64>) -> bool {
    ms >= start_ms && end_ms.map(|end| ms < end).unwrap_or(true)
}

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

    /// Returns the effective end timestamp (in ms) of the syllable at `idx`.
    /// Uses explicit `end_ms` if present; otherwise falls back to the next syllable's `start_ms`,
    /// and finally to the word's own `end_ms`.
    pub fn effective_syllable_end(&self, idx: usize) -> Option<u64> {
        let syl = self.syllables.get(idx)?;
        if let Some(end) = syl.end_ms {
            return Some(end);
        }
        if let Some(next_start) = self.syllables.get(idx + 1).and_then(|s| s.start_ms) {
            return Some(next_start);
        }
        self.end_ms
    }

    /// Returns the active syllable index, syllable reference, and progression at `ms`.
    /// Uses centralized timing windows and progress calculation.
    pub fn active_syllable_at_ms(&self, ms: u64) -> Option<(usize, &LyricSyllable, f32)> {
        for (idx, syl) in self.syllables.iter().enumerate() {
            if let Some(start) = syl.start_ms {
                let end = self.effective_syllable_end(idx);
                if is_in_time_window(ms, start, end) {
                    let prog = match end {
                        Some(end_ms) => time_window_progress(ms, start, end_ms),
                        None => 0.0,
                    };
                    return Some((idx, syl, prog));
                }
            }
        }
        None
    }

    /// Returns the active syllable at the given millisecond position, if available.
    /// Uses centralized timing windows consistent with the synchronizer.
    pub fn syllable_at_ms(&self, ms: u64) -> Option<(usize, &LyricSyllable)> {
        self.active_syllable_at_ms(ms)
            .map(|(idx, syl, _)| (idx, syl))
    }
}

/// A line of lyrics containing optional line timestamps and a list of words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(from = "LyricLineRaw")]
pub struct LyricLine {
    #[serde(default)]
    pub start_ms: Option<u64>,
    #[serde(default)]
    pub end_ms: Option<u64>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub words: Vec<LyricWord>,
    /// True when this line contains background / harmony vocals.
    #[serde(default)]
    pub is_background: bool,
    /// Singer index: 0 = primary (left-aligned), 1 = secondary (right-aligned), 2+ reserved.
    #[serde(default)]
    pub singer: u8,
}

#[derive(Deserialize)]
struct LyricLineRaw {
    #[serde(default)]
    start_ms: Option<u64>,
    #[serde(default)]
    end_ms: Option<u64>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    words: Vec<LyricWord>,
    #[serde(default)]
    is_background: Option<bool>,
    #[serde(default)]
    vocal_type: Option<String>,
    #[serde(default)]
    background: Option<bool>,
    #[serde(default)]
    singer: Option<u8>,
    #[serde(default)]
    speaker: Option<u8>,
}

impl From<LyricLineRaw> for LyricLine {
    fn from(raw: LyricLineRaw) -> Self {
        let is_bg = raw.is_background.unwrap_or(false)
            || raw.background.unwrap_or(false)
            || raw
                .vocal_type
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("background") || s.eq_ignore_ascii_case("bg"))
                .unwrap_or(false);
        let singer = raw.singer.or(raw.speaker).unwrap_or(0);
        let line = LyricLine {
            start_ms: raw.start_ms,
            end_ms: raw.end_ms,
            text: raw.text,
            words: raw.words,
            is_background: is_bg,
            singer,
        };
        line.normalize()
    }
}

fn is_background_text(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('（') && t.ends_with('）'))
}

impl LyricLine {
    pub fn new(text: impl Into<String>, start_ms: Option<u64>, end_ms: Option<u64>) -> Self {
        let text_str = text.into();
        let is_bg = is_background_text(&text_str);
        Self {
            start_ms,
            end_ms,
            text: text_str,
            words: Vec::new(),
            is_background: is_bg,
            singer: 0,
        }
    }

    pub fn with_words(
        text: impl Into<String>,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
        words: Vec<LyricWord>,
    ) -> Self {
        let text_str = text.into();
        let is_bg = is_background_text(&text_str);
        Self {
            start_ms,
            end_ms,
            text: text_str,
            words,
            is_background: is_bg,
            singer: 0,
        }
    }

    pub fn with_background(mut self, is_background: bool) -> Self {
        self.is_background = is_background;
        self
    }

    pub fn with_singer(mut self, singer: u8) -> Self {
        self.singer = singer;
        self
    }

    /// Classify and normalize a raw line.
    /// Lines whose full text is wrapped in `(…)` or `（…）` are tagged as background vocals.
    pub fn normalize(mut self) -> Self {
        if !self.is_background && is_background_text(&self.text) {
            self.is_background = true;
        }
        self
    }

    /// Returns the textual representation of this line.
    pub fn display_text(&self) -> &str {
        &self.text
    }

    /// Returns the effective end timestamp (in ms) of the word at `idx`.
    /// Uses explicit `end_ms` if present; otherwise falls back to the next word's `start_ms`,
    /// and finally to the line's own `end_ms`.
    pub fn effective_word_end(&self, idx: usize) -> Option<u64> {
        let word = self.words.get(idx)?;
        if let Some(end) = word.end_ms {
            return Some(end);
        }
        if let Some(next_start) = self.words.get(idx + 1).and_then(|w| w.start_ms) {
            return Some(next_start);
        }
        self.end_ms
    }

    /// Returns the active word index, word reference, and progression at `ms`.
    /// Uses centralized timing windows and progress calculation.
    pub fn active_word_at_ms(&self, ms: u64) -> Option<(usize, &LyricWord, f32)> {
        for (idx, word) in self.words.iter().enumerate() {
            if let Some(start) = word.start_ms {
                let end = self.effective_word_end(idx);
                if is_in_time_window(ms, start, end) {
                    let prog = match end {
                        Some(end_ms) => time_window_progress(ms, start, end_ms),
                        None => 0.0,
                    };
                    return Some((idx, word, prog));
                }
            }
        }
        None
    }

    /// Finds the active word at a given timestamp in milliseconds.
    /// Uses centralized timing windows consistent with the synchronizer.
    pub fn word_at_ms(&self, ms: u64) -> Option<(usize, &LyricWord)> {
        self.active_word_at_ms(ms).map(|(idx, word, _)| (idx, word))
    }
}

/// Canonical representation of song lyrics in Rust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Lyrics {
    pub source: Option<String>,
    pub sync_type: SyncType,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub isrc: Option<String>,
    pub lines: Vec<LyricLine>,
    pub unavailable_reason: Option<String>,
}

#[derive(Deserialize)]
struct LyricsRaw {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    sync_type: SyncType,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    track_title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    artist_name: Option<String>,
    #[serde(default)]
    isrc: Option<String>,
    #[serde(default)]
    lines: Vec<LyricLine>,
    #[serde(default)]
    unavailable_reason: Option<String>,
}

impl<'de> Deserialize<'de> for Lyrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = LyricsRaw::deserialize(deserializer)?;
        let mut lyrics = Lyrics {
            source: raw.source.or(raw.provider),
            sync_type: raw.sync_type,
            title: raw.title.or(raw.track_title),
            artist: raw.artist.or(raw.artist_name),
            isrc: raw.isrc,
            lines: raw.lines,
            unavailable_reason: raw.unavailable_reason,
        };
        lyrics.normalize_lines();
        Ok(lyrics)
    }
}

impl Lyrics {
    /// Stably sorts lines in chronological order by `start_ms`.
    /// Timed lines are ordered ascending; untimed lines (`None`) are placed at the end.
    /// Lines with identical timestamps preserve their original relative order.
    pub fn normalize_lines(&mut self) {
        self.lines.sort_by(|a, b| match (a.start_ms, b.start_ms) {
            (Some(sa), Some(sb)) => sa.cmp(&sb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
    }

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

            if let Some(curr_start) = line.start_ms
                && !line.is_background
                && line.singer == 0
            {
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
