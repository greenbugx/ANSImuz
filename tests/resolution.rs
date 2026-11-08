use std::sync::mpsc;

use ansimuz::app::{
    handle_player_event, handle_resolve_response, play_search_result, play_track, state::App,
};
use ansimuz::music::{PlaybackSource, ResolveResponse, Track};
use ansimuz::playback::{AudioPlayer, PlayerEvent, Repeat};

fn make_spotify_track(id: &str, title: &str, artist: &str) -> Track {
    Track {
        title: title.to_string(),
        artist: artist.to_string(),
        stream_url: String::new(),
        art_url: format!("https://i.scdn.co/image/{id}"),
        spotify_id: Some(id.to_string()),
        artists: vec![artist.to_string()],
        album: Some("Test Album".to_string()),
        duration_ms: Some(200_000),
    }
}

fn make_resolved_track(id: &str, title: &str, artist: &str, youtube_id: &str) -> Track {
    let mut track = make_spotify_track(id, title, artist);
    track.stream_url = format!("https://music.youtube.com/watch?v={youtube_id}");
    track
}

static RESOLUTION_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_rapid_track_switching_ignores_stale_responses() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.tracks = vec![
        make_spotify_track("sp0", "Track Zero", "Artist Zero"),
        make_spotify_track("sp1", "Track One", "Artist One"),
        make_spotify_track("sp2", "Track Two", "Artist Two"),
    ];

    // Rapidly request track 0, then 1, then 2
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    assert_eq!(app.resolve_generation, 1);
    assert!(app.is_resolving());
    assert_eq!(
        app.pending_track().map(|t| t.title.as_str()),
        Some("Track Zero")
    );

    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    assert_eq!(app.resolve_generation, 2);
    assert!(app.is_resolving());
    assert_eq!(
        app.pending_track().map(|t| t.title.as_str()),
        Some("Track One")
    );

    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        2,
    );
    assert_eq!(app.resolve_generation, 3);
    assert!(app.is_resolving());
    assert_eq!(
        app.pending_track().map(|t| t.title.as_str()),
        Some("Track Two")
    );

    // Stale response from request 0 arrives (generation 1)
    let stale_resp_0 = ResolveResponse {
        generation: 1,
        source: PlaybackSource::SearchResult { index: 0 },
        spotify_id: Some("sp0".into()),
        title: "Track Zero".into(),
        artist: "Artist Zero".into(),
        result: Ok(make_resolved_track(
            "sp0",
            "Track Zero",
            "Artist Zero",
            "yt0",
        )),
    };
    let handled_0 =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, stale_resp_0);
    assert!(!handled_0, "Stale generation 1 response must be ignored");
    assert!(app.is_resolving(), "Track 2 must still be resolving");
    assert_eq!(
        app.pending_track().map(|t| t.title.as_str()),
        Some("Track Two")
    );
    assert!(
        app.tracks[0].stream_url.is_empty(),
        "Track 0 slot must not be touched"
    );

    // Stale response from request 1 arrives (generation 2)
    let stale_resp_1 = ResolveResponse {
        generation: 2,
        source: PlaybackSource::SearchResult { index: 1 },
        spotify_id: Some("sp1".into()),
        title: "Track One".into(),
        artist: "Artist One".into(),
        result: Ok(make_resolved_track("sp1", "Track One", "Artist One", "yt1")),
    };
    let handled_1 =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, stale_resp_1);
    assert!(!handled_1, "Stale generation 2 response must be ignored");
    assert!(app.is_resolving());

    // Fresh response from request 2 arrives (generation 3)
    let fresh_resp_2 = ResolveResponse {
        generation: 3,
        source: PlaybackSource::SearchResult { index: 2 },
        spotify_id: Some("sp2".into()),
        title: "Track Two".into(),
        artist: "Artist Two".into(),
        result: Ok(make_resolved_track("sp2", "Track Two", "Artist Two", "yt2")),
    };
    let handled_2 =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, fresh_resp_2);
    assert!(handled_2, "Fresh generation 3 response must be accepted");
    assert!(!app.is_resolving());
    assert!(app.pending_track().is_none());
    assert!(!app.tracks[2].stream_url.is_empty());
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Track Two")
    );
}

