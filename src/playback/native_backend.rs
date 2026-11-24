use std::sync::mpsc::Sender;
use super::events::PlayerEvent;
use super::mpv::PrefetchResult;
use super::native_mpv::NativeMpv;

// Native libmpv backend implementing the playback interface
pub struct NativeBackend {
    mpv: Option<NativeMpv>,
    paused: bool,
    playing: bool,
    generation: u64,
    last_error: Option<String>,
    is_mock: bool,
    mock_playing: bool,
    mock_fail: bool,
}

impl NativeBackend {
    pub fn new() -> Result<Self, String> {
        let mpv = NativeMpv::new().map_err(|e| format!("Failed to initialize native mpv: {e}"))?;
        Ok(Self {
            mpv: Some(mpv),
            paused: false,
            playing: false,
            generation: 0,
            last_error: None,
            is_mock: false,
            mock_playing: false,
            mock_fail: false,
        })
    }

    pub fn new_mock() -> Self {
        Self {
            mpv: None,
            paused: false,
            playing: false,
            generation: 0,
            last_error: None,
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

    pub fn prefetch_later(&self, _url: &str, _delay_secs: u64) {
        // Prefetching stream caching is delegated to the shared cache
    }

    pub fn prefetch_result(&self, _url: &str) -> Option<PrefetchResult> {
        None
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_playing(&mut self) -> bool {
        if self.is_mock {
            return self.mock_playing && !self.paused;
        }
        self.playing && !self.paused
    }

    pub fn error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn play(
        &mut self,
        url: &str,
        volume: u8,
        _events: Sender<PlayerEvent>,
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

        if let Some(ref mut mpv) = self.mpv {
            mpv.set_volume(volume)?;
            mpv.loadfile(url)?;
            self.playing = true;
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        if self.is_mock {
            self.paused = !self.paused;
            return Ok(());
        }
        if let Some(ref mut mpv) = self.mpv {
            let target = !self.paused;
            mpv.set_pause(target)?;
            self.paused = target;
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        if self.is_mock {
            return Ok(());
        }
        if let Some(ref mut mpv) = self.mpv {
            mpv.set_volume(volume)?;
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn stop(&mut self) {
        if self.is_mock {
            self.mock_playing = false;
            self.paused = false;
            return;
        }
        if let Some(ref mut mpv) = self.mpv {
            let _ = mpv.stop();
        }
        self.playing = false;
        self.paused = false;
        self.generation += 1;
    }
}

impl Default for NativeBackend {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self::new_mock())
    }
}

impl Drop for NativeBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

