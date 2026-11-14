use std::ffi::CString;
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Settings;

pub(crate) struct Root(File);

impl Root {
    pub(crate) fn open(settings: &Settings) -> io::Result<Self> {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&settings.directory)?;
        Self(directory).child(c"rootfs", false)
    }

    pub(crate) fn directory(&self, path: &str, mode: u32) -> io::Result<()> {
        let mut directory = Self(self.0.try_clone()?);
        for part in Path::new(path).components() {
            let Component::Normal(part) = part else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid guest path",
                ));
            };
            let name = CString::new(part.as_bytes()).map_err(io::Error::other)?;
            directory = directory.child(&name, true)?;
        }
        directory.0.set_permissions(Permissions::from_mode(mode))
    }

    pub(crate) fn write(&self, path: &str, bytes: &[u8], mode: u32) -> io::Result<()> {
        let path = Path::new(path);
        let mut directory = Self(self.0.try_clone()?);
        for part in path
            .parent()
            .ok_or_else(|| io::Error::other("missing guest parent"))?
            .components()
        {
            let Component::Normal(part) = part else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid guest path",
                ));
            };
            directory = directory.child(
                &CString::new(part.as_bytes()).map_err(io::Error::other)?,
                true,
            )?;
        }
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("missing guest filename"))?;
        let name = CString::new(name.as_bytes()).map_err(io::Error::other)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let temporary = CString::new(format!(".vmux-{}-{nonce}", std::process::id()))
            .map_err(io::Error::other)?;
        let fd = unsafe {
            libc::openat(
                directory.0.as_raw_fd(),
                temporary.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let result = (|| {
            file.write_all(bytes)?;
            file.set_permissions(Permissions::from_mode(mode))?;
            file.sync_all()?;
            if unsafe {
                libc::renameat(
                    directory.0.as_raw_fd(),
                    temporary.as_ptr(),
                    directory.0.as_raw_fd(),
                    name.as_ptr(),
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        })();
        if result.is_err() {
            unsafe { libc::unlinkat(directory.0.as_raw_fd(), temporary.as_ptr(), 0) };
        }
        result
    }

    fn child(&self, name: &std::ffi::CStr, create: bool) -> io::Result<Self> {
        if create && unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o755) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error);
            }
        }
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(unsafe { File::from_raw_fd(fd) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn guest_leaf_links_are_replaced_without_modifying_host_targets() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::new(directory.path().join("engine"), directory.path());
        std::fs::create_dir_all(settings.rootfs().join("sbin")).unwrap();
        let victim = directory.path().join("victim");
        std::fs::write(&victim, b"host content").unwrap();
        let target = settings.rootfs().join("sbin/vmux-init");
        symlink(&victim, &target).unwrap();
        Root::open(&settings)
            .unwrap()
            .write("sbin/vmux-init", b"guest init", 0o755)
            .unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"host content");
        assert_eq!(std::fs::read(&target).unwrap(), b"guest init");
        assert!(!std::fs::symlink_metadata(target).unwrap().is_symlink());
    }

    #[test]
    fn guest_parent_and_root_links_cannot_redirect_host_preparation() {
        for redirect_root in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let settings = Settings::new(directory.path().join("engine"), directory.path());
            std::fs::create_dir_all(&settings.directory).unwrap();
            let outside = directory.path().join("outside");
            std::fs::create_dir(&outside).unwrap();
            let victim = outside.join("authorized_keys");
            std::fs::write(&victim, b"host keys").unwrap();
            if redirect_root {
                symlink(&outside, settings.rootfs()).unwrap();
                assert!(Root::open(&settings).is_err());
            } else {
                std::fs::create_dir_all(settings.rootfs().join("root")).unwrap();
                symlink(&outside, settings.rootfs().join("root/.ssh")).unwrap();
                let root = Root::open(&settings).unwrap();
                assert!(
                    root.write("root/.ssh/authorized_keys", b"guest keys", 0o600)
                        .is_err()
                );
                assert!(root.directory("root/.ssh", 0o700).is_err());
            }
            assert_eq!(std::fs::read(victim).unwrap(), b"host keys");
        }
    }
}
