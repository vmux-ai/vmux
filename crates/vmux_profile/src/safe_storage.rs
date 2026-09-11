#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(not(target_os = "macos"), test))]
mod other;

#[cfg(any(target_os = "macos", test))]
use std::io::Write;
#[cfg(any(target_os = "macos", test))]
use std::num::NonZeroU32;
#[cfg(any(target_os = "macos", test))]
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", test))]
use std::sync::{Mutex, OnceLock};

#[cfg(any(target_os = "macos", test))]
use ring::{aead, hkdf, pbkdf2};
#[cfg(any(target_os = "macos", test))]
use zeroize::Zeroizing;

#[cfg(any(target_os = "macos", test))]
const ROOT_KEY_LENGTH: usize = 32;
#[cfg(any(target_os = "macos", test))]
const BROWSER_KEY_LENGTH: usize = 16;
#[cfg(any(target_os = "macos", test))]
const DATA_KEY_LENGTH: usize = 32;
#[cfg(any(target_os = "macos", test))]
const NONCE_LENGTH: usize = 12;
#[cfg(any(target_os = "macos", test))]
const ROOT_HEADER: &[u8] = b"vmux-safe-storage-root-v1\0";
#[cfg(any(target_os = "macos", test))]
const ENVELOPE_HEADER: &[u8] = b"vmux-safe-storage-envelope-v1\0";
#[cfg(any(target_os = "macos", test))]
const HKDF_SALT: &[u8] = b"vmux-safe-storage-hkdf-v1";
#[cfg(any(target_os = "macos", test))]
const BROWSER_CONTEXT: &[u8] = b"vmux-safe-storage-browser-v1";
#[cfg(any(target_os = "macos", test))]
const MCP_CONTEXT: &[u8] = b"vmux-safe-storage-mcp-v1";
#[cfg(any(target_os = "macos", test))]
const VAULT_CONTEXT: &[u8] = b"vmux-safe-storage-vault-wrap-v1";
#[cfg(any(target_os = "macos", test))]
const STATE_VERSION: &[u8] = b"1\n";

#[cfg(any(target_os = "macos", test))]
static ROOT_KEY: OnceLock<Mutex<Option<Zeroizing<Vec<u8>>>>> = OnceLock::new();
#[cfg(any(target_os = "macos", test))]
static FILE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "macos")]
pub struct BrowserEncryptionKeys {
    pub current: Zeroizing<Vec<u8>>,
    pub legacy: Option<Zeroizing<Vec<u8>>>,
}

#[cfg(any(target_os = "macos", test))]
pub struct SafeStorage;

#[cfg(any(target_os = "macos", test))]
struct RootKeyStore;

#[cfg(any(target_os = "macos", test))]
struct SafeStorageCipher {
    root: Zeroizing<Vec<u8>>,
}

#[cfg(any(target_os = "macos", test))]
struct DerivedKeyLength(usize);

#[cfg(any(target_os = "macos", test))]
pub(crate) struct ProtectedFile {
    path: PathBuf,
}

#[cfg(any(target_os = "macos", test))]
impl SafeStorage {
    #[cfg(target_os = "macos")]
    pub fn browser_keys() -> Result<BrowserEncryptionKeys, String> {
        let cipher = Self::cipher(true)?;
        let current = cipher.derive(BROWSER_CONTEXT, &[], BROWSER_KEY_LENGTH)?;
        let legacy = if crate::is_test_session() {
            None
        } else {
            RootKeyStore::legacy_browser_key()?
        };
        Ok(BrowserEncryptionKeys { current, legacy })
    }

    pub(crate) fn seal_mcp(account: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        Self::cipher(true)?.seal(MCP_CONTEXT, &[], account.as_bytes(), plaintext)
    }

    pub(crate) fn open_mcp(account: &str, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        Self::cipher(false)?.open(MCP_CONTEXT, &[], account.as_bytes(), ciphertext)
    }

    pub(crate) fn wrap_vault_key(vault_id: &str, key: &[u8]) -> Result<Vec<u8>, String> {
        Self::cipher(true)?.seal(VAULT_CONTEXT, vault_id.as_bytes(), vault_id.as_bytes(), key)
    }

