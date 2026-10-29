use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, Mode};
use crate::ui::components::{
    controls::render_controls,
    header::render_header,
    logo::{bitmap_lines, logo_bitmap, scale_bitmap},
    progress::render_progress_bar,
    search_box::render_search_box,
    track_list::render_track_list,
};
use crate::ui::theme::{Theme, truncate};

pub fn render_player(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    playback: Option<(f64, Option<f64>)>,
    theme: &Theme,
) {
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

    // RIGHT SECTION: Queue 
    // TODO: add a persistant playback queue
    let queue_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(
            " QUEUE ",
            theme.bold_primary,
        )]));
    let queue_inner = queue_block.inner(col_queue);
    frame.render_widget(queue_block, col_queue);

    let queue_layout = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(queue_inner);

    frame.render_widget(
        Paragraph::new("Yet to be in service :)")
            .style(theme.dim)
            .alignment(Alignment::Center),
        queue_layout[1],
    );

    // RIGHT SECTION: Search Results (top) and Keybinds (bottom)
    let keybind_rows = [
        ("↑↓", "SELECT"),
        ("ENTER", "PLAY"),
        ("SPACE", "PAUSE"),
        ("←→", "TRACK"),
        ("X", "SHUFFLE"),
        ("R", "REPEAT"),
        (":", "COMMANDS"),
        ("?", "HELP"),
        ("Q", "QUIT"),
    ];

    let keybinds_height = (keybind_rows.len() as u16 + 2).min(col_left.height.saturating_sub(6));
    let left_sections =
        Layout::vertical([Constraint::Min(6), Constraint::Length(keybinds_height)]).split(col_left);

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
    let search_inner = search_block.inner(left_sections[0]);
    frame.render_widget(search_block, left_sections[0]);

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
    let keybinds_inner = keybinds_block.inner(left_sections[1]);
    frame.render_widget(keybinds_block, left_sections[1]);

    let keybind_lines: Vec<Line> = keybind_rows
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
    let player_inner = player_block.inner(col_player);
    frame.render_widget(player_block, col_player);

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
        Constraint::Length(if show_logo { logo_height } else { 0 }), // Logo
        Constraint::Length(3),                                       // Search box
        Constraint::Length(1),                                       // Status
        Constraint::Min(6),                                          // Artwork + Title + Artist
        Constraint::Length(1), // Volume (left) & Timer (right)
        Constraint::Length(1), // Spacer gap
        Constraint::Length(1), // Progress bar
        Constraint::Length(1), // Controls
    ])
    .split(player_inner);

    let current = app
        .now_playing
        .as_ref()
        .or(app.pending_track.as_ref())
        .or_else(|| app.tracks.get(app.selected));

    if show_logo {
        let scaled = scale_bitmap(&bitmap, target_width, target_px);
        frame.render_widget(
            Paragraph::new(bitmap_lines(&scaled))
                .style(theme.bold_primary)
                .alignment(Alignment::Center),
            player_rows[0],
        );
    }

    render_search_box(
        frame,
        player_rows[1],
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
        player_rows[2],
    );

    crate::artwork::render_art(
        frame,
        player_rows[3],
        current.map(|track| track.art_url.as_str()).unwrap_or(""),
        current.map(|track| track.title.as_str()).unwrap_or(""),
        current.map(|track| track.artist.as_str()).unwrap_or(""),
        app.settings.art_hq,
        Some(theme.accent),
    );

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

    render_header(
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
        bar_area,
        bar_width,
        song_loading,
        playback,
        theme.bold_primary,
        theme.dim,
    );

    let cluster_width = 45.min(player_rows[7].width);
    let cluster = Rect {
        x: player_rows[7].x + (player_rows[7].width - cluster_width) / 2,
        y: player_rows[7].y,
        width: cluster_width,
        height: 1,
    };

    let slot_w = cluster_width / 5;
    let slots = Layout::horizontal([
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
        Constraint::Length(slot_w),
    ])
    .split(cluster);

    render_controls(
        frame,
        &slots,
        app.shuffle,
        app.paused,
        app.repeat,
        theme.bold_primary,
        theme.dim,
    );
}
