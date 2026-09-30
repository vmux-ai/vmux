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

pub(crate) struct RunningDaemon(i32);

impl RunningDaemon {
    pub(crate) fn new(pid: i32) -> Self {
        Self(pid)
    }

    fn wait_for_exit(&self, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            if !self.is_alive() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        !self.is_alive()
    }

    fn is_alive(&self) -> bool {
        unsafe { libc::kill(self.0, 0) == 0 }
    }

    fn signal(&self, signal: i32) -> std::io::Result<()> {
        let result = unsafe { libc::kill(self.0, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub(crate) fn replace<F>(&self, send_shutdown: F) -> ReplaceOutcome
    where
        F: FnOnce() -> std::io::Result<()>,
    {
        if !self.is_alive() {
            return ReplaceOutcome::AlreadyDead;
        }

        if send_shutdown().is_ok() && self.wait_for_exit(Instant::now() + SHUTDOWN_GRACE) {
            tracing::info!(pid = self.0, "old daemon exited via Shutdown handshake");
            return ReplaceOutcome::GracefulShutdown;
        }

        tracing::warn!(pid = self.0, "Shutdown timed out, escalating to SIGTERM");
        let _ = self.signal(libc::SIGTERM);
        if self.wait_for_exit(Instant::now() + SIGTERM_GRACE) {
            return ReplaceOutcome::SigtermExit;
        }

        tracing::warn!(pid = self.0, "SIGTERM timed out, escalating to SIGKILL");
        let _ = self.signal(libc::SIGKILL);
        let _ = self.wait_for_exit(Instant::now() + Duration::from_millis(500));
        ReplaceOutcome::SigkillExit
    }
}

pub(crate) struct ServiceRuntimeFiles;

impl ServiceRuntimeFiles {
    pub(crate) fn remove() {
        let paths = crate::ServicePaths::current();
        let _ = std::fs::remove_file(paths.socket());
        let _ = std::fs::remove_file(paths.pid());
        let _ = std::fs::remove_file(paths.identity());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_dead_pid_returns_alreadydead() {
        let mut pid = 999_999;
        while RunningDaemon::new(pid).is_alive() {
            pid -= 1;
        }
        let outcome = RunningDaemon::new(pid).replace(|| Ok(()));
        assert_eq!(outcome, ReplaceOutcome::AlreadyDead);
    }

    fn start_and_detach() -> i32 {
        let child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id() as i32;
        std::thread::spawn(move || {
            let mut c = child;
            let _ = c.wait();
        });
        pid
    }

    #[test]
    fn graceful_shutdown_when_send_succeeds_and_pid_exits() {
        let pid = start_and_detach();

        let outcome = RunningDaemon::new(pid).replace(|| {
            unsafe { libc::kill(pid, libc::SIGTERM) };
            Ok(())
        });
        assert_eq!(outcome, ReplaceOutcome::GracefulShutdown);
    }

    #[test]
    fn escalates_to_sigterm_when_shutdown_send_fails() {
        let pid = start_and_detach();

        let outcome = RunningDaemon::new(pid).replace(|| {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "no socket",
            ))
        });
        assert_eq!(outcome, ReplaceOutcome::SigtermExit);
    }
}
