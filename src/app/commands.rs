use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{
    runner::{now_playing, play_next, play_previous, play_queue_item, play_search_result},
    state::{App, ListFocus, Mode},
};
use crate::playback::{AudioPlayer, PlayerEvent, Repeat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    PlayPause,
    NextTrack,
    PreviousTrack,
    ToggleShuffle,
    CycleRepeat,
    VolumeUp,
    VolumeDown,
    FocusSearch,
    OpenSettings,
    ToggleHelp,
    OpenCommandPalette,
    ToggleProvider,
    ClearCache,
    Quit,
    ToggleFocus,
    EnqueueTrack,
    RemoveQueueItem,
    MoveQueueUp,
    MoveQueueDown,
}

pub struct CommandInfo {
    pub id: Action,
    pub name: &'static str,
    pub description: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
}

pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        id: Action::PlayPause,
        name: "Play / Pause",
        description: "Toggle playback between play and pause",
        shortcut: "Space",
        category: "Playback",
    },
    CommandInfo {
        id: Action::NextTrack,
        name: "Next Track",
        description: "Skip to next track in queue or shuffle",
        shortcut: "→",
        category: "Playback",
    },
    CommandInfo {
        id: Action::PreviousTrack,
        name: "Previous Track",
        description: "Go back to previous track in queue",
        shortcut: "←",
        category: "Playback",
    },
    CommandInfo {
        id: Action::ToggleShuffle,
        name: "Toggle Shuffle",
        description: "Toggle random track shuffle mode",
        shortcut: "x",
        category: "Playback",
    },
    CommandInfo {
        id: Action::CycleRepeat,
        name: "Cycle Repeat",
        description: "Cycle repeat mode: Off → All → One",
        shortcut: "r",
        category: "Playback",
    },
    CommandInfo {
        id: Action::VolumeUp,
        name: "Volume Up",
        description: "Increase playback volume by 5%",
        shortcut: "+",
        category: "Playback",
    },
    CommandInfo {
        id: Action::VolumeDown,
        name: "Volume Down",
        description: "Decrease playback volume by 5%",
        shortcut: "-",
        category: "Playback",
    },
    CommandInfo {
        id: Action::FocusSearch,
        name: "Search Songs",
        description: "Focus search bar to find new tracks",
        shortcut: "/",
        category: "Navigation",
    },
    CommandInfo {
        id: Action::OpenSettings,
        name: "Settings",
        description: "Open the configuration settings menu",
        shortcut: "s",
        category: "Navigation",
    },
    CommandInfo {
        id: Action::ToggleHelp,
        name: "Help",
        description: "Show keybindings and shortcut help",
        shortcut: "?",
        category: "System",
    },
    CommandInfo {
        id: Action::OpenCommandPalette,
        name: "Command Palette",
        description: "Open searchable command palette",
        shortcut: ":",
        category: "System",
    },
    CommandInfo {
        id: Action::ClearCache,
        name: "Clear Cache",
        description: "Remove cached artwork and audio from disk",
        shortcut: "",
        category: "System",
    },
    CommandInfo {
        id: Action::ToggleProvider,
        name: "Toggle Provider",
        description: "Switch music provider (YouTube Music / Spotify)",
        shortcut: "",
        category: "System",
    },
    CommandInfo {
        id: Action::Quit,
        name: "Quit",
        description: "Exit ANSImuz",
        shortcut: "q",
        category: "System",
    },
    CommandInfo {
        id: Action::ToggleFocus,
        name: "Switch Focus (Search / Queue)",
        description: "Switch list focus between search results and queue",
        shortcut: "Tab",
        category: "Navigation",
    },
    CommandInfo {
        id: Action::EnqueueTrack,
        name: "Enqueue Track",
        description: "Add highlighted search track to queue",
        shortcut: "a",
        category: "Playback",
    },
    CommandInfo {
        id: Action::RemoveQueueItem,
        name: "Remove from Queue",
        description: "Remove highlighted track from playback queue",
        shortcut: "Delete",
        category: "Playback",
    },
    CommandInfo {
        id: Action::MoveQueueUp,
        name: "Move Queue Item Up",
        description: "Move highlighted queue track earlier",
        shortcut: "Alt+↑",
        category: "Playback",
    },
    CommandInfo {
        id: Action::MoveQueueDown,
        name: "Move Queue Item Down",
        description: "Move highlighted queue track later",
        shortcut: "Alt+↓",
        category: "Playback",
    },
];

