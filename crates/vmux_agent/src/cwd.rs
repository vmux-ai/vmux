use std::path::PathBuf;

pub fn default_space_dir() -> PathBuf {
    space_dir("default")
}

pub fn space_dir(space_id: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let dir = home.join(".vmux").join(space_id);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn valid_cwd(cwd: &str) -> Result<Option<std::path::PathBuf>, String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = std::path::PathBuf::from(trimmed);
    if !path.exists() {
        return Err(format!("cwd does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("cwd is not a directory: {}", path.display()));
    }
    Ok(Some(path))
}
