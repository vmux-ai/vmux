use std::path::{Path, PathBuf};

pub use vmux_core::agent::McpServerConfig;

use crate::exec;

pub fn resolve(cwd: &Path) -> Result<McpServerConfig, String> {
    let sidecar = vmux_sidecar_path()?;
    resolve_with_sidecar(&sidecar, cwd)
}

fn resolve_with_sidecar(sidecar: &Path, cwd: &Path) -> Result<McpServerConfig, String> {
    if exec::is_executable_path(sidecar) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_cargo_run_when_sidecar_is_missing() {
        let temp = std::env::temp_dir().join(format!("vmux-agent-mcp-{}", std::process::id()));
        let workspace = temp.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[workspace]\n").unwrap();

        let config = resolve_with_sidecar(&temp.join("missing-vmux"), &workspace).unwrap();
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
}
