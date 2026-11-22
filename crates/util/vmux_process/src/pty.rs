use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::path::Component;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::platform::cell::SyncCell;
use bevy::prelude::*;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use tokio::sync::mpsc;
use vmux_api::protocol::ProcessId;
use vmux_profile::ServicePaths;

use crate::shell_integration::{ShellInput, ShellIntegration};

#[derive(Component)]
pub(crate) struct ProcessPty(pub(crate) SyncCell<NativePty>);

#[derive(Clone)]
pub struct PtyInputWriter {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    input_pending: Arc<AtomicBool>,
    shell_input: Arc<Mutex<ShellInput>>,
}

impl PtyInputWriter {
    pub(crate) fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
            input_pending: Arc::new(AtomicBool::new(false)),
            shell_input: Arc::default(),
        }
    }

    pub(crate) fn write_bytes(&self, data: &[u8]) {
        self.input_pending.store(true, Ordering::Release);
        if let Ok(mut writer) = self.writer.lock() {
            if let Ok(mut input) = self.shell_input.lock() {
                input.typed(data);
            }
            let _ = writer.write_all(data);
        }
    }

    pub(crate) fn write_raw(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
        }
    }

    pub(crate) fn write_directory_command(
        &self,
        command: &str,
        pty_drained: bool,
    ) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|error| error.to_string())?;
        let mut input = self.shell_input.lock().map_err(|error| error.to_string())?;
        if !input.ready() || !pty_drained {
            return Err("terminal is not at an empty shell prompt".into());
        }
        input.typed(command.as_bytes());
        writer
            .write_all(command.as_bytes())
            .map_err(|error| error.to_string())?;
        writer.flush().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn notify_command_start(&self) {
        if let Ok(mut input) = self.shell_input.lock() {
            input.started();
        }
    }

    pub(crate) fn notify_command_end(&self) {
        if let Ok(mut input) = self.shell_input.lock() {
            input.finished();
        }
    }

    pub(crate) fn is_prompt_ready(&self) -> bool {
        self.shell_input.lock().is_ok_and(|input| input.ready())
    }

    pub(crate) fn take_keystroke(&self, got_data: bool) -> bool {
        got_data && self.input_pending.swap(false, Ordering::AcqRel)
    }

    #[cfg(test)]
    pub(crate) fn mark_input_pending(&self) {
        self.input_pending.store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn is_input_pending(&self) -> bool {
        self.input_pending.load(Ordering::Acquire)
    }
}

pub(crate) struct SpawnedPty {
    pub(crate) pid: u32,
    pub(crate) writer: PtyInputWriter,
    pub(crate) pty: NativePty,
}

pub(crate) struct NativePty {
    master: Option<Box<dyn MasterPty + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pty_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    #[cfg(unix)]
    pty_reader_in_flight: Arc<AtomicBool>,
    #[cfg(unix)]
    stop_tx: Option<OwnedFd>,
    reader_thread: Option<JoinHandle<()>>,
    exited: bool,
}

