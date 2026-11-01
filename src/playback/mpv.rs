use std::{
    collections::HashSet,
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
        let owned = url.to_string();

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(delay_secs));

            let already_dumping = dumping
                .lock()
                .map(|set| set.contains(&owned))
                .unwrap_or(true);

            if already_dumping
                || audio_cache_dir()
                    .join(format!("{}.audio", audio_hash(&owned)))
                    .exists()
            {
                return;
            }

            start_dump(owned, dumping);
        });
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

        if let Some(cancel) = self.cancel_observer.take() {
            cancel.store(true, Ordering::Relaxed);
        }

        if self.child.is_some() {
            let _ = self.send(r#"{"command":["quit"]}"#);

            if let Some(mut child) = self.child.take() {
                for _ in 0..10 {
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        _ => thread::sleep(Duration::from_millis(50)),
                    }
                }

                let _ = child.kill();
                let _ = child.wait();
            }
        }

        self.paused = false;
        let _ = fs::remove_file(&self.socket_path);
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
        let _ = events.send(PlayerEvent::Failed {
            generation,
            message: "mpv did not start".into(),
        });
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
                                let _ = events.send(PlayerEvent::Failed {
                                    generation,
                                    message: msg.to_string(),
                                });
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

fn start_dump(url: String, dumping: Arc<Mutex<HashSet<String>>>) {
    {
        let mut set = match dumping.lock() {
            Ok(set) => set,
            Err(_) => return,
        };

        if set.contains(&url) {
            return;
        }
        set.insert(url.clone());
    }

    let _ = fs::create_dir_all(audio_cache_dir());
    let part = audio_cache_dir().join(format!("{}.part", audio_hash(&url)));
    let final_path = audio_cache_dir().join(format!("{}.audio", audio_hash(&url)));

    thread::spawn(move || {
        let mut saved = false;

        if let Some(media) = resolve_stream(&url) {
            let dump = Command::new("mpv")
                .arg("--no-video")
                .arg("--no-audio")
                .arg("--really-quiet")
                .arg("--ytdl-raw-options-append=extractor-args=youtube:player_client=visionos,default")
                .arg(format!("--stream-dump={}", part.display()))
                .arg(&media)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();

            if let Ok(mut child) = dump {
                for _ in 0..600 {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            saved = status.success()
                                && part
                                    .metadata()
                                    .map(|meta| meta.len() >= MIN_DUMP_BYTES)
                                    .unwrap_or(false);
                            break;
                        }
                        Ok(None) => {
                            if part
                                .metadata()
                                .map(|meta| meta.len() > MAX_DUMP_BYTES)
                                .unwrap_or(false)
                            {
                                let _ = child.kill();
                                let _ = child.wait();
                                break;
                            }
                        }
                        Err(_) => {
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
                }
            }
        }

        if saved && part.exists() {
            let _ = fs::rename(&part, &final_path);
        } else {
            let _ = fs::remove_file(&part);
        }

        if let Ok(mut set) = dumping.lock() {
            set.remove(&url);
        }
    });
}

fn resolve_stream(url: &str) -> Option<String> {
    let output = Command::new("yt-dlp")
        .arg("--extractor-args")
        .arg("youtube:player_client=visionos,default")
        .arg("-f")
        .arg("bestaudio")
        .arg("--get-url")
        .arg(url)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let media = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();

    (!media.is_empty()).then_some(media)
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
    }
}
