use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::Paragraph,
};

use crate::playback::Repeat;

pub fn render_controls(
    frame: &mut Frame,
    slots: &[Rect],
    shuffle: bool,
    paused: bool,
    repeat: Repeat,
    bold_green: Style,
    dim: Style,
) {
    let shuffle_icon = if shuffle {
        (">>>", bold_green)
    } else {
        (">>>", dim)
    };
    frame.render_widget(
        Paragraph::new(shuffle_icon.0)
            .style(shuffle_icon.1)
            .alignment(Alignment::Center),
        slots[0],
    );

    frame.render_widget(
        Paragraph::new("|◀")
            .style(bold_green)
            .alignment(Alignment::Center),
        slots[1],
    );

    let play_icon = if paused { "▶" } else { "▮▮" };
    frame.render_widget(
        Paragraph::new(play_icon)
            .style(bold_green)
            .alignment(Alignment::Center),
        slots[2],
    );

    frame.render_widget(
        Paragraph::new("▶|")
            .style(bold_green)
            .alignment(Alignment::Center),
        slots[3],
    );

    let repeat_icon = match repeat {
        Repeat::One => ("R1", bold_green),
        Repeat::All => ("RA", bold_green),
        Repeat::Off => ("R", dim),
    };
    frame.render_widget(
        Paragraph::new(repeat_icon.0)
            .style(repeat_icon.1)
            .alignment(Alignment::Center),
        slots[4],
    );
}
