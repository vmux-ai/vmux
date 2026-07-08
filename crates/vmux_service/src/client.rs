use crate::protocol::{ClientMessage, ServiceMessage};
use crate::{read_message, socket_path, write_message};
use bevy::ecs::resource::Resource;
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::Mutex;

#[derive(Resource)]
pub struct ServiceClient(pub ServiceHandle);

const MAX_SERVICE_MESSAGES_PER_DRAIN: usize = 128;

/// Async client connection to the vmux service.
/// Wraps the Unix socket with framing/serialization.
pub struct ServiceConnection {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
}

impl ServiceConnection {
    /// Connect to the service socket.
    pub async fn connect() -> std::io::Result<Self> {
        let sock = socket_path();
        let stream = UnixStream::connect(&sock).await?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: Mutex::new(BufReader::new(r)),
            writer: Mutex::new(w),
        })
    }

    /// Send a message to the service.
    pub async fn send(&self, msg: &ClientMessage) -> std::io::Result<()> {
        let mut w = self.writer.lock().await;
        write_message!(&mut *w, msg)
    }

    /// Receive a message from the service. Returns None on disconnect.
    pub async fn recv(&self) -> std::io::Result<Option<ServiceMessage>> {
        let mut r = self.reader.lock().await;
        read_message!(&mut *r, ServiceMessage)
    }
}

/// Non-async handle for Bevy systems to communicate with the service.
/// Uses a background tokio task and std mpsc channels.
pub struct ServiceHandle {
    cmd_tx: std::sync::mpsc::Sender<ClientMessage>,
    msg_rx: std::sync::Mutex<std::sync::mpsc::Receiver<ServiceMessage>>,
    _runtime: Arc<tokio::runtime::Runtime>,
}

pub type ServiceWake = Arc<dyn Fn() + Send + Sync + 'static>;

#[allow(clippy::result_large_err)]
fn forward_service_message(
    msg_tx: &std::sync::mpsc::Sender<ServiceMessage>,
    wake: Option<&ServiceWake>,
    msg: ServiceMessage,
) -> Result<(), std::sync::mpsc::SendError<ServiceMessage>> {
    msg_tx.send(msg)?;
    if let Some(wake) = wake {
        wake();
    }
    Ok(())
}

fn clean_service_files(sock: &std::path::Path) {
    let _ = std::fs::remove_file(sock);
    let _ = std::fs::remove_file(crate::pid_path());
    let _ = std::fs::remove_file(crate::identity_path());
}

