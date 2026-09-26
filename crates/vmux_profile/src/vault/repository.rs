use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::recovery::{RECOVERY_DIR, RECOVERY_FILE, read_recovery_envelope};
use super::snapshot::{
    FORMAT_VERSION, INDEX_FILE, MANIFEST_FILE, MANIFEST_VERSION, OBJECTS_DIR, RemoteManifest,
};

pub(super) fn validate_empty_vault_repository(repository: &Path) -> Result<(), String> {
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

pub(super) fn ensure_repository(repository: &Path) -> Result<(), String> {
    std::fs::create_dir_all(repository).map_err(|error| error.to_string())?;
    if !repository.join(".git").is_dir() {
        git(repository, &["init", "-b", "main"])?;
    }
    Ok(())
}

pub(super) fn read_manifest(repository: &Path) -> Result<RemoteManifest, String> {
    let source = std::fs::read(repository.join(MANIFEST_FILE))
        .map_err(|error| format!("failed to read encrypted Vault manifest: {error}"))?;
    parse_manifest(&source)
}

pub(super) fn write_manifest(repository: &Path, manifest: &RemoteManifest) -> Result<(), String> {
    let source = ron::ser::to_string_pretty(manifest, ron::ser::PrettyConfig::new())
        .map_err(|error| error.to_string())?;
    vmux_path::AtomicFile::write(
        repository.join(MANIFEST_FILE),
        format!("{source}\n").as_bytes(),
    )
    .map_err(|error| error.to_string())
}

pub(super) fn manifest_from_ref(repository: &Path, branch: &str) -> Result<RemoteManifest, String> {
    let spec = format!("{branch}:{MANIFEST_FILE}");
    let source = git(repository, &["show", &spec])?;
    parse_manifest(source.as_bytes())
}

pub(super) fn parse_manifest(source: &[u8]) -> Result<RemoteManifest, String> {
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

pub(super) fn validate_remote_history(repository: &Path, branch: &str) -> Result<(), String> {
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
                || valid_recovery_path(entry)
        });
    if !valid {
        return Err("selected repository contains plaintext or non-Vault history".to_string());
    }
    let _ = manifest_from_ref(repository, branch)?;
    Ok(())
}

pub(super) fn remote_has_key_recipients(repository: &Path, branch: &str) -> Result<bool, String> {
    Ok(!git(
        repository,
        &["ls-tree", "-r", "--name-only", branch, "keys"],
    )?
    .is_empty())
}

pub(super) fn validate_encrypted_worktree(repository: &Path) -> Result<(), String> {
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

pub(super) fn valid_recovery_path(path: &str) -> bool {
    path == format!("{RECOVERY_DIR}/{RECOVERY_FILE}")
}

pub(super) fn commit_changes(root: &Path, message: &str) -> Result<(), String> {
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

pub(super) fn current_branch(root: &Path) -> Result<String, String> {
    let branch = git(root, &["branch", "--show-current"])?;
    if branch.is_empty() {
        Err("Vault has no current branch".to_string())
    } else {
        Ok(branch)
    }
}

pub(super) fn remote_branch(root: &Path) -> Option<String> {
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

pub(super) fn gh_command() -> Result<Command, String> {
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

pub(super) fn github_cli() -> Option<PathBuf> {
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

pub(super) fn github_config_dir() -> PathBuf {
    crate::application_data_dir().join("auth/github")
}

pub(super) fn github_environment_variables() -> [&'static str; 7] {
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

pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn git(root: &Path, args: &[&str]) -> Result<String, String> {
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

pub(super) fn git_optional(root: &Path, args: &[&str]) -> String {
    git(root, args).unwrap_or_default()
}

pub(super) fn command_success(output: Output) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}
