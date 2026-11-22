use zeroize::Zeroizing;

use super::super::snapshot::VaultCrypto;
use super::{KeyWriteReceipt, LOCKED, NativeKeyAccess};

impl NativeKeyAccess {
    pub(crate) fn read(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.read(_vault_id);
        }
        Err(LOCKED.to_string())
    }

    pub(crate) fn observe(&self, _vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.observe(_vault_id);
        }
        Ok(None)
    }

    pub(crate) fn generate(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.generate();
        }
        Err("Encrypted Vault key storage is not available on this platform".to_string())
    }

    pub(crate) fn write(&self, _vault_id: &str, key: &[u8]) -> Result<KeyWriteReceipt, String> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.write(_vault_id, key);
        }
        VaultCrypto::new(key)?;
        Ok(KeyWriteReceipt { created: false })
    }
}
