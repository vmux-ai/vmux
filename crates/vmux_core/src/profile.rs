use std::path::PathBuf;

pub fn active_profile_name() -> &'static str {
    "personal"
}

fn data_dir_suffix_for(profile: &str) -> PathBuf {
    match profile {
        "release" | "local" => PathBuf::from("Vmux"),
        other => PathBuf::from("Vmux").join(other),
    }
}

fn data_dir_suffix() -> PathBuf {
    data_dir_suffix_for(env!("VMUX_BUILD_PROFILE"))
}

pub fn shared_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(data_dir_suffix())
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::temp_dir().join(data_dir_suffix())
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

fn space_dir_path(home: &std::path::Path, space_id: &str) -> PathBuf {
    home.join(".vmux").join("spaces").join(space_id)
}

pub fn space_dir(space_id: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let dir = space_dir_path(&home, space_id);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn rename_space_dir(old_id: &str, new_id: &str) {
    if old_id == new_id {
        return;
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let old = space_dir_path(&home, old_id);
    let new = space_dir_path(&home, new_id);
    if old.exists() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_suffix_maps_each_profile() {
        assert_eq!(data_dir_suffix_for("release"), PathBuf::from("Vmux"));
        assert_eq!(data_dir_suffix_for("local"), PathBuf::from("Vmux"));
        assert_eq!(
            data_dir_suffix_for("dev"),
            PathBuf::from("Vmux").join("dev")
        );
        assert_eq!(
            data_dir_suffix_for("custom"),
            PathBuf::from("Vmux").join("custom"),
        );
    }

    #[test]
    fn local_and_release_share_one_space() {
        assert_eq!(data_dir_suffix_for("local"), data_dir_suffix_for("release"));
    }

    #[test]
    fn dev_lives_under_the_release_space() {
        let release = data_dir_suffix_for("release");
        let dev = data_dir_suffix_for("dev");
        assert!(dev.starts_with(&release));
        assert_ne!(dev, release);
        assert_eq!(dev.file_name().unwrap(), "dev");
    }

    #[test]
    fn shared_data_dir_ends_with_profile_suffix() {
        assert!(shared_data_dir().ends_with(data_dir_suffix()));
    }

    #[test]
    fn space_dir_is_under_vmux_spaces() {
        assert_eq!(
            space_dir_path(std::path::Path::new("/home/u"), "work"),
            PathBuf::from("/home/u/.vmux/spaces/work")
        );
    }
}
