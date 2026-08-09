use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};

#[test]
fn vault_key_broker_encoding_round_trips() {
    let key = (0..KEY_LEN).map(|value| value as u8).collect::<Vec<_>>();

    assert_eq!(decode_key_hex(&hex(&key)), Ok(key));
    assert!(decode_key_hex("00").is_err());
    assert!(decode_key_hex(&"z".repeat(KEY_LEN * 2)).is_err());
}

#[test]
fn concurrent_vault_key_loads_prompt_once_per_session() {
    let vault_id = "test-concurrent-vault-key-load";
    SESSION_KEYS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .remove(vault_id);
    let loads = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(8));
    let threads = (0..8)
        .map(|_| {
            let loads = loads.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                load_or_store_session_key(vault_id, || {
                    loads.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(10));
                    Ok(Zeroizing::new(vec![7; KEY_LEN]))
                })
                .unwrap()
            })
        })
        .collect::<Vec<_>>();

    for thread in threads {
        assert_eq!(*thread.join().unwrap(), vec![7; KEY_LEN]);
    }
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn vault_key_broker_reads_signing_leaf_certificate() {
    let hash = signing_leaf_hash(Path::new("/usr/bin/codesign")).unwrap();

    assert_eq!(hash.len(), 40);
    assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[cfg(target_os = "macos")]
#[test]
fn vault_key_broker_validates_running_callers() {
    use security_framework::os::macos::code_signing::{Flags, SecCode, SecRequirement};
    use std::str::FromStr;

    let caller = SecCode::for_self(Flags::NONE).unwrap();
    let requirement = SecRequirement::from_str("true").unwrap();

    validate_key_broker_caller(&caller, &requirement).unwrap();
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
}

impl KeyStore for FixedKeyStore {
    fn load(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        Ok(Zeroizing::new(self.key.clone()))
    }

    fn create(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        Ok(Zeroizing::new(self.key.clone()))
    }

    fn store(&self, _vault_id: &str, key: &[u8]) -> Result<(), String> {
        if key == self.key {
            Ok(())
        } else {
            Err("unexpected key".to_string())
        }
    }
}

#[derive(Default)]
struct MemoryKeyStore {
    key: Mutex<Option<Vec<u8>>>,
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

    fn create(&self, _vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        Err("unexpected create".to_string())
    }

    fn store(&self, _vault_id: &str, key: &[u8]) -> Result<(), String> {
        *self.key.lock().unwrap() = Some(key.to_vec());
        Ok(())
    }
}

#[test]
fn missing_vault_key_distinguishes_registered_recovery_methods() {
    let repository = tempfile::tempdir().unwrap();
    let keys = MemoryKeyStore::default();

    assert_eq!(
        load_repository_key(repository.path(), &keys, "vault").unwrap_err(),
        "This Vault is locked on this device. No recovery method is registered. Open it on a device that can already unlock it, then add a Recovery Key or passkey."
    );

    let credential_id = "00";
    let envelope = PasskeyEnvelope {
        version: FORMAT_VERSION,
        credential_id: credential_id.to_string(),
        wrapped_key: vec![1, 2, 3],
    };
    let directory = repository.path().join(PASSKEYS_DIR);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(passkey_envelope_name(credential_id)),
        ron::to_string(&envelope).unwrap(),
    )
    .unwrap();

    assert_eq!(
        load_repository_key(repository.path(), &keys, "vault").unwrap_err(),
        "key unavailable"
    );
}

#[test]
fn vault_status_requires_an_accessible_encryption_key() {
    let root = tempfile::tempdir().unwrap();
    let repository = prepare_repository(root.path());
    let keys = FixedKeyStore::new(47);
    initialize_paths(root.path(), &repository, &keys).unwrap();
    write_local_state(root.path(), &repository).unwrap();

    assert!(status_paths(root.path(), &repository, &keys).unlocked);
    assert!(!status_paths(root.path(), &repository, &MemoryKeyStore::default()).unlocked);
}

