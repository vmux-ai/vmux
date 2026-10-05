use std::path::{Path, PathBuf};

pub use profile_driver::{ProfileMigration, ProfileRecord, ProfileStore};
pub use service::{RemotePaths, ServicePaths};

pub mod mcp_credentials;
mod profile_driver;
pub mod safe_storage;
mod service;

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

pub struct SessionEnvironment;

impl SessionEnvironment {
    pub fn is_test() -> bool {
        matches!(
            std::env::var("VMUX_TEST").ok().as_deref(),
            Some("1") | Some("true") | Some("yes")
        )
    }
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

    pub fn cef_keychain_switches(&self) -> &'static [&'static str] {
        cef_keychain_switches_for(SessionEnvironment::is_test())
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectsDirectory(PathBuf);

impl ProjectsDirectory {
    pub fn ensure() -> Result<Self, String> {
        Self::ensure_at(ProfilePaths::current().projects())
    }

    pub fn ensure_at(path: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create projects directory: {error}"))?;
        let path = path
            .canonicalize()
            .map_err(|error| format!("failed to resolve projects directory: {error}"))?;
        Ok(Self(path))
    }

    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.0)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn into_path(self) -> PathBuf {
        self.0
    }
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

fn recording_dir_for(data: &Path, profile: &str) -> PathBuf {
    data.join("profiles").join(profile).join("recording")
}

fn config_suffix() -> Option<&'static str> {
    match build_profile() {
        "release" | "local" => None,
        other => Some(other),
    }
}

fn settings_candidates_in(base: &Path, suffix: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(suffix) = suffix {
        candidates.push(base.join(suffix).join("settings.ron"));
    }
    candidates.push(base.join("settings.ron"));
    candidates
}

fn cef_cache_path_in(
    data: &Path,
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

fn store_dir_for(base: &Path, _profile: &str) -> PathBuf {
    base.to_path_buf()
}

#[cfg(test)]
fn spaces_root_for(data: &Path, _profile: &str) -> PathBuf {
    data.join("spaces")
}

#[cfg(test)]
fn space_dir_path(data: &Path, profile: &str, space_id: &str) -> PathBuf {
    spaces_root_for(data, profile).join(space_id)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_dir_is_nested_under_profile() {
        assert_eq!(
            recording_dir_for(Path::new("/data/Vmux"), "personal"),
            PathBuf::from("/data/Vmux/profiles/personal/recording")
        );
    }

    #[test]
    fn recording_dir_test_profile_is_nested() {
        assert_eq!(
            recording_dir_for(Path::new("/data/Vmux"), "test"),
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
    fn store_dir_is_profile_agnostic_base() {
        let base = Path::new("/data/Vmux/dev");
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
    fn session_environment_reads_env() {
        let prev = std::env::var("VMUX_TEST").ok();
        unsafe { std::env::set_var("VMUX_TEST", "1") };
        assert!(SessionEnvironment::is_test());
        unsafe { std::env::remove_var("VMUX_TEST") };
        assert!(!SessionEnvironment::is_test());
        if let Some(p) = prev {
            unsafe { std::env::set_var("VMUX_TEST", p) };
        }
    }

    #[test]
    fn spaces_root_is_profile_agnostic() {
        let data = Path::new("/data/Vmux");
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
            cef_cache_path_in(Path::new("/data/Vmux"), "personal", "release", "worktree-a",),
            PathBuf::from("/data/Vmux/profiles/personal")
        );
    }

    #[test]
    fn local_cef_profiles_are_isolated_by_worktree() {
        let data = Path::new("/data/Vmux");
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
            space_dir_path(Path::new("/data/Vmux"), "personal", "work"),
            PathBuf::from("/data/Vmux/spaces/work")
        );
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
