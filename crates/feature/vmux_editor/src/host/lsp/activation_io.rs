use super::package_path::PackageName;
use super::store::PackageStore;
use bevy::prelude::Component;
use std::path::PathBuf;
use vmux_job::{BlockingCancellation, Job};
use vmux_tool::io::TemporaryDirectory as Staging;

#[derive(Clone)]
pub struct EnsurePackages {
    pub path: PathBuf,
}

impl Job for EnsurePackages {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("package activation preparation canceled".into());
        }
        std::fs::create_dir_all(self.path).map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct Backup {
    pub source: PathBuf,
    pub target: PathBuf,
    pub staging: Staging,
}

impl Job for Backup {
    type Output = Result<BackedUp, String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("package package backup canceled".into());
        }
        let _staging = self.staging;
        match self.target.symlink_metadata() {
            Ok(_) => return Err("package backup target already exists".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match std::fs::rename(self.source, self.target) {
            Ok(()) => Ok(BackedUp(true)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BackedUp(false)),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Component)]
pub struct BackedUp(pub bool);

#[derive(Clone)]
pub struct Restore {
    pub source: PathBuf,
    pub target: PathBuf,
    pub staging: Staging,
}

impl Job for Restore {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        let _staging = self.staging;
        match self.target.symlink_metadata() {
            Ok(_) => return Err("package restore target already exists".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        std::fs::rename(self.source, self.target).map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct Activate {
    pub store: PackageStore,
    pub name: PackageName,
    pub staging: Staging,
}

impl Job for Activate {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err("package package activation canceled".into());
        }
        std::fs::rename(
            self.staging.path().join("package"),
            self.store.package_dir(&self.name),
        )
        .map_err(|error| error.to_string())
    }
}
