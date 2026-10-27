use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::ui::theme::format_time;

pub fn render_header(
    frame: &mut Frame,
    area: Rect,
    volume: u8,
    playback: Option<(f64, Option<f64>)>,
    primary: Style,
    bold_primary: Style,
) {
    let filled = (volume as usize).min(100) / 10;
    let bars = format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled));
    let clock = playback
        .map(|(elapsed, _)| format_time(elapsed))
        .unwrap_or_else(|| "--:--".into());

    let line = Line::from(vec![
        Span::styled(format!("VOL {bars} "), primary),
        Span::styled(clock, bold_primary),
    ]);

    frame.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);
}
