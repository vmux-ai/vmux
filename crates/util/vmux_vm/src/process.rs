use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use bevy_ecs::prelude::Component;

use crate::Settings;

pub struct Executable;

impl Executable {
    pub(crate) fn named(helper: &Path, settings: &Settings, role: &str) -> io::Result<PathBuf> {
        let directory = settings.directory.join("helpers");
        std::fs::create_dir_all(&directory)?;
        let name = settings
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Default");
        let path = directory.join(format!("Vmux VM ({name}){role}"));
        let source = std::fs::read(helper)?;
        if std::fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            && std::fs::read(&path).is_ok_and(|cached| cached == source)
        {
            return Ok(path);
        }
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&source)?;
        temporary
            .as_file()
            .set_permissions(std::fs::metadata(helper)?.permissions())?;
        temporary.persist(&path).map_err(io::Error::other)?;
        Ok(path)
    }

    pub fn find(name: &str) -> io::Result<PathBuf> {
        if let Some(path) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&path) {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
        for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            let candidate = Path::new(directory).join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{name} is not installed"),
        ))
    }
}

#[derive(Component)]
pub(crate) struct Process(Child, bool);

impl Process {
    pub(crate) fn spawn(command: &mut Command) -> io::Result<Self> {
        Ok(Self(command.stdin(Stdio::null()).spawn()?, false))
    }

    pub(crate) fn supervised(command: &mut Command) -> io::Result<Self> {
        Ok(Self(command.stdin(Stdio::piped()).spawn()?, true))
    }

    pub(crate) fn poll(&mut self) -> io::Result<Option<ExitStatus>> {
        self.0.try_wait()
    }

    pub(crate) fn terminate(&mut self) -> io::Result<()> {
        if self.poll()?.is_none() {
            if self.1 {
                self.0.stdin.take();
            } else {
                self.0.kill()?;
            }
        }
        Ok(())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.poll(), Ok(Some(_))) {
            return;
        }
        if self.1 {
            self.0.stdin.take();
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if matches!(self.poll(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Component)]
pub(crate) struct Lease(File);

impl Lease {
    pub(crate) fn acquire(directory: &Path) -> io::Result<Self> {
        Self::named(directory, "engine.lock")
    }

    pub(crate) fn named(directory: &Path, name: &str) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(directory.join(name))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
    }

    pub(crate) fn retired(directory: &Path) -> io::Result<()> {
        let _machine = Self::named(directory, "machine.lock")?;
        let _network = Self::named(directory, "network.lock")?;
        Ok(())
    }
}

pub struct NetworkProxy;

impl NetworkProxy {
    pub fn run(settings: Settings, ssh_port: u16) -> io::Result<()> {
        let _lease = Lease::named(&settings.directory, "network.lock")?;
        let child = Command::new(Executable::find("gvproxy")?)
            .arg("-ssh-port")
            .arg(ssh_port.to_string())
            .arg("-listen-vfkit")
            .arg(format!(
                "unixgram://{}",
                settings.network_socket().display()
            ))
            .arg("-services")
            .arg(format!("unix://{}", settings.network_api().display()))
            .stdin(Stdio::null())
            .spawn()?;
        Self::watch(child, io::stdin())
    }

    fn watch<R: Read + Send + 'static>(child: Child, mut input: R) -> io::Result<()> {
        let mut process = Process(child, false);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("vmux-network-parent".into())
            .spawn(move || {
                let _ = io::copy(&mut input, &mut io::sink());
                let _ = sender.send(());
            })?;
        loop {
            if let Some(status) = process.poll()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other(format!("network helper exited: {status}")))
                };
            }
            match receiver.try_recv() {
                Ok(()) | Err(TryRecvError::Disconnected) => {
                    process.terminate()?;
                    process.0.wait()?;
                    return Ok(());
                }
                Err(TryRecvError::Empty) => {}
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) struct ParentWatch;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl ParentWatch {
    pub(crate) fn monitor(settings: Settings) -> io::Result<()> {
        let Some(parent) = std::env::var_os("VMUX_ENGINE_PARENT") else {
            return Ok(());
        };
        let parent = parent
            .to_string_lossy()
            .parse::<u32>()
            .map_err(io::Error::other)?;
        std::thread::Builder::new()
            .name("vmux-machine-parent".into())
            .spawn(move || {
                while unsafe { libc::getppid() } as u32 == parent {
                    std::thread::sleep(Duration::from_millis(100));
                }
                let deadline = Instant::now() + Duration::from_secs(60);
                while Instant::now() < deadline {
                    let _ = crate::EngineClient::new(settings.clone()).request_stop();
                    std::thread::sleep(Duration::from_secs(1));
                }
                std::process::exit(1);
            })?;
        Ok(())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

impl Settings {
    pub(crate) fn clean_sockets(&self) -> io::Result<()> {
        for path in [
            self.socket(),
            self.transport_socket(),
            self.control_socket(),
            self.guest_control(),
            self.network_socket(),
            self.network_api(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn named_helpers_remain_executable_and_replace_obsolete_copies() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::new(directory.path().join("Docker"), directory.path());
        let source = directory.path().join("helper");
        std::fs::write(&source, b"#!/bin/sh\nprintf '%s' \"$0\"\n").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        let helper = Executable::named(&source, &settings, " - Linux").unwrap();
        std::fs::write(&helper, b"obsolete helper").unwrap();
        let helper = Executable::named(&source, &settings, " - Linux").unwrap();
        let result = Command::new(&helper).output().unwrap();
        assert!(result.status.success());
        assert_eq!(
            Path::new(std::str::from_utf8(&result.stdout).unwrap())
                .file_name()
                .unwrap(),
            "Vmux VM (Docker) - Linux"
        );
    }

    #[test]
    fn dropping_a_stopping_supervised_child_waits_for_its_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("cleaned");
        let mut command = Command::new("sh");
        command
            .args(["-c", "read line; sleep 0.1; touch \"$1\"", "cleanup"])
            .arg(&marker);
        let mut process = Process::supervised(&mut command).unwrap();
        process.terminate().unwrap();
        drop(process);
        assert!(marker.exists());
    }

    #[test]
    fn closing_the_parent_channel_reaps_the_network_child() {
        let (input, parent) = UnixStream::pair().unwrap();
        let child = Command::new("sleep").arg("60").spawn().unwrap();
        let pid = child.id();
        let watcher = std::thread::spawn(move || NetworkProxy::watch(child, input));
        drop(parent);
        watcher.join().unwrap().unwrap();
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }
}
