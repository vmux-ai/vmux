use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use security_framework::passwords::{
    PasswordOptions, generic_password, set_generic_password_options,
};
use security_framework_sys::base::errSecItemNotFound;
use zeroize::Zeroizing;

use super::{RootKeyStore, derive_legacy_browser_key, require_desktop_process};

const ROOT_SERVICE: &str = "ai.vmux.safe-storage";
const ROOT_ACCOUNT: &str = "root";
const ROOT_LABEL: &str = "Vmux Safe Storage";
const LEGACY_BROWSER_SERVICE: &str = "Chromium Safe Storage";
const LEGACY_BROWSER_ACCOUNT: &str = "Chromium";

impl RootKeyStore {
    pub(super) fn read(silent: bool) -> Result<Option<Vec<u8>>, String> {
        require_desktop_process()?;
        if silent {
            return Self::read_silent();
        }
        match generic_password(Self::options()) {
            Ok(payload) => Ok(Some(payload)),
            Err(error) if error.code() == errSecItemNotFound => Ok(None),
            Err(error) => Err(format!("failed to unlock Vmux Safe Storage: {error}")),
        }
    }

    pub(super) fn write(payload: &[u8]) -> Result<(), String> {
        require_desktop_process()?;
        set_generic_password_options(payload, Self::options())
            .map_err(|error| format!("failed to store Vmux Safe Storage root key: {error}"))
    }

    pub(super) fn legacy_browser_key() -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        require_desktop_process()?;
        let mut options =
            PasswordOptions::new_generic_password(LEGACY_BROWSER_SERVICE, LEGACY_BROWSER_ACCOUNT);
        options.set_access_synchronized(Some(false));
        match generic_password(options) {
            Ok(password) => Ok(Some(derive_legacy_browser_key(&password))),
            Err(error) if error.code() == errSecItemNotFound => Ok(None),
            Err(error) => Err(format!("failed to migrate browser Safe Storage: {error}")),
        }
    }

    fn options() -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(ROOT_SERVICE, ROOT_ACCOUNT);
        options.set_access_synchronized(Some(false));
        options.set_label(ROOT_LABEL);
        options
    }

    fn read_silent() -> Result<Option<Vec<u8>>, String> {
        let mut search = ItemSearchOptions::new();
        let results = search
            .class(ItemClass::generic_password())
            .service(ROOT_SERVICE)
            .account(ROOT_ACCOUNT)
            .load_data(true)
            .skip_authenticated_items(true)
            .search();
        let Ok(results) = results else {
            return Ok(None);
        };
        Ok(results.into_iter().find_map(|result| match result {
            SearchResult::Data(payload) => Some(payload),
            _ => None,
        }))
    }
}
