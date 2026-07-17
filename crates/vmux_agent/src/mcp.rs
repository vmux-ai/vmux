use std::path::{Path, PathBuf};

use vmux_core::ProcessId;
pub use vmux_core::agent::McpServerConfig;

use crate::exec;

pub fn resolve(cwd: &Path, anchor: ProcessId) -> Result<McpServerConfig, String> {
    resolve_inner(cwd, anchor, false, false)
}

/// Resolve the MCP sidecar for an ACP agent. Agents that use ACP client terminals hide the
/// overlapping vmux terminal tools; compatibility adapters keep them available.
pub fn resolve_acp(
    cwd: &Path,
    anchor: ProcessId,
    agent_id: &str,
) -> Result<McpServerConfig, String> {
    resolve_inner(cwd, anchor, true, acp_uses_native_terminals(agent_id))
}

fn acp_uses_native_terminals(agent_id: &str) -> bool {
    !matches!(agent_id, "claude" | "codex" | "mistral-vibe" | "vibe")
}

fn resolve_inner(
    cwd: &Path,
    anchor: ProcessId,
    acp_session: bool,
    acp_terminals: bool,
) -> Result<McpServerConfig, String> {
    let sidecar = vmux_sidecar_path()?;
    let profile = vmux_core::profile::active_profile_name();
    resolve_with_sidecar(&sidecar, cwd, anchor, &profile, acp_session, acp_terminals)
}

fn resolve_with_sidecar(
    sidecar: &Path,
    cwd: &Path,
    anchor: ProcessId,
    profile: &str,
    acp_session: bool,
    acp_terminals: bool,
) -> Result<McpServerConfig, String> {
    if exec::is_executable_path(sidecar) {
        return Ok(McpServerConfig {
            command: sidecar.to_string_lossy().to_string(),
            args: mcp_subcommand_args(anchor, profile, acp_session, acp_terminals),
            cwd: None,
        });
    }
    let workspace = find_workspace_dir(cwd)
        .ok_or_else(|| format!("vmux executable not found: {}", sidecar.display()))?;
    let mut args: Vec<String> = ["run", "--quiet", "-p", "vmux_cli", "--bin", "vmux", "--"]
        .into_iter()
        .map(str::to_string)
        .collect();
    args.extend(mcp_subcommand_args(
        anchor,
        profile,
        acp_session,
        acp_terminals,
    ));
    Ok(McpServerConfig {
        command: "cargo".to_string(),
        args,
        cwd: Some(workspace),
    })
}

fn mcp_subcommand_args(
    anchor: ProcessId,
    profile: &str,
    acp_session: bool,
    acp_terminals: bool,
) -> Vec<String> {
    let mut args = vec![
        "mcp".to_string(),
        "--anchor".to_string(),
        anchor.to_string(),
        "--profile".to_string(),
        profile.to_string(),
    ];
    if acp_session {
        args.push("--acp-session".to_string());
    }
    if acp_terminals {
        args.push("--acp-terminals".to_string());
    }
    args
}

fn find_workspace_dir(cwd: &Path) -> Option<PathBuf> {
    let mut current = cwd;
    loop {
        if current.join("Cargo.toml").is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn vmux_sidecar_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("resolve current executable failed: {error}"))?;
    let Some(dir) = current.parent() else {
        return Err("current executable has no parent directory".to_string());
    };
    Ok(dir.join("vmux"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_args_always_append_profile() {
        let anchor = ProcessId::new();
        for profile in ["personal", "gregor"] {
            let args = mcp_subcommand_args(anchor, profile, false, false);
            assert!(
                args.windows(2)
                    .any(|w| w[0] == "--profile" && w[1] == profile)
            );
        }
    }

    #[test]
    fn acp_args_append_acp_terminals_flag() {
        let anchor = ProcessId::new();
        let plain = mcp_subcommand_args(anchor, "personal", false, false);
        let acp = mcp_subcommand_args(anchor, "personal", true, true);
        assert!(!plain.iter().any(|a| a == "--acp-session"));
        assert!(acp.iter().any(|a| a == "--acp-session"));
        assert!(!plain.iter().any(|a| a == "--acp-terminals"));
        assert!(acp.iter().any(|a| a == "--acp-terminals"));
    }

    #[test]
    fn compatibility_acp_agents_keep_vmux_terminal_tools() {
        assert!(!acp_uses_native_terminals("codex"));
        assert!(!acp_uses_native_terminals("claude"));
        assert!(!acp_uses_native_terminals("mistral-vibe"));
        assert!(!acp_uses_native_terminals("vibe"));
        assert!(acp_uses_native_terminals("vibe-acp"));
    }

    #[test]
    fn falls_back_to_cargo_run_when_sidecar_is_missing() {
        let temp = std::env::temp_dir().join(format!("vmux-agent-mcp-{}", std::process::id()));
        let workspace = temp.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

        let anchor = ProcessId::new();
        let config = resolve_with_sidecar(
            &temp.join("missing-vmux"),
            &workspace,
            anchor,
            "personal",
            false,
            false,
        )
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
                "personal"
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
        let config = resolve_with_sidecar(
            &temp.join("missing-vmux"),
            &workspace,
            anchor,
            "personal",
            false,
            false,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&temp);

        assert!(config.args.windows(2).any(|w| w[0] == "--anchor"));
        assert!(config.args.iter().any(|a| a == &anchor.to_string()));
    }
}
