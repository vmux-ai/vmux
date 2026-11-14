use bevy_ecs::component::Component;
use std::path::PathBuf;

use crate::pairing::Credentials;

use std::os::unix::fs::PermissionsExt;

#[derive(Component)]
pub struct StoredCredentials(Option<PathBuf>);

impl Default for StoredCredentials {
    fn default() -> Self {
        Self(Self::path())
    }
}

impl StoredCredentials {
    pub fn load(&self) -> Option<Credentials> {
        let raw = std::fs::read_to_string(self.0.as_ref()?).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, credentials: &Credentials) {
        let (Some(path), Ok(body)) = (self.0.as_ref(), serde_json::to_string(credentials)) else {
            return;
        };
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return;
        }
        if vmux_path::AtomicFile::write(path, body.as_bytes()).is_err() {
            return;
        }
        #[cfg(unix)]
        {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }

    pub fn clear(&self) {
        let Some(path) = self.0.as_ref() else {
            return;
        };
        let _ = std::fs::remove_file(path);
    }

    fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(vmux_app::host::Product::remote_data_name())
                .join("pairing.json"),
        )
    }
}