fn repository(root: &Path) -> PathBuf {
    root.join(".vmux-vault")
}

fn configure_identity(root: &Path) {
    git(root, &["config", "user.name", "Vmux Test"]).unwrap();
    git(root, &["config", "user.email", "vmux@example.com"]).unwrap();
    git(root, &["config", "commit.gpgSign", "false"]).unwrap();
}

fn prepare_repository(root: &Path) -> PathBuf {
    let repository = repository(root);
    ensure_repository(&repository).unwrap();
    configure_identity(&repository);
    repository
}

fn create_bare_remote(path: &Path) {
    command_success(
        Command::new("git")
            .args(["init", "--bare", path.to_string_lossy().as_ref()])
            .output()
            .unwrap(),
    )
    .unwrap();
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

    assert!(has_saved_github_account(source).unwrap());
    assert!(!has_saved_github_account(r#"{"hosts":{}}"#).unwrap());
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
        github_owners_from_graphql(source).unwrap(),
        (
            "octocat".to_string(),
            vec!["octocat".to_string(), "writable-org".to_string()]
        )
    );
}

#[test]
fn github_device_code_is_extracted_from_cli_progress() {
    assert_eq!(
        github_device_code("! First, copy your one-time code: ABCD-1234"),
        Some("ABCD-1234".to_string())
    );
    assert_eq!(
        github_device_code("One-time code (WXYZ-9876) copied to clipboard"),
        Some("WXYZ-9876".to_string())
    );
    assert_eq!(github_device_code("authentication pending"), None);
}

#[test]
fn github_auth_streams_the_device_code() {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "printf 'One-time code (ABCD-1234) copied to clipboard\\n' >&2",
    ]);
    let mut codes = Vec::new();

    run_github_auth(&mut command, &mut |code| codes.push(code), &|| false).unwrap();

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

    let result = run_github_auth(&mut command, &mut |_| {}, &|| {
        canceled.load(std::sync::atomic::Ordering::Relaxed)
    });
    canceler.join().unwrap();

    assert_eq!(result, Err("GitHub authorization canceled".to_string()));
}

#[test]
fn encrypted_data_rejects_wrong_keys_and_tampering() {
    let key = vec![7; KEY_LEN];
    let wrong_key = vec![8; KEY_LEN];
    let encrypted = encrypt_bytes(&key, b"path", b"secret").unwrap();

    assert_eq!(decrypt_bytes(&key, b"path", &encrypted).unwrap(), b"secret");
    assert!(decrypt_bytes(&wrong_key, b"path", &encrypted).is_err());
    let mut tampered = encrypted;
    *tampered.last_mut().unwrap() ^= 1;
    assert!(decrypt_bytes(&key, b"path", &tampered).is_err());
}

#[test]
fn passkey_unlocks_a_fetched_vault_without_the_original_device_key() {
    let first = tempfile::tempdir().unwrap();
    std::fs::write(first.path().join("settings.ron"), "(shared: true)\n").unwrap();
    let first_repository = prepare_repository(first.path());
    let original_keys = FixedKeyStore::new(42);
    initialize_paths(first.path(), &first_repository, &original_keys).unwrap();
    let credential_id = "a1".repeat(32);
    let prf_output = [9_u8; KEY_LEN];
    add_passkey_paths(
        &first_repository,
        &original_keys,
        &credential_id,
        &prf_output,
    )
    .unwrap();

    let remote = tempfile::tempdir().unwrap();
    let remote_path = remote.path().join("vault.git");
    create_bare_remote(&remote_path);
    git(
        &first_repository,
        &["remote", "add", "origin", remote_path.to_str().unwrap()],
    )
    .unwrap();
    git(&first_repository, &["push", "-u", "origin", "main"]).unwrap();

    let second = tempfile::tempdir().unwrap();
    let second_repository = prepare_repository(second.path());
    let second_keys = MemoryKeyStore::default();
    connect_remote_paths(
        second.path(),
        &second_repository,
        remote_path.to_str().unwrap(),
        &second_keys,
    )
    .unwrap();
    assert!(!second.path().join("settings.ron").exists());

    unlock_with_passkey_paths(
        second.path(),
        &second_repository,
        &second_keys,
        &credential_id,
        &prf_output,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(second.path().join("settings.ron")).unwrap(),
        "(shared: true)\n"
    );
    assert_eq!(
        second_keys.load("ignored").unwrap().as_slice(),
        &[42; KEY_LEN]
    );
    assert!(
        unlock_with_passkey_paths(
            second.path(),
            &second_repository,
            &MemoryKeyStore::default(),
            &credential_id,
            &[8_u8; KEY_LEN],
        )
        .is_err()
    );
}

