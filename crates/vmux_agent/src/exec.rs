use std::path::{Path, PathBuf};

pub fn is_executable_path(path: &Path) -> bool {
    is_executable(path)
}

pub fn find_executable(command: &str) -> Option<PathBuf> {
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

#[cfg(test)]
#[path = "exec.test.rs"]
mod tests;
