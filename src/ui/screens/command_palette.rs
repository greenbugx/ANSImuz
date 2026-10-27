use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::commands::filtered_commands;
use crate::ui::theme::Theme;

pub fn render_command_palette(
    frame: &mut Frame,
    area: Rect,
    query: &str,
    cursor: usize,
    theme: &Theme,
) {
    let popup_width = 62.min(area.width.saturating_sub(4));
    let popup_height = 16.min(area.height.saturating_sub(2));

    let rect = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(" COMMAND PALETTE ", theme.bold_primary)]).centered());

    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(inner);

    let search_line = Line::from(vec![
        Span::styled("> ", theme.bold_primary),
        if query.is_empty() {
            Span::styled("Type a command...", theme.dim)
        } else {
            Span::styled(query, theme.bold_primary)
        },
        Span::styled("▌", theme.primary),
    ]);
    frame.render_widget(Paragraph::new(search_line), rows[0]);

    let sep = "─".repeat(rows[1].width as usize);
    frame.render_widget(Paragraph::new(sep).style(theme.dim), rows[1]);

    let commands = filtered_commands(query);
    let visible_capacity = rows[2].height as usize;

    if commands.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching commands")
                .style(theme.dim)
                .alignment(Alignment::Center),
            rows[2],
        );
    } else {
        let selected = cursor.min(commands.len().saturating_sub(1));

        let start = if selected >= visible_capacity {
            selected + 1 - visible_capacity
        } else {
            0
        };
        let end = (start + visible_capacity).min(commands.len());

        let mut lines = Vec::new();

        for (i, cmd) in commands[start..end].iter().enumerate() {
            let actual_idx = start + i;
            let is_active = actual_idx == selected;

            let row_style = if is_active {
                theme.highlight
            } else {
                Style::default().fg(Color::White)
            };

            let shortcut_style = if is_active {
                Style::default().fg(Color::Black).bg(theme.accent)
            } else {
                theme.primary
            };

            let name_width = 22;
            let name_str = format!("{:<width$}", cmd.name, width = name_width);

            let shortcut_str = if !cmd.shortcut.is_empty() {
                format!("[{}]", cmd.shortcut)
            } else {
                String::new()
            };

            let available_desc =
                (rows[2].width as usize).saturating_sub(name_width + shortcut_str.len() + 4);
            let desc_str = if cmd.description.len() > available_desc {
                format!("{}…", &cmd.description[..available_desc.saturating_sub(1)])
            } else {
                cmd.description.to_string()
            };

            lines.push(Line::from(vec![
                Span::styled(if is_active { " > " } else { "   " }, row_style),
                Span::styled(name_str, if is_active { row_style } else { theme.primary }),
                Span::styled(
                    format!("{:<width$}", desc_str, width = available_desc),
                    if is_active { row_style } else { theme.dim },
                ),
                Span::styled(
                    format!("{:>width$}", shortcut_str, width = shortcut_str.len() + 1),
                    shortcut_style,
                ),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), rows[2]);
    }

    frame.render_widget(
        Paragraph::new("↑↓ Navigate   Enter Execute   Esc Close")
            .style(theme.dim)
            .alignment(Alignment::Center),
        rows[3],
    );
}
