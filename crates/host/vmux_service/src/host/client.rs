use crate::protocol::{ClientMessage, ServiceMessage};
use crate::{DaemonBinary, DaemonIdentity, ServicePaths};
use bevy_ecs::resource::Resource;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use vmux_client::client::ServiceConnection;

#[derive(Resource)]
pub struct ServiceClient(pub ServiceHandle);

const MAX_SERVICE_MESSAGES_PER_DRAIN: usize = 128;

pub struct ServiceHandle {
    cmd_tx: std::sync::mpsc::Sender<ClientMessage>,
    msg_rx: std::sync::Mutex<std::sync::mpsc::Receiver<ServiceMessage>>,
    wake_pending: Arc<AtomicBool>,
    _runtime: Arc<tokio::runtime::Runtime>,
}

pub type ServiceWake = Arc<dyn Fn() + Send + Sync + 'static>;

#[allow(clippy::result_large_err)]
fn forward_service_message(
    msg_tx: &std::sync::mpsc::Sender<ServiceMessage>,
    wake: Option<&ServiceWake>,
    wake_pending: &AtomicBool,
    msg: ServiceMessage,
) -> Result<(), std::sync::mpsc::SendError<ServiceMessage>> {
    msg_tx.send(msg)?;
    if let Some(wake) = wake
        && !wake_pending.swap(true, Ordering::AcqRel)
    {
        wake();
    }
    Ok(())
}

fn clean_service_files(sock: &std::path::Path) {
    let paths = ServicePaths::current();
    let _ = std::fs::remove_file(sock);
    let _ = std::fs::remove_file(paths.pid());
    let _ = std::fs::remove_file(paths.identity());
}

impl ServiceHandle {
    pub fn service_running() -> bool {
        let paths = ServicePaths::current();
        let sock = paths.socket();
        if !sock.exists() {
            return false;
        }
        let pid_file = paths.pid();
        let pid_str = match std::fs::read_to_string(&pid_file) {
            Ok(s) => s,
            Err(_) => {
                tracing::warn!("socket exists but no PID file, cleaning up");
                clean_service_files(&sock);
                return false;
            }
        };
        let pid: i32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(pid_file = ?pid_str.trim(), "invalid PID file content");
                clean_service_files(&sock);
                return false;
            }
        };
        if unsafe { libc::kill(pid, 0) } != 0 {
            tracing::warn!(pid, "stale service — cleaning up");
            clean_service_files(&sock);
            return false;
        }

        let current_identity = match DaemonBinary::current().and_then(|daemon| daemon.identity()) {
            Ok(identity) => identity,
            Err(e) => {
                tracing::error!(error = %e, "failed to identify current executable");
                clean_service_files(&sock);
                return false;
            }
        };
        let service_identity = match std::fs::read_to_string(paths.identity()) {
            Ok(identity) => DaemonIdentity::recorded(&identity),
            Err(_) => {
                tracing::warn!("service identity missing, cleaning up");
                clean_service_files(&sock);
                return false;
            }
        };
        if !service_identity.matches(&current_identity) {
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

    pub fn connect() -> Option<Self> {
        Self::connect_with_wake(None)
    }

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
        let wake_pending = Arc::new(AtomicBool::new(false));

        let conn_r = Arc::clone(&conn);
        let rt2 = Arc::clone(&rt);
        let reader_wake_pending = Arc::clone(&wake_pending);
        std::thread::Builder::new()
            .name("service-reader".into())
            .spawn(move || {
                rt2.block_on(async move {
                    loop {
                        match conn_r.recv().await {
                            Ok(Some(msg)) => {
                                if forward_service_message(
                                    &msg_tx,
                                    wake.as_ref(),
                                    &reader_wake_pending,
                                    msg,
                                )
                                .is_err()
                                {
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
            wake_pending,
            _runtime: rt,
        })
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.cmd_tx.send(msg);
    }

    pub fn drain(&self) -> Vec<ServiceMessage> {
        self.drain_with_status().0
    }

    pub fn drain_with_status(&self) -> (Vec<ServiceMessage>, bool) {
        self.wake_pending.store(false, Ordering::Release);
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
    fn forwarding_burst_wakes_consumer_once_until_drain() {
        let (tx, rx) = std::sync::mpsc::channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_for_callback = Arc::clone(&wakes);
        let wake: ServiceWake = Arc::new(move || {
            wakes_for_callback.fetch_add(1, Ordering::Relaxed);
        });
        let wake_pending = AtomicBool::new(false);

        forward_service_message(
            &tx,
            Some(&wake),
            &wake_pending,
            ServiceMessage::ProcessList {
                processes: Vec::new(),
            },
        )
        .expect("message should forward");
        forward_service_message(
            &tx,
            Some(&wake),
            &wake_pending,
            ServiceMessage::ProcessList {
                processes: Vec::new(),
            },
        )
        .expect("message should forward");

        assert!(matches!(
            rx.try_recv(),
            Ok(ServiceMessage::ProcessList { processes }) if processes.is_empty()
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ServiceMessage::ProcessList { processes }) if processes.is_empty()
        ));
        assert_eq!(wakes.load(Ordering::Relaxed), 1);

        wake_pending.store(false, Ordering::Release);
        forward_service_message(
            &tx,
            Some(&wake),
            &wake_pending,
            ServiceMessage::ProcessList {
                processes: Vec::new(),
            },
        )
        .expect("message should forward");

        assert_eq!(wakes.load(Ordering::Relaxed), 2);
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
