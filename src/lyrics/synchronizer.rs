use std::time::Duration;

use crate::lyrics::model::{LyricLine, LyricSyllable, LyricWord, Lyrics};
use crate::playback::PlayerEvent;

/// Detailed synchronization point in time for lyrics rendering.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LyricsSyncPoint {
    /// Primary (lead vocal) active line index within `lyrics.lines`, if any line is current.
    pub active_line_index: Option<usize>,
    /// All simultaneously active line indices (lead + any overlapping background lines).
    /// Always contains `active_line_index` as the first element when it is `Some`.
    pub active_line_indices: Vec<usize>,
    /// Active word index within the active line, if word-level timing is available.
    pub active_word_index: Option<usize>,
    /// Active syllable index within the active word, if syllable-level timing is available.
    pub active_syllable_index: Option<usize>,
    /// Progress through the active line (0.0 to 1.0).
    pub line_progress: f32,
    /// Progress through the active word (0.0 to 1.0).
    pub word_progress: f32,
    /// Progress through the active syllable (0.0 to 1.0).
    pub syllable_progress: f32,
    /// True if playback position falls in a timestamp gap (intro, outro, or interlude between lines).
    pub is_in_gap: bool,
    /// Playback position in milliseconds.
    pub position_ms: u64,
    /// Total duration in milliseconds, if known.
    pub duration_ms: Option<u64>,
    /// Playback generation this sync point corresponds to.
    pub generation: u64,
    /// True if playback has ended or reached track completion.
    pub is_ended: bool,
    /// True if playback is currently paused.
    pub is_paused: bool,
}

impl LyricsSyncPoint {
    /// Returns true if this sync point has an active line.
    #[inline]
    pub fn has_active_line(&self) -> bool {
        self.active_line_index.is_some() || !self.active_line_indices.is_empty()
    }

    /// Retrieves a reference to the active line from the provided lyrics.
    #[inline]
    pub fn active_line<'a>(&self, lyrics: &'a Lyrics) -> Option<&'a LyricLine> {
        self.active_line_index.and_then(|idx| lyrics.lines.get(idx))
    }

    /// Retrieves references to all currently active lines from the provided lyrics.
    #[inline]
    pub fn active_lines<'a>(&self, lyrics: &'a Lyrics) -> Vec<&'a LyricLine> {
        self.active_line_indices
            .iter()
            .filter_map(|&idx| lyrics.lines.get(idx))
            .collect()
    }

    /// Retrieves a reference to the active word from the provided lyrics.
    #[inline]
    pub fn active_word<'a>(&self, lyrics: &'a Lyrics) -> Option<&'a LyricWord> {
        let line = self.active_line(lyrics)?;
        self.active_word_index.and_then(|idx| line.words.get(idx))
    }

    /// Retrieves a reference to the active syllable from the provided lyrics.
    #[inline]
    pub fn active_syllable<'a>(&self, lyrics: &'a Lyrics) -> Option<&'a LyricSyllable> {
        let word = self.active_word(lyrics)?;
        self.active_syllable_index
            .and_then(|idx| word.syllables.get(idx))
    }
}

/// State-preserving lyrics synchronizer driven entirely by MPV playback position.
///
/// Keeps lyrics synchronized across seeking (forward and backward), pause/resume,
/// track completion, and repeat-one replay while avoiding redundant calculations.
#[derive(Debug, Clone, Default)]
pub struct LyricsSynchronizer {
    /// Cached last computed sync point.
    last_sync_point: LyricsSyncPoint,
    /// Last elapsed position in seconds reported by MPV.
    last_elapsed_sec: Option<f64>,
    /// Last total duration in seconds reported by MPV.
    last_duration_sec: Option<f64>,
    /// Last playback generation seen.
    last_generation: u64,
    /// Whether playback is currently paused.
    is_paused: bool,
    /// Whether track has completed.
    is_ended: bool,
}

impl LyricsSynchronizer {
    /// Creates a new, uninitialized lyrics synchronizer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronizes lyrics against an incoming `PlayerEvent`.
    pub fn sync_event(&mut self, event: &PlayerEvent, lyrics: &Lyrics) -> &LyricsSyncPoint {
        match event {
            PlayerEvent::Position {
                generation,
                elapsed,
                duration,
            } => self.sync_position(*generation, *elapsed, *duration, lyrics),
            PlayerEvent::Ended { generation } => self.sync_ended(*generation, lyrics),
            PlayerEvent::Failed { generation, .. } => self.sync_ended(*generation, lyrics),
        }
    }

