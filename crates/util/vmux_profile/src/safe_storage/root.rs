use std::sync::Mutex;

use ring::rand::SecureRandom;
use zeroize::Zeroizing;

use super::{ROOT_KEY_LENGTH, RootKey, SafeStorageContext, SafeStorageError};

const ROOT_HEADER: &[u8] = b"vmux-safe-storage-root-v1\0";
const STATE_VERSION: &[u8] = b"1\n";

static ROOT_IO: Mutex<()> = Mutex::new(());

pub(super) struct RootKeyStore;

struct SafeStorageState<'a> {
    context: &'a SafeStorageContext,
}

#[cfg(test)]
pub(super) trait RootFixture: Send + Sync {
    fn read(&self, silent: bool) -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError>;
    fn write(&self, payload: &[u8]) -> Result<(), SafeStorageError>;
}

impl SafeStorageContext {
    pub(super) fn root_key(&self, create: bool) -> Result<RootKey, SafeStorageError> {
        self.require_desktop_process()?;
        #[cfg(any(test, debug_assertions))]
        if self.access.test_session {
            return Ok(RootKey::new([0x56; ROOT_KEY_LENGTH]));
        }

        let mut cached = self
            .access
            .root_key
            .lock()
            .map_err(|_| SafeStorageError::LockPoisoned)?;
        if let Some(key) = cached.as_ref() {
            return Ok(key.duplicate());
        }
        let _io = ROOT_IO.lock().map_err(|_| SafeStorageError::LockPoisoned)?;
        if let Some(payload) = self.read_root(false)? {
            let key = RootKeyStore::decode(&payload)?;
            SafeStorageState::new(self).write()?;
            *cached = Some(key.duplicate());
            return Ok(key);
        }
        if !create || SafeStorageState::new(self).exists() {
            return Err(SafeStorageError::MissingRoot);
        }
        let key = RootKeyStore::random()?;
        self.write_root(&RootKeyStore::encode(&key))?;
        SafeStorageState::new(self).write()?;
        *cached = Some(key.duplicate());
        Ok(key)
    }

    pub(super) fn root_key_silent(&self) -> Result<Option<RootKey>, SafeStorageError> {
        self.require_desktop_process()?;
        #[cfg(any(test, debug_assertions))]
        if self.access.test_session {
            return Ok(Some(RootKey::new([0x56; ROOT_KEY_LENGTH])));
        }

        let mut cached = self
            .access
            .root_key
            .lock()
            .map_err(|_| SafeStorageError::LockPoisoned)?;
        if let Some(key) = cached.as_ref() {
            return Ok(Some(key.duplicate()));
        }
        let _io = ROOT_IO.lock().map_err(|_| SafeStorageError::LockPoisoned)?;
        let Some(payload) = self.read_root(true)? else {
            if SafeStorageState::new(self).exists() {
                return Err(SafeStorageError::MissingRoot);
            }
            return Ok(None);
        };
        let key = RootKeyStore::decode(&payload)?;
        *cached = Some(key.duplicate());
        Ok(Some(key))
    }

    fn read_root(&self, silent: bool) -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError> {
        #[cfg(test)]
        if let Some(fixture) = &self.access.root_fixture {
            return fixture.read(silent);
        }
        #[cfg(target_os = "macos")]
        {
            RootKeyStore::read(self, silent)
        }
        #[cfg(not(target_os = "macos"))]
        Err(SafeStorageError::DesktopProcessRequired)
    }

    fn write_root(&self, payload: &[u8]) -> Result<(), SafeStorageError> {
        #[cfg(test)]
        if let Some(fixture) = &self.access.root_fixture {
            return fixture.write(payload);
        }
        #[cfg(target_os = "macos")]
        {
            RootKeyStore::write(self, payload)
        }
        #[cfg(not(target_os = "macos"))]
        Err(SafeStorageError::DesktopProcessRequired)
    }

    #[cfg(test)]
    fn fixture(directory: std::path::PathBuf, source: std::sync::Arc<dyn RootFixture>) -> Self {
        Self {
            access: std::sync::Arc::new(super::StorageAccess {
                directory,
                test_session: false,
                desktop_process_required: false,
                root_key: Mutex::new(None),
                root_fixture: Some(source),
            }),
        }
    }
}

