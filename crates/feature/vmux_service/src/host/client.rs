use crate::{DaemonBinary, DaemonIdentity, ServicePaths};
use bevy_ecs::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use vmux_api::protocol::{ClientMessage, ServiceMessage};
use vmux_ecs::service::ServiceConnection;

use super::supervisor::{RunningDaemon, ServiceRuntimeFiles};

#[derive(Component)]
pub(super) struct ServiceClient(pub(super) ServiceHandle);

const MAX_SERVICE_MESSAGES_PER_DRAIN: usize = 128;

pub(super) struct ServiceDrain {
    pub(super) messages: Vec<ServiceMessage>,
    pub(super) disconnected: bool,
    pub(super) capped: bool,
}

pub(super) struct ServiceHandle {
    cmd_tx: std::sync::mpsc::Sender<ClientMessage>,
    msg_rx: std::sync::Mutex<std::sync::mpsc::Receiver<ServiceMessage>>,
    notifier: ServiceNotifier,
    _runtime: Arc<tokio::runtime::Runtime>,
}

pub(super) type ServiceWake = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
struct ServiceNotifier {
    wake: Option<ServiceWake>,
    pending: Arc<AtomicBool>,
    disconnected: Arc<AtomicBool>,
}

impl ServiceNotifier {
    fn new(wake: Option<ServiceWake>) -> Self {
        Self {
            wake,
            pending: Arc::new(AtomicBool::new(false)),
            disconnected: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(test)]
    fn disconnected() -> Self {
        Self {
            wake: None,
            pending: Arc::new(AtomicBool::new(false)),
            disconnected: Arc::new(AtomicBool::new(true)),
        }
    }

    #[allow(clippy::result_large_err)]
    fn forward(
        &self,
        sender: &std::sync::mpsc::Sender<ServiceMessage>,
        message: ServiceMessage,
    ) -> Result<(), std::sync::mpsc::SendError<ServiceMessage>> {
        sender.send(message)?;
        self.wake();
        Ok(())
    }

    fn disconnect(&self) {
        self.disconnected.store(true, Ordering::Release);
        self.wake();
    }

    fn drained(&self) -> bool {
        self.pending.store(false, Ordering::Release);
        self.disconnected.swap(false, Ordering::AcqRel)
    }

    fn wake(&self) {
        if let Some(wake) = &self.wake
            && !self.pending.swap(true, Ordering::AcqRel)
        {
            wake();
        }
    }
}

#[allow(clippy::result_large_err)]
impl ServiceHandle {
    pub(super) fn connect(wake: Option<ServiceWake>) -> Option<Self> {
        if !Self::service_running() {
            return None;
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .ok()?;
        let runtime = Arc::new(runtime);

        let connection = {
            let runtime = Arc::clone(&runtime);
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("service-connect".into())
                .spawn(move || {
                    let result = runtime.block_on(async { ServiceConnection::connect().await });
                    let _ = sender.send(result);
                })
                .ok()?;
            match receiver.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(Ok(connection)) => Arc::new(connection),
                Ok(Err(error)) => {
                    tracing::error!(%error, "service connect failed");
                    return None;
                }
                Err(_) => {
                    tracing::error!("service connect timed out");
                    return None;
                }
            }
        };

        let (command_sender, command_receiver) = std::sync::mpsc::channel::<ClientMessage>();
        let (message_sender, message_receiver) = std::sync::mpsc::channel::<ServiceMessage>();
        let notifier = ServiceNotifier::new(wake);

        let read_connection = Arc::clone(&connection);
        let read_runtime = Arc::clone(&runtime);
        let reader_notifier = notifier.clone();
        std::thread::Builder::new()
            .name("service-reader".into())
            .spawn(move || {
                read_runtime.block_on(async move {
                    loop {
                        match read_connection.recv().await {
                            Ok(Some(message)) => {
                                if reader_notifier.forward(&message_sender, message).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                    reader_notifier.disconnect();
                });
            })
            .ok()?;

        let write_runtime = Arc::clone(&runtime);
        let writer_notifier = notifier.clone();
        std::thread::Builder::new()
            .name("service-writer".into())
            .spawn(move || {
                write_runtime.block_on(async move {
                    while let Ok(message) = command_receiver.recv() {
                        if connection.send(&message).await.is_err() {
                            writer_notifier.disconnect();
                            break;
                        }
                    }
                });
            })
            .ok()?;

        Some(Self {
            cmd_tx: command_sender,
            msg_rx: std::sync::Mutex::new(message_receiver),
            notifier,
            _runtime: runtime,
        })
    }

    pub(super) fn service_running() -> bool {
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
                ServiceRuntimeFiles::remove();
                return false;
            }
        };
        let pid: i32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(pid_file = ?pid_str.trim(), "invalid PID file content");
                ServiceRuntimeFiles::remove();
                return false;
            }
        };
        if unsafe { libc::kill(pid, 0) } != 0 {
            tracing::warn!(pid, "stale service — cleaning up");
            ServiceRuntimeFiles::remove();
            return false;
        }

