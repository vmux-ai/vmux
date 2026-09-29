use std::path::PathBuf;

pub mod mcp_credentials;
pub mod safe_storage;
pub mod vault;

pub const fn build_profile() -> &'static str {
    env!("VMUX_BUILD_PROFILE")
}

pub const fn git_hash() -> &'static str {
    env!("VMUX_GIT_HASH")
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Profile {
    id: String,
}

pub fn is_test_session() -> bool {
    matches!(
        std::env::var("VMUX_TEST").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

impl Profile {
    pub fn current() -> Self {
        Self::named(&std::env::var("VMUX_PROFILE").unwrap_or_default())
    }

    pub fn named(raw: &str) -> Self {
        let cleaned: String = raw
            .trim()
            .to_ascii_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let id = cleaned.trim_matches('-');
        Self {
            id: if id.is_empty() {
                "personal".to_string()
            } else {
                id.to_string()
            },
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn into_id(self) -> String {
        self.id
    }

    pub fn display_name(&self) -> String {
        self.display_name_in(&ProfilePaths::current().shared_data(), is_test_session())
    }

    pub fn set_display_name(&self, name: &str) -> std::io::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "profile name cannot be empty",
            ));
        }
        let path = self.display_name_path(&ProfilePaths::current().shared_data());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, name)
    }

    pub fn exists(&self) -> bool {
        *self == Self::current()
            || self
                .display_name_path(&ProfilePaths::current().shared_data())
                .parent()
                .is_some_and(|path| path.is_dir())
    }

    pub fn all() -> Vec<Self> {
        Self::all_in(&ProfilePaths::current().shared_data(), Self::current())
    }

    pub fn create(name: &str) -> std::io::Result<Self> {
        Self::create_in(&ProfilePaths::current().shared_data(), name)
    }

    pub fn cef_keychain_switches(&self) -> &'static [&'static str] {
        cef_keychain_switches_for(is_test_session())
    }

    fn display_name_path(&self, data: &std::path::Path) -> PathBuf {
        data.join("profiles").join(&self.id).join("display_name")
    }

    fn display_name_in(&self, data: &std::path::Path, is_test: bool) -> String {
        let configured = std::fs::read_to_string(self.display_name_path(data)).ok();
        self.display_name_from(configured.as_deref(), is_test)
    }

    fn display_name_from(&self, configured: Option<&str>, is_test: bool) -> String {
        if !is_test && let Some(name) = configured {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
        let mut characters = self.id.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => "Personal".to_string(),
        }
    }

    fn all_in(data: &std::path::Path, active: Self) -> Vec<Self> {
        let root = data.join("profiles");
        let mut profiles = std::collections::BTreeSet::new();
        profiles.insert(active.id);
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    profiles.insert(Self::named(&entry.file_name().to_string_lossy()).id);
                }
            }
        }
        profiles.into_iter().map(|id| Self { id }).collect()
    }

    fn create_in(data: &std::path::Path, name: &str) -> std::io::Result<Self> {
        let name = name.trim();
        if name.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "profile name cannot be empty",
            ));
        }
        let base = Self::named(name).id;
        let root = data.join("profiles");
        let mut id = base.clone();
        for suffix in 2usize.. {
            if !root.join(&id).exists() {
                break;
            }
            id = format!("{base}-{suffix}");
        }
        let profile = Self { id };
        let directory = root.join(&profile.id);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join("display_name"), name)?;
        Ok(profile)
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.id)
    }
}

impl AsRef<str> for Profile {
    fn as_ref(&self) -> &str {
        &self.id
    }
}

impl From<Profile> for String {
    fn from(profile: Profile) -> Self {
        profile.id
    }
}

#[derive(Clone, Debug)]
pub struct ProfilePaths {
    build: &'static str,
    profile: Profile,
}

impl ProfilePaths {
    pub fn current() -> Self {
        Self {
            build: build_profile(),
            profile: Profile::current(),
        }
    }

