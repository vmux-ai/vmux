use std::path::PathBuf;

pub use vmux_core::profile::{default_space_dir, space_dir};

pub fn valid_cwd(cwd: &str) -> Result<Option<PathBuf>, String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err(format!("cwd does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("cwd is not a directory: {}", path.display()));
    }
    Ok(Some(path))
}
