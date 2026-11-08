use std::sync::mpsc::Sender;

use super::state::{App, Mode};
use crate::music::SearchResponse;

pub fn handle_search_result(app: &mut App, response: SearchResponse) -> bool {
    if response.generation != app.search_generation {
        return false;
    }

    app.loading = false;
    app.cancel_pending_resolution();

    match response.result {
        Ok(tracks) => {
            let count = tracks.len();
            app.tracks = tracks;
            app.last_query = response.query;
            app.mode = Mode::Nav;

            if let Some(now) = app.current_track() {
                app.playing = app.tracks.iter().position(|t| t.same_identity(now));
            } else {
                app.playing = None;
            }

            app.selected = app.playing.unwrap_or(0);

            app.set_status(if count == 0 {
                "NO RESULTS".into()
            } else {
                format!("{} RESULTS ({})", count, app.settings.provider.label())
            });
            true
        }
        Err(error) => {
            // Preserve the latest valid results when a newer search fails
            app.set_status(format!("SEARCH ERROR: {}", error));
            false
        }
    }
}

pub fn refetch_if_needed(app: &mut App, search_tx: &Sender<SearchResponse>) {
    let query = if !app.last_query.is_empty() {
        app.last_query.clone()
    } else {
        app.query.trim().to_string()
    };

    if !query.is_empty() {
        app.cancel_pending_resolution();
        app.loading = true;
        let provider = app.settings.provider;
        app.set_status(format!("REFETCHING ({})…", provider.label()));

        app.search_generation = app.search_generation.wrapping_add(1);
        crate::music::spawn_search(query, provider, app.search_generation, search_tx.clone());
    }
}
