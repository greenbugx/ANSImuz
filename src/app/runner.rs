use std::{
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::{
    events::handle_key,
    state::{App, Mode},
};
use crate::music::SearchResult;
use crate::playback::{AudioPlayer, PlayerEvent, Repeat};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    let mut player = AudioPlayer::new();

    let (search_tx, search_rx): (Sender<SearchResult>, Receiver<SearchResult>) = mpsc::channel();
    let (player_tx, player_rx): (Sender<PlayerEvent>, Receiver<PlayerEvent>) = mpsc::channel();

    let mut playback: Option<(f64, Option<f64>)> = None;

    loop {
        while let Ok((query, result)) = search_rx.try_recv() {
            app.loading = false;

            match result {
                Ok(tracks) => {
                    let count = tracks.len();
                    app.tracks = tracks;
                    app.selected = 0;
                    app.last_query = query;
                    app.mode = Mode::Nav;
                    app.set_status(if count == 0 {
                        "NO RESULTS".into()
                    } else {
                        format!("{} RESULTS", count)
                    });
                }
                Err(error) => {
                    app.set_status(format!("SEARCH ERROR: {}", error));
                }
            }
        }

        if app.playing.is_some() && !player.is_playing() {
            let ended = app.playing;

            let next = match app.repeat {
                Repeat::One if app.played_once => ended,
                Repeat::All if app.played_once => ended.and_then(|index| {
                    if app.shuffle {
                        app.shuffled_next()
                    } else if app.tracks.is_empty() {
                        None
                    } else {
                        Some((index + 1) % app.tracks.len())
                    }
                }),
                _ => None,
            };

            match next {
                Some(index) => play_track(&mut app, &mut player, player_tx.clone(), index),
                None => {
                    app.playing = None;
                    app.now_playing = None;
                    app.paused = false;

                    app.set_status(match player.error() {
                        Some(error) => format!("PLAYER ERROR: {}", error),
                        None => "PLAYBACK ENDED".to_string(),
                    });
                }
            }
        }

        let current = player.generation();

        while let Ok(event) = player_rx.try_recv() {
            match event {
                PlayerEvent::Position {
                    generation,
                    elapsed,
                    duration,
                } if generation == current => {
                    playback = Some((elapsed, duration));
                    app.played_once = true;
                }
                PlayerEvent::Failed {
                    generation,
                    message,
                } if generation == current => {
                    app.set_status(format!("PLAYER ERROR: {}", message));
                }
                _ => {}
            }
        }

        if app.playing.is_none() {
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
                handle_key(&mut app, &mut player, key, &search_tx, &player_tx);
            }
        }

        if app.should_quit {
            break;
        }
    }

    player.stop();
    Ok(())
}

pub fn play_track(
    app: &mut App,
    player: &mut AudioPlayer,
    events: Sender<PlayerEvent>,
    index: usize,
) {
    let Some(mut track) = app.tracks.get(index).cloned() else {
        return;
    };

    if track.stream_url.is_empty() {
        app.set_status(format!("RESOLVING: {} — {}", track.title, track.artist));
        match crate::music::resolve_playable(&track) {
            Ok(resolved) => {
                track = resolved.clone();
                if let Some(slot) = app.tracks.get_mut(index) {
                    *slot = resolved;
                }
            }
            Err(error) => {
                app.set_status(format!("RESOLVE ERROR: {}", error));
                return;
            }
        }
    }

    let source = AudioPlayer::cached_audio(&track.stream_url)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| track.stream_url.clone());

    match player.play(&source, app.volume, events) {
        Ok(()) => {
            app.playing = Some(index);
            app.now_playing = Some(track.clone());
            app.selected = index;
            app.paused = false;
            app.played_once = false;
            app.set_status(format!("PLAYING: {} — {}", track.title, track.artist));

            if app.settings.audio_cache {
                player.prefetch_later(&track.stream_url, 15);
            }

            if app.tracks.len() > 1 {
                let next = app
                    .tracks
                    .get((index + 1) % app.tracks.len())
                    .expect("index within tracks");

                if app.settings.audio_cache {
                    player.prefetch_later(&next.stream_url, 25);
                }
                crate::artwork::prefetch(&next.art_url);
            }
        }
        Err(error) => app.set_status(format!("PLAYER ERROR: {}", error)),
    }
}

pub fn now_playing(app: &App) -> String {
    app.now_playing
        .as_ref()
        .map(|t| format!("PLAYING: {} — {}", t.title, t.artist))
        .unwrap_or_else(|| "PLAYING".into())
}
