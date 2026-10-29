use std::sync::mpsc;

use ansimuz::app::{
    advance_playback, handle_resolve_response, play_next, play_queue_item, play_search_result,
    state::App,
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

    // Track 1 ends: queue priority plays Q0
    let adv1 = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(adv1);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q0")
    );
    assert_eq!(app.queue.len(), 1);

    // Q0 ends: queue priority plays Q1
    let adv2 = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(adv2);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q1")
    );
    assert_eq!(app.queue.len(), 0);

    // Q1 ends: queue is exhausted, resumes search sequence at Search 1!
    let adv3 = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(adv3);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Search 1")
    );
    assert_eq!(app.playing, Some(1));
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
fn test_resuming_search_sequence_after_queue_drains() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Search results: [A, B, C]
    app.tracks = vec![
        make_test_track("A", "Artist", "http://stream/a"),
        make_test_track("B", "Artist", "http://stream/b"),
        make_test_track("C", "Artist", "http://stream/c"),
    ];

    // User plays A
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    assert_eq!(app.search_resume_index, Some(0));

    // User enqueues X, Y
    app.enqueue(make_test_track("X", "Artist", "http://stream/x"));
    app.enqueue(make_test_track("Y", "Artist", "http://stream/y"));

    // A finishes -> X plays
    advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("X")
    );
    assert_eq!(
        app.search_resume_index,
        Some(0),
        "search_resume_index must stay 0"
    );

    // X finishes -> Y plays
    advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Y")
    );
    assert_eq!(
        app.search_resume_index,
        Some(0),
        "search_resume_index must stay 0"
    );

    // Y finishes -> queue is empty! Search resumes at B (0 + 1)
    advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("B")
    );
    assert_eq!(app.playing, Some(1));
    assert_eq!(app.search_resume_index, Some(1));

    // B finishes -> advances to C (1 + 1)
    advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("C")
    );
    assert_eq!(app.playing, Some(2));
    assert_eq!(app.search_resume_index, Some(2));
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
fn test_new_search_synchronizes_resume_position_and_falls_back() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Old search: 3 tracks [T0, T1, T2]
    app.tracks = vec![
        make_test_track("T0", "Artist", "http://stream/t0"),
        make_test_track("T1", "Artist", "http://stream/t1"),
        make_test_track("T2", "Artist", "http://stream/t2"),
    ];

    // User played T2
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        2,
    );
    assert_eq!(app.search_resume_index, Some(2));

    // User enqueues Q0
    app.enqueue(make_test_track("Q0", "Artist", "http://stream/q0"));

    // Q0 starts playing
    play_next(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Q0")
    );
    assert_eq!(app.search_resume_index, Some(2));

    // While Q0 is playing, a new search finishes with only 2 results [New0, New1]
    // Simulate new search result handling
    let new_tracks = vec![
        make_test_track("New 0", "Artist", "http://stream/new0"),
        make_test_track("New 1", "Artist", "http://stream/new1"),
    ];
    app.tracks = new_tracks;
    // Synchronize resume position as runner.rs does:
    app.playing = None; // Q0 is not in the new search results
    app.search_resume_index = app.playing;

    assert_eq!(app.search_resume_index, None);

    // Now Q0 finishes playing and the queue empties.
    // Playback must NOT stop; it must fall back to New 0!
    let advanced = advance_playback(&mut app, &mut player, player_tx.clone(), resolve_tx.clone());
    assert!(
        advanced,
        "Must fall back and advance through new search results"
    );
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("New 0")
    );
    assert_eq!(app.playing, Some(0));
    assert_eq!(app.search_resume_index, Some(0));
}
