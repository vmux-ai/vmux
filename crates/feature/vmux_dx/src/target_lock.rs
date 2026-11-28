use std::fs::{self, File};
use std::path::{Path, PathBuf};
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
            let acquired = crate::fs_io::DxFsIo::shlock(&path)?;
            return Ok(if acquired {
                Some(Self { path, file: None })
            } else {
                None
            });
        }
        let file = crate::fs_io::DxFsIo::open_lock(&path)?;
        if crate::fs_io::DxFsIo::try_lock_file(&file)? {
            Ok(Some(Self {
                path,
                file: Some(file),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn acquire(root: &Path, target: &Path) -> Result<Self> {
        Self::check()?;
        let path = Self::path(root, target)?;
        let mut waiting = false;
        let file = if cfg!(target_os = "macos") {
            loop {
                Self::check()?;
                if crate::fs_io::DxFsIo::shlock(&path)? {
                    break;
                }
                Self::waiting(target, &mut waiting);
                thread::sleep(Duration::from_millis(100));
            }
            None
        } else {
            let file = crate::fs_io::DxFsIo::open_lock(&path)?;
            loop {
                Self::check()?;
                if crate::fs_io::DxFsIo::try_lock_file(&file)? {
                    break;
                }
                Self::waiting(target, &mut waiting);
                thread::sleep(Duration::from_millis(100));
            }
            Some(file)
        };
        let lease = Self { path, file };
        Self::check()?;
        Ok(lease)
    }

    fn check() -> Result<()> {
        Signals::check()?;
        if vmux_job::blocking::BlockingCancellation::canceled() {
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
        let key = crate::fs_io::DxFsIo::git_hash_target(root, target)?;
        let uid = crate::fs_io::DxFsIo::current_uid()?;
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
        crate::fs_io::DxFsIo::ensure_private_dir_all(&lock_root)?;
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
    use std::fs::{OpenOptions, TryLockError};

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
