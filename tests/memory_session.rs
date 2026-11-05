use ansimuz::app::{
    MAX_HISTORY_SIZE, handle_player_event, handle_search_result, play_track, state::App,
};
use ansimuz::artwork::{
    CacheEntry, MAX_ARTWORK_CACHE_ENTRIES, art_cache, art_cache_len, clear_memory_cache,
};
use ansimuz::config::clear_caches;
use ansimuz::music::{
    MAX_RESOLVED_CACHE_ENTRIES, PlaybackSource, ResolveResponse, SearchResponse, Track,
    cache_resolved_track, clear_resolved_cache, get_cached_track, resolved_cache_len,
};
use ansimuz::playback::{AudioPlayer, PlayerEvent};
use image::DynamicImage;
use std::sync::Arc;
use std::sync::mpsc;

fn make_test_track(id: &str, title: &str, stream_url: &str) -> Track {
    Track {
        title: title.into(),
        artist: "Artist".into(),
        stream_url: stream_url.into(),
        art_url: format!("https://img.youtube.com/vi/{id}/hqdefault.jpg"),
        spotify_id: Some(id.into()),
        artists: vec!["Artist".into()],
        album: Some("Album".into()),
        duration_ms: Some(180_000),
    }
}
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_resolved_cache_is_strictly_bounded_under_hundreds_of_resolutions() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_resolved_cache();
    assert_eq!(resolved_cache_len(), 0);
    // Simulate caching 300 tracks with distinct spotify_ids and titles
    for i in 0..300 {
        let t = make_test_track(
            &format!("sp_track_{i}"),
            &format!("Resolved Title {i}"),
            &format!("http://stream/{i}"),
        );
        cache_resolved_track(&t);
    }

    // Cache must never exceed MAX_RESOLVED_CACHE_ENTRIES
    assert!(
        resolved_cache_len() <= MAX_RESOLVED_CACHE_ENTRIES,
        "Resolved track cache ({}) must be bounded by MAX_RESOLVED_CACHE_ENTRIES ({})",
        resolved_cache_len(),
        MAX_RESOLVED_CACHE_ENTRIES
    );

    // Oldest tracks should have been evicted
    let oldest = make_test_track("sp_track_0", "Resolved Title 0", "");
    assert!(
        get_cached_track(&oldest).is_none(),
        "Oldest track must have been evicted from resolved cache"
    );

    // Recent tracks must remain cached
    let recent = make_test_track("sp_track_299", "Resolved Title 299", "");
    assert!(
        get_cached_track(&recent).is_some(),
        "Recent track must remain in resolved cache"
    );

    clear_resolved_cache();
    assert_eq!(resolved_cache_len(), 0);
}

#[test]
fn test_artwork_cache_is_strictly_bounded_under_hundreds_of_artwork_loads() {
    let _guard = TEST_LOCK.lock().unwrap();
    clear_memory_cache();
    assert_eq!(art_cache_len(), 0);

    // Populate art_cache with 100 entries
    let dummy = Arc::new(DynamicImage::new_rgb8(10, 10));
    {
        let mut guard = art_cache().lock().unwrap();
        for i in 0..100 {
            let key = (format!("https://example.com/art_{i}"), 64);
            guard.insert(key, CacheEntry::Ready(Arc::clone(&dummy)));
        }
    }

    // Memory cache must be strictly bounded at MAX_ARTWORK_CACHE_ENTRIES
    assert!(
        art_cache_len() <= MAX_ARTWORK_CACHE_ENTRIES,
        "Art cache ({}) must not exceed MAX_ARTWORK_CACHE_ENTRIES ({})",
        art_cache_len(),
        MAX_ARTWORK_CACHE_ENTRIES
    );

    clear_memory_cache();
    assert_eq!(art_cache_len(), 0);
}

