//! Platforms with no OS key storage. Nothing persists a Vault key across runs, so
//! the Vault stays locked until a passkey or recovery key seeds the session cache,
//! and the key broker is unavailable.

use zeroize::Zeroizing;

use super::LOCKED;

const UNAVAILABLE: &str = "Vault key broker is only available on macOS";

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

    pub(super) fn broker_load(_vault_id: &str) -> Result<Option<String>, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(super) fn broker_load_silent(_vault_id: &str) -> Result<Option<String>, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(super) fn broker_store(_vault_id: &str, _encoded_key: &str) -> Result<(), String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(super) fn authorize_broker_parent() -> Result<(), String> {
        Err(UNAVAILABLE.to_string())
    }
}
