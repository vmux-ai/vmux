use std::path::PathBuf;

use crate::Credentials;

pub struct StoredCredentials;

impl StoredCredentials {
    pub fn load() -> Option<Credentials> {
        let raw = std::fs::read_to_string(Self::path()?).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(credentials: &Credentials) {
        let (Some(path), Ok(body)) = (Self::path(), serde_json::to_string(credentials)) else {
            return;
        };
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return;
        }
        if vmux_path::AtomicFile::write(&path, body.as_bytes()).is_err() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }

    pub fn clear() {
        let Some(path) = Self::path() else {
            return;
        };
        let _ = std::fs::remove_file(path);
    }

    fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support/Vmux Remote")
                .join("pairing.json"),
        )
    }
}
