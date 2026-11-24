use std::path::PathBuf;
use std::sync::mpsc::Sender;
use super::events::PlayerEvent;
use super::mpv::{CliPlayer, PrefetchResult};
use super::native_backend::NativeBackend;

pub enum Inner {
    Cli(CliPlayer),
    Native(NativeBackend),
}

// Facade presenting a unified playback API over CLI and native backends
pub struct AudioPlayer {
    inner: Inner,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            inner: Inner::Cli(CliPlayer::new()),
        }
    }

    pub fn new_mock() -> Self {
        Self {
            inner: Inner::Cli(CliPlayer::new_mock()),
        }
    }

    pub fn new_native() -> Result<Self, String> {
        Ok(Self {
            inner: Inner::Native(NativeBackend::new()?),
        })
    }

    pub fn set_mock_fail(&mut self, fail: bool) {
        match &mut self.inner {
            Inner::Cli(p) => p.set_mock_fail(fail),
            Inner::Native(p) => p.set_mock_fail(fail),
        }
    }

    pub fn record_error(&mut self, error: String) {
        match &mut self.inner {
            Inner::Cli(p) => p.record_error(error),
            Inner::Native(p) => p.record_error(error),
        }
    }

    pub fn clear_error(&mut self) {
        match &mut self.inner {
            Inner::Cli(p) => p.clear_error(),
            Inner::Native(p) => p.clear_error(),
        }
    }

    pub fn cached_audio(url: &str) -> Option<PathBuf> {
        CliPlayer::cached_audio(url)
    }

    pub fn prefetch_later(&self, url: &str, delay_secs: u64) {
        match &self.inner {
            Inner::Cli(p) => p.prefetch_later(url, delay_secs),
            Inner::Native(p) => p.prefetch_later(url, delay_secs),
        }
    }

    pub fn prefetch_result(&self, url: &str) -> Option<PrefetchResult> {
        match &self.inner {
            Inner::Cli(p) => p.prefetch_result(url),
            Inner::Native(p) => p.prefetch_result(url),
        }
    }

    pub fn generation(&self) -> u64 {
        match &self.inner {
            Inner::Cli(p) => p.generation(),
            Inner::Native(p) => p.generation(),
        }
    }

    pub fn is_playing(&mut self) -> bool {
        match &mut self.inner {
            Inner::Cli(p) => p.is_playing(),
            Inner::Native(p) => p.is_playing(),
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.inner {
            Inner::Cli(p) => p.error(),
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
            Inner::Native(p) => p.play(url, volume, events),
        }
    }

    pub fn poll_events(&mut self) -> Result<Vec<PlayerEvent>, String> {
        match &mut self.inner {
            Inner::Cli(_) => Ok(Vec::new()),
            Inner::Native(p) => p.poll_events(),
        }
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        match &mut self.inner {
            Inner::Cli(p) => p.toggle_pause(),
            Inner::Native(p) => p.toggle_pause(),
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        match &mut self.inner {
            Inner::Cli(p) => p.set_volume(volume),
            Inner::Native(p) => p.set_volume(volume),
        }
    }

    pub fn stop(&mut self) {
        match &mut self.inner {
            Inner::Cli(p) => p.stop(),
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
}
