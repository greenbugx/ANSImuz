use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::ui::theme::Theme;

pub fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup_width = 58.min(area.width.saturating_sub(4));
    let popup_height = 20.min(area.height.saturating_sub(2));

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
        .title(
            Line::from(vec![Span::styled(
                " HELP — KEYBOARD SHORTCUTS ",
                theme.bold_primary,
            )])
            .centered(),
        );

    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "PLAYBACK",
            &[
                ("Space", "Play / Pause playback"),
                ("← / →", "Previous / Next track"),
                ("x", "Toggle shuffle mode"),
                ("r", "Cycle repeat (Off / All / One)"),
                ("+ / -", "Volume up / down (±5%)"),
            ],
        ),
        (
            "NAVIGATION",
            &[
                ("↑ / ↓", "Select track in list"),
                ("Enter", "Play selected track"),
                ("/", "Focus song search input"),
                ("Esc", "Clear search query / Close dialog"),
            ],
        ),
        (
            "SYSTEM",
            &[
                (": / Ctrl+P", "Open command palette"),
                ("s", "Open settings screen"),
                ("?", "Toggle this help overlay"),
                ("q / Ctrl+C", "Quit ANSImuz"),
            ],
        ),
    ];

    let mut lines = Vec::new();

    for (cat_name, items) in sections {
        lines.push(Line::from(vec![Span::styled(
            format!(" [{cat_name}]"),
            theme.bold_primary,
        )]));

        for (shortcut, desc) in *items {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(format!("{shortcut:<12}"), theme.bold_primary),
                Span::styled(*desc, theme.dim),
            ]));
        }

        lines.push(Line::raw(""));
    }

    lines.push(Line::from(vec![Span::styled(
        "Press ESC, ?, or Q to close",
        theme.dim,
    )]));

    let content_layout = Layout::vertical([Constraint::Min(1)]).split(inner);

    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Left),
        content_layout[0],
    );
}