#[test]
fn recovery_key_unlocks_knowledge_and_tools_on_a_new_device() {
    let first = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(first.path().join("knowledge")).unwrap();
    std::fs::create_dir_all(first.path().join("tools")).unwrap();
    std::fs::write(first.path().join("knowledge/private.md"), "# Private\n").unwrap();
    std::fs::write(
        first.path().join("tools/tools.toml"),
        "version = 1\n[homebrew]\npackages = [\"ripgrep\"]\n",
    )
    .unwrap();
    std::fs::write(first.path().join("tools/Brewfile"), "brew \"ripgrep\"\n").unwrap();
    let first_repository = prepare_repository(first.path());
    let original_keys = FixedKeyStore::new(43);
    initialize_paths(first.path(), &first_repository, &original_keys).unwrap();
    let recovery_key = format_recovery_key(&[45; KEY_LEN]);
    let recovery =
        create_recovery_key_paths(&first_repository, &original_keys, &recovery_key).unwrap();
    assert!(!recovery.pending_upload);
    assert_eq!(parse_recovery_key(&recovery_key).unwrap().len(), KEY_LEN);
    assert!(read_recovery_envelope(&first_repository).unwrap().is_some());

    let remote = tempfile::tempdir().unwrap();
    let remote_path = remote.path().join("vault.git");
    create_bare_remote(&remote_path);
    git(
        &first_repository,
        &["remote", "add", "origin", remote_path.to_str().unwrap()],
    )
    .unwrap();
    git(&first_repository, &["push", "-u", "origin", "main"]).unwrap();

    let second = tempfile::tempdir().unwrap();
    let second_repository = prepare_repository(second.path());
    let second_keys = MemoryKeyStore::default();
    connect_remote_paths(
        second.path(),
        &second_repository,
        remote_path.to_str().unwrap(),
        &second_keys,
    )
    .unwrap();
    assert!(!second.path().join("knowledge/private.md").exists());

    unlock_with_recovery_key_paths(
        second.path(),
        &second_repository,
        &second_keys,
        &recovery_key,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(second.path().join("knowledge/private.md")).unwrap(),
        "# Private\n"
    );
    assert_eq!(
        std::fs::read_to_string(second.path().join("tools/Brewfile")).unwrap(),
        "brew \"ripgrep\"\n"
    );
    assert_eq!(
        second_keys.load("ignored").unwrap().as_slice(),
        &[43; KEY_LEN]
    );
    assert!(
        unlock_with_recovery_key_paths(
            second.path(),
            &second_repository,
            &MemoryKeyStore::default(),
            "vmux-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000",
        )
        .is_err()
    );
}

#[test]
fn recovery_key_format_round_trips_and_rejects_invalid_input() {
    let key = [0xab; KEY_LEN];
    let encoded = format_recovery_key(&key);

    assert!(encoded.starts_with("vmux-abab-"));
    assert_eq!(parse_recovery_key(&encoded).unwrap().as_slice(), &key);
    assert!(parse_recovery_key("vmux-not-a-key").is_err());
}

