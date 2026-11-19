use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write};
use std::path::PathBuf;

use vmux_ecs::Job;

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(super) struct Read {
    pub(super) path: PathBuf,
}

impl Job for Read {
    type Output = Result<Option<Vec<u8>>, String>;

    fn run(self) -> Self::Output {
        self.read().map_err(|error| error.to_string())
    }
}

impl Read {
    fn read(self) -> std::io::Result<Option<Vec<u8>>> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("installation identity needs a parent"))?;
        std::fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(parent.join("installation.lock"))?;
        lock.lock_shared()?;
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        (&file).take(129).read_to_end(&mut bytes)?;
        if bytes.len() > 128 {
            return Err(std::io::Error::other(
                "installation identity exceeds 128 bytes",
            ));
        }
        file.sync_all()?;
        File::open(parent)?.sync_all()?;
        Ok(Some(bytes))
    }
}

#[derive(Clone, bevy_ecs::prelude::Component)]
#[component(immutable)]
pub(super) struct Create {
    pub(super) path: PathBuf,
    pub(super) identity: String,
}

impl Job for Create {
    type Output = Result<bool, String>;

    fn run(self) -> Self::Output {
        self.create().map_err(|error| error.to_string())
    }
}

impl Create {
    fn create(self) -> std::io::Result<bool> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("installation identity needs a parent"))?;
        std::fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(parent.join("installation.lock"))?;
        lock.lock()?;
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
            Err(error) => return Err(error),
        };
        file.write_all(self.identity.as_bytes())?;
        file.sync_all()?;
        File::open(parent)?.sync_all()?;
        Ok(true)
    }
}
