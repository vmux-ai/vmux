use crate::{CefOsCryptKey, CefOsCryptKeyProvider};

use super::message_loop::cef_framework_binary_path;

type CefSetOsCryptKey = unsafe extern "C" fn(*const u8, usize, *const u8, usize) -> i32;

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

    fn validate(&self, key: &CefOsCryptKey) -> Result<(), String> {
        if key.current().len() != CefOsCryptKey::KEY_LENGTH {
            return Err("custom CEF current encryption key has an invalid length".to_string());
        }
        Ok(())
    }

    fn apply(&self, key: &CefOsCryptKey) -> Result<(), String> {
        let setter = unsafe {
            self.library
                .get::<CefSetOsCryptKey>(b"cef_set_os_crypt_keys\0")
                .map_err(|error| {
                    format!("custom CEF Safe Storage API cef_set_os_crypt_keys is missing: {error}")
                })?
        };
        let result = unsafe {
            setter(
                key.current().as_ptr(),
                key.current().len(),
                std::ptr::null(),
                0,
            )
        };
        if result != 1 {
            return Err("custom CEF rejected the browser encryption keys".to_string());
        }
        Ok(())
    }
}
