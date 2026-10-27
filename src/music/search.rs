use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};

use super::track::Track;

pub type SearchResult = (String, Result<Vec<Track>, String>);

pub fn search_music(query: String) -> SearchResult {
    let result = Python::attach(|py| -> PyResult<Vec<Track>> {
        let sys = PyModule::import(py, "sys")?;
        let path = sys.getattr("path")?.cast_into::<PyList>()?;

        let backend_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("backend");

        path.insert(0, backend_path.to_string_lossy().as_ref())?;

        let bridge = PyModule::import(py, "bridge")?;
        let results = bridge
            .getattr("search_music")?
            .call1((query.clone(),))?
            .cast_into::<PyList>()?;

        let mut tracks = Vec::new();

        for item in results.iter() {
            let title = item.get_item("title")?.extract::<String>()?;
            let artist = item.get_item("artist")?.extract::<String>()?;
            let stream_url = item.get_item("stream_url")?.extract::<String>()?;
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
