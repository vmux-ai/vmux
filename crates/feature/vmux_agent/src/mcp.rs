use std::path::{Path, PathBuf};

use vmux_core::ProcessId;
pub use vmux_core::agent::McpServerConfig;

const LONG_RUN_TIMEOUT_SECS: u64 = 600;
pub(crate) const LONG_MCP_TOOL_TIMEOUT_SECS: u64 = LONG_RUN_TIMEOUT_SECS + 60;

pub(crate) struct McpLaunchSpec {
    cwd: PathBuf,
    anchor: ProcessId,
    acp_session: bool,
    acp_terminals: bool,
    run_timeout_secs: u64,
    shell: String,
}

impl McpLaunchSpec {
    pub fn cli(cwd: &Path, anchor: ProcessId, shell: &str) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            anchor,
            acp_session: false,
            acp_terminals: false,
            run_timeout_secs: LONG_RUN_TIMEOUT_SECS,
            shell: shell.to_string(),
        }
    }

    pub fn acp(cwd: &Path, anchor: ProcessId, shell: &str) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            anchor,
            acp_session: true,
            acp_terminals: true,
            run_timeout_secs: LONG_RUN_TIMEOUT_SECS,
            shell: shell.to_string(),
        }
    }

    pub fn resolve(self) -> Result<McpServerConfig, String> {
        let sidecar = Self::sidecar_path()?;
        let profile = vmux_core::profile::Profile::current().into_id();
        self.resolve_with_sidecar(&sidecar, &profile)
    }

    fn resolve_with_sidecar(
        self,
        sidecar: &Path,
        profile: &str,
    ) -> Result<McpServerConfig, String> {
        if vmux_core::Executable::at(sidecar).is_some() {
            return Ok(McpServerConfig {
                command: sidecar.to_string_lossy().to_string(),
                args: self.args(profile),
                cwd: None,
            });
        }
        let workspace = self
            .workspace_dir()
            .ok_or_else(|| format!("vmux executable not found: {}", sidecar.display()))?;
        let mut args: Vec<String> = ["run", "--quiet", "-p", "vmux_cli", "--bin", "vmux", "--"]
            .into_iter()
            .map(str::to_string)
            .collect();
        args.extend(self.args(profile));
        Ok(McpServerConfig {
            command: "cargo".to_string(),
            args,
            cwd: Some(workspace),
        })
    }

    fn args(&self, profile: &str) -> Vec<String> {
        let mut args = vec![
            "mcp".to_string(),
            "--anchor".to_string(),
            self.anchor.to_string(),
            "--profile".to_string(),
            profile.to_string(),
            "--run-timeout-secs".to_string(),
            self.run_timeout_secs.to_string(),
        ];
        if !self.shell.trim().is_empty() {
            args.push("--shell".to_string());
            args.push(self.shell.clone());
        }
        if self.acp_session {
            args.push("--acp-session".to_string());
        }
        if self.acp_terminals {
            args.push("--acp-terminals".to_string());
        }
        args
    }

    fn workspace_dir(&self) -> Option<PathBuf> {
        let mut current = self.cwd.as_path();
        loop {
            if current.join("Cargo.toml").is_file() {
                return Some(current.to_path_buf());
            }
            current = current.parent()?;
        }
    }

    fn sidecar_path() -> Result<PathBuf, String> {
        let current = std::env::current_exe()
            .map_err(|error| format!("resolve current executable failed: {error}"))?;
        let Some(dir) = current.parent() else {
            return Err("current executable has no parent directory".to_string());
        };
        Ok(dir.join("vmux"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_args_always_append_profile() {
        let anchor = ProcessId::new();
        for profile in ["personal", "gregor"] {
            let args = McpLaunchSpec::cli(Path::new("/workspace"), anchor, "nu").args(profile);
            assert!(
                args.windows(2)
                    .any(|w| w[0] == "--profile" && w[1] == profile)
            );
        }
    }

    #[test]
    fn acp_args_append_acp_terminals_flag() {
        let anchor = ProcessId::new();
        let plain = McpLaunchSpec::cli(Path::new("/workspace"), anchor, "").args("personal");
        let acp = McpLaunchSpec::acp(Path::new("/workspace"), anchor, "").args("personal");
        assert!(!plain.iter().any(|a| a == "--acp-session"));
        assert!(acp.iter().any(|a| a == "--acp-session"));
        assert!(!plain.iter().any(|a| a == "--acp-terminals"));
        assert!(acp.iter().any(|a| a == "--acp-terminals"));
    }

    #[test]
    fn acp_uses_protocol_terminal_and_long_timeout() {
        let acp = McpLaunchSpec::acp(Path::new("/"), ProcessId::new(), "");
        assert!(acp.acp_terminals);
        assert_eq!(acp.run_timeout_secs, LONG_RUN_TIMEOUT_SECS);
    }

    #[test]
    fn cli_uses_long_tool_timeout() {
        assert_eq!(
            McpLaunchSpec::cli(Path::new("/"), ProcessId::new(), "").run_timeout_secs,
            LONG_RUN_TIMEOUT_SECS
        );
    }

    #[test]
    fn falls_back_to_cargo_run_when_sidecar_is_missing() {
        let temp = std::env::temp_dir().join(format!("vmux-agent-mcp-{}", std::process::id()));
        let workspace = temp.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

        let anchor = ProcessId::new();
        let config = McpLaunchSpec::cli(&workspace, anchor, "/bin/zsh")
            .resolve_with_sidecar(&temp.join("missing-vmux"), "personal")
            .unwrap();
        let _ = std::fs::remove_dir_all(&temp);

        assert_eq!(config.command, "cargo");
        assert_eq!(
            config.args,
            vec![
                "run",
                "--quiet",
                "-p",
                "vmux_cli",
                "--bin",
                "vmux",
                "--",
                "mcp",
                "--anchor",
                &anchor.to_string(),
                "--profile",
                "personal",
                "--run-timeout-secs",
                "600",
                "--shell",
                "/bin/zsh"
            ]
        );
        assert_eq!(config.cwd, Some(workspace));
    }

    #[test]
    fn resolve_appends_anchor_to_args() {
        let temp = std::env::temp_dir().join(format!("vmux-anchor-{}", std::process::id()));
        let workspace = temp.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

        let anchor = ProcessId::new();
        let config = McpLaunchSpec::cli(&workspace, anchor, "/bin/zsh")
            .resolve_with_sidecar(&temp.join("missing-vmux"), "personal")
            .unwrap();
        let _ = std::fs::remove_dir_all(&temp);

        assert!(config.args.windows(2).any(|w| w[0] == "--anchor"));
        assert!(config.args.iter().any(|a| a == &anchor.to_string()));
    }
}
