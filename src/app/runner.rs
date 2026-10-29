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
use crate::music::{ResolveResponse, SearchResult, Track};
use crate::playback::{AudioPlayer, PlayerEvent, Repeat};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    let mut player = AudioPlayer::new();

    let (search_tx, search_rx): (Sender<SearchResult>, Receiver<SearchResult>) = mpsc::channel();
    let (player_tx, player_rx): (Sender<PlayerEvent>, Receiver<PlayerEvent>) = mpsc::channel();
    let (resolve_tx, resolve_rx): (Sender<ResolveResponse>, Receiver<ResolveResponse>) =
        mpsc::channel();

    let mut playback: Option<(f64, Option<f64>)> = None;

    loop {
        while let Ok((query, result)) = search_rx.try_recv() {
            app.loading = false;
            app.cancel_pending_resolution();

            match result {
                Ok(tracks) => {
                    let count = tracks.len();
                    app.tracks = tracks;
                    app.last_query = query;
                    app.mode = Mode::Nav;

                    if let Some(now) = &app.now_playing {
                        app.playing = app.tracks.iter().position(|t| {
                            (!now.stream_url.is_empty() && t.stream_url == now.stream_url)
                                || (now.spotify_id.is_some() && t.spotify_id == now.spotify_id)
                                || (t.title.eq_ignore_ascii_case(&now.title)
                                    && t.artist.eq_ignore_ascii_case(&now.artist))
                        });
                    }

                    app.selected = app.playing.unwrap_or(0);

                    app.set_status(if count == 0 {
                        "NO RESULTS".into()
                    } else {
                        format!("{} RESULTS ({})", count, app.settings.provider.label())
                    });
                }
                Err(error) => {
                    app.set_status(format!("SEARCH ERROR: {}", error));
                }
            }
        }

        while let Ok(response) = resolve_rx.try_recv() {
            if response.generation != app.resolve_generation {
                continue;
            }

            app.is_resolving = false;
            app.pending_track = None;

            match response.result {
                Ok(resolved) => {
                    crate::music::cache_resolved_track(&resolved);

                    let Some(slot) = app.tracks.get_mut(response.index) else {
                        app.set_status(if app.now_playing.is_some() && player.is_playing() {
                            now_playing(&app)
                        } else {
                            "RESOLVE CANCELLED".to_string()
                        });
                        continue;
                    };

                    let is_same = match (&slot.spotify_id, &response.spotify_id) {
                        (Some(a), Some(b)) => a == b,
                        _ => {
                            slot.title.eq_ignore_ascii_case(&response.title)
                                && slot.artist.eq_ignore_ascii_case(&response.artist)
                        }
                    };

                    if !is_same {
                        app.set_status(if app.now_playing.is_some() && player.is_playing() {
                            now_playing(&app)
                        } else {
                            "RESOLVE CANCELLED".to_string()
                        });
                        continue;
                    }

                    *slot = resolved.clone();

                    start_playback(
                        &mut app,
                        &mut player,
                        player_tx.clone(),
                        response.index,
                        resolved,
                    );
                }
                Err(error) => {
                    app.set_status(format!("RESOLVE ERROR: {}", error));
                }
            }
        }

        if !app.is_resolving && app.now_playing.is_some() && !player.is_playing() {
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
                Some(index) => play_track(
                    &mut app,
                    &mut player,
                    player_tx.clone(),
                    resolve_tx.clone(),
                    index,
                ),
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

        if app.now_playing.is_none() {
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
    Ok(())
}

pub fn play_track(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
    index: usize,
) {
    let Some(mut track) = app.tracks.get(index).cloned() else {
        return;
    };

    if let Some(cached) = crate::music::get_cached_track(&track) {
        track = cached.clone();
        if let Some(slot) = app.tracks.get_mut(index) {
            *slot = cached;
        }
    }

    if track.stream_url.is_empty() {
        app.cancel_pending_resolution();
        let generation = app.resolve_generation;
        app.is_resolving = true;
        app.pending_track = Some(track.clone());
        if !player.is_playing() {
            app.now_playing = None;
            app.playing = None;
        }
        app.selected = index;
        app.set_status(format!("RESOLVING: {} — {}…", track.title, track.artist));

        let track_to_resolve = track;
        let spotify_id = track_to_resolve.spotify_id.clone();
        let title = track_to_resolve.title.clone();
        let artist = track_to_resolve.artist.clone();

        std::thread::spawn(move || {
            let result = crate::music::resolve_playable(&track_to_resolve);
            let _ = resolve_tx.send(ResolveResponse {
                generation,
                index,
                spotify_id,
                title,
                artist,
                result,
            });
        });
        return;
    }

    app.cancel_pending_resolution();
    start_playback(app, player, player_tx, index, track);
}

pub fn start_playback(
    app: &mut App,
    player: &mut AudioPlayer,
    events: Sender<PlayerEvent>,
    index: usize,
    track: Track,
) {
    let source = AudioPlayer::cached_audio(&track.stream_url)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| track.stream_url.clone());

    match player.play(&source, app.volume, events) {
        Ok(()) => {
            app.is_resolving = false;
            app.pending_track = None;
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
        Err(error) => {
            app.playing = None;
            app.now_playing = None;
            app.set_status(format!("PLAYER ERROR: {}", error));
        }
    }
}

pub fn now_playing(app: &App) -> String {
    app.now_playing
        .as_ref()
        .map(|t| format!("PLAYING: {} — {}", t.title, t.artist))
        .unwrap_or_else(|| "PLAYING".into())
}

pub fn refetch_if_needed(app: &mut App, search_tx: &Sender<SearchResult>) {
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
        let tx = search_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(crate::music::search_music(query, provider));
        });
    }
}
