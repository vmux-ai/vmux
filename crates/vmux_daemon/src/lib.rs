pub mod framing;
pub mod protocol;
pub mod server;
pub mod session;

use std::path::PathBuf;

/// Directory for daemon runtime files (socket, pid).
pub fn daemon_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home).join(".vmux")
}

/// Path to the Unix domain socket.
pub fn socket_path() -> PathBuf {
    daemon_dir().join("vmux.sock")
}

/// Path to the PID file.
pub fn pid_path() -> PathBuf {
    daemon_dir().join("daemon.pid")
}
