use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::Settings;
use crate::music::Track;
use crate::playback::Repeat;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Type,
    Nav,
}

pub struct App {
    pub mode: Mode,
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
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: Mode::Nav,
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

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
        if self.help_open {
            self.command_palette_open = false;
        }
    }

    pub fn apply_setting(&mut self) {
        match self.settings_cursor {
            0 => self.settings.audio_cache = !self.settings.audio_cache,
            1 => self.settings.art_hq = !self.settings.art_hq,
            2 => self.settings.art_palette = !self.settings.art_palette,
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

    pub fn next_index(&self, delta: isize) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }

        let len = self.tracks.len() as isize;
        let base = self.playing.unwrap_or(self.selected) as isize;
        Some((base + delta).rem_euclid(len) as usize)
    }

    pub fn shuffled_next(&mut self) -> Option<usize> {
        let len = self.tracks.len();
        if len < 2 {
            return None;
        }

        let current = self.playing.unwrap_or(self.selected);
        let after = len.saturating_sub(current + 1);

        let index = if after > 0 {
            current + 1 + self.next_random(after)
        } else {
            self.next_random(len)
        };

        Some(index)
    }

    fn next_random(&mut self, bound: usize) -> usize {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;

        (self.rng_state % bound as u64) as usize
    }
}
