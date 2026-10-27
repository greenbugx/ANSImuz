pub mod components;
pub mod screens;
pub mod theme;

use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::app::App;
use screens::{
    command_palette::render_command_palette, help::render_help, player::render_player,
    settings::render_settings,
};

fn current_palette_color(app: &App) -> Option<(u8, u8, u8)> {
    if !app.settings.art_palette {
        return None;
    }

    let current = app
        .now_playing
        .as_ref()
        .or_else(|| app.tracks.get(app.selected));

    current.and_then(|track| crate::artwork::dominant_color(&track.art_url))
}

pub fn render(app: &App, frame: &mut Frame, playback: Option<(f64, Option<f64>)>) {
    let area = frame.area();

    if area.width < 40 || area.height < 24 {
        frame.render_widget(
            Paragraph::new("TERMINAL TOO SMALL").alignment(Alignment::Center),
            area,
        );
        return;
    }

    let palette = current_palette_color(app);
    let theme = theme::Theme::new(palette);

    if app.settings_open {
        render_settings(frame, area, &app.settings, app.settings_cursor, &theme);
    } else {
        render_player(frame, area, app, playback, &theme);
    }

    if app.help_open {
        render_help(frame, area, &theme);
    } else if app.command_palette_open {
        render_command_palette(
            frame,
            area,
            &app.command_palette_query,
            app.command_palette_cursor,
            &theme,
        );
    }
}
