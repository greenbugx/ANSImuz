use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::Paragraph,
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
use crate::ui::theme::Theme;

pub fn render_player(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    playback: Option<(f64, Option<f64>)>,
    theme: &Theme,
) {
    let bitmap = logo_bitmap();
    let art_width = bitmap.first().map_or(0, |row| row.len());
    let art_height = bitmap.len();
    let max_rows = (area.height as usize)
        .saturating_sub(24)
        .min(area.height as usize / 3);

    let mut target_width = ((area.width as usize).saturating_sub(4) * 3 / 5).min(art_width);
    let mut target_px = (art_height * target_width / art_width.max(1)).div_ceil(2) * 2;
    let max_px = max_rows.saturating_sub(1) * 2;

    if target_px > max_px {
        target_px = max_px;
        target_width = art_width * target_px / art_height.max(1);
    }

    let show_logo = target_width >= 40 && target_px >= 6;
    let logo_height = if show_logo {
        target_px as u16 / 2 + 1
    } else {
        0
    };

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(if show_logo { logo_height } else { 0 }),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let current = app
        .now_playing
        .as_ref()
        .or(app.pending_track.as_ref())
        .or_else(|| app.tracks.get(app.selected));

    render_header(
        frame,
        rows[0],
        app.volume,
        playback,
        theme.primary,
        theme.bold_primary,
    );

    if show_logo {
        let scaled = scale_bitmap(&bitmap, target_width, target_px);
        frame.render_widget(
            Paragraph::new(bitmap_lines(&scaled))
                .style(theme.bold_primary)
                .alignment(Alignment::Center),
            rows[1],
        );
    }

    render_search_box(
        frame,
        rows[2],
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
        rows[3],
    );

    render_track_list(
        frame,
        rows[5],
        &app.tracks,
        app.selected,
        app.loading,
        &app.last_query,
        theme,
    );

    crate::artwork::render_art(
        frame,
        rows[7],
        current.map(|track| track.art_url.as_str()).unwrap_or(""),
        current.map(|track| track.title.as_str()).unwrap_or(""),
        current.map(|track| track.artist.as_str()).unwrap_or(""),
        app.settings.art_hq,
        Some(theme.accent),
    );

    let bar_width = (rows[9].width * 3 / 5).max(20) as usize;
    let bar_area = Rect {
        x: rows[9].x + (rows[9].width - bar_width as u16) / 2,
        y: rows[9].y,
        width: bar_width as u16,
        height: 1,
    };

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

    let cluster = Rect {
        x: rows[10].x + rows[10].width.saturating_sub(45) / 2,
        y: rows[10].y,
        width: 45.min(rows[10].width),
        height: rows[10].height,
    };

    let slots = Layout::horizontal([
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
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

    frame.render_widget(
        Paragraph::new(
            "↑↓ SELECT  ENTER PLAY  SPACE PAUSE  ←→ TRACK  X SHUFFLE  R REPEAT  : COMMANDS  ? HELP  Q QUIT",
        )
        .style(theme.dim)
        .alignment(Alignment::Center),
        rows[12],
    );
}
