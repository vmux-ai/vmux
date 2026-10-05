use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::VaultStorage;
use super::keys::{KeyStore, SystemKeyStore};
use super::repository::VaultRepositoryPath;
use super::snapshot::{Hex, KEY_LEN, VaultCrypto};
use super::sync::{VaultLocalState, VaultReconcile};

pub(super) const RECOVERY_DIR: &str = "keys/recovery";
pub(super) const RECOVERY_FILE: &str = "default.ron";
const RECOVERY_AAD_PREFIX: &[u8] = b"vmux-vault-recovery-v1\0";
const RECOVERY_KDF_PREFIX: &[u8] = b"vmux-vault-recovery-kdf-v1\0";
const FORMAT_VERSION: u32 = 1;
const MANIFEST_VERSION: u32 = 3;

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct RecoveryEnvelope {
    pub(super) version: u32,
    pub(super) wrapped_key: Vec<u8>,
}

struct RecoveryKeyLength;

#[derive(Debug)]
pub struct RecoveryKeyCreation {
    pub pending_upload: bool,
}

pub struct GeneratedRecoveryKey(Zeroizing<[u8; KEY_LEN]>);

#[derive(Clone)]
pub struct VaultRecovery {
    root: PathBuf,
    repository: PathBuf,
}

impl hkdf::KeyType for RecoveryKeyLength {
    fn len(&self) -> usize {
        KEY_LEN
    }
}

impl GeneratedRecoveryKey {
    pub fn generate() -> Result<Self, String> {
        let mut bytes = Zeroizing::new([0_u8; KEY_LEN]);
        SystemRandom::new()
            .fill(bytes.as_mut())
            .map_err(|_| "failed to generate secure random data".to_string())?;
        Ok(Self(bytes))
    }

    pub fn display(&self) -> Zeroizing<String> {
        Zeroizing::new(Self::format(self.0.as_ref()))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    pub(super) fn format(key: &[u8]) -> String {
        let encoded = Hex::encode(key);
        let groups = encoded
            .as_bytes()
            .chunks(4)
            .map(|group| std::str::from_utf8(group).unwrap_or_default())
            .collect::<Vec<_>>();
        format!("vmux-{}", groups.join("-"))
    }

    pub(super) fn parse(source: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let compact = source
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|character| !character.is_ascii_whitespace() && *character != '-')
            .collect::<String>();
        let encoded = compact.strip_prefix("vmux").unwrap_or(&compact);
        if encoded.len() != KEY_LEN * 2 {
            return Err("Invalid Vault Recovery Key".to_string());
        }
        let key = Hex::decode(encoded).map_err(|_| "Invalid Vault Recovery Key".to_string())?;
        VaultCrypto::new(&key).map_err(|_| "Invalid Vault Recovery Key".to_string())?;
        Ok(Zeroizing::new(key))
    }
}

impl VaultRecovery {
    pub fn current() -> Self {
        let storage = VaultStorage::current();
        Self {
            root: storage.root,
            repository: storage.repository,
        }
    }

    pub fn create(
        &self,
        recovery_key: GeneratedRecoveryKey,
    ) -> Result<RecoveryKeyCreation, String> {
        self.create_with(&SystemKeyStore, recovery_key.as_bytes())
    }

    pub fn unlock(&self, recovery_key: &str) -> Result<String, String> {
        self.unlock_with(&SystemKeyStore, recovery_key)
    }

