use std::fs::{self, OpenOptions};
use std::io::{Read, Write as _};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;

use bevy_ecs::prelude::Component;
use sha2::{Digest, Sha256};
use vmux_ecs::blocking::{BlockingCancellation, Lane};
use vmux_ecs::jobs::Job;

use crate::environment::Environment;
use crate::execution::Failure;
use crate::storage_io::{Done, StorageJob, ToolOutput};
use crate::temporary::Temporary;

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct Hash(pub PathBuf);

impl StorageJob for Hash {
    type Data = Fingerprint;
}

impl Job for Hash {
    type Output = Result<Fingerprint, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(self.0)
            .map_err(|error| Failure::message(error.to_string()))?;
        if !file
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?
            .is_file()
        {
            return Err(Failure::message(
                "Package hash input must be a regular file",
            ));
        }
        let mut hash = Sha256::new();
        let mut bytes = [0; 65536];
        loop {
            if BlockingCancellation::canceled() {
                return Err(Failure::message("Package hashing canceled"));
            }
            let count = file
                .read(&mut bytes)
                .map_err(|error| Failure::message(error.to_string()))?;
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        Ok(Fingerprint(format!("{:x}", hash.finalize())))
    }
}

#[derive(Component, Clone)]
pub(crate) struct Fingerprint(pub String);

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct Copy {
    pub source: PathBuf,
    pub destination: PathBuf,
}

impl StorageJob for Copy {
    type Data = Done;
}

impl Job for Copy {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err(Failure::message("Package copy canceled"));
        }
        let parent = self
            .destination
            .parent()
            .ok_or_else(|| Failure::message("Missing copy parent"))?;
        fs::create_dir_all(parent).map_err(|error| Failure::message(error.to_string()))?;
        let temporary = Temporary::create(parent, ".vmux-copy").map_err(Failure::from_error)?;
        let staging = temporary.path().join("file");
        let mut source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(self.source)
            .map_err(|error| Failure::message(error.to_string()))?;
        let metadata = source
            .metadata()
            .map_err(|error| Failure::message(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
            return Err(Failure::message(
                "Package copy source must be a bounded regular file",
            ));
        };
        let mut destination = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(metadata.permissions().mode())
            .open(&staging)
            .map_err(|error| Failure::message(error.to_string()))?;
        let mut bytes = [0; 65536];
        let mut size = 0u64;
        loop {
            if BlockingCancellation::canceled() {
                return Err(Failure::message("Package copy canceled"));
            };
            let count = source
                .read(&mut bytes)
                .map_err(|error| Failure::message(error.to_string()))?;
            if count == 0 {
                break;
            };
            size += count as u64;
            if size > 512 * 1024 * 1024 {
                return Err(Failure::message("Package copy exceeds 512 MiB"));
            };
            destination
                .write_all(&bytes[..count])
                .map_err(|error| Failure::message(error.to_string()))?;
        }
        destination
            .set_permissions(metadata.permissions())
            .map_err(|error| Failure::message(error.to_string()))?;
        destination
            .sync_all()
            .map_err(|error| Failure::message(error.to_string()))?;
        if BlockingCancellation::canceled() {
            return Err(Failure::message("Package copy canceled"));
        }
        fs::rename(staging, self.destination)
            .map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct Write {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

impl StorageJob for Write {
    type Data = Done;
}

impl Job for Write {
    type Output = Result<Done, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if BlockingCancellation::canceled() {
            return Err(Failure::message("Package write canceled"));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| Failure::message("Missing write parent"))?;
        fs::create_dir_all(parent).map_err(|error| Failure::message(error.to_string()))?;
        let temporary = Temporary::create(parent, ".vmux-write").map_err(Failure::from_error)?;
        let staging = temporary.path().join("file");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(self.mode)
            .open(&staging)
            .map_err(|error| Failure::message(error.to_string()))?;
        file.set_permissions(fs::Permissions::from_mode(self.mode))
            .map_err(|error| Failure::message(error.to_string()))?;
        file.write_all(&self.bytes)
            .map_err(|error| Failure::message(error.to_string()))?;
        file.sync_all()
            .map_err(|error| Failure::message(error.to_string()))?;
        if BlockingCancellation::canceled() {
            return Err(Failure::message("Package write canceled"));
        }
        fs::rename(staging, self.path).map_err(|error| Failure::message(error.to_string()))?;
        Ok(Done)
    }
}

#[derive(Component, Clone)]
#[component(immutable)]
pub(crate) struct CleanupCapture {
    pub environment: Environment,
    pub directory: PathBuf,
    pub arguments: Vec<std::ffi::OsString>,
}

impl StorageJob for CleanupCapture {
    type Data = ToolOutput;
}

impl Job for CleanupCapture {
    type Output = Result<ToolOutput, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        let mut child = self
            .environment
            .command("security", &self.directory)
            .args(self.arguments)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| Failure::message(error.to_string()))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    crate::signals::Signals::terminate_group(child.id());
                    return Ok(ToolOutput(std::process::Output {
                        status,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    }));
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                result => {
                    crate::signals::Signals::terminate_group(child.id());
                    let _ = child.wait();
                    return Err(Failure::message(match result {
                        Err(error) => error.to_string(),
                        _ => "Keychain cleanup timed out".into(),
                    }));
                }
            }
        }
    }
}

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
    fn copying_a_fifo_or_symlink_refuses_without_replacing_the_destination() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let destination = directory.path().join("destination");
        let fifo = directory.path().join("fifo");
        let link = directory.path().join("link");
        fs::write(&source, b"source").unwrap();
        fs::write(&destination, b"previous").unwrap();
        std::os::unix::fs::symlink(&source, &link).unwrap();
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        for source in [fifo, link] {
            let (sender, receiver) = std::sync::mpsc::channel();
            let destination = destination.clone();
            let worker = std::thread::spawn(move || {
                sender
                    .send(
                        Copy {
                            source,
                            destination,
                        }
                        .run()
                        .is_err(),
                    )
                    .unwrap()
            });
            assert!(
                receiver
                    .recv_timeout(std::time::Duration::from_secs(1))
                    .expect("Package copy blocked on a non-regular source")
            );
            worker.join().unwrap();
        }
        assert_eq!(fs::read(destination).unwrap(), b"previous");
    }

    #[test]
    fn copying_an_executable_replaces_atomically_and_preserves_its_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let destination = directory.path().join("destination");
        fs::write(&source, b"executable").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o751)).unwrap();
        fs::write(&destination, b"previous").unwrap();
        Copy {
            source,
            destination: destination.clone(),
        }
        .run()
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"executable");
        assert_eq!(
            fs::metadata(destination).unwrap().permissions().mode() & 0o777,
            0o751
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