#[test]
fn recovery_key_creation_survives_remote_upload_failure() {
    let root = tempfile::tempdir().unwrap();
    let repository = prepare_repository(root.path());
    let keys = FixedKeyStore::new(44);
    initialize_paths(root.path(), &repository, &keys).unwrap();
    let unavailable = root.path().join("missing-remote.git");
    git(
        &repository,
        &["remote", "add", "origin", unavailable.to_str().unwrap()],
    )
    .unwrap();

    let recovery_key = format_recovery_key(&[46; KEY_LEN]);
    let recovery = create_recovery_key_paths(&repository, &keys, &recovery_key).unwrap();

    assert!(recovery.pending_upload);
    assert_eq!(parse_recovery_key(&recovery_key).unwrap().len(), KEY_LEN);
    assert!(read_recovery_envelope(&repository).unwrap().is_some());
}

#[test]
fn initialization_commits_only_encrypted_paths_and_content() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("knowledge")).unwrap();
    std::fs::create_dir_all(root.path().join("workspace/project")).unwrap();
    std::fs::write(root.path().join("settings.ron"), "(secret: true)\n").unwrap();
    std::fs::write(root.path().join("knowledge/private.md"), "# Private\n").unwrap();
    std::fs::write(
        root.path().join("workspace/project/secret.txt"),
        "ignored\n",
    )
    .unwrap();
    let repository = prepare_repository(root.path());
    let keys = FixedKeyStore::new(3);

    initialize_paths(root.path(), &repository, &keys).unwrap();

    let tree = git(&repository, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap();
    assert!(tree.lines().any(|path| path == MANIFEST_FILE));
    assert!(tree.lines().any(|path| path == INDEX_FILE));
    assert!(tree.lines().any(|path| path.starts_with("objects/")));
    assert!(!tree.contains("settings.ron"));
    assert!(!tree.contains("private.md"));
    let repository_bytes = std::fs::read(repository.join(INDEX_FILE)).unwrap();
    assert!(
        !repository_bytes
            .windows(7)
            .any(|window| window == b"Private")
    );
    let manifest = read_manifest(&repository).unwrap();
    let key = keys.load(&manifest.vault_id).unwrap();
    let (_, files) = load_encrypted_snapshot(&repository, &key).unwrap();
    assert_eq!(files["settings.ron"].data, b"(secret: true)\n");
    assert_eq!(files["knowledge/private.md"].data, b"# Private\n");
    assert!(!files.contains_key("workspace/project/secret.txt"));
}

#[test]
fn managed_path_filter_includes_authored_vault_content_only() {
    let root = root_dir();

    assert!(is_managed_local_path(&root.join("settings.ron")));
    assert!(is_managed_local_path(&root.join("knowledge/note.md")));
    assert!(is_managed_local_path(&root.join("tools/Brewfile")));
    assert!(!is_managed_local_path(&root.join("workspace/repo/file.rs")));
    assert!(!is_managed_local_path(
        &root.join("profiles/personal/store.ron")
    ));
    assert!(!is_managed_local_path(&root.join("knowledge/.DS_Store")));
}

#[test]
fn initialization_rejects_unencrypted_staging_files() {
    let root = tempfile::tempdir().unwrap();
    let repository = prepare_repository(root.path());
    std::fs::write(repository.join("plaintext.txt"), "secret\n").unwrap();

    let error = initialize_paths(root.path(), &repository, &FixedKeyStore::new(11)).unwrap_err();

    assert!(error.contains("unencrypted"));
    assert!(git(&repository, &["rev-parse", "--verify", "HEAD"]).is_err());
}