    pub fn shared_data(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME").expect("HOME not set");
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(data_dir_suffix_for(self.build))
        }
        #[cfg(not(target_os = "macos"))]
        {
            std::env::temp_dir().join(data_dir_suffix_for(self.build))
        }
    }

    pub fn application_data(&self) -> PathBuf {
        let data = self.shared_data();
        match self.build {
            "release" | "local" => data,
            _ => data.parent().map(PathBuf::from).unwrap_or(data),
        }
    }

    pub fn config(&self) -> PathBuf {
        home_dir().join(".vmux")
    }

    pub fn projects(&self) -> PathBuf {
        self.config().join("projects")
    }

    pub fn recording(&self) -> PathBuf {
        recording_dir_for(&self.shared_data(), self.profile.id())
    }

    pub fn settings_candidates(&self) -> Vec<PathBuf> {
        settings_candidates_in(&self.config(), config_suffix())
    }

    pub fn settings(&self) -> PathBuf {
        let candidates = self.settings_candidates();
        candidates
            .iter()
            .find(|path| path.exists())
            .cloned()
            .unwrap_or_else(|| {
                candidates
                    .last()
                    .cloned()
                    .expect("settings candidates always include the shared path")
            })
    }

    pub fn profile(&self) -> PathBuf {
        self.shared_data().join("profiles").join(self.profile.id())
    }

    pub fn session(&self) -> PathBuf {
        self.profile().join("session.ron")
    }

    pub fn cef_cache(&self) -> Option<String> {
        cef_cache_path_in(
            &self.shared_data(),
            self.profile.id(),
            self.build,
            env!("VMUX_WORKTREE_ID"),
        )
        .to_str()
        .map(str::to_owned)
    }

    pub fn store(&self) -> PathBuf {
        let directory = store_dir_for(&self.shared_data(), self.profile.id());
        let _ = std::fs::create_dir_all(&directory);
        directory
    }

    pub fn agents(&self) -> PathBuf {
        self.managed("agents")
    }

    pub fn extensions(&self) -> PathBuf {
        self.managed("extensions")
    }

    pub fn lsp(&self) -> PathBuf {
        self.managed("lsp")
    }

    pub fn migrate_legacy_personal_layout(&self) {
        if is_test_session() {
            return;
        }
        let home = home_dir();
        let data = self.shared_data();
        let managed_data = self.application_data();
        migrate_legacy_personal_layout_in(&home, &data, &managed_data);
        prune_empty_legacy_space_dirs_in(&data);
    }

    fn managed(&self, name: &str) -> PathBuf {
        self.application_data().join(name)
    }
}

fn data_dir_suffix_for(profile: &str) -> PathBuf {
    match profile {
        "release" | "local" => PathBuf::from("Vmux"),
        other => PathBuf::from("Vmux").join(other),
    }
}

fn recording_dir_for(data: &std::path::Path, profile: &str) -> PathBuf {
    data.join("profiles").join(profile).join("recording")
}

fn config_suffix() -> Option<&'static str> {
    match build_profile() {
        "release" | "local" => None,
        other => Some(other),
    }
}

fn settings_candidates_in(base: &std::path::Path, suffix: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(suffix) = suffix {
        candidates.push(base.join(suffix).join("settings.ron"));
    }
    candidates.push(base.join("settings.ron"));
    candidates
}

fn cef_cache_path_in(
    data: &std::path::Path,
    profile: &str,
    build_profile: &str,
    worktree_id: &str,
) -> PathBuf {
    let profile_dir = data.join("profiles").join(profile);
    if build_profile == "release" {
        return profile_dir;
    }
    profile_dir
        .join("cef")
        .join(format!("{build_profile}-{worktree_id}"))
}

fn cef_keychain_switches_for(is_test_session: bool) -> &'static [&'static str] {
    if is_test_session {
        &["use-mock-keychain"]
    } else {
        &[]
    }
}

fn store_dir_for(base: &std::path::Path, _profile: &str) -> PathBuf {
    base.to_path_buf()
}

fn spaces_root_for(data: &std::path::Path, _profile: &str) -> PathBuf {
    data.join("spaces")
}

