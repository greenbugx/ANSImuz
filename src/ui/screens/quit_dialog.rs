use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::state::QuitChoice;
use crate::ui::theme::Theme;

pub fn render_quit_dialog(frame: &mut Frame, area: Rect, choice: QuitChoice, theme: &Theme) {
    let dialog_w = 46.min(area.width.saturating_sub(4));
    let dialog_h = 9.min(area.height.saturating_sub(2));

    if dialog_w < 20 || dialog_h < 5 {
        return;
    }

    let rect = Rect {
        x: area.x + (area.width.saturating_sub(dialog_w)) / 2,
        y: area.y + (area.height.saturating_sub(dialog_h)) / 2,
        width: dialog_w,
        height: dialog_h,
    };

    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.primary)
        .title(Line::from(vec![Span::styled(" QUIT ", theme.bold_primary)]).centered());

    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let yes_style = if choice == QuitChoice::Yes {
        theme.highlight
    } else {
        theme.dim
    };

    let no_style = if choice == QuitChoice::No {
        theme.highlight
    } else {
        theme.dim
    };

    let mut lines = Vec::new();

    if inner.height >= 5 {
        lines.push(Line::raw(""));
    }

    lines.push(
        Line::from(vec![Span::styled(
            "Are you sure you want to quit?",
            theme.bold_primary,
        )])
        .centered(),
    );

    lines.push(Line::raw(""));

    lines.push(
        Line::from(vec![
            Span::styled("  Yes  ", yes_style),
            Span::raw("    "),
            Span::styled("  No  ", no_style),
        ])
        .centered(),
    );

    if inner.height >= 6 {
        lines.push(Line::raw(""));
        lines.push(
            Line::from(vec![Span::styled(
                "←/→ Select  •  Enter Confirm  •  Esc Cancel",
                theme.dim,
            )])
            .centered(),
        );
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_quit_dialog_yes_and_no() {
        let theme = Theme::new(None);

        // Test Yes selected
        let backend = TestBackend::new(50, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_quit_dialog(frame, frame.area(), QuitChoice::Yes, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = (0..15)
            .flat_map(|y| (0..50).map(move |x| buffer[(x, y)].symbol()))
            .collect::<String>();

        assert!(content.contains("QUIT"), "Title must render");
        assert!(
            content.contains("Are you sure you want to quit?"),
            "Prompt must render"
        );
        assert!(content.contains("Yes"), "Yes option must render");
        assert!(content.contains("No"), "No option must render");

        // Test No selected
        terminal
            .draw(|frame| {
                render_quit_dialog(frame, frame.area(), QuitChoice::No, &theme);
            })
            .unwrap();

        let buffer_no = terminal.backend().buffer();
        let content_no = (0..15)
            .flat_map(|y| (0..50).map(move |x| buffer_no[(x, y)].symbol()))
            .collect::<String>();
        assert!(content_no.contains("Yes") && content_no.contains("No"));
    }
}
