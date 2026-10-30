use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::Settings;
use crate::music::Track;
use crate::playback::Repeat;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Type,
    Nav,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListFocus {
    Search,
    Queue,
}

#[derive(Clone, Debug)]
pub struct QueueEntry {
    pub id: u64,
    pub track: Track,
}

pub struct App {
    pub mode: Mode,
    pub list_focus: ListFocus,
    pub query: String,
    pub last_query: String,
    pub tracks: Vec<Track>,
    pub selected: usize,
    pub playing: Option<usize>,
    pub now_playing: Option<Track>,
    pub played_once: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    rng_state: u64,
    pub loading: bool,
    pub paused: bool,
    pub volume: u8,
    pub status: String,
    status_at: Option<Instant>,
    pub settings: Settings,
    pub settings_open: bool,
    pub settings_cursor: usize,
    pub should_quit: bool,
    pub help_open: bool,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_cursor: usize,
    pub pending_track: Option<Track>,
    pub is_resolving: bool,
    pub resolve_generation: u64,
    pub queue: Vec<QueueEntry>,
    pub queue_selected: usize,
    pub next_queue_id: u64,
    pub search_resume_index: Option<usize>,
    pub pending_queue_entry_id: Option<u64>,
    pub history: Vec<Track>,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Nav,
            list_focus: ListFocus::Search,
            query: String::new(),
            last_query: String::new(),
            tracks: Vec::new(),
            selected: 0,
            playing: None,
            now_playing: None,
            played_once: false,
            shuffle: false,
            repeat: Repeat::Off,
            rng_state: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15)
                | 1,
            loading: false,
            paused: false,
            volume: 70,
            status: String::new(),
            status_at: None,
            settings: Settings::load(),
            settings_open: false,
            settings_cursor: 0,
            should_quit: false,
            help_open: false,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_cursor: 0,
            pending_track: None,
            is_resolving: false,
            resolve_generation: 0,
            queue: Vec::new(),
            queue_selected: 0,
            next_queue_id: 1,
            history: Vec::new(),
            search_resume_index: None,
            pending_queue_entry_id: None,
        }
    }

    pub fn open_command_palette(&mut self) {
        self.command_palette_open = true;
        self.command_palette_query.clear();
        self.command_palette_cursor = 0;
        self.help_open = false;
    }

    pub fn close_command_palette(&mut self) {
        self.command_palette_open = false;
        self.command_palette_query.clear();
        self.command_palette_cursor = 0;
    }

    pub fn cancel_pending_resolution(&mut self) {
        self.resolve_generation = self.resolve_generation.wrapping_add(1);
        self.is_resolving = false;
        self.pending_track = None;
        self.pending_queue_entry_id = None;
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
        if self.help_open {
            self.command_palette_open = false;
        }
    }

    pub fn apply_setting(&mut self) {
        match self.settings_cursor {
            0 => self.settings.provider = self.settings.provider.toggle(),
            1 => self.settings.audio_cache = !self.settings.audio_cache,
            2 => self.settings.art_hq = !self.settings.art_hq,
            3 => self.settings.art_palette = !self.settings.art_palette,
            _ => {
                crate::config::clear_caches();
                self.set_status("CACHES CLEARED");
                return;
            }
        }

        self.settings.save();
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_at = Some(Instant::now());
    }

    pub fn visible_status(&self) -> String {
        if self.loading {
            return format!(" {} ", crate::artwork::loading_moons());
        }

        match self.status_at {
            Some(at) if at.elapsed() < Duration::from_secs(3) => self.status.clone(),
            _ => String::new(),
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.tracks.is_empty() {
            self.selected = 0;
            return;
        }

        let len = self.tracks.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    pub fn toggle_focus(&mut self) {
        self.list_focus = match self.list_focus {
            ListFocus::Search => ListFocus::Queue,
            ListFocus::Queue => ListFocus::Search,
        };
    }

    pub fn move_queue_selection(&mut self, delta: isize) {
        if self.queue.is_empty() {
            self.queue_selected = 0;
            return;
        }

        let len = self.queue.len() as isize;
        self.queue_selected = (self.queue_selected as isize + delta).rem_euclid(len) as usize;
    }

    pub fn move_selected_queue_item(&mut self, delta: isize) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        let current = self.queue_selected;
        let target = if delta < 0 {
            if current == 0 {
                return false;
            }
            current - 1
        } else {
            if current + 1 >= self.queue.len() {
                return false;
            }
            current + 1
        };
        self.move_queue_item(current, target)
    }

    pub fn enqueue_selected(&mut self) -> Option<u64> {
        if let Some(track) = self.tracks.get(self.selected).cloned() {
            let title = track.title.clone();
            let id = self.enqueue(track);
            self.set_status(format!("QUEUED: {}", title));
            Some(id)
        } else {
            None
        }
    }

    pub fn next_index(&self, delta: isize) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }

        let len = self.tracks.len() as isize;
        let base = self
            .playing
            .or(self.search_resume_index)
            .unwrap_or(self.selected) as isize;
        Some((base + delta).rem_euclid(len) as usize)
    }

    pub fn shuffled_next(&mut self) -> Option<usize> {
        let len = self.tracks.len();
        if len < 2 {
            return None;
        }

        let current = self
            .playing
            .or(self.search_resume_index)
            .unwrap_or(self.selected);
        let after = len.saturating_sub(current + 1);

        let index = if after > 0 {
            current + 1 + self.next_random(after)
        } else {
            self.next_random(len)
        };

        Some(index)
    }

    pub fn next_random(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;

        (self.rng_state % bound as u64) as usize
    }

    pub fn push_front_queue(&mut self, track: Track) -> u64 {
        if !self.queue.is_empty() {
            self.queue_selected += 1;
        }
        let id = self.next_queue_id;
        self.next_queue_id = self.next_queue_id.wrapping_add(1);
        self.queue.insert(0, QueueEntry { id, track });
        id
    }

    pub fn shuffle_queue(&mut self) {
        let len = self.queue.len();
        if len < 2 {
            return;
        }
        for i in (1..len).rev() {
            let j = self.next_random(i + 1);
            self.queue.swap(i, j);
        }
        self.queue_selected = 0;
    }

    pub fn enqueue(&mut self, track: Track) -> u64 {
        let id = self.next_queue_id;
        self.next_queue_id = self.next_queue_id.wrapping_add(1);
        self.queue.push(QueueEntry { id, track });
        id
    }

    pub fn remove_queue_item(&mut self, index: usize) -> Option<QueueEntry> {
        if index >= self.queue.len() {
            return None;
        }

        let entry = self.queue.remove(index);

        if self.queue.is_empty() {
            self.queue_selected = 0;
        } else if index < self.queue_selected {
            self.queue_selected -= 1;
        } else if self.queue_selected >= self.queue.len() {
            self.queue_selected = self.queue.len() - 1;
        }

        if self.is_resolving && self.pending_queue_entry_id == Some(entry.id) {
            self.cancel_pending_resolution();
            self.set_status("RESOLVE CANCELLED");
        }

        Some(entry)
    }

    pub fn move_queue_item(&mut self, from: usize, to: usize) -> bool {
        if from >= self.queue.len() || to >= self.queue.len() {
            return false;
        }
        if from == to {
            return true;
        }

        let item = self.queue.remove(from);
        self.queue.insert(to, item);

        if self.queue_selected == from {
            self.queue_selected = to;
        } else if from < self.queue_selected && to >= self.queue_selected {
            self.queue_selected = self.queue_selected.saturating_sub(1);
        } else if from > self.queue_selected && to <= self.queue_selected {
            self.queue_selected = self.queue_selected.saturating_add(1);
        }

        true
    }

    pub fn pop_next_queue_entry(&mut self) -> Option<QueueEntry> {
        self.remove_queue_item(0)
    }

    pub fn pop_next_queued(&mut self) -> Option<Track> {
        self.pop_next_queue_entry().map(|e| e.track)
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.queue_selected = 0;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancel_pending_resolution_invalidates_generation() {
        let mut app = App::new();
        assert_eq!(app.resolve_generation, 0);
        assert!(!app.is_resolving);
        assert!(app.pending_track.is_none());

        app.is_resolving = true;
        app.pending_track = Some(Track {
            title: "Test Track".into(),
            artist: "Test Artist".into(),
            stream_url: String::new(),
            art_url: String::new(),
            spotify_id: Some("spotify123".into()),
            artists: vec!["Test Artist".into()],
            album: Some("Test Album".into()),
            duration_ms: Some(180_000),
        });

        app.cancel_pending_resolution();
        assert_eq!(app.resolve_generation, 1);
        assert!(!app.is_resolving);
        assert!(app.pending_track.is_none());
    }

    fn sample_track(title: &str) -> Track {
        Track {
            title: title.into(),
            artist: "Sample Artist".into(),
            stream_url: format!("https://example.com/{title}.mp3"),
            art_url: String::new(),
            spotify_id: None,
            artists: vec!["Sample Artist".into()],
            album: Some("Sample Album".into()),
            duration_ms: Some(210_000),
        }
    }

    #[test]
    fn test_queue_enqueue_unique_ids_and_duplicates() {
        let mut app = App::new();
        assert!(app.queue.is_empty());
        assert_eq!(app.next_queue_id, 1);

        let id1 = app.enqueue(sample_track("Song A"));
        let id2 = app.enqueue(sample_track("Song A")); // Duplicate song
        let id3 = app.enqueue(sample_track("Song B"));

        assert_eq!(app.queue.len(), 3);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
        assert_ne!(id1, id2);
        assert_eq!(app.queue[0].track.title, "Song A");
        assert_eq!(app.queue[1].track.title, "Song A");
        assert_eq!(app.queue[2].track.title, "Song B");
    }

    #[test]
    fn test_queue_remove_item() {
        let mut app = App::new();
        app.enqueue(sample_track("Track 0"));
        app.enqueue(sample_track("Track 1"));
        app.enqueue(sample_track("Track 2"));

        // Out of bounds remove
        assert!(app.remove_queue_item(10).is_none());
        assert_eq!(app.queue.len(), 3);

        // Select index 2, remove index 0 (before selection)
        app.queue_selected = 2;
        let removed = app.remove_queue_item(0).unwrap();
        assert_eq!(removed.track.title, "Track 0");
        assert_eq!(app.queue.len(), 2);
        assert_eq!(app.queue[0].track.title, "Track 1");
        assert_eq!(app.queue[1].track.title, "Track 2");
        assert_eq!(
            app.queue_selected, 1,
            "Cursor should adjust down when earlier item removed"
        );

        // Remove last item while selected
        let removed = app.remove_queue_item(1).unwrap();
        assert_eq!(removed.track.title, "Track 2");
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue_selected, 0, "Cursor should clamp to last element");

        // Remove final remaining item
        let removed = app.remove_queue_item(0).unwrap();
        assert_eq!(removed.track.title, "Track 1");
        assert!(app.queue.is_empty());
        assert_eq!(app.queue_selected, 0);
    }

    #[test]
    fn test_queue_move_item() {
        let mut app = App::new();
        app.enqueue(sample_track("T0"));
        app.enqueue(sample_track("T1"));
        app.enqueue(sample_track("T2"));
        app.enqueue(sample_track("T3"));

        // Invalid bounds
        assert!(!app.move_queue_item(0, 10));
        assert!(!app.move_queue_item(10, 0));

        // Same index
        assert!(app.move_queue_item(1, 1));
        assert_eq!(app.queue[1].track.title, "T1");

        // Move T0 from 0 to 2: [T1, T2, T0, T3]
        app.queue_selected = 0;
        assert!(app.move_queue_item(0, 2));
        assert_eq!(app.queue[0].track.title, "T1");
        assert_eq!(app.queue[1].track.title, "T2");
        assert_eq!(app.queue[2].track.title, "T0");
        assert_eq!(app.queue[3].track.title, "T3");
        assert_eq!(
            app.queue_selected, 2,
            "Selection must follow the moved item"
        );

        // Move T3 from 3 to 0 while cursor is at 2 (T0): [T3, T1, T2, T0]
        assert!(app.move_queue_item(3, 0));
        assert_eq!(app.queue[0].track.title, "T3");
        assert_eq!(app.queue[1].track.title, "T1");
        assert_eq!(app.queue[2].track.title, "T2");
        assert_eq!(app.queue[3].track.title, "T0");
        assert_eq!(app.queue_selected, 3, "Selection on T0 should shift right");
    }

    #[test]
    fn test_queue_pop_next_queued() {
        let mut app = App::new();
        assert!(app.pop_next_queued().is_none());

        app.enqueue(sample_track("First"));
        app.enqueue(sample_track("Second"));
        app.queue_selected = 1;

        let first = app.pop_next_queued().unwrap();
        assert_eq!(first.title, "First");
        assert_eq!(app.queue.len(), 1);
        assert_eq!(app.queue_selected, 0);

        let second = app.pop_next_queued().unwrap();
        assert_eq!(second.title, "Second");
        assert!(app.queue.is_empty());
        assert_eq!(app.queue_selected, 0);

        assert!(app.pop_next_queued().is_none());
    }

    #[test]
    fn test_queue_clear() {
        let mut app = App::new();
        app.enqueue(sample_track("Song 1"));
        app.enqueue(sample_track("Song 2"));
        app.queue_selected = 1;

        app.clear_queue();
        assert!(app.queue.is_empty());
        assert_eq!(app.queue_selected, 0);
    }

    #[test]
    fn test_push_front_queue_adjusts_queue_selected() {
        let mut app = App::new();
        app.enqueue(sample_track("Track A"));
        let id_b = app.enqueue(sample_track("Track B"));
        app.queue_selected = 1;

        assert_eq!(app.queue[app.queue_selected].id, id_b);

        app.push_front_queue(sample_track("Track C"));
        assert_eq!(app.queue.len(), 3);
        assert_eq!(app.queue[0].track.title, "Track C");
        assert_eq!(app.queue[1].track.title, "Track A");
        assert_eq!(app.queue[2].track.title, "Track B");
        assert_eq!(app.queue_selected, 2);
        assert_eq!(app.queue[app.queue_selected].id, id_b);
    }
}