impl ServiceHandle {
    /// Check if the service process is actually alive.
    pub fn service_running() -> bool {
        let sock = socket_path();
        if !sock.exists() {
            return false;
        }
        // Check if the PID file references a live process
        let pid_file = crate::pid_path();
        let pid_str = match std::fs::read_to_string(&pid_file) {
            Ok(s) => s,
            Err(_) => {
                // Socket exists but no PID file — stale state, clean up
                tracing::warn!("socket exists but no PID file, cleaning up");
                clean_service_files(&sock);
                return false;
            }
        };
        let pid: i32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                // Invalid PID file content — clean up
                tracing::warn!(pid_file = ?pid_str.trim(), "invalid PID file content");
                clean_service_files(&sock);
                return false;
            }
        };
        // kill(pid, 0) checks if process exists without sending a signal
        if unsafe { libc::kill(pid, 0) } != 0 {
            // Process is dead — clean up stale files
            tracing::warn!(pid, "stale service — cleaning up");
            clean_service_files(&sock);
            return false;
        }

        let current_identity = match crate::current_executable_identity() {
            Ok(identity) => identity,
            Err(e) => {
                tracing::error!(error = %e, "failed to identify current executable");
                clean_service_files(&sock);
                return false;
            }
        };
        let id_path = crate::identity_path();
        let service_identity = match std::fs::read_to_string(&id_path) {
            Ok(identity) => identity,
            Err(_) => {
                tracing::warn!("service identity missing, cleaning up");
                clean_service_files(&sock);
                return false;
            }
        };
        if !crate::service_identity_matches(&service_identity, &current_identity) {
            tracing::warn!(pid, "service identity mismatch, replacing running daemon");
            let outcome = crate::supervisor::replace_running(pid, || {
                let stream = std::os::unix::net::UnixStream::connect(&sock)?;
                stream.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
                let mut stream = stream;
                crate::write_message_blocking!(
                    &mut stream,
                    &crate::protocol::ClientMessage::Shutdown
                )
            });
            tracing::info!(?outcome, "replaced running daemon");
            crate::supervisor::clean_runtime_files();
            return false;
        }
        true
    }

    /// Connect to the service synchronously.
    /// Returns `None` if the service is not running or connection fails.
    pub fn connect() -> Option<Self> {
        Self::connect_with_wake(None)
    }

    /// Connect to the service synchronously, waking the owner when service messages arrive.
    pub fn connect_with_wake(wake: Option<ServiceWake>) -> Option<Self> {
        if !Self::service_running() {
            return None;
        }

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .ok()?;
        let rt = Arc::new(rt);

        // Verify connection synchronously before returning
        let conn = {
            let rt2 = Arc::clone(&rt);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("service-connect".into())
                .spawn(move || {
                    let result = rt2.block_on(async { ServiceConnection::connect().await });
                    let _ = tx.send(result);
                })
                .ok()?;
            // Wait up to 2s for connection
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(Ok(c)) => Arc::new(c),
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "service connect failed");
                    return None;
                }
                Err(_) => {
                    tracing::error!("service connect timed out");
                    return None;
                }
            }
        };

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<ClientMessage>();
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<ServiceMessage>();

        // Reader task: service -> msg_tx
        let conn_r = Arc::clone(&conn);
        let rt2 = Arc::clone(&rt);
        std::thread::Builder::new()
            .name("service-reader".into())
            .spawn(move || {
                rt2.block_on(async move {
                    loop {
                        match conn_r.recv().await {
                            Ok(Some(msg)) => {
                                if forward_service_message(&msg_tx, wake.as_ref(), msg).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                });
            })
            .ok()?;

        // Writer task: cmd_rx -> service
        let rt3 = Arc::clone(&rt);
        std::thread::Builder::new()
            .name("service-writer".into())
            .spawn(move || {
                rt3.block_on(async move {
                    while let Ok(msg) = cmd_rx.recv() {
                        if conn.send(&msg).await.is_err() {
                            break;
                        }
                    }
                });
            })
            .ok()?;

        Some(Self {
            cmd_tx,
            msg_rx: std::sync::Mutex::new(msg_rx),
            _runtime: rt,
        })
    }

    /// Send a command to the service (non-blocking).
    pub fn send(&self, msg: ClientMessage) {
        let _ = self.cmd_tx.send(msg);
    }

    /// Drain a bounded batch of service messages (non-blocking).
    pub fn drain(&self) -> Vec<ServiceMessage> {
        self.drain_with_status().0
    }

    /// Drain a bounded batch, also reporting whether the per-frame cap was hit.
    ///
    /// When the returned flag is `true` the channel filled the whole batch and
    /// more messages likely remain; the caller must wake the event loop again so
    /// the tail is processed on the next frame instead of stalling until the
    /// reactive timeout.
    pub fn drain_with_status(&self) -> (Vec<ServiceMessage>, bool) {
        let rx = self.msg_rx.lock().unwrap();
        drain_service_messages_bounded(&rx)
    }
}

fn drain_service_messages_bounded(
    rx: &std::sync::mpsc::Receiver<ServiceMessage>,
) -> (Vec<ServiceMessage>, bool) {
    let mut msgs = Vec::with_capacity(MAX_SERVICE_MESSAGES_PER_DRAIN);
    for _ in 0..MAX_SERVICE_MESSAGES_PER_DRAIN {
        let Ok(msg) = rx.try_recv() else {
            return (msgs, false);
        };
        msgs.push(msg);
    }
    (msgs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn forwarding_service_message_wakes_consumer() {
        let (tx, rx) = std::sync::mpsc::channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_for_callback = Arc::clone(&wakes);
        let wake: ServiceWake = Arc::new(move || {
            wakes_for_callback.fetch_add(1, Ordering::Relaxed);
        });

        forward_service_message(
            &tx,
            Some(&wake),
            ServiceMessage::ProcessList {
                processes: Vec::new(),
            },
        )
        .expect("message should forward");

        assert!(matches!(
            rx.try_recv(),
            Ok(ServiceMessage::ProcessList { processes }) if processes.is_empty()
        ));
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn service_message_drain_leaves_excess_messages_for_later_frames() {
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..=MAX_SERVICE_MESSAGES_PER_DRAIN {
            tx.send(ServiceMessage::ProcessList {
                processes: Vec::new(),
            })
            .expect("service message should queue");
        }

        let (drained, capped) = drain_service_messages_bounded(&rx);

        assert_eq!(drained.len(), MAX_SERVICE_MESSAGES_PER_DRAIN);
        assert!(
            capped,
            "hitting the cap must report capped so the caller re-wakes"
        );
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn service_message_drain_reports_not_capped_when_drained_dry() {
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..3 {
            tx.send(ServiceMessage::ProcessList {
                processes: Vec::new(),
            })
            .expect("service message should queue");
        }

        let (drained, capped) = drain_service_messages_bounded(&rx);

        assert_eq!(drained.len(), 3);
        assert!(!capped);
        assert!(rx.try_recv().is_err());
    }
}
