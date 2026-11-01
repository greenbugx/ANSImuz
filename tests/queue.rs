use std::sync::mpsc;

use ansimuz::app::events::handle_key;
use ansimuz::app::{App, ListFocus, Mode};
use ansimuz::music::{ResolveResponse, SearchResponse, Track};
use ansimuz::playback::{AudioPlayer, PlayerEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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
fn test_same_song_cannot_appear_twice() {
    let mut app = App::new();
    let track = test_track("Echoes", "Pink Floyd");

    let id1 = app.enqueue(track.clone());
    let id2 = app.enqueue(track);

    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].track.title, "Echoes");
    assert_eq!(id1, Some(1));
    assert_eq!(
        id2, None,
        "Duplicate song must return None and not be added"
    );
}

#[test]
fn test_queue_contains_matches_same_song() {
    let mut app = App::new();
    let track1 = test_track("Song 1", "Artist 1");
    let mut track1_dup = test_track("song 1", "artist 1");
    track1_dup.stream_url = track1.stream_url.clone();

    app.enqueue(track1);
    assert!(app.queue_contains(&track1_dup));

    let track2 = test_track("Song 2", "Artist 2");
    assert!(!app.queue_contains(&track2));
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

#[test]
fn test_focus_switching_preserves_selections() {
    let mut app = App::new();
    app.tracks = vec![test_track("S0", "A"), test_track("S1", "A")];
    app.enqueue(test_track("Q0", "A"));
    app.enqueue(test_track("Q1", "A"));

    assert_eq!(app.list_focus, ListFocus::Search);
    app.selected = 1;
    app.queue_selected = 0;

    app.toggle_focus();
    assert_eq!(app.list_focus, ListFocus::Queue);
    assert_eq!(app.selected, 1, "Search selection must be preserved");
    assert_eq!(app.queue_selected, 0, "Queue selection must be preserved");

    app.move_queue_selection(1);
    assert_eq!(app.queue_selected, 1);

    app.toggle_focus();
    assert_eq!(app.list_focus, ListFocus::Search);
    assert_eq!(app.selected, 1);
    assert_eq!(app.queue_selected, 1);
}

#[test]
fn test_focus_aware_keyboard_navigation_and_controls() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (search_tx, _search_rx) = mpsc::channel::<SearchResponse>();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    let make_key = |code: KeyCode, modifiers: KeyModifiers| KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };

    app.tracks = vec![test_track("Search 0", "A"), test_track("Search 1", "A")];

    // In Mode::Type, letter 'a' types into search input, does not enqueue
    app.mode = Mode::Type;
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.query, "a");
    assert!(
        app.queue.is_empty(),
        "'a' in Mode::Type must type, not enqueue"
    );

    // Switch to Mode::Nav
    app.mode = Mode::Nav;
    app.selected = 0;

    // In Mode::Nav with Search focus, 'A' enqueues highlighted search result
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].track.title, "Search 0");

    // Down to select Search 1 and enqueue it too
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Down, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.selected, 1);

    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[1].track.title, "Search 1");

    // Tab switches focus to Queue
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Tab, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.list_focus, ListFocus::Queue);

    // 'A' while Queue is focused does NOT enqueue search result
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(
        app.queue.len(),
        2,
        "'a' while Queue is focused must not enqueue"
    );

    // Down while Queue is focused moves queue_selected
    assert_eq!(app.queue_selected, 0);
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Down, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue_selected, 1);
    assert_eq!(app.selected, 1, "Search selected remains unchanged");

    // Alt+Up moves the selected queue item (Search 1 moves from 1 to 0)
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Up, KeyModifiers::ALT),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue[0].track.title, "Search 1");
    assert_eq!(app.queue[1].track.title, "Search 0");
    assert_eq!(
        app.queue_selected, 0,
        "queue_selected follows the moved item"
    );

    // Delete removes highlighted queue item (Search 1 at index 0)
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Delete, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].track.title, "Search 0");

    // Enter plays highlighted queue item
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Enter, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Search 0")
    );
    assert_eq!(app.queue.len(), 0, "Queue item consumed on play");
    assert_eq!(app.playing, None, "Queue playback has playing = None");
}

