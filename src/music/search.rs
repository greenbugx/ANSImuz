use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

use super::track::Track;
use crate::config::Provider;

pub type SearchResult = (String, Result<Vec<Track>, String>);

#[derive(Debug)]
pub struct ResolveResponse {
    pub generation: u64,
    pub index: usize,
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
    let sys = PyModule::import(py, "sys")?;
    let path = sys.getattr("path")?.cast_into::<PyList>()?;
    let backend_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend");
    let backend_str = backend_path.to_string_lossy();

    let mut found = false;
    for item in path.iter() {
        if let Ok(s) = item.extract::<String>()
            && s == backend_str.as_ref()
        {
            found = true;
            break;
        }
    }

    if !found {
        path.insert(0, backend_str.as_ref())?;
    }

    Ok(())
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
