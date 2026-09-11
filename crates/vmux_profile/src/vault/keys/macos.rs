use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

use super::LOCKED;
use crate::safe_storage::{ProtectedFile, SafeStorage, encoded_file_name};
use crate::vault::{KEY_LEN, validate_key};

const LEGACY_KEYCHAIN_SERVICE: &str = "ai.vmux.vault";

struct VaultKeyFile {
    vault_id: String,
    file: ProtectedFile,
}

impl VaultKeyFile {
    fn new(vault_id: &str) -> Self {
        let path = crate::application_data_dir()
            .join("safe-storage")
            .join("vault")
            .join(format!("{}.bin", encoded_file_name(vault_id)));
        Self {
            vault_id: vault_id.to_string(),
            file: ProtectedFile::new(path),
        }
    }

    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let Some(envelope) = self.file.read()? else {
            return Ok(None);
        };
        let key = SafeStorage::unwrap_vault_key(&self.vault_id, &envelope)?;
        validate_key(&key)?;
        Ok(Some(key))
    }

    fn load_silent(&self) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let Some(envelope) = self.file.read()? else {
            return Ok(None);
        };
        let Some(key) = SafeStorage::unwrap_vault_key_silent(&self.vault_id, &envelope)? else {
            return Ok(None);
        };
        validate_key(&key)?;
        Ok(Some(key))
    }

    fn store(&self, key: &[u8]) -> Result<(), String> {
        validate_key(key)?;
        let envelope = SafeStorage::wrap_vault_key(&self.vault_id, key)?;
        self.file.write(&envelope)
    }
}

impl super::DeviceKeys {
    pub(super) fn load(vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let file = VaultKeyFile::new(vault_id);
        if let Some(key) = file.load()? {
            return Ok(key);
        }
        let Some(key) = load_legacy_keychain_key(vault_id, false)? else {
            return Err(LOCKED.to_string());
        };
        migrate_legacy_key(Some(key), |key| file.store(key)).ok_or_else(|| LOCKED.to_string())
    }

    pub(super) fn load_silent(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let file = VaultKeyFile::new(vault_id);
        if let Some(key) = file.load_silent()? {
            return Ok(Some(key));
        }
        load_legacy_keychain_key(vault_id, true)
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

fn keychain_options(vault_id: &str) -> security_framework::passwords::PasswordOptions {
    let mut options = security_framework::passwords::PasswordOptions::new_generic_password(
        LEGACY_KEYCHAIN_SERVICE,
        vault_id,
    );
    options.set_access_synchronized(Some(false));
    options
}

fn load_legacy_keychain_key(
    vault_id: &str,
    silent: bool,
) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    crate::safe_storage::require_desktop_process()?;
    if silent {
        return load_legacy_keychain_key_silent(vault_id);
    }
    use security_framework::passwords::generic_password;
    use security_framework_sys::base::errSecItemNotFound;

    match generic_password(keychain_options(vault_id)) {
        Ok(key) => {
            validate_key(&key)?;
            Ok(Some(Zeroizing::new(key)))
        }
        Err(error) if error.code() == errSecItemNotFound => Ok(None),
        Err(error) => Err(format!("failed to unlock legacy Vault key: {error}")),
    }
}

fn load_legacy_keychain_key_silent(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

    let mut search = ItemSearchOptions::new();
    let results = search
        .class(ItemClass::generic_password())
        .service(LEGACY_KEYCHAIN_SERVICE)
        .account(vault_id)
        .load_data(true)
        .skip_authenticated_items(true)
        .search();
    let Ok(results) = results else {
        return Ok(None);
    };
    let Some(key) = results.into_iter().find_map(|result| match result {
        SearchResult::Data(key) => Some(key),
        _ => None,
    }) else {
        return Ok(None);
    };
    validate_key(&key)?;
    Ok(Some(Zeroizing::new(key)))
}

fn migrate_legacy_key(
    key: Option<Zeroizing<Vec<u8>>>,
    store: impl FnOnce(&[u8]) -> Result<(), String>,
) -> Option<Zeroizing<Vec<u8>>> {
    let key = key?;
    let _ = store(&key);
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_vault_key_is_preserved_while_migrating() {
        let key = Zeroizing::new(vec![7_u8; KEY_LEN]);
        let mut migrated = Vec::new();

        let loaded = migrate_legacy_key(Some(key), |key| {
            migrated.extend_from_slice(key);
            Ok(())
        })
        .unwrap();

        assert_eq!(loaded.as_slice(), vec![7_u8; KEY_LEN]);
        assert_eq!(migrated, vec![7_u8; KEY_LEN]);
    }

    #[test]
    fn legacy_vault_key_survives_a_failed_migration_write() {
        let key = Zeroizing::new(vec![7_u8; KEY_LEN]);

        let loaded =
            migrate_legacy_key(Some(key), |_| Err("read-only storage".to_string())).unwrap();

        assert_eq!(loaded.as_slice(), vec![7_u8; KEY_LEN]);
    }
}
