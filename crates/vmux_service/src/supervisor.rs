//! Replace a running daemon: graceful Shutdown over the socket,
//! escalate to SIGTERM, finally SIGKILL.

use std::time::{Duration, Instant};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const SIGTERM_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
pub enum ReplaceOutcome {
    GracefulShutdown,
    SigtermExit,
    SigkillExit,
    AlreadyDead,
}

pub fn wait_for_pid_exit(pid: i32, deadline: Instant) -> bool {
    while Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    !pid_alive(pid)
}

pub fn pid_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

pub fn send_signal(pid: i32, sig: i32) -> std::io::Result<()> {
    let r = unsafe { libc::kill(pid, sig) };
    if r == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(err)
    }
}

/// Replace the running daemon for the given pid.
///
/// `send_shutdown` performs the IPC Shutdown round-trip.
/// `Ok` means acknowledged or connection closed; `Err` means unreachable
/// (falls through to SIGTERM).
pub fn replace_running<F>(pid: i32, send_shutdown: F) -> ReplaceOutcome
where
    F: FnOnce() -> std::io::Result<()>,
{
    if !pid_alive(pid) {
        return ReplaceOutcome::AlreadyDead;
    }

    if send_shutdown().is_ok() && wait_for_pid_exit(pid, Instant::now() + SHUTDOWN_GRACE) {
        tracing::info!(pid, "old daemon exited via Shutdown handshake");
        return ReplaceOutcome::GracefulShutdown;
    }

    tracing::warn!(pid, "Shutdown timed out, escalating to SIGTERM");
    let _ = send_signal(pid, libc::SIGTERM);
    if wait_for_pid_exit(pid, Instant::now() + SIGTERM_GRACE) {
        return ReplaceOutcome::SigtermExit;
    }

    tracing::warn!(pid, "SIGTERM timed out, escalating to SIGKILL");
    let _ = send_signal(pid, libc::SIGKILL);
    let _ = wait_for_pid_exit(pid, Instant::now() + Duration::from_millis(500));
    ReplaceOutcome::SigkillExit
}

/// Best-effort cleanup of stale runtime files for the current profile.
pub fn clean_runtime_files() {
    let _ = std::fs::remove_file(crate::socket_path());
    let _ = std::fs::remove_file(crate::pid_path());
    let _ = std::fs::remove_file(crate::identity_path());
}

#[cfg(test)]
#[path = "supervisor.test.rs"]
mod tests;
