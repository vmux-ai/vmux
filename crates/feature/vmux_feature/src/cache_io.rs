use std::fs::{File, OpenOptions};
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vmux_ecs::Job;

pub(crate) struct Writer {
    path: PathBuf,
    lock: Mutex<File>,
}

impl Writer {
    #[cfg(test)]
    pub(crate) fn pause(&self) -> std::sync::MutexGuard<'_, File> {
        self.lock.lock().unwrap()
    }

    pub(crate) fn temporary(&self, operation: Uuid) -> PathBuf {
        self.path.with_extension(format!("{operation}.tmp"))
    }
}

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(crate) struct Open {
    pub(crate) path: PathBuf,
}

pub(crate) struct Opened {
    pub(crate) writer: Arc<Writer>,
    pub(crate) bytes: Option<Vec<u8>>,
}

impl Job for Open {
    type Output = Result<Opened, String>;

    fn run(self) -> Self::Output {
        self.open().map_err(|error| error.to_string())
    }
}

impl Open {
    fn open(self) -> std::io::Result<Opened> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("feature cache needs a parent"))?;
        std::fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path.with_extension("lock"))?;
        lock.try_lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        self.cleanup_temporaries(parent)?;
        let bytes = match File::open(&self.path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                (&file).take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > 16 * 1024 * 1024 {
                    return Err(std::io::Error::other("feature cache exceeds 16 MiB"));
                }
                file.sync_all()?;
                File::open(parent)?.sync_all()?;
                Some(bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        Ok(Opened {
            writer: Arc::new(Writer {
                path: self.path,
                lock: Mutex::new(lock),
            }),
            bytes,
        })
    }

    fn cleanup_temporaries(&self, parent: &Path) -> std::io::Result<()> {
        let stem = self
            .path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| std::io::Error::other("feature cache needs a file name"))?;
        let prefix = format!("{stem}.");
        let mut removed = false;
        for entry in std::fs::read_dir(parent)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(identifier) = name
                .strip_prefix(&prefix)
                .and_then(|name| name.strip_suffix(".tmp"))
            else {
                continue;
            };
            let Ok(operation) = Uuid::parse_str(identifier) else {
                continue;
            };
            if operation.to_string() != identifier || !entry.file_type()?.is_file() {
                continue;
            }
            std::fs::remove_file(entry.path())?;
            removed = true;
        }
        if removed {
            File::open(parent)?.sync_all()?
        }
        Ok(())
    }
}

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(crate) struct Write {
    pub(crate) writer: Arc<Writer>,
    pub(crate) operation: Uuid,
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) permit: Option<Arc<AtomicBool>>,
    #[cfg(test)]
    pub(crate) fail_directory_sync: bool,
    #[cfg(test)]
    pub(crate) fail_before_replacement: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    NotInstalled,
    Replaced,
    Durable,
}

pub(crate) struct Written {
    pub(crate) stage: Stage,
    pub(crate) temporary: Option<PathBuf>,
    pub(crate) error: Option<String>,
}

impl Job for Write {
    type Output = Written;

    fn run(self) -> Self::Output {
        let mut stage = Stage::NotInstalled;
        let mut created = false;
        let result = self.write(&mut stage, &mut created);
        Written {
            stage,
            temporary: if created && stage == Stage::NotInstalled {
                Some(self.writer.temporary(self.operation))
            } else {
                None
            },
            error: result.err().map(|error| error.to_string()),
        }
    }
}

impl Write {
    fn write(&self, stage: &mut Stage, created: &mut bool) -> std::io::Result<()> {
        if self.bytes.len() > 16 * 1024 * 1024 {
            return Err(std::io::Error::other("feature cache exceeds 16 MiB"));
        }
        let _owner = self
            .writer
            .lock
            .lock()
            .map_err(|_| std::io::Error::other("feature writer lock is poisoned"))?;
        let temporary = self.writer.temporary(self.operation);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        *created = true;
        file.write_all(&self.bytes)?;
        file.sync_all()?;
        drop(file);
        if self
            .permit
            .as_ref()
            .is_some_and(|permit| !permit.load(Ordering::Acquire))
        {
            return Err(std::io::Error::other(
                "feature cache transaction was canceled",
            ));
        }
        #[cfg(test)]
        if self.fail_before_replacement {
            return Err(std::io::Error::other("injected pre-replacement failure"));
        }
        std::fs::rename(&temporary, &self.writer.path)?;
        *stage = Stage::Replaced;
        #[cfg(test)]
        if self.fail_directory_sync {
            return Err(std::io::Error::other("injected directory sync failure"));
        }
        File::open(
            self.writer
                .path
                .parent()
                .ok_or_else(|| std::io::Error::other("feature cache needs a parent"))?,
        )?
        .sync_all()?;
        *stage = Stage::Durable;
        Ok(())
    }
}

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(crate) struct Confirm {
    pub(crate) writer: Arc<Writer>,
    pub(crate) fingerprint: [u8; 32],
}

