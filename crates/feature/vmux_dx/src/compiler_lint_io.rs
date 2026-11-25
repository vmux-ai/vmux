use std::fs;
use std::io::Read as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::*;
use vmux_job::Job;
use vmux_job::blocking::{BlockingCancellation, Lane};

use crate::debug_io::{Done, FileBytes, IoJob};
use crate::execution::Failure;

#[derive(Component, Clone)]
#[component(immutable)]
#[require(crate::debug::Io)]
pub(crate) struct Prepare {
    pub root: PathBuf,
}

impl IoJob for Prepare {
    type Data = Done;
}

impl Job for Prepare {
    type Output = Result<Done, Failure>;

    fn run(self) -> Self::Output {
        let target = self.root.join(".vmux/lints");
        let directories = [
            self.root.join(".vmux"),
            target.clone(),
            self.root.join(".vmux/dylint"),
            self.root.join(".vmux/dylint/bin"),
            self.root.join(".vmux/dylint-cargo"),
            self.root.join(".vmux/dylint-drivers"),
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
        }
        for directory in &directories {
            Self::validate(directory)?;
        }
        Ok(Done)
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
            Ok(_) => {
                let canonical =
                    fs::canonicalize(path).map_err(|error| Failure::message(error.to_string()))?;
                if canonical != path {
                    return Err(Failure::message(
                        "Compiler tools require dedicated nonsymlink cache directories",
                    ));
                }
                Ok(())
            }
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
