use super::manager::ArtworkManager;
use super::types::{ArtworkError, ArtworkRequest, ArtworkResult};
use crate::config::DiscordArtworkConfig;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    shutdown_flag: Arc<AtomicBool>,
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
        let mut manager =
            ArtworkManager::with_config(None, config).map_err(std::io::Error::other)?;
        Self::spawn_with_handler(event_sender, move |request, is_refresh| {
            if is_refresh {
                manager.refresh(request)
            } else {
                manager.resolve(request)
            }
        })
    }

    /// Spawns the artwork worker together with a managed event-forwarding thread.
    ///
    /// Both thread handles are retained and explicitly joined in order during [`Self::shutdown`].
    pub fn spawn_with_forwarder<F>(
        config: DiscordArtworkConfig,
        forward: F,
    ) -> std::io::Result<Self>
    where
        F: FnMut(ArtworkEvent) + Send + 'static,
    {
        let mut manager =
            ArtworkManager::with_config(None, config).map_err(std::io::Error::other)?;
        Self::spawn_with_handler_and_forwarder(
            move |request, is_refresh| {
                if is_refresh {
                    manager.refresh(request)
                } else {
                    manager.resolve(request)
                }
            },
            forward,
        )
    }

    fn spawn_with_handler<H>(
        event_sender: Sender<ArtworkEvent>,
        handler: H,
    ) -> std::io::Result<Self>
    where
        H: FnMut(ArtworkRequest, bool) -> Result<ArtworkResult, ArtworkError> + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = channel();
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown_flag);

        let thread_handle = thread::Builder::new()
            .name("discord-artwork-worker".to_string())
            .spawn(move || {
                run_artwork_worker(cmd_rx, event_sender, worker_shutdown, handler);
            })?;

        Ok(Self {
            sender: cmd_tx,
            shutdown_flag,
            thread_handle: Some(thread_handle),
            forwarder_handle: None,
        })
    }

    fn spawn_with_handler_and_forwarder<H, F>(handler: H, mut forward: F) -> std::io::Result<Self>
    where
        H: FnMut(ArtworkRequest, bool) -> Result<ArtworkResult, ArtworkError> + Send + 'static,
        F: FnMut(ArtworkEvent) + Send + 'static,
    {
        let (event_tx, event_rx) = channel();
        let mut worker = Self::spawn_with_handler(event_tx, handler)?;

        let forwarder_handle = match thread::Builder::new()
            .name("discord-artwork-forwarder".to_string())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    forward(event);
                }
            }) {
            Ok(handle) => Some(handle),
            Err(err) => {
                worker.shutdown();
                return Err(err);
            }
        };

        worker.forwarder_handle = forwarder_handle;
        Ok(worker)
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

    /// Shuts down in strict order and is idempotent across explicit calls and `Drop`:
    /// 1. Signal shutdown and send `Shutdown` to the artwork worker.
    /// 2. Join the artwork worker thread (once returned, its `event_tx` clone is dropped).
    /// 3. For managed forwarders, the dropped `event_tx` closes the event channel.
    /// 4. Join the forwarder thread.
    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        let _ = self.sender.send(ArtworkCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        // Only joined once via `.take()`, run_artwork_worker has already dropped its event sender.
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

fn run_artwork_worker<H>(
    receiver: Receiver<ArtworkCommand>,
    sender: Sender<ArtworkEvent>,
    shutdown_flag: Arc<AtomicBool>,
    mut handler: H,
) where
    H: FnMut(ArtworkRequest, bool) -> Result<ArtworkResult, ArtworkError>,
{
    while let Ok(cmd) = receiver.recv() {
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }
        match cmd {
            ArtworkCommand::Resolve {
                generation,
                request,
            } => {
                let result = handler(request, false);
                let _ = sender.send(ArtworkEvent::Resolved { generation, result });
            }
            ArtworkCommand::Refresh {
                generation,
                request,
            } => {
                let result = handler(request, true);
                let _ = sender.send(ArtworkEvent::Resolved { generation, result });
            }
            ArtworkCommand::Shutdown => break,
        }
    }
    // Explicitly close this worker's event channel sender before the thread finishes.
    drop(sender);
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

    #[test]
    fn direct_spawn_shutdown_skips_queued_work_and_drop_is_idempotent() {
        let (tx, rx) = channel();
        let mut worker = ArtworkWorker::spawn(tx).expect("spawn artwork worker");

        worker.shutdown();
        assert!(worker.thread_handle.is_none());
        assert!(worker.forwarder_handle.is_none());

        worker.resolve(
            99,
            ArtworkRequest::new(
                "ytmusic",
                Some("t_skip".into()),
                "http://invalid.local/art.jpg",
            ),
        );
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(50))
                .is_err(),
            "requests queued after shutdown must not emit events"
        );

        // Second shutdown and Drop must be no-ops without double-join
        worker.shutdown();
        drop(worker);
    }

    #[test]
    fn shutdown_during_inflight_upload_waits_for_active_skips_queued_and_joins_forwarder_in_order()
    {
        use std::path::PathBuf;
        use std::sync::atomic::AtomicUsize;
        use std::time::Duration;

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let executed_count = Arc::new(AtomicUsize::new(0));
        let executed_in_worker = Arc::clone(&executed_count);
        let (forwarded_tx, forwarded_rx) = channel();

        let mut worker = ArtworkWorker::spawn_with_handler_and_forwarder(
            move |req, _is_refresh| {
                let idx = executed_in_worker.fetch_add(1, Ordering::SeqCst);
                if idx == 0 {
                    let _ = started_tx.send(());
                    let _ = release_rx.recv();
                }
                Ok(ArtworkResult {
                    source_url: req.source_url,
                    hosted_url: "https://files.catbox.moe/active.jpg".to_string(),
                    local_path: PathBuf::from("images/active.jpg"),
                    content_hash: "hash_active".to_string(),
                })
            },
            move |event| match event {
                ArtworkEvent::Resolved { generation, result } => {
                    let _ = forwarded_tx.send((generation, result));
                }
            },
        )
        .expect("spawn artwork worker with forwarder");

        // Request 1 enters simulated blocking upload
        worker.resolve(
            1,
            ArtworkRequest::new("spotify", Some("t1".into()), "https://example.com/1.jpg"),
        );
        started_rx.recv().expect("first upload must start");

        // Queue requests 2 & 3 behind the active upload
        worker.resolve(
            2,
            ArtworkRequest::new("spotify", Some("t2".into()), "https://example.com/2.jpg"),
        );
        worker.resolve(
            3,
            ArtworkRequest::new("spotify", Some("t3".into()), "https://example.com/3.jpg"),
        );

        let (shutdown_done_tx, shutdown_done_rx) = channel();
        let shutdown_thread = thread::spawn(move || {
            worker.shutdown();
            assert!(worker.thread_handle.is_none());
            assert!(worker.forwarder_handle.is_none());
            drop(worker); // Drop after explicit shutdown must remain a safe no-op
            let _ = shutdown_done_tx.send(());
        });

        // While the active upload is still blocked, shutdown() must wait on worker thread join
        assert!(
            shutdown_done_rx
                .recv_timeout(Duration::from_millis(60))
                .is_err(),
            "shutdown must wait for the in-flight upload rather than detaching threads"
        );

        // Unblock the active upload -> worker completes request 1, skips 2 & 3, closes channel, forwarder finishes & joins
        release_tx.send(()).expect("release in-flight upload");
        shutdown_done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("shutdown should complete once active upload finishes");
        shutdown_thread.join().expect("join test shutdown thread");

        let (gen1, res1) = forwarded_rx
            .recv_timeout(Duration::from_millis(100))
            .expect("in-flight upload event must be forwarded before forwarder joins");
        assert_eq!(gen1, 1);
        assert!(res1.is_ok());

        // Forwarder channel must now be disconnected with no queued requests executed
        assert_eq!(executed_count.load(Ordering::SeqCst), 1);
        assert!(matches!(
            forwarded_rx.recv_timeout(Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}
