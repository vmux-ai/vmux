use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::Serialize;

use crate::agent::{AgentProvider, AgentProviders, PreparedAgentLaunch};

pub(crate) mod session;

pub(crate) const VIBE_NEW_ID: &str = "vibe_new";
pub(crate) const VIBE_NEW_STACK_ID: &str = "vibe_new_stack";

pub(crate) struct VibePlugin;

#[derive(Clone, Debug, PartialEq, Eq)]
struct McpServerConfig {
    command: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
}

#[derive(Serialize)]
struct VibeMcpServerEnv {
    name: &'static str,
    transport: &'static str,
    command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

impl Plugin for VibePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AgentProviders>()
            .init_resource::<session::VibeSessionToEntity>()
            .add_systems(
                Update,
                (
                    session::track_session_id_inserts,
                    session::track_session_id_removals,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                session::poll_pending_vibe_sessions.run_if(
                    bevy::time::common_conditions::on_timer(std::time::Duration::from_millis(200)),
                ),
            );
        let mut providers = app.world_mut().resource_mut::<AgentProviders>();
        providers.register(AgentProvider {
            id: VIBE_NEW_ID,
            name: "Vibe New",
            shortcut: "",
            executable: "vibe",
            available: vibe_available,
            prepare: prepare_launch,
        });
        providers.register(AgentProvider {
            id: VIBE_NEW_STACK_ID,
            name: "Vibe New Stack",
            shortcut: "",
            executable: "vibe",
            available: vibe_available,
            prepare: prepare_launch,
        });
    }
}

pub(crate) fn vibe_available() -> bool {
    find_executable("vibe").is_some()
}

fn prepare_launch(cwd: &Path) -> Result<PreparedAgentLaunch, String> {
    Ok(PreparedAgentLaunch {
        cwd: cwd.to_path_buf(),
        command: build_launch_command(cwd)?,
    })
}

fn build_launch_command(cwd: &Path) -> Result<String, String> {
    let vibe = find_executable("vibe").ok_or_else(|| "vibe executable not found".to_string())?;
    let mcp_servers = mcp_servers_env_value(cwd)?;
    build_bash_launch_command(&mcp_servers, &vibe, cwd)
}

pub(crate) fn find_executable(command: &str) -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .and_then(|path| path.into_string().ok())
        .and_then(|path| find_executable_in_path(command, &path));
    from_path.or_else(|| find_executable_in_fallback_dirs(command))
}

fn find_executable_in_path(command: &str, path_env: &str) -> Option<PathBuf> {
    path_env
        .split(':')
        .filter(|part| !part.is_empty())
        .map(|part| Path::new(part).join(command))
        .find(|path| is_executable(path))
}

