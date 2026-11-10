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
        let (cmd_tx, cmd_rx) = channel();

        let thread_handle = thread::Builder::new()
            .name("discord-artwork-worker".to_string())
            .spawn(move || {
                run_artwork_worker(cmd_rx, event_sender, config);
            })?;

        Ok(Self {
            sender: cmd_tx,
            thread_handle: Some(thread_handle),
        })
    }

    /// Dispatches an asynchronous artwork resolution request.
    pub fn resolve(&self, generation: u64, request: ArtworkRequest) {
        let _ = self.sender.send(ArtworkCommand::Resolve {
            generation,
            request,
        });
    }

    /// Shuts down the worker thread and joins its handle.
    pub fn shutdown(&mut self) {
        let _ = self.sender.send(ArtworkCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
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
    config: DiscordArtworkConfig,
) {
    let mut manager = ArtworkManager::with_config(None, config);

    while let Ok(cmd) = receiver.recv() {
        match cmd {
            ArtworkCommand::Resolve { generation, request } => {
                let result = manager.resolve(request);
                let _ = sender.send(ArtworkEvent::Resolved { generation, result });
            }
            ArtworkCommand::Shutdown => break,
        }
    }
}