#[test]
fn test_cancelling_while_old_track_is_playing_preserves_playback() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Initial state: Track 0 is currently playing
    let old_track = make_resolved_track("sp0", "Old Track", "Old Artist", "yt0");
    app.tracks = vec![
        old_track.clone(),
        make_spotify_track("sp1", "New Track", "New Artist"),
    ];
    app.playing = Some(0);
    app.set_playing(old_track.clone());

    // User selects Track 1 (unresolved Spotify track)
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );

    // Track 0 must remain playing while Track 1 is resolving in background
    assert!(app.is_resolving());
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Old Track"),
        "now_playing must NOT be overwritten before resolution finishes"
    );
    assert_eq!(
        app.pending_track().map(|t| t.title.as_str()),
        Some("New Track"),
        "pending_track must hold the track undergoing resolution"
    );
    assert_eq!(app.playing, Some(0), "playing index must stay on old track");

    // User cancels resolution
    app.cancel_pending_resolution();
    app.set_status("RESOLVE CANCELLED");

    assert!(!app.is_resolving());
    assert!(app.pending_track().is_none());
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Old Track"),
        "now_playing must remain intact after cancellation"
    );
    assert_eq!(app.playing, Some(0), "playing index must remain intact");

    // Stale response arriving after cancel must be discarded
    let stale_resp = ResolveResponse {
        generation: 1,
        source: PlaybackSource::SearchResult { index: 1 },
        spotify_id: Some("sp1".into()),
        title: "New Track".into(),
        artist: "New Artist".into(),
        result: Ok(make_resolved_track("sp1", "New Track", "New Artist", "yt1")),
    };
    let handled =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, stale_resp);
    assert!(!handled, "Stale response after cancel must be discarded");
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Old Track")
    );
}

#[test]
fn test_resolution_failure_preserves_playing_track() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Initial state: Track 0 is currently playing
    let old_track = make_resolved_track("sp0", "Current Song", "Current Artist", "yt0");
    app.tracks = vec![
        old_track.clone(),
        make_spotify_track("sp1", "Failing Song", "Failing Artist"),
    ];
    app.playing = Some(0);
    app.set_playing(old_track.clone());

    // Request track 1
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    let generation = app.resolve_generation;

    // Simulate resolution failure
    let fail_resp = ResolveResponse {
        generation,
        source: PlaybackSource::SearchResult { index: 1 },
        spotify_id: Some("sp1".into()),
        title: "Failing Song".into(),
        artist: "Failing Artist".into(),
        result: Err("No matching YouTube video found".into()),
    };

    let handled =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, fail_resp);
    assert!(!handled, "Failed resolution must return false");
    assert!(!app.is_resolving());
    assert!(app.pending_track().is_none());
    assert!(app.status.contains("RESOLVE ERROR"));
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Current Song"),
        "now_playing must NOT be cleared when pending resolution fails"
    );
    assert_eq!(
        app.playing,
        Some(0),
        "playing index must NOT be cleared when pending resolution fails"
    );
}

#[test]
fn test_resolution_failure_when_idle_cleans_up() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.tracks = vec![make_spotify_track("sp0", "Idle Fail", "Artist")];
    app.playing = None;
    app.set_stopped();

    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    let generation = app.resolve_generation;

    let fail_resp = ResolveResponse {
        generation,
        source: PlaybackSource::SearchResult { index: 0 },
        spotify_id: Some("sp0".into()),
        title: "Idle Fail".into(),
        artist: "Artist".into(),
        result: Err("Network timeout".into()),
    };

    let handled =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, fail_resp);
    assert!(!handled);
    assert!(!app.is_resolving());
    assert!(app.pending_track().is_none());
    assert!(app.now_playing().is_none());
    assert!(app.playing.is_none());
    assert!(app.status.contains("RESOLVE ERROR"));
}

