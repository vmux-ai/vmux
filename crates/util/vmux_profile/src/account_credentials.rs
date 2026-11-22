use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

use crate::safe_storage::{SafeStorage, SafeStorageContext, SafeStorageName};

pub struct AccountCredentials {
    context: SafeStorageContext,
    scope: String,
}

impl AccountCredentials {
    pub fn at(data: PathBuf, scope: String) -> Self {
        Self {
            context: SafeStorageContext::at(data),
            scope,
        }
    }

    pub fn load_existing(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        self.context
            .require_desktop_process()
            .map_err(|error| error.to_string())?;
        let name = SafeStorageName::of(&self.scope);
        let file = self
            .context
            .protected_file(PathBuf::from("account").join(format!("{name}.bin")));
        let parent = file.path().parent().ok_or("invalid credential path")?;
        for directory in [self.context.directory(), parent] {
            match directory.symlink_metadata() {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err("credential directory must not be a symlink".into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.to_string()),
            }
        }
        let held = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(file.path())
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let metadata = held.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() > 4096 {
            return Err("invalid encrypted account credential file".into());
        }
        let mut envelope = Vec::new();
        held.take(4097)
            .read_to_end(&mut envelope)
            .map_err(|error| error.to_string())?;
        if envelope.len() > 4096 {
            return Err("invalid encrypted account credential file".into());
        }
        let opened = SafeStorage::open_account_silent(&self.context, &self.scope, &envelope)
            .map_err(|error| error.to_string())?;
        let Some(bytes) = opened else {
            return Ok(None);
        };
        if bytes.len() != 48 {
            return Err("invalid account device key".into());
        }
        Ok(Some(bytes))
    }

    pub fn load_or_create(&self, proposed: &[u8]) -> Result<(Zeroizing<Vec<u8>>, bool), String> {
        self.context
            .require_desktop_process()
            .map_err(|error| error.to_string())?;
        if proposed.len() != 48 {
            return Err("invalid account device key".into());
        }
        let name = SafeStorageName::of(&self.scope);
        let file = self
            .context
            .protected_file(PathBuf::from("account").join(format!("{name}.bin")));
        let lock = self
            .context
            .protected_file(PathBuf::from("account").join(format!("{name}.lock")));
        let parent = lock.path().parent().ok_or("invalid credential path")?;
        for path in [self.context.directory(), parent] {
            if let Ok(metadata) = path.symlink_metadata()
                && (!metadata.is_dir() || metadata.file_type().is_symlink())
            {
                return Err("credential directory must not be a symlink".into());
            }
        }
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        let held = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(lock.path())
            .map_err(|error| error.to_string())?;
        if !held
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("invalid credential lock file".into());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match held.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        if let Ok(metadata) = file.path().symlink_metadata()
            && (!metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096)
        {
            return Err("invalid encrypted account credential file".into());
        }
        if let Some(envelope) = file.read().map_err(|error| error.to_string())? {
            let bytes = SafeStorage::open_account(&self.context, &self.scope, &envelope)
                .map_err(|error| error.to_string())?;
            if bytes.len() != 48 {
                return Err("invalid account device key".into());
            }
            return Ok((bytes, true));
        }
        let encrypted = SafeStorage::seal_account(&self.context, &self.scope, proposed)
            .map_err(|error| error.to_string())?;
        file.write(&encrypted).map_err(|error| error.to_string())?;
        Ok((Zeroizing::new(proposed.to_vec()), false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_enrollment_is_read_without_creating_a_directory_or_key() {
        assert!(
            crate::SessionEnvironment::is_test(),
            "isolated fixed-key test composition required"
        );
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("absent");
        let credentials = AccountCredentials::at(data.clone(), "existing-only".into());
        assert!(credentials.load_existing().unwrap().is_none());
        assert!(!data.exists());
    }

    #[test]
    fn existing_enrollment_decrypts_without_replacing_its_envelope_and_corruption_is_preserved() {
        assert!(
            crate::SessionEnvironment::is_test(),
            "isolated fixed-key test composition required"
        );
        let directory = tempfile::tempdir().unwrap();
        let credentials =
            AccountCredentials::at(directory.path().to_path_buf(), "existing-only".into());
        let mut proposed = Zeroizing::new([0u8; 48]);
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), proposed.as_mut())
            .unwrap();
        credentials.load_or_create(proposed.as_ref()).unwrap();
        let file = credentials.context.protected_file(
            PathBuf::from("account").join(format!("{}.bin", SafeStorageName::of("existing-only"))),
        );
        let before = std::fs::read(file.path()).unwrap();
        assert_eq!(
            &*credentials.load_existing().unwrap().unwrap(),
            proposed.as_ref()
        );
        assert_eq!(std::fs::read(file.path()).unwrap(), before);
        std::fs::write(file.path(), b"corrupt envelope").unwrap();
        assert!(credentials.load_existing().is_err());
        assert_eq!(std::fs::read(file.path()).unwrap(), b"corrupt envelope");
    }
}
