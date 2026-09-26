use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub(crate) struct ToolProcess(PathBuf);

impl ToolProcess {
    pub(crate) fn find(name: &str) -> Option<Self> {
        executable_paths()
            .into_iter()
            .map(|directory| directory.join(name))
            .find(|path| is_executable(path))
            .map(Self)
    }

    pub(crate) fn output(&self, args: &[&str], require_success: bool) -> Result<Output, String> {
        let output = Command::new(&self.0)
            .args(args)
            .env("PATH", executable_path())
            .output()
            .map_err(|error| error.to_string())?;
        if require_success && !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.is_empty() {
                return Err(format!(
                    "{} exited with {}",
                    self.0.display(),
                    output.status
                ));
            }
            return Err(detail);
        }
        Ok(output)
    }
}

fn executable_paths() -> Vec<PathBuf> {
    let mut paths = std::env::var_os("PATH")
        .as_deref()
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join(".local/bin"));
        paths.push(home.join(".cargo/bin"));
    }
    paths.push(PathBuf::from("/opt/homebrew/bin"));
    paths.push(PathBuf::from("/usr/local/bin"));
    paths
}

fn executable_path() -> OsString {
    std::env::join_paths(executable_paths())
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
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
