use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

use super::super::snapshot::{KEY_LEN, VaultCrypto};
use super::{KeyWriteReceipt, LOCKED, NativeKeyAccess};
use vmux_profile::safe_storage::{SafeStorage, SafeStorageName};

impl NativeKeyAccess {
    pub(crate) fn read(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.read(vault_id);
        }
        let file = self.context.protected_file(
            std::path::PathBuf::from("vault")
                .join(format!("{}.bin", SafeStorageName::of(vault_id))),
        );
        let envelope = file
            .read()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| LOCKED.to_string())?;
        let key = SafeStorage::unwrap_vault_key(&self.context, vault_id, &envelope)
            .map_err(|error| error.to_string())?;
        VaultCrypto::new(&key)?;
        Ok(key)
    }

    pub(crate) fn observe(&self, vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.observe(vault_id);
        }
        let file = self.context.protected_file(
            std::path::PathBuf::from("vault")
                .join(format!("{}.bin", SafeStorageName::of(vault_id))),
        );
        let Some(envelope) = file.read().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let Some(key) = SafeStorage::unwrap_vault_key_silent(&self.context, vault_id, &envelope)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        VaultCrypto::new(&key)?;
        Ok(Some(key))
    }

    pub(crate) fn generate(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.generate();
        }
        let mut key = Zeroizing::new(vec![0_u8; KEY_LEN]);
        SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| "failed to generate Vault encryption key".to_string())?;
        Ok(key)
    }

    pub(crate) fn write(&self, vault_id: &str, key: &[u8]) -> Result<KeyWriteReceipt, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.write(vault_id, key);
        }
        VaultCrypto::new(key)?;
        let file = self.context.protected_file(
            std::path::PathBuf::from("vault")
                .join(format!("{}.bin", SafeStorageName::of(vault_id))),
        );
        let created = file.read().map_err(|error| error.to_string())?.is_none();
        let envelope = SafeStorage::wrap_vault_key(&self.context, vault_id, key)
            .map_err(|error| error.to_string())?;
        file.write(&envelope).map_err(|error| error.to_string())?;
        Ok(KeyWriteReceipt { created })
    }
}
