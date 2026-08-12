//! Where the Vault encryption key comes from: this device's OS key storage,
//! fronted by an in-process session cache so a run prompts at most once.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod native;
#[cfg(not(target_os = "macos"))]
mod other;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use zeroize::Zeroizing;

use super::validate_key;

const LOCKED: &str = "This Vault is locked on this device. Unlock it with a passkey.";

static SESSION_KEYS: OnceLock<Mutex<HashMap<String, Zeroizing<Vec<u8>>>>> = OnceLock::new();
static SESSION_KEY_LOAD: OnceLock<Mutex<()>> = OnceLock::new();

/// This device's own key storage. Every operation is implemented once per platform
/// in a sibling module — exactly one of which is compiled.
struct DeviceKeys;

/// How a Vault obtains, mints and persists its encryption key.
pub(super) trait KeyStore {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String>;
    fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String>;
    fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String>;
}

/// Key storage that may prompt the user, for the paths a user asked for.
pub(super) struct SystemKeyStore;

/// Key storage that never prompts, for background status polling.
pub(super) struct SilentSystemKeyStore;

impl KeyStore for SystemKeyStore {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        load_or_store_session_key(vault_id, || DeviceKeys::load(vault_id))
    }

    fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let key = DeviceKeys::create()?;
        self.store(vault_id, &key)?;
        Ok(key)
    }

    fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String> {
        DeviceKeys::store(vault_id, key)?;
        store_session_key(vault_id, key)
    }
}

impl KeyStore for SilentSystemKeyStore {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        if let Some(key) = load_session_key(vault_id)? {
            return Ok(key);
        }
        let Some(key) = DeviceKeys::load_silent(vault_id)? else {
            return Err(LOCKED.to_string());
        };
        store_session_key(vault_id, &key)?;
        Ok(key)
    }

    fn create(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        Err(LOCKED.to_string())
    }

    fn store(&self, _vault_id: &str, _key: &[u8]) -> Result<(), String> {
        Err(LOCKED.to_string())
    }
}

#[doc(hidden)]
pub fn key_broker_load(vault_id: &str) -> Result<Option<String>, String> {
    DeviceKeys::broker_load(vault_id)
}

#[doc(hidden)]
pub fn key_broker_load_silent(vault_id: &str) -> Result<Option<String>, String> {
    DeviceKeys::broker_load_silent(vault_id)
}

#[doc(hidden)]
pub fn key_broker_store(vault_id: &str, encoded_key: &str) -> Result<(), String> {
    DeviceKeys::broker_store(vault_id, encoded_key)
}

#[doc(hidden)]
pub fn authorize_key_broker_parent() -> Result<(), String> {
    DeviceKeys::authorize_broker_parent()
}

fn load_session_key(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    Ok(SESSION_KEYS
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?
        .get(vault_id)
        .map(|key| Zeroizing::new(key.to_vec())))
}

fn store_session_key(vault_id: &str, key: &[u8]) -> Result<(), String> {
    validate_key(key)?;
    SESSION_KEYS
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?
        .insert(vault_id.to_string(), Zeroizing::new(key.to_vec()));
    Ok(())
}

fn load_or_store_session_key<F>(vault_id: &str, load: F) -> Result<Zeroizing<Vec<u8>>, String>
where
    F: FnOnce() -> Result<Zeroizing<Vec<u8>>, String>,
{
    if let Some(key) = load_session_key(vault_id)? {
        return Ok(key);
    }
    let _load = SESSION_KEY_LOAD
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(key) = load_session_key(vault_id)? {
        return Ok(key);
    }
    let key = load()?;
    store_session_key(vault_id, &key)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::KEY_LEN;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn concurrent_vault_key_loads_prompt_once_per_session() {
        let vault_id = "test-concurrent-vault-key-load";
        SESSION_KEYS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .remove(vault_id);
        let loads = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(8));
        let threads = (0..8)
            .map(|_| {
                let loads = loads.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    load_or_store_session_key(vault_id, || {
                        loads.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(10));
                        Ok(Zeroizing::new(vec![7; KEY_LEN]))
                    })
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();

        for thread in threads {
            assert_eq!(*thread.join().unwrap(), vec![7; KEY_LEN]);
        }
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }
}
