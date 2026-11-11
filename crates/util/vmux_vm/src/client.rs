use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::Settings;
use crate::engine::{Phase, State};
use crate::http::UnixHttp;
use crate::image::GuestImage;
use crate::ssh::Ssh;

pub struct EngineClient {
    settings: Settings,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub running: bool,
    pub docker_ready: bool,
    pub socket: PathBuf,
    pub log: PathBuf,
    pub phase: Phase,
    pub failure: Option<String>,
}

impl EngineClient {
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    pub fn prepare(&mut self) -> io::Result<()> {
        self.settings.validate()?;
        #[cfg(unix)]
        {
            std::fs::set_permissions(
                &self.settings.directory,
                std::fs::Permissions::from_mode(0o700),
            )?;
        }
        let _lock = self.lock()?;
        GuestImage::prepare(&self.settings)?;
        Ssh::prepare(&self.settings)?;
        self.settings.save()?;
        Ok(())
    }

    pub fn configure(&mut self) -> io::Result<Settings> {
        self.settings.validate()?;
        let _start = self.lock_named("start.lock")?;
        let _engine = self.lock()?;
        self.settings.save()?;
        Ok(self.settings.clone())
    }

    pub fn start(&mut self, helper: &Path) -> io::Result<Status> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the native VM backend currently requires Apple Silicon macOS",
            ));
        }
        self.settings.validate()?;
        let _start = self.lock_named("start.lock")?;
        if self.status().running {
            let saved = Settings::load(&self.settings.directory.join("settings.json"))?;
            if saved != self.settings {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "stop the running VM before changing its resources or shared directory",
                ));
            }
            return self.wait_ready(None);
        }
        self.prepare()?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.settings.log())?;
        let mut command = Command::new(helper);
        command
            .arg("run")
            .arg(self.settings.directory.join("settings.json"))
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        command.process_group(0);
        let mut child = command.spawn()?;
        self.wait_ready(Some(&mut child))
    }

    pub fn foreground(&mut self, helper: &Path) -> io::Result<()> {
        crate::engine::Runtime::run(self.settings.clone(), helper.to_path_buf())
    }

    pub fn stop(&self) -> io::Result<()> {
        if !self.status().running {
            return Ok(());
        }
        self.request_stop()?;
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.status().running {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "VM did not finish shutdown",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    pub(crate) fn request_stop(&self) -> io::Result<()> {
        let mut control = UnixStream::connect(self.settings.control_socket())?;
        control.set_read_timeout(Some(Duration::from_secs(2)))?;
        control.write_all(b"STOP\n")?;
        let mut reply = [0; 16];
        let read = control.read(&mut reply)?;
        if &reply[..read] != b"STOPPING\n" {
            return Err(io::Error::other("VM refused shutdown"));
        }
        Ok(())
    }

    pub fn status(&self) -> Status {
        let running = self.is_locked();
        let state = std::fs::read(self.settings.directory.join("engine-state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<State>(&bytes).ok());
        let phase = if running {
            match state.as_ref().map(|state| state.phase) {
                Some(Phase::Stopped | Phase::Failed) | None => Phase::Created,
                Some(phase) => phase,
            }
        } else if state
            .as_ref()
            .is_some_and(|state| state.phase == Phase::Failed)
        {
            Phase::Failed
        } else {
            Phase::Stopped
        };
        Status {
            running,
            docker_ready: running && Self::ping(&self.settings.socket()),
            socket: self.settings.socket(),
            log: self.settings.log(),
            phase,
            failure: state.and_then(|state| state.failure),
        }
    }

    fn wait_ready(&self, mut child: Option<&mut Child>) -> io::Result<Status> {
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            let status = self.status();
            if status.docker_ready {
                match std::fs::remove_file(self.settings.directory.join("data.new")) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                return Ok(status);
            }
            if let Some(child) = child.as_deref_mut()
                && let Some(status) = child.try_wait()?
            {
                return Err(io::Error::other(format!(
                    "container engine exited: {status}; see {}",
                    self.settings.log().display()
                )));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Docker did not become ready; see {}",
                        self.settings.log().display()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub(crate) fn ping(path: &Path) -> bool {
        match UnixHttp::new(path).get("/_ping") {
            Ok(response) => matches!(response.require_success(), Ok(body) if body == b"OK"),
            Err(_) => false,
        }
    }

    fn lock(&self) -> io::Result<File> {
        self.lock_named("engine.lock")
    }

    fn lock_named(&self, name: &str) -> io::Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.settings.directory.join(name))?;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(file)
    }

    fn is_locked(&self) -> bool {
        match self.lock() {
            Ok(_) => false,
            Err(error) => error.kind() == io::ErrorKind::WouldBlock,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuring_a_running_engine_preserves_its_saved_resources() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        let path = settings.save().unwrap();
        let previous = std::fs::read(&path).unwrap();
        let mut engine = EngineClient::new(settings);
        let _running = engine.lock().unwrap();
        engine.settings.cpus = 4;
        assert_eq!(
            engine.configure().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(std::fs::read(path).unwrap(), previous);
    }

    #[test]
    fn preparing_a_running_vm_does_not_change_its_guest_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        std::fs::create_dir(settings.rootfs()).unwrap();
        let marker = settings.rootfs().join("untouched");
        std::fs::write(&marker, b"running").unwrap();
        let mut engine = EngineClient::new(settings);
        let _running = engine.lock().unwrap();

        let error = engine.prepare().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(std::fs::read(marker).unwrap(), b"running");
        assert!(!temp.path().join("engine/data.raw").exists());
    }
}
