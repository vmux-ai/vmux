use super::IoJob;
use super::fs::{Location, Written};
use crate::temporary::Temporary;
use std::path::{Path, PathBuf};
use vmux_ecs::jobs::Job;

#[derive(Clone)]
pub(crate) struct StageFiles {
    pub(crate) destination: Location,
    pub(crate) files: Vec<Location>,
}

impl IoJob for StageFiles {
    type Data = Written;
}

impl Job for StageFiles {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let destination = self.destination.resolve()?;
        let parent = destination.parent().ok_or("Missing cache staging parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary =
            Temporary::create(parent, ".vmux-cache-stage").map_err(|error| error.to_string())?;
        for source in &self.files {
            Files::check()?;
            if source.root != self.destination.root {
                return Err("Cache staging roots differ".into());
            }
            let from = Files::leaf(source)?;
            let to = temporary.path().join(&source.relative);
            Files::link(&from, &to)?;
        }
        Files::check()?;
        self.destination.resolve()?;
        if destination.exists() {
            std::fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
        }
        std::fs::rename(temporary.path(), destination).map_err(|error| error.to_string())?;
        Ok(Written)
    }
}

#[derive(Clone)]
pub(crate) struct RestoreFiles {
    pub(crate) source: Location,
    pub(crate) files: Vec<PathBuf>,
}

impl IoJob for RestoreFiles {
    type Data = Written;
}

impl Job for RestoreFiles {
    type Output = Result<Written, String>;

    fn run(self) -> Self::Output {
        let source = self.source.resolve()?;
        let parent = source.parent().ok_or("Missing cache restoration parent")?;
        let temporary =
            Temporary::create(parent, ".vmux-cache-restore").map_err(|error| error.to_string())?;
        for relative in &self.files {
            Files::check()?;
            let from = Files::leaf(&Location {
                root: self.source.root.clone(),
                relative: self.source.relative.join(relative),
            })?;
            let location = Location {
                root: self.source.root.clone(),
                relative: relative.clone(),
            };
            let to = Files::leaf(&location)?;
            let staging = temporary.path().join(relative);
            Files::link(&from, &staging)?;
            std::fs::create_dir_all(to.parent().ok_or("Missing compiler artifact parent")?)
                .map_err(|error| error.to_string())?;
            Files::leaf(&location)?;
            std::fs::rename(staging, to).map_err(|error| error.to_string())?;
        }
        Ok(Written)
    }
}

struct Files;

impl Files {
    fn check() -> Result<(), String> {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err("Cache transfer canceled".into());
        }
        Ok(())
    }

    fn leaf(location: &Location) -> Result<PathBuf, String> {
        let name = location
            .relative
            .file_name()
            .ok_or("Missing cache filename")?;
        let parent = location
            .relative
            .parent()
            .ok_or("Missing cache file parent")?;
        let root = if parent.as_os_str().is_empty() {
            location
                .root
                .canonicalize()
                .map_err(|error| error.to_string())?
        } else {
            Location {
                root: location.root.clone(),
                relative: parent.into(),
            }
            .resolve()?
        };
        Ok(root.join(name))
    }

    fn link(source: &Path, destination: &Path) -> Result<(), String> {
        let metadata = source
            .symlink_metadata()
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(
            destination
                .parent()
                .ok_or("Missing staged artifact parent")?,
        )
        .map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            let link = std::fs::read_link(source).map_err(|error| error.to_string())?;
            std::os::unix::fs::symlink(link, destination).map_err(|error| error.to_string())?;
        } else if metadata.is_file() {
            std::fs::hard_link(source, destination).map_err(|error| error.to_string())?;
        } else {
            return Err("Cache artifact must be a regular file or symbolic link".into());
        }
        Ok(())
    }
}
