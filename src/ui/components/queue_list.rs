use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::app::QueueEntry;
use crate::ui::theme::{Theme, truncate};

pub fn render_queue_list(
    frame: &mut Frame,
    area: Rect,
    queue: &[QueueEntry],
    selected_idx: usize,
    is_focused: bool,
    theme: &Theme,
) {
    if area.width < 3 || area.height == 0 {
        return;
    }

    if queue.is_empty() {
        let lines = if area.height >= 4 && area.width >= 16 {
            if is_focused {
                vec![
                    Line::from("Queue is empty").centered().style(theme.dim),
                    Line::from("Tab → Search").centered().style(theme.dim),
                    Line::from("A → Add to queue").centered().style(theme.dim),
                ]
            } else {
                vec![
                    Line::from("Queue is empty").centered().style(theme.dim),
                    Line::from("Press 'A' to add").centered().style(theme.dim),
                ]
            }
        } else if area.height >= 3 && area.width >= 16 {
            vec![
                Line::from("Queue is empty").centered().style(theme.dim),
                Line::from(if is_focused {
                    "Tab → Search | A → Add"
                } else {
                    "Press 'A' to add"
                })
                .centered()
                .style(theme.dim),
            ]
        } else if area.width >= 14 {
            vec![Line::from("Queue is empty").centered().style(theme.dim)]
        } else if area.width >= 7 {
            vec![Line::from("Empty").centered().style(theme.dim)]
        } else {
            vec![Line::from("…").centered().style(theme.dim)]
        };

        let y_offset = area.height.saturating_sub(lines.len() as u16) / 2;
        let centered_area = Rect {
            x: area.x,
            y: area.y + y_offset,
            width: area.width,
            height: area.height.saturating_sub(y_offset),
        };
        frame.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            centered_area,
        );
        return;
    }

    let available = (area.width as usize).saturating_sub(6);
    let title_w = (available * 3 / 5).max(6);
    let artist_w = available.saturating_sub(title_w + 3).max(4);

    let items: Vec<ListItem> = queue
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let title = truncate(&entry.track.title, title_w).to_uppercase();
            let artist = truncate(&entry.track.artist, artist_w).to_uppercase();
            let label = if available > 18 && !artist.is_empty() {
                format!(" {:>2}. {} | {}", i + 1, title, artist)
            } else {
                format!(" {:>2}. {}", i + 1, title)
            };

            let style = if is_focused {
                if i == selected_idx {
                    theme.highlight
                } else {
                    theme.primary
                }
            } else {
                theme.dim
            };

            ListItem::new(label).style(style)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(selected_idx.min(queue.len() - 1)));

    frame.render_stateful_widget(List::new(items), area, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_empty_queue_vertically_centered() {
        let theme = Theme::new(None);
        let backend = TestBackend::new(30, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_queue_list(frame, frame.area(), &[], 0, false, &theme);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Row 0 should NOT contain "Queue is empty" because it is vertically centered
        let row_0 = (0..30).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
        assert!(
            !row_0.contains("Queue is empty"),
            "Top row 0 must not contain empty text when centered, got: {row_0}"
        );

        // Center row (around row 4) should contain "Queue is empty"
        let center_row = (0..30).map(|x| buffer[(x, 4)].symbol()).collect::<String>();
        assert!(
            center_row.contains("Queue is empty"),
            "Center row 4 must contain 'Queue is empty', got: {center_row}"
        );
    }
}
