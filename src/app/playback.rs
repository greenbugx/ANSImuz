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

    if !entry.track.has_stream_url() {
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
    clear_discord_presence(app);
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
            let dur_opt = duration.filter(|&d| d > 0.0).map(Duration::from_secs_f64);
            if let Some(track) = app.current_track_mut()
                && track.duration_ms.is_none()
                && let Some(dur_sec) = duration
                && dur_sec > 0.0
            {
                track.duration_ms = Some((dur_sec * 1000.0) as u64);
            }
            app.current_position = Some(pos_dur);
            *playback = Some((elapsed, duration));
            if let Some(worker) = &app.discord_worker {
                worker.playback_progress(pos_dur, dur_opt);
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
                retire_active_track_while_resolving(app);
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
                retire_active_track_while_resolving(app);
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

fn clear_discord_presence(app: &App) {
    if let Some(worker) = &app.discord_worker {
        worker.clear();
    }
}

fn retire_active_track_while_resolving(app: &mut App) {
    if app.current_track().is_some() {
        clear_discord_presence(app);
    }
    app.finish_active_track_while_resolving();
}

pub fn handle_playback_ended(
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: Sender<PlayerEvent>,
    resolve_tx: Sender<ResolveResponse>,
) -> bool {
    if app.is_resolving() {
        retire_active_track_while_resolving(app);
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
    clear_discord_presence(app);
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
        retire_active_track_while_resolving(app);
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
        clear_discord_presence(app);
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
        retire_active_track_while_resolving(app);
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

    if !track.has_stream_url() {
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
    let raw_stream = track.stream_url.as_deref().unwrap_or_default();
    let audio_source = AudioPlayer::cached_audio(raw_stream)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| raw_stream.to_string());

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
                if app.settings.audio_cache
                    && let Some(stream_url) = &next_queue.track.stream_url
                {
                    player.prefetch_later(stream_url, 25);
                }
                if let Some(art_url) = &next_queue.track.art_url {
                    let _ = crate::artwork::prefetch(art_url);
                }
            }

            if app.settings.audio_cache
                && let Some(stream_url) = &track.stream_url
            {
                player.prefetch_later(stream_url, 15);
            }

            if let Some(worker) = &app.discord_worker {
                let info = crate::discord::PlaybackInfo::from(&track);
                let artwork = crate::discord::artwork::ArtworkRequest::from_track(&track);
                worker.track_changed(info, artwork);
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

    use super::*;
    use crate::discord::worker::{DiscordCommand, DiscordWorker};

    struct TestHarness {
        app: App,
        player: AudioPlayer,
        player_tx: Sender<PlayerEvent>,
        _player_rx: Receiver<PlayerEvent>,
        resolve_tx: Sender<ResolveResponse>,
        _resolve_rx: Receiver<ResolveResponse>,
        discord_rx: Receiver<DiscordCommand>,
        playback: Option<(f64, Option<f64>)>,
    }

    impl TestHarness {
        fn new_with_active_and_resolving() -> (Self, u64) {
            let mut app = App::new();
            let mut player = AudioPlayer::new_mock();
            let (player_tx, player_rx) = channel();
            let (resolve_tx, resolve_rx) = channel();
            let (discord_worker, discord_rx) = DiscordWorker::spawn_test_recorder();
            app.discord_worker = Some(discord_worker);

            let track_a = Track::new("Song A", "Artist A")
                .with_provider("ytmusic")
                .with_stream_url("https://example.com/a")
                .with_duration_ms(180_000);
            let track_b = Track::new("Song B", "Artist B")
                .with_provider("spotify")
                .with_spotify_id("sp_b");
            app.tracks = vec![track_a.clone(), track_b.clone()];

            play_track(
                &mut app,
                &mut player,
                player_tx.clone(),
                resolve_tx.clone(),
                track_a,
                PlaybackSource::SearchResult { index: 0 },
            );
            assert!(
                matches!(
                    discord_rx.try_recv(),
                    Ok(DiscordCommand::TrackChanged { .. })
                ),
                "starting Track A must publish Discord presence"
            );

            let active_gen = player.generation();
            app.set_resolving(track_b, PlaybackSource::SearchResult { index: 1 });
            assert!(app.is_resolving());
            assert_eq!(
                app.current_track().map(|t| t.title.as_str()),
                Some("Song A")
            );

            (
                Self {
                    app,
                    player,
                    player_tx,
                    _player_rx: player_rx,
                    resolve_tx,
                    _resolve_rx: resolve_rx,
                    discord_rx,
                    playback: Some((42.0, Some(180.0))),
                },
                active_gen,
            )
        }
    }

    #[test]
    fn test_player_ended_while_resolving_clears_discord_and_resolve_failure_stays_stopped() {
        let (mut h, active_gen) = TestHarness::new_with_active_and_resolving();

        let handled = handle_player_event(
            &mut h.app,
            &mut h.player,
            &h.player_tx,
            &h.resolve_tx,
            PlayerEvent::Ended {
                generation: active_gen,
            },
            &mut h.playback,
        );
        assert!(handled);
        assert!(h.playback.is_none());
        assert!(h.app.is_resolving());
        assert!(h.app.current_track().is_none());
        assert!(h.app.playing.is_none());
        assert_eq!(h.app.history.len(), 1);
        assert_eq!(h.app.history[0].title, "Song A");
        assert!(
            matches!(h.discord_rx.try_recv(), Ok(DiscordCommand::Clear)),
            "retiring active playback during resolution must clear Discord presence"
        );
        assert!(matches!(h.discord_rx.try_recv(), Err(TryRecvError::Empty)));

        let response = ResolveResponse {
            generation: h.app.resolve_generation,
            source: PlaybackSource::SearchResult { index: 1 },
            spotify_id: Some("sp_b".into()),
            title: "Song B".into(),
            artist: "Artist B".into(),
            result: Err("resolve failed".into()),
        };
        assert!(!handle_resolve_response(
            &mut h.app,
            &mut h.player,
            &h.player_tx,
            &h.resolve_tx,
            response,
        ));
        assert!(h.app.playback_state.is_stopped());
        assert!(h.app.current_track().is_none());
        assert!(matches!(h.discord_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn test_player_failed_while_resolving_clears_discord_and_cancel_leaves_stopped() {
        let (mut h, active_gen) = TestHarness::new_with_active_and_resolving();

        let handled = handle_player_event(
            &mut h.app,
            &mut h.player,
            &h.player_tx,
            &h.resolve_tx,
            PlayerEvent::Failed {
                generation: active_gen,
                message: "stream error".into(),
            },
            &mut h.playback,
        );
        assert!(handled);
        assert!(h.playback.is_none());
        assert!(h.app.is_resolving());
        assert!(h.app.current_track().is_none());
        assert_eq!(h.player.error(), Some("stream error"));
        assert!(
            matches!(h.discord_rx.try_recv(), Ok(DiscordCommand::Clear)),
            "failing active playback during resolution must clear Discord presence"
        );
        assert!(matches!(h.discord_rx.try_recv(), Err(TryRecvError::Empty)));

        // Calling again after active track already retired must not spam duplicate clears.
        assert!(advance_playback(
            &mut h.app,
            &mut h.player,
            h.player_tx.clone(),
            h.resolve_tx.clone(),
        ));
        assert!(matches!(h.discord_rx.try_recv(), Err(TryRecvError::Empty)));

        h.app.cancel_pending_resolution();
        assert!(h.app.playback_state.is_stopped());
        assert!(h.app.current_track().is_none());
        assert!(matches!(h.discord_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn test_direct_playback_retirement_helpers_clear_discord_while_resolving() {
        let (mut h_end, _) = TestHarness::new_with_active_and_resolving();
        assert!(handle_playback_ended(
            &mut h_end.app,
            &mut h_end.player,
            h_end.player_tx.clone(),
            h_end.resolve_tx.clone(),
        ));
        assert!(h_end.app.current_track().is_none());
        assert!(matches!(
            h_end.discord_rx.try_recv(),
            Ok(DiscordCommand::Clear)
        ));

        let (mut h_fail, _) = TestHarness::new_with_active_and_resolving();
        assert!(handle_playback_failed(
            &mut h_fail.app,
            &mut h_fail.player,
            h_fail.player_tx.clone(),
            h_fail.resolve_tx.clone(),
            "fatal err",
        ));
        assert!(h_fail.app.current_track().is_none());
        assert_eq!(h_fail.player.error(), Some("fatal err"));
        assert!(matches!(
            h_fail.discord_rx.try_recv(),
            Ok(DiscordCommand::Clear)
        ));

        let (mut h_adv, _) = TestHarness::new_with_active_and_resolving();
        assert!(advance_playback(
            &mut h_adv.app,
            &mut h_adv.player,
            h_adv.player_tx.clone(),
            h_adv.resolve_tx.clone(),
        ));
        assert!(h_adv.app.current_track().is_none());
        assert!(matches!(
            h_adv.discord_rx.try_recv(),
            Ok(DiscordCommand::Clear)
        ));
    }

    #[test]
    fn test_resolve_failure_with_active_track_still_playing_keeps_discord_presence() {
        let (mut h, _) = TestHarness::new_with_active_and_resolving();

        let response = ResolveResponse {
            generation: h.app.resolve_generation,
            source: PlaybackSource::SearchResult { index: 1 },
            spotify_id: Some("sp_b".into()),
            title: "Song B".into(),
            artist: "Artist B".into(),
            result: Err("resolve failed".into()),
        };
        assert!(!handle_resolve_response(
            &mut h.app,
            &mut h.player,
            &h.player_tx,
            &h.resolve_tx,
            response,
        ));
        assert!(!h.app.is_resolving());
        assert!(h.app.is_playing());
        assert_eq!(
            h.app.current_track().map(|t| t.title.as_str()),
            Some("Song A")
        );
        assert!(
            matches!(h.discord_rx.try_recv(), Err(TryRecvError::Empty)),
            "Discord presence must not be cleared when Track A keeps playing"
        );
    }
}
