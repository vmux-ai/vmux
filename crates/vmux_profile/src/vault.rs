use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::Deserialize;

const REQUIRED_IGNORES: [&str; 8] = [
    ".DS_Store",
    "/agents/",
    "/extensions/",
    "/lsp/",
    "/local/",
    "/profiles/",
    "/spaces/",
    "/worktrees/",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultStatus {
    pub root: PathBuf,
    pub initialized: bool,
    pub remote: String,
    pub branch: String,
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    pub github_owner: String,
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
    ssh_url: String,
    is_empty: bool,
}

pub fn root_dir() -> PathBuf {
    super::config_dir()
}

pub fn status() -> VaultStatus {
    status_in(&root_dir())
}

pub fn status_with_repositories() -> VaultStatus {
    let mut status = status();
    if !status.initialized || status.remote.is_empty() {
        match github_identity_and_repositories() {
            Ok((owner, repositories)) => {
                status.github_owner = owner;
                status.repositories = repositories;
            }
            Err(error) => status.error = error,
        }
    }
    status
}

pub fn status_in(root: &Path) -> VaultStatus {
    let initialized = root.join(".git").is_dir();
    let mut status = VaultStatus {
        root: root.to_path_buf(),
        initialized,
        ..VaultStatus::default()
    };
    if initialized {
        status.remote = git_optional(root, &["remote", "get-url", "origin"]);
        status.branch = git_optional(root, &["branch", "--show-current"]);
        status.dirty = git_optional(root, &["status", "--porcelain"])
            .lines()
            .count() as u32;
        if !status.remote.is_empty() {
            let counts = git_optional(
                root,
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
    }
    status
}

pub fn create_remote(repository: &str, visibility: RepositoryVisibility) -> Result<String, String> {
    create_remote_in(&root_dir(), repository, visibility)
}

pub fn create_remote_in(
    root: &Path,
    repository: &str,
    visibility: RepositoryVisibility,
) -> Result<String, String> {
    let repository = if repository.trim().is_empty() {
        "vmux-vault"
    } else {
        repository.trim()
    };
    if visibility == RepositoryVisibility::Public {
        validate_public_manifest(root)?;
    }
    initialize_in(root)?;
    if !git_optional(root, &["remote", "get-url", "origin"]).is_empty() {
        return Err("Vault already has an origin remote".to_string());
    }
    let root_arg = root.to_string_lossy().into_owned();
    let visibility = match visibility {
        RepositoryVisibility::Private => "--private",
        RepositoryVisibility::Public => "--public",
    };
    command_success(
        Command::new("gh")
            .current_dir(root)
            .args([
                "repo", "create", repository, visibility, "--source", &root_arg, "--remote",
                "origin", "--push",
            ])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )?;
    Ok(repository.to_string())
}

pub fn connect_remote(repository: &str) -> Result<String, String> {
    connect_remote_in(&root_dir(), repository)
}

pub fn connect_remote_in(root: &Path, repository: &str) -> Result<String, String> {
    let repository = repository.trim();
    if repository.is_empty() {
        return Err("repository is required".to_string());
    }
    initialize_in(root)?;
    let url = resolve_remote_url(repository)?;
    let previous_remote = git_optional(root, &["remote", "get-url", "origin"]);
    if !previous_remote.is_empty() {
        git(root, &["remote", "set-url", "origin", &url])?;
    } else {
        git(root, &["remote", "add", "origin", &url])?;
    }
    let result = (|| {
        git(root, &["fetch", "origin"])?;
        let _ = git(root, &["remote", "set-head", "origin", "--auto"]);
        match remote_branch(root) {
            Some(remote_branch) => {
                if git(root, &["merge-base", "HEAD", &remote_branch]).is_err() {
                    validate_remote_tree(root, &remote_branch)?;
                    if let Err(error) = git(root, &["rebase", "--onto", &remote_branch, "--root"]) {
                        let _ = git(root, &["rebase", "--abort"]);
                        return Err(format!(
                            "existing Vault conflicts with local files: {error}"
                        ));
                    }
                }
                let remote_branch_name = remote_branch
                    .strip_prefix("origin/")
                    .unwrap_or(&remote_branch);
                if current_branch(root)? != remote_branch_name {
                    git(root, &["branch", "--move", remote_branch_name])?;
                }
                git(root, &["branch", "--set-upstream-to", &remote_branch])?;
                sync_in(root)?;
            }
            None => {
                let branch = current_branch(root)?;
                git(root, &["push", "-u", "origin", &branch])?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        if previous_remote.is_empty() {
            let _ = git(root, &["remote", "remove", "origin"]);
        } else {
            let _ = git(root, &["remote", "set-url", "origin", &previous_remote]);
        }
        return Err(error);
    }
    Ok(url)
}

pub fn sync() -> Result<String, String> {
    sync_in(&root_dir())
}

pub fn sync_in(root: &Path) -> Result<String, String> {
    if !root.join(".git").is_dir() {
        return Err("Vault is not connected to Git".to_string());
    }
    if git_optional(root, &["remote", "get-url", "origin"]).is_empty() {
        return Err("Vault has no origin remote".to_string());
    }
    ensure_gitignore(root)?;
    commit_changes(root, "Sync vmux Vault")?;
    git(root, &["fetch", "origin"])?;
    let branch = current_branch(root)?;
    if let Some(remote_branch) = remote_branch(root) {
        if git(root, &["merge-base", "HEAD", &remote_branch]).is_err() {
            return Err("Vault remote has unrelated history".to_string());
        }
        if let Err(error) = git(root, &["rebase", &remote_branch]) {
            let _ = git(root, &["rebase", "--abort"]);
            return Err(format!("Vault has sync conflicts: {error}"));
        }
    }
    git(root, &["push", "-u", "origin", &branch])?;
    Ok("Vault synced".to_string())
}

pub fn initialize() -> Result<(), String> {
    initialize_in(&root_dir())
}

pub fn initialize_in(root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    ensure_gitignore(root)?;
    if !root.join(".git").is_dir() {
        git(root, &["init", "-b", "main"])?;
    }
    commit_changes(root, "Initialize vmux Vault")
}

fn ensure_gitignore(root: &Path) -> Result<(), String> {
    let path = root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut source = existing.trim_end().to_string();
    for entry in REQUIRED_IGNORES {
        if !existing.lines().any(|line| line.trim() == entry) {
            if !source.is_empty() {
                source.push('\n');
            }
            source.push_str(entry);
        }
    }
    source.push('\n');
    std::fs::write(path, source).map_err(|error| error.to_string())
}

fn commit_changes(root: &Path, message: &str) -> Result<(), String> {
    git(root, &["add", "--all"])?;
    if git_optional(root, &["status", "--porcelain"]).is_empty() {
        return Ok(());
    }
    git(root, &["commit", "-m", message])?;
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

fn validate_remote_tree(root: &Path, branch: &str) -> Result<(), String> {
    let entries = git(root, &["ls-tree", "--name-only", branch])?;
    let allowed = [
        ".gitignore",
        "README.md",
        "settings.ron",
        "knowledge",
        "tools",
        "locales",
        "dev",
    ];
    if entries
        .lines()
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .all(|entry| allowed.contains(&entry))
    {
        Ok(())
    } else {
        Err("selected repository does not look like a vmux Vault".to_string())
    }
}

fn resolve_remote_url(repository: &str) -> Result<String, String> {
    if repository.contains("://")
        || repository.starts_with("git@")
        || Path::new(repository).is_absolute()
    {
        return Ok(repository.to_string());
    }
    let output = Command::new("gh")
        .args([
            "repo", "view", repository, "--json", "sshUrl", "--jq", ".sshUrl",
        ])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    command_success(output)
}

fn github_identity_and_repositories() -> Result<(String, Vec<VaultRepository>), String> {
    let owner = command_success(
        Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )?;
    let source = command_success(
        Command::new("gh")
            .args([
                "repo",
                "list",
                "--limit",
                "100",
                "--json",
                "nameWithOwner,isPrivate,sshUrl,isEmpty",
            ])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?,
    )?;
    let mut repositories = serde_json::from_str::<Vec<GhRepository>>(&source)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|repository| VaultRepository {
            name: repository.name_with_owner,
            url: repository.ssh_url,
            private: repository.is_private,
            empty: repository.is_empty,
        })
        .collect::<Vec<_>>();
    repositories.sort_by(|left, right| {
        right
            .empty
            .cmp(&left.empty)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok((owner, repositories))
}

fn validate_public_manifest(root: &Path) -> Result<(), String> {
    let manifest = super::tools::load_manifest_from(&root.join("tools/tools.toml"))?;
    for (name, server) in manifest.mcp.servers {
        for key in server.env.keys() {
            let normalized = key.to_ascii_uppercase();
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY", "PRIVATE_KEY"]
                .iter()
                .any(|needle| normalized.contains(needle))
            {
                return Err(format!(
                    "MCP server {name} contains a literal credential in {key}; use an environment reference before creating a public Vault"
                ));
            }
        }
        if server.headers.keys().any(|key| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "authorization" | "cookie" | "x-api-key"
            )
        }) {
            return Err(format!(
                "MCP server {name} contains a literal credential header; use header_env before creating a public Vault"
            ));
        }
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args);
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

    fn configure_identity(root: &Path) {
        git(root, &["config", "user.name", "Vmux Test"]).unwrap();
        git(root, &["config", "user.email", "vmux@example.com"]).unwrap();
    }

    #[test]
    fn initialization_preserves_gitignore_and_commits_portable_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".gitignore"), "custom\n").unwrap();
        std::fs::write(root.path().join("settings.ron"), "()\n").unwrap();
        git(root.path(), &["init", "-b", "main"]).unwrap();
        configure_identity(root.path());

        initialize_in(root.path()).unwrap();

        let ignore = std::fs::read_to_string(root.path().join(".gitignore")).unwrap();
        assert!(ignore.lines().any(|line| line == "custom"));
        for entry in REQUIRED_IGNORES {
            assert!(ignore.lines().any(|line| line == entry));
        }
        assert_eq!(
            git(root.path(), &["rev-list", "--count", "HEAD"]).unwrap(),
            "1"
        );
    }

    #[test]
    fn empty_remote_receives_initial_and_followup_syncs() {
        let root = tempfile::tempdir().unwrap();
        let remote_parent = tempfile::tempdir().unwrap();
        let remote = remote_parent.path().join("vault.git");
        command_success(
            Command::new("git")
                .args(["init", "--bare", remote.to_string_lossy().as_ref()])
                .output()
                .unwrap(),
        )
        .unwrap();
        std::fs::write(root.path().join("settings.ron"), "()\n").unwrap();
        git(root.path(), &["init", "-b", "main"]).unwrap();
        configure_identity(root.path());

        connect_remote_in(root.path(), remote.to_string_lossy().as_ref()).unwrap();
        std::fs::write(root.path().join("settings.ron"), "(changed: true)\n").unwrap();
        sync_in(root.path()).unwrap();

        assert_eq!(
            git(root.path(), &["rev-list", "--count", "origin/main"]).unwrap(),
            "2"
        );
        assert!(
            git(root.path(), &["status", "--porcelain"])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn existing_vault_history_becomes_the_base_without_losing_local_files() {
        let root = tempfile::tempdir().unwrap();
        let seed = tempfile::tempdir().unwrap();
        let remote_parent = tempfile::tempdir().unwrap();
        let remote = remote_parent.path().join("vault.git");
        command_success(
            Command::new("git")
                .args(["init", "--bare", remote.to_string_lossy().as_ref()])
                .output()
                .unwrap(),
        )
        .unwrap();

        git(seed.path(), &["init", "-b", "main"]).unwrap();
        configure_identity(seed.path());
        std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
        git(seed.path(), &["add", "--all"]).unwrap();
        git(seed.path(), &["commit", "-m", "Remote Vault"]).unwrap();
        git(
            seed.path(),
            &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
        )
        .unwrap();
        git(seed.path(), &["push", "-u", "origin", "main"]).unwrap();

        std::fs::create_dir_all(root.path().join("knowledge")).unwrap();
        std::fs::write(root.path().join("knowledge/local.md"), "# Local\n").unwrap();
        git(root.path(), &["init", "-b", "main"]).unwrap();
        configure_identity(root.path());

        connect_remote_in(root.path(), remote.to_string_lossy().as_ref()).unwrap();

        let tree = git(
            root.path(),
            &["ls-tree", "-r", "--name-only", "origin/main"],
        )
        .unwrap();
        assert!(tree.lines().any(|entry| entry == "settings.ron"));
        assert!(tree.lines().any(|entry| entry == "knowledge/local.md"));
        assert_eq!(
            git(root.path(), &["rev-list", "--count", "origin/main"]).unwrap(),
            "2"
        );
    }

    #[test]
    fn existing_vault_uses_the_remote_default_branch() {
        let root = tempfile::tempdir().unwrap();
        let seed = tempfile::tempdir().unwrap();
        let remote_parent = tempfile::tempdir().unwrap();
        let remote = remote_parent.path().join("vault.git");
        command_success(
            Command::new("git")
                .args(["init", "--bare", remote.to_string_lossy().as_ref()])
                .output()
                .unwrap(),
        )
        .unwrap();

        git(seed.path(), &["init", "-b", "trunk"]).unwrap();
        configure_identity(seed.path());
        std::fs::write(seed.path().join("settings.ron"), "(remote: true)\n").unwrap();
        git(seed.path(), &["add", "--all"]).unwrap();
        git(seed.path(), &["commit", "-m", "Remote Vault"]).unwrap();
        git(
            seed.path(),
            &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
        )
        .unwrap();
        git(seed.path(), &["push", "-u", "origin", "trunk"]).unwrap();
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

        std::fs::create_dir_all(root.path().join("knowledge")).unwrap();
        std::fs::write(root.path().join("knowledge/local.md"), "# Local\n").unwrap();
        git(root.path(), &["init", "-b", "main"]).unwrap();
        configure_identity(root.path());

        connect_remote_in(root.path(), remote.to_string_lossy().as_ref()).unwrap();

        assert_eq!(current_branch(root.path()).unwrap(), "trunk");
        assert_eq!(
            git(root.path(), &["rev-list", "--count", "origin/trunk"]).unwrap(),
            "2"
        );
    }
}