    /// Synchronizes lyrics against explicit MPV seconds and duration.
    pub fn sync_position(
        &mut self,
        generation: u64,
        elapsed_sec: f64,
        duration_sec: Option<f64>,
        lyrics: &Lyrics,
    ) -> &LyricsSyncPoint {
        let position_ms = (elapsed_sec.max(0.0) * 1000.0).round() as u64;
        let duration_ms = duration_sec
            .filter(|&d| d > 0.0)
            .map(|d| (d * 1000.0).round() as u64);

        // Detect repeat-one or rewind (elapsed jumped backwards) or generation change
        let generation_changed = generation != self.last_generation;
        let rewound = self
            .last_elapsed_sec
            .map(|last| elapsed_sec < last)
            .unwrap_or(false);

        // Avoid unnecessary calculations if position, generation, and state are unchanged
        if !generation_changed
            && !rewound
            && !self.is_ended
            && self.last_sync_point.position_ms == position_ms
            && self.last_sync_point.duration_ms == duration_ms
            && self.last_sync_point.is_paused == self.is_paused
        {
            return &self.last_sync_point;
        }

        self.last_generation = generation;
        self.last_elapsed_sec = Some(elapsed_sec);
        self.last_duration_sec = duration_sec;
        self.is_ended = false;

        let hint_index = if generation_changed || rewound {
            None
        } else {
            self.last_sync_point.active_line_index
        };

        self.last_sync_point = compute_sync_point(
            position_ms,
            duration_ms,
            generation,
            self.is_paused,
            false,
            lyrics,
            hint_index,
        );

        &self.last_sync_point
    }

    /// Synchronizes lyrics using standard `Option<Duration>` timestamps.
    pub fn sync_duration(
        &mut self,
        generation: u64,
        position: Option<Duration>,
        duration: Option<Duration>,
        lyrics: &Lyrics,
    ) -> &LyricsSyncPoint {
        match position {
            Some(pos) => {
                let elapsed_sec = pos.as_secs_f64();
                let duration_sec = duration.map(|d| d.as_secs_f64());
                self.sync_position(generation, elapsed_sec, duration_sec, lyrics)
            }
            None => self.sync_ended(generation, lyrics),
        }
    }

    /// Marks the track as completed.
    pub fn sync_ended(&mut self, generation: u64, lyrics: &Lyrics) -> &LyricsSyncPoint {
        self.last_generation = generation;
        self.is_ended = true;

        let duration_ms = self.last_duration_sec.map(|d| (d * 1000.0).round() as u64);
        let final_pos_ms = duration_ms.unwrap_or(self.last_sync_point.position_ms);

        self.last_sync_point = compute_sync_point(
            final_pos_ms,
            duration_ms,
            generation,
            self.is_paused,
            true,
            lyrics,
            Some(lyrics.lines.len().saturating_sub(1)),
        );

        &self.last_sync_point
    }

    /// Updates pause state without altering playback position.
    pub fn set_paused(&mut self, paused: bool) -> &LyricsSyncPoint {
        if self.is_paused != paused {
            self.is_paused = paused;
            self.last_sync_point.is_paused = paused;
        }
        &self.last_sync_point
    }

    /// Returns the most recently computed sync point.
    pub fn current_sync_point(&self) -> &LyricsSyncPoint {
        &self.last_sync_point
    }

    /// Clears all cached state.
    pub fn reset(&mut self) {
        self.last_sync_point = LyricsSyncPoint::default();
        self.last_elapsed_sec = None;
        self.last_duration_sec = None;
        self.last_generation = 0;
        self.is_paused = false;
        self.is_ended = false;
    }
}

