use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;

pub struct Directory {
    path: PathBuf,
}

impl Directory {
    pub fn open(path: PathBuf) -> Result<Option<Self>> {
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => Ok(Some(Self { path })),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct Profile {
    directory: Directory,
}

impl Profile {
    pub fn open(path: PathBuf) -> Result<Option<Self>> {
        match Directory::open(path)? {
            Some(directory) => Ok(Some(Self { directory })),
            None => Ok(None),
        }
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    pub fn cache_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for name in [".fingerprint", "deps", "build", "incremental", "examples"] {
            if let Some(directory) = Directory::open(self.path().join(name))? {
                paths.push(directory.path().to_path_buf());
            }
        }
        Ok(paths)
    }
}
