use crate::protocol::{ClientMessage, DaemonMessage};
use crate::{read_message, socket_path, write_message};
use std::sync::Arc;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::Mutex;

/// Async client connection to the vmux daemon.
/// Wraps the Unix socket with framing/serialization.
pub struct DaemonConnection {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
}

impl DaemonConnection {
    /// Connect to the daemon socket.
    pub async fn connect() -> std::io::Result<Self> {
        let sock = socket_path();
        let stream = UnixStream::connect(&sock).await?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: Mutex::new(BufReader::new(r)),
            writer: Mutex::new(w),
        })
    }

    /// Send a message to the daemon.
    pub async fn send(&self, msg: &ClientMessage) -> std::io::Result<()> {
        let mut w = self.writer.lock().await;
        write_message!(&mut *w, msg)
    }

    /// Receive a message from the daemon. Returns None on disconnect.
    pub async fn recv(&self) -> std::io::Result<Option<DaemonMessage>> {
        let mut r = self.reader.lock().await;
        read_message!(&mut *r, DaemonMessage)
    }
}

/// Non-async handle for Bevy systems to communicate with the daemon.
/// Uses a background tokio task and std mpsc channels.
pub struct DaemonHandle {
    cmd_tx: std::sync::mpsc::Sender<ClientMessage>,
    msg_rx: std::sync::Mutex<std::sync::mpsc::Receiver<DaemonMessage>>,
    _runtime: Arc<tokio::runtime::Runtime>,
}

impl DaemonHandle {
    /// Check if the daemon process is actually alive.
    pub fn daemon_running() -> bool {
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
                eprintln!("vmux-daemon: socket exists but no PID file, cleaning up");
                let _ = std::fs::remove_file(&sock);
                return false;
            }
        };
        let pid: i32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                // Invalid PID file content — clean up
                eprintln!(
                    "vmux-daemon: invalid PID file content: {:?}",
                    pid_str.trim()
                );
                let _ = std::fs::remove_file(&sock);
                let _ = std::fs::remove_file(&pid_file);
                return false;
            }
        };
        // kill(pid, 0) checks if process exists without sending a signal
        if unsafe { libc::kill(pid, 0) } != 0 {
            // Process is dead — clean up stale files
            eprintln!("vmux-daemon: stale daemon (pid {pid}) — cleaning up");
            let _ = std::fs::remove_file(&sock);
            let _ = std::fs::remove_file(&pid_file);
            return false;
        }
        true
    }

    /// Connect to the daemon synchronously.
    /// Returns `None` if the daemon is not running or connection fails.
    pub fn connect() -> Option<Self> {
        if !Self::daemon_running() {
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
                .name("daemon-connect".into())
                .spawn(move || {
                    let result = rt2.block_on(async { DaemonConnection::connect().await });
                    let _ = tx.send(result);
                })
                .ok()?;
            // Wait up to 2s for connection
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(Ok(c)) => Arc::new(c),
                Ok(Err(e)) => {
                    eprintln!("daemon connect failed: {e}");
                    return None;
                }
                Err(_) => {
                    eprintln!("daemon connect timed out");
                    return None;
                }
            }
        };

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<ClientMessage>();
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<DaemonMessage>();

        // Reader task: daemon -> msg_tx
        let conn_r = Arc::clone(&conn);
        let rt2 = Arc::clone(&rt);
        std::thread::Builder::new()
            .name("daemon-reader".into())
            .spawn(move || {
                rt2.block_on(async move {
                    loop {
                        match conn_r.recv().await {
                            Ok(Some(msg)) => {
                                if msg_tx.send(msg).is_err() {
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

        // Writer task: cmd_rx -> daemon
        let rt3 = Arc::clone(&rt);
        std::thread::Builder::new()
            .name("daemon-writer".into())
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

    /// Send a command to the daemon (non-blocking).
    pub fn send(&self, msg: ClientMessage) {
        let _ = self.cmd_tx.send(msg);
    }

    /// Drain all available messages from the daemon (non-blocking).
    pub fn drain(&self) -> Vec<DaemonMessage> {
        let rx = self.msg_rx.lock().unwrap();
        let mut msgs = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            msgs.push(msg);
        }
        msgs
    }
}
