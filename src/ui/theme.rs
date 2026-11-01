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

pub fn truncate(text: &str, max_width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }

    let mut current_width = 0;
    let mut result = String::new();
    let target = max_width.saturating_sub(1);

    for c in text.chars() {
        let char_w = c.width().unwrap_or(0);
        if current_width + char_w > target {
            break;
        }
        current_width += char_w;
        result.push(c);
    }

    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn test_truncate_ascii() {
        assert_eq!(truncate("Hello World", 5), "Hell…");
        assert_eq!(truncate("Hello", 5), "Hello");
        assert_eq!(truncate("Hello", 10), "Hello");
        assert_eq!(truncate("", 5), "");
        assert_eq!(truncate("Hello", 0), "");
    }

    #[test]
    fn test_truncate_cjk_wide_characters() {
        // CJK characters have display width 2
        let japanese = "初音ミクの消失"; // 7 chars, display width 14
        assert_eq!(japanese.width(), 14);

        let truncated = truncate(japanese, 8);
        // Must fit within 8 display columns including '…' (width 1)
        assert!(truncated.width() <= 8);
        assert!(truncated.ends_with('…'));

        // 3 CJK chars = width 6, + '…' = width 7 <= 8
        assert_eq!(truncated, "初音ミ…");
    }
}
