use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{List, ListItem, ListState},
};

use crate::music::Track;
use crate::ui::theme::{Theme, truncate};

pub fn render_track_list(
    frame: &mut Frame,
    area: Rect,
    tracks: &[Track],
    selected_idx: usize,
    empty_msg: Option<&str>,
    is_focused: bool,
    theme: &Theme,
) {
    let items: Vec<ListItem> = if tracks.is_empty() {
        empty_msg
            .map(|m| ListItem::new(Line::from(m).centered().style(theme.dim)))
            .into_iter()
            .collect()
    } else {
        let available = (area.width as usize).saturating_sub(6);
        let title_w = (available * 3 / 5).max(10);
        let artist_w = available.saturating_sub(title_w + 3).max(4);

        tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let title = truncate(&track.title, title_w).to_uppercase();
                let artist = truncate(&track.artist, artist_w).to_uppercase();
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
            .collect()
    };

    let mut list_state = ListState::default();
    list_state.select(Some(selected_idx));

    frame.render_stateful_widget(List::new(items), area, &mut list_state);
}
