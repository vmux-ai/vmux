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
        let _ = write_private(&path, &body);
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

fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, contents)
}
