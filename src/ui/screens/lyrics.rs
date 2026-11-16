use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::App;
use crate::lyrics::synchronizer::compute_sync_point;
use crate::lyrics::types::{LyricsStatus, SyncType};
use crate::lyrics::{LyricLine, Lyrics, LyricsSyncPoint};
use crate::ui::screens::player::{layout_player_panel, render_player_panel};
use crate::ui::theme::{Theme, truncate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LyricsLayout {
    pub col_lyrics: Rect,
    pub col_player: Rect,
}

pub fn layout_lyrics(area: Rect) -> LyricsLayout {
    let (player_w, lyrics_w) = if area.width >= 120 {
        let p_w = (area.width * 36 / 100).clamp(38, 52);
        (p_w, area.width.saturating_sub(p_w))
    } else if area.width >= 85 {
        let p_w = (area.width * 40 / 100).clamp(36, 46);
        (p_w, area.width.saturating_sub(p_w))
    } else {
        let p_w = (area.width / 2).max(20);
        (p_w, area.width.saturating_sub(p_w))
    };

    let cols = Layout::horizontal([Constraint::Length(lyrics_w), Constraint::Length(player_w)])
        .split(area);

    LyricsLayout {
        col_lyrics: cols[0],
        col_player: cols[1],
    }
}

pub fn render_lyrics(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    playback: Option<(f64, Option<f64>)>,
    theme: &Theme,
) {
    let layout = layout_lyrics(area);

    // Right side: Player panel (without search bar)
    let panel_layout = layout_player_panel(layout.col_player, false);
    render_player_panel(
        frame,
        &panel_layout,
        layout.col_player,
        app,
        playback,
        theme,
    );

    let active_track = app
        .current_track()
        .or(app.pending_track())
        .or(app.lyrics_state.current_track.as_ref())
        .or_else(|| app.playing.and_then(|idx| app.tracks.get(idx)));

    // Left / Center side: Lyrics panel
    let title_line = if let Some(track) = active_track {
        let title_text = format!(" LYRICS: {} — {} ", track.title, track.artist);
        let max_w = (layout.col_lyrics.width as usize).saturating_sub(4);
        Line::from(vec![Span::styled(
            truncate(&title_text, max_w),
            theme.bold_primary,
        )])
    } else {
        Line::from(vec![Span::styled(" LYRICS ", theme.bold_primary)])
    };

    let source_name = app
        .lyrics_state
        .lyrics
        .as_ref()
        .and_then(|l| l.source.as_deref())
        .unwrap_or("lrc.red");

    let mut bottom_spans = vec![Span::styled(format!(" via {source_name} "), theme.dim)];
    if !app.lyrics_auto_scroll {
        bottom_spans.push(Span::styled(
            " [MANUAL SCROLL (C to center)] ",
            theme.bold_primary,
        ));
    }

    let lyrics_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(title_line)
        .title_bottom(Line::from(bottom_spans).alignment(Alignment::Right));

    let inner = lyrics_block.inner(layout.col_lyrics);
    frame.render_widget(lyrics_block, layout.col_lyrics);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // State handling:
    // Actively loading lyrics
    if app.lyrics_state.status.is_loading() {
        render_centered_notice(
            frame,
            inner,
            "LOADING LYRICS...",
            "Searching for synchronized lyrics on lrc.red",
            theme,
        );
        return;
    }

    // Error state
    if let LyricsStatus::Failed(ref err) = app.lyrics_state.status {
        render_centered_notice(frame, inner, "LYRICS RETRIEVAL FAILED", err, theme);
        return;
    }

    // Not found or marked unavailable
    if app.lyrics_state.status.is_not_found()
        || app
            .lyrics_state
            .lyrics
            .as_ref()
            .map(|l| l.unavailable_reason.is_some() || l.lines.is_empty())
            .unwrap_or(false)
    {
        let reason = app
            .lyrics_state
            .lyrics
            .as_ref()
            .and_then(|l| l.unavailable_reason.as_deref())
            .unwrap_or("No lyrics available for this song.");
        render_centered_notice(frame, inner, "NO LYRICS AVAILABLE", reason, theme);
        return;
    }

    // Available lyrics
    if let Some(lyrics) = &app.lyrics_state.lyrics {
        render_lyrics_content(frame, inner, app, lyrics, playback, theme);
        return;
    }

    // No track currently active
    if active_track.is_none() {
        render_centered_notice(
            frame,
            inner,
            "NO TRACK PLAYING",
            "Select and play a song to view lyrics.",
            theme,
        );
    } else {
        render_centered_notice(
            frame,
            inner,
            "LYRICS IDLE",
            "Press 'l' or play a track to fetch lyrics.",
            theme,
        );
    }
}

fn render_lyrics_content(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    lyrics: &Lyrics,
    playback: Option<(f64, Option<f64>)>,
    theme: &Theme,
) {
    // Determine active synchronization point
    let sync_point = resolve_sync_point(app, lyrics, playback);

    let is_timed = lyrics.is_timed() && lyrics.sync_type != SyncType::Unsynced;
    let mut rendered_lines: Vec<Line> = Vec::with_capacity(lyrics.lines.len() + 2);

    if is_timed {
        for (idx, line) in lyrics.lines.iter().enumerate() {
            rendered_lines.push(build_timed_line(idx, line, &sync_point, theme));
        }
    } else {
        let unsynced_style = Style::default().fg(Color::Gray);
        for line in &lyrics.lines {
            rendered_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line.display_text().to_string(), unsynced_style),
            ]));
        }
    }

    // Calculate vertical scroll offset (Auto vs Manual)
    let visible_height = inner.height as usize;
    let total_lines = rendered_lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);

    let scroll_y = if app.lyrics_auto_scroll {
        if is_timed && let Some(active_idx) = sync_point.active_line_index {
            let center_offset = visible_height / 3;
            active_idx.saturating_sub(center_offset).min(max_scroll)
        } else {
            0
        }
    } else {
        app.lyrics_scroll_offset.min(max_scroll)
    };

    let paragraph = Paragraph::new(rendered_lines)
        .scroll((scroll_y as u16, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);
}

