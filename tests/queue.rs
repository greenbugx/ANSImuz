use ansimuz::app::App;
use ansimuz::music::Track;

fn test_track(title: &str, artist: &str) -> Track {
    Track {
        title: title.to_string(),
        artist: artist.to_string(),
        stream_url: format!("https://example.com/{title}.mp3"),
        art_url: String::new(),
        spotify_id: None,
        artists: vec![artist.to_string()],
        album: Some("Test Album".to_string()),
        duration_ms: Some(180_000),
    }
}

#[test]
fn test_adding_three_songs_preserves_order() {
    let mut app = App::new();
    app.enqueue(test_track("Song 1", "Artist 1"));
    app.enqueue(test_track("Song 2", "Artist 2"));
    app.enqueue(test_track("Song 3", "Artist 3"));

    assert_eq!(app.queue.len(), 3);
    assert_eq!(app.queue[0].track.title, "Song 1");
    assert_eq!(app.queue[1].track.title, "Song 2");
    assert_eq!(app.queue[2].track.title, "Song 3");
}

#[test]
fn test_next_song_is_always_first_queued_song() {
    let mut app = App::new();
    app.enqueue(test_track("First Track", "Artist A"));
    app.enqueue(test_track("Second Track", "Artist B"));
    app.enqueue(test_track("Third Track", "Artist C"));

    let next = app.pop_next_queued();
    assert!(next.is_some());
    assert_eq!(next.unwrap().title, "First Track");
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[0].track.title, "Second Track");
}

#[test]
fn test_same_song_can_appear_twice() {
    let mut app = App::new();
    let track = test_track("Echoes", "Pink Floyd");

    let id1 = app.enqueue(track.clone());
    let id2 = app.enqueue(track);

    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[0].track.title, "Echoes");
    assert_eq!(app.queue[1].track.title, "Echoes");
    assert_ne!(id1, id2, "Duplicate songs must receive distinct entry IDs");
    assert_ne!(app.queue[0].id, app.queue[1].id);
}

#[test]
fn test_removing_one_entry_does_not_remove_duplicate() {
    let mut app = App::new();
    let track = test_track("Echoes", "Pink Floyd");

    let id1 = app.enqueue(track.clone());
    let id2 = app.enqueue(track);

    assert_eq!(app.queue.len(), 2);

    let removed = app.remove_queue_item(0);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().id, id1);

    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].id, id2);
    assert_eq!(app.queue[0].track.title, "Echoes");
}

#[test]
fn test_moving_entry_changes_order_correctly() {
    let mut app = App::new();
    app.enqueue(test_track("A", "Artist"));
    app.enqueue(test_track("B", "Artist"));
    app.enqueue(test_track("C", "Artist"));
    app.enqueue(test_track("D", "Artist"));

    let moved = app.move_queue_item(0, 2);
    assert!(moved);
    assert_eq!(app.queue[0].track.title, "B");
    assert_eq!(app.queue[1].track.title, "C");
    assert_eq!(app.queue[2].track.title, "A");
    assert_eq!(app.queue[3].track.title, "D");

    let moved_back = app.move_queue_item(3, 0);
    assert!(moved_back);
    assert_eq!(app.queue[0].track.title, "D");
    assert_eq!(app.queue[1].track.title, "B");
    assert_eq!(app.queue[2].track.title, "C");
    assert_eq!(app.queue[3].track.title, "A");
}

#[test]
fn test_removing_or_consuming_entries_keeps_queue_selected_bounded() {
    let mut app = App::new();
    app.enqueue(test_track("Track 1", "Artist"));
    app.enqueue(test_track("Track 2", "Artist"));
    app.enqueue(test_track("Track 3", "Artist"));

    app.queue_selected = 2;

    let removed = app.remove_queue_item(2);
    assert!(removed.is_some());
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue_selected, 1);

    let popped = app.pop_next_queued();
    assert!(popped.is_some());
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue_selected, 0);

    let popped_last = app.pop_next_queued();
    assert!(popped_last.is_some());
    assert!(app.queue.is_empty());
    assert_eq!(app.queue_selected, 0);

    assert!(app.pop_next_queued().is_none());
    assert_eq!(app.queue_selected, 0);
}