impl RootKeyStore {
    fn random() -> Result<RootKey, SafeStorageError> {
        let mut bytes = Zeroizing::new([0_u8; ROOT_KEY_LENGTH]);
        ring::rand::SystemRandom::new()
            .fill(bytes.as_mut())
            .map_err(|_| SafeStorageError::Crypto("failed to generate Vmux Safe Storage key"))?;
        Ok(RootKey::new(*bytes))
    }

    fn encode(key: &RootKey) -> Zeroizing<Vec<u8>> {
        let mut payload = Zeroizing::new(Vec::with_capacity(ROOT_HEADER.len() + ROOT_KEY_LENGTH));
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
            self.context.directory().join("account"),
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
    use crate::safe_storage::cipher::SafeStorageCipher;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, mpsc};
    use std::thread;
    use std::time::Duration;

    struct Fixture {
        directory: tempfile::TempDir,
        context: SafeStorageContext,
        source: Arc<NativeRoot>,
    }

    struct NativeRoot {
        payload: Mutex<Option<Zeroizing<Vec<u8>>>>,
        reads: AtomicUsize,
        authorizations: AtomicUsize,
        writes: AtomicUsize,
        denied: AtomicBool,
        blocked: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    }

    impl Fixture {
        fn new(payload: Option<Vec<u8>>) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let source = Arc::new(NativeRoot::new(payload));
            let context =
                SafeStorageContext::fixture(directory.path().join("safe-storage"), source.clone());
            Self {
                directory,
                context,
                source,
            }
        }
    }

    impl NativeRoot {
        fn new(payload: Option<Vec<u8>>) -> Self {
            Self {
                payload: Mutex::new(payload.map(Zeroizing::new)),
                reads: AtomicUsize::new(0),
                authorizations: AtomicUsize::new(0),
                writes: AtomicUsize::new(0),
                denied: AtomicBool::new(false),
                blocked: Mutex::new(None),
            }
        }

        fn payload(byte: u8) -> Vec<u8> {
            let mut payload = b"vmux-safe-storage-root-v1\0".to_vec();
            payload.extend_from_slice(&[byte; 32]);
            payload
        }

        fn block(&self) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
            let (entered, observed) = mpsc::channel();
            let (release, resumed) = mpsc::channel();
            *self.blocked.lock().unwrap() = Some((entered, resumed));
            (observed, release)
        }
    }

    impl RootFixture for NativeRoot {
        fn read(&self, silent: bool) -> Result<Option<Zeroizing<Vec<u8>>>, SafeStorageError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if !silent {
                self.authorizations.fetch_add(1, Ordering::SeqCst);
            }
            if let Some((entered, release)) = self.blocked.lock().unwrap().take() {
                entered.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            if self.denied.swap(false, Ordering::SeqCst) {
                return Err(SafeStorageError::KeychainDenied("fixture denied".into()));
            }
            let payload = self.payload.lock().unwrap();
            if payload.is_none() {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(payload
                .as_ref()
                .map(|payload| Zeroizing::new(payload.to_vec())))
        }

        fn write(&self, payload: &[u8]) -> Result<(), SafeStorageError> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            *self.payload.lock().unwrap() = Some(Zeroizing::new(payload.to_vec()));
            Ok(())
        }
    }

    #[test]
    fn native_root_payload_preserves_the_existing_versioned_format() {
        let payload = NativeRoot::payload(7);
        let fixture = Fixture::new(Some(payload.clone()));
        let key = fixture.context.root_key(false).unwrap();
        assert_eq!(RootKeyStore::encode(&key).as_slice(), payload);
        assert_eq!(key.as_bytes(), &[7; 32]);
        assert_eq!(
            std::fs::read(fixture.context.directory().join("version")).unwrap(),
            b"1\n"
        );
        assert!(RootKeyStore::decode(key.as_bytes()).is_err());
        assert!(RootKeyStore::decode(b"vmux-safe-storage-root-v1\0").is_err());
    }

    #[test]
    fn concurrent_context_clones_authorize_once_and_share_envelope_access() {
        let fixture = Fixture::new(Some(NativeRoot::payload(11)));
        let barrier = Arc::new(Barrier::new(8));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let context = fixture.context.clone();
            let barrier = barrier.clone();
            tasks.push(thread::spawn(move || {
                barrier.wait();
                let cipher = SafeStorageCipher::new(context.root_key(false).unwrap());
                cipher.wrap_vault_key("vault", b"secret").unwrap()
            }));
        }
        let cipher = SafeStorageCipher::new(fixture.context.root_key(false).unwrap());
        for task in tasks {
            let envelope = task.join().unwrap();
            assert_eq!(
                cipher
                    .unwrap_vault_key("vault", &envelope)
                    .unwrap()
                    .as_slice(),
                b"secret"
            );
        }
        assert_eq!(fixture.source.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn independent_contexts_do_not_share_unlocked_root_authority() {
        let first = Fixture::new(Some(NativeRoot::payload(17)));
        let second = Fixture::new(Some(NativeRoot::payload(29)));
        let envelope = SafeStorageCipher::new(first.context.root_key(false).unwrap())
            .wrap_vault_key("vault", b"secret")
            .unwrap();
        assert!(
            SafeStorageCipher::new(second.context.root_key(false).unwrap())
                .unwrap_vault_key("vault", &envelope)
                .is_err()
        );
        assert_eq!(first.source.reads.load(Ordering::SeqCst), 1);
        assert_eq!(second.source.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn independent_root_creation_preserves_one_fixed_native_identity() {
        let fixture = Fixture::new(None);
        let second = SafeStorageContext::fixture(
            fixture.directory.path().join("second"),
            fixture.source.clone(),
        );
        let first = fixture.context.clone();
        let barrier = Arc::new(Barrier::new(2));
        let ready = barrier.clone();
        let task = thread::spawn(move || {
            ready.wait();
            SafeStorageCipher::new(first.root_key(true).unwrap())
                .wrap_vault_key("vault", b"secret")
                .unwrap()
        });
        barrier.wait();
        let cipher = SafeStorageCipher::new(second.root_key(true).unwrap());
        assert_eq!(
            cipher
                .unwrap_vault_key("vault", &task.join().unwrap())
                .unwrap()
                .as_slice(),
            b"secret"
        );
        assert_eq!(fixture.source.writes.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.source.reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn silent_missing_root_never_creates_or_caches_authority() {
        let fixture = Fixture::new(None);
        assert!(fixture.context.root_key_silent().unwrap().is_none());
        assert_eq!(fixture.source.writes.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.source.authorizations.load(Ordering::SeqCst), 0);
        *fixture.source.payload.lock().unwrap() = Some(Zeroizing::new(NativeRoot::payload(31)));
        let key = fixture.context.root_key_silent().unwrap().unwrap();
        assert_eq!(key.as_bytes(), &[31; 32]);
        assert_eq!(fixture.source.reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn lost_root_refuses_replacement_for_every_encrypted_storage_domain() {
        for relative in [
            "version",
            "mcp/token.bin",
            "vault/key.bin",
            "account/device.bin",
        ] {
            let fixture = Fixture::new(None);
            let path = fixture.context.directory().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"existing encrypted state").unwrap();
            assert!(matches!(
                fixture.context.root_key(true),
                Err(SafeStorageError::MissingRoot)
            ));
            assert!(matches!(
                fixture.context.root_key_silent(),
                Err(SafeStorageError::MissingRoot)
            ));
            assert_eq!(fixture.source.writes.load(Ordering::SeqCst), 0);
            assert_eq!(std::fs::read(path).unwrap(), b"existing encrypted state");
        }
    }

    #[test]
    fn denied_or_corrupt_root_reads_do_not_poison_the_context_cache() {
        for denied in [true, false] {
            let fixture = Fixture::new(Some(b"corrupt root".to_vec()));
            fixture.source.denied.store(denied, Ordering::SeqCst);
            assert!(fixture.context.root_key(false).is_err());
            *fixture.source.payload.lock().unwrap() = Some(Zeroizing::new(NativeRoot::payload(41)));
            assert_eq!(
                fixture.context.root_key(false).unwrap().as_bytes(),
                &[41; 32]
            );
            assert_eq!(fixture.source.reads.load(Ordering::SeqCst), 2);
        }
    }

    #[test]
    fn failed_marker_publication_is_not_acknowledged_as_an_unlocked_cache() {
        let fixture = Fixture::new(Some(NativeRoot::payload(43)));
        let marker = fixture.context.directory().join("version");
        std::fs::create_dir_all(&marker).unwrap();
        assert!(fixture.context.root_key(false).is_err());
        std::fs::remove_dir(marker).unwrap();
        assert_eq!(
            fixture.context.root_key(false).unwrap().as_bytes(),
            &[43; 32]
        );
        assert_eq!(fixture.source.reads.load(Ordering::SeqCst), 2);
        assert_eq!(fixture.source.writes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_unlocked_context_cannot_bypass_another_contexts_desktop_access() {
        let fixture = Fixture::new(Some(NativeRoot::payload(47)));
        fixture.context.root_key(false).unwrap();
        let restricted =
            SafeStorageContext::for_environment(fixture.directory.path().to_path_buf(), false);
        assert!(matches!(
            restricted.root_key(false),
            Err(SafeStorageError::DesktopProcessRequired)
        ));
        assert!(matches!(
            restricted.root_key_silent(),
            Err(SafeStorageError::DesktopProcessRequired)
        ));
    }

    #[test]
    fn native_io_retains_the_context_until_drain_then_releases_its_root() {
        let fixture = Fixture::new(Some(NativeRoot::payload(53)));
        let lifetime = Arc::downgrade(&fixture.context.access);
        let context = fixture.context.clone();
        let (entered, release) = fixture.source.block();
        let task = thread::spawn(move || context.root_key(false).unwrap());
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(fixture.context);
        assert!(lifetime.upgrade().is_some());
        release.send(()).unwrap();
        assert_eq!(task.join().unwrap().as_bytes(), &[53; 32]);
        assert!(lifetime.upgrade().is_none());
    }

    #[test]
    fn test_context_protected_io_preserves_the_production_namespace() {
        let directory = tempfile::tempdir().unwrap();
        let production = directory.path().join("safe-storage/vault/key.bin");
        std::fs::create_dir_all(production.parent().unwrap()).unwrap();
        std::fs::write(&production, b"production envelope").unwrap();
        let context = SafeStorageContext::for_environment(directory.path().to_path_buf(), true);
        let cipher = SafeStorageCipher::new(context.root_key(true).unwrap());
        let envelope = cipher.wrap_vault_key("test", b"test key").unwrap();
        context
            .protected_file("vault/key.bin")
            .write(&envelope)
            .unwrap();
        let stored =
            std::fs::read(directory.path().join("safe-storage-test/vault/key.bin")).unwrap();
        assert_eq!(
            cipher.unwrap_vault_key("test", &stored).unwrap().as_slice(),
            b"test key"
        );
        assert_eq!(std::fs::read(production).unwrap(), b"production envelope");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cloned_mcp_storage_reuses_authorization_without_sharing_accounts() {
        use crate::mcp_credentials::{McpCredentialStorage, McpOauthCredentials};
        let fixture = Fixture::new(Some(NativeRoot::payload(59)));
        let profile = crate::Profile::named("personal");
        let paths =
            crate::ProfilePaths::at(profile.clone(), fixture.directory.path().to_path_buf());
        let storage = McpCredentialStorage::with_context(profile, paths, fixture.context.clone());
        let credentials = McpOauthCredentials {
            access_token: "fixture-secret".into(),
            ..McpOauthCredentials::default()
        };
        storage.store("first", &credentials).unwrap();
        let cloned = storage.clone();
        drop(storage);
        assert_eq!(
            cloned.load("first").unwrap().unwrap().access_token,
            "fixture-secret"
        );
        assert!(cloned.load("second").unwrap().is_none());
        assert!(!format!("{cloned:?}").contains("fixture-secret"));
        cloned.remove("first").unwrap();
        assert!(cloned.load("first").unwrap().is_none());
        assert_eq!(fixture.source.reads.load(Ordering::SeqCst), 1);
    }
}