fn find_executable_in_fallback_dirs(command: &str) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.into_iter()
        .map(|dir| dir.join(command))
        .find(|path| is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn mcp_servers_env_value(cwd: &Path) -> Result<String, String> {
    let config = resolve_mcp_server_config(cwd)?;
    let servers = [VibeMcpServerEnv {
        name: "vmux",
        transport: "stdio",
        command: config.command,
        args: config.args,
        cwd: config.cwd.map(|cwd| cwd.to_string_lossy().to_string()),
    }];
    serde_json::to_string(&servers)
        .map_err(|error| format!("serialize Vibe MCP config failed: {error}"))
}

fn resolve_mcp_server_config(cwd: &Path) -> Result<McpServerConfig, String> {
    let sidecar = vmux_sidecar_path()?;
    mcp_server_config_for(&sidecar, cwd)
}

fn mcp_server_config_for(sidecar: &Path, cwd: &Path) -> Result<McpServerConfig, String> {
    if is_executable(sidecar) {
        return Ok(McpServerConfig {
            command: sidecar.to_string_lossy().to_string(),
            args: vec!["mcp".to_string()],
            cwd: None,
        });
    }

    let workspace = find_workspace_dir(cwd)
        .ok_or_else(|| format!("vmux executable not found: {}", sidecar.display()))?;

    Ok(McpServerConfig {
        command: "cargo".to_string(),
        args: [
            "run", "--quiet", "-p", "vmux_cli", "--bin", "vmux", "--", "mcp",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        cwd: Some(workspace),
    })
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

fn shell_quote(value: &str) -> Result<String, String> {
    if value.contains('\n') || value.contains('\r') {
        return Err("cannot launch Vibe from a path containing a newline".to_string());
    }
    if !value.contains('\'') {
        return Ok(format!("'{value}'"));
    }
    if value.contains('`') {
        return Err(
            "cannot launch Vibe from a path containing both single quotes and backticks"
                .to_string(),
        );
    }
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
    ))
}

fn shell_quote_path(path: &Path) -> Result<String, String> {
    shell_quote(&path.to_string_lossy())
}

fn build_bash_launch_command(mcp_servers: &str, vibe: &Path, cwd: &Path) -> Result<String, String> {
    Ok(format!(
        "bash -lc {} bash {} {} {}",
        shell_quote("cd \"$1\" && VIBE_MCP_SERVERS=\"$2\" exec \"$3\" --trust")?,
        shell_quote_path(cwd)?,
        shell_quote(mcp_servers)?,
        shell_quote_path(vibe)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lookup_finds_executable_on_path() {
        let temp = std::env::temp_dir().join(format!("vmux-vibe-path-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let exe = temp.join("vibe");
        std::fs::write(&exe, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let found = find_executable_in_path("vibe", temp.to_string_lossy().as_ref());
        let _ = std::fs::remove_file(&exe);
        let _ = std::fs::remove_dir(&temp);

        assert_eq!(found, Some(exe));
    }

    #[test]
    fn command_lookup_finds_executable_in_home_local_bin_when_path_misses() {
        let _guard = crate::profile::ENV_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let old_home = std::env::var_os("HOME");
        let old_path = std::env::var_os("PATH");
        let temp = std::env::temp_dir().join(format!("vmux-vibe-home-{}", std::process::id()));
        let bin = temp.join(".local/bin");
        let exe = bin.join("vibe");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(&exe, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        unsafe {
            std::env::set_var("HOME", &temp);
            std::env::set_var("PATH", "/usr/bin:/bin");
        }
        let found = find_executable("vibe");
        unsafe {
            match old_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
            match old_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
        let _ = std::fs::remove_dir_all(&temp);

        assert_eq!(found, Some(exe));
    }

    #[test]
    fn launch_command_cds_and_passes_mcp_servers_to_vibe() {
        let command = build_bash_launch_command(
            r#"[{"name":"vmux","transport":"stdio","command":"target/debug/vmux","args":["mcp"]}]"#,
            Path::new("/Users/test/.local/bin/vibe"),
            Path::new("/tmp/work tree"),
        )
        .unwrap();

        assert_eq!(
            command,
            "bash -lc 'cd \"$1\" && VIBE_MCP_SERVERS=\"$2\" exec \"$3\" --trust' bash '/tmp/work tree' '[{\"name\":\"vmux\",\"transport\":\"stdio\",\"command\":\"target/debug/vmux\",\"args\":[\"mcp\"]}]' '/Users/test/.local/bin/vibe'"
        );
    }

    #[test]
    fn mcp_config_falls_back_to_cargo_run_when_sidecar_is_missing() {
        let temp = std::env::temp_dir().join(format!("vmux-vibe-cargo-{}", std::process::id()));
        let workspace = temp.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

        let config = mcp_server_config_for(&temp.join("missing-vmux"), &workspace).unwrap();

        let _ = std::fs::remove_dir_all(&temp);

        assert_eq!(config.command, "cargo");
        assert_eq!(
            config.args,
            vec![
                "run", "--quiet", "-p", "vmux_cli", "--bin", "vmux", "--", "mcp"
            ]
        );
        assert_eq!(config.cwd, Some(workspace));
    }

    #[test]
    fn vibe_plugin_registers_agent_provider_commands() {
        let mut app = bevy::prelude::App::new();
        app.add_plugins(crate::agent::AgentPlugin);
        app.add_plugins(VibePlugin);

        let providers = app.world().resource::<crate::agent::AgentProviders>();
        assert!(providers.contains(VIBE_NEW_ID));
        assert!(providers.contains(VIBE_NEW_STACK_ID));
        assert_eq!(providers.get(VIBE_NEW_ID).unwrap().name, "Vibe New");
        assert_eq!(
            providers.get(VIBE_NEW_STACK_ID).unwrap().name,
            "Vibe New Stack"
        );
    }
}
