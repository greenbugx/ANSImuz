use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, Mode};
use crate::ui::components::{
    controls::render_controls,
    logo::{bitmap_lines, logo_bitmap, scale_bitmap},
    progress::render_progress_bar,
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
    let (col_left, col_player, col_queue) = if area.width >= 100 {
        let cols = Layout::horizontal([
            Constraint::Percentage(28),
            Constraint::Percentage(50),
            Constraint::Percentage(22),
        ])
        .split(area);
        (cols[0], cols[1], cols[2])
    } else {
        let middle_w = 50.min(area.width);
        let remaining = area.width.saturating_sub(middle_w);
        let left_w = remaining * 3 / 5;
        let right_w = remaining.saturating_sub(left_w);
        let cols = Layout::horizontal([
            Constraint::Length(left_w),
            Constraint::Length(middle_w),
            Constraint::Length(right_w),
        ])
        .split(area);
        (cols[0], cols[1], cols[2])
    };

    let keybinds_height = (crate::app::commands::PANEL_KEYBINDS.len() as u16 + 2)
        .min(col_left.height.saturating_sub(6));
    let left_sections =
        Layout::vertical([Constraint::Min(6), Constraint::Length(keybinds_height)]).split(col_left);
    let search_area = left_sections[0];
    let keybinds_area = left_sections[1];

    let player_block = Block::default().borders(Borders::ALL);
    let player_inner = player_block.inner(col_player);

    let bitmap = logo_bitmap();
    let art_width = bitmap.first().map_or(0, |row| row.len());
    let art_height = bitmap.len();

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
        Constraint::Min(6),                                          // 3: Artwork + Title + Artist
        Constraint::Length(1), // 4: Volume (left) & Timer (right)
        Constraint::Length(1), // 5: Spacer gap
        Constraint::Length(1), // 6: Progress bar
        Constraint::Length(1), // 7: Controls
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

    // LEFT SECTION: Search Results (top) and Keybinds (bottom)
    let search_title = if app.last_query.is_empty() {
        format!(" SEARCH RESULTS ({}) ", app.settings.provider.label())
    } else {
        format!(
            " RESULTS: {} ({}) ",
            truncate(&app.last_query, 14),
            app.settings.provider.label()
        )
    };

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(
            search_title,
            theme.bold_primary,
        )]));
    let search_inner = search_block.inner(layout.search_area);
    frame.render_widget(search_block, layout.search_area);

    render_track_list(
        frame,
        search_inner,
        &app.tracks,
        app.selected,
        app.loading,
        &app.last_query,
        theme,
    );

    let keybinds_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(
            " KEYBINDS ",
            theme.bold_primary,
        )]));
    let keybinds_inner = keybinds_block.inner(layout.keybinds_area);
    frame.render_widget(keybinds_block, layout.keybinds_area);

    let keybind_lines: Vec<Line> = crate::app::commands::PANEL_KEYBINDS
        .iter()
        .map(|(key, action)| {
            Line::from(vec![
                Span::styled(format!(" {:<7}", key), theme.bold_primary),
                Span::styled(*action, theme.dim),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(keybind_lines), keybinds_inner);

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
        let scaled = scale_bitmap(&bitmap, layout.logo_width, layout.logo_px);
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

    render_volume_timer(
        frame,
        layout.vol_timer_area,
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
    // TODO: add a persistent playback queue
    let queue_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(
            " QUEUE ",
            theme.bold_primary,
        )]));
    let queue_inner = queue_block.inner(layout.col_queue);
    frame.render_widget(queue_block, layout.col_queue);

    frame.render_widget(
        Paragraph::new("Yet to be in service :)")
            .style(theme.dim)
            .alignment(Alignment::Center),
        queue_inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_player_breakpoint_wide() {
        let area = Rect::new(0, 0, 120, 30);
        let layout = layout_player(area);

        assert!(area.width >= 100);
        assert!(layout.col_left.width > 0);
        assert!(layout.col_player.width >= 50);
        assert!(layout.col_queue.width > 0);
        assert_eq!(
            layout.col_left.width + layout.col_player.width + layout.col_queue.width,
            area.width
        );
    }

    #[test]
    fn test_layout_player_breakpoint_narrow() {
        let area = Rect::new(0, 0, 80, 30);
        let layout = layout_player(area);

        assert!(area.width < 100);
        assert_eq!(layout.col_player.width, 50);
        assert_eq!(
            layout.col_left.width + layout.col_player.width + layout.col_queue.width,
            area.width
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
        let area = Rect::new(0, 0, 120, 30);
        let layout = layout_player(area);

        assert!(layout.col_left.x + layout.col_left.width <= layout.col_player.x);
        assert!(layout.col_player.x + layout.col_player.width <= layout.col_queue.x);
    }
}
