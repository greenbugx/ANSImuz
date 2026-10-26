use app::{App, Mode, Track};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    env, io,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};

mod app;
mod art;
mod player;
mod settings;
mod visualizer;

use player::{AudioPlayer, PlayerEvent};
use visualizer::AudioVisualizer;

type SearchResult = (String, Result<Vec<Track>, String>);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = setup_terminal()?;

    let result = run_app(&mut terminal);

    let restore_result = restore_terminal(&mut terminal);

    result?;
    restore_result?;

    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    let mut player = AudioPlayer::new();
    let mut visualizer = AudioVisualizer::new();

    let (search_tx, search_rx): (Sender<SearchResult>, Receiver<SearchResult>) =
        mpsc::channel();
    let (player_tx, player_rx): (Sender<PlayerEvent>, Receiver<PlayerEvent>) =
        mpsc::channel();

    let mut playback: Option<(f64, Option<f64>)> = None;

    loop {
        while let Ok((query, result)) = search_rx.try_recv() {
            app.loading = false;

            match result {
                Ok(tracks) => {
                    let count = tracks.len();
                    app.tracks = tracks;
                    app.selected = 0;
                    app.last_query = query;
                    app.mode = Mode::Nav;
                    app.set_status(if count == 0 {
                        "NO RESULTS".into()
                    } else {
                        format!("{} RESULTS", count)
                    });
                }
                Err(error) => {
                    app.set_status(format!("SEARCH ERROR: {}", error));
                }
            }
        }

        if app.playing.is_some() && !player.is_playing() {
            app.playing = None;
            app.paused = false;

            app.set_status(match player.error() {
                Some(error) => format!("PLAYER ERROR: {}", error),
                None => "PLAYBACK ENDED".to_string(),
            });
        }

        let current = player.generation();

        while let Ok(event) = player_rx.try_recv() {
            match event {
                PlayerEvent::Position { generation, elapsed, duration } if generation == current => {
                    playback = Some((elapsed, duration));
                }
                PlayerEvent::Failed { generation, message } if generation == current => {
                    app.set_status(format!("PLAYER ERROR: {}", message));
                }
                _ => {}
            }
        }

        if app.playing.is_none() {
            playback = None;
        }

        if app.playing.is_some() && !visualizer.has_attempted() {
            if let Err(error) = visualizer.start() {
                app.set_status(error);
            }
        } else if app.playing.is_none() && visualizer.has_attempted() {
            visualizer.stop();
        }

        let spectrum = visualizer.bands();
        let spectrum_active = visualizer.is_active();
        terminal.draw(|frame| {
            app.render(frame, playback, &spectrum, spectrum_active)
        })?;

        if event::poll(Duration::from_millis(80))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };

            if key.kind == KeyEventKind::Release {
                continue;
            }

            if key.code == KeyCode::Char('c')
                && key.modifiers.contains(KeyModifiers::CONTROL)
            {
                app.should_quit = true;
            } else {
                handle_key(&mut app, &mut player, key, &search_tx, &player_tx);
            }
        }

        if app.should_quit {
            break;
        }
    }

    visualizer.stop();
    player.stop();
    Ok(())
}

fn handle_key(
    app: &mut App,
    player: &mut AudioPlayer,
    key: KeyEvent,
    search_tx: &Sender<SearchResult>,
    player_tx: &Sender<PlayerEvent>,
) {
    if app.settings_open {
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter => {
                app.settings.audio_cache = !app.settings.audio_cache;
                app.settings.save();
            }
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

            KeyCode::Left => skip_track(app, player, player_tx.clone(), -1),
            KeyCode::Right => skip_track(app, player, player_tx.clone(), 1),

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

fn play_track(
    app: &mut App,
    player: &mut AudioPlayer,
    events: Sender<PlayerEvent>,
    index: usize,
) {
    let Some(track) = app.tracks.get(index).cloned() else {
        return;
    };

    let source = AudioPlayer::cached_audio(&track.stream_url)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| track.stream_url.clone());

    match player.play(&source, app.volume, events) {
        Ok(()) => {
            app.playing = Some(index);
            app.selected = index;
            app.paused = false;
            app.set_status(format!("PLAYING: {} — {}", track.title, track.artist));

            if app.settings.audio_cache {
                player.prefetch_later(&track.stream_url, 15);
            }

            if app.tracks.len() > 1 {
                let next = app
                    .tracks
                    .get((index + 1) % app.tracks.len())
                    .expect("index within tracks");

                if app.settings.audio_cache {
                    player.prefetch_later(&next.stream_url, 25);
                }
                art::prefetch(&next.art_url);
            }
        }
        Err(error) => app.set_status(format!("PLAYER ERROR: {}", error)),
    }
}

fn skip_track(
    app: &mut App,
    player: &mut AudioPlayer,
    events: Sender<PlayerEvent>,
    delta: isize,
) {
    if let Some(index) = app.next_index(delta) {
        play_track(app, player, events, index);
    }
}

fn now_playing(app: &App) -> String {
    app.playing
        .and_then(|i| app.tracks.get(i))
        .map(|t| format!("PLAYING: {} — {}", t.title, t.artist))
        .unwrap_or_else(|| "PLAYING".into())
}

fn search_music(query: String) -> SearchResult {
    let result = Python::attach(|py| -> PyResult<Vec<Track>> {
        let sys = PyModule::import(py, "sys")?;
        let path = sys.getattr("path")?.cast_into::<PyList>()?;

        let backend_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("backend");

        path.insert(0, backend_path.to_string_lossy().as_ref())?;

        let bridge = PyModule::import(py, "bridge")?;
        let results = bridge
            .getattr("search_music")?
            .call1((query.clone(),))?
            .cast_into::<PyList>()?;

        let mut tracks = Vec::new();

        for item in results.iter() {
            let title = item
                .get_item("title")?
                .extract::<String>()?;

            let artist = item
                .get_item("artist")?
                .extract::<String>()?;

            let stream_url = item
                .get_item("stream_url")?
                .extract::<String>()?;

            let art_url = item
                .get_item("art_url")?
                .extract::<String>()
                .unwrap_or_default();

            tracks.push(Track {
                title,
                artist,
                stream_url,
                art_url,
            });
        }

        Ok(tracks)
    })
    .map_err(|error| error.to_string());

    (query, result)
}
