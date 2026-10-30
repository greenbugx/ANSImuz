use std::sync::mpsc;

use ansimuz::app::{
    advance_playback, handle_resolve_response, handle_search_result, play_next, play_queue_item,
    play_search_result, state::App,
};
use ansimuz::music::{PlaybackSource, ResolveResponse, Track};
use ansimuz::playback::{AudioPlayer, PlayerEvent, Repeat};

fn make_test_track(title: &str, artist: &str, stream: &str) -> Track {
    Track {
        title: title.to_string(),
        artist: artist.to_string(),
        stream_url: stream.to_string(),
        art_url: String::new(),
        spotify_id: if stream.is_empty() {
            Some(format!("sp_{title}"))
        } else {
            None
        },
        artists: vec![artist.to_string()],
        album: Some("Album".to_string()),
        duration_ms: Some(180_000),
    }
}

#[test]
fn test_manual_next_plays_queue_item_before_search_results() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Search results: [S0, S1]
    app.tracks = vec![
        make_test_track("S0", "Artist", "http://stream/s0"),
        make_test_track("S1", "Artist", "http://stream/s1"),
    ];

    // Start playing S0
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("S0")
    );
    assert_eq!(app.playing, Some(0));

    // Enqueue Q0
    let q0 = make_test_track("Q0", "Artist", "http://stream/q0");
    app.enqueue(q0);
    assert_eq!(app.queue.len(), 1);

    // Press Next (manual next): must play Q0 first, NOT S1!
    let advanced = play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(advanced);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q0")
    );
    assert_eq!(app.playing, None, "Queued track must have playing = None");
    assert_eq!(
        app.queue.len(),
        0,
        "Played queued track must be consumed from queue"
    );
}

#[test]
fn test_automatic_advance_queue_priority_and_exhaustion() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.tracks = vec![
        make_test_track("Search 0", "Artist", "http://stream/s0"),
        make_test_track("Search 1", "Artist", "http://stream/s1"),
    ];

    // Start playing Search 0
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    app.played_once = true;

    // Enqueue Q0 and Q1
    app.enqueue(make_test_track("Q0", "Artist", "http://stream/q0"));
    app.enqueue(make_test_track("Q1", "Artist", "http://stream/q1"));
    assert_eq!(app.queue.len(), 2);

    // Search 0 ends: queue priority plays Q0, Search 0 pushed to history
    let adv1 = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(adv1);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q0")
    );
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Search 0");

    // Q0 ends: queue priority plays Q1, Q0 pushed to history
    let adv2 = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(adv2);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q1")
    );
    assert_eq!(app.queue.len(), 0);
    assert_eq!(app.history.len(), 2);
    assert_eq!(app.history[1].title, "Q0");

    // Q1 ends: queue is exhausted. MUST NOT play Search 1! Playback ends cleanly.
    let adv3 = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(
        !adv3,
        "Playback must end when queue exhausts without Repeat::All"
    );
    assert!(app.now_playing.is_none());
    assert_eq!(app.history.len(), 3);
    assert_eq!(app.history[2].title, "Q1");
}

#[test]
fn test_repeat_one_overrides_queue_advance() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.tracks = vec![make_test_track(
        "Looping Song",
        "Artist",
        "http://stream/loop",
    )];
    app.repeat = Repeat::One;

    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    app.played_once = true;

    // Enqueue an item
    app.enqueue(make_test_track(
        "Waiting Song",
        "Artist",
        "http://stream/wait",
    ));

    // Track ends: Repeat::One must repeat current song, leaving the queue intact
    let advanced = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(advanced);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Looping Song")
    );
    assert_eq!(
        app.queue.len(),
        1,
        "Queue must not be consumed when Repeat::One is active"
    );
}

#[test]
fn test_previous_track_rewinds_history_and_prepends_to_queue() {
    use ansimuz::app::play_previous;

    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // History: [Song 1]. Currently playing: Song 2. Queue: [Song 3].
    app.history
        .push(make_test_track("Song 1", "Artist", "http://stream/1"));
    app.now_playing = Some(make_test_track("Song 2", "Artist", "http://stream/2"));
    app.enqueue(make_test_track("Song 3", "Artist", "http://stream/3"));

    assert_eq!(app.history.len(), 1);
    assert_eq!(app.queue.len(), 1);

    // User presses Previous
    let prev_ok = play_previous(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(prev_ok);

    // Song 1 must be playing now!
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Song 1")
    );
    assert_eq!(app.history.len(), 0);

    // Song 2 must be prepended to the front of the queue, in front of Song 3!
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[0].track.title, "Song 2");
    assert_eq!(app.queue[1].track.title, "Song 3");

    // Now user presses Next: Song 1 goes to history, Song 2 is played!
    let next_ok = play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(next_ok);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Song 2")
    );
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Song 1");
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].track.title, "Song 3");
}

#[test]
fn test_next_and_previous_do_not_operate_on_search_results() {
    use ansimuz::app::play_previous;

    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Search results: [S0, S1, S2]
    app.tracks = vec![
        make_test_track("S0", "Artist", "http://stream/s0"),
        make_test_track("S1", "Artist", "http://stream/s1"),
        make_test_track("S2", "Artist", "http://stream/s2"),
    ];

    // Play S1 directly from search
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("S1")
    );
    assert!(app.queue.is_empty());
    assert!(app.history.is_empty());

    // Pressing Next with empty queue MUST NOT play S2 from search results!
    let next_result = play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(!next_result);
    assert!(app.status.contains("QUEUE IS EMPTY"));
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("S1")
    );

    // Pressing Previouswith empty history MUST NOT play S0 from search results!
    let prev_result = play_previous(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(prev_result);
    assert!(app.status.contains("REPLAYING TRACK"));
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("S1")
    );
}

