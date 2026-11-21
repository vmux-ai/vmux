use std::ffi::OsString;
use std::fs::{FileType, OpenOptions};
use std::io::{ErrorKind, Read};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use bevy_ecs::prelude::Component;
use vmux_ecs::{BlockingCancellation, Job};

#[derive(Clone)]
pub struct ReadFile {
    pub path: PathBuf,
    pub limit: usize,
}

impl Job for ReadFile {
    type Output = Result<FileBytes, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("file read canceled".into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut file = match options.open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(FileBytes(None)),
            Err(error) => return Err(error.to_string()),
        };
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err(format!(
                "file is not a regular file: {}",
                self.path.display()
            ));
        }
        let limit = self.limit.min(64 * 1024 * 1024);
        if metadata.len() > limit as u64 {
            return Err(format!(
                "file exceeds {limit} bytes: {}",
                self.path.display()
            ));
        }
        let mut bytes = Vec::new();
        let mut buffer = [0; 65536];
        loop {
            if BlockingCancellation::canceled() {
                return Err("file read canceled".into());
            }
            let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if bytes.len().saturating_add(read) > limit {
                return Err(format!(
                    "file exceeds {limit} bytes: {}",
                    self.path.display()
                ));
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        Ok(FileBytes(Some(bytes)))
    }
}

#[derive(Component)]
pub struct FileBytes(pub Option<Vec<u8>>);

#[derive(Clone)]
pub struct ReadDirectory {
    pub path: PathBuf,
}

impl Job for ReadDirectory {
    type Output = Result<DirectoryEntries, String>;

    fn run(self) -> Self::Output {
        let directory = match std::fs::read_dir(&self.path) {
            Ok(directory) => directory,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(DirectoryEntries {
                    exists: false,
                    entries: Vec::new(),
                });
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut entries = Vec::new();
        for entry in directory {
            if BlockingCancellation::canceled() {
                return Err("directory read canceled".into());
            }
            if entries.len() == 4096 {
                return Err(format!(
                    "directory exceeds 4096 entries: {}",
                    self.path.display()
                ));
            }
            let entry = entry.map_err(|error| error.to_string())?;
            entries.push(DirectoryEntry {
                path: entry.path(),
                name: entry.file_name(),
                file_type: entry.file_type().map_err(|error| error.to_string())?,
            });
        }
        Ok(DirectoryEntries {
            exists: true,
            entries,
        })
    }
}

#[derive(Component)]
pub struct DirectoryEntries {
    pub exists: bool,
    pub entries: Vec<DirectoryEntry>,
}

pub struct DirectoryEntry {
    pub path: PathBuf,
    pub name: OsString,
    pub file_type: FileType,
}

#[derive(Clone)]
pub struct WriteFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

impl Job for WriteFile {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("file write canceled".into());
        }
        if self.bytes.len() > 64 * 1024 * 1024 {
            return Err("file write exceeds 67108864 bytes".into());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        vmux_path::AtomicFile::write(&self.path, &self.bytes).map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct InspectLink {
    pub source: PathBuf,
    pub target: PathBuf,
}

impl Job for InspectLink {
    type Output = Result<LinkFacts, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("link inspection canceled".into());
        }
        let source = vmux_path::PathIdentity::resolve(&self.source)
            .as_path()
            .to_path_buf();
        let metadata = match self.target.symlink_metadata() {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        let target_type = metadata.as_ref().map(|metadata| metadata.file_type());
        let target = if target_type.is_some_and(|kind| kind.is_symlink()) {
            match std::fs::read_link(&self.target) {
                Ok(path) => {
                    let path = if path.is_absolute() {
                        path
                    } else {
                        self.target
                            .parent()
                            .unwrap_or(std::path::Path::new("/"))
                            .join(path)
                    };
                    Some(
                        vmux_path::PathIdentity::resolve(&path)
                            .as_path()
                            .to_path_buf(),
                    )
                }
                Err(_) => None,
            }
        } else {
            None
        };
        Ok(LinkFacts {
            source,
            target_type,
            target,
        })
    }
}

#[derive(Component)]
pub struct LinkFacts {
    pub source: PathBuf,
    pub target_type: Option<FileType>,
    pub target: Option<PathBuf>,
}

#[derive(Clone)]
pub struct FindExecutable {
    pub command: String,
    pub explicit: Option<PathBuf>,
}

impl Job for FindExecutable {
    type Output = Result<ExecutableFacts, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("executable lookup canceled".into());
        }
        let path = vmux_path::Executable::find(&self.command).map(vmux_path::Executable::into_path);
        let explicit = self
            .explicit
            .as_ref()
            .and_then(|path| path.symlink_metadata().ok())
            .map(|metadata| metadata.file_type());
        Ok(ExecutableFacts { path, explicit })
    }
}

#[derive(Component)]
pub struct ExecutableFacts {
    pub path: Option<PathBuf>,
    pub explicit: Option<FileType>,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;
    use std::time::Duration;

    #[test]
    fn regular_file_observation_preserves_bytes_links_absence_and_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let bytes = b"{\"key\":\"value\"}\n\0";
        std::fs::write(&path, bytes).unwrap();
        let link = directory.path().join("linked.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        for source in [path.clone(), link] {
            let result = ReadFile {
                path: source,
                limit: bytes.len(),
            }
            .run()
            .unwrap();
            assert_eq!(result.0.as_deref(), Some(bytes.as_slice()));
        }
        let result = ReadFile {
            path: directory.path().join("absent.json"),
            limit: 1024,
        }
        .run()
        .unwrap();
        assert!(result.0.is_none());
        assert!(
            ReadFile {
                path,
                limit: bytes.len() - 1
            }
            .run()
            .is_err()
        );
        assert!(
            ReadFile {
                path: directory.path().into(),
                limit: 1024
            }
            .run()
            .is_err()
        );
    }

    #[test]
    fn file_reads_refuse_a_fifo_without_waiting_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (started, starting) = std::sync::mpsc::sync_channel(1);
        let (completed, completion) = std::sync::mpsc::sync_channel(1);
        let input = ReadFile {
            path: path.clone(),
            limit: 1024,
        };
        let reader = std::thread::spawn(move || {
            started.send(()).unwrap();
            completed.send(input.run()).unwrap();
        });
        starting.recv_timeout(Duration::from_secs(2)).unwrap();
        let early = completion.recv_timeout(Duration::from_secs(1));
        let returned = early.is_ok();
        let result = match early {
            Ok(result) => result,
            Err(_) => {
                let deadline = std::time::Instant::now() + Duration::from_secs(2);
                loop {
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(&path)
                    {
                        Ok(writer) => {
                            drop(writer);
                            break;
                        }
                        Err(error)
                            if error.raw_os_error() == Some(libc::ENXIO)
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("FIFO reader cleanup failed: {error}"),
                    }
                }
                completion.recv_timeout(Duration::from_secs(2)).unwrap()
            }
        };
        reader.join().unwrap();
        assert!(returned, "file observation must not wait for FIFO peers");
        assert!(result.is_err(), "a FIFO is not a configuration file");
    }
}
