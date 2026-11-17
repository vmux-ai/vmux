use std::fs::{self, File, OpenOptions, Permissions, TryLockError};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::Result;
use crate::signals::Signals;

pub struct TargetLock {
    path: PathBuf,
    file: Option<File>,
}

impl TargetLock {
    pub fn try_acquire(root: &Path, target: &Path) -> Result<Option<Self>> {
        let path = Self::path(root, target)?;
        if cfg!(target_os = "macos") {
            let acquired = Command::new("shlock")
                .arg("-f")
                .arg(&path)
                .arg("-p")
                .arg(std::process::id().to_string())
                .status()?;
            return Ok(if acquired.success() {
                Some(Self { path, file: None })
            } else {
                None
            });
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self {
                path,
                file: Some(file),
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub fn acquire(root: &Path, target: &Path) -> Result<Self> {
        Self::check()?;
        let path = Self::path(root, target)?;
        let mut waiting = false;
        let file = if cfg!(target_os = "macos") {
            loop {
                Self::check()?;
                let acquired = Command::new("shlock")
                    .arg("-f")
                    .arg(&path)
                    .arg("-p")
                    .arg(std::process::id().to_string())
                    .status()?;
                if acquired.success() {
                    break;
                }
                Self::waiting(target, &mut waiting);
                thread::sleep(Duration::from_millis(100));
            }
            None
        } else {
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&path)?;
            loop {
                Self::check()?;
                match file.try_lock() {
                    Ok(()) => break,
                    Err(TryLockError::WouldBlock) => {
                        Self::waiting(target, &mut waiting);
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(TryLockError::Error(error)) => return Err(error.into()),
                }
            }
            Some(file)
        };
        let lease = Self { path, file };
        Self::check()?;
        Ok(lease)
    }

    fn check() -> Result<()> {
        Signals::check()?;
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err("Target lock acquisition canceled".into());
        }
        Ok(())
    }

    fn waiting(target: &Path, waiting: &mut bool) {
        if !*waiting {
            eprintln!("waiting for target cache lock: {}", target.display());
            *waiting = true;
        }
    }

    fn path(root: &Path, target: &Path) -> Result<PathBuf> {
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
        let key = String::from_utf8(output.stdout)?;
        let uid = Command::new("id").arg("-u").output()?;
        if !uid.status.success() {
            return Err("Cannot determine target lock owner".into());
        }
        let uid = String::from_utf8(uid.stdout)?;
        let lock_root = match std::env::var_os("VMUX_TARGET_LOCK_ROOT") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => {
                let temporary = match std::env::var_os("TMPDIR") {
                    Some(path) if !path.is_empty() => PathBuf::from(path),
                    _ => PathBuf::from("/tmp"),
                };
                temporary.join(format!("vmux-target-locks-{}", uid.trim()))
            }
        };
        if fs::symlink_metadata(&lock_root).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("Unsafe target lock root".into());
        }
        fs::create_dir_all(&lock_root)?;
        fs::set_permissions(&lock_root, Permissions::from_mode(0o700))?;
        Ok(lock_root.join(format!("{}.lock", key.trim())))
    }
}

impl Drop for TargetLock {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.unlock();
            return;
        }
        if fs::read_to_string(&self.path)
            .is_ok_and(|owner| owner.trim() == std::process::id().to_string())
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releasing_a_target_lock_unlocks_a_still_open_duplicate_handle() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("target.lock");
        let file = File::create(&path).unwrap();
        file.try_lock().unwrap();
        let duplicate = file.try_clone().unwrap();
        let lock = TargetLock {
            path: path.clone(),
            file: Some(file),
        };
        let contender = OpenOptions::new().write(true).open(&path).unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        drop(lock);
        contender.try_lock().unwrap();
        contender.unlock().unwrap();
        drop(duplicate);
    }
}
