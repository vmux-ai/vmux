use crate::{CefOsCryptKeyProvider, CefOsCryptKeys};

use super::message_loop::cef_framework_binary_path;

type CefSetOsCryptKeys = unsafe extern "C" fn(*const u8, usize, *const u8, usize) -> i32;

pub(super) struct OsCryptConfigurator {
    library: libloading::Library,
}

impl OsCryptConfigurator {
    pub(super) fn configure(provider: CefOsCryptKeyProvider) -> Result<(), String> {
        let configurator = Self::load()?;
        let keys =
            provider().map_err(|error| format!("failed to unlock browser storage: {error}"))?;
        configurator.validate(&keys)?;
        configurator.apply(&keys)
    }

    fn load() -> Result<Self, String> {
        let path = cef_framework_binary_path();
        let library = unsafe { libloading::Library::new(&path) }
            .map_err(|error| format!("failed to open custom CEF at {}: {error}", path.display()))?;
        Ok(Self { library })
    }

    fn validate(&self, keys: &CefOsCryptKeys) -> Result<(), String> {
        if keys.current().len() != CefOsCryptKeys::KEY_LENGTH {
            return Err("custom CEF current encryption key has an invalid length".to_string());
        }
        if keys
            .legacy()
            .is_some_and(|key| key.len() != CefOsCryptKeys::KEY_LENGTH)
        {
            return Err("custom CEF legacy encryption key has an invalid length".to_string());
        }
        Ok(())
    }

    fn apply(&self, keys: &CefOsCryptKeys) -> Result<(), String> {
        let setter = unsafe {
            self.library
                .get::<CefSetOsCryptKeys>(b"cef_set_os_crypt_keys\0")
                .map_err(|error| {
                    format!("custom CEF Safe Storage API cef_set_os_crypt_keys is missing: {error}")
                })?
        };
        let (legacy, legacy_length) = match keys.legacy() {
            Some(key) => (key.as_ptr(), key.len()),
            None => (std::ptr::null(), 0),
        };
        let result = unsafe {
            setter(
                keys.current().as_ptr(),
                keys.current().len(),
                legacy,
                legacy_length,
            )
        };
        if result != 1 {
            return Err("custom CEF rejected the browser encryption keys".to_string());
        }
        Ok(())
    }
}
