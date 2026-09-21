use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use security_framework::passwords::{
    PasswordOptions, generic_password, set_generic_password_options,
};
use security_framework_sys::base::errSecItemNotFound;
use zeroize::Zeroizing;

use super::root::RootKeyStore;
use super::{SafeStorageContext, SafeStorageError};

const ROOT_SERVICE: &str = "ai.vmux.safe-storage";
const ROOT_ACCOUNT: &str = "root";
const ROOT_LABEL: &str = "Vmux Safe Storage";

impl RootKeyStore {
    pub(super) fn read(
        context: &SafeStorageContext,
        silent: bool,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError> {
        context.require_desktop_process()?;
        if silent {
            return Self::read_silent();
        }
        match generic_password(Self::options()) {
            Ok(payload) => Ok(Some(Zeroizing::new(payload))),
            Err(error) if error.code() == errSecItemNotFound => Ok(None),
            Err(error) => Err(SafeStorageError::KeychainDenied(format!(
                "failed to unlock Vmux Safe Storage: {error}"
            ))),
        }
    }

    pub(super) fn write(
        context: &SafeStorageContext,
        payload: &[u8],
    ) -> Result<(), SafeStorageError> {
        context.require_desktop_process()?;
        set_generic_password_options(payload, Self::options()).map_err(|error| {
            SafeStorageError::KeychainDenied(format!(
                "failed to store Vmux Safe Storage root key: {error}"
            ))
        })
    }

    fn options() -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(ROOT_SERVICE, ROOT_ACCOUNT);
        options.set_access_synchronized(Some(false));
        options.set_label(ROOT_LABEL);
        options
    }

    fn read_silent() -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError> {
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
            SearchResult::Data(payload) => Some(Zeroizing::new(payload)),
            _ => None,
        }))
    }
}
