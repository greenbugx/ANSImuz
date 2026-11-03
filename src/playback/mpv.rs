use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::Duration,
};

use super::events::PlayerEvent;

const MAX_DUMP_BYTES: u64 = 64 * 1024 * 1024;
const MIN_DUMP_BYTES: u64 = 100 * 1024;

const REQ_ID_VOLUME: u64 = 1;
const REQ_ID_TIME_POS: u64 = 2;
const REQ_ID_DURATION: u64 = 3;

fn generation_socket_path(generation: u64) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ansimuz-mpv-{}-{}.sock",
        std::process::id(),
        generation
    ))
}

pub struct AudioPlayer {
    child: Option<Child>,
    socket_path: PathBuf,
    paused: bool,
    last_error: Option<String>,
    generation: u64,
    dumping: Arc<Mutex<HashSet<String>>>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
    cancel_observer: Option<Arc<AtomicBool>>,
    cancel_prefetch: Arc<AtomicBool>,
    prefetch_results: Arc<Mutex<HashMap<String, PrefetchResult>>>,
    is_mock: bool,
    mock_playing: bool,
    mock_fail: bool,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let generation = 0;
        let socket_path = generation_socket_path(generation);

        Self {
            child: None,
            socket_path,
            paused: false,
            last_error: None,
            generation,
            dumping: Arc::new(Mutex::new(HashSet::new())),
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
            cancel_observer: None,
            cancel_prefetch: Arc::new(AtomicBool::new(false)),
            prefetch_results: Arc::new(Mutex::new(HashMap::new())),
            is_mock: false,
            mock_playing: false,
            mock_fail: false,
        }
    }

    pub fn new_mock() -> Self {
        Self {
            child: None,
            socket_path: PathBuf::new(),
            paused: false,
            last_error: None,
            generation: 0,
            dumping: Arc::new(Mutex::new(HashSet::new())),
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
            cancel_observer: None,
            cancel_prefetch: Arc::new(AtomicBool::new(false)),
            prefetch_results: Arc::new(Mutex::new(HashMap::new())),
            is_mock: true,
            mock_playing: false,
            mock_fail: false,
        }
    }

    pub fn set_mock_fail(&mut self, fail: bool) {
        self.mock_fail = fail;
        if fail {
            self.last_error = Some("Failed to launch mpv: process failed to start".into());
        }
    }

    pub fn record_error(&mut self, error: String) {
        self.last_error = Some(error);
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    pub fn cached_audio(url: &str) -> Option<PathBuf> {
        let path = audio_cache_dir().join(format!("{}.audio", audio_hash(url)));
        path.metadata()
            .map(|meta| meta.len() > 0)
            .ok()
            .filter(|ok| *ok)
            .map(|_| path)
    }

    pub fn prefetch_later(&self, url: &str, delay_secs: u64) {
        if url.trim().is_empty() || Self::cached_audio(url).is_some() {
            return;
        }

        let dumping = Arc::clone(&self.dumping);
        let results = Arc::clone(&self.prefetch_results);
        let cancel = Arc::clone(&self.cancel_prefetch);
        let owned = url.to_string();

        thread::spawn(move || {
            for _ in 0..(delay_secs * 10) {
                if cancel.load(Ordering::Relaxed) {
                    if let Ok(mut lock) = results.lock() {
                        lock.insert(owned, PrefetchResult::Cancelled);
                    }
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }

            if cancel.load(Ordering::Relaxed) {
                if let Ok(mut lock) = results.lock() {
                    lock.insert(owned, PrefetchResult::Cancelled);
                }
                return;
            }

            let res = start_dump_cancellable(&owned, dumping, Some(cancel));
            if let Ok(mut lock) = results.lock() {
                lock.insert(owned, res);
            }
        });
    }

    pub fn prefetch_result(&self, url: &str) -> Option<PrefetchResult> {
        self.prefetch_results.lock().ok()?.get(url).cloned()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_playing(&mut self) -> bool {
        if self.is_mock {
            return self.mock_playing && !self.paused;
        }

        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child = None;
                    if !status.success() {
                        let lines = self
                            .stderr_lines
                            .lock()
                            .map(|l| l.clone())
                            .unwrap_or_default();
                        let error_msg = extract_mpv_error(&lines, Some(status));
                        self.last_error = Some(error_msg);
                    }
                }
                Ok(None) => return true,
                Err(e) => self.last_error = Some(e.to_string()),
            }
        }

        false
    }

    pub fn error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn play(
        &mut self,
        url: &str,
        volume: u8,
        events: Sender<PlayerEvent>,
    ) -> Result<(), String> {
        if self.is_mock {
            self.stop();
            self.last_error = None;
            self.paused = false;
            self.generation += 1;
            if self.mock_fail {
                self.mock_playing = false;
                self.last_error = Some("Failed to launch mpv: process failed to start".into());
                return Err("Failed to launch mpv: process failed to start".into());
            }
            self.mock_playing = true;
            return Ok(());
        }

        self.stop();

        self.last_error = None;
        self.paused = false;
        self.cancel_prefetch = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        let generation = self.generation;
        self.socket_path = generation_socket_path(generation);
        if self.socket_path.exists() {
            let _ = fs::remove_file(&self.socket_path);
        }

        let mut cmd = Command::new("mpv");
        cmd.arg("--no-video")
            .arg("--idle=no")
            .arg("--force-window=no")
            .arg(format!("--input-ipc-server={}", self.socket_path.display()))
            .arg("--ytdl-raw-options-append=extractor-args=youtube:player_client=visionos,default")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to launch mpv: {e}. Make sure mpv is installed!"))?;

        let stderr_lines = Arc::new(Mutex::new(Vec::new()));
        if let Some(stderr) = child.stderr.take() {
            let buf = Arc::clone(&stderr_lines);
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    let trimmed = line.trim();
                    if !trimmed.is_empty()
                        && let Ok(mut lock) = buf.lock()
                    {
                        if lock.len() >= 50 {
                            lock.remove(0);
                        }
                        lock.push(trimmed.to_string());
                    }
                }
            });
        }

        self.stderr_lines = stderr_lines;
        self.child = Some(child);

        let socket_path = self.socket_path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_observer = Some(Arc::clone(&cancel));

        thread::spawn(move || observe(generation, socket_path, volume, events, cancel));

        Ok(())
    }

    fn send(&mut self, command: &str) -> Result<(), String> {
        if !self.is_playing() {
            return Err("Nothing is playing.".into());
        }

        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to MPV socket: {e}"))?;

        stream
            .write_all(format!("{command}\n").as_bytes())
            .map_err(|e| format!("Failed to send command to MPV: {e}"))?;

        Ok(())
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        if self.is_mock {
            self.paused = !self.paused;
            return Ok(());
        }
        self.send(r#"{"command":["cycle","pause"]}"#)?;
        self.paused = !self.paused;
        Ok(())
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        if self.is_mock {
            return Ok(());
        }
        let cmd = serde_json::json!({
            "command": ["set_property", "volume", volume.min(100)],
        });
        self.send(&cmd.to_string())?;
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.is_mock {
            self.mock_playing = false;
            self.paused = false;
            return;
        }

        // Immediately signal observer and prefetch tasks to cancel before any shutdown actions
        if let Some(cancel) = self.cancel_observer.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.cancel_prefetch.store(true, Ordering::SeqCst);
        // Terminate and reap the child process if active
        if let Some(mut child) = self.child.take() {
            // Attempt graceful quit via IPC socket
            if self.socket_path.exists()
                && let Ok(mut stream) = UnixStream::connect(&self.socket_path)
            {
                let _ = stream.write_all(b"{\"command\":[\"quit\"]}\n");
            }

            let mut exited = false;
            for _ in 0..10 {
                match child.try_wait() {
                    Ok(Some(_status)) => {
                        exited = true;
                        break;
                    }
                    Ok(None) => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {
                        exited = false;
                        break;
                    }
                }
            }

            if !exited {
                let _ = child.kill();
            }

            // Always wait to reap zombie process
            let _ = child.wait();
        }

        self.paused = false;

        // Remove IPC socket file after child process has exited
        if self.socket_path.exists() {
            let _ = fs::remove_file(&self.socket_path);
        }
    }

    #[cfg(test)]
    pub fn set_child_for_test(
        &mut self,
        child: Child,
        socket_path: PathBuf,
        cancel: Arc<AtomicBool>,
    ) {
        self.child = Some(child);
        self.socket_path = socket_path;
        self.cancel_observer = Some(cancel);
        self.is_mock = false;
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn observe(
    generation: u64,
    socket_path: PathBuf,
    volume: u8,
    events: Sender<PlayerEvent>,
    cancelled: Arc<AtomicBool>,
) {
    for _ in 0..80 {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        if socket_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    if cancelled.load(Ordering::Relaxed) {
        return;
    }

    if !socket_path.exists() {
        if !cancelled.load(Ordering::Relaxed) {
            let _ = events.send(PlayerEvent::Failed {
                generation,
                message: "mpv did not start".into(),
            });
        }
        return;
    }

    let Ok(stream) = UnixStream::connect(&socket_path) else {
        if !cancelled.load(Ordering::Relaxed) {
            let _ = events.send(PlayerEvent::Failed {
                generation,
                message: "cannot connect to mpv socket".into(),
            });
        }
        return;
    };

    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));

    let mut stream_writer = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);

    let vol_cmd = serde_json::json!({
        "command": ["set_property", "volume", volume.min(100)],
        "request_id": REQ_ID_VOLUME,
    });
    if let Ok(bytes) = serde_json::to_vec(&vol_cmd) {
        let _ = stream_writer.write_all(&bytes);
        let _ = stream_writer.write_all(b"\n");
    }

    let mut duration: Option<f64> = None;
    let mut line = String::new();

    while !cancelled.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(250));
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let time_cmd = serde_json::json!({
            "command": ["get_property", "time-pos"],
            "request_id": REQ_ID_TIME_POS,
        });
        if let Ok(bytes) = serde_json::to_vec(&time_cmd)
            && (stream_writer.write_all(&bytes).is_err() || stream_writer.write_all(b"\n").is_err())
        {
            if !cancelled.load(Ordering::Relaxed) {
                let _ = events.send(PlayerEvent::Failed {
                    generation,
                    message: "Connection to mpv lost".into(),
                });
            }
            break;
        }

        if duration.is_none() {
            let dur_cmd = serde_json::json!({
                "command": ["get_property", "duration"],
                "request_id": REQ_ID_DURATION,
            });
            if let Ok(bytes) = serde_json::to_vec(&dur_cmd) {
                let _ = stream_writer.write_all(&bytes);
                let _ = stream_writer.write_all(b"\n");
            }
        }

        let mut elapsed: Option<f64> = None;

        loop {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    if !cancelled.load(Ordering::Relaxed) {
                        let _ = events.send(PlayerEvent::Failed {
                            generation,
                            message: "Playback terminated unexpectedly".into(),
                        });
                    }
                    return;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        if let Some(req_id) = val.get("request_id").and_then(|id| id.as_u64()) {
                            let is_success =
                                val.get("error").and_then(|e| e.as_str()) == Some("success");
                            if req_id == REQ_ID_TIME_POS && is_success {
                                elapsed = val.get("data").and_then(|d| d.as_f64());
                            } else if req_id == REQ_ID_DURATION && is_success {
                                duration = val.get("data").and_then(|d| d.as_f64());
                            }
                        }

                        if let Some(event) = val.get("event").and_then(|e| e.as_str())
                            && event == "end-file"
                        {
                            let reason = val.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                            if reason == "error" {
                                let msg = val
                                    .get("file_error")
                                    .and_then(|fe| fe.as_str())
                                    .unwrap_or("Playback failed");
                                if !cancelled.load(Ordering::Relaxed) {
                                    let _ = events.send(PlayerEvent::Failed {
                                        generation,
                                        message: msg.to_string(),
                                    });
                                }
                                return;
                            } else if reason == "eof" {
                                if !cancelled.load(Ordering::Relaxed) {
                                    let _ = events.send(PlayerEvent::Ended { generation });
                                }
                                return;
                            } else if reason == "stop" || reason == "quit" {
                                return;
                            }
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(_) => {
                    if !cancelled.load(Ordering::Relaxed) {
                        let _ = events.send(PlayerEvent::Failed {
                            generation,
                            message: "Connection to mpv lost".into(),
                        });
                    }
                    return;
                }
            }
        }
        if let Some(el) = elapsed {
            let _ = events.send(PlayerEvent::Position {
                generation,
                elapsed: el,
                duration,
            });
        }
    }
}

