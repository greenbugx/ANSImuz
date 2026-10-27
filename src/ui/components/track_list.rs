use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{List, ListItem, ListState},
};

use crate::music::Track;
use crate::ui::theme::truncate;

pub fn render_track_list(
    frame: &mut Frame,
    area: Rect,
    tracks: &[Track],
    selected_idx: usize,
    loading: bool,
    last_query: &str,
    selected_style: Style,
    green: Style,
    dim: Style,
) {
    let items: Vec<ListItem> = if tracks.is_empty() {
        let message = if !loading && !last_query.is_empty() {
            Some("NO RESULTS")
        } else {
            None
        };

        message
            .map(|m| ListItem::new(Line::from(m).centered().style(dim)))
            .into_iter()
            .collect()
    } else {
        tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let title = truncate(&track.title, 26).to_uppercase();
                let artist = truncate(&track.artist, 14).to_uppercase();
                let label = format!(" {:>2}. {} | {}", i + 1, title, artist);

                ListItem::new(label).style(if i == selected_idx {
                    selected_style
                } else {
                    green
                })
            })
            .collect()
    };

    let mut list_state = ListState::default();
    list_state.select(Some(selected_idx));

    frame.render_stateful_widget(List::new(items), area, &mut list_state);
}
