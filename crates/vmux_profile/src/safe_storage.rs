#[cfg(any(target_os = "macos", test))]
mod cipher;
#[cfg(target_os = "macos")]
mod file;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod root;

use std::fmt::{Display, Formatter};
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(target_os = "macos", test))]
use std::path::PathBuf;

#[cfg(any(target_os = "macos", test))]
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
use cipher::SafeStorageCipher;
#[cfg(target_os = "macos")]
pub(crate) use file::ProtectedFile;

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
enum RootKeySource {
    Keychain,
    #[cfg(any(test, debug_assertions))]
    FixedTest,
}

#[cfg(any(target_os = "macos", test))]
pub struct SafeStorageContext {
    directory: PathBuf,
    root_key_source: RootKeySource,
    desktop_process_required: bool,
}

#[cfg(any(target_os = "macos", test))]
impl SafeStorageContext {
    #[cfg(target_os = "macos")]
    pub fn current() -> Self {
        #[cfg(any(test, debug_assertions))]
        let test_session = crate::is_test_session();
        #[cfg(not(any(test, debug_assertions)))]
        let test_session = false;

        Self::for_environment(crate::application_data_dir(), test_session)
    }

    fn for_environment(application_data: PathBuf, test_session: bool) -> Self {
        if test_session {
            return Self {
                directory: application_data.join("safe-storage-test"),
                #[cfg(any(test, debug_assertions))]
                root_key_source: RootKeySource::FixedTest,
                #[cfg(not(any(test, debug_assertions)))]
                root_key_source: RootKeySource::Keychain,
                desktop_process_required: false,
            };
        }
        Self {
            directory: application_data.join("safe-storage"),
            root_key_source: RootKeySource::Keychain,
            desktop_process_required: true,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn protected_file(&self, relative: impl AsRef<Path>) -> ProtectedFile {
        ProtectedFile::new(self.directory.clone(), self.directory.join(relative))
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn require_desktop_process(&self) -> Result<(), SafeStorageError> {
        if !self.desktop_process_required {
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

    #[cfg(test)]
    fn is_test(&self) -> bool {
        matches!(self.root_key_source, RootKeySource::FixedTest)
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
    pub(crate) fn wrap_vault_key(
        context: &SafeStorageContext,
        vault_id: &str,
        key: &[u8],
    ) -> Result<Vec<u8>, SafeStorageError> {
        Self::cipher(context, true)?.wrap_vault_key(vault_id, key)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn unwrap_vault_key(
        context: &SafeStorageContext,
        vault_id: &str,
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, SafeStorageError> {
        Self::cipher(context, false)?.unwrap_vault_key(vault_id, ciphertext)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn unwrap_vault_key_silent(
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

pub(crate) fn encoded_file_name(value: &str) -> String {
    use std::fmt::Write;

    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_uses_separate_storage_without_desktop_access() {
        let application_data = PathBuf::from("/tmp/vmux-context");
        let context = SafeStorageContext::for_environment(application_data.clone(), true);

        assert_eq!(
            context.directory(),
            application_data.join("safe-storage-test")
        );
        assert!(context.is_test());
        assert!(!context.desktop_process_required);
    }

    #[test]
    fn production_context_uses_keychain_backed_storage() {
        let application_data = PathBuf::from("/tmp/vmux-context");
        let context = SafeStorageContext::for_environment(application_data.clone(), false);

        assert_eq!(context.directory(), application_data.join("safe-storage"));
        assert!(!context.is_test());
        assert!(context.desktop_process_required);
    }

    #[test]
    fn encoded_names_are_injective_for_path_punctuation() {
        assert_ne!(encoded_file_name("work.dev"), encoded_file_name("work-dev"));
        assert_ne!(encoded_file_name("linear"), encoded_file_name("linear."));
    }
}