pub fn extract_mpv_error(
    lines: &[String],
    exit_status: Option<std::process::ExitStatus>,
) -> String {
    for line in lines.iter().rev() {
        if line.contains("[ytdl_hook] ERROR:") {
            let msg = line
                .split("[ytdl_hook] ERROR:")
                .nth(1)
                .unwrap_or(line)
                .trim();
            if !msg.is_empty() {
                return msg.to_string();
            }
        }
    }

    for line in lines.iter().rev() {
        let l = line.trim();
        if l.starts_with("Exiting...") {
            continue;
        }
        if l.contains("Failed to")
            || l.contains("Cannot open")
            || l.contains("No such file")
            || l.contains("403 Forbidden")
            || l.contains("404 Not Found")
            || l.contains("HTTP error")
            || l.contains("Server returned")
        {
            let clean = if let Some(idx) = l.rfind("] ") {
                &l[idx + 2..]
            } else {
                l
            };
            return clean.to_string();
        }
    }

    for line in lines.iter().rev() {
        let l = line.trim();
        if l.starts_with("Exiting...")
            || l.starts_with("AO: ")
            || l.starts_with("VO: ")
            || l.starts_with("A: ")
        {
            continue;
        }
        if !l.is_empty() {
            let clean = if let Some(idx) = l.rfind("] ") {
                &l[idx + 2..]
            } else {
                l
            };
            return clean.to_string();
        }
    }

    if let Some(status) = exit_status {
        format!("mpv exited with {status}")
    } else {
        "Playback failed".to_string()
    }
}

