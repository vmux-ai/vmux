use std::sync::{Mutex, OnceLock};

use ring::rand::SecureRandom;

use super::{ROOT_KEY_LENGTH, RootKey, RootKeySource, SafeStorageContext, SafeStorageError};

const ROOT_HEADER: &[u8] = b"vmux-safe-storage-root-v1\0";
const STATE_VERSION: &[u8] = b"1\n";

static ROOT_KEY: OnceLock<Mutex<Option<RootKey>>> = OnceLock::new();

pub(super) struct RootKeyStore;

struct SafeStorageState<'a> {
    context: &'a SafeStorageContext,
}

impl SafeStorageContext {
    pub(super) fn root_key(&self, create: bool) -> Result<RootKey, SafeStorageError> {
        #[cfg(any(test, debug_assertions))]
        if matches!(self.root_key_source, RootKeySource::FixedTest) {
            return Ok(RootKey::new([0x56; ROOT_KEY_LENGTH]));
        }

        let mut cached = ROOT_KEY
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| SafeStorageError::LockPoisoned)?;
        if let Some(key) = cached.as_ref() {
            return Ok(key.duplicate());
        }
        if let Some(payload) = RootKeyStore::read(self, false)? {
            let key = RootKeyStore::decode(&payload)?;
            SafeStorageState::new(self).write()?;
            *cached = Some(key.duplicate());
            return Ok(key);
        }
        if !create || SafeStorageState::new(self).exists() {
            return Err(SafeStorageError::MissingRoot);
        }
        let key = RootKeyStore::random()?;
        RootKeyStore::write(self, &RootKeyStore::encode(&key))?;
        SafeStorageState::new(self).write()?;
        *cached = Some(key.duplicate());
        Ok(key)
    }

    pub(super) fn root_key_silent(&self) -> Result<Option<RootKey>, SafeStorageError> {
        #[cfg(any(test, debug_assertions))]
        if matches!(self.root_key_source, RootKeySource::FixedTest) {
            return Ok(Some(RootKey::new([0x56; ROOT_KEY_LENGTH])));
        }

        let mut cached = ROOT_KEY
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| SafeStorageError::LockPoisoned)?;
        if let Some(key) = cached.as_ref() {
            return Ok(Some(key.duplicate()));
        }
        let Some(payload) = RootKeyStore::read(self, true)? else {
            if SafeStorageState::new(self).exists() {
                return Err(SafeStorageError::MissingRoot);
            }
            return Ok(None);
        };
        let key = RootKeyStore::decode(&payload)?;
        *cached = Some(key.duplicate());
        Ok(Some(key))
    }
}

impl RootKeyStore {
    fn random() -> Result<RootKey, SafeStorageError> {
        let mut bytes = [0_u8; ROOT_KEY_LENGTH];
        ring::rand::SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| SafeStorageError::Crypto("failed to generate Vmux Safe Storage key"))?;
        Ok(RootKey::new(bytes))
    }

    fn encode(key: &RootKey) -> Vec<u8> {
        let mut payload = Vec::with_capacity(ROOT_HEADER.len() + ROOT_KEY_LENGTH);
        payload.extend_from_slice(ROOT_HEADER);
        payload.extend_from_slice(key.as_bytes());
        payload
    }

    fn decode(payload: &[u8]) -> Result<RootKey, SafeStorageError> {
        let Some(key) = payload.strip_prefix(ROOT_HEADER) else {
            return Err(SafeStorageError::UnsupportedVersion);
        };
        let key: [u8; ROOT_KEY_LENGTH] =
            key.try_into().map_err(|_| SafeStorageError::CorruptRoot)?;
        Ok(RootKey::new(key))
    }
}

impl<'a> SafeStorageState<'a> {
    fn new(context: &'a SafeStorageContext) -> Self {
        Self { context }
    }

    fn file(&self) -> super::ProtectedFile {
        self.context.protected_file("version")
    }

    fn exists(&self) -> bool {
        if self.file().path().is_file() {
            return true;
        }
        for directory in [
            self.context.directory().join("mcp"),
            self.context.directory().join("vault"),
        ] {
            if std::fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_some()) {
                return true;
            }
        }
        false
    }

    fn write(&self) -> Result<(), SafeStorageError> {
        if self.exists() {
            return Ok(());
        }
        self.file().write(STATE_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_key_payload_is_versioned_and_validated() {
        let key = RootKey::new([7_u8; ROOT_KEY_LENGTH]);
        let payload = RootKeyStore::encode(&key);

        assert_eq!(RootKeyStore::decode(&payload).unwrap().as_bytes(), &[7; 32]);
        assert!(RootKeyStore::decode(key.as_bytes()).is_err());
        assert!(RootKeyStore::decode(ROOT_HEADER).is_err());
    }
}
