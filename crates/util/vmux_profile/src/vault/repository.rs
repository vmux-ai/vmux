use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::recovery::{RECOVERY_DIR, RECOVERY_FILE, RecoveryEnvelope};
use super::snapshot::{
    FORMAT_VERSION, INDEX_FILE, MANIFEST_FILE, MANIFEST_VERSION, OBJECTS_DIR, RemoteManifest,
};

pub(super) struct VaultRepositoryPath {
    root: PathBuf,
}

pub(super) struct GitRepository {
    root: PathBuf,
}

pub(super) struct GitHubCli {
    executable: PathBuf,
    config_dir: PathBuf,
}

pub(super) trait OutputText {
    fn success_text(self) -> Result<String, String>;
}

impl VaultRepositoryPath {
    pub(super) fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub(super) fn git(&self) -> GitRepository {
        GitRepository::at(&self.root)
    }

    pub(super) fn path(&self) -> &Path {
        &self.root
    }

    pub(super) fn state_path(&self) -> PathBuf {
        self.root.join(".git").join("vmux-state.ron")
    }

    pub(super) fn ensure(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        if !self.root.join(".git").is_dir() {
            self.git().run(&["init", "-b", "main"])?;
        }
        Ok(())
    }

    pub(super) fn validate_empty(&self) -> Result<(), String> {
        if self.git().run(&["rev-parse", "--verify", "HEAD"]).is_ok() {
            return Err("Vault staging repository contains unsupported history".to_string());
        }
        let unexpected = std::fs::read_dir(&self.root)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .any(|name| name != ".git");
        if unexpected {
            return Err("Vault staging repository contains unencrypted files".to_string());
        }
        Ok(())
    }

    pub(super) fn manifest(&self) -> Result<RemoteManifest, String> {
        let source = std::fs::read(self.root.join(MANIFEST_FILE))
            .map_err(|error| format!("failed to read encrypted Vault manifest: {error}"))?;
        RemoteManifest::try_from(source.as_slice())
    }

    pub(super) fn write_manifest(&self, manifest: &RemoteManifest) -> Result<(), String> {
        let source = ron::ser::to_string_pretty(manifest, ron::ser::PrettyConfig::new())
            .map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(
            self.root.join(MANIFEST_FILE),
            format!("{source}\n").as_bytes(),
        )
        .map_err(|error| error.to_string())
    }

    pub(super) fn manifest_at(&self, branch: &str) -> Result<RemoteManifest, String> {
        let spec = format!("{branch}:{MANIFEST_FILE}");
        let source = self.git().run(&["show", &spec])?;
        RemoteManifest::try_from(source.as_bytes())
    }

    pub(super) fn validate_remote_history(&self, branch: &str) -> Result<(), String> {
        let entries = self
            .git()
            .run(&["log", branch, "--name-only", "--pretty=format:"])?;
        for entry in entries.lines().map(str::trim) {
            if entry.is_empty() || Self::is_vault_path(entry) {
                continue;
            }
            return Err("selected repository contains plaintext or non-Vault history".to_string());
        }
        self.manifest_at(branch)?;
        Ok(())
    }

    pub(super) fn remote_has_key_recipients(&self, branch: &str) -> Result<bool, String> {
        let keys = self
            .git()
            .run(&["ls-tree", "-r", "--name-only", branch, "keys"])?;
        Ok(!keys.is_empty())
    }

    pub(super) fn validate_encrypted_worktree(&self) -> Result<(), String> {
        for entry in std::fs::read_dir(&self.root).map_err(|error| error.to_string())? {
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
            std::fs::read_dir(self.root.join(OBJECTS_DIR)).map_err(|error| error.to_string())?
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
        let keys = self.root.join("keys");
        if !keys.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(&keys).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
                return Err("invalid encrypted Vault key recipients".to_string());
            }
            if entry.file_name() != "recovery" {
                return Err("invalid encrypted Vault key recipients".to_string());
            }
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
            let _ = RecoveryEnvelope::read(&self.root)?;
        }
        Ok(())
    }

    fn is_vault_path(path: &str) -> bool {
        if path == MANIFEST_FILE
            || path == INDEX_FILE
            || path == format!("{RECOVERY_DIR}/{RECOVERY_FILE}")
        {
            return true;
        }
        let Some(name) = path.strip_prefix(&format!("{OBJECTS_DIR}/")) else {
            return false;
        };
        name.len() == 64 && !name.contains('/')
    }
}