pub const PANEL_KEYBINDS: &[(&str, &str)] = &[
    ("↑↓", "SELECT"),
    ("ENTER", "PLAY"),
    ("SPACE", "PAUSE"),
    ("←→", "TRACK"),
    ("TAB", "FOCUS"),
    ("A", "QUEUE"),
    ("DEL", "REMOVE"),
    ("ALT+↑↓", "REORDER"),
    ("X", "SHUFFLE"),
    ("R", "REPEAT"),
    (":", "COMMANDS"),
    ("?", "HELP"),
    ("Q", "QUIT"),
];

pub const HELP_SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "PLAYBACK",
        &[
            ("Space", "Play / Pause playback"),
            ("← / →", "Previous / Next track"),
            ("X", "Toggle shuffle mode"),
            ("R", "Cycle repeat (Off / All / One)"),
            ("+ / -", "Volume up / down (±5%)"),
        ],
    ),
    (
        "NAVIGATION",
        &[
            ("Tab", "Switch focus between Search and Queue"),
            ("↑ / ↓", "Select track in focused list"),
            ("Enter", "Play selected track"),
            ("A", "Add highlighted search track to queue"),
            ("Delete", "Remove highlighted track from queue"),
            ("Alt+↑ / ↓", "Reorder queue item"),
            ("/", "Focus song search input"),
            ("Esc", "Clear search query / Close dialog"),
        ],
    ),
    (
        "SYSTEM",
        &[
            (": / Ctrl+P", "Open command palette"),
            ("S", "Open settings screen"),
            ("?", "Toggle this help overlay"),
            ("Q / Ctrl+C", "Quit ANSImuz"),
        ],
    ),
];

pub fn filtered_commands(query: &str) -> Vec<&'static CommandInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return COMMANDS.iter().collect();
    }

    COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&q)
                || cmd.description.to_lowercase().contains(&q)
                || cmd.shortcut.to_lowercase().contains(&q)
                || cmd.category.to_lowercase().contains(&q)
        })
        .collect()
}

pub fn action_for_nav_key(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::ALT) {
        return match key.code {
            KeyCode::Up => Some(Action::MoveQueueUp),
            KeyCode::Down => Some(Action::MoveQueueDown),
            _ => None,
        };
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('p') | KeyCode::Char('P') => Some(Action::OpenCommandPalette),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
        KeyCode::Char(' ') => Some(Action::PlayPause),
        KeyCode::Left => Some(Action::PreviousTrack),
        KeyCode::Right => Some(Action::NextTrack),
        KeyCode::Char('x') | KeyCode::Char('X') => Some(Action::ToggleShuffle),
        KeyCode::Char('r') | KeyCode::Char('R') => Some(Action::CycleRepeat),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::VolumeUp),
        KeyCode::Char('-') => Some(Action::VolumeDown),
        KeyCode::Char('/') => Some(Action::FocusSearch),
        KeyCode::Char('s') | KeyCode::Char('S') => Some(Action::OpenSettings),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char(':') => Some(Action::OpenCommandPalette),
        KeyCode::Tab | KeyCode::BackTab => Some(Action::ToggleFocus),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::EnqueueTrack),
        KeyCode::Delete => Some(Action::RemoveQueueItem),
        _ => None,
    }
}