/// Pure calculation of a `LyricsSyncPoint` given current playback position.
pub fn compute_sync_point(
    position_ms: u64,
    duration_ms: Option<u64>,
    generation: u64,
    is_paused: bool,
    is_ended: bool,
    lyrics: &Lyrics,
    hint_index: Option<usize>,
) -> LyricsSyncPoint {
    if lyrics.is_empty() || !lyrics.is_timed() || is_ended {
        let (active_line, progress) = if is_ended && !lyrics.is_empty() {
            (Some(lyrics.lines.len().saturating_sub(1)), 1.0)
        } else {
            (None, 0.0)
        };

        return LyricsSyncPoint {
            active_line_index: active_line,
            active_line_indices: active_line.into_iter().collect(),
            active_word_index: None,
            active_syllable_index: None,
            line_progress: progress,
            word_progress: progress,
            syllable_progress: progress,
            is_in_gap: is_ended,
            position_ms,
            duration_ms,
            generation,
            is_ended,
            is_paused,
        };
    }

    // Collect all concurrently active lines whose [start_ms, end_ms] window contains position_ms
    let all_active = find_all_active_lines(position_ms, &lyrics.lines, duration_ms);

    let (line_idx_opt, in_gap, line_progress, active_line_indices) = if !all_active.is_empty() {
        // Prefer the active lead vocal (non-background, primary singer) as primary scroll anchor
        let primary_idx = all_active
            .iter()
            .copied()
            .find(|&idx| !lyrics.lines[idx].is_background && lyrics.lines[idx].singer == 0)
            .or_else(|| {
                all_active
                    .iter()
                    .copied()
                    .find(|&idx| !lyrics.lines[idx].is_background)
            })
            .unwrap_or(all_active[0]);

        let line = &lyrics.lines[primary_idx];
        let start = line.start_ms.unwrap_or(0);
        let next_start = lyrics.lines.get(primary_idx + 1).and_then(|l| l.start_ms);
        let end = line
            .end_ms
            .or(next_start)
            .or(duration_ms)
            .unwrap_or(start + 5000);
        let prog = ((position_ms - start) as f32 / (end - start).max(1) as f32).clamp(0.0, 1.0);

        let mut indices = all_active;
        if let Some(pos) = indices.iter().position(|&i| i == primary_idx)
            && pos != 0
        {
            indices.swap(0, pos);
        }
        (Some(primary_idx), false, prog, indices)
    } else {
        // When in a timestamp gap, fallback to find_active_line to preserve previous line index
        let (fallback_idx, in_gap, prog) =
            find_active_line(position_ms, &lyrics.lines, hint_index, duration_ms);
        let indices = fallback_idx.into_iter().collect();
        (fallback_idx, in_gap, prog, indices)
    };

    // Identify active word, syllable, and progress within active line
    let (word_idx_opt, word_prog, syl_idx_opt, syl_prog) = match (line_idx_opt, in_gap) {
        (Some(idx), false) => {
            let line = &lyrics.lines[idx];
            find_active_word_and_syllable(position_ms, line, line_progress)
        }
        (Some(_), true) => (None, 1.0, None, 1.0),
        (None, _) => (None, 0.0, None, 0.0),
    };

    LyricsSyncPoint {
        active_line_index: line_idx_opt,
        active_line_indices,
        active_word_index: word_idx_opt,
        active_syllable_index: syl_idx_opt,
        line_progress,
        word_progress: word_prog,
        syllable_progress: syl_prog,
        is_in_gap: in_gap,
        position_ms,
        duration_ms,
        generation,
        is_ended,
        is_paused,
    }
}

/// Returns every line index whose `[start_ms, end_ms]` window contains `ms`.
fn find_all_active_lines(ms: u64, lines: &[LyricLine], duration_ms: Option<u64>) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(i, line)| {
            if let Some(start) = line.start_ms {
                let next_start = if !line.is_background {
                    lines
                        .iter()
                        .skip(i + 1)
                        .find(|l| !l.is_background && l.singer == line.singer)
                        .and_then(|l| l.start_ms)
                        .or_else(|| lines.get(i + 1).and_then(|l| l.start_ms))
                } else {
                    lines.get(i + 1).and_then(|l| l.start_ms)
                };
                let end = line
                    .end_ms
                    .or(next_start)
                    .or(duration_ms)
                    .unwrap_or(start + 5000);
                ms >= start && ms < end
            } else {
                false
            }
        })
        .map(|(i, _)| i)
        .collect()
}