fn resolve_sync_point(
    app: &App,
    lyrics: &Lyrics,
    playback: Option<(f64, Option<f64>)>,
) -> LyricsSyncPoint {
    let cached = app.lyrics_synchronizer.current_sync_point();
    if cached.position_ms > 0 || cached.has_active_line() || cached.is_ended {
        cached.clone()
    } else if let Some((elapsed, duration)) = playback {
        let pos_ms = (elapsed.max(0.0) * 1000.0).round() as u64;
        let dur_ms = duration.map(|d| (d * 1000.0).round() as u64);
        compute_sync_point(
            pos_ms,
            dur_ms,
            app.resolve_generation,
            app.is_paused(),
            false,
            lyrics,
            None,
        )
    } else {
        cached.clone()
    }
}

fn build_timed_line<'a>(
    idx: usize,
    line: &'a LyricLine,
    sync_point: &LyricsSyncPoint,
    theme: &Theme,
) -> Line<'a> {
    let is_current = sync_point.active_line_index == Some(idx) && !sync_point.is_in_gap;
    let is_past = match sync_point.active_line_index {
        Some(curr) => idx < curr || (idx == curr && sync_point.is_in_gap),
        None => false,
    };

    if !is_current {
        // Gone lyrics are dimmer (dark grey + italic), upcoming lyrics are dim (medium gray)
        let style = if is_past {
            theme.dim.add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(Color::Gray)
        };
        return Line::from(vec![
            Span::raw("  "),
            Span::styled(line.display_text().to_string(), style),
        ]);
    }

    // Active line: prominent bold marker and highlighted styling
    let mut spans = vec![Span::styled("▶ ", theme.bold_primary)];

    // If line has no words or active word index is none, highlight the whole line
    if line.words.is_empty() || sync_point.active_word_index.is_none() {
        spans.push(Span::styled(
            line.display_text().to_string(),
            theme.bold_primary,
        ));
        return Line::from(spans);
    }

    let active_w_idx = sync_point.active_word_index.unwrap_or(0);
    let upcoming_word_style = Style::default()
        .fg(Color::Gray)
        .add_modifier(Modifier::BOLD);

    for (w_idx, word) in line.words.iter().enumerate() {
        if w_idx > 0 {
            spans.push(Span::raw(" "));
        }

        if w_idx < active_w_idx {
            // Already sung word: highlight color
            spans.push(Span::styled(word.text.clone(), theme.bold_primary));
        } else if w_idx > active_w_idx {
            // Upcoming word on active line: white
            spans.push(Span::styled(word.text.clone(), upcoming_word_style));
        } else {
            // Active word currently being sung!
            if !word.syllables.is_empty() && sync_point.active_syllable_index.is_some() {
                let active_s_idx = sync_point.active_syllable_index.unwrap_or(0);
                for (s_idx, syl) in word.syllables.iter().enumerate() {
                    if s_idx <= active_s_idx {
                        // Already sung syllable: highlight color
                        spans.push(Span::styled(syl.text.clone(), theme.bold_primary));
                    } else {
                        // Upcoming syllable: white
                        spans.push(Span::styled(syl.text.clone(), upcoming_word_style));
                    }
                }
            } else {
                // Currently singing word: highlight color
                spans.push(Span::styled(word.text.clone(), theme.bold_primary));
            }
        }
    }

    Line::from(spans)
}

