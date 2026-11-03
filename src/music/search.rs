use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};

use super::track::{PlaybackSource, Track};
use crate::config::Provider;

pub type SearchResult = (String, Result<Vec<Track>, String>);

#[derive(Clone, Debug)]
pub struct SearchResponse {
    pub generation: u64,
    pub query: String,
    pub result: Result<Vec<Track>, String>,
}
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

pub fn initialize_backend<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let backend_dir = crate::paths::backend_dir();
    if !backend_dir.is_dir() {
        return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!(
            "Backend directory does not exist: {}",
            backend_dir.display()
        )));
    }

    let bridge_file = backend_dir.join("bridge.py");
    if !bridge_file.is_file() {
        return Err(pyo3::exceptions::PyFileNotFoundError::new_err(format!(
            "Backend bridge script not found: {}",
            bridge_file.display()
        )));
    }

    let sys = PyModule::import(py, "sys")?;
    let path = sys.getattr("path")?.cast_into::<PyList>()?;
    let backend_str = backend_dir.to_string_lossy();

    let already_present = path.iter().any(|item| {
        item.extract::<String>()
            .map(|s| s == backend_str.as_ref())
            .unwrap_or(false)
    });

    if !already_present {
        path.insert(0, backend_str.as_ref())?;
    }

    PyModule::import(py, "bridge")
}

pub fn with_backend<T>(
    f: impl for<'py> FnOnce(Python<'py>, &Bound<'py, PyModule>) -> PyResult<T>,
) -> Result<T, String> {
    Python::attach(|py| {
        let bridge = initialize_backend(py)?;
        f(py, &bridge)
    })
    .map_err(|error| error.to_string())
}

pub fn search_music(query: String, provider: Provider) -> SearchResult {
    let result = with_backend(|_py, bridge| {
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
    });

    (query, result)
}

pub fn spawn_search(
    query: String,
    provider: Provider,
    generation: u64,
    tx: std::sync::mpsc::Sender<SearchResponse>,
) {
    std::thread::spawn(move || {
        let (q, result) = search_music(query, provider);
        let _ = tx.send(SearchResponse {
            generation,
            query: q,
            result,
        });
    });
}

pub fn resolve_playable(track: &Track) -> Result<Track, String> {
    if let Some(cached) = get_cached_track(track) {
        return Ok(cached);
    }
    let resolved_track = with_backend(|py, bridge| {
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
    })?;

    cache_resolved_track(&resolved_track);
    Ok(resolved_track)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_backend_loads_bridge_and_functions() {
        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        let res = with_backend(|_py, bridge| {
            assert!(bridge.hasattr("search_music").unwrap());
            assert!(bridge.hasattr("resolve_playable").unwrap());
            Ok(())
        });
        assert!(
            res.is_ok(),
            "Backend initialization must succeed: {:?}",
            res.err()
        );
    }

    #[test]
    fn test_sys_path_no_redundant_entries() {
        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        Python::attach(|py| {
            // First call
            let _ = initialize_backend(py).unwrap();
            // Second call
            let _ = initialize_backend(py).unwrap();

            let sys = PyModule::import(py, "sys").unwrap();
            let path = sys.getattr("path").unwrap().cast_into::<PyList>().unwrap();
            let backend_str = crate::paths::backend_dir().to_string_lossy().to_string();

            let count = path
                .iter()
                .filter(|item| {
                    item.extract::<String>()
                        .map(|s| s == backend_str)
                        .unwrap_or(false)
                })
                .count();

            assert_eq!(
                count, 1,
                "backend directory must appear exactly once in sys.path"
            );
        });
    }

    #[test]
    fn test_missing_backend_directory_produces_useful_error() {
        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("ANSIMUZ_DATA_DIR", "/tmp/nonexistent_ansimuz_dir_12345");
        }

        let res = with_backend(|_py, _bridge| Ok(()));
        assert!(
            res.is_err(),
            "Missing backend directory must produce an error"
        );
        let err = res.unwrap_err();
        assert!(
            err.contains("Backend directory does not exist")
                || err.contains("Backend bridge script not found"),
            "Error must clearly state missing backend, got: {err}"
        );

        unsafe {
            std::env::remove_var("ANSIMUZ_DATA_DIR");
        }
    }
}
