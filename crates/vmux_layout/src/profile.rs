use bevy::prelude::*;
use moonshine_save::prelude::*;
use std::path::PathBuf;

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Profile>();
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component)]
#[type_path = "vmux_desktop::profile"]
#[require(Save)]
pub struct Profile {
    pub name: String,
    pub color: [f32; 4],
    pub icon: Option<String>,
}

impl Profile {
    pub fn default_profile() -> Self {
        Self {
            name: "default".to_string(),
            color: [0.4, 0.6, 1.0, 1.0],
            icon: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Profile-scoped data paths.
//
// Layout under `~/Library/Application Support/Vmux/`:
//   settings.ron                   shared across profiles
//   services/                      shared (service socket, pid, log)
//   profiles/<name>/
//     session.ron                  per-profile tab/pane layout
//     Cookies, Local State, ...    per-profile CEF state
//
// For now there is exactly one profile (`"default"`). The helpers below are
// the single source of truth so a future profile picker only needs to
// change `active_profile_name()`.
// ---------------------------------------------------------------------------

/// Name of the currently active profile. Hardcoded to `"default"` until
/// profile switching is implemented.
pub fn active_profile_name() -> &'static str {
    "default"
}

/// Root data directory shared across all profiles.
pub fn shared_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home).join("Library/Application Support/Vmux")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir().join("Vmux")
    }
}

/// Directory for the active profile's per-profile data.
pub fn profile_dir() -> PathBuf {
    shared_data_dir()
        .join("profiles")
        .join(active_profile_name())
}

/// Per-profile session file (tab/pane layout).
pub fn session_path() -> PathBuf {
    profile_dir().join("session.ron")
}

/// Per-profile CEF cache root (cookies, localStorage, IndexedDB, etc.).
pub fn cef_cache_path() -> Option<String> {
    profile_dir().to_str().map(|s| s.to_owned())
}
