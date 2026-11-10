use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use vmux_ecs::BlockingCancellation;

pub struct GitProcess;

impl GitProcess {
    const POLL_INTERVAL: Duration = Duration::from_millis(25);

    pub fn command() -> Command {
        let mut command = Command::new(Self::program());
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", Self::null_device())
            .env("GIT_CONFIG_GLOBAL", Self::null_device())
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }

    pub fn prepare(command: &mut Command) {
        #[cfg(unix)]
        {
            command.process_group(0);
        }
    }

    pub fn output(
        command: &mut Command,
        timeout: Duration,
        limit: u64,
        operation: &str,
        cancellation: &BlockingCancellation,
        validate: impl Fn() -> Result<(), String>,
    ) -> Result<String, String> {
        let mut stdout = tempfile::tempfile().map_err(|error| error.to_string())?;
        let mut stderr = tempfile::tempfile().map_err(|error| error.to_string())?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                stdout.try_clone().map_err(|error| error.to_string())?,
            ))
            .stderr(Stdio::from(
                stderr.try_clone().map_err(|error| error.to_string())?,
            ));
        Self::prepare(command);
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let status = Self::wait(&mut child, timeout, operation, cancellation, validate)?;
        let stdout = Self::read(&mut stdout, limit, operation)?;
        let stderr = Self::read(&mut stderr, limit, operation)?;
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            if stderr.is_empty() {
                return Err(format!("{operation} exited with {status}"));
            }
            return Err(stderr);
        }
        String::from_utf8(stdout)
            .map(|value| value.trim().to_string())
            .map_err(|error| error.to_string())
    }

    pub fn wait(
        child: &mut Child,
        timeout: Duration,
        operation: &str,
        cancellation: &BlockingCancellation,
        validate: impl Fn() -> Result<(), String>,
    ) -> Result<ExitStatus, String> {
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => {}
                Err(error) => {
                    Self::terminate(child);
                    return Err(error.to_string());
                }
            }
            if cancellation.is_canceled() {
                Self::terminate(child);
                return Err(format!("{operation} canceled"));
            }
            if let Err(error) = validate() {
                Self::terminate(child);
                return Err(error);
            }
            if Instant::now() >= deadline {
                Self::terminate(child);
                return Err(format!(
                    "{operation} timed out after {} seconds",
                    timeout.as_secs()
                ));
            }
            std::thread::sleep(Self::POLL_INTERVAL);
        }
    }

    fn read(output: &mut File, limit: u64, operation: &str) -> Result<Vec<u8>, String> {
        output
            .seek(SeekFrom::Start(0))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        output
            .by_ref()
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > limit {
            return Err(format!("{operation} output is too large"));
        }
        Ok(bytes)
    }

    #[cfg(unix)]
    fn terminate(child: &mut Child) {
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(not(unix))]
    fn terminate(child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    fn program() -> PathBuf {
        for path in ["/usr/bin/git", "/bin/git"] {
            let program = PathBuf::from(path);
            if program.is_file() {
                return program;
            }
        }
        PathBuf::from("/usr/bin/git")
    }

    #[cfg(unix)]
    fn null_device() -> &'static str {
        "/dev/null"
    }

    #[cfg(not(unix))]
    fn null_device() -> &'static str {
        "NUL"
    }
}
