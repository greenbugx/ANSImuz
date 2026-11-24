use std::sync::mpsc::Sender;
use super::events::PlayerEvent;
use super::mpv::PrefetchResult;
use super::native_adapter::Translator;
use super::native_mpv::NativeMpv;

// Native libmpv backend implementing the playback interface
pub struct NativeBackend {
    mpv: Option<NativeMpv>,
    translator: Translator,
    playing: bool,
    generation: u64,
    last_error: Option<String>,
    events: Option<Sender<PlayerEvent>>,
    is_mock: bool,
    mock_playing: bool,
    mock_fail: bool,
}

impl NativeBackend {
    pub fn new() -> Result<Self, String> {
        let mpv = NativeMpv::new().map_err(|e| format!("Failed to initialize native mpv: {e}"))?;
        Ok(Self {
            mpv: Some(mpv),
            translator: Translator::new(),
            playing: false,
            generation: 0,
            last_error: None,
            events: None,
            is_mock: false,
            mock_playing: false,
            mock_fail: false,
        })
    }

    pub fn new_mock() -> Self {
        Self {
            mpv: None,
            translator: Translator::new(),
            playing: false,
            generation: 0,
            last_error: None,
            events: None,
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
            return self.mock_playing && !self.translator.is_paused();
        }
        self.playing && !self.translator.is_paused()
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
            self.generation += 1;
            if self.mock_fail {
                self.mock_playing = false;
                self.last_error = Some("Failed to launch mpv: process failed to start".into());
                return Err("Failed to launch mpv: process failed to start".into());
            }
            self.mock_playing = true;
            self.events = Some(events);
            return Ok(());
        }

        self.stop();
        self.last_error = None;
        self.generation += 1;
        let generation = self.generation;

        if let Some(ref mut mpv) = self.mpv {
            mpv.set_volume(volume)?;
            let load_id = mpv.loadfile(url)?;
            self.translator.begin(load_id, generation);
            self.playing = true;
            self.events = Some(events);
            Ok(())
        } else {
            Err("Native player instance not initialized".into())
        }
    }

    pub fn poll_events(&mut self) -> Result<Vec<PlayerEvent>, String> {
        let mut emitted = Vec::new();
        if let Some(ref mut mpv) = self.mpv {
            loop {
                match mpv.poll_event() {
                    Ok(Some(ev)) => {
                        let desc = |code| mpv.error_message(code);
                        if let Some(player_event) = self.translator.translate(ev, &desc) {
                            match &player_event {
                                PlayerEvent::Ended { .. } => {
                                    self.playing = false;
                                }
                                PlayerEvent::Failed { message, .. } => {
                                    self.playing = false;
                                    self.last_error = Some(message.clone());
                                }
                                _ => {}
                            }
                            if let Some(ref tx) = self.events {
                                let _ = tx.send(player_event.clone());
                            }
                            emitted.push(player_event);
                        }
                    }
                    Ok(None) => break,
                    Err(err) => {
                        self.last_error = Some(err.clone());
                        break;
                    }
                }
            }
        }
        Ok(emitted)
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        if self.is_mock {
            return Ok(());
        }
        if let Some(ref mut mpv) = self.mpv {
            let target = !self.translator.is_paused();
            mpv.set_pause(target)?;
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
            return;
        }
        if let Some(ref mut mpv) = self.mpv {
            let _ = mpv.stop();
        }
        self.playing = false;
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
