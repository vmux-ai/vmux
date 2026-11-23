use super::connect::*;
use super::keys::KeyStore;
use super::recovery::*;
use super::repository::*;
use super::snapshot::*;
use super::status::*;
use super::sync::*;
use super::*;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use zeroize::Zeroizing;

#[test]
fn agent_status_reports_provider_and_pending_sync() {
    let status = VaultStatus {
        root: "/Users/test/.vmux".into(),
        initialized: true,
        encrypted: true,
        unlocked: true,
        recovery_enabled: true,
        remote: "https://github.com/vmux-ai/vault.git".into(),
        branch: "main".into(),
        dirty: 2,
        ahead: 1,
        behind: 0,
        ..Default::default()
    };
    let status = status.snapshot();

    assert!(status.connected);
    assert!(status.encrypted);
    assert!(status.unlocked);
    assert!(status.recovery_key);
    assert!(status.automatic_backup);
    assert_eq!(
        status.provider,
        Some(vmux_api::vault::VaultProvider::Github)
    );
    assert_eq!(
        status.remote.as_deref(),
        Some("https://github.com/vmux-ai/vault.git")
    );
    assert_eq!(status.local_changes, 2);
    assert_eq!(status.ahead, 1);
    assert!(status.sync_needed);
}

#[test]
fn agent_status_matches_github_hosts_exactly() {
    for (remote, provider) in [
        (
            "git@github.com:vmux-ai/vault.git",
            vmux_api::vault::VaultProvider::Github,
        ),
        (
            "https://notgithub.com/vmux-ai/vault.git",
            vmux_api::vault::VaultProvider::Git,
        ),
        (
            "/Volumes/github.com/vault.git",
            vmux_api::vault::VaultProvider::CloudFolder,
        ),
    ] {
        let status = VaultStatus {
            initialized: true,
            remote: remote.to_string(),
            ..VaultStatus::default()
        };
        assert_eq!(status.snapshot().provider, Some(provider));
    }
}

#[test]
fn agent_status_removes_remote_url_credentials() {
    let status = VaultStatus {
        initialized: true,
        remote: "https://user:secret@github.com/vmux-ai/vault.git".to_string(),
        ..VaultStatus::default()
    };
    let status = status.snapshot();

    assert_eq!(
        status.provider,
        Some(vmux_api::vault::VaultProvider::Github)
    );
    assert_eq!(
        status.remote.as_deref(),
        Some("https://github.com/vmux-ai/vault.git")
    );
    assert!(!serde_json::to_string(&status).unwrap().contains("secret"));
}

#[test]
fn agent_status_omits_an_unparseable_remote_url() {
    let status = VaultStatus {
        initialized: true,
        remote: "https://user:secret@[".to_string(),
        ..VaultStatus::default()
    };
    let status = status.snapshot();

    assert!(!status.connected);
    assert!(status.remote.is_none());
    assert!(!serde_json::to_string(&status).unwrap().contains("secret"));
}

#[test]
fn generated_recovery_keys_are_independent_and_parseable() {
    let key = GeneratedRecoveryKey::generate().unwrap();
    let displayed = key.display();
    assert!(
        GeneratedRecoveryKey::parse(&displayed).is_ok(),
        "the displayed form has to be the one the unlock path parses back"
    );

    let again = GeneratedRecoveryKey::generate().unwrap();
    assert_ne!(
        *displayed,
        *again.display(),
        "each draw must be independent"
    );
}

struct FixedKeyStore {
    key: Vec<u8>,
}

impl FixedKeyStore {
    fn new(byte: u8) -> Self {
        Self {
            key: vec![byte; KEY_LEN],
        }
    }

    fn status(&self, root: &Path, repository: &Path) -> VaultStatus {
        VaultStatus::at(root, repository, self)
    }

    fn repository(root: &Path) -> PathBuf {
        root.join(".vmux-vault")
    }

    fn configure_identity(root: &Path) {
        let git = GitRepository::at(root);
        git.run(&["config", "user.name", "Vmux Test"]).unwrap();
        git.run(&["config", "user.email", "vmux@example.com"])
            .unwrap();
        git.run(&["config", "commit.gpgSign", "false"]).unwrap();
    }

