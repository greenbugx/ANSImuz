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
use crate::music::{PlaybackSource, ResolveResponse, SearchResponse, Track};
use crate::playback::{AudioPlayer, PlayerEvent, Repeat};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
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
    Ok(())
}

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
                app.playing = app.tracks.iter().position(|t| {
                    (!now.stream_url.is_empty() && t.stream_url == now.stream_url)
                        || (now.spotify_id.is_some() && t.spotify_id == now.spotify_id)
                        || (t.title.eq_ignore_ascii_case(&now.title)
                            && t.artist.eq_ignore_ascii_case(&now.artist))
                });
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

pub fn play_search_result(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
    index: usize,
) -> bool {
    let Some(mut track) = app.tracks.get(index).cloned() else {
        return false;
    };

    if let Some(cached) = crate::music::get_cached_track(&track) {
        track = cached.clone();
        if let Some(slot) = app.tracks.get_mut(index) {
            *slot = cached;
        }
    }
    play_track(
        app,
        player,
        player_tx,
        resolve_tx,
        track,
        PlaybackSource::SearchResult { index },
    )
}

pub fn play_queue_item(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
    index: usize,
) -> bool {
    let Some(mut entry) = app.queue.get(index).cloned() else {
        return false;
    };

    if let Some(cached) = crate::music::get_cached_track(&entry.track) {
        entry.track = cached.clone();
        if let Some(slot) = app.queue.get_mut(index) {
            slot.track = cached;
        }
    }

    if entry.track.stream_url.is_empty() {
        play_track(
            app,
            player,
            player_tx,
            resolve_tx,
            entry.track,
            PlaybackSource::Queue { entry_id: entry.id },
        )
    } else {
        let Some(removed) = app.remove_queue_item(index) else {
            return false;
        };
        play_track(
            app,
            player,
            player_tx,
            resolve_tx,
            removed.track,
            PlaybackSource::Queue {
                entry_id: removed.id,
            },
        )
    }
}

pub fn refill_queue_from_repeat_all(app: &mut App) -> bool {
    if app.repeat != Repeat::All {
        return false;
    }
    if app.history.is_empty() && app.current_track().is_none() {
        return false;
    }
    if let Some(current) = app.current_track().cloned() {
        app.push_history(current);
    }
    app.set_stopped();
    let history = std::mem::take(&mut app.history);
    for track in history {
        app.enqueue(track);
    }
    if app.shuffle {
        app.shuffle_queue();
    }
    !app.queue.is_empty()
}

pub(crate) fn play_next_queued(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    let can_refill = player.error().is_none() && refill_queue_from_repeat_all(app);
    if !app.queue.is_empty() || can_refill {
        return play_queue_item(app, player, player_tx, resolve_tx, 0);
    }
    false
}

pub fn play_next(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    if app.queue.is_empty() && !refill_queue_from_repeat_all(app) {
        app.set_status("QUEUE IS EMPTY");
        return false;
    }

    play_queue_item(app, player, player_tx, resolve_tx, 0)
}

pub fn play_previous(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    if let Some(prev) = app.history.pop_back() {
        if let Some(current) = app.current_track().cloned() {
            app.push_front_queue(current);
            app.set_stopped();
        }
        play_track(
            app,
            player,
            player_tx,
            resolve_tx,
            prev,
            PlaybackSource::History,
        );
        true
    } else if let Some(current) = app.current_track().cloned() {
        play_track(
            app,
            player,
            player_tx,
            resolve_tx,
            current,
            PlaybackSource::Current,
        );
        app.set_status("REPLAYING TRACK");
        true
    } else {
        app.set_status("NO PREVIOUS TRACK");
        false
    }
}

pub fn recover_queue_failure(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    // If more queued items exist, try the next one
    if !app.queue.is_empty() {
        return play_queue_item(app, player, player_tx, resolve_tx, 0);
    }

    // If the previous track is still playing in the audio player, preserve it!
    if player.is_playing() && app.current_track().is_some() {
        return false;
    }

    // Queue is exhausted after failures: do NOT refill from Repeat::All to avoid endless retries!
    // Idle/stopped: push any ended track to history and clear playback state
    if let Some(current) = app.current_track().cloned() {
        app.push_history(current);
    }
    app.set_stopped();
    false
}

