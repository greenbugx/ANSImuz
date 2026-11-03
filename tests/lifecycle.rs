use std::sync::mpsc;

use ansimuz::app::{handle_player_event, play_next, play_track, state::App};
use ansimuz::music::{PlaybackSource, ResolveResponse, Track};
use ansimuz::playback::{AudioPlayer, PlayerEvent, Repeat};
fn make_test_track(title: &str, artist: &str, stream_url: &str) -> Track {
    Track {
        title: title.into(),
        artist: artist.into(),
        stream_url: stream_url.into(),
        art_url: String::new(),
        spotify_id: None,
        artists: vec![artist.into()],
        album: None,
        duration_ms: Some(180_000),
    }
}

#[test]
fn test_natural_completion_advances_queue_and_ends_cleanly() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    // Start playing Track 0
    let t0 = make_test_track("Track 0", "Artist", "http://stream/0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 0"));

    // Enqueue Track 1
    app.enqueue(make_test_track("Track 1", "Artist", "http://stream/1"));
    assert_eq!(app.queue.len(), 1);

    // Track 0 completes naturally via explicit Ended event
    let handled = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen0 },
        &mut playback,
    );
    assert!(handled, "Ended event must be handled");

    // Track 0 must be in history, Track 1 must be playing
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Track 0");
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 1"));
    assert!(app.queue.is_empty());
    let gen1 = player.generation();

    // Track 1 completes naturally via explicit Ended event
    let handled2 = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen1 },
        &mut playback,
    );
    assert!(handled2);

    // Queue exhausted: Track 1 pushed to history, state stopped, status updated
    assert_eq!(app.history.len(), 2);
    assert_eq!(app.history[1].title, "Track 1");
    assert!(app.now_playing().is_none());
    assert_eq!(app.status, "PLAYBACK ENDED");
}

#[test]
fn test_early_stream_failure_advances_queue_without_infinite_loop() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    // Start playing Failing Track 0
    let t0 = make_test_track("Failing 0", "Artist", "http://stream/fail0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();

    // Enqueue Next Track
    app.enqueue(make_test_track("Queued Track", "Artist", "http://stream/q"));

    // Explicit Failed event arrives
    let handled = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Failed {
            generation: gen0,
            message: "HTTP 403 Forbidden".into(),
        },
        &mut playback,
    );
    assert!(handled);

    // Failing 0 must be in history, status reflects error
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Failing 0");
    // Queue must advance to Queued Track
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Queued Track")
    );
    assert!(app.queue.is_empty());

    let gen1 = player.generation();

    // Queued track also fails
    let handled2 = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Failed {
            generation: gen1,
            message: "Stream unavailable".into(),
        },
        &mut playback,
    );
    assert!(handled2);

    // Queue is empty: stops cleanly, no endless retries
    assert_eq!(app.history.len(), 2);
    assert!(app.now_playing().is_none());
    assert_eq!(player.error(), Some("Stream unavailable"));
    assert!(app.status.contains("PLAYER ERROR: Stream unavailable"));
}

#[test]
fn test_manual_stop_cancels_playback_without_emitting_ended() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    let t0 = make_test_track("Track 0", "Artist", "http://stream/0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    assert!(player.is_playing());

    // User calls stop manually
    player.stop();
    assert!(!player.is_playing());

    // App state can be stopped manually
    app.set_stopped();
    assert!(app.now_playing().is_none());
    // Manual stop must NOT auto-advance or trigger "PLAYBACK ENDED"
    assert_ne!(app.status, "PLAYBACK ENDED");
}

#[test]
fn test_pause_preserves_playback_state_without_emitting_ended() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    let t0 = make_test_track("Track 0", "Artist", "http://stream/0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 0"));
    assert!(app.is_playing());

    // Pause
    player.toggle_pause().unwrap();
    app.set_paused(true);
    assert!(app.is_paused());
    assert!(!app.is_playing());
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 0"));

    // Unpause
    player.toggle_pause().unwrap();
    app.set_paused(false);
    assert!(!app.is_paused());
    assert!(app.is_playing());
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 0"));
}

#[test]
fn test_repeat_one_replays_on_natural_ended_but_not_on_failure() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    app.repeat = Repeat::One;
    let t0 = make_test_track("Loop Track", "Artist", "http://stream/loop");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();

    // Natural Ended event arrives
    let handled = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen0 },
        &mut playback,
    );
    assert!(handled);

    // Repeat::One must have restarted Loop Track (new generation)
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Loop Track")
    );
    let gen1 = player.generation();
    assert_ne!(gen0, gen1);

    // Now simulate failure on the restarted track
    let handled_fail = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Failed {
            generation: gen1,
            message: "Codec decode failed".into(),
        },
        &mut playback,
    );
    assert!(handled_fail);

    // Must NOT replay on failure! Must stop cleanly.
    assert!(app.now_playing().is_none());
    assert!(app.status.contains("PLAYER ERROR: Codec decode failed"));
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Loop Track");
}

