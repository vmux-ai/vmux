use std::path::{Path, PathBuf};

use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Executable(PathBuf);

impl Executable {
    pub fn at(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();
        Self::is_executable(&path).then_some(Self(path))
    }

    pub fn find(command: &str) -> Option<Self> {
        let from_path = std::env::var_os("PATH")
            .and_then(|path| path.into_string().ok())
            .and_then(|path| Self::find_in_path(command, &path));
        from_path
            .or_else(|| Self::find_in_fallback_dirs(command))
            .and_then(Self::at)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path(self) -> PathBuf {
        self.0
    }

    fn find_in_path(command: &str, path_env: &str) -> Option<PathBuf> {
        path_env
            .split(':')
            .filter(|part| !part.is_empty())
            .map(|part| Path::new(part).join(command))
            .find(|path| Self::is_executable(path))
    }

    fn find_in_fallback_dirs(command: &str) -> Option<PathBuf> {
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
            .find(|path| Self::is_executable(path))
    }

    #[cfg(unix)]
    fn is_executable(path: &Path) -> bool {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn finds_an_executable_on_path() {
        let temp = std::env::temp_dir().join(format!("vmux-executable-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let path = temp.join("fake-cli");
        std::fs::write(&path, b"").unwrap();
        #[cfg(unix)]
        {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let found = Executable::find_in_path("fake-cli", temp.to_string_lossy().as_ref());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&temp);
        assert_eq!(found, Some(path));
    }
}
