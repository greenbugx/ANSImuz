use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::Sender,
    thread,
    time::Duration,
};

pub enum PlayerEvent {
    Position {
        generation: u64,
        elapsed: f64,
        duration: Option<f64>,
    },
    Failed {
        generation: u64,
        message: String,
    },
}

pub struct AudioPlayer {
    child: Option<Child>,
    socket_path: PathBuf,
    paused: bool,
    last_error: Option<String>,
    generation: u64,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let socket_path = std::env::temp_dir()
            .join(format!("ansimuz-mpv-{}.sock", std::process::id()));

        Self {
            child: None,
            socket_path,
            paused: false,
            last_error: None,
            generation: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_playing(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => self.child = None,
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
        self.stop();

        self.last_error = None;
        self.paused = false;
        self.generation += 1;
        let generation = self.generation;

        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path)
                .map_err(|e| format!("Cannot clear MPV socket: {e}"))?;
        }

        let child = Command::new("mpv")
            .arg("--no-video")
            .arg("--idle=no")
            .arg("--force-window=no")
            .arg(format!("--input-ipc-server={}", self.socket_path.display()))
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to launch mpv: {e}. Make sure mpv is installed!"))?;

        self.child = Some(child);

        let socket_path = self.socket_path.clone();

        thread::spawn(move || observe(generation, socket_path, volume, events));

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
        self.send(r#"{"command":["cycle","pause"]}"#)?;
        self.paused = !self.paused;
        Ok(())
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        self.send(&format!(
            r#"{{"command":["set_property","volume",{}]}}"#,
            volume.min(100)
        ))?;
        Ok(())
    }

    pub fn stop(&mut self) {
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

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn observe(generation: u64, socket_path: PathBuf, volume: u8, events: Sender<PlayerEvent>) {
    for _ in 0..80 {
        if socket_path.exists() {
            break;
        }

        thread::sleep(Duration::from_millis(25));
    }

    if !socket_path.exists() {
        let _ = events.send(PlayerEvent::Failed {
            generation,
            message: "mpv did not start".into(),
        });
        return;
    }

    if let Ok(mut stream) = UnixStream::connect(&socket_path) {
        let _ = stream.write_all(
            format!(
                r#"{{"command":["set_property","volume",{}]}}"#,
                volume.min(100)
            )
            .as_bytes(),
        );
        let _ = stream.write_all(b"\n");
    }

    let mut duration: Option<f64> = None;

    loop {
        thread::sleep(Duration::from_millis(250));

        let Ok(mut stream) = UnixStream::connect(&socket_path) else {
            break;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));

        if stream
            .write_all(br#"{"command":["get_property","time-pos"]}"#)
            .and_then(|_| stream.write_all(b"\n"))
            .is_err()
        {
            break;
        }

        let elapsed = read_reply(&mut stream);

        if duration.is_none() {
            let _ = stream.write_all(br#"{"command":["get_property","duration"]}"#);
            let _ = stream.write_all(b"\n");
            duration = read_reply(&mut stream);
        }

        if let Some(elapsed) = elapsed {
            let _ = events.send(PlayerEvent::Position {
                generation,
                elapsed,
                duration,
            });
        }
    }
}

fn read_reply(stream: &mut UnixStream) -> Option<f64> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    for _ in 0..8 {
        line.clear();

        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {
                if line.contains("\"error\"") {
                    return json_number(&line, "data");
                }
            }
        }
    }

    None
}

fn json_number(line: &str, key: &str) -> Option<f64> {
    let key = format!("\"{key}\":");
    let start = line.find(&key)? + key.len();
    let rest = line[start..].trim_start();
    let end = rest
        .find(|c: char| !matches!(c, '0'..='9' | '.' | '-' | '+' | 'e' | 'E'))
        .unwrap_or(rest.len());

    rest[..end].parse().ok()
}