    fn prepare_repository(root: &Path) -> PathBuf {
        let repository = Self::repository(root);
        VaultRepositoryPath::at(&repository).ensure().unwrap();
        Self::configure_identity(&repository);
        repository
    }
}

impl KeyStore for FixedKeyStore {
    fn load(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        Ok(Zeroizing::new(self.key.clone()))
    }
}

#[derive(Default)]
struct MemoryKeyStore {
    key: Mutex<Option<Vec<u8>>>,
}

impl MemoryKeyStore {
    fn status(&self, root: &Path, repository: &Path) -> VaultStatus {
        VaultStatus::at(root, repository, self)
    }
}

impl KeyStore for MemoryKeyStore {
    fn load(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        self.key
            .lock()
            .unwrap()
            .clone()
            .map(Zeroizing::new)
            .ok_or_else(|| "key unavailable".to_string())
    }
}

#[test]
fn missing_vault_key_distinguishes_registered_recovery_methods() {
    let repository = tempfile::tempdir().unwrap();
    let keys = MemoryKeyStore::default();

    assert_eq!(
        RepositoryKey::new(repository.path(), &keys)
            .load("vault")
            .unwrap_err(),
        "This Vault is locked on this device. No recovery method is registered. Open it on a device that can already unlock it, then add a Recovery Key."
    );

    let envelope = RecoveryEnvelope {
        version: FORMAT_VERSION,
        wrapped_key: vec![1, 2, 3],
    };
    let directory = repository.path().join(RECOVERY_DIR);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(RECOVERY_FILE),
        ron::to_string(&envelope).unwrap(),
    )
    .unwrap();

    assert_eq!(
        RepositoryKey::new(repository.path(), &keys)
            .load("vault")
            .unwrap_err(),
        "key unavailable",
        "with a recovery method on record the real error surfaces, not the no-method advice"
    );
}

#[test]
fn vault_status_requires_an_accessible_encryption_key() {
    let root = tempfile::tempdir().unwrap();
    let repository = FixedKeyStore::prepare_repository(root.path());
    let keys = FixedKeyStore::new(47);
    let files = VaultSync::collect(root.path()).unwrap();
    VaultRepositoryPath::at(&repository)
        .write_encrypted_snapshot("vault-47", &keys.key, &files, None)
        .unwrap();
    GitRepository::at(&repository)
        .commit("Initialize vmux Vault")
        .unwrap();
    VaultLocalState::write(root.path(), &repository).unwrap();

    assert!(keys.status(root.path(), &repository).unlocked);
    assert!(
        !MemoryKeyStore::default()
            .status(root.path(), &repository)
            .unlocked
    );
}

#[test]
fn invalid_saved_github_account_uses_reauthentication_flow() {
    let source = r#"{
            "hosts": {
                "github.com": [{
                    "state": "error",
                    "active": true,
                    "host": "github.com",
                    "login": "octocat"
                }]
            }
        }"#;

    assert!(GitHubAuthorization::saved_account(source).unwrap());
    assert!(!GitHubAuthorization::saved_account(r#"{"hosts":{}}"#).unwrap());
}

#[test]
fn github_owner_picker_only_includes_organizations_that_can_create_repositories() {
    let source = r#"{
            "data": {
                "viewer": {
                    "login": "octocat",
                    "organizations": {
                        "nodes": [
                            {"login": "stale-org", "viewerCanCreateRepositories": false},
                            {"login": "writable-org", "viewerCanCreateRepositories": true}
                        ]
                    }
                }
            }
        }"#;

    assert_eq!(
        GitHubAuthorization::owners_from_graphql(source).unwrap(),
        (
            "octocat".to_string(),
            vec!["octocat".to_string(), "writable-org".to_string()]
        )
    );
}

#[test]
fn github_device_code_is_extracted_from_cli_progress() {
    assert_eq!(
        GitHubAuthorization::device_code("! First, copy your one-time code: ABCD-1234"),
        Some("ABCD-1234".to_string())
    );
    assert_eq!(
        GitHubAuthorization::device_code("One-time code (WXYZ-9876) copied to clipboard"),
        Some("WXYZ-9876".to_string())
    );
    assert_eq!(
        GitHubAuthorization::device_code("authentication pending"),
        None
    );
}