    pub(crate) fn unwrap_vault_key(
        vault_id: &str,
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, String> {
        Self::cipher(false)?.open(
            VAULT_CONTEXT,
            vault_id.as_bytes(),
            vault_id.as_bytes(),
            ciphertext,
        )
    }

    pub(crate) fn unwrap_vault_key_silent(
        vault_id: &str,
        ciphertext: &[u8],
    ) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        let Some(cipher) = Self::cipher_silent()? else {
            return Ok(None);
        };
        cipher
            .open(
                VAULT_CONTEXT,
                vault_id.as_bytes(),
                vault_id.as_bytes(),
                ciphertext,
            )
            .map(Some)
    }

    fn cipher(create: bool) -> Result<SafeStorageCipher, String> {
        let root = Self::root_key(create)?;
        Ok(SafeStorageCipher { root })
    }

    fn cipher_silent() -> Result<Option<SafeStorageCipher>, String> {
        Self::root_key_silent().map(|root| root.map(|root| SafeStorageCipher { root }))
    }

    fn root_key(create: bool) -> Result<Zeroizing<Vec<u8>>, String> {
        #[cfg(test)]
        if crate::is_test_session() {
            return Ok(Zeroizing::new(vec![0x56; ROOT_KEY_LENGTH]));
        }
        let mut cached = ROOT_KEY
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(key) = cached.as_ref() {
            return Ok(Zeroizing::new(key.to_vec()));
        }
        if let Some(payload) = RootKeyStore::read(false)? {
            let key = Self::decode_root_key(&payload)?;
            SafeStorageState::write()?;
            *cached = Some(Zeroizing::new(key.to_vec()));
            return Ok(key);
        }
        if !create {
            return Err(Self::missing_root_error());
        }
        if SafeStorageState::exists() {
            return Err(Self::missing_root_error());
        }
        let key = Self::random_key(ROOT_KEY_LENGTH)?;
        RootKeyStore::write(&Self::encode_root_key(&key))?;
        SafeStorageState::write()?;
        *cached = Some(Zeroizing::new(key.to_vec()));
        Ok(key)
    }

    fn root_key_silent() -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        #[cfg(test)]
        if crate::is_test_session() {
            return Ok(Some(Zeroizing::new(vec![0x56; ROOT_KEY_LENGTH])));
        }
        let mut cached = ROOT_KEY
            .get_or_init(Default::default)
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(key) = cached.as_ref() {
            return Ok(Some(Zeroizing::new(key.to_vec())));
        }
        let Some(payload) = RootKeyStore::read(true)? else {
            if SafeStorageState::exists() {
                return Err(Self::missing_root_error());
            }
            return Ok(None);
        };
        let key = Self::decode_root_key(&payload)?;
        *cached = Some(Zeroizing::new(key.to_vec()));
        Ok(Some(key))
    }

    fn random_key(length: usize) -> Result<Zeroizing<Vec<u8>>, String> {
        use ring::rand::SecureRandom;

        let mut key = Zeroizing::new(vec![0_u8; length]);
        ring::rand::SystemRandom::new()
            .fill(&mut key)
            .map_err(|_| "failed to generate Vmux Safe Storage key".to_string())?;
        Ok(key)
    }

    fn encode_root_key(key: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(ROOT_HEADER.len() + key.len());
        payload.extend_from_slice(ROOT_HEADER);
        payload.extend_from_slice(key);
        payload
    }

    fn decode_root_key(payload: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        let Some(key) = payload.strip_prefix(ROOT_HEADER) else {
            return Err("Vmux Safe Storage root key has an unsupported version".to_string());
        };
        if key.len() != ROOT_KEY_LENGTH {
            return Err("Vmux Safe Storage root key is corrupt".to_string());
        }
        Ok(Zeroizing::new(key.to_vec()))
    }

    fn missing_root_error() -> String {
        "Vmux Safe Storage root key is missing; sign in again and recover encrypted Vaults"
            .to_string()
    }
}

