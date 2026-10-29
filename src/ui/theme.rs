use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: Color,
    pub primary: Style,
    pub bold_primary: Style,
    pub dim: Style,
    pub highlight: Style,
}

impl Theme {
    pub fn new(palette_color: Option<(u8, u8, u8)>) -> Self {
        let accent = match palette_color {
            Some((r, g, b)) => Color::Rgb(r, g, b),
            None => Color::Green,
        };

        let primary = Style::default().fg(accent);
        let bold_primary = primary.add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(Color::DarkGray);
        let highlight = Style::default()
            .fg(Color::Black)
            .bg(accent)
            .add_modifier(Modifier::BOLD);

        Self {
            accent,
            primary,
            bold_primary,
            dim,
            highlight,
        }
    }
}

pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if text.chars().count() <= max {
        return text.to_string();
    }

    format!(
        "{}…",
        text.chars().take(max.saturating_sub(1)).collect::<String>()
    )
}
