use super::events::PlayerEvent;
use super::mpv::{CliPlayer, PrefetchResult};
#[cfg(feature = "native-mpv")]
use super::native_backend::NativeBackend;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub enum Inner {
    Cli(CliPlayer),
    #[cfg(feature = "native-mpv")]
    Native(NativeBackend),
}

// Appends timestamped playback log messages to cache storage
pub(crate) fn log_playback(msg: &str) {
    use std::io::Write as _;
    let log_dir = crate::config::cache_base_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("playback.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "[{secs}] {msg}");
    }
}

// Facade presenting a unified playback API over CLI and native backends
pub struct AudioPlayer {
    inner: Inner,
    startup_message: Option<String>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            inner: Inner::Cli(CliPlayer::new()),
            startup_message: None,
        }
    }

    pub fn new_mock() -> Self {
        Self {
            inner: Inner::Cli(CliPlayer::new_mock()),
            startup_message: None,
        }
    }

    #[cfg(feature = "native-mpv")]
    pub fn new_native() -> Result<Self, String> {
        Ok(Self {
            inner: Inner::Native(NativeBackend::new()?),
            startup_message: None,
        })
    }

    #[cfg(not(feature = "native-mpv"))]
    pub fn new_native() -> Result<Self, String> {
        Err("Native mpv support is not compiled in this build".to_string())
    }

    #[cfg(feature = "native-mpv")]
    pub fn from_native(native: NativeBackend) -> Self {
        Self {
            inner: Inner::Native(native),
            startup_message: None,
        }
    }

    pub fn from_backend(backend: crate::config::Backend) -> Self {
        match backend {
            crate::config::Backend::Cli => {
                log_playback("Active playback backend: CLI");
                Self::new()
            }
            crate::config::Backend::Native => {
                #[cfg(feature = "native-mpv")]
                {
                    match NativeBackend::new() {
                        Ok(native) => {
                            log_playback("Active playback backend: native libmpv");
                            Self::from_native(native)
                        }
                        Err(e) => {
                            let msg = format!(
                                "Native mpv initialization failed ({e}), falling back to CLI"
                            );
                            log_playback(&msg);
                            let mut player = Self::new();
                            player.startup_message = Some(msg);
                            player
                        }
                    }
                }
                #[cfg(not(feature = "native-mpv"))]
                {
                    let msg = "Native backend requested but binary compiled without native-mpv feature, falling back to CLI".to_string();
                    log_playback(&msg);
                    let mut player = Self::new();
                    player.startup_message = Some(msg);
                    player
                }
            }
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match &self.inner {
            Inner::Cli(_) => "cli",
            #[cfg(feature = "native-mpv")]
            Inner::Native(_) => "native",
        }
    }

    pub fn startup_message(&self) -> Option<&str> {
        self.startup_message.as_deref()
    }

    pub fn is_mock(&self) -> bool {
        match &self.inner {
            Inner::Cli(p) => p.is_mock(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.is_mock(),
        }
    }

    pub fn set_mock_fail(&mut self, fail: bool) {
        match &mut self.inner {
            Inner::Cli(p) => p.set_mock_fail(fail),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.set_mock_fail(fail),
        }
    }

    pub fn record_error(&mut self, error: String) {
        match &mut self.inner {
            Inner::Cli(p) => p.record_error(error),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.record_error(error),
        }
    }

    pub fn clear_error(&mut self) {
        match &mut self.inner {
            Inner::Cli(p) => p.clear_error(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.clear_error(),
        }
    }

    pub fn cached_audio(url: &str) -> Option<PathBuf> {
        CliPlayer::cached_audio(url)
    }

    pub fn prefetch_later(&self, url: &str, delay_secs: u64) {
        match &self.inner {
            Inner::Cli(p) => p.prefetch_later(url, delay_secs),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.prefetch_later(url, delay_secs),
        }
    }

    pub fn prefetch_result(&self, url: &str) -> Option<PrefetchResult> {
        match &self.inner {
            Inner::Cli(p) => p.prefetch_result(url),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.prefetch_result(url),
        }
    }

    pub fn generation(&self) -> u64 {
        match &self.inner {
            Inner::Cli(p) => p.generation(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.generation(),
        }
    }

    pub fn is_playing(&mut self) -> bool {
        match &mut self.inner {
            Inner::Cli(p) => p.is_playing(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.is_playing(),
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.inner {
            Inner::Cli(p) => p.error(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.error(),
        }
    }

    pub fn play(
        &mut self,
        url: &str,
        volume: u8,
        events: Sender<PlayerEvent>,
    ) -> Result<(), String> {
        match &mut self.inner {
            Inner::Cli(p) => p.play(url, volume, events),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.play(url, volume, events),
        }
    }

    pub fn poll_events(&mut self) -> Result<Vec<PlayerEvent>, String> {
        match &mut self.inner {
            Inner::Cli(_) => Ok(Vec::new()),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.poll_events(),
        }
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        match &mut self.inner {
            Inner::Cli(p) => p.toggle_pause(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.toggle_pause(),
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        match &mut self.inner {
            Inner::Cli(p) => p.set_volume(volume),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.set_volume(volume),
        }
    }

    pub fn stop(&mut self) {
        match &mut self.inner {
            Inner::Cli(p) => p.stop(),
            #[cfg(feature = "native-mpv")]
            Inner::Native(p) => p.stop(),
        }
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_facade_mock_lifecycle() {
        let mut player = AudioPlayer::new_mock();
        assert!(!player.is_playing());
        assert_eq!(player.generation(), 0);
        assert_eq!(player.error(), None);

        let (tx, _rx) = std::sync::mpsc::channel();
        let res = player.play("https://example.com/audio.mp3", 80, tx);
        assert!(res.is_ok());
        assert!(player.is_playing());
        assert_eq!(player.generation(), 1);

        player.toggle_pause().expect("toggle pause");
        assert!(!player.is_playing());

        player.toggle_pause().expect("toggle pause resume");
        assert!(player.is_playing());

        player.set_volume(90).expect("set volume");
        player.stop();
        assert!(!player.is_playing());
    }

    #[test]
    fn test_facade_error_recording() {
        let mut player = AudioPlayer::new_mock();
        player.record_error("Sample error".to_string());
        assert_eq!(player.error(), Some("Sample error"));

        player.clear_error();
        assert_eq!(player.error(), None);
    }

    #[test]
    fn test_audio_player_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AudioPlayer>();
    }

    #[test]
    fn test_facade_backend_selection() {
        let player_cli = AudioPlayer::from_backend(crate::config::Backend::Cli);
        assert_eq!(player_cli.backend_name(), "cli");
        assert!(player_cli.startup_message().is_none());
    }

    #[test]
    fn test_facade_fallback_message_when_native_requested() {
        let player = AudioPlayer::from_backend(crate::config::Backend::Native);
        if player.backend_name() == "cli" {
            assert!(player.startup_message().is_some());
        }
    }
}