pub fn audio_cache_dir() -> PathBuf {
    crate::config::audio_cache_dir()
}

fn audio_hash(url: &str) -> String {
    url.bytes()
        .fold(14695981039346656037u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
        })
        .to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefetchResult {
    Cached(PathBuf),
    AlreadyCached(PathBuf),
    Failed(String),
    Cancelled,
}

pub fn prefetch_stream(url: &str, dumping: Arc<Mutex<HashSet<String>>>) -> PrefetchResult {
    start_dump(url, dumping)
}

pub fn start_dump(url: &str, dumping: Arc<Mutex<HashSet<String>>>) -> PrefetchResult {
    start_dump_cancellable(url, dumping, None)
}

pub fn start_dump_cancellable(
    url: &str,
    dumping: Arc<Mutex<HashSet<String>>>,
    cancel: Option<Arc<AtomicBool>>,
) -> PrefetchResult {
    if url.trim().is_empty() {
        return PrefetchResult::Failed("Empty URL".into());
    }

    if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
        return PrefetchResult::Cancelled;
    }

    let final_path = audio_cache_dir().join(format!("{}.audio", audio_hash(url)));
    if final_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return PrefetchResult::AlreadyCached(final_path);
    }

    {
        let mut set = match dumping.lock() {
            Ok(set) => set,
            Err(_) => return PrefetchResult::Failed("Dumping lock poisoned".into()),
        };

        if set.contains(url) {
            return PrefetchResult::Cancelled;
        }
        set.insert(url.to_string());
    }

    struct DumpingGuard {
        url: String,
        dumping: Arc<Mutex<HashSet<String>>>,
    }

    impl Drop for DumpingGuard {
        fn drop(&mut self) {
            if let Ok(mut set) = self.dumping.lock() {
                set.remove(&self.url);
            }
        }
    }

    let _dump_guard = DumpingGuard {
        url: url.to_string(),
        dumping: Arc::clone(&dumping),
    };

    if let Err(e) = fs::create_dir_all(audio_cache_dir()) {
        return PrefetchResult::Failed(format!("Cannot create audio cache directory: {e}"));
    }

    let part = audio_cache_dir().join(format!("{}.part", audio_hash(url)));
    if part.exists() {
        let _ = fs::remove_file(&part);
    }

    struct PartGuard {
        path: PathBuf,
        success: bool,
    }

    impl Drop for PartGuard {
        fn drop(&mut self) {
            if !self.success && self.path.exists() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    let mut part_guard = PartGuard {
        path: part.clone(),
        success: false,
    };
    if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
        return PrefetchResult::Cancelled;
    }

    let media = match resolve_stream(url) {
        Ok(m) => m,
        Err(err) => return PrefetchResult::Failed(err),
    };

    if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
        return PrefetchResult::Cancelled;
    }
    let dump = Command::new("mpv")
        .arg("--no-video")
        .arg("--no-audio")
        .arg("--really-quiet")
        .arg("--ytdl-raw-options-append=extractor-args=youtube:player_client=visionos,default")
        .arg(format!("--stream-dump={}", part.display()))
        .arg(&media)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match dump {
        Ok(c) => c,
        Err(e) => return PrefetchResult::Failed(format!("Failed to spawn mpv stream-dump: {e}")),
    };

    let stderr_lines = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = child.stderr.take() {
        let buf = Arc::clone(&stderr_lines);
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty()
                    && let Ok(mut lock) = buf.lock()
                {
                    if lock.len() >= 50 {
                        lock.remove(0);
                    }
                    lock.push(trimmed.to_string());
                }
            }
        });
    }

    let mut failure_reason: Option<String> = None;
    let mut completed_ok = false;

    for _ in 0..600 {
        if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return PrefetchResult::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    completed_ok = true;
                } else {
                    let lines = stderr_lines.lock().map(|l| l.clone()).unwrap_or_default();
                    let err = extract_mpv_error(&lines, Some(status));
                    failure_reason = Some(format!("mpv stream-dump exited with error: {err}"));
                }
                break;
            }
            Ok(None) => {
                if let Ok(meta) = part.metadata()
                    && meta.len() > MAX_DUMP_BYTES
                {
                    failure_reason = Some(format!(
                        "stream-dump exceeded maximum size ({} bytes > {} bytes)",
                        meta.len(),
                        MAX_DUMP_BYTES
                    ));
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
            Err(e) => {
                failure_reason = Some(format!("Error polling mpv stream-dump: {e}"));
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }

    if let Ok(None) = child.try_wait() {
        let _ = child.kill();
        let _ = child.wait();
        failure_reason = Some("mpv stream-dump timed out after 600 seconds".into());
    }

    if let Some(err) = failure_reason {
        return PrefetchResult::Failed(err);
    }

    if !completed_ok {
        return PrefetchResult::Failed("mpv stream-dump failed to complete successfully".into());
    }

    let file_size = match part.metadata() {
        Ok(meta) => meta.len(),
        Err(e) => {
            return PrefetchResult::Failed(format!(
                "Failed to read stream-dump file metadata: {e}"
            ));
        }
    };

    if file_size < MIN_DUMP_BYTES {
        return PrefetchResult::Failed(format!(
            "Prefetched audio file too small: {file_size} bytes (minimum is {MIN_DUMP_BYTES} bytes)"
        ));
    }

    if file_size > MAX_DUMP_BYTES {
        return PrefetchResult::Failed(format!(
            "Prefetched audio file too large: {file_size} bytes (maximum is {MAX_DUMP_BYTES} bytes)"
        ));
    }

    if let Err(e) = fs::rename(&part, &final_path) {
        return PrefetchResult::Failed(format!("Failed to rename .part to final cache file: {e}"));
    }

    part_guard.success = true;
    PrefetchResult::Cached(final_path)
}

fn resolve_stream(url: &str) -> Result<String, String> {
    let output = Command::new("yt-dlp")
        .arg("--extractor-args")
        .arg("youtube:player_client=visionos,default")
        .arg("-f")
        .arg("bestaudio")
        .arg("--get-url")
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to execute yt-dlp: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let err_msg = if !stderr.is_empty() {
            stderr
        } else {
            format!("yt-dlp exited with status {}", output.status)
        };
        return Err(format!("yt-dlp stream resolution failed: {err_msg}"));
    }

    let media = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    if media.is_empty() {
        return Err("yt-dlp returned empty media stream URL".into());
    }

    Ok(media)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_socket_path_unique() {
        let path1 = generation_socket_path(1);
        let path2 = generation_socket_path(2);
        assert_ne!(path1, path2);
        assert!(path1.to_string_lossy().contains("-1.sock"));
        assert!(path2.to_string_lossy().contains("-2.sock"));
    }

    #[test]
    fn test_extract_mpv_error_diagnostics() {
        let lines = vec![
            "[ffmpeg] tcp: Failed to resolve hostname invalid.example.com: No address".into(),
            "Failed to open https://invalid.example.com/audio.mp3.".into(),
            "[ytdl_hook] ERROR: [youtube] Video unavailable".into(),
            "Exiting... (Errors when loading file)".into(),
        ];
        let err = extract_mpv_error(&lines, None);
        assert_eq!(err, "[youtube] Video unavailable");

        let lines_media = vec![
            "Failed to recognize file format.".into(),
            "Exiting... (Errors when loading file)".into(),
        ];
        let err_media = extract_mpv_error(&lines_media, None);
        assert_eq!(err_media, "Failed to recognize file format.");
    }

    #[test]
    fn test_ipc_json_parsing_with_request_ids() {
        let time_reply = r#"{"data":14.5,"request_id":2,"error":"success"}"#;
        let val: serde_json::Value = serde_json::from_str(time_reply).unwrap();
        assert_eq!(
            val.get("request_id").and_then(|id| id.as_u64()),
            Some(REQ_ID_TIME_POS)
        );
        assert_eq!(val.get("data").and_then(|d| d.as_f64()), Some(14.5));

        let end_file_error =
            r#"{"event":"end-file","reason":"error","file_error":"Connection refused"}"#;
        let val_err: serde_json::Value = serde_json::from_str(end_file_error).unwrap();
        assert_eq!(
            val_err.get("event").and_then(|e| e.as_str()),
            Some("end-file")
        );
        assert_eq!(
            val_err.get("reason").and_then(|r| r.as_str()),
            Some("error")
        );
        assert_eq!(
            val_err.get("file_error").and_then(|f| f.as_str()),
            Some("Connection refused")
        );

        let end_file_eof = r#"{"event":"end-file","reason":"eof"}"#;
        let val_eof: serde_json::Value = serde_json::from_str(end_file_eof).unwrap();
        assert_eq!(
            val_eof.get("event").and_then(|e| e.as_str()),
            Some("end-file")
        );
        assert_eq!(val_eof.get("reason").and_then(|r| r.as_str()), Some("eof"));

        let end_file_stop = r#"{"event":"end-file","reason":"stop"}"#;
        let val_stop: serde_json::Value = serde_json::from_str(end_file_stop).unwrap();
        assert_eq!(
            val_stop.get("reason").and_then(|r| r.as_str()),
            Some("stop")
        );
    }

    #[test]
    fn test_stop_normal_shutdown_when_process_exits_gracefully() {
        let child = Command::new("true").spawn().expect("spawn true");
        let socket_path =
            std::env::temp_dir().join(format!("ansimuz-test-normal-{}.sock", std::process::id()));
        fs::write(&socket_path, b"test").expect("write socket");
        assert!(socket_path.exists());

        let cancel = Arc::new(AtomicBool::new(false));
        let mut player = AudioPlayer::new();
        player.set_child_for_test(child, socket_path.clone(), cancel.clone());

        player.stop();

        assert!(
            cancel.load(Ordering::SeqCst),
            "Observer cancellation must be set on stop"
        );
        assert!(player.child.is_none(), "Child must be cleared on stop");
        assert!(
            !socket_path.exists(),
            "Socket file must be removed after exit"
        );
    }

    #[test]
    fn test_stop_forced_shutdown_kills_unresponsive_process() {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let socket_path =
            std::env::temp_dir().join(format!("ansimuz-test-forced-{}.sock", std::process::id()));
        fs::write(&socket_path, b"test").expect("write socket");
        assert!(socket_path.exists());

        let cancel = Arc::new(AtomicBool::new(false));
        let mut player = AudioPlayer::new();
        player.set_child_for_test(child, socket_path.clone(), cancel.clone());

        player.stop();

        assert!(
            cancel.load(Ordering::SeqCst),
            "Observer cancellation must be set on forced stop"
        );
        assert!(
            player.child.is_none(),
            "Child must be cleared on forced stop"
        );
        assert!(
            !socket_path.exists(),
            "Socket file must be removed after forced kill"
        );
    }

    #[test]
    fn test_drop_reaps_child_and_removes_socket() {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let socket_path =
            std::env::temp_dir().join(format!("ansimuz-test-drop-{}.sock", std::process::id()));
        fs::write(&socket_path, b"test").expect("write socket");
        assert!(socket_path.exists());

        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut player = AudioPlayer::new();
            player.set_child_for_test(child, socket_path.clone(), cancel.clone());
            // player dropped here
        }

        assert!(
            cancel.load(Ordering::SeqCst),
            "Observer cancellation must be set on drop"
        );
        assert!(
            !socket_path.exists(),
            "Socket file must be removed after drop"
        );
    }
    #[test]
    fn test_prefetch_result_already_cached() {
        let test_url = "https://example.com/already_cached_track";
        let cache_dir = audio_cache_dir();
        fs::create_dir_all(&cache_dir).expect("create cache dir");
        let final_path = cache_dir.join(format!("{}.audio", audio_hash(test_url)));
        fs::write(&final_path, b"valid audio data").expect("write audio cache");

        let dumping = Arc::new(Mutex::new(HashSet::new()));
        let res = start_dump(test_url, dumping.clone());

        assert_eq!(res, PrefetchResult::AlreadyCached(final_path.clone()));
        assert!(!dumping.lock().unwrap().contains(test_url));

        let _ = fs::remove_file(&final_path);
    }

    #[test]
    fn test_prefetch_duplicate_request_cancelled() {
        let test_url = "https://example.com/duplicate_track";
        let dumping = Arc::new(Mutex::new(HashSet::new()));
        dumping.lock().unwrap().insert(test_url.to_string());

        let res = start_dump(test_url, dumping.clone());
        assert_eq!(res, PrefetchResult::Cancelled);
        // The URL was originally in dumping, guard does not remove it because it returned before guard created
        assert!(dumping.lock().unwrap().contains(test_url));
    }

    #[test]
    fn test_prefetch_empty_url_fails() {
        let dumping = Arc::new(Mutex::new(HashSet::new()));
        let res = start_dump("   ", dumping);
        assert_eq!(res, PrefetchResult::Failed("Empty URL".into()));
    }

    #[test]
    fn test_prefetch_resolution_failure_cleans_up_part_and_dumping() {
        let test_url = "https://invalid.domain.that.does.not.exist/audio_fail";
        let dumping = Arc::new(Mutex::new(HashSet::new()));
        let part = audio_cache_dir().join(format!("{}.part", audio_hash(test_url)));
        let _ = fs::remove_file(&part);

        let res = start_dump(test_url, dumping.clone());
        match res {
            PrefetchResult::Failed(err) => {
                assert!(
                    err.contains("resolution failed") || err.contains("yt-dlp"),
                    "Expected resolution failure message, got: {err}"
                );
            }
            other => panic!("Expected PrefetchResult::Failed, got {other:?}"),
        }

        assert!(!part.exists(), ".part file must not remain on failure");
        assert!(
            !dumping.lock().unwrap().contains(test_url),
            "URL must be removed from dumping set after failure"
        );
    }
    #[test]
    fn test_prefetch_cancellation_aborts_immediately() {
        let test_url = "https://example.com/cancelled_prefetch_track";
        let dumping = Arc::new(Mutex::new(HashSet::new()));
        let cancel = Arc::new(AtomicBool::new(true));

        let res = start_dump_cancellable(test_url, dumping.clone(), Some(cancel));
        assert_eq!(res, PrefetchResult::Cancelled);
        assert!(!dumping.lock().unwrap().contains(test_url));
    }

    #[test]
    fn test_player_stop_signals_prefetch_cancellation() {
        let mut player = AudioPlayer::new();
        assert!(!player.cancel_prefetch.load(Ordering::Relaxed));

        player.stop();
        assert!(player.cancel_prefetch.load(Ordering::Relaxed));
    }
}
