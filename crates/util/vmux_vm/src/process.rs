use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

use bevy_ecs::prelude::Component;

use crate::Settings;

#[derive(Component)]
pub(crate) struct Process(Child);

impl Process {
    pub(crate) fn spawn(command: &mut Command) -> io::Result<Self> {
        Ok(Self(command.stdin(Stdio::null()).spawn()?))
    }

    pub(crate) fn poll(&mut self) -> io::Result<Option<ExitStatus>> {
        self.0.try_wait()
    }

    pub(crate) fn terminate(&mut self) -> io::Result<()> {
        if self.poll()?.is_none() {
            self.0.kill()?;
        }
        Ok(())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.poll(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Component)]
pub(crate) struct Lease(File);

impl Lease {
    pub(crate) fn acquire(directory: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(directory.join("engine.lock"))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
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