#[cfg(test)]
fn space_dir_path(data: &std::path::Path, profile: &str, space_id: &str) -> PathBuf {
    spaces_root_for(data, profile).join(space_id)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn is_empty_dir(path: &std::path::Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

fn collect_subdirs(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            collect_subdirs(&path, out);
            out.push(path);
        }
    }
}

fn prune_empty_legacy_space_dirs_in(data: &std::path::Path) {
    let root = spaces_root_for(data, "personal");
    if root
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return;
    }
    let mut dirs = Vec::new();
    collect_subdirs(&root, &mut dirs);
    for dir in dirs {
        if is_empty_dir(&dir) {
            let _ = std::fs::remove_dir(&dir);
        }
    }
    if is_empty_dir(&root) {
        let _ = std::fs::remove_dir(root);
    }
}

fn migrate_dir(legacy: &std::path::Path, target: &std::path::Path) {
    let Ok(legacy_metadata) = legacy.symlink_metadata() else {
        return;
    };
    if target.symlink_metadata().is_err() {
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(legacy, target);
        return;
    }
    if !legacy_metadata.file_type().is_dir()
        || !target
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(legacy) else {
        return;
    };
    for entry in entries.flatten() {
        migrate_dir(&entry.path(), &target.join(entry.file_name()));
    }
    let _ = std::fs::remove_dir(legacy);
}

