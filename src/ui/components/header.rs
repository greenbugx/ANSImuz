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

    let vol_line = Line::from(vec![
        Span::styled("VOL ", primary),
        Span::styled(bars, bold_primary),
    ]);
    frame.render_widget(Paragraph::new(vol_line).alignment(Alignment::Left), area);

    let time_line = Line::from(Span::styled(clock, bold_primary));
    frame.render_widget(Paragraph::new(time_line).alignment(Alignment::Right), area);
}
