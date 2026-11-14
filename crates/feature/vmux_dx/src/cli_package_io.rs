use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use bevy_ecs::prelude::Component;
use vmux_ecs::blocking::Lane;
use vmux_ecs::jobs::Job;

use crate::execution::Failure;
use crate::storage_io::{Done, StorageJob};
use crate::temporary::Temporary;

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct CreateTemporary {
    pub parent: PathBuf,
    pub prefix: String,
}

impl StorageJob for CreateTemporary {
    type Data = Directory;
}

impl Job for CreateTemporary {
    type Output = Result<Directory, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Packaging canceled"));
        }
        Temporary::create(&self.parent, &self.prefix)
            .map(|value| Directory(Arc::new(value)))
            .map_err(Failure::from_error)
    }
}

#[derive(Component, Clone)]
pub(crate) struct Directory(pub Arc<Temporary>);

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct WriteArchive {
    pub binary: PathBuf,
    pub destination: PathBuf,
}

impl StorageJob for WriteArchive {
    type Data = Done;
}

impl Job for WriteArchive {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Packaging canceled"));
        }
        let binary = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(self.binary)
            .map_err(|error| Failure::message(error.to_string()))?;
        let metadata = binary
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
            return Err(Failure::message(
                "Standalone release CLI has an invalid size",
            ));
        }
        let output =
            File::create(self.destination).map_err(|error| Failure::message(error.to_string()))?;
        let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(metadata.len());
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "vmux", binary)
            .map_err(|error| Failure::message(error.to_string()))?;
        let encoder = archive
            .into_inner()
            .map_err(|error| Failure::message(error.to_string()))?;
        encoder
            .finish()
            .map_err(|error| Failure::message(error.to_string()))?
            .sync_all()
            .map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct ReadArchive(pub PathBuf);

impl StorageJob for ReadArchive {
    type Data = Bytes;
}

impl Job for ReadArchive {
    type Output = Result<Bytes, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_ecs::blocking::BlockingCancellation::canceled() {
            return Err(Failure::message("Packaging canceled"));
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message("CLI archive must be a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| Failure::message(error.to_string()))?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(Failure::message("CLI archive exceeds 64 MiB"));
        }
        Ok(Bytes(bytes))
    }
}

#[derive(Component, Clone)]
pub(crate) struct Bytes(pub Vec<u8>);

pub(crate) struct Scoped<J: StorageJob>(pub J);

impl<J: StorageJob> Clone for Scoped<J> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<J: StorageJob> Job for Scoped<J> {
    type Output = Result<J::Data, Failure>;

    fn lane(&self) -> Lane {
        self.0.lane()
    }

    fn run(self) -> Self::Output {
        self.0.run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_archive_contains_one_executable_with_its_original_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("binary");
        let archive = directory.path().join("cli.tar.gz");
        let payload = b"#!/bin/sh\nprintf 'vmux 9.0.0\\n'\n";
        std::fs::write(&binary, payload).unwrap();
        WriteArchive {
            binary,
            destination: archive.clone(),
        }
        .run()
        .unwrap();
        let decoder = flate2::read::GzDecoder::new(File::open(archive).unwrap());
        let mut archive = tar::Archive::new(decoder);
        let mut entries = archive.entries().unwrap();
        let mut entry = entries.next().unwrap().unwrap();
        assert_eq!(entry.path().unwrap().as_ref(), std::path::Path::new("vmux"));
        assert_eq!(entry.header().mode().unwrap(), 0o755);
        let mut actual = Vec::new();
        entry.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, payload);
        assert!(entries.next().is_none());
    }

    #[test]
    fn fifo_executable_is_refused_before_archive_creation() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("binary");
        let archive = directory.path().join("cli.tar.gz");
        let path = std::ffi::CString::new(binary.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(
            WriteArchive {
                binary,
                destination: archive.clone()
            }
            .run()
            .is_err()
        );
        assert!(!archive.exists());
    }
}