#[test]
fn empty_remote_receives_encrypted_initial_and_followup_syncs() {
    let root = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    std::fs::write(root.path().join("settings.ron"), "()\n").unwrap();
    let repository = prepare_repository(root.path());
    let keys = FixedKeyStore::new(4);

    connect_remote_paths(
        root.path(),
        &repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(root.path().join("settings.ron"), "(changed: true)\n").unwrap();
    sync_paths(root.path(), &repository, &keys).unwrap();

    assert_eq!(
        git(&repository, &["rev-list", "--count", "origin/main"]).unwrap(),
        "2"
    );
    assert_eq!(local_change_count(root.path(), &repository).unwrap(), 0);
    let tree = git(
        &repository,
        &["ls-tree", "-r", "--name-only", "origin/main"],
    )
    .unwrap();
    assert!(!tree.contains("settings.ron"));
}

#[test]
fn stale_encrypted_commit_is_discarded_and_regenerated_from_plaintext() {
    let root = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let path = root.path().join("settings.ron");
    std::fs::write(&path, "(value: 1)\n").unwrap();
    let repository = prepare_repository(root.path());
    let keys = FixedKeyStore::new(15);
    connect_remote_paths(
        root.path(),
        &repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(&path, "(value: 2)\n").unwrap();
    std::fs::write(repository.join(INDEX_FILE), b"stale encrypted commit").unwrap();
    git(&repository, &["add", INDEX_FILE]).unwrap();
    git(&repository, &["commit", "-m", "Stale local snapshot"]).unwrap();

    sync_paths(root.path(), &repository, &keys).unwrap();

    let manifest = read_manifest(&repository).unwrap();
    let key = keys.load(&manifest.vault_id).unwrap();
    let (_, files) = load_encrypted_snapshot(&repository, &key).unwrap();
    assert_eq!(files["settings.ron"].data, b"(value: 2)\n");
    assert_eq!(git(&repository, &["status", "--short"]).unwrap(), "");
}

#[test]
fn existing_encrypted_vault_merges_non_conflicting_local_files() {
    let seed = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(5);
    std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
    let seed_repository = prepare_repository(seed.path());
    connect_remote_paths(
        seed.path(),
        &seed_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("knowledge")).unwrap();
    std::fs::write(root.path().join("knowledge/local.md"), "# Local\n").unwrap();
    let repository = prepare_repository(root.path());

    connect_remote_paths(
        root.path(),
        &repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(root.path().join("settings.ron")).unwrap(),
        "(remote: true)\n"
    );
    let manifest = read_manifest(&repository).unwrap();
    let key = keys.load(&manifest.vault_id).unwrap();
    let (_, files) = load_encrypted_snapshot(&repository, &key).unwrap();
    assert!(files.contains_key("settings.ron"));
    assert!(files.contains_key("knowledge/local.md"));
}

#[test]
fn existing_encrypted_vault_merges_structured_files_by_key() {
    let seed = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(6);
    std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
    let seed_repository = prepare_repository(seed.path());
    connect_remote_paths(
        seed.path(),
        &seed_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(root.path().join("settings.ron"), "(local: true)\n").unwrap();
    let repository = prepare_repository(root.path());

    connect_remote_paths(
        root.path(),
        &repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();

    let source = std::fs::read_to_string(root.path().join("settings.ron")).unwrap();
    let ron::Value::Map(settings) = ron::from_str::<ron::Value>(&source).unwrap() else {
        panic!("settings must remain a RON map");
    };
    assert_eq!(
        ron_map_get(&settings, &ron::Value::String("local".to_string())),
        Some(&ron::Value::Bool(true))
    );
    assert_eq!(
        ron_map_get(&settings, &ron::Value::String("remote".to_string())),
        Some(&ron::Value::Bool(true))
    );
}

#[test]
fn same_structured_key_prefers_the_local_value() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(10);
    std::fs::write(first.path().join("settings.ron"), "(value: 1)\n").unwrap();
    let first_repository = prepare_repository(first.path());
    connect_remote_paths(
        first.path(),
        &first_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    let second_repository = prepare_repository(second.path());
    connect_remote_paths(
        second.path(),
        &second_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(first.path().join("settings.ron"), "(value: 2)\n").unwrap();
    sync_paths(first.path(), &first_repository, &keys).unwrap();
    std::fs::write(second.path().join("settings.ron"), "(value: 3)\n").unwrap();

    let result = sync_paths(second.path(), &second_repository, &keys).unwrap();
    sync_paths(second.path(), &second_repository, &keys).unwrap();

    assert!(result.contains("automatic merge"));
    let source = std::fs::read_to_string(second.path().join("settings.ron")).unwrap();
    let ron::Value::Map(settings) = ron::from_str::<ron::Value>(&source).unwrap() else {
        panic!("settings must remain a RON map");
    };
    assert_eq!(
        ron_map_get(&settings, &ron::Value::String("value".to_string())),
        Some(&ron::Value::Number(ron::value::Number::Integer(3)))
    );
}

#[test]
fn markdown_changes_from_two_devices_merge_automatically() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(12);
    std::fs::create_dir_all(first.path().join("knowledge")).unwrap();
    std::fs::write(
        first.path().join("knowledge/note.md"),
        "# Note\n\nAlpha\n\nOmega\n",
    )
    .unwrap();
    let first_repository = prepare_repository(first.path());
    connect_remote_paths(
        first.path(),
        &first_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    let second_repository = prepare_repository(second.path());
    connect_remote_paths(
        second.path(),
        &second_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(
        first.path().join("knowledge/note.md"),
        "# Note\n\nAlpha from first\n\nOmega\n",
    )
    .unwrap();
    sync_paths(first.path(), &first_repository, &keys).unwrap();
    std::fs::write(
        second.path().join("knowledge/note.md"),
        "# Note\n\nAlpha\n\nOmega from second\n",
    )
    .unwrap();

    let result = sync_paths(second.path(), &second_repository, &keys).unwrap();

    assert!(result.contains("automatic merge"));
    assert_eq!(
        std::fs::read_to_string(second.path().join("knowledge/note.md")).unwrap(),
        "# Note\n\nAlpha from first\n\nOmega from second\n"
    );
}

#[test]
fn toml_changes_from_two_devices_merge_by_key() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(13);
    std::fs::create_dir_all(first.path().join("tools")).unwrap();
    std::fs::write(
        first.path().join("tools/tools.toml"),
        "[values]\nfirst = 1\nsecond = 1\n",
    )
    .unwrap();
    let first_repository = prepare_repository(first.path());
    connect_remote_paths(
        first.path(),
        &first_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    let second_repository = prepare_repository(second.path());
    connect_remote_paths(
        second.path(),
        &second_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(
        first.path().join("tools/tools.toml"),
        "[values]\nfirst = 2\nsecond = 1\n",
    )
    .unwrap();
    sync_paths(first.path(), &first_repository, &keys).unwrap();
    std::fs::write(
        second.path().join("tools/tools.toml"),
        "[values]\nfirst = 1\nsecond = 2\n",
    )
    .unwrap();

    sync_paths(second.path(), &second_repository, &keys).unwrap();

    let merged = toml::from_str::<toml::Value>(
        &std::fs::read_to_string(second.path().join("tools/tools.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(merged["values"]["first"].as_integer(), Some(2));
    assert_eq!(merged["values"]["second"].as_integer(), Some(2));
}

#[test]
fn opaque_conflicts_keep_remote_and_create_one_local_copy() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(14);
    std::fs::create_dir_all(first.path().join("knowledge")).unwrap();
    std::fs::write(first.path().join("knowledge/data.bin"), b"baseline").unwrap();
    let first_repository = prepare_repository(first.path());
    connect_remote_paths(
        first.path(),
        &first_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    let second_repository = prepare_repository(second.path());
    connect_remote_paths(
        second.path(),
        &second_repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();
    std::fs::write(first.path().join("knowledge/data.bin"), b"remote").unwrap();
    sync_paths(first.path(), &first_repository, &keys).unwrap();
    std::fs::write(second.path().join("knowledge/data.bin"), b"local").unwrap();

    let result = sync_paths(second.path(), &second_repository, &keys).unwrap();
    sync_paths(second.path(), &second_repository, &keys).unwrap();

    assert!(result.contains("1 conflicted copy"));
    assert_eq!(
        std::fs::read(second.path().join("knowledge/data.bin")).unwrap(),
        b"remote"
    );
    let copies = std::fs::read_dir(second.path().join("knowledge"))
        .unwrap()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("data (Conflicted copy ") && name.ends_with(".bin")
        })
        .collect::<Vec<_>>();
    assert_eq!(copies.len(), 1);
    assert_eq!(std::fs::read(copies[0].path()).unwrap(), b"local");
}

#[test]
fn plaintext_remote_history_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let seed = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    git(seed.path(), &["init", "-b", "main"]).unwrap();
    configure_identity(seed.path());
    std::fs::write(seed.path().join("settings.ron"), "()\n").unwrap();
    git(seed.path(), &["add", "--all"]).unwrap();
    git(seed.path(), &["commit", "-m", "Plaintext"]).unwrap();
    git(
        seed.path(),
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    )
    .unwrap();
    git(seed.path(), &["push", "-u", "origin", "main"]).unwrap();
    let repository = prepare_repository(root.path());

    let error = connect_remote_paths(
        root.path(),
        &repository,
        remote.to_string_lossy().as_ref(),
        &FixedKeyStore::new(7),
    )
    .unwrap_err();

    assert!(error.contains("plaintext"));
}

#[test]
fn existing_vault_uses_the_remote_default_branch() {
    let seed = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let remote_parent = tempfile::tempdir().unwrap();
    let remote = remote_parent.path().join("vault.git");
    create_bare_remote(&remote);
    let keys = FixedKeyStore::new(8);
    std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
    let seed_repository = prepare_repository(seed.path());
    initialize_paths(seed.path(), &seed_repository, &keys).unwrap();
    git(&seed_repository, &["branch", "--move", "trunk"]).unwrap();
    git(
        &seed_repository,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    )
    .unwrap();
    git(&seed_repository, &["push", "-u", "origin", "trunk"]).unwrap();
    command_success(
        Command::new("git")
            .args([
                "--git-dir",
                remote.to_string_lossy().as_ref(),
                "symbolic-ref",
                "HEAD",
                "refs/heads/trunk",
            ])
            .output()
            .unwrap(),
    )
    .unwrap();
    let repository = prepare_repository(root.path());

    connect_remote_paths(
        root.path(),
        &repository,
        remote.to_string_lossy().as_ref(),
        &keys,
    )
    .unwrap();

    assert_eq!(current_branch(&repository).unwrap(), "trunk");
    assert_eq!(
        git(&repository, &["rev-list", "--count", "origin/trunk"]).unwrap(),
        "1"
    );
}

#[test]
fn cloud_folder_creates_an_encrypted_bare_vault_repository() {
    let root = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let repository = prepare_repository(root.path());
    std::fs::write(root.path().join("settings.ron"), "()\n").unwrap();

    let remote = connect_folder_paths(
        root.path(),
        &repository,
        cloud.path(),
        &FixedKeyStore::new(9),
    )
    .unwrap();

    assert_eq!(
        git(&repository, &["remote", "get-url", "origin"]).unwrap(),
        remote
    );
    assert_eq!(
        command_success(
            Command::new("git")
                .args(["--git-dir", &remote, "rev-parse", "--is-bare-repository"])
                .output()
                .unwrap(),
        )
        .unwrap(),
        "true"
    );
    let tree = git(
        &repository,
        &["ls-tree", "-r", "--name-only", "origin/main"],
    )
    .unwrap();
    assert!(!tree.contains("settings.ron"));
}