        let current_identity = match DaemonBinary::current().and_then(|daemon| daemon.identity()) {
            Ok(identity) => identity,
            Err(e) => {
                tracing::error!(error = %e, "failed to identify current executable");
                ServiceRuntimeFiles::remove();
                return false;
            }
        };
        let service_identity = match std::fs::read_to_string(paths.identity()) {
            Ok(identity) => DaemonIdentity::recorded(&identity),
            Err(_) => {
                tracing::warn!("service identity missing, cleaning up");
                ServiceRuntimeFiles::remove();
                return false;
            }
        };
        if !service_identity.matches(&current_identity) {
            tracing::warn!(pid, "service identity mismatch, replacing running daemon");
            let outcome = RunningDaemon::new(pid).replace(|| {
                let stream = std::os::unix::net::UnixStream::connect(&sock)?;
                stream.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
                let mut stream = stream;
                vmux_ecs::service::write_client_message_blocking(
                    &mut stream,
                    &ClientMessage::Shutdown,
                )
            });
            tracing::info!(?outcome, "replaced running daemon");
            ServiceRuntimeFiles::remove();
            return false;
        }
        true
    }

    pub(super) fn send(&self, msg: ClientMessage) -> bool {
        self.cmd_tx.send(msg).is_ok()
    }

    pub(super) fn drain_with_status(&self) -> ServiceDrain {
        let rx = self.msg_rx.lock().unwrap();
        ServiceDrain::read(&rx, self.notifier.drained())
    }

    #[cfg(test)]
    pub(crate) fn disconnected() -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        drop(cmd_rx);
        let (_msg_tx, msg_rx) = std::sync::mpsc::channel();
        Self {
            cmd_tx,
            msg_rx: std::sync::Mutex::new(msg_rx),
            notifier: ServiceNotifier::disconnected(),
            _runtime: Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("runtime should build"),
            ),
        }
    }
}

impl ServiceDrain {
    fn read(rx: &std::sync::mpsc::Receiver<ServiceMessage>, disconnected: bool) -> Self {
        let mut messages = Vec::with_capacity(MAX_SERVICE_MESSAGES_PER_DRAIN);
        for _ in 0..MAX_SERVICE_MESSAGES_PER_DRAIN {
            let Ok(message) = rx.try_recv() else {
                return Self {
                    messages,
                    disconnected,
                    capped: false,
                };
            };
            messages.push(message);
        }
        Self {
            messages,
            disconnected,
            capped: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn forwarding_burst_wakes_consumer_once_until_drain() {
        let (tx, rx) = std::sync::mpsc::channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_for_callback = Arc::clone(&wakes);
        let wake: ServiceWake = Arc::new(move || {
            wakes_for_callback.fetch_add(1, Ordering::Relaxed);
        });
        let notifier = ServiceNotifier::new(Some(wake));

        notifier
            .forward(
                &tx,
                ServiceMessage::ProcessList {
                    processes: Vec::new(),
                },
            )
            .expect("message should forward");
        notifier
            .forward(
                &tx,
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

        assert!(!notifier.drained());
        notifier
            .forward(
                &tx,
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

        let drained = ServiceDrain::read(&rx, false);

        assert_eq!(drained.messages.len(), MAX_SERVICE_MESSAGES_PER_DRAIN);
        assert!(
            drained.capped,
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

        let drained = ServiceDrain::read(&rx, false);

        assert_eq!(drained.messages.len(), 3);
        assert!(!drained.disconnected);
        assert!(!drained.capped);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn service_message_drain_reports_disconnect() {
        let (_tx, rx) = std::sync::mpsc::channel();

        let drained = ServiceDrain::read(&rx, true);

        assert!(drained.messages.is_empty());
        assert!(drained.disconnected);
        assert!(!drained.capped);
    }
}
