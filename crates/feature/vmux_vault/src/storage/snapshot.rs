use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use ring::rand::{SecureRandom, SystemRandom};
use ring::{aead, digest, hmac};
use serde::{Deserialize, Serialize};

use super::repository::VaultRepositoryPath;
use super::sync::LocalPath;

pub(super) const FORMAT_VERSION: u32 = 1;
pub(super) const MANIFEST_VERSION: u32 = 3;
pub(crate) const MANIFEST_FILE: &str = "vault.ron";
pub(crate) const INDEX_FILE: &str = "index.enc";
pub(crate) const OBJECTS_DIR: &str = "objects";
pub(super) const KEY_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EntryKind {
    File,
    Symlink,
}

impl EntryKind {
    pub(super) fn digest(self, mode: u32, data: &[u8]) -> String {
        let mut context = digest::Context::new(&digest::SHA256);
        context.update(match self {
            Self::File => b"file\0",
            Self::Symlink => b"symlink\0",
        });
        context.update(&mode.to_be_bytes());
        context.update(data);
        Hex::encode(context.finish().as_ref())
    }
}

pub(super) struct VaultCrypto<'a> {
    key: &'a [u8],
}

impl<'a> VaultCrypto<'a> {
    const INDEX_AAD: &'static [u8] = b"vmux-vault-index-v1";
    const OBJECT_AAD_PREFIX: &'static [u8] = b"vmux-vault-object-v1\0";

    pub(super) fn new(key: &'a [u8]) -> Result<Self, String> {
        if key.len() != KEY_LEN {
            return Err("Vault encryption key has an invalid length".to_string());
        }
        Ok(Self { key })
    }

    pub(super) fn encrypt(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let mut nonce = [0_u8; aead::NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| "failed to generate Vault nonce".to_string())?;
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, self.key)
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

    pub(super) fn decrypt(&self, aad: &[u8], encrypted: &[u8]) -> Result<Vec<u8>, String> {
        if encrypted.len() < aead::NONCE_LEN + aead::AES_256_GCM.tag_len() {
            return Err("encrypted Vault data is truncated".to_string());
        }
        let nonce = <[u8; aead::NONCE_LEN]>::try_from(&encrypted[..aead::NONCE_LEN])
            .map_err(|_| "invalid Vault nonce".to_string())?;
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, self.key)
            .map_err(|_| "invalid Vault encryption key".to_string())?;
        let key = aead::LessSafeKey::new(unbound);
        let mut data = encrypted[aead::NONCE_LEN..].to_vec();
        let plaintext = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut data,
            )
            .map_err(|_| "Vault data could not be decrypted or was modified".to_string())?;
        Ok(plaintext.to_vec())
    }

    fn object_id(&self, path: &str) -> String {
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.key);
        Hex::encode(hmac::sign(&key, path.as_bytes()).as_ref())
    }

    fn object_aad(path: &str) -> Vec<u8> {
        let mut aad = Self::OBJECT_AAD_PREFIX.to_vec();
        aad.extend_from_slice(path.as_bytes());
        aad
    }
}

pub(crate) struct Hex;

impl Hex {
    pub(crate) fn random(bytes: usize) -> Result<String, String> {
        let mut value = vec![0_u8; bytes];
        SystemRandom::new()
            .fill(&mut value)
            .map_err(|_| "failed to generate secure random data".to_string())?;
        Ok(Self::encode(&value))
    }

    pub(super) fn encode(bytes: &[u8]) -> String {
        let digits = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push(digits[(byte >> 4) as usize] as char);
            output.push(digits[(byte & 0x0f) as usize] as char);
        }
        output
    }

    pub(super) fn decode(source: &str) -> Result<Vec<u8>, String> {
        if !source.len().is_multiple_of(2) {
            return Err("invalid hexadecimal data".to_string());
        }
        source
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = Self::decode_digit(pair[0])?;
                let low = Self::decode_digit(pair[1])?;
                Ok((high << 4) | low)
            })
            .collect()
    }

    fn decode_digit(byte: u8) -> Result<u8, String> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err("invalid hexadecimal data".to_string()),
        }
    }
}

pub(crate) struct SnapshotPath<'a>(pub(crate) &'a str);

impl SnapshotPath<'_> {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let path = Path::new(self.0);
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
}

pub(crate) struct FileTimestamp<'a>(pub(crate) &'a std::fs::Metadata);