    #[cfg(test)]
    pub(super) fn at(root: &Path, repository: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            repository: repository.to_path_buf(),
        }
    }

    pub(super) fn create_with<K: KeyStore>(
        &self,
        keys: &K,
        recovery_key: &[u8],
    ) -> Result<RecoveryKeyCreation, String> {
        let vault = VaultRepositoryPath::at(&self.repository);
        let git = vault.git();
        if RecoveryEnvelope::read(&self.repository)?.is_some() {
            return Err("This Vault already has a Recovery Key".to_string());
        }
        let mut manifest = vault.manifest()?;
        let previous_manifest = manifest.clone();
        let key = RepositoryKey::new(&self.repository, keys).load(&manifest.vault_id)?;
        VaultCrypto::new(recovery_key)?;
        let wrapping_key = derive_recovery_wrapping_key(recovery_key, &manifest.vault_id)?;
        let wrapping_crypto = VaultCrypto::new(&wrapping_key)?;
        let envelope = RecoveryEnvelope {
            version: FORMAT_VERSION,
            wrapped_key: wrapping_crypto.encrypt(&recovery_aad(&manifest.vault_id), &key)?,
        };
        let source = ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::new())
            .map_err(|error| error.to_string())?;
        let directory = self.repository.join(RECOVERY_DIR);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(
            directory.join(RECOVERY_FILE),
            format!("{source}\n").as_bytes(),
        )
        .map_err(|error| error.to_string())?;
        let finalization = (|| {
            manifest.version = MANIFEST_VERSION;
            vault.write_manifest(&manifest)?;
            vault.validate_encrypted_worktree()?;
            git.commit("Add Vault Recovery Key")
        })();
        if let Err(error) = finalization {
            let _ = std::fs::remove_file(directory.join(RECOVERY_FILE));
            let _ = std::fs::remove_dir(&directory);
            let _ = vault.write_manifest(&previous_manifest);
            let _ = git.run(&["reset"]);
            return Err(error);
        }
        let mut pending_upload = false;
        if !git.optional(&["remote", "get-url", "origin"]).is_empty() {
            pending_upload = git
                .current_branch()
                .and_then(|branch| git.run(&["push", "-u", "origin", &branch]))
                .is_err();
        }
        Ok(RecoveryKeyCreation { pending_upload })
    }

    pub(super) fn unlock_with<K: KeyStore>(
        &self,
        keys: &K,
        recovery_key: &str,
    ) -> Result<String, String> {
        let recovery_key = GeneratedRecoveryKey::parse(recovery_key)?;
        let vault = VaultRepositoryPath::at(&self.repository);
        let manifest = vault.manifest()?;
        let envelope = RecoveryEnvelope::read(&self.repository)?
            .ok_or_else(|| "This Vault has no Recovery Key".to_string())?;
        let wrapping_key = derive_recovery_wrapping_key(&recovery_key, &manifest.vault_id)?;
        let key = Zeroizing::new(
            VaultCrypto::new(&wrapping_key)?
                .decrypt(&recovery_aad(&manifest.vault_id), &envelope.wrapped_key)?,
        );
        VaultCrypto::new(&key)?;
        let (_, remote_files) = vault.load_encrypted_snapshot(&key)?;
        keys.store(&manifest.vault_id, &key)?;
        VaultReconcile::run(&self.root, &BTreeMap::new(), &remote_files)?;
        VaultLocalState::write(&self.root, &self.repository)?;
        Ok("Vault unlocked".to_string())
    }
}

impl RecoveryEnvelope {
    pub(super) fn read(repository: &Path) -> Result<Option<Self>, String> {
        let path = repository.join(RECOVERY_DIR).join(RECOVERY_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let envelope = ron::from_str::<Self>(&source)
            .map_err(|error| format!("invalid Vault Recovery Key recipient: {error}"))?;
        if envelope.version != FORMAT_VERSION {
            return Err("unsupported Vault Recovery Key recipient".to_string());
        }
        Ok(Some(envelope))
    }
}

pub(super) struct RepositoryKey<'a, K> {
    repository: &'a Path,
    keys: &'a K,
}

impl<'a, K: KeyStore> RepositoryKey<'a, K> {
    pub(super) fn new(repository: &'a Path, keys: &'a K) -> Self {
        Self { repository, keys }
    }

    pub(super) fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        self.keys.load(vault_id).map_err(|error| {
            let has_recovery = RecoveryEnvelope::read(self.repository)
                .is_ok_and(|envelope| envelope.is_some());
            if !has_recovery {
                "This Vault is locked on this device. No recovery method is registered. Open it on a device that can already unlock it, then add a Recovery Key."
                    .to_string()
            } else {
                error
            }
        })
    }
}

fn derive_recovery_wrapping_key(
    recovery_key: &[u8],
    vault_id: &str,
) -> Result<[u8; KEY_LEN], String> {
    VaultCrypto::new(recovery_key)?;
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, vault_id.as_bytes());
    let prk = salt.extract(recovery_key);
    let info = [RECOVERY_KDF_PREFIX];
    let output = prk
        .expand(&info, RecoveryKeyLength)
        .map_err(|_| "failed to derive Vault Recovery Key".to_string())?;
    let mut key = [0_u8; KEY_LEN];
    output
        .fill(&mut key)
        .map_err(|_| "failed to derive Vault Recovery Key".to_string())?;
    Ok(key)
}

fn recovery_aad(vault_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(RECOVERY_AAD_PREFIX.len() + vault_id.len());
    aad.extend_from_slice(RECOVERY_AAD_PREFIX);
    aad.extend_from_slice(vault_id.as_bytes());
    aad
}