fn migrate_legacy_personal_layout_in(
    home: &std::path::Path,
    data: &std::path::Path,
    managed_data: &std::path::Path,
) {
    let config = home.join(".vmux");
    migrate_dir(
        &config.join("profiles").join("personal").join("spaces"),
        &spaces_root_for(data, "personal"),
    );
    migrate_dir(&config.join("spaces"), &spaces_root_for(data, "personal"));
    migrate_dir(
        &config.join("recording"),
        &recording_dir_for(data, "personal"),
    );
    for name in ["agents", "extensions", "lsp"] {
        migrate_dir(&config.join(name), &managed_data.join(name));
    }
    if let Ok(profiles) = std::fs::read_dir(config.join("profiles")) {
        for profile in profiles.flatten() {
            if profile
                .file_type()
                .is_ok_and(|file_type| file_type.is_dir())
            {
                migrate_dir(
                    &profile.path(),
                    &data.join("profiles").join(profile.file_name()),
                );
            }
        }
    }
    let _ = std::fs::remove_dir(config.join("profiles"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_dir_is_nested_under_profile() {
        assert_eq!(
            recording_dir_for(std::path::Path::new("/data/Vmux"), "personal"),
            PathBuf::from("/data/Vmux/profiles/personal/recording")
        );
    }

    #[test]
    fn recording_dir_test_profile_is_nested() {
        assert_eq!(
            recording_dir_for(std::path::Path::new("/data/Vmux"), "test"),
            PathBuf::from("/data/Vmux/profiles/test/recording")
        );
    }

    #[test]
    fn sanitize_profile_keeps_safe_and_defaults_empty() {
        assert_eq!(Profile::named("test").id(), "test");
        assert_eq!(Profile::named("Test").id(), "test");
        assert_eq!(Profile::named("").id(), "personal");
        assert_eq!(Profile::named("  ").id(), "personal");
        assert_eq!(Profile::named("a/b").id(), "a-b");
        assert_eq!(Profile::named("../evil").id(), "evil");
    }

    #[test]
    fn profile_listing_includes_active_and_saved_profile_ids() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("profiles/work")).unwrap();
        std::fs::write(
            temp.path().join("profiles/work/display_name"),
            "Client Work",
        )
        .unwrap();

        let profiles = Profile::all_in(temp.path(), Profile::named("personal"));

        assert_eq!(
            profiles.iter().map(Profile::id).collect::<Vec<_>>(),
            ["personal", "work"]
        );
        assert_eq!(
            Profile::named("work").display_name_in(temp.path(), false),
            "Client Work"
        );
    }

    #[test]
    fn profile_creation_uses_a_unique_safe_id() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("profiles/client-work")).unwrap();

        let created = Profile::create_in(temp.path(), "Client Work").unwrap();

        assert_eq!(created.id(), "client-work-2");
        assert_eq!(
            std::fs::read_to_string(temp.path().join("profiles/client-work-2/display_name"))
                .unwrap(),
            "Client Work"
        );
    }

    #[test]
    fn store_dir_is_profile_agnostic_base() {
        let base = std::path::Path::new("/data/Vmux/dev");
        assert_eq!(
            store_dir_for(base, "personal"),
            PathBuf::from("/data/Vmux/dev")
        );
        assert_eq!(
            store_dir_for(base, "gregor"),
            PathBuf::from("/data/Vmux/dev")
        );
    }

    #[test]
    fn is_test_session_reads_env() {
        let prev = std::env::var("VMUX_TEST").ok();
        unsafe { std::env::set_var("VMUX_TEST", "1") };
        assert!(is_test_session());
        unsafe { std::env::remove_var("VMUX_TEST") };
        assert!(!is_test_session());
        if let Some(p) = prev {
            unsafe { std::env::set_var("VMUX_TEST", p) };
        }
    }

    #[test]
    fn display_name_uses_config_or_capitalized_id() {
        let personal = Profile::named("personal");
        assert_eq!(personal.display_name_from(None, false), "Personal");
        assert_eq!(
            personal.display_name_from(Some("Junichi"), false),
            "Junichi"
        );
        assert_eq!(
            personal.display_name_from(Some("Junichi"), true),
            "Personal"
        );
        assert_eq!(
            Profile::named("gregor").display_name_from(Some("  "), false),
            "Gregor"
        );
    }

    #[test]
    fn spaces_root_is_profile_agnostic() {
        let data = std::path::Path::new("/data/Vmux");
        assert_eq!(
            spaces_root_for(data, "personal"),
            PathBuf::from("/data/Vmux/spaces")
        );
        assert_eq!(
            spaces_root_for(data, "gregor"),
            PathBuf::from("/data/Vmux/spaces")
        );
    }

    #[test]
    fn active_profile_name_reads_and_sanitizes_env() {
        let prev = std::env::var("VMUX_PROFILE").ok();
        unsafe { std::env::set_var("VMUX_PROFILE", "Test/X") };
        assert_eq!(Profile::current().id(), "test-x");
        unsafe { std::env::remove_var("VMUX_PROFILE") };
        assert_eq!(Profile::current().id(), "personal");
        if let Some(p) = prev {
            unsafe { std::env::set_var("VMUX_PROFILE", p) };
        }
    }

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
    fn release_keeps_the_shared_cef_profile() {
        assert_eq!(
            cef_cache_path_in(
                std::path::Path::new("/data/Vmux"),
                "personal",
                "release",
                "worktree-a",
            ),
            PathBuf::from("/data/Vmux/profiles/personal")
        );
    }

    #[test]
    fn local_cef_profiles_are_isolated_by_worktree() {
        let data = std::path::Path::new("/data/Vmux");
        let first = cef_cache_path_in(data, "personal", "local", "worktree-a");
        let second = cef_cache_path_in(data, "personal", "local", "worktree-b");

        assert_eq!(
            first,
            PathBuf::from("/data/Vmux/profiles/personal/cef/local-worktree-a")
        );
        assert_ne!(first, second);
    }

    #[test]
    fn test_sessions_use_mock_keychain() {
        assert_eq!(
            cef_keychain_switches_for(true),
            ["use-mock-keychain"].as_slice()
        );
    }

    #[test]
    fn interactive_sessions_use_real_keychain() {
        assert!(cef_keychain_switches_for(false).is_empty());
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
        assert!(
            ProfilePaths::current()
                .shared_data()
                .ends_with(data_dir_suffix_for(build_profile()))
        );
    }

    #[test]
    fn managed_data_uses_build_agnostic_application_support_root() {
        let paths = ProfilePaths::current();
        let shared = paths.shared_data();
        let managed = paths.application_data();
        assert!(shared.starts_with(&managed));
        if matches!(build_profile(), "release" | "local") {
            assert_eq!(shared, managed);
        } else {
            assert_eq!(shared.parent(), Some(managed.as_path()));
        }
    }

    #[test]
    fn space_dir_is_under_vmux_spaces() {
        assert_eq!(
            space_dir_path(std::path::Path::new("/data/Vmux"), "personal", "work"),
            PathBuf::from("/data/Vmux/spaces/work")
        );
    }

    #[test]
    fn migrate_relocates_nested_spaces_and_recording() {
        let home = std::env::temp_dir().join(format!("vmux-migrate-{}", std::process::id()));
        let managed_data = home.join("data/Vmux");
        let data = managed_data.join("dev");
        let _ = std::fs::remove_dir_all(&home);
        let nested_space = home
            .join(".vmux")
            .join("profiles")
            .join("personal")
            .join("spaces")
            .join("space-1");
        std::fs::create_dir_all(&nested_space).unwrap();
        std::fs::write(nested_space.join("space.ron"), b"x").unwrap();
        let legacy_rec = home.join(".vmux").join("recording");
        std::fs::create_dir_all(&legacy_rec).unwrap();
        std::fs::write(legacy_rec.join("a.mp4"), b"y").unwrap();
        let agents = home.join(".vmux/agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(agents.join("registry.json"), b"{}").unwrap();
        let gregor = home.join(".vmux/profiles/gregor/recording");
        std::fs::create_dir_all(&gregor).unwrap();
        std::fs::write(gregor.join("b.mp4"), b"z").unwrap();

        migrate_legacy_personal_layout_in(&home, &data, &managed_data);

        assert!(
            space_dir_path(&data, "personal", "space-1")
                .join("space.ron")
                .exists()
        );
        assert!(
            !home
                .join(".vmux")
                .join("profiles")
                .join("personal")
                .join("spaces")
                .exists()
        );
        assert!(!legacy_rec.exists());
        assert!(recording_dir_for(&data, "personal").join("a.mp4").exists());
        assert!(managed_data.join("agents/registry.json").exists());
        assert!(data.join("profiles/gregor/recording/b.mp4").exists());
        assert!(!home.join(".vmux/profiles").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_keeps_existing_agnostic_spaces() {
        let home = std::env::temp_dir().join(format!("vmux-migrate-noop-{}", std::process::id()));
        let data = home.join("data/Vmux");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(
            home.join(".vmux")
                .join("profiles")
                .join("personal")
                .join("spaces"),
        )
        .unwrap();
        let target = spaces_root_for(&data, "personal");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), b"keep").unwrap();

        migrate_legacy_personal_layout_in(&home, &data, &data);

        assert!(target.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn cleanup_removes_empty_legacy_space_dirs_and_preserves_files() {
        let home = std::env::temp_dir().join(format!("vmux-prune-{}", std::process::id()));
        let data = home.join("data/Vmux");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(space_dir_path(&data, "personal", "org/empty")).unwrap();
        std::fs::create_dir_all(space_dir_path(&data, "personal", "solo")).unwrap();
        std::fs::create_dir_all(space_dir_path(&data, "personal", "keep")).unwrap();
        std::fs::write(
            space_dir_path(&data, "personal", "keep").join("f.txt"),
            b"x",
        )
        .unwrap();

        prune_empty_legacy_space_dirs_in(&data);

        assert!(!space_dir_path(&data, "personal", "org/empty").exists());
        assert!(!space_dir_path(&data, "personal", "org").exists());
        assert!(!space_dir_path(&data, "personal", "solo").exists());
        assert!(space_dir_path(&data, "personal", "keep").is_dir());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn settings_live_in_dot_vmux_not_data_dir() {
        let paths = ProfilePaths::current();
        for candidate in paths.settings_candidates() {
            assert!(candidate.starts_with(paths.config()));
            assert!(!candidate.starts_with(paths.shared_data()));
        }
    }

    #[test]
    fn settings_candidates_prefer_per_build_override_then_shared() {
        let base = PathBuf::from("/base");
        assert_eq!(
            settings_candidates_in(&base, None),
            vec![base.join("settings.ron")]
        );
        assert_eq!(
            settings_candidates_in(&base, Some("dev")),
            vec![
                base.join("dev").join("settings.ron"),
                base.join("settings.ron"),
            ]
        );
    }
}
