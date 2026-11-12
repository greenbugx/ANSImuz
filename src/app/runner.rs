use std::{
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::{events::handle_key, state::App};
use crate::music::{ResolveResponse, SearchResponse};
use crate::playback::{AudioPlayer, PlayerEvent};

pub use super::playback::{
    advance_playback, handle_playback_ended, handle_playback_failed, handle_player_event,
    handle_resolve_response, now_playing, play_next, play_next_queued, play_previous,
    play_queue_item, play_search_result, play_track, recover_queue_failure,
    refill_queue_from_repeat_all, start_playback,
};
pub use super::search::{handle_search_result, refetch_if_needed};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.init_discord();
    let mut player = AudioPlayer::new();

    let (search_tx, search_rx): (Sender<SearchResponse>, Receiver<SearchResponse>) =
        mpsc::channel();
    let (player_tx, player_rx): (Sender<PlayerEvent>, Receiver<PlayerEvent>) = mpsc::channel();
    let (resolve_tx, resolve_rx): (Sender<ResolveResponse>, Receiver<ResolveResponse>) =
        mpsc::channel();

    let mut playback: Option<(f64, Option<f64>)> = None;

    loop {
        while let Ok(response) = search_rx.try_recv() {
            handle_search_result(&mut app, response);
        }

        while let Ok(response) = resolve_rx.try_recv() {
            handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, response);
        }

        while let Ok(event) = player_rx.try_recv() {
            handle_player_event(
                &mut app,
                &mut player,
                &player_tx,
                &resolve_tx,
                event,
                &mut playback,
            );
        }
        if app.current_track().is_none() {
            playback = None;
        }

        terminal.draw(|frame| crate::ui::render(&app, frame, playback))?;

        if event::poll(Duration::from_millis(80))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };

            if key.kind == KeyEventKind::Release {
                continue;
            }

            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                app.should_quit = true;
            } else {
                handle_key(
                    &mut app,
                    &mut player,
                    key,
                    &search_tx,
                    &player_tx,
                    &resolve_tx,
                );
            }
        }

        if app.should_quit {
            break;
        }
    }

    player.stop();

    if let Some(mut worker) = app.discord_worker.take() {
        worker.shutdown();
    }

    Ok(())
}