/// Identifies the active line, whether the timestamp falls into a gap, and line progression.
fn find_active_line(
    ms: u64,
    lines: &[LyricLine],
    hint_index: Option<usize>,
    duration_ms: Option<u64>,
) -> (Option<usize>, bool, f32) {
    if lines.is_empty() {
        return (None, false, 0.0);
    }

    // Intro gap check: before the first timed line
    if let Some(first_start) = lines[0].start_ms
        && ms < first_start
    {
        return (None, true, 0.0);
    }

    // Fast-path O(1) verification of current or next line
    if let Some(hint) = hint_index
        && hint < lines.len()
    {
        let curr = &lines[hint];
        let curr_start = curr.start_ms.unwrap_or(0);
        let next_start = lines.get(hint + 1).and_then(|l| l.start_ms);
        let curr_end = curr.end_ms.or(next_start);

        if ms >= curr_start {
            if let Some(end_val) = curr_end {
                if ms < end_val {
                    let prog = ((ms - curr_start) as f32 / (end_val - curr_start).max(1) as f32)
                        .clamp(0.0, 1.0);
                    return (Some(hint), false, prog);
                }
                // Check if in gap between current line and next line
                if let Some(nxt) = next_start
                    && ms < nxt
                {
                    return (Some(hint), true, 1.0);
                }
            }
            // Check immediate next line
            if hint + 1 < lines.len() {
                let next_idx = hint + 1;
                let next_line = &lines[next_idx];
                let n_start = next_line.start_ms.unwrap_or(0);
                let n_end = next_line
                    .end_ms
                    .or_else(|| lines.get(next_idx + 1).and_then(|l| l.start_ms));

                if ms >= n_start {
                    if let Some(e) = n_end {
                        if ms < e {
                            let prog = ((ms - n_start) as f32 / (e - n_start).max(1) as f32)
                                .clamp(0.0, 1.0);
                            return (Some(next_idx), false, prog);
                        }
                    } else {
                        return (Some(next_idx), false, 0.0);
                    }
                }
            }
        }
    }

    // Fallback search via binary search for seeks / jumps
    let found_idx = match lines.binary_search_by(|line| match line.start_ms {
        Some(s) => s.cmp(&ms),
        None => std::cmp::Ordering::Less,
    }) {
        Ok(exact) => exact,
        Err(insert) => {
            if insert == 0 {
                return (None, true, 0.0);
            }
            insert - 1
        }
    };

    let line = &lines[found_idx];
    let start = line.start_ms.unwrap_or(0);
    let next_start = lines.get(found_idx + 1).and_then(|l| l.start_ms);
    let effective_end = line
        .end_ms
        .or(next_start)
        .or(duration_ms)
        .unwrap_or(start + 5000);

    if ms < effective_end {
        let prog = ((ms - start) as f32 / (effective_end - start).max(1) as f32).clamp(0.0, 1.0);
        (Some(found_idx), false, prog)
    } else {
        // Line has finished
        if let Some(nxt) = next_start {
            if ms < nxt {
                // Interlude gap before next line
                (Some(found_idx), true, 1.0)
            } else {
                (Some(found_idx + 1), false, 0.0)
            }
        } else {
            // Outro gap after the final line
            (Some(found_idx), true, 1.0)
        }
    }
}

/// Identifies active word, active syllable, and calculates word and syllable progression.
fn find_active_word_and_syllable(
    ms: u64,
    line: &LyricLine,
    line_progress: f32,
) -> (Option<usize>, f32, Option<usize>, f32) {
    if line.words.is_empty() {
        return (None, line_progress, None, line_progress);
    }

    // Find the word active at ms
    for (w_idx, word) in line.words.iter().enumerate() {
        if let Some(w_start) = word.start_ms {
            let next_word_start = line.words.get(w_idx + 1).and_then(|w| w.start_ms);
            let w_end = word.end_ms.or(next_word_start);

            if ms >= w_start {
                if let Some(end_val) = w_end {
                    if ms < end_val {
                        // Word is active!
                        let word_prog = ((ms - w_start) as f32 / (end_val - w_start).max(1) as f32)
                            .clamp(0.0, 1.0);

                        // Find active syllable within this word
                        let (syl_idx, syl_prog) = find_active_syllable(ms, word, word_prog);
                        return (Some(w_idx), word_prog, syl_idx, syl_prog);
                    }
                } else {
                    return (Some(w_idx), 0.0, None, 0.0);
                }
            }
        }
    }

    (None, line_progress, None, line_progress)
}