pub fn handle_player_event(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: &Sender<PlayerEvent>,
    resolve_tx: &Sender<ResolveResponse>,
    event: PlayerEvent,
    playback: &mut Option<(f64, Option<f64>)>,
) -> bool {
    let current = player.generation();
    match event {
        PlayerEvent::Position {
            generation,
            elapsed,
            duration,
        } => {
            if generation != current {
                return false;
            }
            *playback = Some((elapsed, duration));
            true
        }
        PlayerEvent::Ended { generation } => {
            if generation != current {
                return false;
            }
            *playback = None;
            handle_playback_ended(app, player, player_tx.clone(), resolve_tx.clone());
            true
        }
        PlayerEvent::Failed {
            generation,
            message,
        } => {
            if generation != current {
                return false;
            }
            *playback = None;
            handle_playback_failed(app, player, player_tx.clone(), resolve_tx.clone(), &message);
            true
        }
    }
}

pub fn handle_playback_ended(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    if app.repeat == Repeat::One
        && let Some(track) = app.current_track().cloned()
    {
        play_track(
            app,
            player,
            player_tx,
            resolve_tx,
            track,
            PlaybackSource::Current,
        );
        return true;
    }

    if play_next_queued(app, player, player_tx, resolve_tx) {
        return true;
    }

    if let Some(current) = app.current_track().cloned() {
        app.push_history(current);
    }
    app.set_stopped();
    app.set_status("PLAYBACK ENDED");
    false
}

pub fn handle_playback_failed(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
    message: &str,
) -> bool {
    player.record_error(message.to_string());
    app.set_status(format!("PLAYER ERROR: {}", message));

    if let Some(current) = app.current_track().cloned() {
        app.push_history(current);
    }
    app.set_stopped();

    if !app.queue.is_empty() {
        play_queue_item(app, player, player_tx, resolve_tx, 0)
    } else {
        false
    }
}

pub fn advance_playback(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    if player.error().is_some() {
        let err = player.error().unwrap().to_string();
        handle_playback_failed(app, player, player_tx, resolve_tx, &err)
    } else {
        handle_playback_ended(app, player, player_tx, resolve_tx)
    }
}
pub fn play_track(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
    mut track: Track,
    source: PlaybackSource,
) -> bool {
    if let Some(cached) = crate::music::get_cached_track(&track) {
        track = cached.clone();
        if let PlaybackSource::SearchResult { index } = source
            && let Some(slot) = app.tracks.get_mut(index)
        {
            *slot = cached;
        }
    }

    if track.stream_url.is_empty() {
        app.cancel_pending_resolution();
        let generation = app.resolve_generation;
        app.set_resolving(track.clone(), source);
        match source {
            PlaybackSource::SearchResult { index } => {
                app.selected = index;
                app.pending_queue_entry_id = None;
            }
            PlaybackSource::Queue { entry_id } => {
                app.pending_queue_entry_id = Some(entry_id);
            }
            PlaybackSource::History | PlaybackSource::Current => {
                app.pending_queue_entry_id = None;
            }
        }
        app.set_status(format!("RESOLVING: {} — {}…", track.title, track.artist));

        let track_to_resolve = track;
        let spotify_id = track_to_resolve.spotify_id.clone();
        let title = track_to_resolve.title.clone();
        let artist = track_to_resolve.artist.clone();

        std::thread::spawn(move || {
            let result = crate::music::resolve_playable(&track_to_resolve);
            let _ = resolve_tx.send(ResolveResponse {
                generation,
                source,
                spotify_id,
                title,
                artist,
                result,
            });
        });
        return true;
    }

    app.cancel_pending_resolution();
    match start_playback(app, player, player_tx.clone(), track, source) {
        Ok(()) => true,
        Err(error) => {
            if let PlaybackSource::Queue { .. } = source {
                app.set_status(format!("PLAYER ERROR: {error}"));
                recover_queue_failure(app, player, player_tx, resolve_tx)
            } else {
                false
            }
        }
    }
}