impl FileTimestamp<'_> {
    pub(crate) fn get(&self) -> (u64, u32) {
        self.0
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LocalEntry {
    pub(crate) kind: EntryKind,
    pub(crate) mode: u32,
    pub(crate) size: u64,
    pub(crate) modified_secs: u64,
    pub(crate) modified_nanos: u32,
    pub(crate) data: Vec<u8>,
    pub(crate) digest: String,
}

impl LocalEntry {
    pub(crate) fn with_data(&self, data: Vec<u8>) -> Self {
        let mut entry = self.clone();
        entry.size = data.len() as u64;
        entry.modified_secs = 0;
        entry.modified_nanos = 0;
        entry.digest = entry.kind.digest(entry.mode, &data);
        entry.data = data;
        entry
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalFingerprint {
    pub(crate) kind: EntryKind,
    pub(crate) mode: u32,
    pub(crate) size: u64,
    pub(crate) modified_secs: u64,
    pub(crate) modified_nanos: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RemoteManifest {
    pub(crate) version: u32,
    pub(crate) cipher: String,
    pub(crate) vault_id: String,
    pub(crate) index: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct EncryptedIndex {
    version: u32,
    files: Vec<EncryptedIndexEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct EncryptedIndexEntry {
    path: String,
    object: String,
    digest: String,
    kind: EntryKind,
    mode: u32,
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

impl VaultRepositoryPath {
    pub(crate) fn write_encrypted_snapshot(
        &self,
        vault_id: &str,
        key: &[u8],
        files: &BTreeMap<String, LocalEntry>,
        previous: Option<&BTreeMap<String, LocalEntry>>,
    ) -> Result<(), String> {
        let repository = self.path();
        let crypto = VaultCrypto::new(key)?;
        if previous.is_some_and(|previous| Self::same_files(previous, files))
            && repository.join(MANIFEST_FILE).is_file()
            && repository.join(INDEX_FILE).is_file()
            && self
                .manifest()
                .is_ok_and(|manifest| manifest.version == MANIFEST_VERSION)
        {
            return self.validate_encrypted_worktree();
        }
        let objects = repository.join(OBJECTS_DIR);
        std::fs::create_dir_all(&objects).map_err(|error| error.to_string())?;
        let mut index_files = Vec::with_capacity(files.len());
        let mut retained = BTreeSet::new();
        for (path, entry) in files {
            SnapshotPath(path).validate()?;
            let object = crypto.object_id(path);
            retained.insert(object.clone());
            let object_path = objects.join(&object);
            let unchanged = previous
                .and_then(|files| files.get(path))
                .is_some_and(|old| LocalEntry::same(Some(old), Some(entry)))
                && object_path.is_file();
            if !unchanged {
                let encrypted = crypto.encrypt(&VaultCrypto::object_aad(path), &entry.data)?;
                vmux_path::AtomicFile::write(&object_path, &encrypted)
                    .map_err(|error| error.to_string())?;
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
                LocalPath(&entry.path()).remove()?;
            }
        }
        let index = EncryptedIndex {
            version: FORMAT_VERSION,
            files: index_files,
        };
        let index_source = ron::ser::to_string(&index)
            .map_err(|error| error.to_string())?
            .into_bytes();
        let encrypted_index = crypto.encrypt(VaultCrypto::INDEX_AAD, &index_source)?;
        vmux_path::AtomicFile::write(repository.join(INDEX_FILE), &encrypted_index)
            .map_err(|error| error.to_string())?;
        let manifest = RemoteManifest {
            version: MANIFEST_VERSION,
            cipher: "AES-256-GCM".to_string(),
            vault_id: vault_id.to_string(),
            index: INDEX_FILE.to_string(),
        };
        self.write_manifest(&manifest)?;
        self.validate_encrypted_worktree()
    }

    pub(crate) fn load_encrypted_snapshot(
        &self,
        key: &[u8],
    ) -> Result<(RemoteManifest, BTreeMap<String, LocalEntry>), String> {
        let repository = self.path();
        let crypto = VaultCrypto::new(key)?;
        let manifest = self.manifest()?;
        let encrypted_index = std::fs::read(repository.join(&manifest.index))
            .map_err(|error| format!("failed to read encrypted Vault index: {error}"))?;
        let index_source = crypto.decrypt(VaultCrypto::INDEX_AAD, &encrypted_index)?;
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
            SnapshotPath(&file.path).validate()?;
            let expected_object = crypto.object_id(&file.path);
            if file.object != expected_object {
                return Err(format!("encrypted Vault object mismatch for {}", file.path));
            }
            let encrypted = std::fs::read(repository.join(OBJECTS_DIR).join(&file.object))
                .map_err(|error| format!("missing encrypted Vault object: {error}"))?;
            let data = crypto.decrypt(&VaultCrypto::object_aad(&file.path), &encrypted)?;
            let actual_digest = file.kind.digest(file.mode, &data);
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

    fn same_files(
        left: &BTreeMap<String, LocalEntry>,
        right: &BTreeMap<String, LocalEntry>,
    ) -> bool {
        left.len() == right.len()
            && left
                .iter()
                .all(|(path, entry)| LocalEntry::same(Some(entry), right.get(path)))
    }
}
