use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde::Deserialize;

use super::keys::{KeyStore, SystemKeyStore};
use super::repository::{GitHubCli, OutputText, VaultRepositoryPath};
use super::snapshot::{load_encrypted_snapshot, write_encrypted_snapshot};
use super::sync::{collect_local_files, initialize_paths, reconcile_local, write_local_state};
use super::{repository_dir, root_dir};

const GITHUB_VIEWER_QUERY: &str = "query { viewer { login organizations(first: 100) { nodes { login viewerCanCreateRepositories } } } }";

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
pub(super) struct GhRepository {
    pub(super) name_with_owner: String,
    pub(super) is_private: bool,
    pub(super) url: String,
    pub(super) is_empty: bool,
}

#[derive(Deserialize)]
pub(super) struct GhAuthStatus {
    pub(super) hosts: HashMap<String, Vec<GhAuthAccount>>,
}

#[derive(Deserialize)]
pub(super) struct GhAuthAccount {
    pub(super) login: String,
}

#[derive(Deserialize)]
pub(super) struct GhViewerResponse {
    pub(super) data: GhViewerData,
}

#[derive(Deserialize)]
pub(super) struct GhViewerData {
    pub(super) viewer: GhViewer,
}

#[derive(Deserialize)]
pub(super) struct GhViewer {
    pub(super) login: String,
    pub(super) organizations: GhOrganizations,
}

#[derive(Deserialize)]
pub(super) struct GhOrganizations {
    pub(super) nodes: Vec<GhOrganization>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GhOrganization {
    pub(super) login: String,
    pub(super) viewer_can_create_repositories: bool,
}

pub fn connect_github_with_progress<F, C>(mut progress: F, canceled: C) -> Result<String, String>
where
    F: FnMut(String),
    C: Fn() -> bool,
{
    let has_saved_account = github_has_saved_account()?;
    let mut command = GitHubCli::command()?;
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
    GitHubCli::command()?
        .args(["api", "user", "--jq", ".login"])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?
        .success_text()
}

pub(super) fn run_github_auth<F, C>(
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

pub(super) fn github_device_code(line: &str) -> Option<String> {
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

pub(super) fn connect_folder_paths<K: KeyStore>(
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
        let bare = Command::new("git")
            .args([
                "--git-dir",
                &remote_arg,
                "rev-parse",
                "--is-bare-repository",
            ])
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?
            .success_text()?;
        if bare != "true" {
            return Err("selected folder is not a Vault repository".to_string());
        }
    } else {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        let remote_arg = remote.to_string_lossy().into_owned();
        Command::new("git")
            .args(["init", "--bare", &remote_arg])
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?
            .success_text()?;
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
    let vault = VaultRepositoryPath::at(vault_repository);
    let repository = if repository.trim().is_empty() {
        "vmux-vault"
    } else {
        repository.trim()
    };
    initialize_paths(root, vault_repository, keys)?;
    if !vault
        .git()
        .optional(&["remote", "get-url", "origin"])
        .is_empty()
    {
        return Err("Vault already has an origin remote".to_string());
    }
    let root_arg = vault_repository.to_string_lossy().into_owned();
    let visibility = match visibility {
        RepositoryVisibility::Private => "--private",
        RepositoryVisibility::Public => "--public",
    };
    GitHubCli::command()?
        .current_dir(vault_repository)
        .args([
            "repo", "create", repository, visibility, "--source", &root_arg, "--remote", "origin",
            "--push",
        ])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?
        .success_text()?;
    write_local_state(root, vault_repository)?;
    Ok(repository.to_string())
}

pub(super) fn connect_remote_paths<K: KeyStore>(
    root: &Path,
    vault_repository: &Path,
    repository: &str,
    keys: &K,
) -> Result<String, String> {
    let repository = repository.trim();
    if repository.is_empty() {
        return Err("repository is required".to_string());
    }
    let vault = VaultRepositoryPath::at(vault_repository);
    let git = vault.git();
    vault.ensure()?;
    let url = resolve_remote_url(repository)?;
    let previous_remote = git.optional(&["remote", "get-url", "origin"]);
    if !previous_remote.is_empty() {
        git.run(&["remote", "set-url", "origin", &url])?;
    } else {
        git.run(&["remote", "add", "origin", &url])?;
    }
    let result = (|| {
        git.run(&["fetch", "origin"])?;
        let _ = git.run(&["remote", "set-head", "origin", "--auto"]);
        match git.remote_branch() {
            Some(remote_branch) => {
                vault.validate_remote_history(&remote_branch)?;
                let manifest = vault.manifest_at(&remote_branch)?;
                let key = match keys.load(&manifest.vault_id) {
                    Ok(key) => Some(key),
                    Err(_error) if vault.remote_has_key_recipients(&remote_branch)? => None,
                    Err(error) => return Err(error),
                };
                let branch = remote_branch
                    .strip_prefix("origin/")
                    .unwrap_or(&remote_branch);
                git.run(&["checkout", "-B", branch, &remote_branch])?;
                git.run(&["branch", "--set-upstream-to", &remote_branch])?;
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
                git.commit("Connect vmux Vault")?;
                git.run(&["push", "-u", "origin", branch])?;
            }
            None => {
                initialize_paths(root, vault_repository, keys)?;
                let branch = git.current_branch()?;
                git.run(&["push", "-u", "origin", &branch])?;
            }
        }
        write_local_state(root, vault_repository)
    })();
    if let Err(error) = result {
        if previous_remote.is_empty() {
            let _ = git.run(&["remote", "remove", "origin"]);
        } else {
            let _ = git.run(&["remote", "set-url", "origin", &previous_remote]);
        }
        return Err(error);
    }
    Ok(url)
}

fn resolve_remote_url(repository: &str) -> Result<String, String> {
    if repository.contains("://")
        || repository.starts_with("git@")
        || Path::new(repository).is_absolute()
    {
        return Ok(repository.to_string());
    }
    GitHubCli::command()?
        .args(["repo", "view", repository, "--json", "url", "--jq", ".url"])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?
        .success_text()
}

pub(super) fn github_identity_and_repositories()
-> Result<(String, Vec<String>, Vec<VaultRepository>), String> {
    if !github_has_saved_account()? {
        return Ok((String::new(), Vec::new(), Vec::new()));
    }
    let mut command = GitHubCli::command()?;
    command
        .args(["api", "graphql", "-f"])
        .arg(format!("query={GITHUB_VIEWER_QUERY}"));
    let source = command
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?
        .success_text()?;
    let (owner, owners) = github_owners_from_graphql(&source)?;
    let mut repositories = Vec::new();
    for repository_owner in &owners {
        let Ok(source) = GitHubCli::command()?
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
            .map_err(|error| format!("failed to run gh: {error}"))?
            .success_text()
        else {
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

pub(super) fn github_owners_from_graphql(source: &str) -> Result<(String, Vec<String>), String> {
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
    let output = GitHubCli::command()?
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

pub(super) fn has_saved_github_account(source: &str) -> Result<bool, String> {
    let status = serde_json::from_str::<GhAuthStatus>(source).map_err(|error| error.to_string())?;
    Ok(status
        .hosts
        .get("github.com")
        .is_some_and(|accounts| accounts.iter().any(|account| !account.login.is_empty())))
}