#[test]
fn test_stale_response_rejected_on_slot_mismatch() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Search results replaced while resolution was in-flight
    app.tracks = vec![make_spotify_track(
        "different_id",
        "Different Track",
        "Different Artist",
    )];
    app.set_resolving(
        make_spotify_track("old_id", "Old Track", "Old Artist"),
        PlaybackSource::SearchResult { index: 0 },
    );
    app.resolve_generation = 1;

    // Incoming response claims index 0, matching generation, but identity is from old search
    let response = ResolveResponse {
        generation: 1,
        source: PlaybackSource::SearchResult { index: 0 },
        spotify_id: Some("old_id".into()),
        title: "Old Track".into(),
        artist: "Old Artist".into(),
        result: Ok(make_resolved_track(
            "old_id",
            "Old Track",
            "Old Artist",
            "yt_old",
        )),
    };

    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, response);
    assert!(!handled, "Mismatched slot identity must reject playback");
    assert_eq!(
        app.tracks[0].title, "Different Track",
        "Slot must not be overwritten by mismatched track"
    );
}

#[test]
fn test_queue_track_resolution_plays_directly_without_touching_search_results() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Search results present
    let search_track =
        make_resolved_track("sp_search", "Search Song", "Search Artist", "yt_search");
    app.tracks = vec![search_track.clone()];
    app.playing = Some(0);
    app.set_playing(search_track);

    // Enqueue an unresolved Spotify track
    let queue_track = make_spotify_track("sp_queue", "Queued Song", "Queued Artist");
    let entry_id = app.enqueue(queue_track.clone()).unwrap();

    // Play the queued track
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        queue_track,
        PlaybackSource::Queue { entry_id },
    );

    assert!(app.is_resolving());
    let generation = app.resolve_generation;

    // Resolution arrives for the queue track
    let resolved_queue_track =
        make_resolved_track("sp_queue", "Queued Song", "Queued Artist", "yt_queue");
    let response = ResolveResponse {
        generation,
        source: PlaybackSource::Queue { entry_id },
        spotify_id: Some("sp_queue".into()),
        title: "Queued Song".into(),
        artist: "Queued Artist".into(),
        result: Ok(resolved_queue_track),
    };

    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, response);
    assert!(handled, "Queue resolution must be accepted");
    assert!(!app.is_resolving());

    // Critical assertions:
    // app.tracks[0] was NOT overwritten by the queue track
    assert_eq!(app.tracks.len(), 1);
    assert_eq!(app.tracks[0].title, "Search Song");
    assert_eq!(
        app.tracks[0].stream_url,
        "https://music.youtube.com/watch?v=yt_search"
    );

    // Queue track is playing directly
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("Queued Song")
    );

    // Queue track is not represented as an index in app.tracks
    assert_eq!(
        app.playing, None,
        "Queued track must have app.playing = None"
    );
}

