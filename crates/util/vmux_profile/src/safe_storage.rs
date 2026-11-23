#[cfg(target_os = "macos")]
use cipher::SafeStorageCipher;
#[cfg(any(target_os = "macos", all(test, unix)))]
pub use file::ProtectedFile;
use std::fmt::{Display, Formatter};
#[cfg(any(target_os = "macos", test))]
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", test))]
use std::sync::{Arc, Mutex};
#[cfg(any(target_os = "macos", test))]
use zeroize::Zeroizing;

use std::fmt::Write;

#[cfg(any(target_os = "macos", test))]
mod cipher;
#[cfg(any(target_os = "macos", all(test, unix)))]
mod file;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", all(test, unix)))]
mod root;

#[cfg(any(target_os = "macos", test))]
const ROOT_KEY_LENGTH: usize = 32;
#[cfg(any(target_os = "macos", test))]
const BROWSER_KEY_LENGTH: usize = 16;
#[cfg(any(target_os = "macos", test))]
const DATA_KEY_LENGTH: usize = 32;

#[derive(Debug)]
pub enum SafeStorageError {
    MissingRoot,
    KeychainDenied(String),
    CorruptEnvelope,
    CorruptRoot,
    UnsupportedVersion,
    InvalidKeyLength {
        key: &'static str,
        expected: usize,
        actual: usize,
    },
    Io(String),
    Crypto(&'static str),
    DesktopProcessRequired,
    LockPoisoned,
}

impl Display for SafeStorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRoot => formatter.write_str(
                "Vmux Safe Storage root key is missing; sign in again and recover encrypted Vaults",
            ),
            Self::KeychainDenied(error) => formatter.write_str(error),
            Self::CorruptEnvelope => formatter.write_str("Vmux Safe Storage data is corrupt"),
            Self::CorruptRoot => formatter.write_str("Vmux Safe Storage root key is corrupt"),
            Self::UnsupportedVersion => {
                formatter.write_str("Vmux Safe Storage data has an unsupported version")
            }
            Self::InvalidKeyLength {
                key,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid {key} length: expected {expected} bytes, got {actual}"
            ),
            Self::Io(error) => formatter.write_str(error),
            Self::Crypto(error) => formatter.write_str(error),
            Self::DesktopProcessRequired => {
                formatter.write_str("Vmux Safe Storage is only available to the desktop process")
            }
            Self::LockPoisoned => formatter.write_str("Vmux Safe Storage lock is poisoned"),
        }
    }
}

impl std::error::Error for SafeStorageError {}

#[cfg(any(target_os = "macos", test))]
pub struct RootKey(Zeroizing<[u8; ROOT_KEY_LENGTH]>);

#[cfg(any(target_os = "macos", test))]
pub struct BrowserKey(Zeroizing<[u8; BROWSER_KEY_LENGTH]>);

#[cfg(any(target_os = "macos", test))]
pub struct DataKey(Zeroizing<[u8; DATA_KEY_LENGTH]>);

#[cfg(any(target_os = "macos", test))]
pub struct VaultWrappingKey(Zeroizing<[u8; DATA_KEY_LENGTH]>);

