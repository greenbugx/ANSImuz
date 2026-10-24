use std::process::{Child, Command, Stdio};

pub struct AudioPlayer {
    current_process: Option<Child>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self { current_process: None }
    }

    /// spawns a headless mpv instance to stream the audio URL asynchronously
    pub fn play(&mut self, url: &str) {
        // kill the currently playing track
        self.stop();

        // spawn mpv in background
        let child = Command::new("mpv")
            .arg("--no-video") // disable video playback
            .arg("--ao=pulse")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match child {
            Ok(c) => self.current_process = Some(c),
            Err(e) => eprintln!("[Player Error] Failed to launch mpv: {e}. Make sure mpv is installed!"),
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.current_process.take() {
            let _ = child.kill();
            let _ = child.wait(); // prevent zombie processes
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}