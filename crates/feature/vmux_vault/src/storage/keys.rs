use std::fmt::Write;
use std::path::PathBuf;
use zeroize::Zeroizing;

pub(crate) mod port;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

pub(crate) const LOCKED: &str = "This Vault is locked on this device. Unlock it with a passkey.";

pub(super) trait KeyStore {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String>;
}

#[derive(Clone)]
pub(crate) struct NativeKeyAccess {
    directory: PathBuf,
    #[cfg(target_os = "macos")]
    context: vmux_profile::safe_storage::SafeStorageContext,
    #[cfg(test)]
    fixture: Option<std::sync::Arc<dyn NativeKeyFixture>>,
}

impl NativeKeyAccess {
    pub(crate) fn current() -> Self {
        let application_data = vmux_profile::ProfilePaths::current().application_data();
        let directory = application_data.join(
            if cfg!(debug_assertions) && vmux_profile::SessionEnvironment::is_test() {
                "safe-storage-test"
            } else {
                "safe-storage"
            },
        );
        Self {
            directory,
            #[cfg(target_os = "macos")]
            context: vmux_profile::safe_storage::SafeStorageContext::at(application_data),
            #[cfg(test)]
            fixture: None,
        }
    }

    pub(crate) fn path(&self, vault_id: &str) -> PathBuf {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.path(vault_id);
        }
        let mut name = String::with_capacity(vault_id.len() * 2);
        for byte in vault_id.as_bytes() {
            let _ = write!(name, "{byte:02x}");
        }
        self.directory.join("vault").join(format!("{name}.bin"))
    }

    #[cfg(test)]
    pub(crate) fn fixture(
        directory: PathBuf,
        fixture: std::sync::Arc<dyn NativeKeyFixture>,
    ) -> Self {
        Self {
            #[cfg(target_os = "macos")]
            context: vmux_profile::safe_storage::SafeStorageContext::at(directory.clone()),
            directory,
            fixture: Some(fixture),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct KeyWriteReceipt {
    pub(crate) created: bool,
}

#[cfg(test)]
pub(crate) trait NativeKeyFixture: Send + Sync {
    fn path(&self, vault_id: &str) -> PathBuf;
    fn read(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String>;
    fn observe(&self, vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String>;
    fn generate(&self) -> Result<Zeroizing<Vec<u8>>, String>;
    fn write(&self, vault_id: &str, key: &[u8]) -> Result<KeyWriteReceipt, String>;
}