#[test]
fn test_concurrent_search_and_resolve_non_blocking() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    use ansimuz::config::Provider;
    use ansimuz::music::{resolve_playable, search_music};
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use std::ffi::CString;

    // Install mock bridge functions with a synchronization barrier in Python
    Python::attach(|py| -> PyResult<()> {
        let sys = PyModule::import(py, "sys")?;
        let path = sys.getattr("path")?.cast_into::<pyo3::types::PyList>()?;
        let backend_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend");
        path.insert(0, backend_path.to_string_lossy().as_ref())?;

        let bridge = PyModule::import(py, "bridge")?;
        let locals = PyDict::new(py);
        locals.set_item("bridge", bridge)?;
        let code = CString::new(
            r#"
import bridge
import threading

bridge.orig_search = getattr(bridge, "orig_search", bridge.search_music)
bridge.orig_resolve = getattr(bridge, "orig_resolve", bridge.resolve_playable)
bridge.barrier = threading.Barrier(2)

def mock_search(query, provider="ytmusic"):
    import bridge
    if query == "deterministic_mock_query_for_concurrency":
        bridge.barrier.wait(timeout=3)
        return [{
            "title": f"Mock for {query}",
            "artist": "Mock Artist",
            "stream_url": "https://youtube.com/watch?v=mock_search",
            "art_url": "",
            "source": provider,
        }]
    return bridge.orig_search(query, provider)

def mock_resolve(track):
    import bridge
    if track.get("spotify_id") == "mock_spotify_id_barrier_test":
        bridge.barrier.wait(timeout=3)
        resolved = dict(track)
        resolved["stream_url"] = "https://music.youtube.com/watch?v=mock_resolved"
        resolved["resolved"] = True
        return resolved
    return bridge.orig_resolve(track)

bridge.search_music = mock_search
bridge.resolve_playable = mock_resolve
"#,
        )
        .unwrap();
        py.run(code.as_c_str(), None, Some(&locals))?;
        Ok(())
    })
    .expect("Failed to install mock bridge functions with barrier");

    let sample = Track {
        title: "Mock Title".to_string(),
        artist: "Mock Artist".to_string(),
        stream_url: String::new(),
        art_url: String::new(),
        spotify_id: Some("mock_spotify_id_barrier_test".to_string()),
        artists: vec!["Mock Artist".to_string()],
        album: Some("Mock Album".to_string()),
        duration_ms: Some(180_000),
    };

    let t_search = std::thread::spawn(|| {
        search_music(
            "deterministic_mock_query_for_concurrency".into(),
            Provider::Spotify,
        )
    });
    let sample_clone = sample.clone();
    let t_resolve = std::thread::spawn(move || resolve_playable(&sample_clone));

    let search_res = t_search.join().expect("search thread must not panic");
    let resolve_res = t_resolve.join().expect("resolve thread must not panic");

    // Restore original bridge functions
    let _ = Python::attach(|py| -> PyResult<()> {
        let bridge = PyModule::import(py, "bridge")?;
        let locals = PyDict::new(py);
        locals.set_item("bridge", bridge)?;
        let code = CString::new(
            r#"
if hasattr(bridge, "orig_search"):
    bridge.search_music = bridge.orig_search
if hasattr(bridge, "orig_resolve"):
    bridge.resolve_playable = bridge.orig_resolve
"#,
        )
        .unwrap();
        py.run(code.as_c_str(), None, Some(&locals))?;
        Ok(())
    });

    assert_eq!(search_res.0, "deterministic_mock_query_for_concurrency");
    let tracks = search_res.1.expect("Search must succeed concurrently");
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        tracks[0].title,
        "Mock for deterministic_mock_query_for_concurrency"
    );

    let resolved = resolve_res.expect("Resolution must succeed concurrently");
    assert_eq!(
        resolved.stream_url,
        "https://music.youtube.com/watch?v=mock_resolved"
    );
}

#[test]
fn test_old_track_ended_preserves_in_flight_resolution_with_nonempty_queue() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    // Initial state: Track 0 is currently playing
    let t0 = make_resolved_track("inflight_end_0", "End Track 0", "End Artist 0", "yt0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("End Track 0")
    );

    // Non-empty queue: Queued Track
    let q_track = make_resolved_track(
        "inflight_end_q",
        "End Queued Track",
        "End Queued Artist",
        "ytq",
    );
    app.enqueue(q_track);
    assert_eq!(app.queue.len(), 1);

    // User selects Track 1 (unresolved Spotify track) from search results
    let t1_unresolved = make_spotify_track("inflight_end_1", "End Track 1", "End Artist 1");
    app.tracks = vec![
        make_resolved_track("inflight_end_0", "End Track 0", "End Artist 0", "yt0"),
        t1_unresolved,
    ];
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    assert!(app.is_resolving());
    assert_eq!(app.pending_track().unwrap().title, "End Track 1");

    // While Track 1 is resolving, Track 0 finishes playing
    let handled = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen0 },
        &mut playback,
    );
    assert!(
        handled,
        "Ended event for current generation must be handled"
    );

    // Critical assertions: In-flight resolution must NOT be cancelled!
    // The queue item must NOT have been played/consumed over the user's selection.
    assert!(app.is_resolving(), "Resolution must remain in flight");
    assert_eq!(app.pending_track().unwrap().title, "End Track 1");
    assert_eq!(app.queue.len(), 1, "Queue must remain untouched");
    assert_eq!(app.queue[0].track.title, "End Queued Track");
    assert_eq!(app.history.len(), 1, "Finished Track 0 must be in history");
    assert_eq!(app.history[0].title, "End Track 0");
    assert!(playback.is_none());

    // When resolution of Track 1 finishes, it plays successfully
    let t1_resolved = make_resolved_track("inflight_end_1", "End Track 1", "End Artist 1", "yt1");
    let resp = ResolveResponse {
        generation: app.resolve_generation,
        source: PlaybackSource::SearchResult { index: 1 },
        spotify_id: Some("inflight_end_1".into()),
        title: "End Track 1".into(),
        artist: "End Artist 1".into(),
        result: Ok(t1_resolved),
    };
    let handled_resolve =
        handle_resolve_response(&mut app, &mut player, &player_tx, &resolve_tx, resp);
    assert!(handled_resolve);
    assert_eq!(
        app.now_playing().map(|t| t.title.as_str()),
        Some("End Track 1")
    );
    assert!(!app.is_resolving());
    assert_eq!(app.queue.len(), 1, "Queued track still intact in queue");
}

