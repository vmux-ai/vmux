use std::io;
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
        SettingsIo::validate(self)
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
        SettingsIo::save(self)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        SettingsIo::load(path)
    }
}

pub(crate) struct SettingsIo;

impl SettingsIo {
    fn validate(settings: &mut Settings) -> io::Result<()> {
        if settings.cpus == 0
            || settings.memory_mib < 512
            || !(1..=1024).contains(&settings.disk_gib)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid VM resources",
            ));
        }
        std::fs::create_dir_all(&settings.directory)?;
        settings.directory = settings.directory.canonicalize()?;
        settings.shared_directory = settings.shared_directory.canonicalize()?;
        if !settings.shared_directory.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared path must be a directory",
            ));
        }
        for path in [&settings.directory, &settings.shared_directory] {
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
        if settings.guest_control().as_os_str().len() >= 104 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "VM directory is too long for a Unix socket",
            ));
        }
        Ok(())
    }

    fn save(settings: &Settings) -> io::Result<PathBuf> {
        let path = settings.directory.join("settings.json");
        let bytes = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    fn load(path: &Path) -> io::Result<Settings> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    pub(crate) fn clean_sockets(settings: &Settings) -> io::Result<()> {
        for path in [
            settings.socket(),
            settings.transport_socket(),
            settings.control_socket(),
            settings.guest_control(),
            settings.network_socket(),
            settings.network_api(),
        ] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