#[test]
fn github_auth_streams_the_device_code() {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'One-time code (ABCD-1234) copied to clipboard\\n' >&2",
    ]);
    let mut codes = Vec::new();

    GitHubAuthorization::run(&mut command, &mut |code| codes.push(code), &|| false).unwrap();

    assert_eq!(codes, vec!["ABCD-1234"]);
}

#[test]
fn github_auth_can_be_canceled() {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'One-time code (ABCD-1234) copied to clipboard\\n' >&2; exec sleep 10",
    ]);
    let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel = canceled.clone();
    let canceler = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    let result = GitHubAuthorization::run(&mut command, &mut |_| {}, &|| {
        canceled.load(std::sync::atomic::Ordering::Relaxed)
    });
    canceler.join().unwrap();

    assert_eq!(result, Err("GitHub authorization canceled".to_string()));
}

#[test]
fn encrypted_data_rejects_wrong_keys_and_tampering() {
    let key = vec![7; KEY_LEN];
    let wrong_key = vec![8; KEY_LEN];
    let encrypted = VaultCrypto::new(&key)
        .unwrap()
        .encrypt(b"path", b"secret")
        .unwrap();

    assert_eq!(
        VaultCrypto::new(&key)
            .unwrap()
            .decrypt(b"path", &encrypted)
            .unwrap(),
        b"secret"
    );
    assert!(
        VaultCrypto::new(&wrong_key)
            .unwrap()
            .decrypt(b"path", &encrypted)
            .is_err()
    );
    let mut tampered = encrypted;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(
        VaultCrypto::new(&key)
            .unwrap()
            .decrypt(b"path", &tampered)
            .is_err()
    );
}

#[test]
fn recovery_key_format_round_trips_and_rejects_invalid_input() {
    let key = [0xab; KEY_LEN];
    let encoded = GeneratedRecoveryKey::format(&key);

    assert!(encoded.starts_with("vmux-abab-"));
    assert_eq!(
        GeneratedRecoveryKey::parse(&encoded).unwrap().as_slice(),
        &key
    );
    assert!(GeneratedRecoveryKey::parse("vmux-not-a-key").is_err());
}

#[test]
fn recovery_key_creation_survives_remote_upload_failure() {
    let root = tempfile::tempdir().unwrap();
    let repository = FixedKeyStore::prepare_repository(root.path());
    let keys = FixedKeyStore::new(44);
    let files = VaultSync::collect(root.path()).unwrap();
    VaultRepositoryPath::at(&repository)
        .write_encrypted_snapshot("vault-44", &keys.key, &files, None)
        .unwrap();
    GitRepository::at(&repository)
        .commit("Initialize vmux Vault")
        .unwrap();
    let unavailable = root.path().join("missing-remote.git");
    GitRepository::at(&repository)
        .run(&["remote", "add", "origin", unavailable.to_str().unwrap()])
        .unwrap();

    let recovery_key_bytes = [46; KEY_LEN];
    let recovery_key = GeneratedRecoveryKey::format(&recovery_key_bytes);
    let recovery = VaultRecovery::at(root.path(), &repository)
        .create_with(&keys, &recovery_key_bytes)
        .unwrap();

    assert!(recovery.pending_upload);
    assert_eq!(
        GeneratedRecoveryKey::parse(&recovery_key).unwrap().len(),
        KEY_LEN
    );
    assert!(RecoveryEnvelope::read(&repository).unwrap().is_some());
}

#[test]
fn managed_path_filter_includes_authored_vault_content_only() {
    let storage = VaultStorage::current();
    let root = storage.root();

    assert!(storage.manages(&root.join("settings.ron")));
    assert!(storage.manages(&root.join("knowledge/note.md")));
    assert!(storage.manages(&root.join("tools/Brewfile")));
    assert!(!storage.manages(&root.join("workspace/repo/file.rs")));
    assert!(!storage.manages(&root.join("profiles/personal/store.ron")));
    assert!(!storage.manages(&root.join("knowledge/.DS_Store")));
}
