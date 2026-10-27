use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

pub fn render_progress_bar(
    frame: &mut Frame,
    bar_area: Rect,
    width: usize,
    song_loading: bool,
    playback: Option<(f64, Option<f64>)>,
    bold_green: Style,
    dim: Style,
) {
    if song_loading {
        frame.render_widget(
            Paragraph::new(format!(" {} ", crate::artwork::loading_moons()))
                .style(bold_green)
                .alignment(Alignment::Center),
            bar_area,
        );
    } else {
        let bar: Line = playback
            .and_then(|(elapsed, duration)| duration.filter(|d| *d > 0.0).map(|d| (elapsed, d)))
            .map(|(elapsed, duration)| {
                let ratio = (elapsed / duration).clamp(0.0, 1.0);
                let marker = ((width as f64) * ratio).round() as usize;

                Line::from(
                    (0..width)
                        .map(|i| {
                            if i == marker {
                                Span::styled("█", bold_green)
                            } else if i < marker {
                                Span::styled("▄", bold_green)
                            } else {
                                Span::styled("▄", dim)
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_else(|| Line::from(Span::styled("▄".repeat(width), dim)));

        frame.render_widget(Paragraph::new(bar), bar_area);
    }
}
