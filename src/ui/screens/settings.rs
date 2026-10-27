use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::config::Settings;

pub fn render_settings(frame: &mut Frame, area: Rect, settings: &Settings, cursor: usize) {
    let green = Style::default().fg(Color::Green);
    let bold_green = green.add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let cache_state = if settings.audio_cache { "ON" } else { "OFF" };
    let cache_style = if settings.audio_cache {
        bold_green
    } else {
        dim
    };

    let palette_state = if settings.art_palette { "ON" } else { "OFF" };
    let palette_style = if settings.art_palette {
        bold_green
    } else {
        dim
    };

    let mode = if settings.art_hq { "HQ" } else { "PIXEL" };
    let mode_style = if settings.art_hq { bold_green } else { dim };

    let cache_size = crate::config::cache_bytes();
    let size_text = if cache_size > 0 {
        crate::config::human_size(cache_size)
    } else {
        "EMPTY".to_string()
    };
    let size_style = if cache_size > 0 { bold_green } else { dim };

    let setting_rows = vec![
        (
            "Audio Cache:".to_string(),
            cache_state.to_string(),
            cache_style,
            "Save played songs to /tmp for instant replays".to_string(),
        ),
        (
            "Art Mode:".to_string(),
            mode.to_string(),
            mode_style,
            "HQ uses true graphics, pixel uses unicode blocks".to_string(),
        ),
        (
            "Art Palette:".to_string(),
            palette_state.to_string(),
            palette_style,
            "Tint the selected track with the artwork colors".to_string(),
        ),
        (
            "Clear Caches:".to_string(),
            size_text,
            size_style,
            "Removes cached artwork and audio".to_string(),
        ),
    ];

    let cursor = cursor.min(setting_rows.len() - 1);
    let block_height = setting_rows.len() as u16 + 4;
    let top = area.y + (area.height - block_height) / 2;

    let name_width = setting_rows
        .iter()
        .map(|(name, value, ..)| name.chars().count() + value.chars().count() + 1)
        .max()
        .unwrap_or(0) as u16;
    let description_width = setting_rows
        .iter()
        .map(|(.., description)| description.chars().count())
        .max()
        .unwrap_or(0) as u16;

    let table_width = (name_width + 2 + description_width).min(area.width);
    let table_x = area.x + (area.width - table_width) / 2;

    frame.render_widget(
        Paragraph::new("SETTINGS")
            .style(bold_green)
            .alignment(Alignment::Center),
        Rect {
            x: area.x,
            y: top,
            width: area.width,
            height: 1,
        },
    );

    for (offset, (name, value, value_style, description)) in setting_rows.iter().enumerate() {
        let active = offset == cursor;
        let row = Rect {
            x: table_x,
            y: top + 2 + offset as u16,
            width: table_width,
            height: 1,
        };

        let columns = Layout::horizontal([
            Constraint::Length(name_width),
            Constraint::Length(2),
            Constraint::Min(description_width),
        ])
        .split(row);

        let cell_style = if active {
            Style::default().bg(Color::Green)
        } else {
            Style::default()
        };

        let name_style = if active {
            Style::default().fg(Color::Black)
        } else {
            green
        };

        let value_style = if active {
            Style::default()
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            *value_style
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(name.as_str(), name_style),
                Span::raw(" "),
                Span::styled(value.as_str(), value_style),
            ]))
            .style(cell_style)
            .alignment(Alignment::Right),
            columns[0],
        );

        let description_style = if active {
            Style::default().fg(Color::Black)
        } else {
            dim
        };

        frame.render_widget(
            Paragraph::new(description.as_str()).style(description_style),
            columns[2],
        );
    }

    frame.render_widget(
        Paragraph::new("↑↓ SELECT   SPACE TOGGLE   ESC BACK")
            .style(dim)
            .alignment(Alignment::Center),
        Rect {
            x: area.x,
            y: top + block_height - 1,
            width: area.width,
            height: 1,
        },
    );
}