#[test]
fn test_stage_4_connected_interactions() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (search_tx, _search_rx) = mpsc::channel::<SearchResponse>();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    let make_key = |code: KeyCode, modifiers: KeyModifiers| KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };

    app.tracks = vec![
        test_track("Track A", "Artist"),
        test_track("Track B", "Artist"),
    ];

    // Add: Enqueue highlighted search track, verify status and duplicate handling
    app.mode = Mode::Nav;
    app.list_focus = ListFocus::Search;
    app.selected = 0;

    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].track.title, "Track A");
    assert!(app.status.contains("QUEUED: Track A"));

    // Enqueue duplicate
    // must be rejected with ALREADY IN QUEUE status
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(
        app.queue.len(),
        1,
        "Duplicate song must not be added to queue"
    );
    assert!(app.status.contains("ALREADY IN QUEUE"));

    // Add Track B as 2nd item
    app.selected = 1;
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Char('a'), KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[1].track.title, "Track B");

    // Add Track C as 3rd item
    app.enqueue(test_track("Track C", "Artist"));
    assert_eq!(app.queue.len(), 3);

    // Reorder: Switch to Queue focus, move Track C up from index 2 to index 1
    app.list_focus = ListFocus::Queue;
    app.queue_selected = 2; // on Track C

    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Up, KeyModifiers::ALT),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue[1].track.title, "Track C");
    assert_eq!(
        app.queue_selected, 1,
        "Cursor must follow moved item Track C to index 1"
    );

    // Remove: Delete Track C at index 1
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Delete, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 2);
    assert!(app.status.contains("REMOVED: Track C"));
    assert_eq!(app.queue_selected, 1, "Cursor clamps/adjusts correctly");
    assert_eq!(app.queue[0].track.title, "Track A");
    assert_eq!(app.queue[1].track.title, "Track B");

    // Add a distinct Track X at the end: queue is now [Track A (0), Track B (1), Track X (2)]
    app.enqueue(test_track("Track X", "Artist"));
    assert_eq!(app.queue.len(), 3);

    // Play: Highlight Track X at index 2, press Enter -> plays Track X directly
    app.queue_selected = 2;
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Enter, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Track X")
    );
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[0].track.title, "Track A");
    assert_eq!(app.queue[1].track.title, "Track B");

    // Enqueue Track D at end: queue is now [Track A, Track B, Track D]
    app.enqueue(test_track("Track D", "Artist"));
    assert_eq!(app.queue.len(), 3);

    // Manual Next: highlight Track D at index 2, then press Right (Next)
    // Invariant: Next MUST play the front of the queue (Track A at index 0), NOT highlighted selection Track D!
    app.queue_selected = 2; // highlight Track D
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Right, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(
        app.now_playing.as_ref().map(|t| t.title.as_str()),
        Some("Track A"),
        "Next button must prioritize front of queue (index 0), ignoring queue_selected cursor"
    );
    assert_eq!(app.queue.len(), 2);
    assert_eq!(app.queue[0].track.title, "Track B");
    assert_eq!(app.queue[1].track.title, "Track D");
}

#[test]
fn test_queue_mutations_ignored_when_search_focused() {
    let mut app = App::new();
    let mut player = AudioPlayer::new_mock();
    let (search_tx, _search_rx) = mpsc::channel::<SearchResponse>();
    let (player_tx, _player_rx) = mpsc::channel::<PlayerEvent>();
    let (resolve_tx, _resolve_rx) = mpsc::channel::<ResolveResponse>();

    let make_key = |code: KeyCode, modifiers: KeyModifiers| KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };

    app.tracks = vec![test_track("Search 0", "A"), test_track("Search 1", "A")];
    app.enqueue(test_track("Queue 0", "Artist"));
    app.enqueue(test_track("Queue 1", "Artist"));

    // Ensure list_focus is Search
    app.mode = Mode::Nav;
    app.list_focus = ListFocus::Search;
    app.queue_selected = 1;

    // Delete while Search is focused: must NOT delete anything from the queue
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Delete, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(
        app.queue.len(),
        2,
        "Delete while Search is focused must not remove queue items"
    );
    assert_eq!(app.queue[0].track.title, "Queue 0");
    assert_eq!(app.queue[1].track.title, "Queue 1");

    // Alt+Up while Search is focused: must NOT reorder the queue
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Up, KeyModifiers::ALT),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue[0].track.title, "Queue 0");
    assert_eq!(app.queue[1].track.title, "Queue 1");
    assert_eq!(
        app.queue_selected, 1,
        "Alt+Up while Search is focused must not reorder"
    );

    // Alt+Down while Search is focused: must NOT reorder the queue
    app.queue_selected = 0;
    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Down, KeyModifiers::ALT),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue[0].track.title, "Queue 0");
    assert_eq!(app.queue[1].track.title, "Queue 1");
    assert_eq!(
        app.queue_selected, 0,
        "Alt+Down while Search is focused must not reorder"
    );

    // Switch to Queue focus: now Delete, Alt+Up, Alt+Down must work
    app.list_focus = ListFocus::Queue;
    app.queue_selected = 1;

    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Up, KeyModifiers::ALT),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue[0].track.title, "Queue 1");
    assert_eq!(app.queue[1].track.title, "Queue 0");
    assert_eq!(app.queue_selected, 0);

    handle_key(
        &mut app,
        &mut player,
        make_key(KeyCode::Delete, KeyModifiers::empty()),
        &search_tx,
        &player_tx,
        &resolve_tx,
    );
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].track.title, "Queue 0");
}
