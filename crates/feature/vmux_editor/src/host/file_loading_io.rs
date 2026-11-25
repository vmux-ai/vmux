use std::io::{self, Read};
use std::path::{Path, PathBuf};

use vmux_ecs::event::FileDirEntry;
use vmux_job::Job;

#[derive(Clone)]
pub(super) struct Inspect {
    pub path: PathBuf,
}

impl Inspect {
    pub(super) fn modified_at(path: &Path) -> Option<std::time::SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }
}

impl Job for Inspect {
    type Output = io::Result<std::fs::Metadata>;

    fn run(self) -> Self::Output {
        std::fs::metadata(self.path)
    }
}

#[derive(Clone)]
pub(super) struct Directory {
    pub path: PathBuf,
}

impl Job for Directory {
    type Output = io::Result<Vec<FileDirEntry>>;

    fn run(self) -> Self::Output {
        let directory = std::fs::read_dir(self.path)?;
        let mut entries = Vec::new();
        for entry in directory {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            let is_dir = match entry.file_type() {
                Ok(kind) if kind.is_dir() => true,
                Ok(kind) if kind.is_symlink() => {
                    std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir())
                }
                _ => false,
            };
            entries.push(FileDirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                is_dir,
            });
        }
        Ok(entries)
    }
}

#[derive(Clone)]
pub(super) struct Bytes {
    pub path: PathBuf,
    pub maximum: u64,
}

impl Job for Bytes {
    type Output = io::Result<Vec<u8>>;

    fn run(self) -> Self::Output {
        let file = std::fs::File::open(self.path)?;
        let mut bytes = Vec::new();
        file.take(self.maximum.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > self.maximum {
            return Err(io::Error::other(format!(
                "file too large (max {} bytes)",
                self.maximum
            )));
        }
        Ok(bytes)
    }
}
