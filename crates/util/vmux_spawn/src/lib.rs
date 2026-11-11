#![cfg(unix)]

use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub struct Descriptors;

impl Descriptors {
    pub fn isolate(command: &mut Command) -> io::Result<()> {
        let limit = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
        if limit < 0 {
            return Err(io::Error::last_os_error());
        }
        unsafe {
            command.pre_exec(move || {
                for descriptor in 3..limit.min(i32::MAX as libc::c_long) as i32 {
                    let flags = libc::fcntl(descriptor, libc::F_GETFD);
                    if flags >= 0
                        && libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC) < 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }
}

pub struct ProcessGroup(libc::pid_t);

impl ProcessGroup {
    pub fn of(pid: u32) -> io::Result<Self> {
        let pid = libc::pid_t::try_from(pid).map_err(|_| io::Error::other("invalid child PID"))?;
        if pid <= 1 {
            return Err(io::Error::other("invalid child PID"));
        }
        Ok(Self(pid))
    }

    pub fn terminate(&self) -> io::Result<()> {
        self.signal(libc::SIGTERM)
    }

    pub fn kill(&self) -> io::Result<()> {
        self.signal(libc::SIGKILL)
    }

    fn signal(&self, signal: i32) -> io::Result<()> {
        if unsafe { libc::kill(-self.0, signal) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn isolated_child_cannot_access_a_parent_descriptor_that_would_otherwise_be_inherited() {
        let file = tempfile::tempfile().unwrap();
        let descriptor = file.as_raw_fd();
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) },
            0
        );
        let path = format!("/dev/fd/{descriptor}");
        assert!(
            Command::new("/bin/test")
                .args(["-e", &path])
                .status()
                .unwrap()
                .success()
        );
        let mut child = Command::new("/bin/test");
        child.args(["-e", &path]);
        Descriptors::isolate(&mut child).unwrap();
        assert!(!child.status().unwrap().success());
    }

    #[test]
    fn isolated_spawn_still_reports_an_executable_that_does_not_exist() {
        let directory = tempfile::tempdir().unwrap();
        let mut command = Command::new(directory.path().join("missing"));
        Descriptors::isolate(&mut command).unwrap();
        assert_eq!(command.spawn().unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
