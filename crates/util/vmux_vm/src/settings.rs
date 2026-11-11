use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Settings {
    pub directory: PathBuf,
    pub shared_directory: PathBuf,
    pub cpus: u8,
    pub memory_mib: u32,
    pub disk_gib: u64,
}

impl Settings {
    pub fn new(directory: impl Into<PathBuf>, shared_directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            shared_directory: shared_directory.into(),
            cpus: 2,
            memory_mib: 2048,
            disk_gib: 32,
        }
    }

    pub fn validate(&mut self) -> io::Result<()> {
        if self.cpus == 0 || self.memory_mib < 512 || !(1..=1024).contains(&self.disk_gib) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid VM resources",
            ));
        }
        std::fs::create_dir_all(&self.directory)?;
        self.directory = self.directory.canonicalize()?;
        self.shared_directory = self.shared_directory.canonicalize()?;
        if !self.shared_directory.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared path must be a directory",
            ));
        }
        for path in [&self.directory, &self.shared_directory] {
            if path
                .to_str()
                .is_none_or(|path| path.contains(['\n', '\r', '\0']))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "VM paths must be valid UTF-8 without control characters",
                ));
            }
        }
        if self.guest_control().as_os_str().len() >= 104 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "VM directory is too long for a Unix socket",
            ));
        }
        Ok(())
    }

    pub fn rootfs(&self) -> PathBuf {
        self.directory.join("rootfs")
    }

    pub fn socket(&self) -> PathBuf {
        self.directory.join("docker.sock")
    }

    pub fn transport_socket(&self) -> PathBuf {
        self.directory.join("docker-vsock.sock")
    }

    pub fn control_socket(&self) -> PathBuf {
        self.directory.join("control.sock")
    }

    pub fn guest_control(&self) -> PathBuf {
        self.directory.join("guest-control.sock")
    }

    pub fn network_socket(&self) -> PathBuf {
        self.directory.join("network.sock")
    }

    pub fn network_api(&self) -> PathBuf {
        self.directory.join("network-api.sock")
    }

    pub fn disk(&self) -> PathBuf {
        self.directory.join("data.raw")
    }

    pub fn log(&self) -> PathBuf {
        self.directory.join("engine.log")
    }

    pub fn save(&self) -> io::Result<PathBuf> {
        let path = self.directory.join("settings.json");
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }
}
