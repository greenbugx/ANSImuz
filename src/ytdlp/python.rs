use std::time::{Duration, UNIX_EPOCH};

use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::{
    cache::{cache_stream, get_cached_stream},
    types::{ResolvedStream, ResolverStatus, SelfTestReport, StreamResolver},
};
use crate::music::search::with_backend;

pub struct PyResolver;

impl StreamResolver for PyResolver {
    fn resolve(&self, watch_url: &str, background: bool) -> Result<ResolvedStream, String> {
        with_backend(|_py, bridge| {
            let obj = bridge
                .getattr("resolve_stream")?
                .call1((watch_url, background))?;
            let dict = obj.cast_into::<PyDict>()?;

            let url: String = dict
                .get_item("url")?
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("missing url"))?
                .extract()?;

            let headers = match dict.get_item("headers")? {
                Some(h) if !h.is_none() => h
                    .cast_into::<PyDict>()?
                    .iter()
                    .filter_map(|(k, v)| Some((k.extract().ok()?, v.extract().ok()?)))
                    .collect(),
                _ => Vec::new(),
            };

            let expires_at = dict
                .get_item("expires_at")?
                .and_then(|v| v.extract::<u64>().ok())
                .map(|secs| UNIX_EPOCH + Duration::from_secs(secs));

            Ok(ResolvedStream {
                url,
                headers,
                expires_at,
                ext: dict.get_item("ext")?.and_then(|v| v.extract().ok()),
                acodec: dict.get_item("acodec")?.and_then(|v| v.extract().ok()),
                abr: dict.get_item("abr")?.and_then(|v| v.extract().ok()),
                duration_s: dict.get_item("duration_s")?.and_then(|v| v.extract().ok()),
                video_id: dict.get_item("video_id")?.and_then(|v| v.extract().ok()),
                title: dict.get_item("title")?.and_then(|v| v.extract().ok()),
                ytdlp_version: dict
                    .get_item("ytdlp_version")?
                    .and_then(|v| v.extract().ok())
                    .unwrap_or_default(),
                js_runtime: dict
                    .get_item("js_runtime")?
                    .and_then(|v| v.extract().ok())
                    .unwrap_or_default(),
            })
        })
    }
}

fn parse_status(dict: &Bound<'_, PyDict>) -> PyResult<ResolverStatus> {
    Ok(ResolverStatus {
        version: dict
            .get_item("version")?
            .and_then(|v| v.extract().ok())
            .unwrap_or_default(),
        bundle_age: dict.get_item("bundle_age")?.and_then(|v| v.extract().ok()),
        js_runtime: dict
            .get_item("js_runtime")?
            .and_then(|v| v.extract().ok())
            .unwrap_or_default(),
        js_runtime_path: dict
            .get_item("js_runtime_path")?
            .and_then(|v| v.extract().ok()),
    })
}

pub fn warmup() -> Result<ResolverStatus, String> {
    with_backend(|_py, bridge| {
        let obj = bridge.getattr("ytdlp_warmup")?.call0()?;
        let dict = obj.cast_into::<PyDict>()?;
        parse_status(&dict)
    })
}

pub fn spawn_warmup() -> std::sync::mpsc::Receiver<Result<ResolverStatus, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(warmup());
    });
    rx
}

pub fn status() -> Result<ResolverStatus, String> {
    with_backend(|_py, bridge| {
        let obj = bridge.getattr("ytdlp_status")?.call0()?;
        let dict = obj.cast_into::<PyDict>()?;
        parse_status(&dict)
    })
}

pub fn run_self_test() -> Result<SelfTestReport, String> {
    with_backend(|_py, bridge| {
        let data_dir = crate::paths::data_dir();
        let obj = bridge
            .getattr("self_test")?
            .call1((data_dir.to_string_lossy().as_ref(),))?;
        let dict = obj.cast_into::<PyDict>()?;
        let extract_str = |key: &str| -> PyResult<String> {
            Ok(dict
                .get_item(key)?
                .and_then(|v| v.extract().ok())
                .unwrap_or_default())
        };
        Ok(SelfTestReport {
            ytdlp_version: extract_str("ytdlp_version")?,
            ejs_version: extract_str("ejs_version")?,
            js_runtime: extract_str("js_runtime")?,
            js_runtime_path: extract_str("js_runtime_path")?,
            solver_output: extract_str("solver_output")?,
            ytdlp_binary_needed: dict
                .get_item("ytdlp_binary_needed")?
                .and_then(|v| v.extract().ok())
                .unwrap_or(false),
        })
    })
}

pub fn resolve_with(
    resolver: &dyn StreamResolver,
    watch_url: &str,
    background: bool,
) -> Result<ResolvedStream, String> {
    if let Some(cached) = get_cached_stream(watch_url) {
        return Ok(cached);
    }

    let resolved = resolver.resolve(watch_url, background)?;
    cache_stream(watch_url, &resolved);
    Ok(resolved)
}

pub fn resolve_stream(watch_url: &str, background: bool) -> Result<ResolvedStream, String> {
    resolve_with(&PyResolver, watch_url, background)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ytdlp::cache::clear_stream_cache;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockResolver {
        calls: AtomicUsize,
        result: Result<ResolvedStream, String>,
    }

    impl StreamResolver for MockResolver {
        fn resolve(&self, _watch_url: &str, _background: bool) -> Result<ResolvedStream, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    #[test]
    fn test_resolve_with_uses_mock_and_populates_cache() {
        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        clear_stream_cache();

        let stream = ResolvedStream {
            url: "https://rr1.googlevideo.com/videoplayback?id=abc".into(),
            headers: vec![("User-Agent".into(), "ANSImuz".into())],
            expires_at: None,
            ext: Some("webm".into()),
            acodec: Some("opus".into()),
            abr: Some(128.0),
            duration_s: Some(180.0),
            video_id: Some("mock_vid_1".into()),
            title: Some("Track".into()),
            ytdlp_version: "2026.11.23".into(),
            js_runtime: "system-node".into(),
        };

        let mock = MockResolver {
            calls: AtomicUsize::new(0),
            result: Ok(stream.clone()),
        };

        let watch = "https://www.youtube.com/watch?v=mock_vid_1";
        let first = resolve_with(&mock, watch, false).expect("first resolve");
        let second = resolve_with(&mock, watch, true).expect("cached resolve");

        assert_eq!(first, stream);
        assert_eq!(second, stream);
        assert_eq!(mock.calls.load(Ordering::SeqCst), 1);

        clear_stream_cache();
    }

    #[test]
    fn test_resolve_with_propagates_errors_without_caching() {
        let _guard = crate::paths::ENV_LOCK.lock().unwrap();
        clear_stream_cache();

        let mock = MockResolver {
            calls: AtomicUsize::new(0),
            result: Err("This track is not available in your region.".into()),
        };

        let watch = "https://www.youtube.com/watch?v=mock_vid_err";
        assert!(resolve_with(&mock, watch, false).is_err());
        assert!(resolve_with(&mock, watch, false).is_err());
        assert_eq!(mock.calls.load(Ordering::SeqCst), 2);
    }
}
