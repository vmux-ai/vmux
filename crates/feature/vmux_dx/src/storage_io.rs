use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Output;
use std::sync::{Arc, Mutex};

use bevy_ecs::prelude::Component;
use vmux_ecs::blocking::Lane;
use vmux_ecs::jobs::Job;

use crate::environment::Environment;
use crate::execution::{Execution, Failure};

#[derive(Component, Default)]
pub(crate) struct Io;

pub(crate) trait StorageJob: Job<Output = Result<Self::Data, Failure>> + Component {
    type Data: Component + Clone;
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct Inspect(pub PathBuf);

impl StorageJob for Inspect {
    type Data = PathMetadata;
}

impl Job for Inspect {
    type Output = Result<PathMetadata, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        match fs::symlink_metadata(self.0) {
            Ok(metadata) => Ok(PathMetadata(Some(metadata))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(PathMetadata(None)),
            Err(error) => Err(Failure::message(error.to_string())),
        }
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct Canonicalize(pub PathBuf);

impl StorageJob for Canonicalize {
    type Data = CanonicalPath;
}

impl Job for Canonicalize {
    type Output = Result<CanonicalPath, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        self.0
            .canonicalize()
            .map(CanonicalPath)
            .map_err(|error| Failure::message(error.to_string()))
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct ReadDirectory(pub PathBuf);

impl ReadDirectory {
    const MAX_ENTRIES: usize = 4096;
}

impl StorageJob for ReadDirectory {
    type Data = DirectoryEntries;
}

impl Job for ReadDirectory {
    type Output = Result<DirectoryEntries, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        let entries = fs::read_dir(&self.0).map_err(|error| Failure::message(error.to_string()))?;
        let mut observed = Vec::new();
        for entry in entries {
            if vmux_ecs::blocking::BlockingCancellation::canceled() {
                return Err(Failure::message("Storage IO canceled"));
            }
            if observed.len() >= Self::MAX_ENTRIES {
                return Err(Failure::message(format!(
                    "Storage directory {} exceeds {} entries",
                    self.0.display(),
                    Self::MAX_ENTRIES
                )));
            }
            let entry = entry.map_err(|error| Failure::message(error.to_string()))?;
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| Failure::message(error.to_string()))?;
            observed.push((path, metadata));
        }
        Ok(DirectoryEntries(observed))
    }
}

pub(crate) type DirectoryCursor = Arc<Mutex<Option<fs::ReadDir>>>;

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct ReadDirectoryBatch {
    pub path: PathBuf,
    pub cursor: Option<DirectoryCursor>,
    pub limit: usize,
}

impl ReadDirectoryBatch {
    pub(crate) const DEFAULT_BATCH: usize = 512;
    pub(crate) const DEFAULT_LIMIT: usize = 65_536;
}

impl StorageJob for ReadDirectoryBatch {
    type Data = DirectoryBatch;
}

impl Job for ReadDirectoryBatch {
    type Output = Result<DirectoryBatch, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        let cursor = match self.cursor {
            Some(cursor) => cursor,
            None => {
                let reader = fs::read_dir(&self.path)
                    .map_err(|error| Failure::message(error.to_string()))?;
                Arc::new(Mutex::new(Some(reader)))
            }
        };
        let mut observed = Vec::with_capacity(self.limit.min(Self::DEFAULT_BATCH));
        let mut eof = false;
        {
            let mut guard = cursor.lock().unwrap();
            let Some(reader) = guard.as_mut() else {
                return Ok(DirectoryBatch {
                    path: self.path,
                    entries: observed,
                    next: None,
                });
            };
            while observed.len() < self.limit {
                if vmux_ecs::blocking::BlockingCancellation::canceled() {
                    *guard = None;
                    return Err(Failure::message("Storage IO canceled"));
                }
                match reader.next() {
                    Some(Ok(entry)) => {
                        let path = entry.path();
                        let metadata = match fs::symlink_metadata(&path) {
                            Ok(metadata) => metadata,
                            Err(error) => {
                                *guard = None;
                                return Err(Failure::message(error.to_string()));
                            }
                        };
                        observed.push((path, metadata));
                    }
                    Some(Err(error)) => {
                        *guard = None;
                        return Err(Failure::message(error.to_string()));
                    }
                    None => {
                        *guard = None;
                        eof = true;
                        break;
                    }
                }
            }
        }
        Ok(DirectoryBatch {
            path: self.path,
            entries: observed,
            next: if eof { None } else { Some(cursor) },
        })
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct ReadFile(pub PathBuf);

impl StorageJob for ReadFile {
    type Data = FileBytes;
}

impl Job for ReadFile {
    type Output = Result<FileBytes, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message("Storage input must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| Failure::message(error.to_string()))?;
        if bytes.len() > 1024 * 1024 {
            return Err(Failure::message("Storage metadata exceeds 1 MiB"));
        }
        Ok(FileBytes(bytes))
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct Open(pub PathBuf, pub OpenOptions);

impl StorageJob for Open {
    type Data = OpenedFile;
}

impl Job for Open {
    type Output = Result<OpenedFile, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        let file = self
            .1
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message("Cargo lock must be a regular file"));
        }
        Ok(OpenedFile(Arc::new(file)))
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct Remove {
    pub path: PathBuf,
    pub directory: bool,
    pub lock: Option<Arc<File>>,
}

impl StorageJob for Remove {
    type Data = Done;
}

impl Job for Remove {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        let _lock = self.lock;
        let result = if self.directory {
            fs::remove_dir_all(self.path)
        } else {
            fs::remove_file(self.path)
        };
        result.map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(Io)]
pub(crate) struct Capture {
    pub environment: Environment,
    pub directory: PathBuf,
    pub program: OsString,
    pub arguments: Vec<OsString>,
}

impl StorageJob for Capture {
    type Data = ToolOutput;
}

impl Job for Capture {
    type Output = Result<ToolOutput, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Storage IO canceled"));
        }
        let mut command = self.environment.command(self.program, &self.directory);
        command.args(self.arguments);
        Execution::new(command)
            .output_any(None)
            .map(ToolOutput)
            .map_err(Failure::from_error)
    }
}

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct PathMetadata(pub Option<Metadata>);

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct CanonicalPath(pub PathBuf);

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct DirectoryEntries(pub Vec<(PathBuf, Metadata)>);

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct DirectoryBatch {
    pub path: PathBuf,
    pub entries: Vec<(PathBuf, Metadata)>,
    pub next: Option<DirectoryCursor>,
}

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct FileBytes(pub Vec<u8>);

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct OpenedFile(pub Arc<File>);

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct ToolOutput(pub Output);

#[derive(Component, Clone)]
#[require(Io)]
pub(crate) struct Done;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_directory_batch_streams_large_directories_in_bounded_chunks_until_eof() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..4200 {
            fs::write(directory.path().join(format!("dep-{index}.d")), b"").unwrap();
        }
        let mut cursor = None;
        let mut batches = 0usize;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let batch = ReadDirectoryBatch {
                path: directory.path().to_path_buf(),
                cursor,
                limit: ReadDirectoryBatch::DEFAULT_BATCH,
            }
            .run()
            .unwrap();
            assert!(batch.entries.len() <= ReadDirectoryBatch::DEFAULT_BATCH);
            batches += 1;
            for (path, _) in batch.entries {
                assert!(seen.insert(path));
            }
            match batch.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(seen.len(), 4200);
        assert!(batches >= 9);
    }
}
