use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    runner::{now_playing, play_track},
    state::{App, Mode},
};
use crate::music::{SearchResult, search_music};
use crate::playback::{AudioPlayer, PlayerEvent, Repeat};

pub fn handle_key(
    app: &mut App,
    player: &mut AudioPlayer,
    key: KeyEvent,
    search_tx: &Sender<SearchResult>,
    player_tx: &Sender<PlayerEvent>,
) {
    if app.settings_open {
        match key.code {
            KeyCode::Up => app.settings_cursor = app.settings_cursor.saturating_sub(1),
            KeyCode::Down => app.settings_cursor = (app.settings_cursor + 1).min(3),
            KeyCode::Char(' ') | KeyCode::Enter => app.apply_setting(),
            KeyCode::Char('/') => {
                app.settings_open = false;
                app.mode = Mode::Type;
            }
            KeyCode::Esc => {
                app.settings_open = false;
                app.mode = Mode::Nav;
            }
            _ => {}
        }

        return;
    }

    match app.mode {
        Mode::Type => match key.code {
            KeyCode::Char('/') | KeyCode::Esc => app.mode = Mode::Nav,

            KeyCode::Enter if !app.loading => {
                let query = app.query.trim().to_string();

                if !query.is_empty() && query != app.last_query {
                    app.loading = true;
                    app.set_status("SEARCHING…");

                    let tx = search_tx.clone();

                    std::thread::spawn(move || {
                        let _ = tx.send(search_music(query));
                    });
                } else {
                    play_track(app, player, player_tx.clone(), app.selected);
                }
            }

            KeyCode::Up => app.move_selection(-1),
            KeyCode::Down => app.move_selection(1),

            KeyCode::Backspace => {
                app.query.pop();
            }

            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.query.push(ch);
            }

            _ => {}
        },

        Mode::Nav => match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,

            KeyCode::Esc => app.query.clear(),

            KeyCode::Up => app.move_selection(-1),
            KeyCode::Down => app.move_selection(1),

            KeyCode::Enter => play_track(app, player, player_tx.clone(), app.selected),

            KeyCode::Char(' ') => {
                if app.playing.is_some() {
                    match player.toggle_pause() {
                        Ok(()) => {
                            app.paused = !app.paused;
                            app.set_status(if app.paused {
                                "PAUSED".to_string()
                            } else {
                                now_playing(app)
                            });
                        }
                        Err(error) => app.set_status(format!("PLAYER ERROR: {}", error)),
                    }
                }
            }

            KeyCode::Left => {
                if let Some(index) = app.next_index(-1) {
                    play_track(app, player, player_tx.clone(), index);
                }
            }

            KeyCode::Right => {
                let next = if app.shuffle {
                    app.shuffled_next()
                } else {
                    app.next_index(1)
                };

                if let Some(index) = next {
                    play_track(app, player, player_tx.clone(), index);
                }
            }

            KeyCode::Char('x') | KeyCode::Char('X') => {
                app.shuffle = !app.shuffle;
                app.set_status(if app.shuffle {
                    "SHUFFLE ON"
                } else {
                    "SHUFFLE OFF"
                });
            }

            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.repeat = match app.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };

                app.set_status(match app.repeat {
                    Repeat::All => "REPEAT ALL",
                    Repeat::One => "REPEAT ONE",
                    Repeat::Off => "REPEAT OFF",
                });
            }

            KeyCode::Char('+') | KeyCode::Char('=') => {
                app.volume = app.volume.saturating_add(5).min(100);
                let _ = player.set_volume(app.volume);
                app.set_status(format!("VOLUME {}", app.volume));
            }

            KeyCode::Char('-') => {
                app.volume = app.volume.saturating_sub(5);
                let _ = player.set_volume(app.volume);
                app.set_status(format!("VOLUME {}", app.volume));
            }

            KeyCode::Char('/') => app.mode = Mode::Type,

            KeyCode::Char('s') | KeyCode::Char('S') => app.settings_open = true,

            _ => {}
        },
    }
}
