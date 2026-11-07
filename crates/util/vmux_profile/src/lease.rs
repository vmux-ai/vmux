use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct DataLease {
    locks: Vec<File>,
}

impl DataLease {
    pub fn desktop() -> std::io::Result<Self> {
        let paths = crate::ProfilePaths::current();
        let mut lease = Self { locks: Vec::new() };
        lease.exclusive(&paths.shared_data().join("desktop.lock"))?;
        lease.join(&paths.shared_data(), &Self::owner())?;
        paths.prepare_browser_profile()?;
        Ok(lease)
    }

    pub fn daemon() -> std::io::Result<Self> {
        let paths = crate::ProfilePaths::current();
        let mut lease = Self { locks: Vec::new() };
        lease.join(&paths.shared_data(), &Self::owner())?;
        lease.exclusive(&crate::ServicePaths::current().daemon_lock())?;
        Ok(lease)
    }

    fn owner() -> String {
        format!("{}:{}", crate::build_profile(), env!("VMUX_WORKTREE_ID"))
    }

    fn open(path: &Path) -> std::io::Result<File> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
    }

    fn exclusive(&mut self, path: &Path) -> std::io::Result<()> {
        let file = Self::open(path)?;
        file.try_lock().map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!(
                    "{} is in use: {error}. Quit the other Vmux instance first.",
                    path.display()
                ),
            )
        })?;
        self.locks.push(file);
        Ok(())
    }

    fn join(&mut self, data: &Path, owner: &str) -> std::io::Result<()> {
        let path = data.join("runtime.lock");
        let mut file = Self::open(&path)?;
        match file.try_lock() {
            Ok(()) => {
                file.set_len(0)?;
                file.write_all(owner.as_bytes())?;
                file.flush()?;
                file.unlock()?;
            }
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        file.try_lock_shared()
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::WouldBlock, error))?;
        file.seek(SeekFrom::Start(0))?;
        let mut active = String::new();
        file.read_to_string(&mut active)?;
        if active != owner {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                format!(
                    "Vmux data is held by {active}. Quit its windows and stop its service before switching builds."
                ),
            ));
        }
        self.locks.push(file);
        Ok(())
    }
}

impl Drop for DataLease {
    fn drop(&mut self) {
        for file in &self.locks {
            let _ = file.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_owner_can_join_but_another_build_waits_for_every_holder() {
        let directory = tempfile::tempdir().unwrap();
        let mut desktop = DataLease { locks: Vec::new() };
        desktop.join(directory.path(), "local:worktree-a").unwrap();
        let mut daemon = DataLease { locks: Vec::new() };
        daemon.join(directory.path(), "local:worktree-a").unwrap();
        let mut next = DataLease { locks: Vec::new() };
        assert!(next.join(directory.path(), "dev:worktree-b").is_err());
        drop(desktop);
        assert!(next.join(directory.path(), "dev:worktree-b").is_err());
        drop(daemon);
        next.join(directory.path(), "dev:worktree-b").unwrap();
    }

    #[test]
    fn exclusive_lease_prevents_duplicate_writers_without_truncating_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("desktop.lock");
        std::fs::write(&path, "existing owner").unwrap();
        let mut first = DataLease { locks: Vec::new() };
        first.exclusive(&path).unwrap();
        let mut second = DataLease { locks: Vec::new() };
        assert!(second.exclusive(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing owner");
        drop(first);
        second.exclusive(&path).unwrap();
    }
}
