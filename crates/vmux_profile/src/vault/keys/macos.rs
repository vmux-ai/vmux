use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

use super::super::snapshot::{KEY_LEN, validate_key};
use super::LOCKED;
use crate::safe_storage::{ProtectedFile, SafeStorage, SafeStorageContext, encoded_file_name};

struct VaultKeyFile {
    context: SafeStorageContext,
    vault_id: String,
    file: ProtectedFile,
}

impl VaultKeyFile {
    fn new(vault_id: &str) -> Self {
        let context = SafeStorageContext::current();
        let file = context.protected_file(
            std::path::PathBuf::from("vault").join(format!("{}.bin", encoded_file_name(vault_id))),
        );
        Self {
            context,
            vault_id: vault_id.to_string(),
            file,
        }
    }

    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let Some(envelope) = self.file.read().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let key = SafeStorage::unwrap_vault_key(&self.context, &self.vault_id, &envelope)
            .map_err(|error| error.to_string())?;
        validate_key(&key)?;
        Ok(Some(key))
    }

    fn load_silent(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let Some(envelope) = self.file.read().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let Some(key) =
            SafeStorage::unwrap_vault_key_silent(&self.context, &self.vault_id, &envelope)
                .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        validate_key(&key)?;
        Ok(Some(key))
    }

    fn store(&self, key: &[u8]) -> Result<(), String> {
        validate_key(key)?;
        let envelope = SafeStorage::wrap_vault_key(&self.context, &self.vault_id, key)
            .map_err(|error| error.to_string())?;
        self.file
            .write(&envelope)
            .map_err(|error| error.to_string())
    }
}

impl super::DeviceKeys {
    pub(super) fn load(vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        VaultKeyFile::new(vault_id)
            .load()?
            .ok_or_else(|| LOCKED.to_string())
    }

    pub(super) fn load_silent(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        VaultKeyFile::new(vault_id).load_silent()
    }

    pub(super) fn create() -> Result<Zeroizing<Vec<u8>>, String> {
        let mut key = Zeroizing::new(vec![0_u8; KEY_LEN]);
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| "failed to generate Vault encryption key".to_string())?;
        Ok(key)
    }

    pub(super) fn store(vault_id: &str, key: &[u8]) -> Result<(), String> {
        VaultKeyFile::new(vault_id).store(key)
    }
}