#[test]
fn test_removed_resolving_queue_entry_does_not_start_playback() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Unresolved queue track
    let track = make_test_track("Unresolved Queue Track", "Artist", "");
    let entry_id = app.enqueue(track);

    // Trigger playback of the queue item
    play_queue_item(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    assert!(app.is_resolving);
    let generation = app.resolve_generation;
    assert_eq!(app.pending_queue_entry_id, Some(entry_id));

    // User cancels / removes pending resolution
    app.cancel_pending_resolution();
    assert!(!app.is_resolving);
    assert_eq!(app.pending_queue_entry_id, None);

    // Resolution response arrives
    let mut resolved_track =
        make_test_track("Unresolved Queue Track", "Artist", "http://stream/resolved");
    resolved_track.spotify_id = Some("sp_Unresolved Queue Track".to_string());
    let response = ResolveResponse {
        generation,
        source: PlaybackSource::Queue { entry_id },
        spotify_id: Some("sp_Unresolved Queue Track".to_string()),
        title: "Unresolved Queue Track".to_string(),
        artist: "Artist".to_string(),
        result: Ok(resolved_track),
    };

    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, response);
    assert!(
        !handled,
        "Stale resolution for removed/cancelled queue item must be rejected"
    );
    assert!(
        app.now_playing.is_none(),
        "Removed track must NOT start playing"
    );
}

#[test]
fn test_failed_queue_resolution_skips_to_next_queued_item() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Queue: [Failing Track (unresolved), Working Track (resolved)]
    let failing_track = make_test_track("Failing Track", "Artist", "");
    let entry_id = app.enqueue(failing_track.clone());

    let working_track = make_test_track("Working Track", "Artist", "http://stream/working");
    app.enqueue(working_track);

    // Play failing track from queue
    // Play failing track from queue using play_queue_item (which consumes it from queue)
    play_queue_item(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    let generation = app.resolve_generation;
    let fail_response = ResolveResponse {
        generation,
        source: PlaybackSource::Queue { entry_id },
        spotify_id: Some("sp_Failing Track".to_string()),
        title: "Failing Track".to_string(),
        artist: "Artist".to_string(),
        result: Err("404 Not Found".to_string()),
    };

    let handled = handle_resolve_response(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        fail_response,
    );
    assert!(!handled, "Failed resolution returns false");
    assert!(app.status.contains("PLAYING: Working Track"));
    // Player automatically skips and starts the working track!
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Working Track")
    );
    assert_eq!(app.queue.len(), 0);
}

#[test]
fn test_repeat_one_does_not_interfere_with_failed_queue_resolution() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Song A is playing with Repeat::One enabled and played_once = true
    app.tracks = vec![make_test_track("Song A", "Artist", "http://stream/a")];
    app.repeat = Repeat::One;
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    app.played_once = true;

    // Queue: [Q_fail (unresolved), Q_good (resolved)]
    let fail_track = make_test_track("Q Fail", "Artist", "");
    let fail_id = app.enqueue(fail_track.clone());
    let good_track = make_test_track("Q Good", "Artist", "http://stream/good");
    app.enqueue(good_track);

    // Manual Next: plays Q_fail
    play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(app.is_resolving);
    let generation = app.resolve_generation;

    // Resolution fails for Q_fail
    let fail_resp = ResolveResponse {
        generation,
        source: PlaybackSource::Queue { entry_id: fail_id },
        spotify_id: Some("sp_Q Fail".to_string()),
        title: "Q Fail".to_string(),
        artist: "Artist".to_string(),
        result: Err("Audio stream unavailable".to_string()),
    };

    handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, fail_resp);

    // Must NOT restart Song A! Must advance to Q_good!
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q Good"),
        "Repeat::One on previous song must not restart previous song when a queued track fails"
    );
}

#[test]
fn test_repeat_all_loops_session_history_when_queue_exhausted() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.repeat = Repeat::All;
    app.enqueue(make_test_track("Track 1", "Artist", "http://stream/1"));
    app.enqueue(make_test_track("Track 2", "Artist", "http://stream/2"));

    // Play Track 1
    play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Track 1")
    );

    // Play Track 2 (Track 1 pushed to history)
    play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Track 2")
    );
    assert_eq!(app.queue.len(), 0);
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Track 1");

    // Track 2 ends: Repeat::All re-enqueues History and loops!
    let advanced = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(advanced);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Track 1")
    );
}

#[test]
fn test_new_search_does_not_affect_active_queue_playback() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Playing Q0 with Q1 queued
    app.enqueue(make_test_track("Q0", "Artist", "http://stream/q0"));
    app.enqueue(make_test_track("Q1", "Artist", "http://stream/q1"));
    play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q0")
    );

    // User searches for something else
    let new_tracks = vec![
        make_test_track("New Search 0", "Artist", "http://stream/new0"),
        make_test_track("New Search 1", "Artist", "http://stream/new1"),
    ];
    handle_search_result(&mut app, "new query".to_string(), Ok(new_tracks));

    // Q0 continues playing unaffected
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q0")
    );

    // Next plays Q1 from queue, NOT any of the new search results!
    let next_ok = play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(next_ok);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q1")
    );

    // When Q1 ends, playback stops; does NOT play New Search 0!
    let advance_result =
        advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(!advance_result);
    assert!(app.now_playing.is_none());
}
