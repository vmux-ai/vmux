use std::fs::{self, Permissions};
#[cfg(feature = "development")]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(feature = "development")]
use std::path::PathBuf;
#[cfg(feature = "development")]
use std::process::{Command, Stdio};

#[cfg(feature = "development")]
use crate::Result;

pub(crate) struct DxFsIo;

impl DxFsIo {
    #[cfg(feature = "development")]
    pub(crate) fn read(path: &Path) -> std::io::Result<Vec<u8>> {
        fs::read(path)
    }

    #[cfg(feature = "development")]
    pub(crate) fn read_to_string(path: &Path) -> std::io::Result<String> {
        fs::read_to_string(path)
    }

    #[cfg(feature = "development")]
    pub(crate) fn write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        fs::write(path, contents)
    }

    #[cfg(feature = "development")]
    pub(crate) fn copy(from: &Path, to: &Path) -> std::io::Result<u64> {
        fs::copy(from, to)
    }

    #[cfg(feature = "development")]
    pub(crate) fn create_dir_all(path: &Path) -> std::io::Result<()> {
        fs::create_dir_all(path)
    }

    pub(crate) fn create_private_dir(path: &Path) -> std::io::Result<()> {
        fs::create_dir(path)?;
        #[cfg(unix)]
        fs::set_permissions(path, Permissions::from_mode(0o700))?;
        Ok(())
    }

    #[cfg(feature = "development")]
    pub(crate) fn ensure_private_dir_all(path: &Path) -> Result<()> {
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("Unsafe target lock root".into());
        }
        fs::create_dir_all(path)?;
        #[cfg(unix)]
        fs::set_permissions(path, Permissions::from_mode(0o700))?;
        Ok(())
    }

    #[cfg(feature = "development")]
    pub(crate) fn is_regular_non_symlink_file(path: &Path) -> bool {
        fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    }

    #[cfg(feature = "development")]
    pub(crate) fn canonical_non_symlink_dir(path: &Path) -> Option<PathBuf> {
        let metadata = fs::symlink_metadata(path).ok()?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            path.canonicalize().ok()
        } else {
            None
        }
    }

    #[cfg(feature = "development")]
    pub(crate) fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let target = destination.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                Self::copy_dir_recursive(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }

    #[cfg(feature = "development")]
    pub(crate) fn shlock(path: &Path) -> std::io::Result<bool> {
        let status = Command::new("shlock")
            .arg("-f")
            .arg(path)
            .arg("-p")
            .arg(std::process::id().to_string())
            .status()?;
        Ok(status.success())
    }

    #[cfg(feature = "development")]
    pub(crate) fn open_lock(path: &Path) -> std::io::Result<fs::File> {
        fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)
    }

    #[cfg(feature = "development")]
    pub(crate) fn try_lock_file(file: &fs::File) -> std::io::Result<bool> {
        match file.try_lock() {
            Ok(()) => Ok(true),
            Err(fs::TryLockError::WouldBlock) => Ok(false),
            Err(fs::TryLockError::Error(error)) => Err(error),
        }
    }

    #[cfg(feature = "development")]
    pub(crate) fn git_hash_target(root: &Path, target: &Path) -> Result<String> {
        let mut git = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["hash-object", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        {
            let mut input = git.stdin.take().ok_or("Missing lock hash input")?;
            input.write_all(target.as_os_str().as_encoded_bytes())?;
            input.write_all(b"\n")?;
        }
        let output = git.wait_with_output()?;
        if !output.status.success() {
            return Err("Cannot compute target lock key".into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    #[cfg(feature = "development")]
    pub(crate) fn current_uid() -> Result<String> {
        let uid = Command::new("id").arg("-u").output()?;
        if !uid.status.success() {
            return Err("Cannot determine target lock owner".into());
        }
        Ok(String::from_utf8(uid.stdout)?)
    }
}
