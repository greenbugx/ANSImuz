use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::lyrics::model::Lyrics;
use crate::lyrics::types::LyricsError;
use crate::music::track::Track;

thread_local! {
    /// Thread-local cancellation token for active lyrics fetch task.
    pub static CURRENT_CANCEL_TOKEN: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// PyO3 wrapper exposing cancellation checks directly to Python lyrics provider chains.
#[pyclass]
#[derive(Clone)]
pub struct CancelTokenWrapper {
    token: Arc<AtomicBool>,
}

#[pymethods]
impl CancelTokenWrapper {
    fn is_cancelled(&self) -> bool {
        self.token.load(Ordering::Relaxed)
    }

    fn __call__(&self) -> bool {
        self.token.load(Ordering::Relaxed)
    }
}

/// Converts a Rust `Track` into a Python dictionary compatible with the backend bridge.
pub fn track_to_py_dict<'py>(py: Python<'py>, track: &Track) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("provider", &track.provider)?;
    if let Some(id) = &track.track_id {
        dict.set_item("track_id", id)?;
    }
    dict.set_item("title", &track.title)?;
    dict.set_item("artist", &track.artist)?;
    if let Some(album) = &track.album {
        dict.set_item("album", album)?;
    }
    if let Some(duration_ms) = track.duration_ms {
        dict.set_item("duration_ms", duration_ms)?;
    }
    if let Some(stream_url) = &track.stream_url {
        dict.set_item("stream_url", stream_url)?;
    } else {
        dict.set_item("stream_url", py.None())?;
    }
    if let Some(art_url) = &track.art_url {
        dict.set_item("art_url", art_url)?;
    } else {
        dict.set_item("art_url", py.None())?;
    }
    if let Some(spotify_id) = &track.spotify_id {
        dict.set_item("spotify_id", spotify_id)?;
    }
    dict.set_item("artists", track.artists.clone())?;
    Ok(dict)
}

/// Classifies backend error strings into structured `LyricsError` variants.
pub fn classify_lyrics_error(msg: &str) -> LyricsError {
    let lower = msg.to_lowercase();
    if lower.contains("rate limit") || lower.contains("429") {
        LyricsError::RateLimit(msg.to_string())
    } else if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("unreachable")
    {
        LyricsError::Network(msg.to_string())
    } else if lower.contains("not found") || lower.contains("404") {
        LyricsError::NotFound(msg.to_string())
    } else {
        LyricsError::Backend(msg.to_string())
    }
}

/// Fetches and deserializes normalized lyrics for the given track using the Python backend.
/// Checks the persistent disk cache before issuing network requests.
pub fn fetch_lyrics(track: &Track) -> Result<Option<Lyrics>, LyricsError> {
    // Check persistent disk cache first
    if let Some(cached) = crate::lyrics::cache::get_lyrics_from_cache(track) {
        return Ok(Some(cached));
    }

    let cancel_token = CURRENT_CANCEL_TOKEN.with(|c| c.borrow().clone());
    if let Some(ref token) = cancel_token
        && token.load(Ordering::Relaxed)
    {
        return Ok(None);
    }

    let json_opt = crate::music::with_backend(|py, bridge| {
        let py_dict = track_to_py_dict(py, track)?;
        let cancel_arg = match cancel_token {
            Some(token) => Bound::new(py, CancelTokenWrapper { token })?.into_any(),
            None => py.None().into_bound(py),
        };
        let res = bridge.getattr("fetch_lyrics")?.call1((py_dict, cancel_arg))?;
        if res.is_none() {
            Ok(None)
        } else {
            let json_module = py.import("json")?;
            let s: String = json_module.getattr("dumps")?.call1((res,))?.extract()?;
            Ok(Some(s))
        }
    })
    .map_err(|err_msg| classify_lyrics_error(&err_msg))?;

    if CURRENT_CANCEL_TOKEN.with(|c| {
        c.borrow()
            .as_ref()
            .map(|t| t.load(Ordering::Relaxed))
            .unwrap_or(false)
    }) {
        return Ok(None);
    }

    match json_opt {
        Some(json_str) => {
            let lyrics: Lyrics = serde_json::from_str(&json_str).map_err(|err| {
                LyricsError::Backend(format!("Failed to deserialize lyrics: {err}"))
            })?;
            lyrics.validate()?;
            crate::lyrics::cache::store_lyrics_in_cache(track, &lyrics);
            Ok(Some(lyrics))
        }
        None => Ok(None),
    }
}

/// Asynchronous response wrapper for lyrics retrieval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsResponse {
    pub generation: u64,
    pub track_title: String,
    pub artist: String,
    pub result: Result<Option<Lyrics>, LyricsError>,
}

impl LyricsResponse {
    /// Returns true if this response belongs to the specified generation and track identity.
    pub fn is_valid_for(&self, expected_generation: u64, track: &Track) -> bool {
        self.generation == expected_generation && self.matches_track(track)
    }

    /// Returns true if this response matches the given track title and artist.
    pub fn matches_track(&self, track: &Track) -> bool {
        self.track_title
            .trim()
            .eq_ignore_ascii_case(track.title.trim())
            && self.artist.trim().eq_ignore_ascii_case(track.artist.trim())
    }

    /// Returns true if the response should be rejected based on the expected generation
    /// and current playing track.
    pub fn should_reject(&self, expected_generation: u64, current_track: Option<&Track>) -> bool {
        match current_track {
            Some(track) => !self.is_valid_for(expected_generation, track),
            None => true,
        }
    }
}

/// Spawns a background thread to retrieve lyrics without blocking the UI event loop or playback.
pub fn spawn_fetch_lyrics(
    track: Track,
    generation: u64,
    tx: std::sync::mpsc::Sender<LyricsResponse>,
) -> JoinHandle<()> {
    let track_title = track.title.clone();
    let artist = track.artist.clone();

    std::thread::spawn(move || {
        let result = fetch_lyrics(&track);
        let _ = tx.send(LyricsResponse {
            generation,
            track_title,
            artist,
            result,
        });
    })
}
