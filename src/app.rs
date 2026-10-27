use std::{
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::art;
use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

fn logo_bitmap() -> Vec<Vec<bool>> {
    let lines: Vec<&str> = include_str!("../ascii.art")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    let mut bitmap = Vec::new();
    for line in &lines {
        let mut top = vec![false; width];
        let mut bottom = vec![false; width];
        for (x, ch) in line.chars().enumerate() {
            match ch {
                '▀' => top[x] = true,
                '▄' => bottom[x] = true,
                '█' | '▌' | '▐' => {
                    top[x] = true;
                    bottom[x] = true;
                }
                _ => {}
            }
        }
        bitmap.push(top);
        bitmap.push(bottom);
    }

    let right = (0..width)
        .rev()
        .find(|&x| bitmap.iter().any(|row| row.get(x) == Some(&true)))
        .map_or(0, |x| x + 1);
    for row in &mut bitmap {
        row.truncate(right);
    }

    bitmap
}

fn scale_bitmap(bitmap: &[Vec<bool>], width: usize, height: usize) -> Vec<Vec<bool>> {
    let src_height = bitmap.len();
    let src_width = bitmap.first().map_or(1, |row| row.len());

    (0..height)
        .map(|y| {
            let sy = y * src_height / height;
            (0..width)
                .map(|x| bitmap[sy][x * src_width / width])
                .collect()
        })
        .collect()
}

fn bitmap_lines(bitmap: &[Vec<bool>]) -> Vec<Line<'static>> {
    let width = bitmap.first().map_or(0, |row| row.len());

    bitmap
        .chunks(2)
        .map(|pair| {
            let line: String = (0..width)
                .map(|x| match (pair[0][x], pair.get(1).is_some_and(|row| row[x])) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                })
                .collect();
            Line::from(line)
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub stream_url: String,
    pub art_url: String,
}

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
        }
    }

    pub fn apply_setting(&mut self) {
        match self.settings_cursor {
            0 => self.settings.audio_cache = !self.settings.audio_cache,
            1 => self.settings.art_hq = !self.settings.art_hq,
            2 => self.settings.art_palette = !self.settings.art_palette,
            _ => {
                crate::settings::clear_caches();
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

    /// random track strictly after the current one; wraps when at the end
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

    pub fn render(&self, frame: &mut Frame, playback: Option<(f64, Option<f64>)>) {
        let area = frame.area();

        if area.width < 40 || area.height < 24 {
            frame.render_widget(
                Paragraph::new("TERMINAL TOO SMALL").alignment(Alignment::Center),
                area,
            );
            return;
        }

        if self.settings_open {
            self.render_settings(frame, area);
            return;
        }

        let bitmap = logo_bitmap();
        let art_width = bitmap.first().map_or(0, |row| row.len());
        let art_height = bitmap.len();
        let max_rows = (area.height as usize)
            .saturating_sub(24)
            .min(area.height as usize / 3);

        let mut target_width = ((area.width as usize).saturating_sub(4) * 3 / 5).min(art_width);
        let mut target_px = ((art_height * target_width / art_width.max(1)) + 1) / 2 * 2;
        let max_px = max_rows.saturating_sub(1) * 2;

        if target_px > max_px {
            target_px = max_px;
            target_width = art_width * target_px / art_height.max(1);
        }

        let show_logo = target_width >= 40 && target_px >= 6;
        let logo_height = if show_logo { target_px as u16 / 2 + 1 } else { 0 };

        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(if show_logo { logo_height } else { 0 }),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(8),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

        let green = Style::default().fg(Color::Green);
        let bold_green = green.add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(Color::DarkGray);

        let current = self
            .now_playing
            .as_ref()
            .or_else(|| self.tracks.get(self.selected));

        let selected = match self
            .settings
            .art_palette
            .then(|| current.and_then(|track| art::dominant_color(&track.art_url)))
            .flatten()
        {
            Some((r, g, b)) => Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(r, g, b))
                .add_modifier(Modifier::BOLD),
            None => Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        };

        let filled = (self.volume as usize).min(100) / 10;
        let bars = format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled));
        let clock = playback
            .map(|(elapsed, _)| format_time(elapsed))
            .unwrap_or_else(|| "--:--".into());

        frame.render_widget(
            Paragraph::new(format!("VOL {} {}", bars, clock))
                .style(green)
                .alignment(Alignment::Right),
            rows[0],
        );

        if show_logo {
            let scaled = scale_bitmap(&bitmap, target_width, target_px);
            frame.render_widget(
                Paragraph::new(bitmap_lines(&scaled))
                    .style(bold_green)
                    .alignment(Alignment::Center),
                rows[1],
            );
        }

        self.render_search_box(frame, rows[2], green, bold_green, dim);

        frame.render_widget(
            Paragraph::new(self.visible_status())
                .style(dim)
                .alignment(Alignment::Center),
            rows[3],
        );

        let items: Vec<ListItem> = if self.tracks.is_empty() {
            let message = if !self.loading && !self.last_query.is_empty() {
                Some("NO RESULTS")
            } else {
                None
            };

            message
                .map(|m| ListItem::new(Line::from(m).centered().style(dim)))
                .into_iter()
                .collect()
        } else {
            self.tracks
                .iter()
                .enumerate()
                .map(|(i, track)| {
                    let title = truncate(&track.title, 26).to_uppercase();
                    let artist = truncate(&track.artist, 14).to_uppercase();
                    let label = format!(" {:>2}. {} | {}", i + 1, title, artist);

                    ListItem::new(label).style(if i == self.selected { selected } else { green })
                })
                .collect()
        };

        let mut list_state = ListState::default();
        list_state.select(Some(self.selected));

        frame.render_stateful_widget(List::new(items), rows[5], &mut list_state);

        art::render_art(
            frame,
            rows[7],
            current.map(|track| track.art_url.as_str()).unwrap_or(""),
            current.map(|track| track.title.as_str()).unwrap_or(""),
            current.map(|track| track.artist.as_str()).unwrap_or(""),
            self.settings.art_hq,
        );

        let bar_width = (rows[9].width * 3 / 5).max(20) as usize;
        let bar_area = Rect {
            x: rows[9].x + (rows[9].width - bar_width as u16) / 2,
            y: rows[9].y,
            width: bar_width as u16,
            height: 1,
        };

        let width = bar_width;
        let song_loading = self.playing.is_some() && playback.is_none();

        if song_loading {
            frame.render_widget(
                Paragraph::new(format!(" {} ", crate::art::loading_moons()))
                    .style(bold_green)
                    .alignment(Alignment::Center),
                bar_area,
            );
        } else {
            let bar: Line = playback
                .and_then(|(elapsed, duration)| {
                    duration.filter(|d| *d > 0.0).map(|d| (elapsed, d))
                })
                .map(|(elapsed, duration)| {
                    let ratio = (elapsed / duration).clamp(0.0, 1.0);
                    let marker = ((width as f64) * ratio).round() as usize;

                    Line::from(
                        (0..width)
                            .map(|i| {
                                if i == marker {
                                    Span::styled("█", bold_green)
                                } else if i < marker {
                                    Span::styled("▄", bold_green)
                                } else {
                                    Span::styled("▄", dim)
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .unwrap_or_else(|| Line::from(Span::styled("▄".repeat(width), dim)));

            frame.render_widget(Paragraph::new(bar), bar_area);
        }

        let cluster = Rect {
            x: rows[10].x + rows[10].width.saturating_sub(45) / 2,
            y: rows[10].y,
            width: 45.min(rows[10].width),
            height: rows[10].height,
        };

        let slots = Layout::horizontal([
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(9),
        ])
        .split(cluster);

        let shuffle_icon = if self.shuffle {
            ("><", bold_green)
        } else {
            ("><", dim)
        };
        frame.render_widget(
            Paragraph::new(shuffle_icon.0)
                .style(shuffle_icon.1)
                .alignment(Alignment::Center),
            slots[0],
        );

        frame.render_widget(
            Paragraph::new("|◀").style(bold_green).alignment(Alignment::Center),
            slots[1],
        );

        let play_icon = if self.paused { "▶" } else { "▮▮" };
        frame.render_widget(
            Paragraph::new(play_icon)
                .style(bold_green)
                .alignment(Alignment::Center),
            slots[2],
        );

        frame.render_widget(
            Paragraph::new("▶|").style(bold_green).alignment(Alignment::Center),
            slots[3],
        );

        let repeat_icon = match self.repeat {
            Repeat::One => ("R1", bold_green),
            Repeat::All => ("RA", bold_green),
            Repeat::Off => ("R", dim),
        };
        frame.render_widget(
            Paragraph::new(repeat_icon.0)
                .style(repeat_icon.1)
                .alignment(Alignment::Center),
            slots[4],
        );

        frame.render_widget(
            Paragraph::new("↑↓ SELECT  ENTER PLAY  SPACE PAUSE  ←→ TRACK  X SHUFFLE  R REPEAT  S SETTINGS  Q QUIT")
                .style(dim)
                .alignment(Alignment::Center),
            rows[12],
        );
    }

    fn visible_status(&self) -> String {
        if self.loading {
            return format!(" {} ", crate::art::loading_moons());
        }

        match self.status_at {
            Some(at) if at.elapsed() < Duration::from_secs(3) => self.status.clone(),
            _ => String::new(),
        }
    }

    fn render_settings(&self, frame: &mut Frame, area: Rect) {
        let green = Style::default().fg(Color::Green);
        let bold_green = green.add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(Color::DarkGray);

        let cache_state = if self.settings.audio_cache { "ON" } else { "OFF" };
        let cache_style = if self.settings.audio_cache { bold_green } else { dim };

        let palette_state = if self.settings.art_palette { "ON" } else { "OFF" };
        let palette_style = if self.settings.art_palette { bold_green } else { dim };

        let mode = if self.settings.art_hq { "HQ" } else { "PIXEL" };
        let mode_style = if self.settings.art_hq { bold_green } else { dim };

        let cache_size = crate::settings::cache_bytes();
        let size_text = if cache_size > 0 {
            crate::settings::human_size(cache_size)
        } else {
            "EMPTY".to_string()
        };
        let size_style = if cache_size > 0 { bold_green } else { dim };

        let setting_rows = vec![
            (
                "Audio Cache:".to_string(),
                cache_state.to_string(),
                cache_style,
                "Save played songs to /tmp for instant replays".to_string(),
            ),
            (
                "Art Mode:".to_string(),
                mode.to_string(),
                mode_style,
                "HQ uses true graphics, pixel uses unicode blocks".to_string(),
            ),
            (
                "Art Palette:".to_string(),
                palette_state.to_string(),
                palette_style,
                "Tint the selected track with the artwork colors".to_string(),
            ),
            (
                "Clear Caches:".to_string(),
                size_text,
                size_style,
                "Removes cached artwork and audio".to_string(),
            ),
        ];

        let cursor = self.settings_cursor.min(setting_rows.len() - 1);
        let block_height = setting_rows.len() as u16 + 4;
        let top = area.y + (area.height - block_height) / 2;

        let name_width = setting_rows
            .iter()
            .map(|(name, value, ..)| name.chars().count() + value.chars().count() + 1)
            .max()
            .unwrap_or(0) as u16;
        let description_width = setting_rows
            .iter()
            .map(|(.., description)| description.chars().count())
            .max()
            .unwrap_or(0) as u16;

        let table_width = (name_width + 2 + description_width).min(area.width);
        let table_x = area.x + (area.width - table_width) / 2;

        frame.render_widget(
            Paragraph::new("SETTINGS")
                .style(bold_green)
                .alignment(Alignment::Center),
            Rect {
                x: area.x,
                y: top,
                width: area.width,
                height: 1,
            },
        );

        for (offset, (name, value, value_style, description)) in
            setting_rows.iter().enumerate()
        {
            let active = offset == cursor;
            let row = Rect {
                x: table_x,
                y: top + 2 + offset as u16,
                width: table_width,
                height: 1,
            };

            let columns = Layout::horizontal([
                Constraint::Length(name_width),
                Constraint::Length(2),
                Constraint::Min(description_width),
            ])
            .split(row);

            let cell_style = if active {
                Style::default().bg(Color::Green)
            } else {
                Style::default()
            };

            let name_style = if active {
                Style::default().fg(Color::Black)
            } else {
                green
            };

            let value_style = if active {
                Style::default()
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                *value_style
            };

            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(name.as_str(), name_style),
                    Span::raw(" "),
                    Span::styled(value.as_str(), value_style),
                ]))
                .style(cell_style)
                .alignment(Alignment::Right),
                columns[0],
            );

            let description_style = if active {
                Style::default().fg(Color::Black)
            } else {
                dim
            };

            frame.render_widget(
                Paragraph::new(description.as_str()).style(description_style),
                columns[2],
            );
        }

        frame.render_widget(
            Paragraph::new("↑↓ SELECT   SPACE TOGGLE   ESC BACK")
                .style(dim)
                .alignment(Alignment::Center),
            Rect {
                x: area.x,
                y: top + block_height - 1,
                width: area.width,
                height: 1,
            },
        );
    }

    fn render_search_box(
        &self,
        frame: &mut Frame,
        area: Rect,
        green: Style,
        bold_green: Style,
        dim: Style,
    ) {
        let focused = self.mode == Mode::Type;
        let cursor = if focused { "▌" } else { " " };
        let placeholder = "Search songs...";

        let mut text = if self.query.is_empty() {
            placeholder.to_string()
        } else {
            self.query.clone()
        };

        let min_inner = placeholder.chars().count() + 3;
        let max_inner = area.width.saturating_sub(6) as usize;
        let inner_w = (text.chars().count() + 3).max(min_inner).min(max_inner);

        let text_w = inner_w - 3;
        let count = text.chars().count();
        if count > text_w {
            text = format!("…{}", text.chars().skip(count - text_w).collect::<String>());
        }

        let box_w = inner_w as u16 + 4;
        let rect = Rect {
            x: area.x + (area.width - box_w) / 2,
            y: area.y,
            width: box_w,
            height: 3,
        };

        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if focused { green } else { dim }),
            rect,
        );

        let text_area = Rect {
            x: rect.x + 2,
            y: rect.y + 1,
            width: box_w - 4,
            height: 1,
        };

        let spans = if self.query.is_empty() {
            vec![
                Span::styled("> ", dim),
                Span::styled(text, dim),
                Span::styled(cursor, if focused { green } else { dim }),
            ]
        } else {
            vec![
                Span::raw("> "),
                Span::styled(text, bold_green),
                Span::styled(cursor, if focused { green } else { dim }),
            ]
        };

        frame.render_widget(Paragraph::new(Line::from(spans)), text_area);
    }
}

fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }

    format!(
        "{}…",
        text.chars()
            .take(max.saturating_sub(1))
            .collect::<String>()
    )
}
