use std::sync::mpsc;

use ansimuz::app::{
    runner::{handle_resolve_response, play_track},
    state::App,
};
use ansimuz::music::{ResolveResponse, Track};
use ansimuz::playback::{AudioPlayer, PlayerEvent};

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

#[test]
fn test_rapid_track_switching_ignores_stale_responses() {
    let mut app = App::new();
    let mut player = AudioPlayer::new();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.tracks = vec![
        make_spotify_track("sp0", "Track Zero", "Artist Zero"),
        make_spotify_track("sp1", "Track One", "Artist One"),
        make_spotify_track("sp2", "Track Two", "Artist Two"),
    ];

    // Rapidly request track 0, then 1, then 2
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    assert_eq!(app.resolve_generation, 1);
    assert!(app.is_resolving);
    assert_eq!(
        app.pending_track.as_ref().map(|t| t.title.as_str()),
        Some("Track Zero")
    );

    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );
    assert_eq!(app.resolve_generation, 2);
    assert!(app.is_resolving);
    assert_eq!(
        app.pending_track.as_ref().map(|t| t.title.as_str()),
        Some("Track One")
    );

    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        2,
    );
    assert_eq!(app.resolve_generation, 3);
    assert!(app.is_resolving);
    assert_eq!(
        app.pending_track.as_ref().map(|t| t.title.as_str()),
        Some("Track Two")
    );

    // Stale response from request 0 arrives (generation 1)
    let stale_resp_0 = ResolveResponse {
        generation: 1,
        index: 0,
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
    let handled_0 = handle_resolve_response(&mut app, &mut player, &player_tx, stale_resp_0);
    assert!(!handled_0, "Stale generation 1 response must be ignored");
    assert!(app.is_resolving, "Track 2 must still be resolving");
    assert_eq!(
        app.pending_track.as_ref().map(|t| t.title.as_str()),
        Some("Track Two")
    );
    assert!(
        app.tracks[0].stream_url.is_empty(),
        "Track 0 slot must not be touched"
    );

    // Stale response from request 1 arrives (generation 2)
    let stale_resp_1 = ResolveResponse {
        generation: 2,
        index: 1,
        spotify_id: Some("sp1".into()),
        title: "Track One".into(),
        artist: "Artist One".into(),
        result: Ok(make_resolved_track("sp1", "Track One", "Artist One", "yt1")),
    };
    let handled_1 = handle_resolve_response(&mut app, &mut player, &player_tx, stale_resp_1);
    assert!(!handled_1, "Stale generation 2 response must be ignored");
    assert!(app.is_resolving);

    // Fresh response from request 2 arrives (generation 3)
    let fresh_resp_2 = ResolveResponse {
        generation: 3,
        index: 2,
        spotify_id: Some("sp2".into()),
        title: "Track Two".into(),
        artist: "Artist Two".into(),
        result: Ok(make_resolved_track("sp2", "Track Two", "Artist Two", "yt2")),
    };
    let handled_2 = handle_resolve_response(&mut app, &mut player, &player_tx, fresh_resp_2);
    assert!(handled_2, "Fresh generation 3 response must be accepted");
    assert!(!app.is_resolving);
    assert!(app.pending_track.is_none());
    assert!(!app.tracks[2].stream_url.is_empty());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Track Two")
    );
}

