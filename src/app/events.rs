use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    commands::{action_for_nav_key, dispatch_action, filtered_commands},
    runner::{play_queue_item, play_search_result},
    state::{App, ListFocus, Mode},
};
use crate::music::{SearchResult, search_music};
use crate::playback::{AudioPlayer, PlayerEvent};

pub fn handle_key(
    app: &mut App,
    player: &mut AudioPlayer,
    key: KeyEvent,
    search_tx: &Sender<SearchResult>,
    player_tx: &Sender<PlayerEvent>,
    resolve_tx: &Sender<crate::music::ResolveResponse>,
) {
    if app.command_palette_open {
        match key.code {
            KeyCode::Esc => app.close_command_palette(),

            KeyCode::Up => {
                app.command_palette_cursor = app.command_palette_cursor.saturating_sub(1);
            }

            KeyCode::Down => {
                let count = filtered_commands(&app.command_palette_query).len();
                if count > 0 {
                    app.command_palette_cursor = (app.command_palette_cursor + 1).min(count - 1);
                }
            }

            KeyCode::Enter => {
                let commands = filtered_commands(&app.command_palette_query);
                if let Some(cmd) = commands.get(app.command_palette_cursor) {
                    let action = cmd.id;
                    app.close_command_palette();
                    dispatch_action(action, app, player, player_tx, search_tx, resolve_tx);
                }
            }

            KeyCode::Backspace => {
                app.command_palette_query.pop();
                app.command_palette_cursor = 0;
            }

            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.command_palette_query.push(ch);
                app.command_palette_cursor = 0;
            }

            _ => {}
        }
        return;
    }

    if app.help_open {
        match key.code {
            KeyCode::Esc
            | KeyCode::Char('?')
            | KeyCode::Char('q')
            | KeyCode::Char('Q')
            | KeyCode::Enter
            | KeyCode::Char(' ') => {
                app.help_open = false;
            }
            _ => {}
        }
        return;
    }

    if app.settings_open {
        match key.code {
            KeyCode::Up => app.settings_cursor = app.settings_cursor.saturating_sub(1),
            KeyCode::Down => app.settings_cursor = (app.settings_cursor + 1).min(4),
            KeyCode::Char(' ') | KeyCode::Enter => {
                let old_provider = app.settings.provider;
                app.apply_setting();
                if app.settings.provider != old_provider {
                    app.cancel_pending_resolution();
                    super::runner::refetch_if_needed(app, search_tx);
                }
            }
            KeyCode::Char('/') => {
                app.settings_open = false;
                app.mode = Mode::Type;
            }
            KeyCode::Esc => {
                app.settings_open = false;
                app.mode = Mode::Nav;
            }
            KeyCode::Char('?') => {
                app.toggle_help();
            }
            KeyCode::Char(':') => {
                app.open_command_palette();
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
                let provider = app.settings.provider;

                if !query.is_empty() && query != app.last_query {
                    app.cancel_pending_resolution();
                    app.loading = true;
                    app.set_status("SEARCHING…");

                    let tx = search_tx.clone();

                    std::thread::spawn(move || {
                        let _ = tx.send(search_music(query, provider));
                    });
                } else {
                    play_search_result(
                        app,
                        player,
                        player_tx.clone(),
                        resolve_tx.clone(),
                        app.selected,
                    );
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

        Mode::Nav => {
            if let Some(action) = action_for_nav_key(key) {
                dispatch_action(action, app, player, player_tx, search_tx, resolve_tx);
                return;
            }

            match key.code {
                KeyCode::Esc => {
                    if app.is_resolving {
                        app.cancel_pending_resolution();
                        app.set_status("RESOLVE CANCELLED");
                    } else {
                        app.query.clear();
                    }
                }
                KeyCode::Up => match app.list_focus {
                    ListFocus::Search => app.move_selection(-1),
                    ListFocus::Queue => app.move_queue_selection(-1),
                },
                KeyCode::Down => match app.list_focus {
                    ListFocus::Search => app.move_selection(1),
                    ListFocus::Queue => app.move_queue_selection(1),
                },
                KeyCode::Enter => match app.list_focus {
                    ListFocus::Search => play_search_result(
                        app,
                        player,
                        player_tx.clone(),
                        resolve_tx.clone(),
                        app.selected,
                    ),
                    ListFocus::Queue => {
                        if !app.queue.is_empty() {
                            play_queue_item(
                                app,
                                player,
                                player_tx.clone(),
                                resolve_tx.clone(),
                                app.queue_selected,
                            );
                        }
                    }
                },
                _ => {}
            }
        }
    }
}
