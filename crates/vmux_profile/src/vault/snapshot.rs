use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use ring::rand::{SecureRandom, SystemRandom};
use ring::{aead, digest, hmac};
use serde::{Deserialize, Serialize};

use super::repository::{read_manifest, validate_encrypted_worktree, write_manifest};
use super::sync::{remove_existing_path, same_entry};

pub(super) const FORMAT_VERSION: u32 = 1;
pub(super) const MANIFEST_VERSION: u32 = 3;
pub(super) const MANIFEST_FILE: &str = "vault.ron";
pub(super) const INDEX_FILE: &str = "index.enc";
pub(super) const OBJECTS_DIR: &str = "objects";
const INDEX_AAD: &[u8] = b"vmux-vault-index-v1";
const OBJECT_AAD_PREFIX: &[u8] = b"vmux-vault-object-v1\0";
pub(super) const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntryKind {
    File,
    Symlink,
}

#[derive(Clone, Debug)]
pub(super) struct LocalEntry {
    pub(super) kind: EntryKind,
    pub(super) mode: u32,
    pub(super) size: u64,
    pub(super) modified_secs: u64,
    pub(super) modified_nanos: u32,
    pub(super) data: Vec<u8>,
    pub(super) digest: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LocalFingerprint {
    pub(super) kind: EntryKind,
    pub(super) mode: u32,
    pub(super) size: u64,
    pub(super) modified_secs: u64,
    pub(super) modified_nanos: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RemoteManifest {
    pub(super) version: u32,
    pub(super) cipher: String,
    pub(super) vault_id: String,
    pub(super) index: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct EncryptedIndex {
    pub(super) version: u32,
    pub(super) files: Vec<EncryptedIndexEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct EncryptedIndexEntry {
    pub(super) path: String,
    pub(super) object: String,
    pub(super) digest: String,
    pub(super) kind: EntryKind,
    pub(super) mode: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct LocalState {
    pub(super) version: u32,
    pub(super) files: Vec<LocalStateEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct LocalStateEntry {
    pub(super) path: String,
    pub(super) digest: String,
    pub(super) kind: EntryKind,
    pub(super) mode: u32,
    #[serde(default)]
    pub(super) data: Option<Vec<u8>>,
    #[serde(default)]
    pub(super) size: u64,
    #[serde(default)]
    pub(super) modified_secs: u64,
    #[serde(default)]
    pub(super) modified_nanos: u32,
}

pub(super) fn write_encrypted_snapshot(
    repository: &Path,
    vault_id: &str,
    key: &[u8],
    files: &BTreeMap<String, LocalEntry>,
    previous: Option<&BTreeMap<String, LocalEntry>>,
) -> Result<(), String> {
    validate_key(key)?;
    if previous.is_some_and(|previous| same_files(previous, files))
        && repository.join(MANIFEST_FILE).is_file()
        && repository.join(INDEX_FILE).is_file()
        && read_manifest(repository).is_ok_and(|manifest| manifest.version == MANIFEST_VERSION)
    {
        return validate_encrypted_worktree(repository);
    }
    let objects = repository.join(OBJECTS_DIR);
    std::fs::create_dir_all(&objects).map_err(|error| error.to_string())?;
    let mut index_files = Vec::with_capacity(files.len());
    let mut retained = BTreeSet::new();
    for (path, entry) in files {
        validate_relative_path(path)?;
        let object = object_id(key, path);
        retained.insert(object.clone());
        let object_path = objects.join(&object);
        let unchanged = previous
            .and_then(|files| files.get(path))
            .is_some_and(|old| same_entry(Some(old), Some(entry)))
            && object_path.is_file();
        if !unchanged {
            let encrypted = encrypt_bytes(key, &object_aad(path), &entry.data)?;
            write_atomic(&object_path, &encrypted)?;
        }
        index_files.push(EncryptedIndexEntry {
            path: path.clone(),
            object,
            digest: entry.digest.clone(),
            kind: entry.kind,
            mode: entry.mode,
        });
    }
    for entry in std::fs::read_dir(&objects)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !retained.contains(&name) {
            remove_existing_path(&entry.path())?;
        }
    }
    let index = EncryptedIndex {
        version: FORMAT_VERSION,
        files: index_files,
    };
    let index_source = ron::ser::to_string(&index)
        .map_err(|error| error.to_string())?
        .into_bytes();
    let encrypted_index = encrypt_bytes(key, INDEX_AAD, &index_source)?;
    write_atomic(&repository.join(INDEX_FILE), &encrypted_index)?;
    let manifest = RemoteManifest {
        version: MANIFEST_VERSION,
        cipher: "AES-256-GCM".to_string(),
        vault_id: vault_id.to_string(),
        index: INDEX_FILE.to_string(),
    };
    write_manifest(repository, &manifest)?;
    validate_encrypted_worktree(repository)
}

pub(super) fn same_files(
    left: &BTreeMap<String, LocalEntry>,
    right: &BTreeMap<String, LocalEntry>,
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|(path, entry)| same_entry(Some(entry), right.get(path)))
}

pub(super) fn load_encrypted_snapshot(
    repository: &Path,
    key: &[u8],
) -> Result<(RemoteManifest, BTreeMap<String, LocalEntry>), String> {
    validate_key(key)?;
    let manifest = read_manifest(repository)?;
    let encrypted_index = std::fs::read(repository.join(&manifest.index))
        .map_err(|error| format!("failed to read encrypted Vault index: {error}"))?;
    let index_source = decrypt_bytes(key, INDEX_AAD, &encrypted_index)?;
    let index_source = std::str::from_utf8(&index_source)
        .map_err(|error| format!("invalid encrypted Vault index: {error}"))?;
    let index = ron::from_str::<EncryptedIndex>(index_source)
        .map_err(|error| format!("invalid encrypted Vault index: {error}"))?;
    if index.version != FORMAT_VERSION {
        return Err(format!(
            "unsupported encrypted Vault index {}",
            index.version
        ));
    }
    let mut files = BTreeMap::new();
    for file in index.files {
        validate_relative_path(&file.path)?;
        let expected_object = object_id(key, &file.path);
        if file.object != expected_object {
            return Err(format!("encrypted Vault object mismatch for {}", file.path));
        }
        let encrypted = std::fs::read(repository.join(OBJECTS_DIR).join(&file.object))
            .map_err(|error| format!("missing encrypted Vault object: {error}"))?;
        let data = decrypt_bytes(key, &object_aad(&file.path), &encrypted)?;
        let actual_digest = entry_digest(file.kind, file.mode, &data);
        if actual_digest != file.digest {
            return Err(format!(
                "encrypted Vault object failed integrity check: {}",
                file.path
            ));
        }
        if files
            .insert(
                file.path.clone(),
                LocalEntry {
                    kind: file.kind,
                    mode: file.mode,
                    size: data.len() as u64,
                    modified_secs: 0,
                    modified_nanos: 0,
                    data,
                    digest: file.digest,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate encrypted Vault path: {}", file.path));
        }
    }
    Ok((manifest, files))
}

pub(super) fn encrypt_bytes(key: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    validate_key(key)?;
    let mut nonce = [0_u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| "failed to generate Vault nonce".to_string())?;
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key)
        .map_err(|_| "invalid Vault encryption key".to_string())?;
    let key = aead::LessSafeKey::new(unbound);
    let mut encrypted = plaintext.to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(aad),
        &mut encrypted,
    )
    .map_err(|_| "failed to encrypt Vault data".to_string())?;
    let mut output = nonce.to_vec();
    output.extend_from_slice(&encrypted);
    Ok(output)
}

pub(super) fn decrypt_bytes(key: &[u8], aad: &[u8], encrypted: &[u8]) -> Result<Vec<u8>, String> {
    validate_key(key)?;
    if encrypted.len() < NONCE_LEN + aead::AES_256_GCM.tag_len() {
        return Err("encrypted Vault data is truncated".to_string());
    }
    let nonce = <[u8; NONCE_LEN]>::try_from(&encrypted[..NONCE_LEN])
        .map_err(|_| "invalid Vault nonce".to_string())?;
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key)
        .map_err(|_| "invalid Vault encryption key".to_string())?;
    let key = aead::LessSafeKey::new(unbound);
    let mut data = encrypted[NONCE_LEN..].to_vec();
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut data,
        )
        .map_err(|_| "Vault data could not be decrypted or was modified".to_string())?;
    Ok(plaintext.to_vec())
}

pub(super) fn object_id(key: &[u8], path: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hex(hmac::sign(&key, path.as_bytes()).as_ref())
}

pub(super) fn object_aad(path: &str) -> Vec<u8> {
    let mut aad = OBJECT_AAD_PREFIX.to_vec();
    aad.extend_from_slice(path.as_bytes());
    aad
}

pub(super) fn entry_digest(kind: EntryKind, mode: u32, data: &[u8]) -> String {
    let mut context = digest::Context::new(&digest::SHA256);
    context.update(match kind {
        EntryKind::File => b"file\0",
        EntryKind::Symlink => b"symlink\0",
    });
    context.update(&mode.to_be_bytes());
    context.update(data);
    hex(context.finish().as_ref())
}

pub(super) fn validate_key(key: &[u8]) -> Result<(), String> {
    if key.len() == KEY_LEN {
        Ok(())
    } else {
        Err("Vault encryption key has an invalid length".to_string())
    }
}

pub(super) fn validate_relative_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("encrypted Vault contains an unsafe path".to_string());
    }
    Ok(())
}

pub(super) fn state_path(repository: &Path) -> PathBuf {
    repository.join(".git").join("vmux-state.ron")
}

pub(super) fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("vault");
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", random_hex(8)?));
    std::fs::write(&temporary, data).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

pub(super) fn random_hex(bytes: usize) -> Result<String, String> {
    let mut value = vec![0_u8; bytes];
    SystemRandom::new()
        .fill(&mut value)
        .map_err(|_| "failed to generate secure random data".to_string())?;
    Ok(hex(&value))
}

pub(super) fn modified_time(metadata: &std::fs::Metadata) -> (u64, u32) {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_default()
}

pub(super) fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(super) fn decode_hex(source: &str) -> Result<Vec<u8>, String> {
    if !source.len().is_multiple_of(2) {
        return Err("invalid hexadecimal data".to_string());
    }
    source
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = decode_hex_digit(pair[0])?;
            let low = decode_hex_digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

pub(super) fn decode_hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("invalid hexadecimal data".to_string()),
    }
}
