use super::manager::ArtworkManager;
use super::types::{ArtworkError, ArtworkRequest, ArtworkResult};
use crate::config::DiscordArtworkConfig;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

/// Commands received by the dedicated artwork worker thread.
#[derive(Debug)]
pub enum ArtworkCommand {
    Resolve {
        generation: u64,
        request: ArtworkRequest,
    },
    Refresh {
        generation: u64,
        request: ArtworkRequest,
    },
    Shutdown,
}

/// Events emitted by the dedicated artwork worker thread upon completion.
#[derive(Debug)]
pub enum ArtworkEvent {
    Resolved {
        generation: u64,
        result: Result<ArtworkResult, ArtworkError>,
    },
}

/// Dedicated artwork worker keeping network operations off the playback path.
pub struct ArtworkWorker {
    sender: Sender<ArtworkCommand>,
    thread_handle: Option<JoinHandle<()>>,
    forwarder_handle: Option<JoinHandle<()>>,
}

impl ArtworkWorker {
    /// Spawns the dedicated artwork worker background thread with default settings.
    pub fn spawn(event_sender: Sender<ArtworkEvent>) -> std::io::Result<Self> {
        Self::spawn_with_config(event_sender, DiscordArtworkConfig::default())
    }

    /// Spawns the dedicated artwork worker background thread with custom configuration.
    pub fn spawn_with_config(
        event_sender: Sender<ArtworkEvent>,
        config: DiscordArtworkConfig,
    ) -> std::io::Result<Self> {
        let manager = ArtworkManager::with_config(None, config).map_err(std::io::Error::other)?;
        let (cmd_tx, cmd_rx) = channel();

        let thread_handle = thread::Builder::new()
            .name("discord-artwork-worker".to_string())
            .spawn(move || {
                run_artwork_worker(cmd_rx, event_sender, manager);
            })?;

        Ok(Self {
            sender: cmd_tx,
            thread_handle: Some(thread_handle),
            forwarder_handle: None,
        })
    }

    /// Spawns the artwork worker together with a managed event-forwarding thread.
    ///
    /// Both thread handles are retained and explicitly joined in order during [`Self::shutdown`].
    pub fn spawn_with_forwarder<F>(
        config: DiscordArtworkConfig,
        mut forward: F,
    ) -> std::io::Result<Self>
    where
        F: FnMut(ArtworkEvent) + Send + 'static,
    {
        let manager = ArtworkManager::with_config(None, config).map_err(std::io::Error::other)?;
        let (cmd_tx, cmd_rx) = channel();
        let (event_tx, event_rx) = channel();

        let thread_handle = thread::Builder::new()
            .name("discord-artwork-worker".to_string())
            .spawn(move || {
                run_artwork_worker(cmd_rx, event_tx, manager);
            })?;

        let forwarder_handle = match thread::Builder::new()
            .name("discord-artwork-forwarder".to_string())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    forward(event);
                }
            }) {
            Ok(handle) => Some(handle),
            Err(err) => {
                let _ = cmd_tx.send(ArtworkCommand::Shutdown);
                let _ = thread_handle.join();
                return Err(err);
            }
        };

        Ok(Self {
            sender: cmd_tx,
            thread_handle: Some(thread_handle),
            forwarder_handle,
        })
    }

    /// Dispatches an asynchronous artwork resolution request.
    pub fn resolve(&self, generation: u64, request: ArtworkRequest) {
        let _ = self.sender.send(ArtworkCommand::Resolve {
            generation,
            request,
        });
    }

    /// Invalidates any existing cache mapping and freshly resolves artwork.
    pub fn refresh(&self, generation: u64, request: ArtworkRequest) {
        let _ = self.sender.send(ArtworkCommand::Refresh {
            generation,
            request,
        });
    }

    /// Shuts down the worker thread (and managed forwarder thread, if any) and joins all handles.
    pub fn shutdown(&mut self) {
        let _ = self.sender.send(ArtworkCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        // Once run_artwork_worker exits, event_tx is dropped, closing event_rx cleanly.
        if let Some(forwarder) = self.forwarder_handle.take() {
            let _ = forwarder.join();
        }
    }
}

impl Drop for ArtworkWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_artwork_worker(
    receiver: Receiver<ArtworkCommand>,
    sender: Sender<ArtworkEvent>,
    mut manager: ArtworkManager,
) {
    while let Ok(cmd) = receiver.recv() {
        match cmd {
            ArtworkCommand::Resolve {
                generation,
                request,
            } => {
                let result = manager.resolve(request);
                let _ = sender.send(ArtworkEvent::Resolved { generation, result });
            }
            ArtworkCommand::Refresh {
                generation,
                request,
            } => {
                let result = manager.refresh(request);
                let _ = sender.send(ArtworkEvent::Resolved { generation, result });
            }
            ArtworkCommand::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn worker_with_forwarder_joins_cleanly_on_shutdown() {
        let (tx, rx) = channel();
        let mut worker =
            ArtworkWorker::spawn_with_forwarder(DiscordArtworkConfig::default(), move |event| {
                match event {
                    ArtworkEvent::Resolved { generation, result } => {
                        let _ = tx.send((generation, result));
                    }
                }
            })
            .expect("spawn artwork worker with forwarder");

        // Trigger an immediate local error (non-HTTPS scheme) to exercise full roundtrip
        worker.resolve(
            42,
            ArtworkRequest::new("ytmusic", Some("t1".into()), "http://invalid.local/art.jpg"),
        );

        let (gen_id, result) = rx.recv().expect("forwarded event must arrive");
        assert_eq!(gen_id, 42);
        assert!(result.is_err());

        assert!(worker.thread_handle.is_some());
        assert!(worker.forwarder_handle.is_some());

        worker.shutdown();

        assert!(worker.thread_handle.is_none());
        assert!(worker.forwarder_handle.is_none());
    }
}