/// Finds the active syllable within a word and calculates syllable progression.
fn find_active_syllable(ms: u64, word: &LyricWord, fallback_prog: f32) -> (Option<usize>, f32) {
    if word.syllables.is_empty() {
        return (None, fallback_prog);
    }

    for (s_idx, syl) in word.syllables.iter().enumerate() {
        if let Some(s_start) = syl.start_ms {
            let next_syl_start = word.syllables.get(s_idx + 1).and_then(|s| s.start_ms);
            let s_end = syl.end_ms.or(next_syl_start);

            if ms >= s_start {
                if let Some(end_val) = s_end {
                    if ms < end_val {
                        let syl_prog = ((ms - s_start) as f32 / (end_val - s_start).max(1) as f32)
                            .clamp(0.0, 1.0);
                        return (Some(s_idx), syl_prog);
                    }
                } else {
                    return (Some(s_idx), 0.0);
                }
            }
        }
    }

    (None, fallback_prog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::types::SyncType;

    fn build_test_lyrics() -> Lyrics {
        Lyrics {
            source: Some("lrc.red".to_string()),
            sync_type: SyncType::Word,
            title: Some("Creep".to_string()),
            artist: Some("Radiohead".to_string()),
            isrc: None,
            lines: vec![
                LyricLine::with_words(
                    "When you were here before",
                    Some(10000),
                    Some(14000),
                    vec![
                        LyricWord::new("When", Some(10000), Some(10500)),
                        LyricWord::new("you", Some(10500), Some(11000)),
                        LyricWord::new("were", Some(11000), Some(11500)),
                        LyricWord::new("here", Some(11500), Some(12500)),
                        LyricWord::new("before", Some(12500), Some(14000)),
                    ],
                ),
                // 2 second gap between lines: 14000 to 16000
                LyricLine::with_words(
                    "Couldn't look you in the eye",
                    Some(16000),
                    Some(20000),
                    vec![
                        LyricWord::with_syllables(
                            "Couldn't",
                            Some(16000),
                            Some(17000),
                            vec![
                                LyricSyllable::new("Could", Some(16000), Some(16500)),
                                LyricSyllable::new("n't", Some(16500), Some(17000)),
                            ],
                        ),
                        LyricWord::new("look", Some(17000), Some(17800)),
                        LyricWord::new("you", Some(17800), Some(18400)),
                        LyricWord::new("in", Some(18400), Some(19000)),
                        LyricWord::new("the", Some(19000), Some(19400)),
                        LyricWord::new("eye", Some(19400), Some(20000)),
                    ],
                ),
            ],
            unavailable_reason: None,
        }
    }

    #[test]
    fn test_sync_intro_gap_before_first_line() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        let pt = sync.sync_position(1, 5.0, Some(200.0), &lyrics);
        assert_eq!(pt.position_ms, 5000);
        assert!(pt.is_in_gap, "Must be in gap during intro");
        assert_eq!(pt.active_line_index, None);
        assert_eq!(pt.active_word_index, None);
        assert_eq!(pt.line_progress, 0.0);
    }

    #[test]
    fn test_sync_active_line_and_word_progression() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        // Position at 10,250ms -> midpoint of "When" (10000..10500)
        let pt = sync.sync_position(1, 10.250, Some(200.0), &lyrics);
        assert_eq!(pt.active_line_index, Some(0));
        assert!(!pt.is_in_gap);
        assert_eq!(pt.active_word_index, Some(0));
        assert_eq!(
            pt.active_word(&lyrics).map(|w| w.text.as_str()),
            Some("When")
        );
        assert!((pt.word_progress - 0.5).abs() < 0.01);
        assert!((pt.line_progress - 0.0625).abs() < 0.01);

        // Position at 13,250ms -> midpoint of "before" (12500..14000)
        let pt2 = sync.sync_position(1, 13.250, Some(200.0), &lyrics);
        assert_eq!(pt2.active_line_index, Some(0));
        assert_eq!(pt2.active_word_index, Some(4));
        assert_eq!(
            pt2.active_word(&lyrics).map(|w| w.text.as_str()),
            Some("before")
        );
        assert!((pt2.word_progress - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_sync_syllable_timing() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        // Line 2 word 0 "Couldn't": "Could" (16000..16500), "n't" (16500..17000)
        let pt = sync.sync_position(1, 16.250, Some(200.0), &lyrics);
        assert_eq!(pt.active_line_index, Some(1));
        assert_eq!(pt.active_word_index, Some(0));
        assert_eq!(pt.active_syllable_index, Some(0));
        assert_eq!(
            pt.active_syllable(&lyrics).map(|s| s.text.as_str()),
            Some("Could")
        );
        assert!((pt.syllable_progress - 0.5).abs() < 0.01);

        let pt2 = sync.sync_position(1, 16.750, Some(200.0), &lyrics);
        assert_eq!(pt2.active_syllable_index, Some(1));
        assert_eq!(
            pt2.active_syllable(&lyrics).map(|s| s.text.as_str()),
            Some("n't")
        );
        assert!((pt2.syllable_progress - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_sync_interlude_timestamp_gap() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        // Line 1 ends at 14,000; Line 2 starts at 16,000. Position at 15,000:
        let pt = sync.sync_position(1, 15.0, Some(200.0), &lyrics);
        assert!(pt.is_in_gap, "Must report gap during interlude");
        assert_eq!(pt.active_line_index, Some(0));
        assert_eq!(pt.line_progress, 1.0);
        assert_eq!(pt.active_word_index, None);
    }

    #[test]
    fn test_seeking_forwards_and_backwards() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        // Start at 11.0s (Line 0)
        let pt1 = sync.sync_position(1, 11.0, Some(200.0), &lyrics);
        assert_eq!(pt1.active_line_index, Some(0));

        // Seek forward to 18.0s (Line 1)
        let pt2 = sync.sync_position(1, 18.0, Some(200.0), &lyrics);
        assert_eq!(pt2.active_line_index, Some(1));
        assert_eq!(pt2.active_word_index, Some(2));
        assert_eq!(
            pt2.active_word(&lyrics).map(|w| w.text.as_str()),
            Some("you")
        );

        // Seek backward to 12.0s (Line 0)
        let pt3 = sync.sync_position(1, 12.0, Some(200.0), &lyrics);
        assert_eq!(pt3.active_line_index, Some(0));
        assert_eq!(pt3.active_word_index, Some(3));
        assert_eq!(
            pt3.active_word(&lyrics).map(|w| w.text.as_str()),
            Some("here")
        );

        // Seek backward to intro gap (2.0s)
        let pt4 = sync.sync_position(1, 2.0, Some(200.0), &lyrics);
        assert_eq!(pt4.active_line_index, None);
        assert!(pt4.is_in_gap);
    }

    #[test]
    fn test_pause_and_resume_hold_position() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        sync.sync_position(1, 12.0, Some(200.0), &lyrics);
        assert!(!sync.current_sync_point().is_paused);

        // Pause
        sync.set_paused(true);
        let pt_paused = sync.current_sync_point();
        assert!(pt_paused.is_paused);
        assert_eq!(pt_paused.position_ms, 12000);
        assert_eq!(pt_paused.active_line_index, Some(0));

        // Repeated sync while paused holds position
        let pt_held = sync.sync_position(1, 12.0, Some(200.0), &lyrics);
        assert!(pt_held.is_paused);
        assert_eq!(pt_held.position_ms, 12000);

        // Resume
        sync.set_paused(false);
        let pt_resumed = sync.sync_position(1, 12.1, Some(200.0), &lyrics);
        assert!(!pt_resumed.is_paused);
        assert_eq!(pt_resumed.position_ms, 12100);
    }

    #[test]
    fn test_track_completion_and_ended_event() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        sync.sync_position(1, 19.5, Some(20.0), &lyrics);
        assert!(!sync.current_sync_point().is_ended);

        // PlayerEvent::Ended arrives
        let pt = sync.sync_event(&PlayerEvent::Ended { generation: 1 }, &lyrics);
        assert!(pt.is_ended);
        assert_eq!(pt.line_progress, 1.0);
        assert_eq!(pt.word_progress, 1.0);
    }

    #[test]
    fn test_repeat_one_playback_loop() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        // Playing near end of track
        sync.sync_position(1, 19.8, Some(20.0), &lyrics);
        assert_eq!(sync.current_sync_point().active_line_index, Some(1));

        // Repeat-one loops back to 0.5s (same generation or new generation)
        let pt_looped = sync.sync_position(1, 0.5, Some(20.0), &lyrics);
        assert_eq!(pt_looped.position_ms, 500);
        assert!(pt_looped.is_in_gap);
        assert_eq!(pt_looped.active_line_index, None);

        // Generation bumps on restart
        let pt_next_gen = sync.sync_position(2, 10.5, Some(20.0), &lyrics);
        assert_eq!(pt_next_gen.generation, 2);
        assert_eq!(pt_next_gen.active_line_index, Some(0));
        assert_eq!(pt_next_gen.active_word_index, Some(1));
    }

    #[test]
    fn test_redundant_sync_calculation_avoided() {
        let lyrics = build_test_lyrics();
        let mut sync = LyricsSynchronizer::new();

        let pt1 = sync.sync_position(1, 11.0, Some(200.0), &lyrics) as *const LyricsSyncPoint;
        let pt2 = sync.sync_position(1, 11.0, Some(200.0), &lyrics) as *const LyricsSyncPoint;

        // Pointer must match because the exact same cached reference was returned
        assert_eq!(
            pt1, pt2,
            "Unchanged timestamp must return cached sync point directly"
        );
    }

    #[test]
    fn test_overlapping_background_line_both_active() {
        let lyrics = Lyrics {
            lines: vec![
                LyricLine {
                    start_ms: Some(10000),
                    end_ms: Some(14000),
                    text: "Lead line".into(),
                    is_background: false,
                    singer: 0,
                    words: vec![],
                },
                LyricLine {
                    start_ms: Some(12000),
                    end_ms: Some(15000),
                    text: "(Background line)".into(),
                    is_background: true,
                    singer: 0,
                    words: vec![],
                },
                LyricLine {
                    start_ms: Some(16000),
                    end_ms: Some(20000),
                    text: "Next lead line".into(),
                    is_background: false,
                    singer: 0,
                    words: vec![],
                },
            ],
            sync_type: SyncType::Line,
            ..Default::default()
        };

        let mut sync = LyricsSynchronizer::new();

        // At 11.0s: only lead line is active
        let pt1 = sync.sync_position(1, 11.0, Some(20.0), &lyrics);
        assert_eq!(pt1.active_line_index, Some(0));
        assert_eq!(pt1.active_line_indices, vec![0]);
        assert_eq!(pt1.active_lines(&lyrics).len(), 1);

        // At 13.0s: both lead line (10..14s) and background line (12..15s) are active simultaneously
        let pt2 = sync.sync_position(1, 13.0, Some(20.0), &lyrics);
        assert_eq!(pt2.active_line_index, Some(0)); // primary scroll anchor is lead line
        assert_eq!(pt2.active_line_indices, vec![0, 1]);
        assert!(pt2.active_line_indices.contains(&0));
        assert!(pt2.active_line_indices.contains(&1));
        assert_eq!(pt2.active_lines(&lyrics).len(), 2);

        // At 14.5s: lead line finished (14s), but background line (12..15s) is still active
        let pt3 = sync.sync_position(1, 14.5, Some(20.0), &lyrics);
        assert_eq!(pt3.active_line_index, Some(1));
        assert_eq!(pt3.active_line_indices, vec![1]);

        // At 15.5s: interlude gap before next lead line (16s)
        let pt4 = sync.sync_position(1, 15.5, Some(20.0), &lyrics);
        assert!(pt4.is_in_gap);
        assert_eq!(pt4.active_line_index, Some(1)); // stays on previous line, no jump to top
        assert_eq!(pt4.active_line_indices, vec![1]);
    }

    #[test]
    fn test_duet_singers_overlapping_sync_point() {
        let lyrics = Lyrics {
            lines: vec![
                LyricLine::new("I can show you the world", Some(10000), Some(14000)).with_singer(0),
                LyricLine::new("Shining, shimmering, splendid", Some(12000), Some(15000))
                    .with_singer(1),
            ],
            sync_type: SyncType::Line,
            ..Default::default()
        };

        let mut sync = LyricsSynchronizer::new();

        // At 13.0s: both singers are active at the same time
        let pt = sync.sync_position(1, 13.0, Some(20.0), &lyrics);
        // Singer 0 is primary scroll anchor
        assert_eq!(pt.active_line_index, Some(0));
        assert_eq!(pt.active_line_indices, vec![0, 1]);
        assert_eq!(lyrics.lines[pt.active_line_indices[0]].singer, 0);
        assert_eq!(lyrics.lines[pt.active_line_indices[1]].singer, 1);
    }
}
