use std::sync::mpsc::Sender;
use std::time::Duration;

use super::state::App;
use crate::music::{PlaybackSource, ResolveResponse, Track};
use crate::playback::{AudioPlayer, PlayerEvent, Repeat};

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

pub fn play_next_queued(
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
            let pos_dur = Duration::from_secs_f64(elapsed);
            app.current_position = Some(pos_dur);
            *playback = Some((elapsed, duration));
            if let Some(worker) = &app.discord_worker {
                worker.position_changed(pos_dur);
            }
            true
        }
        PlayerEvent::Ended { generation } => {
            if generation != current {
                return false;
            }
            app.current_position = None;
            *playback = None;
            if app.is_resolving() {
                app.finish_active_track_while_resolving();
            } else {
                handle_playback_ended(app, player, player_tx.clone(), resolve_tx.clone());
            }
            true
        }
        PlayerEvent::Failed {
            generation,
            message,
        } => {
            if generation != current {
                return false;
            }
            app.current_position = None;
            *playback = None;
            if app.is_resolving() {
                player.record_error(message.to_string());
                app.finish_active_track_while_resolving();
            } else {
                handle_playback_failed(
                    app,
                    player,
                    player_tx.clone(),
                    resolve_tx.clone(),
                    &message,
                );
            }
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
    if app.is_resolving() {
        app.finish_active_track_while_resolving();
        return true;
    }

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
    if app.is_resolving() {
        app.finish_active_track_while_resolving();
        return true;
    }
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
    if app.is_resolving() {
        app.finish_active_track_while_resolving();
        return true;
    }

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
        crate::music::spawn_resolve(track, source, generation, resolve_tx);
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
                && !old.same_identity(&track)
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
                let _ = crate::artwork::prefetch(&next_queue.track.art_url);
            }

            if app.settings.audio_cache {
                player.prefetch_later(&track.stream_url, 15);
            }

            if let Some(worker) = &app.discord_worker {
                let mut info = crate::discord::PlaybackInfo::new(&track.title, &track.artist);
                if let Some(album) = &track.album {
                    info = info.with_album(album);
                }
                if let Some(dur_ms) = track.duration_ms {
                    info = info.with_duration(Duration::from_millis(dur_ms));
                }
                // Sharing temporary/internal stream URLs through Discord activity button
                // Disabled for now until proper public track URLs are available.
                // if track.has_stream_url() {
                //     info = info.with_stream_url(&track.stream_url);
                // }
                worker.track_changed(info);
            }
            Ok(())
        }
        Err(error) => {
            if let Some(old) = app.current_track().cloned()
                && !old.same_identity(&track)
            {
                app.push_history(old);
            }
            app.set_stopped();
            app.set_status(format!("PLAYER ERROR: {}", error));
            if let Some(worker) = &app.discord_worker {
                worker.clear();
            }
            Err(error)
        }
    }
}

pub fn now_playing(app: &App) -> String {
    app.current_track()
        .map(|t| format!("PLAYING: {} — {}", t.title, t.artist))
        .unwrap_or_else(|| "PLAYING".into())
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

                    let is_same = slot.matches_identity(
                        response.spotify_id.as_deref(),
                        &response.title,
                        &response.artist,
                    );

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