#[test]
fn test_repeated_searches_with_changing_generations_do_not_leak_state() {
    let mut app = App::new();

    // Perform 250 search cycles with increasing generations
    for search_gen in 1..=250 {
        app.search_generation = search_gen;
        let tracks = vec![
            make_test_track(
                &format!("sp_{search_gen}_0"),
                &format!("Track {search_gen}-0"),
                "http://s/0",
            ),
            make_test_track(
                &format!("sp_{search_gen}_1"),
                &format!("Track {search_gen}-1"),
                "http://s/1",
            ),
        ];

        let handled = handle_search_result(
            &mut app,
            SearchResponse {
                generation: search_gen,
                query: format!("query {search_gen}"),
                result: Ok(tracks),
            },
        );

        assert!(handled);
        assert_eq!(app.tracks.len(), 2);
        assert_eq!(app.last_query, format!("query {search_gen}"));
    }

    // Outdated generation responses must be discarded
    let stale_handled = handle_search_result(
        &mut app,
        SearchResponse {
            generation: 10,
            query: "stale".into(),
            result: Ok(vec![]),
        },
    );
    assert!(!stale_handled);
    assert_eq!(app.tracks.len(), 2);
}

#[test]
fn test_repeated_track_changes_and_playback_failures_bound_history() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();
    let mut playback = None;

    // Simulate 200 track playbacks and failures
    for i in 0..200 {
        let t = make_test_track(
            &format!("t_{i}"),
            &format!("Track {i}"),
            &format!("http://s/{i}"),
        );
        play_track(
            &mut app,
            &mut player,
            player_tx.clone(),
            resolve_tx.clone(),
            t,
            PlaybackSource::SearchResult { index: 0 },
        );
        let track_gen = player.generation();

        // Alternate between natural finish and failure
        let event = if i % 2 == 0 {
            PlayerEvent::Ended {
                generation: track_gen,
            }
        } else {
            PlayerEvent::Failed {
                generation: track_gen,
                message: format!("Error on {i}"),
            }
        };

        handle_player_event(
            &mut app,
            &mut player,
            &player_tx,
            &resolve_tx,
            event,
            &mut playback,
        );
    }

    // Playback history must be capped at MAX_HISTORY_SIZE
    assert_eq!(
        app.history.len(),
        MAX_HISTORY_SIZE,
        "History length must be strictly capped at MAX_HISTORY_SIZE (100)"
    );

    // Oldest tracks should not be in history
    assert_eq!(app.history[0].title, "Track 100");
    assert_eq!(app.history.back().unwrap().title, "Track 199");
}

#[test]
fn test_repeated_queue_additions_and_removals_maintain_consistent_state() {
    let mut app = App::new();

    // Add and remove 300 queue items
    for i in 0..300 {
        let t = make_test_track(
            &format!("q_{i}"),
            &format!("Queued {i}"),
            &format!("http://q/{i}"),
        );
        app.enqueue(t);
        assert!(app.queue.len() <= 1);
        let removed = app.remove_queue_item(0);
        assert!(removed.is_some());
        assert_eq!(app.queue.len(), 0);
        assert_eq!(app.queue_selected, 0);
    }

    // Queue is empty and cursor is clamped to 0
    assert!(app.queue.is_empty());
    assert_eq!(app.queue_selected, 0);
}

#[test]
fn test_repeated_cache_clearing_safely_resets_all_memory_and_disk_caches() {
    let _guard = TEST_LOCK.lock().unwrap();
    for _ in 0..5 {
        // Populate resolved cache
        let t = make_test_track("sp_c", "Song Clear", "http://stream/c");
        cache_resolved_track(&t);
        assert!(resolved_cache_len() > 0);

        // Populate art cache
        let dummy = Arc::new(DynamicImage::new_rgb8(10, 10));
        art_cache().lock().unwrap().insert(
            ("https://example.com/clear_test".into(), 64),
            CacheEntry::Ready(dummy),
        );
        assert!(art_cache_len() > 0);

        // Clear all caches
        let res = clear_caches();
        assert!(res.is_ok());

        // Verify all memory caches are empty
        assert_eq!(resolved_cache_len(), 0);
        assert_eq!(art_cache_len(), 0);
    }
}