#[test]
fn test_old_track_failed_preserves_in_flight_resolution_with_nonempty_queue() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    let t0 = make_resolved_track("inflight_fail_0", "Fail Track 0", "Fail Artist 0", "yt0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();

    app.enqueue(make_resolved_track(
        "inflight_fail_q",
        "Fail Queued Track",
        "Fail Queued Artist",
        "ytq",
    ));
    assert_eq!(app.queue.len(), 1);

    let t1_unresolved = make_spotify_track("inflight_fail_1", "Fail Track 1", "Fail Artist 1");
    app.tracks = vec![
        make_resolved_track("inflight_fail_0", "Fail Track 0", "Fail Artist 0", "yt0"),
        t1_unresolved,
    ];
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    assert!(app.is_resolving());

    // Track 0 fails while Track 1 is resolving
    let handled = handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Failed {
            generation: gen0,
            message: "Stream read error".into(),
        },
        &mut playback,
    );
    assert!(handled);

    // In-flight resolution must be preserved
    assert!(
        app.is_resolving(),
        "Resolution must remain in flight after old track failure"
    );
    assert_eq!(app.pending_track().unwrap().title, "Fail Track 1");
    assert_eq!(
        app.queue.len(),
        1,
        "Queue must not advance over in-flight resolution"
    );
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Fail Track 0");
}

#[test]
fn test_old_track_ended_preserves_in_flight_resolution_with_repeat_one() {
    let _guard = RESOLUTION_TEST_LOCK.lock().unwrap();
    let mut app = App::new();
    app.repeat = Repeat::One;
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    let t0 = make_resolved_track("inflight_rep_0", "Rep Track 0", "Rep Artist 0", "yt0");
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        t0,
        PlaybackSource::SearchResult { index: 0 },
    );
    let gen0 = player.generation();

    let t1_unresolved = make_spotify_track("inflight_rep_1", "Rep Track 1", "Rep Artist 1");
    app.tracks = vec![
        make_resolved_track("inflight_rep_0", "Rep Track 0", "Rep Artist 0", "yt0"),
        t1_unresolved,
    ];
    play_search_result(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    assert!(app.is_resolving());

    // Track 0 ends while Track 1 is resolving (even with Repeat::One)
    handle_player_event(
        &mut app,
        &mut player,
        &player_tx,
        &resolve_tx,
        PlayerEvent::Ended { generation: gen0 },
        &mut playback,
    );

    // Repeat::One must NOT replay Track 0 and discard Track 1!
    assert!(app.is_resolving());
    assert_eq!(app.pending_track().unwrap().title, "Rep Track 1");
    assert_eq!(app.history.len(), 1);
    assert_eq!(app.history[0].title, "Rep Track 0");
}