#[cfg(any(target_os = "macos", test))]
impl SafeStorageCipher {
    fn derive(
        &self,
        label: &[u8],
        context: &[u8],
        length: usize,
    ) -> Result<Zeroizing<Vec<u8>>, String> {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, HKDF_SALT);
        let prk = salt.extract(&self.root);
        let info = [label, context];
        let output = prk
            .expand(&info, DerivedKeyLength(length))
            .map_err(|_| "failed to derive Vmux Safe Storage key".to_string())?;
        let mut key = Zeroizing::new(vec![0_u8; length]);
        output
            .fill(&mut key)
            .map_err(|_| "failed to derive Vmux Safe Storage key".to_string())?;
        Ok(key)
    }

    fn seal(
        &self,
        label: &[u8],
        context: &[u8],
        aad: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, String> {
        use ring::rand::SecureRandom;

        let key = self.derive(label, context, DATA_KEY_LENGTH)?;
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, &key)
            .map_err(|_| "failed to initialize Vmux Safe Storage encryption".to_string())?;
        let key = aead::LessSafeKey::new(key);
        let mut nonce = [0_u8; NONCE_LENGTH];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| "failed to generate Vmux Safe Storage nonce".to_string())?;
        let mut encrypted = plaintext.to_vec();
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut encrypted,
        )
        .map_err(|_| "failed to encrypt Vmux Safe Storage data".to_string())?;
        let mut envelope =
            Vec::with_capacity(ENVELOPE_HEADER.len() + nonce.len() + encrypted.len());
        envelope.extend_from_slice(ENVELOPE_HEADER);
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&encrypted);
        Ok(envelope)
    }

    fn open(
        &self,
        label: &[u8],
        context: &[u8],
        aad: &[u8],
        envelope: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, String> {
        let Some(envelope) = envelope.strip_prefix(ENVELOPE_HEADER) else {
            return Err("Vmux Safe Storage data has an unsupported version".to_string());
        };
        if envelope.len() < NONCE_LENGTH + aead::AES_256_GCM.tag_len() {
            return Err("Vmux Safe Storage data is corrupt".to_string());
        }
        let (nonce, ciphertext) = envelope.split_at(NONCE_LENGTH);
        let nonce: [u8; NONCE_LENGTH] = nonce
            .try_into()
            .map_err(|_| "Vmux Safe Storage data is corrupt".to_string())?;
        let key = self.derive(label, context, DATA_KEY_LENGTH)?;
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, &key)
            .map_err(|_| "failed to initialize Vmux Safe Storage decryption".to_string())?;
        let key = aead::LessSafeKey::new(key);
        let mut plaintext = Zeroizing::new(ciphertext.to_vec());
        let length = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut plaintext,
            )
            .map_err(|_| "failed to decrypt Vmux Safe Storage data".to_string())?
            .len();
        plaintext.truncate(length);
        Ok(plaintext)
    }
}

#[cfg(any(target_os = "macos", test))]
impl hkdf::KeyType for DerivedKeyLength {
    fn len(&self) -> usize {
        self.0
    }
}

#[cfg(any(target_os = "macos", test))]
impl ProtectedFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn read(&self) -> Result<Option<Vec<u8>>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("failed to read {}: {error}", self.path.display())),
        }
    }

    pub(crate) fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let Some(parent) = self.path.parent() else {
            return Err("Vmux Safe Storage path has no parent".to_string());
        };
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        let safe_storage = crate::application_data_dir().join("safe-storage");
        Self::restrict_directory(&safe_storage)?;
        Self::restrict_directory(parent)?;
        let sequence = FILE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("safe-storage");
        let temporary = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
        let result = Self::write_temporary(&temporary, bytes)
            .and_then(|_| {
                std::fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
            })
            .and_then(|_| {
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|error| error.to_string())
            });
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|error| format!("failed to write {}: {error}", self.path.display()))
    }

    pub(crate) fn remove(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("failed to remove {}: {error}", self.path.display())),
        }
    }

    fn write_temporary(path: &Path, bytes: &[u8]) -> Result<(), String> {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())
    }

    #[cfg(unix)]
    fn restrict_directory(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())
    }

    #[cfg(not(unix))]
    fn restrict_directory(_path: &Path) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(any(target_os = "macos", test))]
struct SafeStorageState;

#[cfg(any(target_os = "macos", test))]
impl SafeStorageState {
    fn file() -> ProtectedFile {
        ProtectedFile::new(
            crate::application_data_dir()
                .join("safe-storage")
                .join("version"),
        )
    }

    fn exists() -> bool {
        if Self::file().path.is_file() {
            return true;
        }
        let root = crate::application_data_dir().join("safe-storage");
        for directory in [root.join("mcp"), root.join("vault")] {
            if std::fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_some()) {
                return true;
            }
        }
        false
    }

    fn write() -> Result<(), String> {
        if Self::exists() {
            return Ok(());
        }
        Self::file().write(STATE_VERSION)
    }
}

