use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::Paragraph,
};

use crate::ui::theme::format_time;

pub fn render_header(
    frame: &mut Frame,
    area: Rect,
    volume: u8,
    playback: Option<(f64, Option<f64>)>,
    green: Style,
) {
    let filled = (volume as usize).min(100) / 10;
    let bars = format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled));
    let clock = playback
        .map(|(elapsed, _)| format_time(elapsed))
        .unwrap_or_else(|| "--:--".into());

    frame.render_widget(
        Paragraph::new(format!("VOL {} {}", bars, clock))
            .style(green)
            .alignment(Alignment::Right),
        area,
    );
}
