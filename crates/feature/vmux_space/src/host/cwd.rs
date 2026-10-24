use std::path::PathBuf;

use vmux_setting::StartupDir;

pub struct WorkspaceCwd(Option<PathBuf>);

impl WorkspaceCwd {
    pub fn into_path(self) -> Option<PathBuf> {
        self.0
    }

    pub fn from_tab(tab_cwd: Option<&str>) -> Result<Option<PathBuf>, String> {
        let Some(tab_cwd) = tab_cwd else {
            return Ok(None);
        };
        StartupDir::from_tab(tab_cwd).map(|dir| Some(dir.path))
    }

    pub fn resolve(tab_cwd: Option<&str>, launch_cwd: Option<&str>) -> Result<PathBuf, String> {
        if let Some(path) = Self::from_tab(tab_cwd)? {
            return Ok(path);
        }
        if let Some(cwd) = launch_cwd
            && let Ok(cwd) = Self::try_from(cwd)
        {
            return cwd
                .into_path()
                .ok_or_else(|| "agent project directory is missing".to_string());
        }
        Err("tab and agent project directories are missing".to_string())
    }
}

impl TryFrom<&str> for WorkspaceCwd {
    type Error = String;

    fn try_from(cwd: &str) -> Result<Self, Self::Error> {
        let trimmed = cwd.trim();
        if trimmed.is_empty() {
            return Ok(Self(None));
        }
        let path = PathBuf::from(trimmed);
        if !path.exists() {
            return Err(format!("cwd does not exist: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("cwd is not a directory: {}", path.display()));
        }
        Ok(Self(Some(path)))
    }
}
