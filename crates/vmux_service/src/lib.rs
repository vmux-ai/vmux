pub mod client;
pub mod framing;
pub mod process;
pub mod protocol;
pub mod server;

use std::path::PathBuf;

/// Directory for service runtime files (socket, pid, log).
pub fn service_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home).join("Library/Application Support/Vmux/services")
}

/// Path to the Unix domain socket.
pub fn socket_path() -> PathBuf {
    service_dir().join("vmux.sock")
}

/// Path to the PID file.
pub fn pid_path() -> PathBuf {
    service_dir().join("service.pid")
}