impl Job for Confirm {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        self.confirm().map_err(|error| error.to_string())
    }
}

impl Confirm {
    fn confirm(self) -> std::io::Result<()> {
        let _owner = self
            .writer
            .lock
            .lock()
            .map_err(|_| std::io::Error::other("feature writer lock is poisoned"))?;
        let file = File::open(&self.writer.path)?;
        let mut bytes = Vec::new();
        (&file).take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 * 1024
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != self.fingerprint
        {
            return Err(std::io::Error::other(
                "feature cache differs from its uncertain transaction",
            ));
        }
        file.sync_all()?;
        File::open(
            self.writer
                .path
                .parent()
                .ok_or_else(|| std::io::Error::other("feature cache needs a parent"))?,
        )?
        .sync_all()?;
        Ok(())
    }
}

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(crate) struct Cleanup {
    pub(crate) writer: Arc<Writer>,
    pub(crate) operation: Uuid,
}

impl Job for Cleanup {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        self.cleanup().map_err(|error| error.to_string())
    }
}

impl Cleanup {
    fn cleanup(self) -> std::io::Result<()> {
        let _owner = self
            .writer
            .lock
            .lock()
            .map_err(|_| std::io::Error::other("feature writer lock is poisoned"))?;
        let temporary = self.writer.temporary(self.operation);
        match std::fs::remove_file(&temporary) {
            Ok(()) => File::open(self.writer.path.parent().unwrap_or(Path::new(".")))?.sync_all(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_recovers_only_regular_temporary_files_owned_by_the_selected_cache() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("flags.json");
        let orphan = directory
            .path()
            .join(format!("flags.{}.tmp", Uuid::new_v4()));
        let unrelated = directory
            .path()
            .join(format!("other.{}.tmp", Uuid::new_v4()));
        let malformed = directory.path().join("flags.unrecognized.tmp");
        for file in [&orphan, &unrelated, &malformed] {
            std::fs::write(file, b"preserved input").unwrap()
        }
        let opened = Open { path }.run().unwrap();
        assert!(opened.bytes.is_none());
        assert!(!orphan.exists());
        assert_eq!(std::fs::read(unrelated).unwrap(), b"preserved input");
        assert_eq!(std::fs::read(malformed).unwrap(), b"preserved input");
    }

    #[test]
    fn uncertain_replacement_requires_confirmation_of_the_same_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        std::fs::write(&path, b"previous").unwrap();
        let opened = Open { path: path.clone() }.run().unwrap();
        let operation = Uuid::new_v4();
        let candidate: Arc<[u8]> = Arc::from(&b"candidate"[..]);
        let result = Write {
            writer: opened.writer.clone(),
            operation,
            bytes: candidate.clone(),
            permit: None,
            fail_directory_sync: true,
            fail_before_replacement: false,
        }
        .run();
        assert_eq!(result.stage, Stage::Replaced);
        assert!(result.error.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), &*candidate);
        assert!(
            Confirm {
                writer: opened.writer.clone(),
                fingerprint: Sha256::digest(b"wrong candidate").into()
            }
            .run()
            .is_err()
        );
        Confirm {
            writer: opened.writer.clone(),
            fingerprint: Sha256::digest(&candidate).into(),
        }
        .run()
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), &*candidate);
        assert!(!opened.writer.temporary(operation).exists());
    }

    #[test]
    fn pre_replacement_failure_preserves_current_bytes_and_owned_cleanup_allows_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        std::fs::write(&path, b"previous").unwrap();
        let opened = Open { path: path.clone() }.run().unwrap();
        let operation = Uuid::new_v4();
        let candidate: Arc<[u8]> = Arc::from(&b"candidate"[..]);
        let result = Write {
            writer: opened.writer.clone(),
            operation,
            bytes: candidate.clone(),
            permit: None,
            fail_directory_sync: false,
            fail_before_replacement: true,
        }
        .run();
        assert_eq!(result.stage, Stage::NotInstalled);
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert_eq!(
            result.temporary.as_ref(),
            Some(&opened.writer.temporary(operation))
        );
        Cleanup {
            writer: opened.writer.clone(),
            operation,
        }
        .run()
        .unwrap();
        assert!(!opened.writer.temporary(operation).exists());
        let retried = Write {
            writer: opened.writer.clone(),
            operation,
            bytes: candidate.clone(),
            permit: None,
            fail_directory_sync: false,
            fail_before_replacement: false,
        }
        .run();
        assert_eq!(retried.stage, Stage::Durable);
        assert_eq!(std::fs::read(&path).unwrap(), &*candidate);
    }
}
