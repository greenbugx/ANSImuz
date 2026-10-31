use std::sync::LazyLock;

use image::DynamicImage;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::artwork::render_static_image;
use crate::ui::theme::Theme;

static LOGO_IMAGE: LazyLock<Option<DynamicImage>> = LazyLock::new(|| {
    let path = crate::paths::data_dir().join("assets/logo_nobg.png");
    let bytes = std::fs::read(path).ok()?;
    image::load_from_memory(&bytes).ok()
});

pub fn render_about(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup_width = 64.min(area.width.saturating_sub(4));
    let popup_height = 21.min(area.height.saturating_sub(2));

    if popup_width < 10 || popup_height < 6 {
        return;
    }

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
        .title(Line::from(vec![Span::styled(" ABOUT ANSIMUZ ", theme.bold_primary)]).centered());

    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();

    if inner.width >= 55 {
        lines.push(Line::from(vec![Span::styled(
            "A lightweight fast TUI to stream your favourite music",
            theme.primary,
        )]));
        lines.push(Line::from(vec![Span::styled(
            "with pure 8-bit ANSI vibes",
            theme.primary,
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "A lightweight fast TUI to",
            theme.primary,
        )]));
        lines.push(Line::from(vec![Span::styled(
            "stream your favourite music",
            theme.primary,
        )]));
        lines.push(Line::from(vec![Span::styled(
            "with pure 8-bit ANSI vibes",
            theme.primary,
        )]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Version: ", theme.dim),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            theme.bold_primary,
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("License: ", theme.dim),
        Span::styled("MIT", theme.bold_primary),
    ]));

    if inner.width >= 48 {
        lines.push(Line::from(vec![
            Span::styled("Source:  ", theme.dim),
            Span::styled("https://github.com/greenbugx/ANSImuz/", theme.bold_primary),
        ]));
    } else if inner.width >= 38 {
        lines.push(Line::from(vec![
            Span::styled("Source: ", theme.dim),
            Span::styled("github.com/greenbugx/ANSImuz", theme.bold_primary),
        ]));
    } else {
        lines.push(Line::from(vec![Span::styled("Source: ", theme.dim)]));
        lines.push(Line::from(vec![Span::styled(
            "github.com/greenbugx/ANSImuz",
            theme.bold_primary,
        )]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![Span::styled(
        "Press '?' or 'Esc' to close",
        theme.dim,
    )]));

    let text_lines_count = lines.len() as u16;
    let available_for_logo = inner.height.saturating_sub(text_lines_count + 1);

    let logo_h = if available_for_logo >= 8 && inner.width >= 50 {
        8
    } else if available_for_logo >= 6 {
        6
    } else if available_for_logo >= 4 {
        4
    } else {
        0
    };

    let logo_w = (logo_h * 2).min(inner.width);
    let spacing = if logo_h > 0 { 1 } else { 0 };
    let total_used = logo_h + spacing + text_lines_count;
    let y_pad = (inner.height.saturating_sub(total_used)) / 2;

    if logo_h > 0
        && let Some(logo) = LOGO_IMAGE.as_ref()
    {
        let logo_rect = Rect {
            x: inner.x + (inner.width.saturating_sub(logo_w)) / 2,
            y: inner.y + y_pad,
            width: logo_w,
            height: logo_h,
        };
        render_static_image(frame, logo_rect, logo);
    }

    let text_y = inner.y + y_pad + logo_h + spacing;
    let text_h = inner.height.saturating_sub(text_y - inner.y);
    let text_rect = Rect {
        x: inner.x,
        y: text_y,
        width: inner.width,
        height: text_h,
    };

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        text_rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_about_screen() {
        let theme = Theme::new(None);
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_about(frame, frame.area(), &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = (0..25)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol()))
            .collect::<String>();

        assert!(content.contains("ABOUT ANSIMUZ"), "Title must render");
        assert!(
            content.contains("A lightweight fast TUI to stream your favourite music"),
            "Description line 1 must render"
        );
        assert!(
            content.contains("with pure 8-bit ANSI vibes"),
            "Description line 2 must render"
        );
        assert!(
            content.contains(env!("CARGO_PKG_VERSION")),
            "Version must render"
        );
        assert!(content.contains("MIT"), "License must render");
        assert!(
            content.contains("https://github.com/greenbugx/ANSImuz/"),
            "Source link must render"
        );
    }

    #[test]
    fn test_render_about_screen_compact_40x24() {
        let theme = Theme::new(None);
        let backend = TestBackend::new(40, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_about(frame, frame.area(), &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = (0..24)
            .flat_map(|y| (0..40).map(move |x| buffer[(x, y)].symbol()))
            .collect::<String>();

        assert!(content.contains("ABOUT ANSIMUZ"), "Title must render");
        assert!(
            content.contains("stream your favourite music"),
            "Wrapped description must render without clipping"
        );
        assert!(
            content.contains("with pure 8-bit ANSI vibes"),
            "Description must render"
        );
        assert!(
            content.contains("github.com/greenbugx/ANSImuz"),
            "Compact source URL must render without clipping"
        );
        assert!(
            content.contains("Press '?' or 'Esc' to close"),
            "Close prompt must render without vertical clipping"
        );

        assert_eq!(buffer[(2, 1)].symbol(), "┌");
        assert_eq!(buffer[(37, 1)].symbol(), "┐");
        assert_eq!(buffer[(2, 21)].symbol(), "└");
        assert_eq!(buffer[(37, 21)].symbol(), "┘");

        for y in 2..21 {
            let row = (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>();
            assert_eq!(
                buffer[(2, y)].symbol(),
                "│",
                "Left border intact at row {y}: {row}"
            );
            assert_eq!(
                buffer[(37, y)].symbol(),
                "│",
                "Right border intact at row {y}: {row}"
            );
        }
    }
}
