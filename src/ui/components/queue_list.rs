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

        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
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

            let style = if i == selected_idx {
                if is_focused {
                    theme.highlight
                } else {
                    theme.bold_primary
                }
            } else if is_focused {
                theme.primary
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
