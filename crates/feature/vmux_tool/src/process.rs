use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Clone)]
pub struct Execute {
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub timeout: std::time::Duration,
}

impl Execute {
    fn find(name: &str) -> Option<PathBuf> {
        for directory in Self::executable_paths() {
            let path = directory.join(name);
            if Self::is_executable(&path) {
                return Some(path);
            }
        }
        None
    }

    fn executable_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(path) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&path) {
                paths.push(directory);
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            paths.push(home.join(".local/bin"));
            paths.push(home.join(".cargo/bin"));
        }
        paths.push(PathBuf::from("/opt/homebrew/bin"));
        paths.push(PathBuf::from("/usr/local/bin"));
        paths
    }

    fn executable_path() -> OsString {
        std::env::join_paths(Self::executable_paths())
            .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
    }

    #[cfg(unix)]
    fn is_executable(path: &Path) -> bool {
        path.is_file()
            && path
                .metadata()
                .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    }

    #[cfg(not(unix))]
    fn is_executable(path: &Path) -> bool {
        path.is_file()
    }
}

impl vmux_ecs::Job for Execute {
    type Output = Result<Option<ProcessOutput>, String>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::BlockingCancellation::canceled() {
            return Err("process execution canceled".into());
        }
        let Some(program) = Self::find(&self.program) else {
            return Ok(None);
        };
        let mut command = Command::new(program);
        command
            .args(self.arguments)
            .envs(self.environment)
            .env("PATH", Self::executable_path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            command.process_group(0);
        }
        let child = command.spawn().map_err(|error| error.to_string())?;
        Capture::new(
            child,
            self.timeout.min(std::time::Duration::from_secs(1200)),
        )?
        .output()
        .map(|output| Some(ProcessOutput(output)))
    }
}

#[derive(bevy_ecs::prelude::Component)]
pub struct ProcessOutput(pub Output);

struct Capture {
    child: std::process::Child,
    stdout: Option<Pipe<std::process::ChildStdout>>,
    stderr: Option<Pipe<std::process::ChildStderr>>,
    deadline: std::time::Instant,
    reaped: bool,
}

impl Capture {
    fn new(child: std::process::Child, timeout: std::time::Duration) -> Result<Self, String> {
        let mut capture = Self {
            child,
            stdout: None,
            stderr: None,
            deadline: std::time::Instant::now() + timeout,
            reaped: false,
        };
        let stdout = capture
            .child
            .stdout
            .take()
            .ok_or("stdout pipe is unavailable")?;
        capture.stdout = Some(Pipe::new(stdout)?);
        let stderr = capture
            .child
            .stderr
            .take()
            .ok_or("stderr pipe is unavailable")?;
        capture.stderr = Some(Pipe::new(stderr)?);
        Ok(capture)
    }

