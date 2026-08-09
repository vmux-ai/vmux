use std::path::{Path, PathBuf};

use vmux_core::ProcessId;
pub use vmux_core::agent::McpServerConfig;

use crate::{AgentKind, exec};

const DEFAULT_RUN_TIMEOUT_SECS: u64 = 50;
pub(crate) const LONG_RUN_TIMEOUT_SECS: u64 = 600;
pub(crate) const LONG_MCP_TOOL_TIMEOUT_SECS: u64 = LONG_RUN_TIMEOUT_SECS + 60;

pub fn resolve(cwd: &Path, anchor: ProcessId, kind: AgentKind) -> Result<McpServerConfig, String> {
    resolve_inner(cwd, anchor, false, false, run_timeout_secs_for_kind(kind))
}

/// Resolve the MCP sidecar for an ACP agent. Agents that use ACP client terminals hide the
/// overlapping vmux terminal tools; compatibility adapters keep them available.
pub fn resolve_acp(
    cwd: &Path,
    anchor: ProcessId,
    agent_id: &str,
) -> Result<McpServerConfig, String> {
    resolve_inner(
        cwd,
        anchor,
        true,
        acp_uses_native_terminals(agent_id),
        run_timeout_secs_for_agent_id(agent_id),
    )
}

fn run_timeout_secs_for_kind(kind: AgentKind) -> u64 {
    match kind {
        AgentKind::Vibe => DEFAULT_RUN_TIMEOUT_SECS,
        AgentKind::Claude | AgentKind::Codex => LONG_RUN_TIMEOUT_SECS,
    }
}

fn run_timeout_secs_for_agent_id(agent_id: &str) -> u64 {
    match crate::acp_install::registry_id_alias(agent_id) {
        "claude-acp" | "codex-acp" => LONG_RUN_TIMEOUT_SECS,
        _ => DEFAULT_RUN_TIMEOUT_SECS,
    }
}

fn acp_uses_native_terminals(agent_id: &str) -> bool {
    !matches!(
        crate::acp_install::registry_id_alias(agent_id),
        "claude-acp" | "codex-acp" | "mistral-vibe"
    )
}

fn resolve_inner(
    cwd: &Path,
    anchor: ProcessId,
    acp_session: bool,
    acp_terminals: bool,
    run_timeout_secs: u64,
) -> Result<McpServerConfig, String> {
    let sidecar = vmux_sidecar_path()?;
    let profile = vmux_core::profile::active_profile_name();
    resolve_with_sidecar(
        &sidecar,
        cwd,
        anchor,
        &profile,
        acp_session,
        acp_terminals,
        run_timeout_secs,
    )
}

fn resolve_with_sidecar(
    sidecar: &Path,
    cwd: &Path,
    anchor: ProcessId,
    profile: &str,
    acp_session: bool,
    acp_terminals: bool,
    run_timeout_secs: u64,
) -> Result<McpServerConfig, String> {
    if exec::is_executable_path(sidecar) {
        return Ok(McpServerConfig {
            command: sidecar.to_string_lossy().to_string(),
            args: mcp_subcommand_args(
                anchor,
                profile,
                acp_session,
                acp_terminals,
                run_timeout_secs,
            ),
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
        run_timeout_secs,
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
    run_timeout_secs: u64,
) -> Vec<String> {
    let mut args = vec![
        "mcp".to_string(),
        "--anchor".to_string(),
        anchor.to_string(),
        "--profile".to_string(),
        profile.to_string(),
        "--run-timeout-secs".to_string(),
        run_timeout_secs.to_string(),
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
#[path = "mcp.test.rs"]
mod tests;
