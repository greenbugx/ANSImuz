use ratatui::style::{Color, Modifier, Style};

pub fn green() -> Style {
    Style::default().fg(Color::Green)
}

pub fn bold_green() -> Style {
    green().add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }

    format!(
        "{}…",
        text.chars().take(max.saturating_sub(1)).collect::<String>()
    )
}
