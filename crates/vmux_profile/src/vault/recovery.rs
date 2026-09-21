use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ring::hkdf;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::keys::{KeyStore, SystemKeyStore};
use super::repository::{
    commit_changes, current_branch, git, git_optional, read_manifest, validate_encrypted_worktree,
    write_manifest,
};
use super::snapshot::{
    KEY_LEN, decode_hex, decrypt_bytes, encrypt_bytes, hex, load_encrypted_snapshot, validate_key,
    write_atomic,
};
use super::sync::{reconcile_local, write_local_state};
use super::{repository_dir, root_dir};

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

pub(super) struct RecoveryKeyLength;

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
        Zeroizing::new(format_recovery_key(self.0.as_ref()))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl VaultRecovery {
    pub fn current() -> Self {
        Self {
            root: root_dir(),
            repository: repository_dir(),
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
        if read_recovery_envelope(&self.repository)?.is_some() {
            return Err("This Vault already has a Recovery Key".to_string());
        }
        let mut manifest = read_manifest(&self.repository)?;
        let previous_manifest = manifest.clone();
        let key = load_repository_key(&self.repository, keys, &manifest.vault_id)?;
        validate_key(recovery_key)?;
        let wrapping_key = derive_recovery_wrapping_key(recovery_key, &manifest.vault_id)?;
        let envelope = RecoveryEnvelope {
            version: FORMAT_VERSION,
            wrapped_key: encrypt_bytes(&wrapping_key, &recovery_aad(&manifest.vault_id), &key)?,
        };
        let source = ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::new())
            .map_err(|error| error.to_string())?;
        let directory = self.repository.join(RECOVERY_DIR);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        write_atomic(
            &directory.join(RECOVERY_FILE),
            format!("{source}\n").as_bytes(),
        )?;
        let finalization = (|| {
            manifest.version = MANIFEST_VERSION;
            write_manifest(&self.repository, &manifest)?;
            validate_encrypted_worktree(&self.repository)?;
            commit_changes(&self.repository, "Add Vault Recovery Key")
        })();
        if let Err(error) = finalization {
            let _ = std::fs::remove_file(directory.join(RECOVERY_FILE));
            let _ = std::fs::remove_dir(&directory);
            let _ = write_manifest(&self.repository, &previous_manifest);
            let _ = git(&self.repository, &["reset"]);
            return Err(error);
        }
        let mut pending_upload = false;
        if !git_optional(&self.repository, &["remote", "get-url", "origin"]).is_empty() {
            pending_upload = current_branch(&self.repository)
                .and_then(|branch| git(&self.repository, &["push", "-u", "origin", &branch]))
                .is_err();
        }
        Ok(RecoveryKeyCreation { pending_upload })
    }

    pub(super) fn unlock_with<K: KeyStore>(
        &self,
        keys: &K,
        recovery_key: &str,
    ) -> Result<String, String> {
        let recovery_key = parse_recovery_key(recovery_key)?;
        let manifest = read_manifest(&self.repository)?;
        let envelope = read_recovery_envelope(&self.repository)?
            .ok_or_else(|| "This Vault has no Recovery Key".to_string())?;
        let wrapping_key = derive_recovery_wrapping_key(&recovery_key, &manifest.vault_id)?;
        let key = Zeroizing::new(decrypt_bytes(
            &wrapping_key,
            &recovery_aad(&manifest.vault_id),
            &envelope.wrapped_key,
        )?);
        validate_key(&key)?;
        let (_, remote_files) = load_encrypted_snapshot(&self.repository, &key)?;
        keys.store(&manifest.vault_id, &key)?;
        reconcile_local(&self.root, &BTreeMap::new(), &remote_files)?;
        write_local_state(&self.root, &self.repository)?;
        Ok("Vault unlocked".to_string())
    }
}

pub(super) fn format_recovery_key(key: &[u8]) -> String {
    let encoded = hex(key);
    let groups = encoded
        .as_bytes()
        .chunks(4)
        .map(|group| std::str::from_utf8(group).unwrap_or_default())
        .collect::<Vec<_>>();
    format!("vmux-{}", groups.join("-"))
}

pub(super) fn read_recovery_envelope(
    repository: &Path,
) -> Result<Option<RecoveryEnvelope>, String> {
    let path = repository.join(RECOVERY_DIR).join(RECOVERY_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let envelope = ron::from_str::<RecoveryEnvelope>(&source)
        .map_err(|error| format!("invalid Vault Recovery Key recipient: {error}"))?;
    if envelope.version != FORMAT_VERSION {
        return Err("unsupported Vault Recovery Key recipient".to_string());
    }
    Ok(Some(envelope))
}

pub(super) fn load_repository_key<K: KeyStore>(
    repository: &Path,
    keys: &K,
    vault_id: &str,
) -> Result<Zeroizing<Vec<u8>>, String> {
    keys.load(vault_id).map_err(|error| {
        let has_recovery =
            read_recovery_envelope(repository).is_ok_and(|envelope| envelope.is_some());
        if !has_recovery {
            "This Vault is locked on this device. No recovery method is registered. Open it on a device that can already unlock it, then add a Recovery Key."
                .to_string()
        } else {
            error
        }
    })
}

pub(super) fn derive_recovery_wrapping_key(
    recovery_key: &[u8],
    vault_id: &str,
) -> Result<[u8; KEY_LEN], String> {
    validate_key(recovery_key)?;
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

pub(super) fn recovery_aad(vault_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(RECOVERY_AAD_PREFIX.len() + vault_id.len());
    aad.extend_from_slice(RECOVERY_AAD_PREFIX);
    aad.extend_from_slice(vault_id.as_bytes());
    aad
}

pub(super) fn parse_recovery_key(source: &str) -> Result<Zeroizing<Vec<u8>>, String> {
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
    let key = decode_hex(encoded).map_err(|_| "Invalid Vault Recovery Key".to_string())?;
    validate_key(&key).map_err(|_| "Invalid Vault Recovery Key".to_string())?;
    Ok(Zeroizing::new(key))
}