impl GitRepository {
    pub(super) fn at(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub(super) fn run(&self, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0");
        if let Some(cli) = GitHubCli::find() {
            cli.configure_git(&mut command);
        }
        command.args(args);
        Self::remove_inherited_environment(&mut command);
        command
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?
            .success_text()
    }

    pub(super) fn optional(&self, args: &[&str]) -> String {
        self.run(args).unwrap_or_default()
    }

    pub(super) fn commit(&self, message: &str) -> Result<(), String> {
        self.run(&["add", "--all"])?;
        if self.optional(&["status", "--porcelain"]).is_empty() {
            return Ok(());
        }
        self.run(&["-c", "commit.gpgSign=false", "commit", "-m", message])?;
        Ok(())
    }

    pub(super) fn current_branch(&self) -> Result<String, String> {
        let branch = self.run(&["branch", "--show-current"])?;
        if branch.is_empty() {
            return Err("Vault has no current branch".to_string());
        }
        Ok(branch)
    }

    pub(super) fn remote_branch(&self) -> Option<String> {
        let symbolic = self.optional(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]);
        if symbolic.starts_with("origin/")
            && self.run(&["rev-parse", "--verify", &symbolic]).is_ok()
        {
            return Some(symbolic);
        }
        for branch in ["origin/main", "origin/master"] {
            if self.run(&["rev-parse", "--verify", branch]).is_ok() {
                return Some(branch.to_string());
            }
        }
        let branches = self
            .optional(&[
                "for-each-ref",
                "--format=%(refname:short)",
                "refs/remotes/origin",
            ])
            .lines()
            .filter(|branch| *branch != "origin/HEAD")
            .map(str::to_string)
            .collect::<Vec<_>>();
        if branches.len() != 1 {
            return None;
        }
        branches.into_iter().next()
    }

    fn remove_inherited_environment(command: &mut Command) {
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
    }
}

impl GitHubCli {
    pub(super) fn command() -> Result<Command, String> {
        let cli = Self::find().ok_or_else(|| "GitHub CLI is not installed".to_string())?;
        std::fs::create_dir_all(&cli.config_dir)
            .map_err(|error| format!("failed to create GitHub config directory: {error}"))?;
        let mut command = Command::new(cli.executable);
        Self::remove_inherited_environment(&mut command);
        command
            .env("GH_CONFIG_DIR", cli.config_dir)
            .env("GH_NO_UPDATE_NOTIFIER", "1");
        Ok(command)
    }

    fn find() -> Option<Self> {
        let executable = std::env::var_os("PATH")
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
            })?;
        Some(Self {
            executable,
            config_dir: crate::ProfilePaths::current()
                .application_data()
                .join("auth/github"),
        })
    }

    fn configure_git(&self, command: &mut Command) {
        let executable = self.executable.to_string_lossy();
        let credential_helper = format!(
            "credential.https://github.com.helper=!{} auth git-credential",
            Self::shell_quote(&executable)
        );
        command
            .args([
                "-c",
                "credential.https://github.com.helper=",
                "-c",
                &credential_helper,
            ])
            .env("GH_CONFIG_DIR", &self.config_dir);
        for variable in Self::environment_variables() {
            if variable != "GH_CONFIG_DIR" {
                command.env_remove(variable);
            }
        }
    }

    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    fn remove_inherited_environment(command: &mut Command) {
        for variable in Self::environment_variables() {
            command.env_remove(variable);
        }
    }

    fn environment_variables() -> [&'static str; 7] {
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
}

impl TryFrom<&[u8]> for RemoteManifest {
    type Error = String;

    fn try_from(source: &[u8]) -> Result<Self, Self::Error> {
        let source = std::str::from_utf8(source).map_err(|error| {
            format!("selected repository is not an encrypted vmux Vault: {error}")
        })?;
        let manifest = ron::from_str::<Self>(source).map_err(|error| {
            format!("selected repository is not an encrypted vmux Vault: {error}")
        })?;
        if !(FORMAT_VERSION..=MANIFEST_VERSION).contains(&manifest.version)
            || manifest.cipher != "AES-256-GCM"
            || manifest.index != INDEX_FILE
            || manifest.vault_id.is_empty()
        {
            return Err(
                "selected repository uses an unsupported Vault encryption format".to_string(),
            );
        }
        Ok(manifest)
    }
}

impl OutputText for Output {
    fn success_text(self) -> Result<String, String> {
        let stdout = String::from_utf8_lossy(&self.stdout).trim().to_string();
        if self.status.success() {
            return Ok(stdout);
        }
        let stderr = String::from_utf8_lossy(&self.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(stdout)
        } else {
            Err(stderr)
        }
    }
}
