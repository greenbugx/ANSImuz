use std::{
    collections::VecDeque,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const MAX_HISTORY_SIZE: usize = 100;

use crate::config::Settings;
use crate::music::{PlaybackSource, Track};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitChoice {
    Yes,
    No,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Resolving {
        track: Track,
        source: PlaybackSource,
        active: Option<Box<PlaybackState>>,
    },
    Playing {
        track: Track,
    },
    Paused {
        track: Track,
    },
}

impl PlaybackState {
    pub fn current_track(&self) -> Option<&Track> {
        match self {
            PlaybackState::Playing { track } | PlaybackState::Paused { track } => Some(track),
            PlaybackState::Resolving { active, .. } => {
                active.as_ref().and_then(|a| a.current_track())
            }
            PlaybackState::Stopped => None,
        }
    }

    pub fn current_track_mut(&mut self) -> Option<&mut Track> {
        match self {
            PlaybackState::Playing { track } | PlaybackState::Paused { track } => Some(track),
            PlaybackState::Resolving { active, .. } => {
                active.as_mut().and_then(|a| a.current_track_mut())
            }
            PlaybackState::Stopped => None,
        }
    }

    pub fn pending_track(&self) -> Option<&Track> {
        match self {
            PlaybackState::Resolving { track, .. } => Some(track),
            _ => None,
        }
    }

    pub fn is_playing(&self) -> bool {
        match self {
            PlaybackState::Playing { .. } => true,
            PlaybackState::Resolving { active, .. } => {
                active.as_ref().is_some_and(|a| a.is_playing())
            }
            _ => false,
        }
    }

    pub fn is_paused(&self) -> bool {
        match self {
            PlaybackState::Paused { .. } => true,
            PlaybackState::Resolving { active, .. } => {
                active.as_ref().is_some_and(|a| a.is_paused())
            }
            _ => false,
        }
    }

    pub fn is_resolving(&self) -> bool {
        matches!(self, PlaybackState::Resolving { .. })
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, PlaybackState::Stopped)
    }
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
    pub shuffle: bool,
    pub repeat: Repeat,
    rng_state: u64,
    pub loading: bool,
    pub volume: u8,
    pub status: String,
    status_at: Option<Instant>,
    pub settings: Settings,
    pub settings_open: bool,
    pub settings_cursor: usize,
    pub should_quit: bool,
    pub quit_dialog_open: bool,
    pub quit_dialog_choice: QuitChoice,
    pub about_open: bool,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_cursor: usize,
    pub resolve_generation: u64,
    pub search_generation: u64,
    pub queue: Vec<QueueEntry>,
    pub queue_selected: usize,
    pub next_queue_id: u64,
    pub pending_queue_entry_id: Option<u64>,
    pub history: VecDeque<Track>,
    pub playback_state: PlaybackState,
    pub current_position: Option<Duration>,
    pub discord_worker: Option<crate::discord::DiscordWorker>,
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
            shuffle: false,
            repeat: Repeat::Off,
            rng_state: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15)
                | 1,
            loading: false,
            volume: 70,
            status: String::new(),
            status_at: None,
            settings: Settings::load(),
            settings_open: false,
            settings_cursor: 0,
            quit_dialog_open: false,
            quit_dialog_choice: QuitChoice::Yes,
            should_quit: false,
            about_open: false,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_cursor: 0,
            resolve_generation: 0,
            search_generation: 0,
            queue: Vec::new(),
            queue_selected: 0,
            next_queue_id: 1,
            history: VecDeque::new(),
            pending_queue_entry_id: None,
            playback_state: PlaybackState::Stopped,
            current_position: None,
            discord_worker: None,
        }
    }

    pub fn init_discord(&mut self) {
        if self.settings.discord.enabled && self.discord_worker.is_none() {
            match crate::discord::DiscordWorker::spawn_with_config(
                crate::discord::DEFAULT_CLIENT_ID,
                self.settings.discord.clone(),
            ) {
                Ok(worker) => self.discord_worker = Some(worker),
                Err(err) => self.set_status(format!("DISCORD RPC ERROR: {err}")),
            }
        }
    }

    pub fn is_discord_connected(&self) -> bool {
        self.settings.discord.enabled
            && self
                .discord_worker
                .as_ref()
                .is_some_and(|w| w.is_connected())
    }

    pub fn toggle_discord_rpc(&mut self) {
        self.settings.discord.enabled = !self.settings.discord.enabled;
        if let Err(error) = self.settings.save() {
            self.set_status(format!("SETTINGS ERROR: {error}"));
        }

        if self.settings.discord.enabled {
            if self.discord_worker.is_none() {
                match crate::discord::DiscordWorker::spawn_with_config(
                    crate::discord::DEFAULT_CLIENT_ID,
                    self.settings.discord.clone(),
                ) {
                    Ok(worker) => {
                        if let Some(track) = self.current_track() {
                            let mut info = crate::discord::PlaybackInfo::from(track)
                                .with_paused(self.is_paused());
                            if let Some(pos) = self.current_position {
                                info = info.with_position(pos);
                            }
                            let artwork =
                                crate::discord::artwork::ArtworkRequest::from_track(track);
                            worker.track_changed(info, artwork);
                        }
                        self.discord_worker = Some(worker);
                    }
                    Err(err) => {
                        self.set_status(format!("DISCORD RPC ERROR: {err}"));
                    }
                }
            }
        } else if let Some(mut worker) = self.discord_worker.take() {
            worker.clear();
            worker.shutdown();
        }
    }

    pub fn open_command_palette(&mut self) {
        self.command_palette_open = true;
        self.command_palette_query.clear();
        self.command_palette_cursor = 0;
        self.about_open = false;
    }

    pub fn close_command_palette(&mut self) {
        self.command_palette_open = false;
        self.command_palette_query.clear();
        self.command_palette_cursor = 0;
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.playback_state.current_track()
    }

    pub fn current_track_mut(&mut self) -> Option<&mut Track> {
        self.playback_state.current_track_mut()
    }

    pub fn now_playing(&self) -> Option<&Track> {
        self.playback_state.current_track()
    }

    pub fn pending_track(&self) -> Option<&Track> {
        self.playback_state.pending_track()
    }

    pub fn is_playing(&self) -> bool {
        self.playback_state.is_playing()
    }

    pub fn is_paused(&self) -> bool {
        self.playback_state.is_paused()
    }

    pub fn is_resolving(&self) -> bool {
        self.playback_state.is_resolving()
    }

    pub fn playback_state(&self) -> &PlaybackState {
        &self.playback_state
    }

    pub fn set_stopped(&mut self) {
        self.playback_state = PlaybackState::Stopped;
        self.current_position = None;
        self.pending_queue_entry_id = None;
        self.playing = None;
    }

    pub fn set_resolving(&mut self, track: Track, source: PlaybackSource) {
        let active = match &self.playback_state {
            PlaybackState::Playing { track: t } => {
                Some(Box::new(PlaybackState::Playing { track: t.clone() }))
            }
            PlaybackState::Paused { track: t } => {
                Some(Box::new(PlaybackState::Paused { track: t.clone() }))
            }
            PlaybackState::Resolving { active: prev, .. } => prev.clone(),
            PlaybackState::Stopped => None,
        };

        self.playback_state = PlaybackState::Resolving {
            track,
            source,
            active,
        };
    }

    pub fn cancel_pending_resolution(&mut self) {
        self.resolve_generation = self.resolve_generation.wrapping_add(1);
        self.pending_queue_entry_id = None;

        if let PlaybackState::Resolving { active, .. } = &self.playback_state {
            self.playback_state = active
                .as_ref()
                .map(|b| *b.clone())
                .unwrap_or(PlaybackState::Stopped);
        }
    }

    pub fn finish_active_track_while_resolving(&mut self) {
        let prev_active_track = match &self.playback_state {
            PlaybackState::Resolving { active, .. } => {
                active.as_ref().and_then(|a| a.current_track()).cloned()
            }
            _ => None,
        };

        if let Some(track) = prev_active_track {
            self.push_history(track);
        }

        if let PlaybackState::Resolving { active, .. } = &mut self.playback_state {
            *active = None;
            self.playing = None;
        }
    }

    pub fn set_playing(&mut self, track: Track) {
        self.playback_state = PlaybackState::Playing { track };
        self.current_position = Some(Duration::ZERO);
        self.pending_queue_entry_id = None;
    }

    pub fn set_paused(&mut self, paused: bool) {
        if let Some(track) = self.current_track().cloned() {
            if paused {
                self.playback_state = PlaybackState::Paused { track };
            } else {
                self.playback_state = PlaybackState::Playing { track };
            }
        }
    }
    pub fn toggle_about(&mut self) {
        self.about_open = !self.about_open;
        if self.about_open {
            self.command_palette_open = false;
        }
    }

    pub fn open_quit_dialog(&mut self) {
        self.quit_dialog_open = true;
        self.quit_dialog_choice = QuitChoice::Yes;
        self.command_palette_open = false;
        self.about_open = false;
        self.settings_open = false;
    }

    pub fn close_quit_dialog(&mut self) {
        self.quit_dialog_open = false;
    }

    pub fn apply_setting(&mut self) {
        match self.settings_cursor {
            0 => self.settings.provider = self.settings.provider.toggle(),
            1 => self.settings.audio_cache = !self.settings.audio_cache,
            2 => self.settings.art_hq = !self.settings.art_hq,
            3 => self.settings.art_palette = !self.settings.art_palette,
            4 => self.toggle_discord_rpc(),
            _ => {
                match crate::config::clear_caches() {
                    Ok(()) => self.set_status("CACHES CLEARED"),
                    Err(error) => self.set_status(format!("CACHE ERROR: {error}")),
                }
                return;
            }
        }

        if let Err(error) = self.settings.save() {
            self.set_status(format!("SETTINGS ERROR: {error}"));
        }
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

    pub fn queue_contains(&self, track: &Track) -> bool {
        self.queue.iter().any(|entry| entry.track.is_same(track))
    }

    pub fn enqueue_selected(&mut self) -> Option<u64> {
        let track = self.tracks.get(self.selected)?.clone();
        let title = track.title.clone();
        if let Some(id) = self.enqueue(track) {
            self.set_status(format!("QUEUED: {}", title));
            Some(id)
        } else {
            self.set_status("ALREADY IN QUEUE");
            None
        }
    }

    pub fn push_history(&mut self, track: Track) {
        if self.history.len() >= MAX_HISTORY_SIZE {
            self.history.pop_front();
        }
        self.history.push_back(track);
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
        let selected_id = self.queue.get(self.queue_selected).map(|e| e.id);
        self.queue.retain(|entry| !entry.track.is_same(&track));

        let id = self.next_queue_id;
        self.next_queue_id = self.next_queue_id.wrapping_add(1);
        self.queue.insert(0, QueueEntry { id, track });

        if let Some(sel_id) = selected_id {
            if let Some(new_idx) = self.queue.iter().position(|e| e.id == sel_id) {
                self.queue_selected = new_idx;
            } else {
                self.queue_selected = (self.queue_selected + 1).min(self.queue.len() - 1);
            }
        } else {
            self.queue_selected = 0;
        }

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

    pub fn enqueue(&mut self, track: Track) -> Option<u64> {
        if self.queue_contains(&track) {
            return None;
        }
        let id = self.next_queue_id;
        self.next_queue_id = self.next_queue_id.wrapping_add(1);
        self.queue.push(QueueEntry { id, track });
        Some(id)
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

        if self.is_resolving() && self.pending_queue_entry_id == Some(entry.id) {
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
    fn test_playback_state_transitions() {
        let mut app = App::new();
        assert_eq!(*app.playback_state(), PlaybackState::Stopped);

        let track = Track {
            provider: "ytmusic".into(),
            track_id: None,
            title: "Test Track".into(),
            artist: "Test Artist".into(),
            album: None,
            stream_url: None,
            art_url: None,
            spotify_id: None,
            artists: vec![],
            duration_ms: None,
        };

        // Resolving from stopped
        app.set_resolving(track.clone(), PlaybackSource::SearchResult { index: 0 });
        assert_eq!(
            *app.playback_state(),
            PlaybackState::Resolving {
                track: track.clone(),
                source: PlaybackSource::SearchResult { index: 0 },
                active: None,
            }
        );
        assert!(app.is_resolving());
        assert!(app.current_track().is_none());
        assert_eq!(app.pending_track().unwrap().title, "Test Track");

        // Cancel resolution when nothing was playing -> Stopped
        app.cancel_pending_resolution();
        assert_eq!(*app.playback_state(), PlaybackState::Stopped);
        assert!(!app.is_resolving());
        assert!(app.pending_track().is_none());

        // Playing
        let mut resolved = track.clone();
        resolved.stream_url = Some("http://stream/test".into());
        app.set_playing(resolved.clone());
        assert_eq!(
            *app.playback_state(),
            PlaybackState::Playing {
                track: resolved.clone()
            }
        );
        assert_eq!(
            app.now_playing().map(|t| t.title.as_str()),
            Some("Test Track")
        );
        assert!(!app.is_paused());
        assert!(app.is_playing());

        // Resolving a second track while first is playing preserves active
        let track2 = Track {
            provider: "ytmusic".into(),
            track_id: None,
            title: "Second Track".into(),
            artist: "Second Artist".into(),
            album: None,
            stream_url: None,
            art_url: None,
            spotify_id: None,
            artists: vec![],
            duration_ms: None,
        };
        app.set_resolving(track2.clone(), PlaybackSource::SearchResult { index: 1 });
        assert!(app.is_resolving());
        // Active track is still Test Track!
        assert_eq!(
            app.now_playing().map(|t| t.title.as_str()),
            Some("Test Track")
        );
        assert_eq!(
            app.pending_track().map(|t| t.title.as_str()),
            Some("Second Track")
        );

        // Cancel restores active playing track
        app.cancel_pending_resolution();
        assert_eq!(
            *app.playback_state(),
            PlaybackState::Playing {
                track: resolved.clone()
            }
        );
        assert_eq!(
            app.now_playing().map(|t| t.title.as_str()),
            Some("Test Track")
        );

        // Paused
        app.set_paused(true);
        assert_eq!(
            *app.playback_state(),
            PlaybackState::Paused {
                track: resolved.clone()
            }
        );
        assert!(app.is_paused());
        assert!(!app.is_playing());

        // Unpaused
        app.set_paused(false);
        assert_eq!(
            *app.playback_state(),
            PlaybackState::Playing {
                track: resolved.clone()
            }
        );
        assert!(!app.is_paused());
        assert!(app.is_playing());

        // Stopped
        app.set_stopped();
        assert_eq!(*app.playback_state(), PlaybackState::Stopped);
        assert!(app.now_playing().is_none());
        assert!(!app.is_paused());
        assert!(!app.is_playing());
    }

    #[test]
    fn test_cancel_pending_resolution_invalidates_generation() {
        let mut app = App::new();
        assert_eq!(app.resolve_generation, 0);
        assert!(!app.is_resolving());
        assert!(app.pending_track().is_none());

        app.set_resolving(
            Track {
                provider: "spotify".into(),
                track_id: Some("spotify123".into()),
                title: "Test Track".into(),
                artist: "Test Artist".into(),
                album: Some("Test Album".into()),
                stream_url: None,
                art_url: None,
                spotify_id: Some("spotify123".into()),
                artists: vec!["Test Artist".into()],
                duration_ms: Some(180_000),
            },
            PlaybackSource::SearchResult { index: 0 },
        );

        assert_eq!(app.resolve_generation, 0);
        assert!(app.is_resolving());
        assert!(app.pending_track().is_some());

        app.cancel_pending_resolution();
        assert_eq!(app.resolve_generation, 1);
        assert!(!app.is_resolving());
        assert!(app.pending_track().is_none());
    }

    fn sample_track(title: &str) -> Track {
        Track {
            provider: "ytmusic".into(),
            track_id: None,
            title: title.into(),
            artist: "Sample Artist".into(),
            album: Some("Sample Album".into()),
            stream_url: Some(format!("https://example.com/{title}.mp3")),
            art_url: None,
            spotify_id: None,
            artists: vec!["Sample Artist".into()],
            duration_ms: Some(210_000),
        }
    }

    #[test]
    fn test_queue_enqueue_rejects_duplicates() {
        let mut app = App::new();
        assert!(app.queue.is_empty());
        assert_eq!(app.next_queue_id, 1);

        let id1 = app.enqueue(sample_track("Song A"));
        let id2 = app.enqueue(sample_track("Song A")); // Duplicate song must be rejected
        let id3 = app.enqueue(sample_track("Song B"));

        assert_eq!(app.queue.len(), 2);
        assert_eq!(id1, Some(1));
        assert_eq!(
            id2, None,
            "Duplicate song must return None and not be added"
        );
        assert_eq!(id3, Some(2));
        assert_eq!(app.queue[0].track.title, "Song A");
        assert_eq!(app.queue[1].track.title, "Song B");
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
        let id_b = app.enqueue(sample_track("Track B")).unwrap();
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

    #[test]
    fn test_finish_active_track_while_resolving() {
        let mut app = App::new();
        let t0 = sample_track("Track 0");
        let t1 = sample_track("Track 1");
        app.set_playing(t0.clone());
        app.set_resolving(t1.clone(), PlaybackSource::SearchResult { index: 1 });
        assert!(app.is_resolving());
        assert_eq!(app.now_playing().map(|t| t.title.as_str()), Some("Track 0"));

        app.finish_active_track_while_resolving();

        assert!(app.is_resolving());
        assert!(app.current_track().is_none());
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].title, "Track 0");

        // Calling again does not duplicate history entry
        app.finish_active_track_while_resolving();
        assert_eq!(app.history.len(), 1);
    }
}