#[cfg(any(target_os = "macos", test))]
impl RootKey {
    fn new(bytes: [u8; ROOT_KEY_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    #[cfg(any(target_os = "macos", all(test, unix)))]
    fn duplicate(&self) -> Self {
        Self::new(*self.0)
    }

    fn as_bytes(&self) -> &[u8; ROOT_KEY_LENGTH] {
        &self.0
    }
}

#[cfg(any(target_os = "macos", test))]
impl BrowserKey {
    fn new(bytes: [u8; BROWSER_KEY_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; BROWSER_KEY_LENGTH] {
        &self.0
    }
}

#[cfg(any(target_os = "macos", test))]
impl DataKey {
    fn new(bytes: [u8; DATA_KEY_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    fn as_bytes(&self) -> &[u8; DATA_KEY_LENGTH] {
        &self.0
    }
}

#[cfg(any(target_os = "macos", test))]
impl VaultWrappingKey {
    fn new(bytes: [u8; DATA_KEY_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    fn as_bytes(&self) -> &[u8; DATA_KEY_LENGTH] {
        &self.0
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone)]
pub struct SafeStorageContext {
    access: Arc<StorageAccess>,
}

#[cfg(any(target_os = "macos", test))]
struct StorageAccess {
    directory: PathBuf,
    #[cfg(any(test, all(target_os = "macos", debug_assertions)))]
    test_session: bool,
    desktop_process_required: bool,
    root_key: Mutex<Option<RootKey>>,
    #[cfg(all(test, unix))]
    root_fixture: Option<Arc<dyn root::RootFixture>>,
}

#[cfg(any(target_os = "macos", test))]
impl SafeStorageContext {
    #[cfg(target_os = "macos")]
    pub fn current() -> Self {
        Self::at(crate::ProfilePaths::current().application_data())
    }

    #[cfg(target_os = "macos")]
    pub fn at(application_data: PathBuf) -> Self {
        #[cfg(any(test, debug_assertions))]
        let test_session = crate::SessionEnvironment::is_test();
        #[cfg(not(any(test, debug_assertions)))]
        let test_session = false;

        Self::for_environment(application_data, test_session)
    }

    pub(crate) fn for_environment(application_data: PathBuf, test_session: bool) -> Self {
        let directory = if test_session {
            application_data.join("safe-storage-test")
        } else {
            application_data.join("safe-storage")
        };
        Self {
            access: Arc::new(StorageAccess {
                directory,
                #[cfg(any(test, all(target_os = "macos", debug_assertions)))]
                test_session,
                desktop_process_required: !test_session,
                root_key: Mutex::new(None),
                #[cfg(all(test, unix))]
                root_fixture: None,
            }),
        }
    }

    #[cfg(any(target_os = "macos", all(test, unix)))]
    pub fn protected_file(&self, relative: impl AsRef<Path>) -> ProtectedFile {
        ProtectedFile::new(
            self.directory().to_path_buf(),
            self.directory().join(relative),
        )
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.access.directory
    }

    #[cfg(any(target_os = "macos", all(test, unix)))]
    pub(crate) fn require_desktop_process(&self) -> Result<(), SafeStorageError> {
        if !self.access.desktop_process_required {
            return Ok(());
        }
        let executable = std::env::current_exe().map_err(|error| {
            SafeStorageError::Io(format!("failed to find current executable: {error}"))
        })?;
        if executable.file_name().and_then(|name| name.to_str()) == Some("vmux_desktop") {
            return Ok(());
        }
        Err(SafeStorageError::DesktopProcessRequired)
    }
}

#[cfg(any(target_os = "macos", test))]
pub struct SafeStorage;

#[cfg(any(target_os = "macos", test))]
impl SafeStorage {
    #[cfg(target_os = "macos")]
    pub fn browser_key() -> Result<BrowserKey, SafeStorageError> {
        let context = SafeStorageContext::current();
        Self::cipher(&context, true)?.browser_key()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn seal_mcp(
        context: &SafeStorageContext,
        account: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        Self::cipher(context, true)?.seal_mcp(account, plaintext)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn open_mcp(
        context: &SafeStorageContext,
        account: &str,
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        Self::cipher(context, false)?.open_mcp(account, ciphertext)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn seal_account(
        context: &SafeStorageContext,
        scope: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        Self::cipher(context, true)?.seal_account(scope, plaintext)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn open_account(
        context: &SafeStorageContext,
        scope: &str,
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        Self::cipher(context, false)?.open_account(scope, ciphertext)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn open_account_silent(
        context: &SafeStorageContext,
        scope: &str,
        ciphertext: &[u8],
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError> {
        let Some(cipher) = Self::cipher_silent(context)? else {
            return Ok(None);
        };
        cipher.open_account(scope, ciphertext).map(Some)
    }

    #[cfg(target_os = "macos")]
    pub fn wrap_vault_key(
        context: &SafeStorageContext,
        vault_id: &str,
        key: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        Self::cipher(context, true)?.wrap_vault_key(vault_id, key)
    }

    #[cfg(target_os = "macos")]
    pub fn unwrap_vault_key(
        context: &SafeStorageContext,
        vault_id: &str,
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        Self::cipher(context, false)?.unwrap_vault_key(vault_id, ciphertext)
    }

    #[cfg(target_os = "macos")]
    pub fn unwrap_vault_key_silent(
        context: &SafeStorageContext,
        vault_id: &str,
        ciphertext: &[u8],
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError> {
        let Some(cipher) = Self::cipher_silent(context)? else {
            return Ok(None);
        };
        cipher.unwrap_vault_key(vault_id, ciphertext).map(Some)
    }

    #[cfg(target_os = "macos")]
    fn cipher(
        context: &SafeStorageContext,
        create: bool,
    ) -> Result<SafeStorageCipher, SafeStorageError> {
        let root = context.root_key(create)?;
        Ok(SafeStorageCipher::new(root))
    }

    #[cfg(target_os = "macos")]
    fn cipher_silent(
        context: &SafeStorageContext,
    ) -> Result<Option<SafeStorageCipher>, SafeStorageError> {
        context
            .root_key_silent()
            .map(|root| root.map(SafeStorageCipher::new))
    }
}

pub struct SafeStorageName;

impl SafeStorageName {
    pub fn of(value: &str) -> String {
        let mut encoded = String::with_capacity(value.len() * 2);
        for byte in value.as_bytes() {
            write!(&mut encoded, "{byte:02x}").unwrap();
        }
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_names_are_injective_for_path_punctuation() {
        assert_ne!(
            SafeStorageName::of("work.dev"),
            SafeStorageName::of("work-dev")
        );
        assert_ne!(
            SafeStorageName::of("linear"),
            SafeStorageName::of("linear.")
        );
    }
}
