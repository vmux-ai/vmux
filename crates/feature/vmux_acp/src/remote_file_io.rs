use std::fs::{DirEntry, FileType, Metadata, ReadDir};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use bevy::prelude::Component;

#[derive(Component, Clone)]
pub(super) struct Home {
    pub raw: PathBuf,
    pub canonical: PathBuf,
}

impl Home {
    pub(super) fn read() -> Result<Self, String> {
        let raw = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is unavailable")?;
        let canonical = raw.canonicalize().map_err(|error| error.to_string())?;
        Ok(Self { raw, canonical })
    }
}

#[derive(Component)]
pub(super) struct FileFacts {
    pub requested: PathBuf,
    pub canonical: Option<PathBuf>,
    pub metadata: Option<Metadata>,
}

impl FileFacts {
    pub(super) fn read(requested: PathBuf) -> Self {
        let canonical = requested.canonicalize().ok();
        let metadata = canonical.as_ref().and_then(|path| path.metadata().ok());
        Self {
            requested,
            canonical,
            metadata,
        }
    }

    pub(super) fn mime(path: &Path) -> String {
        if let Some(mime) = vmux_api::media::MediaKind::mime(&path.to_string_lossy()) {
            return mime.to_string();
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "tif" | "tiff" => "image/tiff",
            "heic" | "heif" => "image/heic",
            "json" => "application/json",
            "csv" => "text/csv",
            "html" | "htm" => "text/html",
            "md" | "markdown" => "text/markdown",
            "txt" | "rs" | "toml" | "ron" | "yaml" | "yml" | "js" | "ts" | "tsx" | "jsx"
            | "css" | "sh" | "zsh" | "bash" | "py" | "go" | "c" | "h" | "cc" | "cpp" | "hpp"
            | "java" | "kt" | "swift" => "text/plain",
            "zip" => "application/zip",
            "gz" => "application/gzip",
            "tar" => "application/x-tar",
            _ => "application/octet-stream",
        }
        .to_string()
    }

    pub(super) fn batch(paths: Vec<(usize, PathBuf)>) -> Vec<(usize, Self)> {
        let mut facts = Vec::new();
        for (index, path) in paths {
            facts.push((index, Self::read(path)));
        }
        facts
    }
}

pub(super) struct Directory {
    pub path: PathBuf,
    entries: Mutex<ReadDir>,
}

impl Directory {
    pub(super) fn open(path: PathBuf) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let entries = std::fs::read_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self {
            path,
            entries: Mutex::new(entries),
        })
    }

    pub(super) fn batch(&self, limit: usize) -> Result<DirectoryBatch, String> {
        let mut directory = self.entries.lock().map_err(|error| error.to_string())?;
        let mut entries = Vec::new();
        let mut ended = false;
        for _ in 0..limit {
            let Some(entry) = directory.next() else {
                ended = true;
                break;
            };
            if let Ok(entry) = entry {
                entries.push(EntryFacts::read(entry));
            }
        }
        Ok(DirectoryBatch { entries, ended })
    }
}

pub(super) struct DirectoryBatch {
    pub entries: Vec<EntryFacts>,
    pub ended: bool,
}

pub(super) struct EntryFacts {
    pub path: PathBuf,
    pub name: String,
    pub file_type: Option<FileType>,
}

impl EntryFacts {
    fn read(entry: DirEntry) -> Self {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_type = entry.file_type().ok();
        Self {
            path,
            name,
            file_type,
        }
    }
}

pub(super) struct Thumbnail;

impl Thumbnail {
    pub(super) fn read(path: PathBuf, limit: u64, edge: u32) -> String {
        let Ok(file) = std::fs::File::open(path) else {
            return String::new();
        };
        let mut bytes = Vec::new();
        if file.take(limit + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > limit {
            return String::new();
        }
        let Ok(image) = image::load_from_memory(&bytes) else {
            return String::new();
        };
        let thumbnail = image.thumbnail(edge, edge);
        let mut output = std::io::Cursor::new(Vec::new());
        if thumbnail
            .write_to(&mut output, image::ImageFormat::Png)
            .is_err()
        {
            return String::new();
        }
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(output.into_inner())
        )
    }
}
