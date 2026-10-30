use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

use super::track::{PlaybackSource, Track};
use crate::config::Provider;

pub type SearchResult = (String, Result<Vec<Track>, String>);

#[derive(Debug)]
pub struct ResolveResponse {
    pub generation: u64,
    pub source: PlaybackSource,
    pub spotify_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub result: Result<Track, String>,
}

static RESOLVED_CACHE: LazyLock<Mutex<HashMap<String, Track>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn get_cached_track(track: &Track) -> Option<Track> {
    if !track.stream_url.is_empty() {
        return Some(track.clone());
    }

    let guard = RESOLVED_CACHE.lock().ok()?;

    if let Some(id) = &track.spotify_id
        && let Some(cached) = guard.get(id)
    {
        return Some(cached.clone());
    }

    let key = format!(
        "{}—{}",
        track.title.to_lowercase().trim(),
        track.artist.to_lowercase().trim()
    );
    guard.get(&key).cloned()
}

pub fn cache_resolved_track(track: &Track) {
    if track.stream_url.is_empty() {
        return;
    }

    if let Ok(mut guard) = RESOLVED_CACHE.lock() {
        if let Some(id) = &track.spotify_id {
            guard.insert(id.clone(), track.clone());
        }
        let key = format!(
            "{}—{}",
            track.title.to_lowercase().trim(),
            track.artist.to_lowercase().trim()
        );
        guard.insert(key, track.clone());
    }
}

fn ensure_backend_path(py: Python<'_>) -> PyResult<()> {
    static INIT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();

    let res = INIT.get_or_init(|| {
        let sys = PyModule::import(py, "sys").map_err(|e| e.to_string())?;
        let attr = sys.getattr("path").map_err(|e| e.to_string())?;
        let path = attr.cast_into::<PyList>().map_err(|e| e.to_string())?;

        let backend_path = crate::paths::data_dir().join("backend");
        let backend_str = backend_path.to_string_lossy();

        let already_present = path.iter().any(|item| {
            item.extract::<String>()
                .map(|s| s == backend_str.as_ref())
                .unwrap_or(false)
        });

        if !already_present {
            path.insert(0, backend_str.as_ref())
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    });

    match res {
        Ok(()) => Ok(()),
        Err(err) => Err(pyo3::exceptions::PyRuntimeError::new_err(err.clone())),
    }
}

pub fn search_music(query: String, provider: Provider) -> SearchResult {
    let result = Python::attach(|py| -> PyResult<Vec<Track>> {
        ensure_backend_path(py)?;

        let bridge = PyModule::import(py, "bridge")?;
        let results = bridge
            .getattr("search_music")?
            .call1((query.clone(), provider.as_str()))?
            .cast_into::<PyList>()?;

        let mut tracks = Vec::new();

        for item in results.iter() {
            let title = item.get_item("title")?.extract::<String>()?;
            let artist = item.get_item("artist")?.extract::<String>()?;
            let stream_url = item
                .get_item("stream_url")
                .and_then(|v| v.extract::<String>())
                .unwrap_or_default();
            let art_url = item
                .get_item("art_url")
                .and_then(|v| v.extract::<String>())
                .unwrap_or_default();

            let spotify_id = item
                .get_item("spotify_id")
                .ok()
                .and_then(|v| v.extract::<String>().ok())
                .filter(|s| !s.is_empty());

            let artists = item
                .get_item("artists")
                .ok()
                .and_then(|v| v.extract::<Vec<String>>().ok())
                .unwrap_or_else(|| vec![artist.clone()]);

            let album = item
                .get_item("album")
                .ok()
                .and_then(|v| v.extract::<String>().ok())
                .filter(|s| !s.is_empty());

            let duration_ms = item
                .get_item("duration_ms")
                .ok()
                .and_then(|v| v.extract::<u64>().ok());

            tracks.push(Track {
                title,
                artist,
                stream_url,
                art_url,
                spotify_id,
                artists,
                album,
                duration_ms,
            });
        }

        Ok(tracks)
    })
    .map_err(|error| error.to_string());

    (query, result)
}

pub fn resolve_playable(track: &Track) -> Result<Track, String> {
    if let Some(cached) = get_cached_track(track) {
        return Ok(cached);
    }
    let resolved_track = Python::attach(|py| -> PyResult<Track> {
        ensure_backend_path(py)?;

        let bridge = PyModule::import(py, "bridge")?;

        let dict = PyDict::new(py);
        dict.set_item("title", &track.title)?;
        dict.set_item("artist", &track.artist)?;
        dict.set_item("stream_url", &track.stream_url)?;
        dict.set_item("art_url", &track.art_url)?;
        if let Some(id) = &track.spotify_id {
            dict.set_item("spotify_id", id)?;
        }
        dict.set_item("artists", track.artists.clone())?;
        if let Some(album) = &track.album {
            dict.set_item("album", album)?;
        }
        if let Some(ms) = track.duration_ms {
            dict.set_item("duration_ms", ms)?;
        }
        let resolved = bridge.getattr("resolve_playable")?.call1((dict,))?;

        let title = resolved.get_item("title")?.extract::<String>()?;
        let artist = resolved.get_item("artist")?.extract::<String>()?;
        let stream_url = resolved
            .get_item("stream_url")
            .and_then(|v| v.extract::<String>())
            .unwrap_or_default();
        let art_url = resolved
            .get_item("art_url")
            .and_then(|v| v.extract::<String>())
            .unwrap_or_else(|_| track.art_url.clone());

        let spotify_id = resolved
            .get_item("spotify_id")
            .ok()
            .and_then(|v| v.extract::<String>().ok())
            .or_else(|| track.spotify_id.clone());

        let artists = resolved
            .get_item("artists")
            .ok()
            .and_then(|v| v.extract::<Vec<String>>().ok())
            .unwrap_or_else(|| track.artists.clone());

        let album = resolved
            .get_item("album")
            .ok()
            .and_then(|v| v.extract::<String>().ok())
            .or_else(|| track.album.clone());

        let duration_ms = resolved
            .get_item("duration_ms")
            .ok()
            .and_then(|v| v.extract::<u64>().ok())
            .or(track.duration_ms);

        Ok(Track {
            title,
            artist,
            stream_url,
            art_url,
            spotify_id,
            artists,
            album,
            duration_ms,
        })
    })
    .map_err(|error| error.to_string())?;

    cache_resolved_track(&resolved_track);
    Ok(resolved_track)
}
