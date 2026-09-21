use zeroize::Zeroizing;

use super::LOCKED;

impl super::DeviceKeys {
    pub(super) fn load(_vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        Err(LOCKED.to_string())
    }

    pub(super) fn load_silent(_vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        Ok(None)
    }

    pub(super) fn create() -> Result<Zeroizing<Vec<u8>>, String> {
        Err("Encrypted Vault key storage is not available on this platform".to_string())
    }

    pub(super) fn store(_vault_id: &str, _key: &[u8]) -> Result<(), String> {
        Ok(())
    }
}
