pub mod components;
pub mod screens;
pub mod theme;

use ratatui::{Frame, layout::Alignment, widgets::Paragraph};

use crate::app::App;
use screens::{
    command_palette::render_command_palette, help::render_help, player::render_player,
    settings::render_settings,
};

pub fn render(app: &App, frame: &mut Frame, playback: Option<(f64, Option<f64>)>) {
    let area = frame.area();

    if area.width < 40 || area.height < 24 {
        frame.render_widget(
            Paragraph::new("TERMINAL TOO SMALL").alignment(Alignment::Center),
            area,
        );
        return;
    }

    if app.settings_open {
        render_settings(frame, area, &app.settings, app.settings_cursor);
    } else {
        render_player(frame, area, app, playback);
    }

    if app.help_open {
        render_help(frame, area);
    } else if app.command_palette_open {
        render_command_palette(
            frame,
            area,
            &app.command_palette_query,
            app.command_palette_cursor,
        );
    }
}