pub fn dispatch_action(
    action: Action,
    app: &mut App,
    player: &mut AudioPlayer,
    player_tx: &Sender<PlayerEvent>,
    search_tx: &Sender<crate::music::SearchResult>,
    resolve_tx: &Sender<crate::music::ResolveResponse>,
) {
    match action {
        Action::PlayPause => {
            if app.is_resolving {
                app.cancel_pending_resolution();
                app.set_status("RESOLVE CANCELLED");
                return;
            }

            if app.playing.is_some() || app.now_playing.is_some() {
                match player.toggle_pause() {
                    Ok(()) => {
                        app.paused = !app.paused;
                        app.set_status(if app.paused {
                            "PAUSED".to_string()
                        } else {
                            now_playing(app)
                        });
                    }
                    Err(error) => app.set_status(format!("PLAYER ERROR: {error}")),
                }
            } else if app.list_focus == ListFocus::Queue && !app.queue.is_empty() {
                play_queue_item(
                    app,
                    player,
                    player_tx.clone(),
                    resolve_tx.clone(),
                    app.queue_selected,
                );
            } else if !app.tracks.is_empty() {
                play_search_result(
                    app,
                    player,
                    player_tx.clone(),
                    resolve_tx.clone(),
                    app.selected,
                );
            }
        }

        Action::NextTrack => {
            play_next(app, player, player_tx.clone(), resolve_tx.clone());
        }

        Action::PreviousTrack => {
            play_previous(app, player, player_tx.clone(), resolve_tx.clone());
        }

        Action::ToggleShuffle => {
            app.shuffle = !app.shuffle;
            if app.shuffle {
                app.shuffle_queue();
            }
            app.set_status(if app.shuffle {
                "SHUFFLE ON"
            } else {
                "SHUFFLE OFF"
            });
        }

        Action::CycleRepeat => {
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

        Action::VolumeUp => {
            app.volume = app.volume.saturating_add(5).min(100);
            let _ = player.set_volume(app.volume);
            app.set_status(format!("VOLUME {}", app.volume));
        }

        Action::VolumeDown => {
            app.volume = app.volume.saturating_sub(5);
            let _ = player.set_volume(app.volume);
            app.set_status(format!("VOLUME {}", app.volume));
        }

        Action::FocusSearch => {
            app.close_command_palette();
            app.help_open = false;
            app.settings_open = false;
            app.mode = Mode::Type;
        }

        Action::OpenSettings => {
            app.close_command_palette();
            app.help_open = false;
            app.settings_open = true;
        }

        Action::ToggleHelp => {
            app.toggle_help();
        }

        Action::OpenCommandPalette => {
            app.open_command_palette();
        }

        Action::ToggleProvider => {
            app.cancel_pending_resolution();
            app.settings.provider = app.settings.provider.toggle();
            app.settings.save();
            app.set_status(format!("PROVIDER: {}", app.settings.provider.label()));
            crate::app::runner::refetch_if_needed(app, search_tx);
        }
        Action::ClearCache => {
            crate::config::clear_caches();
            app.set_status("CACHES CLEARED");
        }

        Action::Quit => {
            app.should_quit = true;
        }
        Action::ToggleFocus => {
            app.toggle_focus();
        }
        Action::EnqueueTrack => {
            if app.list_focus == ListFocus::Search {
                app.enqueue_selected();
            }
        }
        Action::RemoveQueueItem => {
            if app.list_focus == ListFocus::Queue
                && let Some(entry) = app.remove_queue_item(app.queue_selected)
            {
                app.set_status(format!("REMOVED: {}", entry.track.title));
            }
        }
        Action::MoveQueueUp => {
            if app.list_focus == ListFocus::Queue {
                app.move_selected_queue_item(-1);
            }
        }
        Action::MoveQueueDown => {
            if app.list_focus == ListFocus::Queue {
                app.move_selected_queue_item(1);
            }
        }
    }
}
