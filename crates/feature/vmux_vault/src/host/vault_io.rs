use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};

use super::VaultOperationOutput;

#[derive(Component, Clone)]
pub(super) struct RepoClaim {
    pub(super) root: PathBuf,
    pub(super) repository: PathBuf,
}

impl RepoClaim {
    pub(super) fn conflicts(&self, other: &Self) -> bool {
        self.root == other.root || self.repository == other.repository
    }
}

#[derive(Component, Clone)]
pub(super) struct VaultWake(
    pub(super) Option<bevy::winit::EventLoopProxy<bevy::winit::WinitUserEvent>>,
);

impl VaultWake {
    pub(super) fn from_proxy(proxy: Option<&bevy::winit::EventLoopProxyWrapper>) -> Self {
        Self(proxy.map(|wrapper| (**wrapper).clone()))
    }

    pub(super) fn notify(&self) {
        if let Some(wake) = &self.0 {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }

    pub(super) fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Task<T> {
        let wake = self.0.clone();
        IoTaskPool::get().spawn(async move {
            let result = work();
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        })
    }
}

#[derive(Component)]
pub(super) struct ActiveVaultOperation;

#[derive(Component)]
pub(super) struct VaultOperationOutcome(pub(super) Result<VaultOperationOutput, String>);

#[derive(Component, Clone, Debug)]
pub(super) struct ProcessOutcome {
    pub(super) success: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: Option<i32>,
}

impl ProcessOutcome {
    pub(super) fn from_output(output: Output) -> Self {
        Self {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            exit_code: output.status.code(),
        }
    }

    pub(super) fn text(self) -> Result<String, String> {
        if self.success {
            return Ok(self.stdout);
        }
        if !self.stderr.is_empty() {
            Err(self.stderr)
        } else if !self.stdout.is_empty() {
            Err(self.stdout)
        } else if let Some(code) = self.exit_code {
            Err(format!("command failed with exit status {code}"))
        } else {
            Err("command terminated by signal".to_string())
        }
    }

    pub(super) fn is_remote_change_refusal(&self) -> bool {
        if self.success {
            return false;
        }
        let combined = format!("{}\n{}", self.stderr, self.stdout).to_ascii_lowercase();
        combined.contains("non-fast-forward")
            || combined.contains("fetch first")
            || combined.contains("cannot lock ref")
    }
}

#[derive(Component)]
pub(super) struct GitProcessTask(pub(super) Task<Result<ProcessOutcome, String>>);

impl GitProcessTask {
    pub(super) fn spawn(wake: &VaultWake, directory: PathBuf, args: Vec<String>) -> Self {
        Self(wake.spawn(move || Self::run_in(&directory, &args)))
    }

    pub(super) fn spawn_global(wake: &VaultWake, args: Vec<String>) -> Self {
        Self(wake.spawn(move || Self::run_global(&args)))
    }

    pub(super) fn spawn_gh(
        wake: &VaultWake,
        directory: Option<PathBuf>,
        args: Vec<String>,
    ) -> Self {
        Self(wake.spawn(move || {
            let (executable, config_dir) =
                Self::find_gh().ok_or_else(|| "GitHub CLI is not installed".to_string())?;
            std::fs::create_dir_all(&config_dir)
                .map_err(|error| format!("failed to create GitHub config directory: {error}"))?;
            let mut command = Command::new(executable);
            Self::scrub_gh_env(&mut command);
            command
                .env("GH_CONFIG_DIR", config_dir)
                .env("GH_NO_UPDATE_NOTIFIER", "1");
            if let Some(directory) = directory {
                command.current_dir(directory);
            }
            let output = command
                .args(&args)
                .output()
                .map_err(|error| format!("failed to run gh: {error}"))?;
            Ok(ProcessOutcome::from_output(output))
        }))
    }

    fn run_in(directory: &Path, args: &[String]) -> Result<ProcessOutcome, String> {
        let mut command = Command::new("git");
        command
            .current_dir(directory)
            .env("GIT_TERMINAL_PROMPT", "0");
        if let Some((executable, config_dir)) = Self::find_gh() {
            let quoted = format!("'{}'", executable.to_string_lossy().replace('\'', "'\\''"));
            let helper =
                format!("credential.https://github.com.helper=!{quoted} auth git-credential");
            command
                .args(["-c", "credential.https://github.com.helper=", "-c", &helper])
                .env("GH_CONFIG_DIR", &config_dir);
            Self::scrub_gh_tokens(&mut command);
        }
        command.args(args);
        Self::scrub_git_env(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?;
        Ok(ProcessOutcome::from_output(output))
    }

    fn run_global(args: &[String]) -> Result<ProcessOutcome, String> {
        let mut command = Command::new("git");
        command.env("GIT_TERMINAL_PROMPT", "0").args(args);
        Self::scrub_git_env(&mut command);
        let output = command
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?;
        Ok(ProcessOutcome::from_output(output))
    }

    fn find_gh() -> Option<(PathBuf, PathBuf)> {
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
        let config_dir = vmux_profile::ProfilePaths::current()
            .application_data()
            .join("auth/github");
        Some((executable, config_dir))
    }

    fn scrub_gh_env(command: &mut Command) {
        command.env_remove("GH_CONFIG_DIR");
        Self::scrub_gh_tokens(command);
    }

    fn scrub_gh_tokens(command: &mut Command) {
        for variable in [
            "GH_ENTERPRISE_TOKEN",
            "GH_HOST",
            "GH_PROMPT_DISABLED",
            "GH_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GITHUB_TOKEN",
        ] {
            command.env_remove(variable);
        }
    }

    fn scrub_git_env(command: &mut Command) {
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
