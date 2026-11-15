use std::fs;
use std::io::Read as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy_ecs::prelude::*;
use vmux_ecs::blocking::{BlockingCancellation, Lane};
use vmux_ecs::jobs::Job;

use crate::debug_io::{AcquiredLock, FileBytes, IoJob};
use crate::execution::Failure;
use crate::policy::Policy;
use crate::storage::Storage;
use crate::target_lock::TargetLock;

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub(crate) struct Prepare {
    pub root: PathBuf,
    pub policy: Policy,
}

impl IoJob for Prepare {
    type Data = AcquiredLock;
}

impl Job for Prepare {
    type Output = Result<AcquiredLock, Failure>;

    fn run(self) -> Self::Output {
        let target = self.root.join(".build-tools/lints");
        let directories = [
            self.root.join(".build-tools"),
            target.clone(),
            self.root.join(".build-tools/dylint"),
            self.root.join(".build-tools/dylint/bin"),
            self.root.join(".build-tools/dylint-cargo"),
            self.root.join(".build-tools/dylint-drivers"),
        ];
        for directory in &directories {
            Self::validate(directory)?;
        }
        for directory in &directories {
            if BlockingCancellation::canceled() {
                return Err(Failure::message("Compiler tool preparation canceled"));
            }
            match fs::create_dir(directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(Failure::message(error.to_string())),
            }
            Self::validate(directory)?;
            Storage::new(self.root.clone(), directory.clone(), self.policy.clone())
                .map_err(Failure::from_error)?;
        }
        Storage::new(self.root.clone(), target.clone(), self.policy.clone())
            .map_err(Failure::from_error)?
            .guard()
            .map_err(Failure::from_error)?;
        let lock = TargetLock::acquire(&self.root, &target).map_err(Failure::from_error)?;
        if BlockingCancellation::canceled() {
            return Err(Failure::message("Compiler tool preparation canceled"));
        }
        for directory in &directories {
            Self::validate(directory)?;
        }
        Storage::new(self.root.clone(), target, self.policy)
            .map_err(Failure::from_error)?
            .guard()
            .map_err(Failure::from_error)?;
        Ok(AcquiredLock(Arc::new(lock)))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}

impl Prepare {
    fn validate(path: &Path) -> Result<(), Failure> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => Err(Failure::message(
                "Compiler tools require dedicated nonsymlink cache directories",
            )),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Failure::message(error.to_string())),
        }
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub(crate) struct Read(pub PathBuf);

impl IoJob for Read {
    type Data = FileBytes;
}

impl Job for Read {
    type Output = Result<FileBytes, Failure>;
    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err(Failure::message("Compiler metadata read canceled"));
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message("Compiler metadata must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|error| Failure::message(error.to_string()))?;
        if bytes.len() > 4096 {
            return Err(Failure::message("Compiler lint metadata exceeds 4 KiB"));
        }
        Ok(FileBytes(bytes))
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}
