#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::ffi::CString;
use std::ffi::OsString;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::ffi::OsStrExt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::io::AsRawFd;
use std::path::Path;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod other;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryObservation {
    pub exists: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
}

impl EntryObservation {
    pub fn is_missing(self) -> bool {
        !self.exists
    }

    pub fn is_directory(self) -> bool {
        self.exists && self.is_dir && !self.is_symlink
    }

    pub fn is_symlink(self) -> bool {
        self.exists && self.is_symlink
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryChild {
    pub name: OsString,
    pub is_dir: bool,
    pub is_symlink: bool,
}

impl DirectoryChild {
    pub fn is_directory(&self) -> bool {
        self.is_dir && !self.is_symlink
    }
}

pub struct NativeRelocation;

impl NativeRelocation {
    pub fn observe(path: &Path) -> io::Result<EntryObservation> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Ok(EntryObservation {
                exists: true,
                is_dir: metadata.file_type().is_dir(),
                is_symlink: metadata.file_type().is_symlink(),
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(EntryObservation {
                exists: false,
                is_dir: false,
                is_symlink: false,
            }),
            Err(error) => Err(error),
        }
    }

    pub fn read_directory_entries(path: &Path) -> io::Result<Vec<DirectoryChild>> {
        let mut children = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            children.push(DirectoryChild {
                name: entry.file_name(),
                is_dir: file_type.is_dir(),
                is_symlink: file_type.is_symlink(),
            });
        }
        children.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(children)
    }

    pub fn ensure_parent(path: &Path) -> io::Result<()> {
        let Some(parent) = path.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "relocation target has no parent directory",
            ));
        };
        std::fs::create_dir_all(parent)
    }

    pub fn remove_empty_directory(path: &Path) -> io::Result<()> {
        let observation = Self::observe(path)?;
        if observation.is_missing() {
            return Ok(());
        }
        if observation.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to prune symlink",
            ));
        }
        if !observation.is_directory() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to prune non-directory",
            ));
        }
        std::fs::remove_dir(path)
    }

    pub fn read_display_name(path: &Path) -> io::Result<Option<String>> {
        match std::fs::read_to_string(path) {
            Ok(content) => Ok(Some(content)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) struct DirectoryAnchor {
    directory: File,
    name: CString,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl DirectoryAnchor {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "relocation path has no parent directory",
            )
        })?;
        let file_name = path.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "relocation path has no final component",
            )
        })?;
        let bytes = file_name.as_bytes();
        if bytes.is_empty() || bytes.contains(&b'/') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "relocation name must be directory-relative",
            ));
        }
        let name = CString::new(bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "relocation name contains interior nul",
            )
        })?;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(parent)?;
        Ok(Self { directory, name })
    }

    pub(super) fn raw_fd(&self) -> libc::c_int {
        self.directory.as_raw_fd()
    }

    pub(super) fn name_ptr(&self) -> *const libc::c_char {
        self.name.as_ptr()
    }
}
