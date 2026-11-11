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
use crate::http::UnixHttp;
use crate::image::GuestImage;
use crate::ports::PortForwarder;
use crate::ssh::Ssh;

pub struct Engine {
    settings: Settings,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub running: bool,
    pub docker_ready: bool,
    pub socket: PathBuf,
    pub log: PathBuf,
}

impl Engine {
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
        self.settings.validate()?;
        let _lock = self.lock()?;
        self.clean_sockets()?;
        let settings_path = self.settings.save()?;
        let ssh_port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let mut network = Process::new(
            Command::new("gvproxy")
                .arg("-ssh-port")
                .arg(ssh_port.to_string())
                .arg("-listen-vfkit")
                .arg(format!(
                    "unixgram://{}",
                    self.settings.network_socket().display()
                ))
                .arg("-services")
                .arg(format!("unix://{}", self.settings.network_api().display()))
                .stdin(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self.settings.network_socket().exists() {
            if let Some(status) = network.child.try_wait()? {
                return Err(io::Error::other(format!("network helper exited: {status}")));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "network helper did not start",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        #[cfg(target_os = "macos")]
        let mut native = crate::sandbox::Sandbox::command(helper, &self.settings, &settings_path)?;
        #[cfg(not(target_os = "macos"))]
        let mut native = {
            let mut command = Command::new(helper);
            command.arg("native").arg(settings_path);
            command
        };
        let mut machine = Process::new(native.stdin(Stdio::null()).spawn()?);
        let mut ports = PortForwarder::new(&self.settings.socket(), &self.settings.network_api());
        let mut last_error = None;
        let mut forwarding = false;
        let result = loop {
            if let Some(status) = machine.child.try_wait()? {
                break status;
            }
            if !forwarding && Self::ping(&self.settings.transport_socket()) {
                match Ssh::forward(&self.settings) {
                    Ok(()) => forwarding = true,
                    Err(error) => {
                        eprintln!("Docker SSH forwarding: {error}");
                    }
                }
            }
            if forwarding && Self::ping(&self.settings.socket()) {
                match ports.refresh() {
                    Ok(()) => last_error = None,
                    Err(error) => {
                        let error = error.to_string();
                        if last_error.as_ref() != Some(&error) {
                            eprintln!("container port forwarding: {error}");
                            last_error = Some(error);
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        self.clean_sockets()?;
        if result.success() {
            return Ok(());
        }
        Err(io::Error::other(format!("VM exited: {result}")))
    }

    pub fn stop(&self) -> io::Result<()> {
        if !self.status().running {
            return Ok(());
        }
        let mut control = UnixStream::connect(self.settings.control_socket())?;
        control.set_read_timeout(Some(Duration::from_secs(2)))?;
        control.write_all(b"STOP\n")?;
        let mut reply = [0; 16];
        let read = control.read(&mut reply)?;
        if &reply[..read] != b"STOPPING\n" {
            return Err(io::Error::other("VM refused shutdown"));
        }
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

    pub fn status(&self) -> Status {
        Status {
            running: self.is_locked(),
            docker_ready: Self::ping(&self.settings.socket()),
            socket: self.settings.socket(),
            log: self.settings.log(),
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

    fn ping(path: &Path) -> bool {
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

    fn clean_sockets(&self) -> io::Result<()> {
        for path in [
            self.settings.socket(),
            self.settings.transport_socket(),
            self.settings.control_socket(),
            self.settings.guest_control(),
            self.settings.network_socket(),
            self.settings.network_api(),
        ] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

struct Process {
    child: Child,
}

impl Process {
    fn new(child: Child) -> Self {
        Self { child }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparing_a_running_vm_does_not_change_its_guest_files() {
        let temp = tempfile::tempdir().unwrap();
        let mut settings = Settings::new(temp.path().join("engine"), temp.path());
        settings.validate().unwrap();
        std::fs::create_dir(settings.rootfs()).unwrap();
        let marker = settings.rootfs().join("untouched");
        std::fs::write(&marker, b"running").unwrap();
        let mut engine = Engine::new(settings);
        let _running = engine.lock().unwrap();

        let error = engine.prepare().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(std::fs::read(marker).unwrap(), b"running");
        assert!(!temp.path().join("engine/data.raw").exists());
    }
}
