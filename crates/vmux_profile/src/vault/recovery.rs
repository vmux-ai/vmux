use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

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

impl hkdf::KeyType for RecoveryKeyLength {
    fn len(&self) -> usize {
        KEY_LEN
    }
}

pub(super) static PENDING_RECOVERY_KEY: Mutex<Option<Zeroizing<String>>> = Mutex::new(None);

pub(super) fn pending_recovery_key() -> std::sync::MutexGuard<'static, Option<Zeroizing<String>>> {
    PENDING_RECOVERY_KEY.lock().unwrap_or_else(|poisoned| {
        PENDING_RECOVERY_KEY.clear_poison();
        poisoned.into_inner()
    })
}

pub fn generate_recovery_key() -> Result<Zeroizing<String>, String> {
    let mut bytes = Zeroizing::new(vec![0_u8; KEY_LEN]);
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "failed to generate secure random data".to_string())?;
    let encoded = Zeroizing::new(hex(&bytes));
    let groups = encoded
        .as_bytes()
        .chunks(4)
        .map(|group| std::str::from_utf8(group).unwrap_or_default())
        .collect::<Vec<_>>();
    let key = Zeroizing::new(format!("vmux-{}", groups.join("-")));
    *pending_recovery_key() = Some(key.clone());
    Ok(key)
}

pub fn create_recovery_key() -> Result<RecoveryKeyCreation, String> {
    let key = pending_recovery_key()
        .take()
        .ok_or_else(|| "No Recovery Key has been generated for this Vault".to_string())?;
    create_recovery_key_paths(&repository_dir(), &SystemKeyStore, &key)
}

pub fn unlock_with_recovery_key(recovery_key: &str) -> Result<String, String> {
    unlock_with_recovery_key_paths(
        &root_dir(),
        &repository_dir(),
        &SystemKeyStore,
        recovery_key,
    )
}

pub(super) fn create_recovery_key_paths<K: KeyStore>(
    repository: &Path,
    keys: &K,
    recovery_key: &str,
) -> Result<RecoveryKeyCreation, String> {
    if read_recovery_envelope(repository)?.is_some() {
        return Err("This Vault already has a Recovery Key".to_string());
    }
    let mut manifest = read_manifest(repository)?;
    let previous_manifest = manifest.clone();
    let key = load_repository_key(repository, keys, &manifest.vault_id)?;
    let recovery_key = parse_recovery_key(recovery_key)?;
    let wrapping_key = derive_recovery_wrapping_key(&recovery_key, &manifest.vault_id)?;
    let envelope = RecoveryEnvelope {
        version: FORMAT_VERSION,
        wrapped_key: encrypt_bytes(&wrapping_key, &recovery_aad(&manifest.vault_id), &key)?,
    };
    let source = ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?;
    let directory = repository.join(RECOVERY_DIR);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    write_atomic(
        &directory.join(RECOVERY_FILE),
        format!("{source}\n").as_bytes(),
    )?;
    let finalization = (|| {
        manifest.version = MANIFEST_VERSION;
        write_manifest(repository, &manifest)?;
        validate_encrypted_worktree(repository)?;
        commit_changes(repository, "Add Vault Recovery Key")
    })();
    if let Err(error) = finalization {
        let _ = std::fs::remove_file(directory.join(RECOVERY_FILE));
        let _ = std::fs::remove_dir(&directory);
        let _ = write_manifest(repository, &previous_manifest);
        let _ = git(repository, &["reset"]);
        return Err(error);
    }
    let mut pending_upload = false;
    if !git_optional(repository, &["remote", "get-url", "origin"]).is_empty() {
        pending_upload = current_branch(repository)
            .and_then(|branch| git(repository, &["push", "-u", "origin", &branch]))
            .is_err();
    }
    Ok(RecoveryKeyCreation { pending_upload })
}

pub(super) fn unlock_with_recovery_key_paths<K: KeyStore>(
    root: &Path,
    repository: &Path,
    keys: &K,
    recovery_key: &str,
) -> Result<String, String> {
    let recovery_key = parse_recovery_key(recovery_key)?;
    let manifest = read_manifest(repository)?;
    let envelope = read_recovery_envelope(repository)?
        .ok_or_else(|| "This Vault has no Recovery Key".to_string())?;
    let wrapping_key = derive_recovery_wrapping_key(&recovery_key, &manifest.vault_id)?;
    let key = Zeroizing::new(decrypt_bytes(
        &wrapping_key,
        &recovery_aad(&manifest.vault_id),
        &envelope.wrapped_key,
    )?);
    validate_key(&key)?;
    let (_, remote_files) = load_encrypted_snapshot(repository, &key)?;
    keys.store(&manifest.vault_id, &key)?;
    reconcile_local(root, &BTreeMap::new(), &remote_files)?;
    write_local_state(root, repository)?;
    Ok("Vault unlocked".to_string())
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

#[cfg(test)]
pub(super) fn format_recovery_key(key: &[u8]) -> String {
    let encoded = hex(key);
    let groups = encoded
        .as_bytes()
        .chunks(4)
        .map(|group| std::str::from_utf8(group).unwrap())
        .collect::<Vec<_>>();
    format!("vmux-{}", groups.join("-"))
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
