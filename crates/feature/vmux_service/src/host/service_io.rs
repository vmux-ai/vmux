#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use vmux_profile::ServicePaths;

pub(crate) struct ServiceFsIo;

impl ServiceFsIo {
    pub(crate) fn read_to_string(path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub(crate) fn write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        std::fs::write(path, contents)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn create_dir_all(path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    pub(crate) fn remove_file(path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    pub(crate) fn read_dir_paths(path: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(path)? {
            paths.push(entry?.path());
        }
        Ok(paths)
    }

    pub(crate) fn lock_desktop_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
        let file = Self::open_lock_file(path)?;
        file.try_lock().map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!("Quit Vmux before stopping its service: {error}"),
            )
        })?;
        Ok(file)
    }

    #[cfg(unix)]
    pub(crate) fn is_socket(path: &Path) -> std::io::Result<Option<bool>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Ok(Some(metadata.file_type().is_socket())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn open_nonblocking_lock(path: &Path) -> std::io::Result<Option<std::fs::File>> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        match options.open(path) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
    }

    pub(crate) fn lock_is_file(file: &std::fs::File) -> std::io::Result<bool> {
        Ok(file.metadata()?.is_file())
    }

    pub(crate) fn try_lock_and_unlock(file: &std::fs::File) -> std::io::Result<bool> {
        match file.try_lock() {
            Ok(()) => {
                file.unlock()?;
                Ok(true)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(false),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }

    #[cfg(unix)]
    pub(crate) fn connect_unix(path: &Path) -> std::io::Result<UnixStream> {
        UnixStream::connect(path)
    }

    #[cfg(unix)]
    pub(crate) fn spawn_detached(binary: &Path) -> std::io::Result<Child> {
        let log_dir = ServicePaths::log_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let stderr = match std::fs::File::create(ServicePaths::current().log()) {
            Ok(file) => Stdio::from(file),
            Err(error) => {
                tracing::warn!(%error, "could not create service log; stderr will be discarded");
                Stdio::null()
            }
        };
        let mut command = Command::new(binary);
        vmux_spawn::Descriptors::isolate(&mut command)?;
        let child = unsafe {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(stderr)
                .pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                })
                .spawn()?
        };
        Ok(child)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn run_launchctl(
        args: &[&std::ffi::OsStr],
    ) -> std::io::Result<std::process::ExitStatus> {
        Command::new("launchctl").args(args).status()
    }
}
