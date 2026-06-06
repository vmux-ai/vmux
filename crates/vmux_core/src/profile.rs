use std::path::PathBuf;

pub fn active_profile_name() -> &'static str {
    "personal"
}

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

pub fn profile_dir() -> PathBuf {
    shared_data_dir()
        .join("profiles")
        .join(active_profile_name())
}

pub fn session_path() -> PathBuf {
    profile_dir().join("session.ron")
}

pub fn cef_cache_path() -> Option<String> {
    profile_dir().to_str().map(|s| s.to_owned())
}

pub fn default_space_dir() -> PathBuf {
    space_dir("space-1")
}

pub fn space_dir(space_id: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let dir = home.join(".vmux").join(space_id);
    let _ = std::fs::create_dir_all(&dir);
    dir
}
