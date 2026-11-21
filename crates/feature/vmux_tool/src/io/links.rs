use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy_ecs::prelude::Component as EcsComponent;
use vmux_ecs::{BlockingCancellation, Job};

#[derive(EcsComponent, Clone, Default)]
pub struct PathReceipt(Arc<AtomicBool>);

impl PathReceipt {
    pub fn changed(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(super) fn record(&self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(EcsComponent)]
pub struct PathChanged(pub bool);

#[derive(Clone)]
pub struct LinkPath {
    pub source: PathBuf,
    pub target: PathBuf,
    pub receipt: PathReceipt,
}

impl Job for LinkPath {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("link creation canceled".into());
        }
        if !self.source.is_absolute() || !self.target.is_absolute() {
            return Err("link paths must be absolute".into());
        }
        match self.target.symlink_metadata() {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() && self.matches()? {
                    return Ok(PathChanged(false));
                }
                return Err(format!(
                    "link target is occupied: {}",
                    self.target.display()
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let parent = self.target.parent().ok_or("link target has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        if BlockingCancellation::canceled() {
            return Err("link creation canceled".into());
        }
        let relative = Self::relative(parent, &self.source);
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(relative, &self.target);
        #[cfg(not(unix))]
        let result = std::os::windows::fs::symlink_file(relative, &self.target);
        match result {
            Ok(()) => {
                self.receipt.record();
                Ok(PathChanged(true))
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists && self.matches()? => {
                Ok(PathChanged(false))
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

impl LinkPath {
    fn matches(&self) -> Result<bool, String> {
        let link = match std::fs::read_link(&self.target) {
            Ok(link) => link,
            Err(error) if error.kind() == ErrorKind::InvalidInput => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };
        let target = if link.is_absolute() {
            link
        } else {
            self.target
                .parent()
                .ok_or("link target has no parent")?
                .join(link)
        };
        Ok(vmux_path::PathIdentity::resolve(&target)
            == vmux_path::PathIdentity::resolve(&self.source))
    }

    fn normalized(path: &Path) -> PathBuf {
        let mut result = PathBuf::new();
        for part in path.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                _ => result.push(part.as_os_str()),
            }
        }
        result
    }

    fn relative(from: &Path, to: &Path) -> PathBuf {
        let from = Self::normalized(from);
        let to = Self::normalized(to);
        let left: Vec<_> = from.components().collect();
        let right: Vec<_> = to.components().collect();
        let mut shared = 0;
        while shared < left.len() && shared < right.len() && left[shared] == right[shared] {
            shared += 1;
        }
        let mut result = PathBuf::new();
        for _ in shared..left.len() {
            result.push("..");
        }
        for part in &right[shared..] {
            result.push(part.as_os_str());
        }
        result
    }
}

#[derive(Clone)]
pub struct UnlinkPath {
    pub source: PathBuf,
    pub target: PathBuf,
    pub receipt: PathReceipt,
}

impl Job for UnlinkPath {
    type Output = Result<PathChanged, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("link removal canceled".into());
        }
        if !self.source.is_absolute() || !self.target.is_absolute() {
            return Err("link paths must be absolute".into());
        }
        let metadata = match self.target.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(PathChanged(false)),
            Err(error) => return Err(error.to_string()),
        };
        if !metadata.file_type().is_symlink() {
            return Ok(PathChanged(false));
        }
        let link = std::fs::read_link(&self.target).map_err(|error| error.to_string())?;
        let target = if link.is_absolute() {
            link
        } else {
            self.target
                .parent()
                .ok_or("link target has no parent")?
                .join(link)
        };
        if vmux_path::PathIdentity::resolve(&target)
            != vmux_path::PathIdentity::resolve(&self.source)
        {
            return Ok(PathChanged(false));
        }
        if BlockingCancellation::canceled() {
            return Err("link removal canceled".into());
        }
        match std::fs::remove_file(&self.target) {
            Ok(()) => {
                self.receipt.record();
                Ok(PathChanged(true))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(PathChanged(false)),
            Err(error) => Err(error.to_string()),
        }
    }
}