    fn output(mut self) -> Result<Output, String> {
        let mut status = None;
        let mut drain_deadline = None;
        loop {
            if vmux_ecs::BlockingCancellation::canceled() {
                return Err("process execution canceled".into());
            }
            let now = std::time::Instant::now();
            if now >= self.deadline {
                return Err("process execution timed out".into());
            }
            self.stdout.as_mut().unwrap().read()?;
            self.stderr.as_mut().unwrap().read()?;
            if status.is_none() {
                status = self.child.try_wait().map_err(|error| error.to_string())?;
                if status.is_some() {
                    self.reaped = true;
                }
            }
            let drained =
                self.stdout.as_ref().unwrap().closed && self.stderr.as_ref().unwrap().closed;
            if let Some(status) = status {
                if drained {
                    return Ok(Output {
                        status,
                        stdout: self.stdout.take().unwrap().bytes,
                        stderr: self.stderr.take().unwrap().bytes,
                    });
                }
                if drain_deadline.is_none() {
                    self.kill_group();
                    drain_deadline = Some(now + std::time::Duration::from_millis(250));
                }
            }
            if drain_deadline.is_some_and(|deadline| now >= deadline) {
                return Err("process pipes remained open after child exit".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn kill_group(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.kill();
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if !self.reaped {
            self.kill_group();
            let _ = self.child.wait();
        }
    }
}

struct Pipe<R> {
    stream: R,
    bytes: Vec<u8>,
    closed: bool,
}

impl<R: std::io::Read + std::os::fd::AsRawFd> Pipe<R> {
    fn new(stream: R) -> Result<Self, String> {
        let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                < 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Self {
            stream,
            bytes: Vec::new(),
            closed: false,
        })
    }

    fn read(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        let mut buffer = [0; 65536];
        loop {
            let read = match self.stream.read(&mut buffer) {
                Ok(0) => {
                    self.closed = true;
                    return Ok(());
                }
                Ok(read) => read,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(());
                }
                Err(error) => return Err(error.to_string()),
            };
            if self.bytes.len().saturating_add(read) > 1048576 {
                return Err("process output exceeds 1048576 bytes".into());
            }
            self.bytes.extend_from_slice(&buffer[..read]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::Job;

    struct EscapedPipe {
        directory: tempfile::TempDir,
    }

    impl EscapedPipe {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            std::fs::write(
                directory.path().join("holder.py"),
                r#"
import pathlib, sys, time
root = pathlib.Path(sys.argv[1])
root.joinpath('started').write_text('ready')
deadline = time.monotonic() + 8
while not root.joinpath('stop').exists() and time.monotonic() < deadline:
    time.sleep(0.005)
root.joinpath('finished').write_text('done')
"#,
            )
            .unwrap();
            Self { directory }
        }

        fn execute(&self, waiting_parent: bool) -> Execute {
            let script = r#"
import pathlib, subprocess, sys, time
root = pathlib.Path(sys.argv[1])
subprocess.Popen([sys.executable, str(root / 'holder.py'), str(root)], start_new_session=True)
while not root.joinpath('started').exists():
    time.sleep(0.005)
sys.stdout.write('parent-ready')
sys.stdout.flush()
if sys.argv[2] == 'wait':
    time.sleep(8)
"#;
            Execute {
                program: "python3".into(),
                arguments: vec![
                    "-c".into(),
                    script.into(),
                    self.directory.path().to_str().unwrap().into(),
                    if waiting_parent { "wait" } else { "exit" }.into(),
                ],
                environment: Vec::new(),
                timeout: if waiting_parent {
                    std::time::Duration::from_millis(750)
                } else {
                    std::time::Duration::from_secs(4)
                },
            }
        }
    }

    impl Drop for EscapedPipe {
        fn drop(&mut self) {
            std::fs::write(self.directory.path().join("stop"), "stop").unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !self.directory.path().join("finished").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "escaped fixture process did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn process_output_drain_cannot_wait_for_an_escaped_descendant_pipe() {
        for waiting_parent in [false, true] {
            let fixture = EscapedPipe::new();
            let start = std::time::Instant::now();
            let result = fixture.execute(waiting_parent).run();
            assert!(
                fixture.directory.path().join("started").exists(),
                "fixture must exercise inherited open pipes"
            );
            assert!(
                result.is_err(),
                "escaped output ownership must not be reported as successful drain"
            );
            assert!(
                start.elapsed() < std::time::Duration::from_secs(3),
                "process timeout or completed-child drain must close owned pipes promptly"
            );
        }
    }

    #[test]
    fn native_process_capture_drains_both_streams_and_refuses_oversized_output() {
        let result = Execute {
            program: "python3".into(),
            arguments: vec![
                "-c".into(),
                "import sys; sys.stdout.write('stdout-end'); sys.stderr.write('stderr-end')".into(),
            ],
            environment: Vec::new(),
            timeout: std::time::Duration::from_secs(5),
        }
        .run()
        .unwrap()
        .expect("Python fixture executable is required");
        assert!(result.0.status.success());
        assert_eq!(result.0.stdout, b"stdout-end");
        assert_eq!(result.0.stderr, b"stderr-end");
        let result = Execute {
            program: "python3".into(),
            arguments: vec![
                "-c".into(),
                "import sys; sys.stdout.write('x' * (2 * 1024 * 1024))".into(),
            ],
            environment: Vec::new(),
            timeout: std::time::Duration::from_secs(5),
        }
        .run();
        assert!(result.is_err());
    }

    #[test]
    fn process_environment_is_scoped_to_the_requested_invocation() {
        let directory = tempfile::tempdir().unwrap();
        let inherited = std::env::var("GOBIN").unwrap_or_else(|_| "unconfigured".into());
        let destination = directory.path().join("installed-command");
        let result = Execute {
            program: "python3".into(),
            arguments: vec![
                "-c".into(),
                "import os, pathlib; pathlib.Path(os.environ['GOBIN']).write_bytes(b'scoped-install')".into(),
            ],
            environment: vec![("GOBIN".into(), destination.to_string_lossy().into_owned())],
            timeout: std::time::Duration::from_secs(5),
        }.run().unwrap().expect("Python fixture executable is required");
        assert!(result.0.status.success());
        assert_eq!(std::fs::read(destination).unwrap(), b"scoped-install");
        let result = Execute {
            program: "python3".into(),
            arguments: vec![
                "-c".into(),
                "import os, sys; sys.stdout.write(os.environ.get('GOBIN', 'unconfigured'))".into(),
            ],
            environment: Vec::new(),
            timeout: std::time::Duration::from_secs(5),
        }
        .run()
        .unwrap()
        .expect("Python fixture executable is required");
        assert_eq!(result.0.stdout, inherited.as_bytes());
    }
}