pub fn start_playback(
    app: &mut App,
    player: &mut AudioPlayer,
    events: Sender<PlayerEvent>,
    track: Track,
    source: PlaybackSource,
) -> Result<(), String> {
    let audio_source = AudioPlayer::cached_audio(&track.stream_url)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| track.stream_url.clone());

    match player.play(&audio_source, app.volume, events) {
        Ok(()) => {
            if let Some(old) = app.current_track().cloned()
                && (old.stream_url != track.stream_url || old.title != track.title)
            {
                app.push_history(old);
            }
            app.set_playing(track.clone());
            app.set_status(format!("PLAYING: {} — {}", track.title, track.artist));

            match source {
                PlaybackSource::SearchResult { index } => {
                    app.playing = Some(index);
                    app.selected = index;
                }
                PlaybackSource::Queue { .. }
                | PlaybackSource::History
                | PlaybackSource::Current => {
                    app.playing = None;
                }
            }

            if let Some(next_queue) = app.queue.first() {
                if app.settings.audio_cache {
                    player.prefetch_later(&next_queue.track.stream_url, 25);
                }
                crate::artwork::prefetch(&next_queue.track.art_url);
            }

            if app.settings.audio_cache {
                player.prefetch_later(&track.stream_url, 15);
            }
            Ok(())
        }
        Err(error) => {
            if let Some(old) = app.current_track().cloned()
                && (old.stream_url != track.stream_url || old.title != track.title)
            {
                app.push_history(old);
            }
            app.set_stopped();
            app.set_status(format!("PLAYER ERROR: {}", error));
            Err(error)
        }
    }
}

pub fn now_playing(app: &App) -> String {
    app.current_track()
        .map(|t| format!("PLAYING: {} — {}", t.title, t.artist))
        .unwrap_or_else(|| "PLAYING".into())
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

pub fn handle_resolve_response(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: &Sender<PlayerEvent>,
    resolve_tx: &Sender<ResolveResponse>,
    response: ResolveResponse,
) -> bool {
    if response.generation != app.resolve_generation {
        return false;
    }

    match response.result {
        Ok(resolved) => {
            crate::music::cache_resolved_track(&resolved);

            match response.source {
                PlaybackSource::SearchResult { index } => {
                    let Some(slot) = app.tracks.get_mut(index) else {
                        app.cancel_pending_resolution();
                        app.set_status(if app.current_track().is_some() && player.is_playing() {
                            now_playing(app)
                        } else {
                            "RESOLVE CANCELLED".to_string()
                        });
                        return false;
                    };

                    let is_same = match (&slot.spotify_id, &response.spotify_id) {
                        (Some(a), Some(b)) => a == b,
                        _ => {
                            slot.title.eq_ignore_ascii_case(&response.title)
                                && slot.artist.eq_ignore_ascii_case(&response.artist)
                        }
                    };

                    if !is_same {
                        app.cancel_pending_resolution();
                        app.set_status(if app.current_track().is_some() && player.is_playing() {
                            now_playing(app)
                        } else {
                            "RESOLVE CANCELLED".to_string()
                        });
                        return false;
                    }

                    *slot = resolved.clone();

                    if let Err(error) = start_playback(
                        app,
                        player,
                        player_tx.clone(),
                        resolved,
                        PlaybackSource::SearchResult { index },
                    ) {
                        app.set_status(format!("PLAYER ERROR: {error}"));
                        return false;
                    }

                    true
                }
                PlaybackSource::Queue { entry_id } => {
                    if let Some(pos) = app.queue.iter().position(|e| e.id == entry_id) {
                        let is_first = pos == 0;
                        if is_first {
                            let entry = app.remove_queue_item(pos).expect("valid queue item");
                            let mut track = resolved;
                            track.spotify_id = entry.track.spotify_id;
                            if let Err(error) = start_playback(
                                app,
                                player,
                                player_tx.clone(),
                                track,
                                PlaybackSource::Queue { entry_id },
                            ) {
                                app.set_status(format!("PLAYER ERROR: {error}"));
                                return recover_queue_failure(
                                    app,
                                    player,
                                    player_tx.clone(),
                                    resolve_tx.clone(),
                                );
                            }
                            true
                        } else {
                            app.queue[pos].track = resolved;
                            app.cancel_pending_resolution();
                            true
                        }
                    } else {
                        app.cancel_pending_resolution();
                        false
                    }
                }
                PlaybackSource::History | PlaybackSource::Current => {
                    if let Err(error) =
                        start_playback(app, player, player_tx.clone(), resolved, response.source)
                    {
                        app.set_status(format!("PLAYER ERROR: {error}"));
                        return false;
                    }
                    true
                }
            }
        }
        Err(error) => {
            app.set_status(format!("RESOLVE ERROR: {}", error));
            app.cancel_pending_resolution();

            if let PlaybackSource::Queue { entry_id } = response.source {
                if let Some(pos) = app.queue.iter().position(|e| e.id == entry_id) {
                    app.remove_queue_item(pos);
                }
                recover_queue_failure(app, player, player_tx.clone(), resolve_tx.clone());
            }

            false
        }
    }
}