fn render_centered_notice(
    frame: &mut Frame,
    area: Rect,
    headline: &str,
    subtext: &str,
    theme: &Theme,
) {
    let vertical_pad = (area.height.saturating_sub(3) / 2) as usize;
    let mut lines = Vec::with_capacity(vertical_pad + 3);
    for _ in 0..vertical_pad {
        lines.push(Line::raw(""));
    }
    lines.push(
        Line::from(vec![Span::styled(headline.to_string(), theme.bold_primary)])
            .alignment(Alignment::Center),
    );
    lines.push(Line::raw(""));
    lines.push(
        Line::from(vec![Span::styled(subtext.to_string(), theme.dim)]).alignment(Alignment::Center),
    );

    let p = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{LyricSyllable, LyricWord};
    use crate::music::Track;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_layout_lyrics_proportions_wide() {
        for width in [120, 140, 160, 200] {
            let area = Rect::new(0, 0, width, 30);
            let layout = layout_lyrics(area);

            assert!(layout.col_player.width >= 38 && layout.col_player.width <= 52);
            assert!(layout.col_lyrics.width >= 70);
            assert_eq!(
                layout.col_lyrics.width + layout.col_player.width,
                width,
                "Columns must sum to total width at {width}"
            );
            assert_eq!(layout.col_lyrics.x, 0);
            assert_eq!(layout.col_player.x, layout.col_lyrics.width);
        }
    }

    #[test]
    fn test_layout_lyrics_proportions_medium() {
        for width in [85, 90, 100, 110] {
            let area = Rect::new(0, 0, width, 30);
            let layout = layout_lyrics(area);

            assert!(layout.col_player.width >= 36 && layout.col_player.width <= 46);
            assert!(layout.col_lyrics.width >= 45);
            assert_eq!(
                layout.col_lyrics.width + layout.col_player.width,
                width,
                "Columns must sum to total width at {width}"
            );
            assert_eq!(layout.col_lyrics.x, 0);
            assert_eq!(layout.col_player.x, layout.col_lyrics.width);
        }
    }

    #[test]
    fn test_layout_lyrics_proportions_compact() {
        for width in [40, 50, 60, 70, 80] {
            let area = Rect::new(0, 0, width, 24);
            let layout = layout_lyrics(area);

            assert_eq!(
                layout.col_lyrics.width + layout.col_player.width,
                width,
                "Columns must sum to total width at {width}"
            );
            assert!(layout.col_player.width >= 20);
            assert!(layout.col_lyrics.width >= 20);
            assert_eq!(layout.col_lyrics.x, 0);
            assert_eq!(layout.col_player.x, layout.col_lyrics.width);
        }
    }

    #[test]
    fn test_render_lyrics_no_track_state() {
        let app = App::new();
        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer[(x, y)].symbol()))
            .collect();

        assert!(text.contains("NO TRACK PLAYING"));
        assert!(text.contains("Select and play a song"));
    }

    #[test]
    fn test_render_lyrics_loading_state() {
        let mut app = App::new();
        app.tracks.push(Track::new("Creep", "Radiohead"));
        app.playing = Some(0);
        app.lyrics_state.status = LyricsStatus::Loading;

        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer[(x, y)].symbol()))
            .collect();

        assert!(text.contains("LOADING LYRICS..."));
        assert!(text.contains("Searching for synchronized lyrics"));
    }

    #[test]
    fn test_render_lyrics_not_found_and_failed_states() {
        let mut app = App::new();
        app.tracks.push(Track::new("Creep", "Radiohead"));
        app.playing = Some(0);

        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);

        // NotFound
        app.lyrics_state.status = LyricsStatus::NotFound;
        let backend1 = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend1).unwrap();
        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();
        let buffer1 = terminal.backend().buffer();
        let text1: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer1[(x, y)].symbol()))
            .collect();
        assert!(text1.contains("NO LYRICS AVAILABLE"));

        // Failed
        app.lyrics_state.status = LyricsStatus::Failed("Network timeout".to_string());
        let backend2 = TestBackend::new(area.width, area.height);
        let mut terminal2 = Terminal::new(backend2).unwrap();
        terminal2
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();
        let buffer2 = terminal2.backend().buffer();
        let text2: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer2[(x, y)].symbol()))
            .collect();
        assert!(text2.contains("LYRICS RETRIEVAL FAILED"));
        assert!(text2.contains("Network timeout"));
    }

    #[test]
    fn test_render_unsynchronized_lyrics() {
        let mut app = App::new();
        app.tracks.push(Track::new("Plain Song", "Artist"));
        app.playing = Some(0);
        app.lyrics_state.status = LyricsStatus::Available;
        app.lyrics_state.lyrics = Some(Lyrics::from_plain_text(
            Some("Plain Song".to_string()),
            Some("Artist".to_string()),
            "First unsynced line\nSecond unsynced line",
        ));

        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer[(x, y)].symbol()))
            .collect();

        assert!(text.contains("First unsynced line"));
        assert!(text.contains("Second unsynced line"));
    }

    #[test]
    fn test_render_line_and_word_synchronized_lyrics() {
        let mut app = App::new();
        app.tracks.push(Track::new("Timed Song", "Queen"));
        app.playing = Some(0);
        app.lyrics_state.status = LyricsStatus::Available;

        let lyrics = Lyrics {
            source: Some("lrc.red".to_string()),
            sync_type: SyncType::Word,
            title: Some("Timed Song".to_string()),
            artist: Some("Queen".to_string()),
            isrc: None,
            lines: vec![
                LyricLine::with_words(
                    "Is this the real life?",
                    Some(2000),
                    Some(5000),
                    vec![
                        LyricWord::new("Is", Some(2000), Some(2500)),
                        LyricWord::new("this", Some(2500), Some(3000)),
                        LyricWord::new("the", Some(3000), Some(3500)),
                        LyricWord::new("real", Some(3500), Some(4200)),
                        LyricWord::new("life?", Some(4200), Some(5000)),
                    ],
                ),
                LyricLine::with_words(
                    "Is this just fantasy?",
                    Some(6000),
                    Some(9000),
                    vec![
                        LyricWord::new("Is", Some(6000), Some(6500)),
                        LyricWord::with_syllables(
                            "fantasy?",
                            Some(7000),
                            Some(9000),
                            vec![
                                LyricSyllable::new("fan", Some(7000), Some(7600)),
                                LyricSyllable::new("ta", Some(7600), Some(8200)),
                                LyricSyllable::new("sy?", Some(8200), Some(9000)),
                            ],
                        ),
                    ],
                ),
            ],
            unavailable_reason: None,
        };
        app.lyrics_state.lyrics = Some(lyrics);

        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        // Position at 3.2s -> active line 0, active word "the"
        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, Some((3.2, Some(180.0))), &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer[(x, y)].symbol()))
            .collect();

        // Must display the active line marker
        assert!(text.contains("▶"));
        assert!(text.contains("Is this the real life?"));
        assert!(text.contains("Is this just fantasy?"));
        assert!(text.contains("via lrc.red"));

        // At 3.2s: line 0 active, line 1 upcoming (dim gray)
        // Check that upcoming line 1 has gray cells
        let has_gray_upcoming = (0..area.height).any(|y| {
            (0..area.width).any(|x| {
                let cell = &buffer[(x, y)];
                cell.symbol() == "f" && cell.fg == Color::Gray
            })
        });
        assert!(has_gray_upcoming, "Upcoming lyrics must be dim gray");

        // Now test later position at 7.5s -> line 0 is past (dimmed/italic), line 1 is active
        let mut terminal2 = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal2
            .draw(|frame| {
                render_lyrics(frame, area, &app, Some((7.5, Some(180.0))), &theme);
            })
            .unwrap();
        let buffer2 = terminal2.backend().buffer();

        // Check that past line 0 has dimmed italic cells
        let has_dim_italic_past = (0..area.height).any(|y| {
            (0..area.width).any(|x| {
                let cell = &buffer2[(x, y)];
                cell.symbol() == "r"
                    && cell.fg == Color::DarkGray
                    && cell.modifier.contains(Modifier::ITALIC)
            })
        });
        assert!(
            has_dim_italic_past,
            "Past gone lyrics must be dimmer with italic modifier"
        );
    }

    #[test]
    fn test_render_manual_scroll_badge() {
        let mut app = App::new();
        app.tracks.push(Track::new("Song", "Artist"));
        app.playing = Some(0);
        app.lyrics_auto_scroll = false;
        app.lyrics_scroll_offset = 5;

        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 120, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_lyrics(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| buffer[(x, y)].symbol()))
            .collect();

        assert!(text.contains("MANUAL SCROLL"));
    }
}
