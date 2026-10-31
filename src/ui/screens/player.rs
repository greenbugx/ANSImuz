use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, ListFocus, Mode};
use crate::ui::components::{
    controls::render_controls,
    logo::{bitmap_lines, logo_bitmap, scale_bitmap},
    progress::render_progress_bar,
    queue_list::render_queue_list,
    search_box::render_search_box,
    track_list::render_track_list,
    volume_timer::render_volume_timer,
};
use crate::ui::theme::{Theme, truncate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerLayout {
    pub col_left: Rect,
    pub col_player: Rect,
    pub col_queue: Rect,
    pub search_area: Rect,
    pub keybinds_area: Rect,
    pub show_logo: bool,
    pub logo_area: Rect,
    pub logo_width: usize,
    pub logo_px: usize,
    pub search_box_area: Rect,
    pub status_area: Rect,
    pub art_area: Rect,
    pub vol_timer_area: Rect,
    pub bar_area: Rect,
    pub bar_width: usize,
    pub controls_slots: [Rect; 5],
}

pub fn layout_player(area: Rect) -> PlayerLayout {
    let (col_left, col_player, col_queue) = if area.width >= 120 {
        // Wide (>= 120 cols):
        //Balanced proportional three-panel layout
        let cols = Layout::horizontal([
            Constraint::Percentage(26),
            Constraint::Percentage(48),
            Constraint::Percentage(26),
        ])
        .split(area);
        (cols[0], cols[1], cols[2])
    } else if area.width >= 100 {
        // Transition (100..120 cols):
        // Queue smoothly scales up to proportional width
        let target_queue_w = area.width * 26 / 100;
        let queue_w = (area.width - 100) * target_queue_w / 20;
        let remaining = area.width - queue_w;
        let left_w = remaining * 35 / 100;
        let player_w = remaining - left_w;

        let cols = Layout::horizontal([
            Constraint::Length(left_w),
            Constraint::Length(player_w),
            Constraint::Length(queue_w),
        ])
        .split(area);
        (cols[0], cols[1], cols[2])
    } else if area.width >= 75 {
        // Medium (75..100 cols):
        // Two-panel layout (Search/Keybinds | Player)
        // Queue hidden
        let left_w = area.width * 35 / 100;
        let player_w = area.width - left_w;

        let cols = Layout::horizontal([Constraint::Length(left_w), Constraint::Length(player_w)])
            .split(area);
        (cols[0], cols[1], Rect::default())
    } else {
        // Compact (< 75 cols):
        // Stacked vertical layout (Player on top, Search below)
        // Queue hidden
        let player_h = (area.height * 60 / 100)
            .max(14)
            .min(area.height.saturating_sub(8));
        let search_h = area.height.saturating_sub(player_h);

        let rows = Layout::vertical([Constraint::Length(player_h), Constraint::Length(search_h)])
            .split(area);
        (rows[1], rows[0], Rect::default())
    };

    let keybinds_height = if col_left.height >= 16 {
        (crate::app::commands::PANEL_KEYBINDS.len() as u16 + 2)
            .min(col_left.height.saturating_sub(6))
    } else if col_left.height >= 8 {
        4
    } else {
        0
    };

    let left_sections =
        Layout::vertical([Constraint::Min(4), Constraint::Length(keybinds_height)]).split(col_left);
    let search_area = left_sections[0];
    let keybinds_area = left_sections[1];

    let player_block = Block::default().borders(Borders::ALL);
    let player_inner = player_block.inner(col_player);

    let bitmap = logo_bitmap();
    let art_width = bitmap.first().map_or(0, |row| row.len());
    let art_height = bitmap.len();

    let is_compact_height = player_inner.height < 14;
    let spacer_gap: u16 = if is_compact_height { 0 } else { 1 };

    let max_logo_height = (player_inner.height as usize).saturating_sub(15).min(8);
    let mut target_width = (player_inner.width as usize)
        .saturating_sub(4)
        .min(art_width);
    let mut target_px = (art_height * target_width / art_width.max(1)).div_ceil(2) * 2;
    let max_px = max_logo_height * 2;

    if target_px > max_px {
        target_px = max_px;
        target_width = art_width * target_px / art_height.max(1);
    }

    let show_logo = target_width >= 20 && target_px >= 4 && max_logo_height >= 3;
    let logo_height = if show_logo {
        target_px as u16 / 2 + 1
    } else {
        0
    };

    let player_rows = Layout::vertical([
        Constraint::Length(if show_logo { logo_height } else { 0 }), // 0: Logo at top
        Constraint::Length(3),                                       // 1: Search box
        Constraint::Length(1),                                       // 2: Status
        Constraint::Min(4),                                          // 3: Artwork + Title + Artist
        Constraint::Length(1),          // 4: Volume (left) & Timer (right)
        Constraint::Length(spacer_gap), // 5: Spacer gap
        Constraint::Length(1),          // 6: Progress bar
        Constraint::Length(1),          // 7: Controls
    ])
    .split(player_inner);

    let logo_area = player_rows[0];
    let search_box_area = player_rows[1];
    let status_area = player_rows[2];
    let art_area = player_rows[3];

    let bar_width = (player_rows[6].width as usize).saturating_sub(4).max(20);
    let bar_area = Rect {
        x: player_rows[6].x + (player_rows[6].width - bar_width as u16) / 2,
        y: player_rows[6].y,
        width: bar_width as u16,
        height: 1,
    };

    let vol_timer_area = Rect {
        x: bar_area.x,
        y: player_rows[4].y,
        width: bar_area.width,
        height: 1,
    };

    let cluster_width = 45.min(player_rows[7].width);
    let cluster = Rect {
        x: player_rows[7].x + (player_rows[7].width - cluster_width) / 2,
        y: player_rows[7].y,
        width: cluster_width,
        height: 1,
    };

    let slot_w = cluster_width / 5;
    let raw_slots = Layout::horizontal([
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
    ])
    .split(cluster);

    let controls_slots = [
        raw_slots[0],
        raw_slots[1],
        raw_slots[2],
        raw_slots[3],
        raw_slots[4],
    ];

    PlayerLayout {
        col_left,
        col_player,
        col_queue,
        search_area,
        keybinds_area,
        show_logo,
        logo_area,
        logo_width: target_width,
        logo_px: target_px,
        search_box_area,
        status_area,
        art_area,
        vol_timer_area,
        bar_area,
        bar_width,
        controls_slots,
    }
}

pub fn render_player(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    playback: Option<(f64, Option<f64>)>,
    theme: &Theme,
) {
    let layout = layout_player(area);
    let has_queue_panel = layout.col_queue.width >= 8;
    let is_search_focused = app.list_focus == ListFocus::Search;
    let is_queue_focused = app.list_focus == ListFocus::Queue;

    // LEFT LOWER SECTION: Search Results or Queue (when queue panel is hidden)
    if !has_queue_panel && is_queue_focused {
        let count = app.queue.len();
        let queue_title = if layout.search_area.width >= 16 {
            format!(" UP NEXT ({count}) ")
        } else if layout.search_area.width >= 10 {
            format!(" Q ({count}) ")
        } else {
            " Q ".to_string()
        };

        let queue_block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.bold_primary)
            .title(Line::from(vec![Span::styled(
                queue_title,
                theme.bold_primary,
            )]));
        let queue_inner = queue_block.inner(layout.search_area);
        frame.render_widget(queue_block, layout.search_area);

        render_queue_list(
            frame,
            queue_inner,
            &app.queue,
            app.queue_selected,
            true,
            theme,
        );
    } else {
        let search_title = if app.last_query.is_empty() {
            format!(" SEARCH RESULTS ({}) ", app.settings.provider.label())
        } else {
            format!(
                " RESULTS: {} ({}) ",
                truncate(&app.last_query, 14),
                app.settings.provider.label()
            )
        };

        let search_border_style = if is_search_focused || !has_queue_panel {
            theme.bold_primary
        } else {
            theme.dim
        };

        let search_title_style = if is_search_focused || !has_queue_panel {
            theme.bold_primary
        } else {
            theme.primary
        };

        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(search_border_style)
            .title(Line::from(vec![Span::styled(
                search_title,
                search_title_style,
            )]));
        let search_inner = search_block.inner(layout.search_area);
        frame.render_widget(search_block, layout.search_area);

        let empty_msg = (!app.loading && !app.last_query.is_empty()).then_some("NO RESULTS");
        render_track_list(
            frame,
            search_inner,
            &app.tracks,
            app.selected,
            empty_msg,
            is_search_focused,
            theme,
        );
    }

    if layout.keybinds_area.height >= 3 {
        let keybinds_block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.primary)
            .title(Line::from(vec![Span::styled(
                " KEYBINDS ",
                theme.bold_primary,
            )]));
        let keybinds_inner = keybinds_block.inner(layout.keybinds_area);
        frame.render_widget(keybinds_block, layout.keybinds_area);

        if keybinds_inner.height >= 9 {
            let keybind_lines: Vec<Line> = crate::app::commands::PANEL_KEYBINDS
                .iter()
                .map(|(key, action)| {
                    Line::from(vec![
                        Span::styled(format!(" {:<8}", key), theme.bold_primary),
                        Span::styled(*action, theme.dim),
                    ])
                })
                .collect();
            frame.render_widget(Paragraph::new(keybind_lines), keybinds_inner);
        } else {
            let lines = vec![
                Line::from(vec![
                    Span::styled("Enter:", theme.bold_primary),
                    Span::styled("Play  ", theme.dim),
                    Span::styled("Space:", theme.bold_primary),
                    Span::styled("Pause  ", theme.dim),
                    Span::styled("←→:", theme.bold_primary),
                    Span::styled("Track", theme.dim),
                ]),
                Line::from(vec![
                    Span::styled("X:", theme.bold_primary),
                    Span::styled("Shuffle  ", theme.dim),
                    Span::styled("R:", theme.bold_primary),
                    Span::styled("Repeat  ", theme.dim),
                    Span::styled("?:", theme.bold_primary),
                    Span::styled("Help  ", theme.dim),
                    Span::styled("Q:", theme.bold_primary),
                    Span::styled("Quit", theme.dim),
                ]),
            ];
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center),
                keybinds_inner,
            );
        }
    }

    // MIDDLE SECTION: Player
    let player_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(
            " PLAYER ",
            theme.bold_primary,
        )]));
    frame.render_widget(player_block, layout.col_player);

    let current = app
        .now_playing
        .as_ref()
        .or(app.pending_track.as_ref())
        .or_else(|| app.tracks.get(app.selected));

    if layout.show_logo {
        let bitmap = logo_bitmap();
        let scaled = scale_bitmap(bitmap, layout.logo_width, layout.logo_px);
        frame.render_widget(
            Paragraph::new(bitmap_lines(&scaled))
                .style(theme.bold_primary)
                .alignment(Alignment::Center),
            layout.logo_area,
        );
    }

    render_search_box(
        frame,
        layout.search_box_area,
        &app.query,
        app.mode == Mode::Type,
        theme.primary,
        theme.bold_primary,
        theme.dim,
    );

    frame.render_widget(
        Paragraph::new(app.visible_status())
            .style(theme.dim)
            .alignment(Alignment::Center),
        layout.status_area,
    );

    crate::artwork::render_art(
        frame,
        layout.art_area,
        current.map(|track| track.art_url.as_str()).unwrap_or(""),
        current.map(|track| track.title.as_str()).unwrap_or(""),
        current.map(|track| track.artist.as_str()).unwrap_or(""),
        app.settings.art_hq,
        Some(theme.accent),
    );

    let vol_timer_area = Rect {
        x: layout.bar_area.x,
        y: layout.vol_timer_area.y,
        width: layout.bar_area.width,
        height: 1,
    };

    render_volume_timer(
        frame,
        vol_timer_area,
        app.volume,
        playback,
        theme.primary,
        theme.bold_primary,
    );

    let song_loading = (app.now_playing.is_some() || app.is_resolving) && playback.is_none();
    render_progress_bar(
        frame,
        layout.bar_area,
        layout.bar_width,
        song_loading,
        playback,
        theme.bold_primary,
        theme.dim,
    );

    render_controls(
        frame,
        &layout.controls_slots,
        app.shuffle,
        app.paused,
        app.repeat,
        theme.bold_primary,
        theme.dim,
    );

    // RIGHT SECTION: Queue
    // only rendered when wide enough for 3-panel layout
    if has_queue_panel {
        let count = app.queue.len();
        let queue_title = if layout.col_queue.width >= 16 {
            format!(" UP NEXT ({count}) ")
        } else if layout.col_queue.width >= 10 {
            format!(" Q ({count}) ")
        } else {
            " Q ".to_string()
        };

        let is_queue_focused = app.list_focus == ListFocus::Queue;
        let queue_border_style = if is_queue_focused {
            theme.bold_primary
        } else {
            theme.dim
        };
        let queue_title_style = if is_queue_focused {
            theme.bold_primary
        } else {
            theme.primary
        };

        let queue_block = Block::default()
            .borders(Borders::ALL)
            .border_style(queue_border_style)
            .title(Line::from(vec![Span::styled(
                queue_title,
                queue_title_style,
            )]));
        let queue_inner = queue_block.inner(layout.col_queue);
        frame.render_widget(queue_block, layout.col_queue);

        render_queue_list(
            frame,
            queue_inner,
            &app.queue,
            app.queue_selected,
            is_queue_focused,
            theme,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_layout_player_breakpoint_wide() {
        for width in [120, 140, 160, 196, 200] {
            let area = Rect::new(0, 0, width, 30);
            let layout = layout_player(area);

            assert!(layout.col_left.width >= 30);
            assert!(layout.col_player.width >= 55);
            assert!(
                layout.col_queue.width >= 30,
                "Queue must be substantial on wide screens at {width}, got {}",
                layout.col_queue.width
            );
            assert_eq!(
                layout.col_left.width + layout.col_player.width + layout.col_queue.width,
                width,
                "Three panels must sum to total width at {width}"
            );
        }
    }

    #[test]
    fn test_layout_player_breakpoint_medium() {
        for width in [75, 80, 90, 99] {
            let area = Rect::new(0, 0, width, 30);
            let layout = layout_player(area);

            assert_eq!(
                layout.col_queue.width, 0,
                "Queue must be hidden under 100 cols"
            );
            assert!(
                layout.col_left.width >= 26,
                "Left panel must be >= 26 cols at {width}, got {}",
                layout.col_left.width
            );
            assert!(
                layout.col_player.width >= 48,
                "Player panel must be >= 48 cols at {width}, got {}",
                layout.col_player.width
            );
            assert_eq!(
                layout.col_left.width + layout.col_player.width,
                width,
                "Two panels must sum to total width at {width}"
            );
        }
    }

    #[test]
    fn test_layout_player_seamless_breakpoint_99_to_100() {
        let l99 = layout_player(Rect::new(0, 0, 99, 30));
        let l100 = layout_player(Rect::new(0, 0, 100, 30));

        assert_eq!(l99.col_queue.width, 0);
        assert_eq!(l100.col_queue.width, 0);

        let left_diff = (l100.col_left.width as i32 - l99.col_left.width as i32).abs();
        let player_diff = (l100.col_player.width as i32 - l99.col_player.width as i32).abs();

        assert!(
            left_diff <= 1,
            "Left panel transition from 99 to 100 must be smooth, diff was {left_diff}"
        );
        assert!(
            player_diff <= 1,
            "Player panel transition from 99 to 100 must be smooth, diff was {player_diff}"
        );
    }

    #[test]
    fn test_layout_player_seamless_breakpoint_119_to_120() {
        let l119 = layout_player(Rect::new(0, 0, 119, 30));
        let l120 = layout_player(Rect::new(0, 0, 120, 30));

        let left_diff = (l120.col_left.width as i32 - l119.col_left.width as i32).abs();
        let player_diff = (l120.col_player.width as i32 - l119.col_player.width as i32).abs();
        let queue_diff = (l120.col_queue.width as i32 - l119.col_queue.width as i32).abs();

        assert!(
            left_diff <= 4,
            "Left panel transition from 119 to 120 must be smooth, diff was {left_diff}"
        );
        assert!(
            player_diff <= 4,
            "Player panel transition from 119 to 120 must be smooth, diff was {player_diff}"
        );
        assert!(
            queue_diff <= 4,
            "Queue panel transition from 119 to 120 must be smooth, diff was {queue_diff}"
        );
    }

    #[test]
    fn test_layout_player_compact_mode_narrow() {
        for width in [40, 45, 50, 51, 60, 70, 74] {
            let area = Rect::new(0, 0, width, 24);
            let layout = layout_player(area);

            assert_eq!(
                layout.col_queue.width, 0,
                "Queue must be hidden in compact mode"
            );
            assert_eq!(
                layout.col_player.width, width,
                "Player panel must have full width in compact mode at {width}"
            );
            assert_eq!(
                layout.col_left.width, width,
                "Search panel must have full width in compact mode at {width}"
            );
            assert!(
                layout.col_player.y + layout.col_player.height <= layout.col_left.y,
                "Player must be stacked above search at {width}"
            );
            assert_eq!(
                layout.col_player.height + layout.col_left.height,
                area.height,
                "Stacked heights must sum to area height at {width}"
            );

            // Verify inner vertical budget:
            // all player elements must fit within col_player
            assert!(
                layout.controls_slots[0].y + layout.controls_slots[0].height < layout.col_left.y,
                "Controls must not overlap search section at width {width}"
            );
            assert!(
                layout.art_area.height >= 4,
                "Artwork area must have at least 4 rows at width {width}"
            );
        }
    }

    #[test]
    fn test_render_player_40x24_output() {
        let app = App::new();
        let theme = Theme::new(None);
        let area = Rect::new(0, 0, 40, 24);

        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_player(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Check PLAYER block header rendered at top
        let top_row = (0..area.width)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>();
        assert!(
            top_row.contains("PLAYER"),
            "Top row must contain PLAYER header"
        );

        // Check SEARCH RESULTS block rendered below player (at row 14)
        let search_row = (0..area.width)
            .map(|x| buffer[(x, 14)].symbol())
            .collect::<String>();
        assert!(
            search_row.contains("SEARCH"),
            "Search block must start at row 14, got {search_row}"
        );

        // Check KEYBINDS block rendered at row 20
        let keybinds_header = (0..area.width)
            .map(|x| buffer[(x, 20)].symbol())
            .collect::<String>();
        assert!(
            keybinds_header.contains("KEYBINDS"),
            "Keybinds header must render at row 20, got {keybinds_header}"
        );

        // Check keybind content rendered on rows 21 and 22 without clipping
        let keybind_content_1 = (0..area.width)
            .map(|x| buffer[(x, 21)].symbol())
            .collect::<String>();
        let keybind_content_2 = (0..area.width)
            .map(|x| buffer[(x, 22)].symbol())
            .collect::<String>();

        assert!(
            keybind_content_1.contains("Enter:") && keybind_content_1.contains("Space:"),
            "Row 21 must contain Enter and Space shortcuts, got: {keybind_content_1}"
        );
        assert!(
            keybind_content_2.contains("X:") && keybind_content_2.contains("R:"),
            "Row 22 must contain Shuffle (X) and Repeat (R) shortcuts, got: {keybind_content_2}"
        );
    }

    #[test]
    fn test_layout_player_vol_timer_matches_bar_span() {
        let area = Rect::new(0, 0, 140, 35);
        let layout = layout_player(area);

        assert_eq!(layout.vol_timer_area.x, layout.bar_area.x);
        assert_eq!(layout.vol_timer_area.width, layout.bar_area.width);
        assert!(layout.vol_timer_area.y < layout.bar_area.y);
    }

    #[test]
    fn test_layout_player_non_overlapping_columns() {
        // Wide (3 panels side-by-side)
        let area_wide = Rect::new(0, 0, 140, 30);
        let layout_wide = layout_player(area_wide);
        assert!(layout_wide.col_left.x + layout_wide.col_left.width <= layout_wide.col_player.x);
        assert!(layout_wide.col_player.x + layout_wide.col_player.width <= layout_wide.col_queue.x);

        // Medium (2 panels side-by-side)
        let area_med = Rect::new(0, 0, 90, 30);
        let layout_med = layout_player(area_med);
        assert!(layout_med.col_left.x + layout_med.col_left.width <= layout_med.col_player.x);
        assert_eq!(layout_med.col_queue.width, 0);

        // Compact (stacked vertically)
        let area_compact = Rect::new(0, 0, 60, 30);
        let layout_compact = layout_player(area_compact);
        assert!(
            layout_compact.col_player.y + layout_compact.col_player.height
                <= layout_compact.col_left.y
        );
        assert_eq!(layout_compact.col_queue.width, 0);
    }

    #[test]
    fn test_render_player_queue_reveal_unclipped() {
        let app = App::new();
        let theme = Theme::new(None);

        for width in [108, 110, 115, 120, 124, 140] {
            let area = Rect::new(0, 0, width, 30);
            let backend = TestBackend::new(area.width, area.height);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|frame| {
                    render_player(frame, area, &app, None, &theme);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let layout = layout_player(area);

            assert!(
                layout.col_queue.width >= 8,
                "Queue must be rendered at width {width}"
            );

            // Check that the queue contains its title without overflow
            let title_row = (layout.col_queue.x..layout.col_queue.x + layout.col_queue.width)
                .map(|x| buffer[(x, layout.col_queue.y)].symbol())
                .collect::<String>();
            if layout.col_queue.width >= 16 {
                assert!(
                    title_row.contains("UP NEXT (0)"),
                    "Must contain UP NEXT (0) title at width {width}, got {title_row}"
                );
            } else if layout.col_queue.width >= 10 {
                assert!(
                    title_row.contains("Q (0)"),
                    "Must contain Q (0) title at width {width}, got {title_row}"
                );
            } else {
                assert!(
                    title_row.contains("Q"),
                    "Must contain Q title at width {width}, got {title_row}"
                );
            }

            // Check that the empty state is rendered
            let mut found_empty = false;
            for y in layout.col_queue.y + 1..layout.col_queue.y + layout.col_queue.height - 1 {
                let row_str = (layout.col_queue.x + 1
                    ..layout.col_queue.x + layout.col_queue.width - 1)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>();
                if row_str.contains("empty") || row_str.contains("Empty") {
                    found_empty = true;
                    break;
                }
            }
            assert!(
                found_empty,
                "Queue empty state must render at width {width}"
            );
        }
    }

    #[test]
    fn test_render_player_queue_with_items_and_focus() {
        use crate::music::Track;

        let mut app = App::new();
        let theme = Theme::new(None);
        app.enqueue(Track {
            title: "First Queue Song".into(),
            artist: "Queue Artist".into(),
            stream_url: "http://stream/1".into(),
            art_url: "".into(),
            spotify_id: None,
            artists: vec!["Queue Artist".into()],
            album: None,
            duration_ms: None,
        });
        app.enqueue(Track {
            title: "Second Queue Song".into(),
            artist: "Queue Artist 2".into(),
            stream_url: "http://stream/2".into(),
            art_url: "".into(),
            spotify_id: None,
            artists: vec!["Queue Artist 2".into()],
            album: None,
            duration_ms: None,
        });
        app.tracks = vec![Track {
            title: "Search Track".into(),
            artist: "Search Artist".into(),
            stream_url: "".into(),
            art_url: "".into(),
            spotify_id: None,
            artists: vec![],
            album: None,
            duration_ms: None,
        }];
        app.selected = 0;
        app.list_focus = ListFocus::Queue;
        let area = Rect::new(0, 0, 140, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_player(frame, area, &app, None, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let layout = layout_player(area);

        // Title must show live count
        let title_row = (layout.col_queue.x..layout.col_queue.x + layout.col_queue.width)
            .map(|x| buffer[(x, layout.col_queue.y)].symbol())
            .collect::<String>();
        assert!(
            title_row.contains("UP NEXT (2)"),
            "Title must show live count UP NEXT (2), got {title_row}"
        );

        // Inner rows must contain position numbers and track titles
        let mut found_first = false;
        let mut found_second = false;
        for y in layout.col_queue.y + 1..layout.col_queue.y + layout.col_queue.height - 1 {
            let row_str = (layout.col_queue.x + 1..layout.col_queue.x + layout.col_queue.width - 1)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>();
            if row_str.contains("1.") && row_str.contains("FIRST QUEUE") {
                found_first = true;
            }
            if row_str.contains("2.") && row_str.contains("SECOND QUEUE") {
                found_second = true;
            }
        }
        assert!(
            found_first,
            "Queue list must render first queued track with position"
        );
        assert!(
            found_second,
            "Queue list must render second queued track with position"
        );

        // Focus highlight must be ONLY on the focused panel
        let search_selected_cell = &buffer[(layout.search_area.x + 2, layout.search_area.y + 1)];
        let queue_selected_cell = &buffer[(layout.col_queue.x + 2, layout.col_queue.y + 1)];

        assert_eq!(
            queue_selected_cell.bg,
            theme.highlight.bg.unwrap(),
            "Focused Queue panel must have the highlight box on selected item"
        );
        assert_ne!(
            search_selected_cell.bg,
            theme.highlight.bg.unwrap(),
            "Unfocused Search panel must NOT have the highlight box on selected item"
        );
        assert_eq!(
            search_selected_cell.fg,
            theme.dim.fg.unwrap(),
            "Unfocused Search panel must render selected item in dim text without highlight"
        );
    }

    #[test]
    fn test_render_player_narrow_fallback_medium_75_to_99() {
        use crate::music::Track;

        let mut app = App::new();
        let theme = Theme::new(None);
        app.enqueue(Track {
            title: "Queued Item".into(),
            artist: "Artist".into(),
            stream_url: "http://stream/q".into(),
            art_url: "".into(),
            spotify_id: None,
            artists: vec!["Artist".into()],
            album: None,
            duration_ms: None,
        });

        let area = Rect::new(0, 0, 85, 30);
        let layout = layout_player(area);
        assert_eq!(layout.col_queue.width, 0, "Queue panel hidden at 85 cols");

        // With Search focus, left area renders SEARCH RESULTS
        app.list_focus = ListFocus::Search;
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_player(frame, area, &app, None, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        let left_title_row = (layout.search_area.x
            ..layout.search_area.x + layout.search_area.width)
            .map(|x| buffer[(x, layout.search_area.y)].symbol())
            .collect::<String>();
        assert!(
            left_title_row.contains("SEARCH"),
            "Left panel must show SEARCH when focused at 85 cols, got: {left_title_row}"
        );

        // With Queue focus, left area switches to UP NEXT (1)
        app.list_focus = ListFocus::Queue;
        terminal
            .draw(|frame| {
                render_player(frame, area, &app, None, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        let left_title_row = (layout.search_area.x
            ..layout.search_area.x + layout.search_area.width)
            .map(|x| buffer[(x, layout.search_area.y)].symbol())
            .collect::<String>();
        assert!(
            left_title_row.contains("UP NEXT (1)"),
            "Left panel must show UP NEXT (1) when queue is focused at 85 cols, got: {left_title_row}"
        );
    }

    #[test]
    fn test_render_player_compact_fallback_40_to_74() {
        use crate::music::Track;

        let mut app = App::new();
        let theme = Theme::new(None);
        app.enqueue(Track {
            title: "Compact Queue Song".into(),
            artist: "Artist".into(),
            stream_url: "http://stream/cq".into(),
            art_url: "".into(),
            spotify_id: None,
            artists: vec!["Artist".into()],
            album: None,
            duration_ms: None,
        });

        let area = Rect::new(0, 0, 50, 24);
        let layout = layout_player(area);
        assert_eq!(layout.col_queue.width, 0, "Queue panel hidden at 50 cols");

        // With Search focus, lower area shows SEARCH
        app.list_focus = ListFocus::Search;
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_player(frame, area, &app, None, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        let lower_title_row = (layout.search_area.x
            ..layout.search_area.x + layout.search_area.width)
            .map(|x| buffer[(x, layout.search_area.y)].symbol())
            .collect::<String>();
        assert!(
            lower_title_row.contains("SEARCH"),
            "Lower panel must show SEARCH when focused in compact mode, got: {lower_title_row}"
        );

        // With Queue focus, lower area switches to UP NEXT (1)
        app.list_focus = ListFocus::Queue;
        terminal
            .draw(|frame| {
                render_player(frame, area, &app, None, &theme);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        let lower_title_row = (layout.search_area.x
            ..layout.search_area.x + layout.search_area.width)
            .map(|x| buffer[(x, layout.search_area.y)].symbol())
            .collect::<String>();
        assert!(
            lower_title_row.contains("UP NEXT (1)"),
            "Lower panel must switch to UP NEXT (1) when focused in compact mode, got: {lower_title_row}"
        );
    }
}
