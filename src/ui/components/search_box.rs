use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render_search_box(
    frame: &mut Frame,
    area: Rect,
    query: &str,
    focused: bool,
    green: Style,
    bold_green: Style,
    dim: Style,
) {
    let cursor = if focused { "▌" } else { " " };
    let placeholder = "Search songs...";

    let mut text = if query.is_empty() {
        placeholder.to_string()
    } else {
        query.to_string()
    };

    let min_inner = placeholder.chars().count() + 3;
    let max_inner = area.width.saturating_sub(6) as usize;
    let inner_w = (text.chars().count() + 3).max(min_inner).min(max_inner);

    let text_w = inner_w - 3;
    let count = text.chars().count();
    if count > text_w {
        text = format!("…{}", text.chars().skip(count - text_w).collect::<String>());
    }

    let box_w = inner_w as u16 + 4;
    let rect = Rect {
        x: area.x + (area.width - box_w) / 2,
        y: area.y,
        width: box_w,
        height: 3,
    };

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(if focused { green } else { dim }),
        rect,
    );

    let text_area = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: box_w - 4,
        height: 1,
    };

    let spans = if query.is_empty() {
        vec![
            Span::styled("> ", dim),
            Span::styled(text, dim),
            Span::styled(cursor, if focused { green } else { dim }),
        ]
    } else {
        vec![
            Span::raw("> "),
            Span::styled(text, bold_green),
            Span::styled(cursor, if focused { green } else { dim }),
        ]
    };

    frame.render_widget(Paragraph::new(Line::from(spans)), text_area);
}
