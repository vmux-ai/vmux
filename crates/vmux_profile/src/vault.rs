use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

#[cfg(target_os = "macos")]
use std::io::Write;

use ring::aead;
use ring::digest;
use ring::hkdf;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const FORMAT_VERSION: u32 = 1;
const MANIFEST_VERSION: u32 = 3;
const MANIFEST_FILE: &str = "vault.ron";
const INDEX_FILE: &str = "index.enc";
const OBJECTS_DIR: &str = "objects";
const PASSKEYS_DIR: &str = "keys/passkeys";
const RECOVERY_DIR: &str = "keys/recovery";
const RECOVERY_FILE: &str = "default.ron";
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "ai.vmux.vault";
#[cfg(target_os = "macos")]
const KEY_BROKER_SERVICE: &str = "ai.vmux.vault.key.v1";
const INDEX_AAD: &[u8] = b"vmux-vault-index-v1";
const OBJECT_AAD_PREFIX: &[u8] = b"vmux-vault-object-v1\0";
const PASSKEY_AAD_PREFIX: &[u8] = b"vmux-vault-passkey-v1\0";
const PASSKEY_KDF_PREFIX: &[u8] = b"vmux-vault-passkey-kdf-v1\0";
const PASSKEY_PRF_PREFIX: &[u8] = b"vmux-vault-passkey-prf-v1\0";
const RECOVERY_AAD_PREFIX: &[u8] = b"vmux-vault-recovery-v1\0";
const RECOVERY_KDF_PREFIX: &[u8] = b"vmux-vault-recovery-kdf-v1\0";
const GITHUB_VIEWER_QUERY: &str = "query { viewer { login organizations(first: 100) { nodes { login viewerCanCreateRepositories } } } }";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
static SESSION_KEYS: OnceLock<Mutex<HashMap<String, Zeroizing<Vec<u8>>>>> = OnceLock::new();
static SESSION_KEY_LOAD: OnceLock<Mutex<()>> = OnceLock::new();
const IGNORED_ROOTS: [&str; 8] = [
    "agents",
    "extensions",
    "lsp",
    "local",
    "profiles",
    "spaces",
    "workspace",
    "worktrees",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultStatus {
    pub root: PathBuf,
    pub initialized: bool,
    pub encrypted: bool,
    pub unlocked: bool,
    pub vault_id: String,
    pub passkey_credentials: Vec<String>,
    pub passkey_salt: Vec<u8>,
    pub recovery_enabled: bool,
    pub remote: String,
    pub branch: String,
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    pub github_owner: String,
    pub github_owners: Vec<String>,
    pub repositories: Vec<VaultRepository>,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRepository {
    pub name: String,
    pub url: String,
    pub private: bool,
    pub empty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepositoryVisibility {
    Private,
    Public,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRepository {
    name_with_owner: String,
    is_private: bool,
    url: String,
    is_empty: bool,
}

#[derive(Deserialize)]
struct GhAuthStatus {
    hosts: HashMap<String, Vec<GhAuthAccount>>,
}

#[derive(Deserialize)]
struct GhAuthAccount {
    login: String,
}

#[derive(Deserialize)]
struct GhViewerResponse {
    data: GhViewerData,
}

#[derive(Deserialize)]
struct GhViewerData {
    viewer: GhViewer,
}

#[derive(Deserialize)]
struct GhViewer {
    login: String,
    organizations: GhOrganizations,
}

#[derive(Deserialize)]
struct GhOrganizations {
    nodes: Vec<GhOrganization>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhOrganization {
    login: String,
    viewer_can_create_repositories: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum EntryKind {
    File,
    Symlink,
}

#[derive(Clone, Debug)]
struct LocalEntry {
    kind: EntryKind,
    mode: u32,
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
    data: Vec<u8>,
    digest: String,
}

#[derive(Clone, Copy, Debug)]
struct LocalFingerprint {
    kind: EntryKind,
    mode: u32,
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RemoteManifest {
    version: u32,
    cipher: String,
    vault_id: String,
    index: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct PasskeyEnvelope {
    version: u32,
    credential_id: String,
    wrapped_key: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RecoveryEnvelope {
    version: u32,
    wrapped_key: Vec<u8>,
}

struct RecoveryKeyLength;

pub struct RecoveryKeyCreation {
    pub pending_upload: bool,
}

impl hkdf::KeyType for RecoveryKeyLength {
    fn len(&self) -> usize {
        KEY_LEN
    }
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
struct LocalState {
    version: u32,
    files: Vec<LocalStateEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LocalStateEntry {
    path: String,
    digest: String,
    kind: EntryKind,
    mode: u32,
    #[serde(default)]
    data: Option<Vec<u8>>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    modified_secs: u64,
    #[serde(default)]
    modified_nanos: u32,
}

#[derive(Default)]
struct ReconcileOutcome {
    automatic_merges: usize,
    conflict_copies: usize,
}

#[derive(Clone, Copy)]
enum TextMergeStrategy {
    Local,
    Union,
}

trait KeyStore {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String>;
    fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String>;
    fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String>;
}

struct SystemKeyStore;

impl KeyStore for SystemKeyStore {
    fn load(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        load_system_key(vault_id)
    }

    fn create(&self, vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        create_system_key(vault_id)
    }

    fn store(&self, vault_id: &str, key: &[u8]) -> Result<(), String> {
        store_system_key(vault_id, key)
    }
}

pub fn root_dir() -> PathBuf {
    super::config_dir()
}

pub fn repository_dir() -> PathBuf {
    super::application_data_dir().join("vault")
}

pub fn is_managed_local_path(path: &Path) -> bool {
    path.strip_prefix(root_dir())
        .ok()
        .is_some_and(|relative| !relative.as_os_str().is_empty() && !ignored_path(relative))
}

pub fn status() -> VaultStatus {
    status_paths(&root_dir(), &repository_dir(), &SystemKeyStore)
}

pub fn status_with_repositories() -> VaultStatus {
    let mut status = status();
    if !status.initialized || status.remote.is_empty() {
        match github_identity_and_repositories() {
            Ok((owner, owners, repositories)) => {
                status.github_owner = owner;
                status.github_owners = owners;
                status.repositories = repositories;
            }
            Err(error) => status.error = error,
        }
    }
    status
}

pub fn connect_github_with_progress<F, C>(mut progress: F, canceled: C) -> Result<String, String>
where
    F: FnMut(String),
    C: Fn() -> bool,
{
    let has_saved_account = github_has_saved_account()?;
    let mut command = gh_command()?;
    if has_saved_account {
        command.args([
            "auth",
            "refresh",
            "--hostname",
            "github.com",
            "--reset-scopes",
            "--clipboard",
        ]);
    } else {
        command.args([
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--git-protocol",
            "https",
            "--web",
            "--clipboard",
            "--skip-ssh-key",
        ]);
    }
    run_github_auth(&mut command, &mut progress, &canceled)?;
    if canceled() {
        return Err("GitHub authorization canceled".to_string());
    }
    command_success(
        gh_command()?
            .args(["api", "user", "--jq", ".login"])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )
}

fn run_github_auth<F, C>(
    command: &mut Command,
    progress: &mut F,
    canceled: &C,
) -> Result<(), String>
where
    F: FnMut(String),
    C: Fn() -> bool,
{
    let mut child = command
        .env("BROWSER", "/usr/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to read gh output".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to read gh errors".to_string())?;
    let (sender, receiver) = mpsc::channel();
    let stdout_reader = spawn_line_reader(stdout, sender.clone());
    let stderr_reader = spawn_line_reader(stderr, sender);
    let mut lines = Vec::new();
    let mut reported_code = false;
    let status = loop {
        if canceled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("GitHub authorization canceled".to_string());
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                if !reported_code && let Some(code) = github_device_code(&line) {
                    progress(code);
                    reported_code = true;
                }
                lines.push(line);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {}
        }
        match child
            .try_wait()
            .map_err(|error| format!("failed to wait for gh: {error}"))?
        {
            Some(status) => break status,
            None => continue,
        }
    };
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    for line in receiver.try_iter() {
        if !reported_code && let Some(code) = github_device_code(&line) {
            progress(code);
            reported_code = true;
        }
        lines.push(line);
    }
    if status.success() {
        Ok(())
    } else {
        let message = lines
            .into_iter()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_else(|| "GitHub authorization failed".to_string());
        Err(message)
    }
}

fn spawn_line_reader<R>(reader: R, sender: mpsc::Sender<String>) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    })
}

fn github_device_code(line: &str) -> Option<String> {
    line.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .find(|token| {
            let bytes = token.as_bytes();
            bytes.len() == 9
                && bytes[4] == b'-'
                && bytes.iter().enumerate().all(|(index, byte)| {
                    index == 4 || byte.is_ascii_uppercase() || byte.is_ascii_digit()
                })
        })
        .map(str::to_string)
}

pub fn connect_folder(folder: &Path) -> Result<String, String> {
    connect_folder_paths(&root_dir(), &repository_dir(), folder, &SystemKeyStore)
}

pub fn create_remote(repository: &str, visibility: RepositoryVisibility) -> Result<String, String> {
    create_remote_paths(
        &root_dir(),
        &repository_dir(),
        repository,
        visibility,
        &SystemKeyStore,
    )
}

pub fn connect_remote(repository: &str) -> Result<String, String> {
    connect_remote_paths(&root_dir(), &repository_dir(), repository, &SystemKeyStore)
}

pub fn sync() -> Result<String, String> {
    sync_paths(&root_dir(), &repository_dir(), &SystemKeyStore)
}

pub fn initialize() -> Result<(), String> {
    initialize_paths(&root_dir(), &repository_dir(), &SystemKeyStore)
}

pub fn add_passkey(credential_id: &str, prf_output: &[u8]) -> Result<String, String> {
    add_passkey_paths(
        &repository_dir(),
        &SystemKeyStore,
        credential_id,
        prf_output,
    )
}

pub fn prepare_passkey() -> Result<String, String> {
    let repository = repository_dir();
    let manifest = read_manifest(&repository)?;
    let key = load_repository_key(&repository, &SystemKeyStore, &manifest.vault_id)?;
    load_encrypted_snapshot(&repository, &key)?;
    Ok("Vault unlocked".to_string())
}

pub fn unlock_with_passkey(credential_id: &str, prf_output: &[u8]) -> Result<String, String> {
    unlock_with_passkey_paths(
        &root_dir(),
        &repository_dir(),
        &SystemKeyStore,
        credential_id,
        prf_output,
    )
}

pub fn create_recovery_key(recovery_key: &str) -> Result<RecoveryKeyCreation, String> {
    create_recovery_key_paths(&repository_dir(), &SystemKeyStore, recovery_key)
}

pub fn unlock_with_recovery_key(recovery_key: &str) -> Result<String, String> {
    unlock_with_recovery_key_paths(
        &root_dir(),
        &repository_dir(),
        &SystemKeyStore,
        recovery_key,
    )
}

fn status_paths<K: KeyStore>(root: &Path, repository: &Path, keys: &K) -> VaultStatus {
    let initialized = repository.join(".git").is_dir();
    let manifest = initialized
        .then(|| read_manifest(repository))
        .transpose()
        .ok()
        .flatten();
    let unlocked = manifest.as_ref().is_some_and(|manifest| {
        state_path(repository).is_file() && keys.load(&manifest.vault_id).is_ok()
    });
    let mut status = VaultStatus {
        root: root.to_path_buf(),
        initialized,
        encrypted: manifest.is_some(),
        unlocked,
        ..VaultStatus::default()
    };
    if let Some(manifest) = manifest {
        status.vault_id = manifest.vault_id.clone();
        status.passkey_salt = passkey_salt(&manifest.vault_id).to_vec();
        match read_passkey_envelopes(repository) {
            Ok(envelopes) => {
                status.passkey_credentials = envelopes
                    .into_values()
                    .map(|envelope| envelope.credential_id)
                    .collect();
            }
            Err(error) => status.error = error,
        }
        match read_recovery_envelope(repository) {
            Ok(envelope) => status.recovery_enabled = envelope.is_some(),
            Err(error) => status.error = error,
        }
    }
    if initialized {
        status.remote = git_optional(repository, &["remote", "get-url", "origin"]);
        status.branch = git_optional(repository, &["branch", "--show-current"]);
        status.dirty = local_change_count(root, repository).unwrap_or(0);
        if !status.remote.is_empty() {
            let counts = git_optional(
                repository,
                &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
            );
            let mut values = counts.split_whitespace();
            status.ahead = values
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            status.behind = values
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
        }
    } else if root.join(".git").is_dir() {
        status.error = "A legacy plaintext Vault was found. Create a new encrypted repository; reusing its remote would leave plaintext in Git history.".to_string();
    }
    status
}

fn connect_folder_paths<K: KeyStore>(
    root: &Path,
    repository: &Path,
    folder: &Path,
    keys: &K,
) -> Result<String, String> {
    let remote = if folder
        .extension()
        .is_some_and(|extension| extension == "git")
    {
        folder.to_path_buf()
    } else {
        folder.join("vmux-vault.git")
    };
    if remote.exists() {
        let remote_arg = remote.to_string_lossy().into_owned();
        let bare = command_success(
            Command::new("git")
                .args([
                    "--git-dir",
                    &remote_arg,
                    "rev-parse",
                    "--is-bare-repository",
                ])
                .output()
                .map_err(|error| format!("failed to run git: {error}"))?,
        )?;
        if bare != "true" {
            return Err("selected folder is not a Vault repository".to_string());
        }
    } else {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        let remote_arg = remote.to_string_lossy().into_owned();
        command_success(
            Command::new("git")
                .args(["init", "--bare", &remote_arg])
                .output()
                .map_err(|error| format!("failed to run git: {error}"))?,
        )?;
    }
    connect_remote_paths(root, repository, &remote.to_string_lossy(), keys)?;
    Ok(remote.to_string_lossy().into_owned())
}

fn create_remote_paths<K: KeyStore>(
    root: &Path,
    vault_repository: &Path,
    repository: &str,
    visibility: RepositoryVisibility,
    keys: &K,
) -> Result<String, String> {
    let repository = if repository.trim().is_empty() {
        "vmux-vault"
    } else {
        repository.trim()
    };
    initialize_paths(root, vault_repository, keys)?;
    if !git_optional(vault_repository, &["remote", "get-url", "origin"]).is_empty() {
        return Err("Vault already has an origin remote".to_string());
    }
    let root_arg = vault_repository.to_string_lossy().into_owned();
    let visibility = match visibility {
        RepositoryVisibility::Private => "--private",
        RepositoryVisibility::Public => "--public",
    };
    command_success(
        gh_command()?
            .current_dir(vault_repository)
            .args([
                "repo", "create", repository, visibility, "--source", &root_arg, "--remote",
                "origin", "--push",
            ])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )?;
    write_local_state(root, vault_repository)?;
    Ok(repository.to_string())
}

fn connect_remote_paths<K: KeyStore>(
    root: &Path,
    vault_repository: &Path,
    repository: &str,
    keys: &K,
) -> Result<String, String> {
    let repository = repository.trim();
    if repository.is_empty() {
        return Err("repository is required".to_string());
    }
    ensure_repository(vault_repository)?;
    let url = resolve_remote_url(repository)?;
    let previous_remote = git_optional(vault_repository, &["remote", "get-url", "origin"]);
    if !previous_remote.is_empty() {
        git(vault_repository, &["remote", "set-url", "origin", &url])?;
    } else {
        git(vault_repository, &["remote", "add", "origin", &url])?;
    }
    let result = (|| {
        git(vault_repository, &["fetch", "origin"])?;
        let _ = git(
            vault_repository,
            &["remote", "set-head", "origin", "--auto"],
        );
        match remote_branch(vault_repository) {
            Some(remote_branch) => {
                validate_remote_history(vault_repository, &remote_branch)?;
                let manifest = manifest_from_ref(vault_repository, &remote_branch)?;
                let key = match keys.load(&manifest.vault_id) {
                    Ok(key) => Some(key),
                    Err(_error) if remote_has_key_recipients(vault_repository, &remote_branch)? => {
                        None
                    }
                    Err(error) => return Err(error),
                };
                let branch = remote_branch
                    .strip_prefix("origin/")
                    .unwrap_or(&remote_branch);
                git(
                    vault_repository,
                    &["checkout", "-B", branch, &remote_branch],
                )?;
                git(
                    vault_repository,
                    &["branch", "--set-upstream-to", &remote_branch],
                )?;
                let Some(key) = key else {
                    return Ok(());
                };
                let (_, remote_files) = load_encrypted_snapshot(vault_repository, &key)?;
                reconcile_local(root, &BTreeMap::new(), &remote_files)?;
                let files = collect_local_files(root)?;
                write_encrypted_snapshot(
                    vault_repository,
                    &manifest.vault_id,
                    &key,
                    &files,
                    Some(&remote_files),
                )?;
                commit_changes(vault_repository, "Connect vmux Vault")?;
                git(vault_repository, &["push", "-u", "origin", branch])?;
            }
            None => {
                initialize_paths(root, vault_repository, keys)?;
                let branch = current_branch(vault_repository)?;
                git(vault_repository, &["push", "-u", "origin", &branch])?;
            }
        }
        write_local_state(root, vault_repository)
    })();
    if let Err(error) = result {
        if previous_remote.is_empty() {
            let _ = git(vault_repository, &["remote", "remove", "origin"]);
        } else {
            let _ = git(
                vault_repository,
                &["remote", "set-url", "origin", &previous_remote],
            );
        }
        return Err(error);
    }
    Ok(url)
}

fn sync_paths<K: KeyStore>(root: &Path, repository: &Path, keys: &K) -> Result<String, String> {
    if !repository.join(".git").is_dir() {
        return Err("Vault is not connected to Git".to_string());
    }
    if git_optional(repository, &["remote", "get-url", "origin"]).is_empty() {
        return Err("Vault has no origin remote".to_string());
    }
    let manifest = read_manifest(repository)?;
    let key = load_repository_key(repository, keys, &manifest.vault_id)?;
    let baseline = baseline_files(repository).unwrap_or_else(|_| {
        load_encrypted_snapshot(repository, &key)
            .map(|(_, files)| files)
            .unwrap_or_default()
    });
    let branch = current_branch(repository)?;
    for attempt in 0..3 {
        git(repository, &["fetch", "origin"])?;
        if let Some(remote_branch) = remote_branch(repository) {
            validate_remote_history(repository, &remote_branch)?;
            if git(repository, &["merge-base", "HEAD", &remote_branch]).is_err() {
                return Err("Vault remote has unrelated history".to_string());
            }
            git(repository, &["reset", "--hard", &remote_branch])?;
        }
        let (_, remote_files) = load_encrypted_snapshot(repository, &key)?;
        let outcome = reconcile_local(root, &baseline, &remote_files)?;
        let files = collect_local_files(root)?;
        write_encrypted_snapshot(
            repository,
            &manifest.vault_id,
            &key,
            &files,
            Some(&remote_files),
        )?;
        commit_changes(repository, "Sync vmux Vault")?;
        match git(repository, &["push", "-u", "origin", &branch]) {
            Ok(_) => {
                write_local_state(root, repository)?;
                return Ok(sync_message(&outcome));
            }
            Err(error) if attempt < 2 && push_rejected_for_remote_change(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Err("Vault remote kept changing during sync".to_string())
}

fn push_rejected_for_remote_change(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("non-fast-forward")
        || error.contains("fetch first")
        || error.contains("failed to push some refs")
}

fn sync_message(outcome: &ReconcileOutcome) -> String {
    if outcome.conflict_copies > 0 {
        format!(
            "Vault synced with {} conflicted {}",
            outcome.conflict_copies,
            if outcome.conflict_copies == 1 {
                "copy"
            } else {
                "copies"
            }
        )
    } else if outcome.automatic_merges > 0 {
        format!(
            "Vault synced with {} automatic {}",
            outcome.automatic_merges,
            if outcome.automatic_merges == 1 {
                "merge"
            } else {
                "merges"
            }
        )
    } else {
        "Vault synced".to_string()
    }
}

fn initialize_paths<K: KeyStore>(root: &Path, repository: &Path, keys: &K) -> Result<(), String> {
    ensure_repository(repository)?;
    let (vault_id, key, previous) = match read_manifest(repository) {
        Ok(manifest) => {
            let key = load_repository_key(repository, keys, &manifest.vault_id)?;
            let previous = load_encrypted_snapshot(repository, &key)
                .ok()
                .map(|(_, files)| files);
            (manifest.vault_id, key, previous)
        }
        Err(_) => {
            validate_empty_vault_repository(repository)?;
            let vault_id = random_hex(16)?;
            let key = keys.create(&vault_id)?;
            (vault_id, key, None)
        }
    };
    let files = collect_local_files(root)?;
    write_encrypted_snapshot(repository, &vault_id, &key, &files, previous.as_ref())?;
    commit_changes(repository, "Initialize vmux Vault")
}

fn add_passkey_paths<K: KeyStore>(
    repository: &Path,
    keys: &K,
    credential_id: &str,
    prf_output: &[u8],
) -> Result<String, String> {
    validate_credential_id(credential_id)?;
    let mut manifest = read_manifest(repository)?;
    let key = load_repository_key(repository, keys, &manifest.vault_id)?;
    let wrapping_key = derive_passkey_wrapping_key(prf_output, &manifest.vault_id, credential_id)?;
    let wrapped_key = encrypt_bytes(
        &wrapping_key,
        &passkey_aad(&manifest.vault_id, credential_id),
        &key,
    )?;
    let envelope = PasskeyEnvelope {
        version: FORMAT_VERSION,
        credential_id: credential_id.to_string(),
        wrapped_key,
    };
    let source = ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?;
    let directory = repository.join(PASSKEYS_DIR);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    write_atomic(
        &directory.join(passkey_envelope_name(credential_id)),
        format!("{source}\n").as_bytes(),
    )?;
    manifest.version = MANIFEST_VERSION;
    write_manifest(repository, &manifest)?;
    validate_encrypted_worktree(repository)?;
    commit_changes(repository, "Add Vault passkey")?;
    if !git_optional(repository, &["remote", "get-url", "origin"]).is_empty() {
        let branch = current_branch(repository)?;
        git(repository, &["push", "-u", "origin", &branch])?;
    }
    Ok("Vault passkey added".to_string())
}

fn unlock_with_passkey_paths<K: KeyStore>(
    root: &Path,
    repository: &Path,
    keys: &K,
    credential_id: &str,
    prf_output: &[u8],
) -> Result<String, String> {
    validate_credential_id(credential_id)?;
    let manifest = read_manifest(repository)?;
    let envelope = read_passkey_envelopes(repository)?
        .remove(credential_id)
        .ok_or_else(|| "This passkey is not authorized for the Vault".to_string())?;
    let wrapping_key = derive_passkey_wrapping_key(prf_output, &manifest.vault_id, credential_id)?;
    let key = Zeroizing::new(decrypt_bytes(
        &wrapping_key,
        &passkey_aad(&manifest.vault_id, credential_id),
        &envelope.wrapped_key,
    )?);
    validate_key(&key)?;
    let (_, remote_files) = load_encrypted_snapshot(repository, &key)?;
    keys.store(&manifest.vault_id, &key)?;
    reconcile_local(root, &BTreeMap::new(), &remote_files)?;
    write_local_state(root, repository)?;
    Ok("Vault unlocked".to_string())
}

fn create_recovery_key_paths<K: KeyStore>(
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

fn unlock_with_recovery_key_paths<K: KeyStore>(
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

fn read_passkey_envelopes(repository: &Path) -> Result<BTreeMap<String, PasskeyEnvelope>, String> {
    let directory = repository.join(PASSKEYS_DIR);
    if !directory.exists() {
        return Ok(BTreeMap::new());
    }
    let mut envelopes = BTreeMap::new();
    for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
            return Err("invalid Vault passkey recipient".to_string());
        }
        let source = std::fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
        let envelope = ron::from_str::<PasskeyEnvelope>(&source)
            .map_err(|error| format!("invalid Vault passkey recipient: {error}"))?;
        if envelope.version != FORMAT_VERSION {
            return Err("unsupported Vault passkey recipient".to_string());
        }
        validate_credential_id(&envelope.credential_id)?;
        if entry.file_name().to_string_lossy() != passkey_envelope_name(&envelope.credential_id) {
            return Err("Vault passkey recipient identifier mismatch".to_string());
        }
        if envelopes
            .insert(envelope.credential_id.clone(), envelope)
            .is_some()
        {
            return Err("duplicate Vault passkey recipient".to_string());
        }
    }
    Ok(envelopes)
}

fn read_recovery_envelope(repository: &Path) -> Result<Option<RecoveryEnvelope>, String> {
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

fn load_repository_key<K: KeyStore>(
    repository: &Path,
    keys: &K,
    vault_id: &str,
) -> Result<Zeroizing<Vec<u8>>, String> {
    keys.load(vault_id).map_err(|error| {
        let has_passkey = read_passkey_envelopes(repository)
            .is_ok_and(|envelopes| !envelopes.is_empty());
        let has_recovery = read_recovery_envelope(repository).is_ok_and(|envelope| envelope.is_some());
        if !has_passkey && !has_recovery {
            "This Vault is locked on this device. No recovery method is registered. Open it on a device that can already unlock it, then add a Recovery Key or passkey."
                .to_string()
        } else {
            error
        }
    })
}

fn passkey_envelope_name(credential_id: &str) -> String {
    format!(
        "{}.ron",
        hex(digest::digest(&digest::SHA256, credential_id.as_bytes()).as_ref())
    )
}

fn passkey_salt(vault_id: &str) -> [u8; KEY_LEN] {
    let mut input = Vec::with_capacity(PASSKEY_PRF_PREFIX.len() + vault_id.len());
    input.extend_from_slice(PASSKEY_PRF_PREFIX);
    input.extend_from_slice(vault_id.as_bytes());
    let digest = digest::digest(&digest::SHA256, &input);
    digest.as_ref().try_into().unwrap()
}

fn derive_passkey_wrapping_key(
    prf_output: &[u8],
    vault_id: &str,
    credential_id: &str,
) -> Result<[u8; KEY_LEN], String> {
    if prf_output.len() != KEY_LEN {
        return Err("passkey did not return an encryption-capable PRF result".to_string());
    }
    let key = hmac::Key::new(hmac::HMAC_SHA256, prf_output);
    let mut input =
        Vec::with_capacity(PASSKEY_KDF_PREFIX.len() + vault_id.len() + credential_id.len() + 1);
    input.extend_from_slice(PASSKEY_KDF_PREFIX);
    input.extend_from_slice(vault_id.as_bytes());
    input.push(0);
    input.extend_from_slice(credential_id.as_bytes());
    Ok(hmac::sign(&key, &input).as_ref().try_into().unwrap())
}

fn passkey_aad(vault_id: &str, credential_id: &str) -> Vec<u8> {
    let mut aad =
        Vec::with_capacity(PASSKEY_AAD_PREFIX.len() + vault_id.len() + credential_id.len() + 1);
    aad.extend_from_slice(PASSKEY_AAD_PREFIX);
    aad.extend_from_slice(vault_id.as_bytes());
    aad.push(0);
    aad.extend_from_slice(credential_id.as_bytes());
    aad
}

fn derive_recovery_wrapping_key(
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

fn recovery_aad(vault_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(RECOVERY_AAD_PREFIX.len() + vault_id.len());
    aad.extend_from_slice(RECOVERY_AAD_PREFIX);
    aad.extend_from_slice(vault_id.as_bytes());
    aad
}

#[cfg(test)]
fn format_recovery_key(key: &[u8]) -> String {
    let encoded = hex(key);
    let groups = encoded
        .as_bytes()
        .chunks(4)
        .map(|group| std::str::from_utf8(group).unwrap())
        .collect::<Vec<_>>();
    format!("vmux-{}", groups.join("-"))
}

fn parse_recovery_key(source: &str) -> Result<Zeroizing<Vec<u8>>, String> {
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

fn validate_credential_id(credential_id: &str) -> Result<(), String> {
    if credential_id.is_empty()
        || credential_id.len() > 4096
        || !credential_id.len().is_multiple_of(2)
        || !credential_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid Vault passkey credential".to_string());
    }
    Ok(())
}

fn validate_empty_vault_repository(repository: &Path) -> Result<(), String> {
    if git(repository, &["rev-parse", "--verify", "HEAD"]).is_ok() {
        return Err("Vault staging repository contains unsupported history".to_string());
    }
    let unexpected = std::fs::read_dir(repository)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .any(|name| name != ".git");
    if unexpected {
        Err("Vault staging repository contains unencrypted files".to_string())
    } else {
        Ok(())
    }
}

fn ensure_repository(repository: &Path) -> Result<(), String> {
    std::fs::create_dir_all(repository).map_err(|error| error.to_string())?;
    if !repository.join(".git").is_dir() {
        git(repository, &["init", "-b", "main"])?;
    }
    Ok(())
}

fn collect_local_files(root: &Path) -> Result<BTreeMap<String, LocalEntry>, String> {
    let mut files = BTreeMap::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_directory(root, root, &mut files)?;
    Ok(files)
}

fn collect_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, LocalEntry>,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if ignored_path(relative) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_dir() {
            collect_directory(root, &path, files)?;
            continue;
        }
        let kind = if metadata.file_type().is_symlink() {
            EntryKind::Symlink
        } else if metadata.file_type().is_file() {
            EntryKind::File
        } else {
            continue;
        };
        let relative = relative
            .to_str()
            .ok_or_else(|| "Vault paths must be valid UTF-8".to_string())?
            .replace(std::path::MAIN_SEPARATOR, "/");
        validate_relative_path(&relative)?;
        let data = match kind {
            EntryKind::File => std::fs::read(&path).map_err(|error| error.to_string())?,
            EntryKind::Symlink => symlink_target_bytes(&path)?,
        };
        let mode = file_mode(&metadata);
        let (modified_secs, modified_nanos) = modified_time(&metadata);
        let digest = entry_digest(kind, mode, &data);
        files.insert(
            relative,
            LocalEntry {
                kind,
                mode,
                size: metadata.len(),
                modified_secs,
                modified_nanos,
                data,
                digest,
            },
        );
    }
    Ok(())
}

fn collect_local_fingerprints(root: &Path) -> Result<BTreeMap<String, LocalFingerprint>, String> {
    let mut files = BTreeMap::new();
    if root.exists() {
        collect_fingerprint_directory(root, root, &mut files)?;
    }
    Ok(files)
}

fn collect_fingerprint_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, LocalFingerprint>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if ignored_path(relative) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_dir() {
            collect_fingerprint_directory(root, &path, files)?;
            continue;
        }
        let kind = if metadata.file_type().is_symlink() {
            EntryKind::Symlink
        } else if metadata.file_type().is_file() {
            EntryKind::File
        } else {
            continue;
        };
        let relative = relative
            .to_str()
            .ok_or_else(|| "Vault paths must be valid UTF-8".to_string())?
            .replace(std::path::MAIN_SEPARATOR, "/");
        validate_relative_path(&relative)?;
        let (modified_secs, modified_nanos) = modified_time(&metadata);
        files.insert(
            relative,
            LocalFingerprint {
                kind,
                mode: file_mode(&metadata),
                size: metadata.len(),
                modified_secs,
                modified_nanos,
            },
        );
    }
    Ok(())
}

fn ignored_path(relative: &Path) -> bool {
    if relative.file_name().is_some_and(|name| name == ".DS_Store") {
        return true;
    }
    let first = relative
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        });
    first.is_some_and(|name| {
        name == ".git" || name == ".vmux-vault" || IGNORED_ROOTS.contains(&name)
    })
}

fn reconcile_local(
    root: &Path,
    baseline: &BTreeMap<String, LocalEntry>,
    remote: &BTreeMap<String, LocalEntry>,
) -> Result<ReconcileOutcome, String> {
    let local = collect_local_files(root)?;
    let paths = baseline
        .keys()
        .chain(local.keys())
        .chain(remote.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut updates = Vec::new();
    let mut occupied = paths.clone();
    let mut outcome = ReconcileOutcome::default();
    for path in paths {
        let baseline_entry = baseline.get(&path);
        let local_entry = local.get(&path);
        let remote_entry = remote.get(&path);
        let local_changed = !same_entry(local_entry, baseline_entry);
        let remote_changed = !same_entry(remote_entry, baseline_entry);
        if local_changed && remote_changed && !same_entry(local_entry, remote_entry) {
            if let Some(entry) =
                merge_changed_file(&path, baseline_entry, local_entry, remote_entry)?
            {
                updates.push((path, Some(entry)));
                outcome.automatic_merges += 1;
                continue;
            }
            updates.push((path.clone(), remote_entry.cloned()));
            if let Some(local_entry) = local_entry {
                let copy_path = conflict_copy_path(&path, &mut occupied)?;
                updates.push((copy_path, Some(local_entry.clone())));
                outcome.conflict_copies += 1;
            }
        } else if remote_changed && !local_changed {
            updates.push((path, remote_entry.cloned()));
        }
    }
    let mut merged = local.clone();
    for (path, entry) in &updates {
        if let Some(entry) = entry {
            merged.insert(path.clone(), entry.clone());
        } else {
            merged.remove(path);
        }
    }
    validate_file_tree(&merged)?;
    for (path, entry) in updates {
        apply_local_entry(root, &path, entry.as_ref())?;
    }
    Ok(outcome)
}

fn merge_changed_file(
    path: &str,
    baseline: Option<&LocalEntry>,
    local: Option<&LocalEntry>,
    remote: Option<&LocalEntry>,
) -> Result<Option<LocalEntry>, String> {
    let (Some(local), Some(remote)) = (local, remote) else {
        return Ok(None);
    };
    if local.kind != EntryKind::File || remote.kind != EntryKind::File {
        return Ok(None);
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let baseline_data = baseline
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.data.as_slice())
        .unwrap_or_default();
    let data = match extension.as_str() {
        "md" | "markdown" => {
            if [baseline_data, local.data.as_slice(), remote.data.as_slice()]
                .into_iter()
                .any(|data| std::str::from_utf8(data).is_err())
            {
                return Ok(None);
            }
            merge_text(
                baseline_data,
                &local.data,
                &remote.data,
                TextMergeStrategy::Union,
            )?
        }
        "ron" => {
            if [baseline_data, local.data.as_slice(), remote.data.as_slice()]
                .into_iter()
                .any(|data| std::str::from_utf8(data).is_err())
            {
                return Ok(None);
            }
            let ron_baseline = if baseline.is_none() {
                b"{}".as_slice()
            } else {
                baseline_data
            };
            let merged = match merge_ron(ron_baseline, &local.data, &remote.data)? {
                Some(merged) => merged,
                None => merge_text(
                    baseline_data,
                    &local.data,
                    &remote.data,
                    TextMergeStrategy::Local,
                )?,
            };
            let source = std::str::from_utf8(&merged).ok();
            if source.is_none_or(|source| ron::from_str::<serde::de::IgnoredAny>(source).is_err()) {
                return Ok(None);
            }
            merged
        }
        "toml" => {
            let Some(merged) = merge_toml(baseline_data, &local.data, &remote.data)? else {
                return Ok(None);
            };
            merged
        }
        "json" => {
            let json_baseline = if baseline.is_none() {
                b"{}".as_slice()
            } else {
                baseline_data
            };
            let Some(merged) = merge_json(json_baseline, &local.data, &remote.data)? else {
                return Ok(None);
            };
            merged
        }
        _ => return Ok(None),
    };
    Ok(Some(entry_with_data(local, data)))
}

fn entry_with_data(template: &LocalEntry, data: Vec<u8>) -> LocalEntry {
    let mut entry = template.clone();
    entry.size = data.len() as u64;
    entry.modified_secs = 0;
    entry.modified_nanos = 0;
    entry.digest = entry_digest(entry.kind, entry.mode, &data);
    entry.data = data;
    entry
}

fn merge_text(
    baseline: &[u8],
    local: &[u8],
    remote: &[u8],
    strategy: TextMergeStrategy,
) -> Result<Vec<u8>, String> {
    if std::str::from_utf8(baseline).is_err()
        || std::str::from_utf8(local).is_err()
        || std::str::from_utf8(remote).is_err()
    {
        return Err("Vault text merge requires UTF-8 files".to_string());
    }
    let directory = std::env::temp_dir().join(format!("vmux-vault-merge-{}", random_hex(8)?));
    std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let baseline_path = directory.join("baseline");
    let local_path = directory.join("local");
    let remote_path = directory.join("remote");
    let result = (|| {
        std::fs::write(&baseline_path, baseline).map_err(|error| error.to_string())?;
        std::fs::write(&local_path, local).map_err(|error| error.to_string())?;
        std::fs::write(&remote_path, remote).map_err(|error| error.to_string())?;
        let strategy = match strategy {
            TextMergeStrategy::Local => "--ours",
            TextMergeStrategy::Union => "--union",
        };
        let output = Command::new("git")
            .arg("merge-file")
            .arg(strategy)
            .arg("--stdout")
            .arg(&local_path)
            .arg(&baseline_path)
            .arg(&remote_path)
            .output()
            .map_err(|error| format!("failed to merge Vault text: {error}"))?;
        if output.status.code().is_some_and(|code| code <= 127) {
            Ok(output.stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    })();
    let _ = std::fs::remove_dir_all(directory);
    result
}

fn merge_toml(baseline: &[u8], local: &[u8], remote: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Ok(baseline) = std::str::from_utf8(baseline) else {
        return Ok(None);
    };
    let Ok(local) = std::str::from_utf8(local) else {
        return Ok(None);
    };
    let Ok(remote) = std::str::from_utf8(remote) else {
        return Ok(None);
    };
    let Ok(baseline) = toml::from_str::<toml::Value>(baseline) else {
        return Ok(None);
    };
    let Ok(local) = toml::from_str::<toml::Value>(local) else {
        return Ok(None);
    };
    let Ok(remote) = toml::from_str::<toml::Value>(remote) else {
        return Ok(None);
    };
    let Some(merged) = merge_toml_value(Some(&baseline), Some(&local), Some(&remote)) else {
        return Ok(None);
    };
    Ok(Some(
        toml::to_string_pretty(&merged)
            .map_err(|error| error.to_string())?
            .into_bytes(),
    ))
}

fn merge_ron(baseline: &[u8], local: &[u8], remote: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Ok(baseline) = ron::from_str::<ron::Value>(std::str::from_utf8(baseline).unwrap_or(""))
    else {
        return Ok(None);
    };
    let Ok(local) = ron::from_str::<ron::Value>(std::str::from_utf8(local).unwrap_or("")) else {
        return Ok(None);
    };
    let Ok(remote) = ron::from_str::<ron::Value>(std::str::from_utf8(remote).unwrap_or("")) else {
        return Ok(None);
    };
    let Some(merged) = merge_ron_value(Some(&baseline), Some(&local), Some(&remote)) else {
        return Ok(None);
    };
    let mut output = ron::ser::to_string_pretty(&merged, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?
        .into_bytes();
    output.push(b'\n');
    Ok(Some(output))
}

fn merge_ron_value(
    baseline: Option<&ron::Value>,
    local: Option<&ron::Value>,
    remote: Option<&ron::Value>,
) -> Option<ron::Value> {
    if ron_value_options_equal(local, remote) {
        return local.cloned();
    }
    if ron_value_options_equal(local, baseline) {
        return remote.cloned();
    }
    if ron_value_options_equal(remote, baseline) {
        return local.cloned();
    }
    match (baseline, local, remote) {
        (
            Some(ron::Value::Map(baseline)),
            Some(ron::Value::Map(local)),
            Some(ron::Value::Map(remote)),
        ) => {
            let keys = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(ron::Value::Map(
                keys.into_iter()
                    .filter_map(|key| {
                        merge_ron_value(
                            ron_map_get(baseline, &key),
                            ron_map_get(local, &key),
                            ron_map_get(remote, &key),
                        )
                        .map(|value| (key, value))
                    })
                    .collect(),
            ))
        }
        _ => local.cloned(),
    }
}

fn ron_value_options_equal(left: Option<&ron::Value>, right: Option<&ron::Value>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => ron_values_equal(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn ron_values_equal(left: &ron::Value, right: &ron::Value) -> bool {
    match (left, right) {
        (ron::Value::Map(left), ron::Value::Map(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    ron_map_get(right, key).is_some_and(|other| ron_values_equal(value, other))
                })
        }
        (ron::Value::Seq(left), ron::Value::Seq(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| ron_values_equal(left, right))
        }
        (ron::Value::Option(left), ron::Value::Option(right)) => {
            ron_value_options_equal(left.as_deref(), right.as_deref())
        }
        _ => left == right,
    }
}

fn ron_map_get<'a>(map: &'a ron::value::Map, key: &ron::Value) -> Option<&'a ron::Value> {
    map.iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

fn merge_toml_value(
    baseline: Option<&toml::Value>,
    local: Option<&toml::Value>,
    remote: Option<&toml::Value>,
) -> Option<toml::Value> {
    if local == remote {
        return local.cloned();
    }
    if local == baseline {
        return remote.cloned();
    }
    if remote == baseline {
        return local.cloned();
    }
    match (baseline, local, remote) {
        (
            Some(toml::Value::Table(baseline)),
            Some(toml::Value::Table(local)),
            Some(toml::Value::Table(remote)),
        ) => {
            let keys = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(toml::Value::Table(
                keys.into_iter()
                    .filter_map(|key| {
                        merge_toml_value(baseline.get(&key), local.get(&key), remote.get(&key))
                            .map(|value| (key, value))
                    })
                    .collect(),
            ))
        }
        _ => local.cloned(),
    }
}

fn merge_json(baseline: &[u8], local: &[u8], remote: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Ok(baseline) = serde_json::from_slice::<serde_json::Value>(baseline) else {
        return Ok(None);
    };
    let Ok(local) = serde_json::from_slice::<serde_json::Value>(local) else {
        return Ok(None);
    };
    let Ok(remote) = serde_json::from_slice::<serde_json::Value>(remote) else {
        return Ok(None);
    };
    let Some(merged) = merge_json_value(Some(&baseline), Some(&local), Some(&remote)) else {
        return Ok(None);
    };
    let mut output = serde_json::to_vec_pretty(&merged).map_err(|error| error.to_string())?;
    output.push(b'\n');
    Ok(Some(output))
}

fn merge_json_value(
    baseline: Option<&serde_json::Value>,
    local: Option<&serde_json::Value>,
    remote: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    if local == remote {
        return local.cloned();
    }
    if local == baseline {
        return remote.cloned();
    }
    if remote == baseline {
        return local.cloned();
    }
    match (baseline, local, remote) {
        (
            Some(serde_json::Value::Object(baseline)),
            Some(serde_json::Value::Object(local)),
            Some(serde_json::Value::Object(remote)),
        ) => {
            let keys = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            Some(serde_json::Value::Object(
                keys.into_iter()
                    .filter_map(|key| {
                        merge_json_value(baseline.get(&key), local.get(&key), remote.get(&key))
                            .map(|value| (key, value))
                    })
                    .collect(),
            ))
        }
        _ => local.cloned(),
    }
}

fn conflict_copy_path(path: &str, occupied: &mut BTreeSet<String>) -> Result<String, String> {
    let path = Path::new(path);
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|extension| extension.to_str());
    let label = conflict_copy_label();
    for index in 1..=u16::MAX {
        let suffix = if index == 1 {
            String::new()
        } else {
            format!(" {index}")
        };
        let file_name = match extension {
            Some(extension) => {
                format!("{stem} (Conflicted copy {label}){suffix}.{extension}")
            }
            None => format!("{stem} (Conflicted copy {label}){suffix}"),
        };
        let candidate = parent.join(file_name).to_string_lossy().replace('\\', "/");
        validate_relative_path(&candidate)?;
        if occupied.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err("failed to allocate Vault conflict copy".to_string())
}

fn conflict_copy_label() -> String {
    let device = Command::new("/bin/hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|device| !device.is_empty())
        .unwrap_or_else(|| "device".to_string());
    let device = device
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let device = device.trim_matches('-');
    let device = if device.is_empty() { "device" } else { device };
    format!(
        "{} {}",
        device,
        chrono::Local::now().format("%Y-%m-%d %H-%M-%S")
    )
}

fn validate_file_tree(files: &BTreeMap<String, LocalEntry>) -> Result<(), String> {
    for path in files.keys() {
        let mut parent = Path::new(path).parent();
        while let Some(candidate) = parent {
            if let Some(candidate) = candidate.to_str()
                && files.contains_key(candidate)
            {
                return Err(format!(
                    "Vault has incompatible file and directory changes: {candidate}, {path}"
                ));
            }
            parent = candidate.parent();
        }
    }
    Ok(())
}

fn same_entry(left: Option<&LocalEntry>, right: Option<&LocalEntry>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            left.digest == right.digest && left.kind == right.kind && left.mode == right.mode
        }
        (None, None) => true,
        _ => false,
    }
}

fn apply_local_entry(
    root: &Path,
    relative: &str,
    entry: Option<&LocalEntry>,
) -> Result<(), String> {
    validate_relative_path(relative)?;
    let path = root.join(relative);
    match entry {
        Some(entry) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            remove_existing_path(&path)?;
            match entry.kind {
                EntryKind::File => {
                    write_atomic(&path, &entry.data)?;
                    set_file_mode(&path, entry.mode)?;
                }
                EntryKind::Symlink => create_symlink(&path, &entry.data)?,
            }
        }
        None => {
            remove_existing_path(&path)?;
            prune_empty_parents(root, path.parent());
        }
    }
    Ok(())
}

fn remove_existing_path(path: &Path) -> Result<(), String> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        std::fs::remove_file(path).map_err(|error| error.to_string())
    }
}

fn prune_empty_parents(root: &Path, mut parent: Option<&Path>) {
    while let Some(directory) = parent {
        if directory == root || !directory.starts_with(root) {
            break;
        }
        if std::fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}

fn write_encrypted_snapshot(
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

fn same_files(left: &BTreeMap<String, LocalEntry>, right: &BTreeMap<String, LocalEntry>) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|(path, entry)| same_entry(Some(entry), right.get(path)))
}

fn load_encrypted_snapshot(
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

fn read_manifest(repository: &Path) -> Result<RemoteManifest, String> {
    let source = std::fs::read(repository.join(MANIFEST_FILE))
        .map_err(|error| format!("failed to read encrypted Vault manifest: {error}"))?;
    parse_manifest(&source)
}

fn write_manifest(repository: &Path, manifest: &RemoteManifest) -> Result<(), String> {
    let source = ron::ser::to_string_pretty(manifest, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?;
    write_atomic(
        &repository.join(MANIFEST_FILE),
        format!("{source}\n").as_bytes(),
    )
}

fn manifest_from_ref(repository: &Path, branch: &str) -> Result<RemoteManifest, String> {
    let spec = format!("{branch}:{MANIFEST_FILE}");
    let source = git(repository, &["show", &spec])?;
    parse_manifest(source.as_bytes())
}

fn parse_manifest(source: &[u8]) -> Result<RemoteManifest, String> {
    let source = std::str::from_utf8(source)
        .map_err(|error| format!("selected repository is not an encrypted vmux Vault: {error}"))?;
    let manifest = ron::from_str::<RemoteManifest>(source)
        .map_err(|error| format!("selected repository is not an encrypted vmux Vault: {error}"))?;
    if !(FORMAT_VERSION..=MANIFEST_VERSION).contains(&manifest.version)
        || manifest.cipher != "AES-256-GCM"
        || manifest.index != INDEX_FILE
        || manifest.vault_id.is_empty()
    {
        return Err("selected repository uses an unsupported Vault encryption format".to_string());
    }
    Ok(manifest)
}

fn validate_remote_history(repository: &Path, branch: &str) -> Result<(), String> {
    let entries = git(
        repository,
        &["log", branch, "--name-only", "--pretty=format:"],
    )?;
    let valid = entries
        .lines()
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .all(|entry| {
            entry == MANIFEST_FILE
                || entry == INDEX_FILE
                || entry
                    .strip_prefix(&format!("{OBJECTS_DIR}/"))
                    .is_some_and(|name| name.len() == 64 && !name.contains('/'))
                || valid_passkey_path(entry)
                || valid_recovery_path(entry)
        });
    if !valid {
        return Err("selected repository contains plaintext or non-Vault history".to_string());
    }
    let _ = manifest_from_ref(repository, branch)?;
    Ok(())
}

fn remote_has_key_recipients(repository: &Path, branch: &str) -> Result<bool, String> {
    Ok(!git(
        repository,
        &["ls-tree", "-r", "--name-only", branch, "keys"],
    )?
    .is_empty())
}

fn validate_encrypted_worktree(repository: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(repository).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if name != ".git"
            && name != MANIFEST_FILE
            && name != INDEX_FILE
            && name != OBJECTS_DIR
            && name != "keys"
        {
            return Err(format!(
                "Vault staging repository contains unencrypted file: {}",
                name.to_string_lossy()
            ));
        }
    }
    for entry in
        std::fs::read_dir(repository.join(OBJECTS_DIR)).map_err(|error| error.to_string())?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let id = name.as_str();
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file())
            || id.len() != 64
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!("invalid encrypted Vault object: {name}"));
        }
    }
    let keys = repository.join("keys");
    if keys.exists() {
        for entry in std::fs::read_dir(&keys).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                return Err("invalid encrypted Vault key recipients".to_string());
            }
            match entry.file_name().to_str() {
                Some("passkeys") => {
                    let _ = read_passkey_envelopes(repository)?;
                }
                Some("recovery") => {
                    let entries = std::fs::read_dir(entry.path())
                        .map_err(|error| error.to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| error.to_string())?;
                    if entries.len() != 1
                        || entries[0].file_name() != RECOVERY_FILE
                        || !entries[0]
                            .file_type()
                            .is_ok_and(|file_type| file_type.is_file())
                    {
                        return Err("invalid Vault Recovery Key recipients".to_string());
                    }
                    let _ = read_recovery_envelope(repository)?;
                }
                _ => return Err("invalid encrypted Vault key recipients".to_string()),
            }
        }
    }
    Ok(())
}

fn valid_passkey_path(path: &str) -> bool {
    let Some(name) = path.strip_prefix(&format!("{PASSKEYS_DIR}/")) else {
        return false;
    };
    name.len() == 68
        && name.ends_with(".ron")
        && name[..64]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_recovery_path(path: &str) -> bool {
    path == format!("{RECOVERY_DIR}/{RECOVERY_FILE}")
}

fn encrypt_bytes(key: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
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

fn decrypt_bytes(key: &[u8], aad: &[u8], encrypted: &[u8]) -> Result<Vec<u8>, String> {
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

fn object_id(key: &[u8], path: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hex(hmac::sign(&key, path.as_bytes()).as_ref())
}

fn object_aad(path: &str) -> Vec<u8> {
    let mut aad = OBJECT_AAD_PREFIX.to_vec();
    aad.extend_from_slice(path.as_bytes());
    aad
}

fn entry_digest(kind: EntryKind, mode: u32, data: &[u8]) -> String {
    let mut context = digest::Context::new(&digest::SHA256);
    context.update(match kind {
        EntryKind::File => b"file\0",
        EntryKind::Symlink => b"symlink\0",
    });
    context.update(&mode.to_be_bytes());
    context.update(data);
    hex(context.finish().as_ref())
}

fn validate_key(key: &[u8]) -> Result<(), String> {
    if key.len() == KEY_LEN {
        Ok(())
    } else {
        Err("Vault encryption key has an invalid length".to_string())
    }
}

fn validate_relative_path(path: &str) -> Result<(), String> {
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

fn write_local_state(root: &Path, repository: &Path) -> Result<(), String> {
    let files = collect_local_files(root)?;
    let state = LocalState {
        version: FORMAT_VERSION,
        files: files
            .into_iter()
            .map(|(path, entry)| LocalStateEntry {
                path,
                digest: entry.digest,
                kind: entry.kind,
                mode: entry.mode,
                data: Some(entry.data),
                size: entry.size,
                modified_secs: entry.modified_secs,
                modified_nanos: entry.modified_nanos,
            })
            .collect(),
    };
    let source = ron::ser::to_string(&state).map_err(|error| error.to_string())?;
    write_atomic(&state_path(repository), source.as_bytes())
}

fn local_change_count(root: &Path, repository: &Path) -> Result<u32, String> {
    let local = collect_local_fingerprints(root)?;
    let state = read_local_state(repository).unwrap_or_default();
    let paths = local
        .keys()
        .chain(state.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    Ok(paths
        .into_iter()
        .filter(|path| {
            let local = local.get(path);
            let state = state.get(path);
            match (local, state) {
                (Some(local), Some(state)) => {
                    local.kind != state.kind
                        || local.mode != state.mode
                        || local.size != state.size
                        || local.modified_secs != state.modified_secs
                        || local.modified_nanos != state.modified_nanos
                }
                (None, None) => false,
                _ => true,
            }
        })
        .count() as u32)
}

fn read_local_state(repository: &Path) -> Result<BTreeMap<String, LocalStateEntry>, String> {
    let source = std::fs::read(state_path(repository)).map_err(|error| error.to_string())?;
    let source = std::str::from_utf8(&source).map_err(|error| error.to_string())?;
    let state = ron::from_str::<LocalState>(source).map_err(|error| error.to_string())?;
    if state.version != FORMAT_VERSION {
        return Err("unsupported Vault local state".to_string());
    }
    Ok(state
        .files
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect())
}

fn baseline_files(repository: &Path) -> Result<BTreeMap<String, LocalEntry>, String> {
    read_local_state(repository)?
        .into_iter()
        .map(|(path, entry)| {
            let data = entry
                .data
                .ok_or_else(|| "Vault baseline needs refresh".to_string())?;
            Ok((
                path,
                LocalEntry {
                    kind: entry.kind,
                    mode: entry.mode,
                    size: entry.size,
                    modified_secs: entry.modified_secs,
                    modified_nanos: entry.modified_nanos,
                    data,
                    digest: entry.digest,
                },
            ))
        })
        .collect::<Result<_, String>>()
}

fn state_path(repository: &Path) -> PathBuf {
    repository.join(".git").join("vmux-state.ron")
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
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

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut value = vec![0_u8; bytes];
    SystemRandom::new()
        .fill(&mut value)
        .map_err(|_| "failed to generate secure random data".to_string())?;
    Ok(hex(&value))
}

fn modified_time(metadata: &std::fs::Metadata) -> (u64, u32) {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(source: &str) -> Result<Vec<u8>, String> {
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

fn decode_hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("invalid hexadecimal data".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn keychain_options(
    service: &str,
    vault_id: &str,
    synchronized: bool,
) -> security_framework::passwords::PasswordOptions {
    let mut options =
        security_framework::passwords::PasswordOptions::new_generic_password(service, vault_id);
    options.set_access_synchronized(Some(synchronized));
    options
}

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn key_broker_load(vault_id: &str) -> Result<Option<String>, String> {
    use security_framework::passwords::generic_password;
    use security_framework_sys::base::errSecItemNotFound;

    match generic_password(keychain_options(KEY_BROKER_SERVICE, vault_id, false)) {
        Ok(key) => {
            validate_key(&key)?;
            Ok(Some(hex(&key)))
        }
        Err(error) if error.code() == errSecItemNotFound => Ok(None),
        Err(error) => Err(format!("failed to unlock Vault key: {error}")),
    }
}

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn key_broker_store(vault_id: &str, encoded_key: &str) -> Result<(), String> {
    let key = decode_key_hex(encoded_key)?;
    store_keychain_key(KEY_BROKER_SERVICE, vault_id, &key)
}

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn migrate_legacy_key(vault_id: &str) -> Result<bool, String> {
    let Some(key) = load_legacy_key(vault_id)? else {
        return Ok(false);
    };
    if key_broker_path().is_none() {
        return Err("Vault key broker is missing".to_string());
    }
    store_key_with_broker(vault_id, &key)?;
    Ok(true)
}

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn authorize_key_broker_parent() -> Result<(), String> {
    use security_framework::os::macos::code_signing::{
        Flags, GuestAttributes, SecCode, SecRequirement,
    };
    use std::str::FromStr;

    let parent_pid = unsafe { libc::getppid() };
    let mut attributes = GuestAttributes::new();
    attributes.set_pid(parent_pid);
    let parent = SecCode::copy_guest_with_attribues(None, &attributes, Flags::NONE)
        .map_err(|error| format!("failed to identify Vault key broker caller: {error}"))?;
    let broker = std::env::current_exe().map_err(|error| error.to_string())?;
    let certificate = signing_leaf_hash(&broker)?;
    let requirement =
        SecRequirement::from_str(&format!("certificate leaf = H\"{certificate}\""))
            .map_err(|error| format!("failed to create Vault key broker requirement: {error}"))?;
    validate_key_broker_caller(&parent, &requirement)?;

    let expected = broker
        .parent()
        .ok_or_else(|| "Vault key broker has no parent directory".to_string())?
        .join("vmux_desktop")
        .canonicalize()
        .map_err(|error| format!("failed to locate vmux desktop: {error}"))?;
    let actual = parent_process_path(parent_pid)?
        .canonicalize()
        .map_err(|error| format!("failed to resolve Vault key broker caller: {error}"))?;
    if actual != expected {
        return Err("Vault key broker rejected its caller".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_key_broker_caller(
    caller: &security_framework::os::macos::code_signing::SecCode,
    requirement: &security_framework::os::macos::code_signing::SecRequirement,
) -> Result<(), String> {
    use security_framework::os::macos::code_signing::Flags;

    caller
        .check_validity(Flags::NONE, requirement)
        .map_err(|_| "Vault key broker rejected its caller".to_string())
}

#[cfg(not(target_os = "macos"))]
#[doc(hidden)]
pub fn key_broker_load(_vault_id: &str) -> Result<Option<String>, String> {
    Err("Vault key broker is only available on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
#[doc(hidden)]
pub fn key_broker_store(_vault_id: &str, _encoded_key: &str) -> Result<(), String> {
    Err("Vault key broker is only available on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
#[doc(hidden)]
pub fn migrate_legacy_key(_vault_id: &str) -> Result<bool, String> {
    Err("Legacy Vault keys are only available on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
#[doc(hidden)]
pub fn authorize_key_broker_parent() -> Result<(), String> {
    Err("Vault key broker is only available on macOS".to_string())
}

#[cfg(target_os = "macos")]
fn load_legacy_key(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    use security_framework::passwords::generic_password;
    use security_framework_sys::base::errSecItemNotFound;

    for synchronized in [false, true] {
        match generic_password(keychain_options(KEYCHAIN_SERVICE, vault_id, synchronized)) {
            Ok(key) => {
                validate_key(&key)?;
                return Ok(Some(Zeroizing::new(key)));
            }
            Err(error) if error.code() == errSecItemNotFound => {}
            Err(error) => return Err(format!("failed to unlock Vault encryption key: {error}")),
        }
    }
    Ok(None)
}

#[cfg(target_os = "macos")]
fn load_system_key(vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    load_or_store_session_key(vault_id, || {
        if let Some(key) = load_key_from_broker(vault_id)? {
            return Ok(key);
        }
        let Some(key) = load_legacy_key(vault_id)? else {
            return Err(
                "This Vault is locked on this device. Unlock it with a passkey.".to_string(),
            );
        };
        if key_broker_path().is_some() {
            store_key_with_broker(vault_id, &key)?;
        }
        Ok(key)
    })
}

#[cfg(target_os = "macos")]
fn create_system_key(vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut key = Zeroizing::new(vec![0_u8; KEY_LEN]);
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| "failed to generate Vault encryption key".to_string())?;
    store_system_key(vault_id, &key)?;
    Ok(key)
}

#[cfg(target_os = "macos")]
fn store_system_key(vault_id: &str, key: &[u8]) -> Result<(), String> {
    if key_broker_path().is_some() {
        store_key_with_broker(vault_id, key)?;
    } else {
        store_keychain_key(KEYCHAIN_SERVICE, vault_id, key)?;
    }
    store_session_key(vault_id, key)
}

#[cfg(target_os = "macos")]
fn store_keychain_key(service: &str, vault_id: &str, key: &[u8]) -> Result<(), String> {
    use security_framework::passwords::{AccessControlOptions, set_generic_password_options};

    validate_key(key)?;
    let mut protected = keychain_options(service, vault_id, false);
    protected.set_access_control_options(AccessControlOptions::USER_PRESENCE);
    if set_generic_password_options(key, protected).is_err() {
        set_generic_password_options(key, keychain_options(service, vault_id, false))
            .map_err(|error| format!("failed to store Vault key in system Keychain: {error}"))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn key_broker_path() -> Option<PathBuf> {
    if super::build_profile() == "dev" {
        return None;
    }
    let path = std::env::current_exe().ok()?.parent()?.join("vmux");
    path.is_file().then_some(path)
}

#[cfg(target_os = "macos")]
fn parent_process_path(pid: libc::pid_t) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;

    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            buffer.len().try_into().unwrap_or(u32::MAX),
        )
    };
    if length <= 0 {
        return Err("failed to locate Vault key broker caller".to_string());
    }
    buffer.truncate(length as usize);
    Ok(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

#[cfg(target_os = "macos")]
fn signing_leaf_hash(path: &Path) -> Result<String, String> {
    let directory =
        std::env::temp_dir().join(format!("vmux-vault-certificate-{}", random_hex(16)?));
    std::fs::create_dir(&directory)
        .map_err(|error| format!("failed to prepare Vault certificate check: {error}"))?;
    let output = Command::new("/usr/bin/codesign")
        .args(["--display", "--extract-certificates"])
        .arg(path)
        .current_dir(&directory)
        .output()
        .map_err(|error| format!("failed to inspect Vault key broker signature: {error}"))?;
    let certificate_path = directory.join("codesign0");
    let result = if output.status.success() {
        std::fs::read(&certificate_path)
            .map(|certificate| {
                hex(
                    ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &certificate)
                        .as_ref(),
                )
            })
            .map_err(|error| format!("failed to read Vault key broker certificate: {error}"))
    } else {
        Err("Vault key broker is not signed with a certificate".to_string())
    };
    let _ = std::fs::remove_dir_all(directory);
    result
}

#[cfg(target_os = "macos")]
fn run_key_broker(
    action: &str,
    vault_id: &str,
    input: Option<&str>,
) -> Result<Option<Output>, String> {
    let Some(path) = key_broker_path() else {
        return Ok(None);
    };
    let mut command = Command::new(path);
    command
        .args(["vault-key", action, "--vault-id", vault_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_none() {
        return command
            .output()
            .map(Some)
            .map_err(|error| format!("failed to run Vault key broker: {error}"));
    }
    let mut child = command
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run Vault key broker: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open Vault key broker input".to_string())?
        .write_all(input.unwrap_or_default().as_bytes())
        .map_err(|error| format!("failed to send key to Vault key broker: {error}"))?;
    child
        .wait_with_output()
        .map(Some)
        .map_err(|error| format!("failed to wait for Vault key broker: {error}"))
}

#[cfg(target_os = "macos")]
fn load_key_from_broker(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let Some(output) = run_key_broker("load", vault_id, None)? else {
        return Ok(None);
    };
    if output.status.success() {
        let key = decode_key_hex(String::from_utf8_lossy(&output.stdout).trim())?;
        return Ok(Some(Zeroizing::new(key)));
    }
    if output.status.code() == Some(2) {
        return Ok(None);
    }
    Err(key_broker_error(&output))
}

#[cfg(target_os = "macos")]
fn store_key_with_broker(vault_id: &str, key: &[u8]) -> Result<(), String> {
    let encoded = Zeroizing::new(hex(key));
    let Some(output) = run_key_broker("store", vault_id, Some(&encoded))? else {
        return store_keychain_key(KEYCHAIN_SERVICE, vault_id, key);
    };
    if output.status.success() {
        Ok(())
    } else {
        Err(key_broker_error(&output))
    }
}

#[cfg(target_os = "macos")]
fn key_broker_error(output: &Output) -> String {
    let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if error.is_empty() {
        "Vault key broker failed".to_string()
    } else {
        error
    }
}

#[cfg(any(target_os = "macos", test))]
fn decode_key_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() != KEY_LEN * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Vault encryption key has an invalid encoding".to_string());
    }
    let bytes = value.as_bytes();
    let mut key = Vec::with_capacity(KEY_LEN);
    for pair in bytes.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        key.push((high << 4) | low);
    }
    validate_key(&key)?;
    Ok(key)
}

#[cfg(any(target_os = "macos", test))]
fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("Vault encryption key has an invalid encoding".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
fn load_system_key(vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    load_session_key(vault_id)?
        .ok_or_else(|| "This Vault is locked on this device. Unlock it with a passkey.".to_string())
}

#[cfg(not(target_os = "macos"))]
fn create_system_key(_vault_id: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    Err("Encrypted Vault key storage is not available on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
fn store_system_key(vault_id: &str, key: &[u8]) -> Result<(), String> {
    store_session_key(vault_id, key)
}

fn load_session_key(vault_id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    Ok(SESSION_KEYS
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?
        .get(vault_id)
        .map(|key| Zeroizing::new(key.to_vec())))
}

fn store_session_key(vault_id: &str, key: &[u8]) -> Result<(), String> {
    validate_key(key)?;
    SESSION_KEYS
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?
        .insert(vault_id.to_string(), Zeroizing::new(key.to_vec()));
    Ok(())
}

fn load_or_store_session_key<F>(vault_id: &str, load: F) -> Result<Zeroizing<Vec<u8>>, String>
where
    F: FnOnce() -> Result<Zeroizing<Vec<u8>>, String>,
{
    if let Some(key) = load_session_key(vault_id)? {
        return Ok(key);
    }
    let _load = SESSION_KEY_LOAD
        .get_or_init(Default::default)
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(key) = load_session_key(vault_id)? {
        return Ok(key);
    }
    let key = load()?;
    store_session_key(vault_id, &key)?;
    Ok(key)
}

#[cfg(unix)]
fn file_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn file_mode(_metadata: &std::fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn symlink_target_bytes(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::unix::ffi::OsStrExt;
    Ok(std::fs::read_link(path)
        .map_err(|error| error.to_string())?
        .as_os_str()
        .as_bytes()
        .to_vec())
}

#[cfg(not(unix))]
fn symlink_target_bytes(_path: &Path) -> Result<Vec<u8>, String> {
    Err("Vault symlinks are not supported on this platform".to_string())
}

#[cfg(unix)]
fn create_symlink(path: &Path, target: &[u8]) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    std::os::unix::fs::symlink(OsStr::from_bytes(target), path).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn create_symlink(_path: &Path, _target: &[u8]) -> Result<(), String> {
    Err("Vault symlinks are not supported on this platform".to_string())
}

fn commit_changes(root: &Path, message: &str) -> Result<(), String> {
    git(root, &["add", "--all"])?;
    if git_optional(root, &["status", "--porcelain"]).is_empty() {
        return Ok(());
    }
    git(
        root,
        &["-c", "commit.gpgSign=false", "commit", "-m", message],
    )?;
    Ok(())
}

fn current_branch(root: &Path) -> Result<String, String> {
    let branch = git(root, &["branch", "--show-current"])?;
    if branch.is_empty() {
        Err("Vault has no current branch".to_string())
    } else {
        Ok(branch)
    }
}

fn remote_branch(root: &Path) -> Option<String> {
    let symbolic = git_optional(
        root,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    );
    if symbolic.starts_with("origin/") && git(root, &["rev-parse", "--verify", &symbolic]).is_ok() {
        return Some(symbolic);
    }
    for branch in ["origin/main", "origin/master"] {
        if git(root, &["rev-parse", "--verify", branch]).is_ok() {
            return Some(branch.to_string());
        }
    }
    let branches = git_optional(
        root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin",
        ],
    )
    .lines()
    .filter(|branch| *branch != "origin/HEAD")
    .map(str::to_string)
    .collect::<Vec<_>>();
    if branches.len() == 1 {
        branches.into_iter().next()
    } else {
        None
    }
}

fn resolve_remote_url(repository: &str) -> Result<String, String> {
    if repository.contains("://")
        || repository.starts_with("git@")
        || Path::new(repository).is_absolute()
    {
        return Ok(repository.to_string());
    }
    command_success(
        gh_command()?
            .args(["repo", "view", repository, "--json", "url", "--jq", ".url"])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )
}

fn github_identity_and_repositories() -> Result<(String, Vec<String>, Vec<VaultRepository>), String>
{
    if !github_has_saved_account()? {
        return Ok((String::new(), Vec::new(), Vec::new()));
    }
    let mut command = gh_command()?;
    command
        .args(["api", "graphql", "-f"])
        .arg(format!("query={GITHUB_VIEWER_QUERY}"));
    let source = command_success(
        command
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )?;
    let (owner, owners) = github_owners_from_graphql(&source)?;
    let mut repositories = Vec::new();
    for repository_owner in &owners {
        let Ok(source) = command_success(
            gh_command()?
                .args([
                    "repo",
                    "list",
                    repository_owner,
                    "--limit",
                    "100",
                    "--json",
                    "nameWithOwner,isPrivate,url,isEmpty",
                ])
                .output()
                .map_err(|error| format!("failed to run gh: {error}"))?,
        ) else {
            continue;
        };
        repositories.extend(
            serde_json::from_str::<Vec<GhRepository>>(&source)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|repository| VaultRepository {
                    name: repository.name_with_owner,
                    url: repository.url,
                    private: repository.is_private,
                    empty: repository.is_empty,
                }),
        );
    }
    repositories.sort_by(|left, right| {
        right
            .empty
            .cmp(&left.empty)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok((owner, owners, repositories))
}

fn github_owners_from_graphql(source: &str) -> Result<(String, Vec<String>), String> {
    let viewer = serde_json::from_str::<GhViewerResponse>(source)
        .map_err(|error| error.to_string())?
        .data
        .viewer;
    let owner = viewer.login;
    let mut owners = vec![owner.clone()];
    owners.extend(
        viewer
            .organizations
            .nodes
            .into_iter()
            .filter(|organization| organization.viewer_can_create_repositories)
            .map(|organization| organization.login),
    );
    owners.sort();
    owners.dedup();
    if let Some(index) = owners.iter().position(|candidate| candidate == &owner) {
        owners.swap(0, index);
    }
    Ok((owner, owners))
}

fn github_has_saved_account() -> Result<bool, String> {
    let output = gh_command()?
        .args([
            "auth",
            "status",
            "--hostname",
            "github.com",
            "--json",
            "hosts",
        ])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    let source = String::from_utf8_lossy(&output.stdout);
    has_saved_github_account(&source)
}

fn has_saved_github_account(source: &str) -> Result<bool, String> {
    let status = serde_json::from_str::<GhAuthStatus>(source).map_err(|error| error.to_string())?;
    Ok(status
        .hosts
        .get("github.com")
        .is_some_and(|accounts| accounts.iter().any(|account| !account.login.is_empty())))
}

fn gh_command() -> Result<Command, String> {
    let executable = github_cli().ok_or_else(|| "GitHub CLI is not installed".to_string())?;
    let config_dir = github_config_dir();
    std::fs::create_dir_all(&config_dir)
        .map_err(|error| format!("failed to create GitHub config directory: {error}"))?;
    let mut command = Command::new(executable);
    for variable in github_environment_variables() {
        command.env_remove(variable);
    }
    command
        .env("GH_CONFIG_DIR", config_dir)
        .env("GH_NO_UPDATE_NOTIFIER", "1");
    Ok(command)
}

fn github_cli() -> Option<PathBuf> {
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join("gh"))
                .find(|candidate| candidate.is_file())
        })
        .or_else(|| {
            ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]
                .into_iter()
                .map(PathBuf::from)
                .find(|candidate| candidate.is_file())
        })
}

fn github_config_dir() -> PathBuf {
    super::application_data_dir().join("auth/github")
}

fn github_environment_variables() -> [&'static str; 7] {
    [
        "GH_CONFIG_DIR",
        "GH_ENTERPRISE_TOKEN",
        "GH_HOST",
        "GH_PROMPT_DISABLED",
        "GH_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
        "GITHUB_TOKEN",
    ]
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.current_dir(root).env("GIT_TERMINAL_PROMPT", "0");
    if let Some(executable) = github_cli() {
        let credential_helper = format!(
            "credential.https://github.com.helper=!{} auth git-credential",
            shell_quote(&executable.to_string_lossy())
        );
        command
            .args([
                "-c",
                "credential.https://github.com.helper=",
                "-c",
                &credential_helper,
            ])
            .env("GH_CONFIG_DIR", github_config_dir());
        for variable in github_environment_variables() {
            if variable != "GH_CONFIG_DIR" {
                command.env_remove(variable);
            }
        }
    }
    command.args(args);
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
        "GIT_GRAFT_FILE",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_PREFIX",
        "GIT_REPLACE_REF_BASE",
        "GIT_SHALLOW_FILE",
    ] {
        command.env_remove(variable);
    }
    command_success(
        command
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?,
    )
}

fn git_optional(root: &Path, args: &[&str]) -> String {
    git(root, args).unwrap_or_default()
}

fn command_success(output: Output) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

#[cfg(test)]
mod tests {
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

        let error =
            initialize_paths(root.path(), &repository, &FixedKeyStore::new(11)).unwrap_err();

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
}