#[test]
fn test_repeat_all_loops_on_natural_ended() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    app.repeat = Repeat::All;
    app.enqueue(make_test_track("Song 1", "Artist", "http://stream/1"));
    app.enqueue(make_test_track("Song 2", "Artist", "http://stream/2"));

    // Start Song 1
    play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Song 1"));
    let gen1 = player.generation();

    // Song 1 ends naturally -> Song 2 plays
    handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen1 },
        &mut playback,
    );
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Song 2"));
    let gen2 = player.generation();

    // Song 2 ends naturally -> Queue empty -> Repeat::All refills history -> Song 1 plays!
    handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen2 },
        &mut playback,
    );
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Song 1"));
}

#[test]
fn test_switching_tracks_during_playback_ignores_stale_events() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    // Start playing Track 0 (generation 1)
    let t0 = make_test_track("Track 0", "Artist", "http://stream/0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();

    // User switches to Track 1 (generation 2)
    let t1 = make_test_track("Track 1", "Artist", "http://stream/1");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t1,
        PlaybackSource::SearchResult { index: 1 },
    );
    let gen1 = player.generation();
    assert_ne!(gen0, gen1);
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 1"));

    // Stale Position event from Track 0 arrives -> ignored
    let handled_pos = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Position {
            generation: gen0,
            elapsed: 45.0,
            duration: Some(180.0),
        },
        &mut playback,
    );
    assert!(!handled_pos, "Stale Position event must be ignored");
    assert_eq!(playback, None);

    // Stale Ended event from Track 0 arrives -> ignored
    let handled_ended = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen0 },
        &mut playback,
    );
    assert!(!handled_ended, "Stale Ended event must be ignored");
    // Track 1 must still be playing!
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 1"));

    // Stale Failed event from Track 0 arrives -> ignored
    let handled_failed = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Failed {
            generation: gen0,
            message: "Old error".into(),
        },
        &mut playback,
    );
    assert!(!handled_failed, "Stale Failed event must be ignored");
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 1"));
    assert_ne!(app.status, "PLAYER ERROR: Old error");
}
#[test]
fn test_failed_mpv_startup_handled_gracefully() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    player.set_mock_fail(true);
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    let track = make_test_track("Unplayable", "Artist", "http://stream/bad");
    let result = play_track(
        &mut app,
        &mut player,
        player_tx,
        resolve_tx,
        track,
        PlaybackSource::SearchResult { index: 0 },
    );

    assert!(!result, "play_track must return false on spawn failure");
    assert!(
        app.now_playing().is_none(),
        "Nothing should be playing on failure"
    );
    assert!(app.status.contains("PLAYER ERROR: Failed to launch mpv"));
    assert_eq!(
        player.error(),
        Some("Failed to launch mpv: process failed to start")
    );
}

#[test]
fn test_rapid_track_switching_across_multiple_generations() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    let mut gens = Vec::new();
    for i in 0..5 {
        let t = make_test_track(
            &format!("Track {i}"),
            "Artist",
            &format!("http://stream/{i}"),
        );
        play_track(
            &mut app,
            &mut player,
            player_tx.clone(),
            resolve_tx.clone(),
            t,
            PlaybackSource::SearchResult { index: i },
        );
        gens.push(player.generation());
    }

    // Generations must be strictly increasing
    for i in 1..gens.len() {
        assert!(
            gens[i] > gens[i - 1],
            "Generation must increment on each switch"
        );
    }

    let final_gen = *gens.last().unwrap();
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 4"));

    // Stale events from all prior generations (0..4) are dropped
    for prior_gen in &gens[0..4] {
        let ended = handle_player_event(
            &mut app,
            &mut player,
            &player_tx,
            &resolve_tx,
            PlayerEvent::Ended {
                generation: *prior_gen,
            },
            &mut playback,
        );
        assert!(!ended, "Events from prior generation must be dropped");

        let failed = handle_player_event(
            &mut app,
            &mut player,
            &player_tx,
            &resolve_tx,
            PlayerEvent::Failed {
                generation: *prior_gen,
                message: "Old error".into(),
            },
            &mut playback,
        );
        assert!(!failed, "Failures from prior generation must be dropped");
    }

    // Only the final generation's event takes effect
    assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 4"));
    let final_ended = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended {
            generation: final_gen,
        },
        &mut playback,
    );
    assert!(final_ended, "Final generation event must be handled");
    assert_eq!(app.status, "PLAYBACK ENDED");
}