#[cfg(any(target_os = "macos", test))]
fn derive_legacy_browser_key(password: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut key = Zeroizing::new(vec![0_u8; BROWSER_KEY_LENGTH]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA1,
        NonZeroU32::new(1003).unwrap(),
        b"saltysalt",
        password,
        &mut key,
    );
    key
}

pub(crate) fn encoded_file_name(value: &str) -> String {
    use std::fmt::Write;

    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

#[cfg(target_os = "macos")]
pub(crate) fn require_desktop_process() -> Result<(), String> {
    if crate::is_test_session() {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    if executable.file_name().and_then(|name| name.to_str()) == Some("vmux_desktop") {
        return Ok(());
    }
    Err("Vmux Safe Storage is only available to the desktop process".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> SafeStorageCipher {
        SafeStorageCipher {
            root: Zeroizing::new((0..ROOT_KEY_LENGTH).map(|value| value as u8).collect()),
        }
    }

    #[test]
    fn derived_keys_are_domain_separated() {
        let cipher = cipher();
        let browser = cipher
            .derive(BROWSER_CONTEXT, &[], BROWSER_KEY_LENGTH)
            .unwrap();
        let mcp = cipher.derive(MCP_CONTEXT, &[], BROWSER_KEY_LENGTH).unwrap();
        let first_vault = cipher
            .derive(VAULT_CONTEXT, b"first", BROWSER_KEY_LENGTH)
            .unwrap();
        let second_vault = cipher
            .derive(VAULT_CONTEXT, b"second", BROWSER_KEY_LENGTH)
            .unwrap();

        assert_ne!(browser.as_slice(), mcp.as_slice());
        assert_ne!(mcp.as_slice(), first_vault.as_slice());
        assert_ne!(first_vault.as_slice(), second_vault.as_slice());
    }

    #[test]
    fn encrypted_data_requires_the_matching_domain_and_context() {
        let cipher = cipher();
        let encrypted = cipher
            .seal(MCP_CONTEXT, &[], b"personal:linear", b"secret")
            .unwrap();

        assert_eq!(
            cipher
                .open(MCP_CONTEXT, &[], b"personal:linear", &encrypted)
                .unwrap()
                .as_slice(),
            b"secret"
        );
        assert!(
            cipher
                .open(MCP_CONTEXT, &[], b"personal:github", &encrypted)
                .is_err()
        );
        assert!(
            cipher
                .open(
                    VAULT_CONTEXT,
                    b"personal:linear",
                    b"personal:linear",
                    &encrypted
                )
                .is_err()
        );
    }

    #[test]
    fn encrypted_data_rejects_tampering() {
        let cipher = cipher();
        let mut encrypted = cipher
            .seal(VAULT_CONTEXT, b"vault", b"vault", b"secret")
            .unwrap();
        *encrypted.last_mut().unwrap() ^= 1;

        assert!(
            cipher
                .open(VAULT_CONTEXT, b"vault", b"vault", &encrypted)
                .is_err()
        );
    }

    #[test]
    fn root_key_payload_is_versioned_and_validated() {
        let key = vec![7_u8; ROOT_KEY_LENGTH];
        let payload = SafeStorage::encode_root_key(&key);

        assert_eq!(
            SafeStorage::decode_root_key(&payload).unwrap().as_slice(),
            key
        );
        assert!(SafeStorage::decode_root_key(&key).is_err());
        assert!(SafeStorage::decode_root_key(ROOT_HEADER).is_err());
    }

    #[test]
    fn legacy_browser_key_matches_chromium_derivation() {
        assert_eq!(
            derive_legacy_browser_key(b"peanuts").as_slice(),
            [
                0xd9, 0xa0, 0x9d, 0x49, 0x9b, 0x4e, 0x1b, 0x74, 0x61, 0xf2, 0x8e, 0x67, 0x97, 0x2c,
                0x6d, 0xbd
            ]
        );
    }

    #[test]
    fn encoded_names_are_injective_for_path_punctuation() {
        assert_ne!(encoded_file_name("work.dev"), encoded_file_name("work-dev"));
        assert_ne!(encoded_file_name("linear"), encoded_file_name("linear."));
    }
}