impl NativePty {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn(
        id: ProcessId,
        command: &str,
        mut args: Vec<String>,
        cwd: &str,
        mut env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
        wake_tx: mpsc::UnboundedSender<()>,
        reader_send_delay: Duration,
    ) -> Result<SpawnedPty, String> {
        ShellIntegration::inject(
            command,
            &mut args,
            &mut env,
            &ServicePaths::shell_integration_dir(),
        );
        let pair = Self::open_pair(cols, rows)?;
        let cmd = Self::build_command(command, &args, cwd, &env)?;
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|error| format!("failed to spawn shell: {error}"))?;
        let pid = child
            .process_id()
            .ok_or_else(|| "spawned PTY child has no PID".to_string())?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("failed to clone PTY reader: {error}"))?;
        #[cfg(unix)]
        let reader_fd = pair
            .master
            .as_raw_fd()
            .ok_or_else(|| "PTY master has no file descriptor".to_string())?;
        let writer = PtyInputWriter::new(Box::new(
            pair.master
                .take_writer()
                .map_err(|error| format!("failed to take PTY writer: {error}"))?,
        ));
        drop(pair.slave);

        let (pty_tx, pty_rx) = mpsc::unbounded_channel();
        #[cfg(unix)]
        let (pty_reader_in_flight, stop_tx, reader_thread) =
            Self::spawn_unix_reader(id, reader, reader_fd, pty_tx, wake_tx, reader_send_delay)?;
        #[cfg(not(unix))]
        let reader_thread =
            Self::spawn_fallback_reader(id, reader, pty_tx, wake_tx, reader_send_delay)?;

        Ok(SpawnedPty {
            pid,
            writer,
            pty: Self {
                master: Some(pair.master),
                child,
                pty_rx,
                #[cfg(unix)]
                pty_reader_in_flight,
                #[cfg(unix)]
                stop_tx: Some(stop_tx),
                reader_thread: Some(reader_thread),
                exited: false,
            },
        })
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        if let Some(master) = &self.master {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub(crate) fn try_recv_chunk(&mut self) -> Result<Vec<u8>, mpsc::error::TryRecvError> {
        self.pty_rx.try_recv()
    }

    pub(crate) fn queued_chunks(&self) -> usize {
        self.pty_rx.len()
    }

    pub(crate) fn has_queued_output(&self) -> bool {
        !self.pty_rx.is_empty()
    }

    pub(crate) fn reader_drained(&self) -> bool {
        #[cfg(unix)]
        {
            if self.pty_reader_in_flight.load(Ordering::Acquire) {
                return false;
            }
            let Some(fd) = self.master.as_ref().and_then(|master| master.as_raw_fd()) else {
                return true;
            };
            let mut available: libc::c_int = 0;
            if unsafe { libc::ioctl(fd, libc::FIONREAD as _, &mut available) } < 0 {
                return false;
            }
            available == 0 && !self.pty_reader_in_flight.load(Ordering::Acquire)
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    pub(crate) fn try_wait(&mut self) -> Option<i32> {
        if self.exited {
            return None;
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            self.exited = true;
            return Some(status.exit_code() as i32);
        }
        None
    }

    pub(crate) fn kill(&mut self) -> Option<i32> {
        let mut exit_code = None;
        if !self.exited {
            let _ = self.child.kill();
            if let Ok(status) = self.child.wait() {
                exit_code = Some(status.exit_code() as i32);
            }
            self.exited = true;
        }
        self.drain_reader();
        exit_code
    }

    pub(crate) fn drain_reader(&mut self) {
        #[cfg(unix)]
        {
            drop(self.stop_tx.take());
        }
        self.pty_rx.close();
        drop(self.master.take());
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }

    #[cfg(test)]
    pub(crate) fn replace_pty_rx(&mut self, pty_rx: mpsc::UnboundedReceiver<Vec<u8>>) {
        self.pty_rx = pty_rx;
    }

    fn open_pair(cols: u16, rows: u16) -> Result<portable_pty::PtyPair, String> {
        let pty_system = NativePtySystem::default();
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let mut attempts = 0;
        loop {
            match pty_system.openpty(size) {
                Ok(pair) => return Ok(pair),
                Err(_) if attempts < 2 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(format!("failed to open PTY: {error}")),
            }
        }
    }

    fn build_command(
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
    ) -> Result<CommandBuilder, String> {
        #[cfg(unix)]
        let mut cmd = if cwd.is_empty() {
            let mut cmd = CommandBuilder::new(command);
            cmd.args(args);
            cmd
        } else {
            let executable = Self::resolve_executable(command, cwd, env)?;
            let original_home = env
                .iter()
                .rev()
                .find(|(name, _)| name == "HOME")
                .map(|(_, value)| value.clone())
                .or_else(|| std::env::var("HOME").ok());
            let mut cmd = CommandBuilder::new("/bin/sh");
            cmd.arg("-c");
            cmd.arg(
                "if [ \"$1\" = 1 ]; then export HOME=\"$2\"; else unset HOME; fi; shift 2; exec \"$@\"",
            );
            cmd.arg("vmux-cwd");
            cmd.arg(if original_home.is_some() { "1" } else { "0" });
            cmd.arg(original_home.as_deref().unwrap_or_default());
            cmd.arg(executable);
            cmd.args(args);
            cmd.cwd(cwd);
            cmd
        };
        #[cfg(not(unix))]
        let mut cmd = {
            let mut cmd = CommandBuilder::new(command);
            cmd.args(args);
            if !cwd.is_empty() {
                let cwd_path = Path::new(cwd);
                if !cwd_path.is_dir() {
                    return Err(format!("cwd is not a directory: {cwd}"));
                }
                cmd.cwd(cwd_path);
            }
            cmd
        };
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_CTYPE", "UTF-8");
        for (key, value) in env {
            cmd.env(key, value);
        }
        #[cfg(unix)]
        if !cwd.is_empty() {
            cmd.env("HOME", cwd);
        }
        Ok(cmd)
    }

    #[cfg(unix)]
    fn is_executable(path: &Path) -> bool {
        if path.is_dir() {
            return false;
        }
        let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
    }

    #[cfg(unix)]
    fn resolve_executable(
        command: &str,
        cwd: &str,
        env: &[(String, String)],
    ) -> Result<std::path::PathBuf, String> {
        let command_path = Path::new(command);
        let explicit_cwd = matches!(
            command_path.components().next(),
            Some(Component::CurDir | Component::ParentDir)
        );
        if command_path.is_absolute() || explicit_cwd {
            let candidate = if command_path.is_absolute() {
                command_path.to_path_buf()
            } else {
                Path::new(cwd).join(command_path)
            };
            return Self::is_executable(&candidate)
                .then_some(candidate)
                .ok_or_else(|| format!("unable to execute command: {command}"));
        }

        let path = env
            .iter()
            .rev()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| std::ffi::OsString::from(value))
            .or_else(|| std::env::var_os("PATH"))
            .ok_or_else(|| format!("unable to resolve command without PATH: {command}"))?;
        for path_entry in std::env::split_paths(&path) {
            let candidate = Path::new(cwd).join(path_entry).join(command_path);
            if Self::is_executable(&candidate) {
                return Ok(candidate);
            }
        }
        Err(format!("unable to resolve executable: {command}"))
    }

    #[cfg(unix)]
    fn spawn_unix_reader(
        id: ProcessId,
        reader: Box<dyn Read + Send>,
        reader_fd: RawFd,
        pty_tx: mpsc::UnboundedSender<Vec<u8>>,
        wake_tx: mpsc::UnboundedSender<()>,
        send_delay: Duration,
    ) -> Result<(Arc<AtomicBool>, OwnedFd, JoinHandle<()>), String> {
        let mut fds = [0 as libc::c_int; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(format!(
                "failed to create PTY reader stop pipe: {}",
                io::Error::last_os_error()
            ));
        }
        let stop_rx = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let stop_tx = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        for fd in [stop_rx.as_raw_fd(), stop_tx.as_raw_fd()] {
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
                return Err(format!(
                    "failed to configure PTY reader stop pipe: {}",
                    io::Error::last_os_error()
                ));
            }
        }
        let in_flight = Arc::new(AtomicBool::new(false));
        let thread_in_flight = Arc::clone(&in_flight);
        let handle = std::thread::Builder::new()
            .name(format!("pty-reader-{}", &id.to_string()[..8]))
            .spawn(move || {
                Self::run_unix_reader(
                    reader,
                    reader_fd,
                    stop_rx,
                    pty_tx,
                    wake_tx,
                    thread_in_flight,
                    send_delay,
                );
            })
            .map_err(|error| format!("failed to spawn PTY reader: {error}"))?;
        Ok((in_flight, stop_tx, handle))
    }

    #[cfg(unix)]
    fn run_unix_reader(
        mut reader: Box<dyn Read + Send>,
        reader_fd: RawFd,
        stop_rx: OwnedFd,
        pty_tx: mpsc::UnboundedSender<Vec<u8>>,
        wake_tx: mpsc::UnboundedSender<()>,
        in_flight: Arc<AtomicBool>,
        send_delay: Duration,
    ) {
        let mut buf = [0u8; 4096];
        let stop_fd = stop_rx.as_raw_fd();
        loop {
            let mut poll_fds = [
                libc::pollfd {
                    fd: reader_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: stop_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let ready = unsafe { libc::poll(poll_fds.as_mut_ptr(), 2, -1) };
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            if poll_fds[1].revents != 0 {
                break;
            }
            in_flight.store(true, Ordering::Release);
            match reader.read(&mut buf) {
                Ok(0) => {
                    in_flight.store(false, Ordering::Release);
                    break;
                }
                Ok(n) => {
                    if !send_delay.is_zero() {
                        std::thread::sleep(send_delay);
                    }
                    let sent = pty_tx.send(buf[..n].to_vec()).is_ok();
                    in_flight.store(false, Ordering::Release);
                    if !sent {
                        break;
                    }
                    let _ = wake_tx.send(());
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    in_flight.store(false, Ordering::Release);
                }
                Err(_) => {
                    in_flight.store(false, Ordering::Release);
                    break;
                }
            }
        }
        drop(stop_rx);
        let _ = wake_tx.send(());
    }

    #[cfg(not(unix))]
    fn spawn_fallback_reader(
        id: ProcessId,
        mut reader: Box<dyn Read + Send>,
        pty_tx: mpsc::UnboundedSender<Vec<u8>>,
        wake_tx: mpsc::UnboundedSender<()>,
        send_delay: Duration,
    ) -> Result<JoinHandle<()>, String> {
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", &id.to_string()[..8]))
            .spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if !send_delay.is_zero() {
                                std::thread::sleep(send_delay);
                            }
                            if pty_tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                            let _ = wake_tx.send(());
                        }
                        Err(_) => break,
                    }
                }
                let _ = wake_tx.send(());
            })
            .map_err(|error| format!("failed to spawn PTY reader: {error}"))
    }
}

impl Drop for NativePty {
    fn drop(&mut self) {
        self.kill();
    }
}
