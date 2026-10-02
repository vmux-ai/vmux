use std::path::PathBuf;

pub struct WorkspaceCwd(Option<PathBuf>);

impl WorkspaceCwd {
    pub fn into_path(self) -> Option<PathBuf> {
        self.0
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
