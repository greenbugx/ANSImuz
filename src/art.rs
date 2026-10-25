use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_art_placeholder(frame: &mut Frame, area: Rect) {
    if area.width < 4 || area.height < 3 {
        return;
    }

    let width = (area.height * 2).min(area.width - 2).max(4);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y,
        width,
        height: area.height,
    };

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green)),
        rect,
    );

    let note = Rect {
        x: rect.x,
        y: rect.y + rect.height / 2,
        width: rect.width,
        height: 1,
    };

    frame.render_widget(
        Paragraph::new(Line::from("♪").centered()).style(Style::default().fg(Color::Green)),
        note,
    );
}
