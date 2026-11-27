use std::io::{self, Write};
use std::path::PathBuf;

use vmux_vm::{Container as DockerContainer, Settings};

pub(crate) struct Cache(pub(crate) PathBuf);

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedInventory {
    version: u32,
    containers: Vec<DockerContainer>,
}

impl Cache {
    pub(crate) fn new(settings: &Settings) -> Self {
        Self(settings.directory.join("inventory.json"))
    }

    pub(crate) fn load(&self) -> io::Result<Vec<DockerContainer>> {
        let bytes = std::fs::read(&self.0)?;
        let saved: SavedInventory = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if saved.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported inventory cache version",
            ));
        }
        Ok(saved.containers)
    }

    pub(crate) fn save(&self, mut containers: Vec<DockerContainer>) -> io::Result<()> {
        for container in &mut containers {
            container.status.clear();
        }
        let saved = SavedInventory {
            version: 1,
            containers,
        };
        let bytes = serde_json::to_vec(&saved).map_err(io::Error::other)?;
        if std::fs::read(&self.0).is_ok_and(|current| current == bytes) {
            return Ok(());
        }
        let directory = self
            .0
            .parent()
            .ok_or_else(|| io::Error::other("missing inventory cache directory"))?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_data()?;
        file.persist(&self.0).map_err(|error| error.error)?;
        Ok(())
    }
}
