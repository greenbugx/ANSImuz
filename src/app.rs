use std::time::{Duration, Instant};

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::art;

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
    pub loading: bool,
    pub paused: bool,
    pub volume: u8,
    pub status: String,
    status_at: Option<Instant>,
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
            loading: false,
            paused: false,
            volume: 70,
            status: String::new(),
            status_at: None,
            should_quit: false,
        }
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

    pub fn render(&self, frame: &mut Frame, playback: Option<(f64, Option<f64>)>) {
        let area = frame.area();

        if area.width < 40 || area.height < 24 {
            frame.render_widget(
                Paragraph::new("TERMINAL TOO SMALL").alignment(Alignment::Center),
                area,
            );
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
        let selected = Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);

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

        let current = self
            .playing
            .and_then(|index| self.tracks.get(index))
            .or_else(|| self.tracks.get(self.selected));

        art::render_art(
            frame,
            rows[7],
            current.map(|track| track.art_url.as_str()).unwrap_or(""),
            current.map(|track| track.title.as_str()).unwrap_or(""),
            current.map(|track| track.artist.as_str()).unwrap_or(""),
        );

        let bar_width = (rows[9].width * 3 / 5).max(20) as usize;
        let bar_area = Rect {
            x: rows[9].x + (rows[9].width - bar_width as u16) / 2,
            y: rows[9].y,
            width: bar_width as u16,
            height: 1,
        };

        let width = bar_width;
        let bar: Line = playback
            .and_then(|(elapsed, duration)| duration.filter(|d| *d > 0.0).map(|d| (elapsed, d)))
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

        let cluster = Rect {
            x: rows[10].x + rows[10].width.saturating_sub(45) / 2,
            y: rows[10].y,
            width: 45.min(rows[10].width),
            height: rows[10].height,
        };

        let thirds = Layout::horizontal([
            Constraint::Length(15),
            Constraint::Length(15),
            Constraint::Length(15),
        ])
        .split(cluster);

        frame.render_widget(
            Paragraph::new("|◀").style(bold_green).alignment(Alignment::Center),
            thirds[0],
        );

        let play_icon = if self.paused { "▶" } else { "▮▮" };
        frame.render_widget(
            Paragraph::new(play_icon)
                .style(bold_green)
                .alignment(Alignment::Center),
            thirds[1],
        );

        frame.render_widget(
            Paragraph::new("▶|").style(bold_green).alignment(Alignment::Center),
            thirds[2],
        );

        frame.render_widget(
            Paragraph::new("↑↓ SELECT  ENTER PLAY  SPACE PAUSE  ←→ TRACK  Q QUIT")
                .style(dim)
                .alignment(Alignment::Center),
            rows[12],
        );
    }

    fn visible_status(&self) -> &str {
        if self.loading {
            return "SEARCHING…";
        }

        match self.status_at {
            Some(at) if at.elapsed() < Duration::from_secs(3) => self.status.as_str(),
            _ => "",
        }
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