#[test]
fn test_cancelling_while_old_track_is_playing_preserves_playback() {
    let mut app = App::new();
    let mut player = AudioPlayer::new();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Initial state: Track 0 is currently playing
    let old_track = make_resolved_track("sp0", "Old Track", "Old Artist", "yt0");
    app.tracks = vec![
        old_track.clone(),
        make_spotify_track("sp1", "New Track", "New Artist"),
    ];
    app.playing = Some(0);
    app.now_playing = Some(old_track.clone());

    // User selects Track 1 (unresolved Spotify track)
    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        1,
    );

    // Track 0 must remain playing while Track 1 is resolving in background
    assert!(app.is_resolving);
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Old Track"),
        "now_playing must NOT be overwritten before resolution finishes"
    );
    assert_eq!(
        app.pending_track.as_ref().map(|t| t.title.as_str()),
        Some("New Track"),
        "pending_track must hold the track undergoing resolution"
    );
    assert_eq!(app.playing, Some(0), "playing index must stay on old track");

    // User cancels resolution
    app.cancel_pending_resolution();
    app.set_status("RESOLVE CANCELLED");

    assert!(!app.is_resolving);
    assert!(app.pending_track.is_none());
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Old Track"),
        "now_playing must remain intact after cancellation"
    );
    assert_eq!(app.playing, Some(0), "playing index must remain intact");

    // Stale response arriving after cancel must be discarded
    let stale_resp = ResolveResponse {
        generation: 1,
        index: 1,
        spotify_id: Some("sp1".into()),
        title: "New Track".into(),
        artist: "New Artist".into(),
        result: Ok(make_resolved_track("sp1", "New Track", "New Artist", "yt1")),
    };
    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, stale_resp);
    assert!(!handled, "Stale response after cancel must be discarded");
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Old Track")
    );
}

#[test]
fn test_resolution_failure_preserves_playing_track() {
    let mut app = App::new();
    let mut player = AudioPlayer::new();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    // Initial state: Track 0 is currently playing
    let old_track = make_resolved_track("sp0", "Current Song", "Current Artist", "yt0");
    app.tracks = vec![
        old_track.clone(),
        make_spotify_track("sp1", "Failing Song", "Failing Artist"),
    ];
    app.playing = Some(0);
    app.now_playing = Some(old_track.clone());

    // Request track 1
    play_track(
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
        index: 1,
        spotify_id: Some("sp1".into()),
        title: "Failing Song".into(),
        artist: "Failing Artist".into(),
        result: Err("No matching YouTube video found".into()),
    };

    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, fail_resp);
    assert!(!handled, "Failed resolution must return false");
    assert!(!app.is_resolving);
    assert!(app.pending_track.is_none());
    assert!(app.status.contains("RESOLVE ERROR"));
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
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
    let mut app = App::new();
    let mut player = AudioPlayer::new();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    app.tracks = vec![make_spotify_track("sp0", "Idle Fail", "Artist")];
    app.playing = None;
    app.now_playing = None;

    play_track(
        &mut app,
        &mut player,
        player_tx.clone(),
        resolve_tx.clone(),
        0,
    );
    let generation = app.resolve_generation;

    let fail_resp = ResolveResponse {
        generation,
        index: 0,
        spotify_id: Some("sp0".into()),
        title: "Idle Fail".into(),
        artist: "Artist".into(),
        result: Err("Network timeout".into()),
    };

    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, fail_resp);
    assert!(!handled);
    assert!(!app.is_resolving);
    assert!(app.pending_track.is_none());
    assert!(app.now_playing.is_none());
    assert!(app.playing.is_none());
    assert!(app.status.contains("RESOLVE ERROR"));
}

#[test]
fn test_stale_response_rejected_on_slot_mismatch() {
    let mut app = App::new();
    let mut player = AudioPlayer::new();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();

    // Search results replaced while resolution was in-flight
    app.tracks = vec![make_spotify_track(
        "different_id",
        "Different Track",
        "Different Artist",
    )];
    app.resolve_generation = 1;
    app.is_resolving = true;

    // Incoming response claims index 0, matching generation, but identity is from old search
    let response = ResolveResponse {
        generation: 1,
        index: 0,
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

    let handled = handle_resolve_response(&mut app, &mut player, &player_tx, response);
    assert!(!handled, "Mismatched slot identity must reject playback");
    assert_eq!(
        app.tracks[0].title, "Different Track",
        "Slot must not be overwritten by mismatched track"
    );
}

#[test]
fn test_concurrent_search_and_resolve_non_blocking() {
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
