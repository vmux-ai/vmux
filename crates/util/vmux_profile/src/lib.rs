use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
pub use credentials::CredentialStore;
pub use lease::DataLease;
pub use profile_driver::{ProfileMigration, ProfileRecord, ProfileStore};
pub use service::{RemotePaths, ServicePaths};

#[cfg(target_os = "macos")]
mod credentials;
mod lease;
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
    pub fn requires_inherited_service_environment() -> bool {
        Self::is_test()
            || Profile::current().id() != "personal"
            || std::env::var_os("VMUX_DATA_DIR").is_some_and(|path| !path.is_empty())
    }

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
    data: PathBuf,
    test: bool,
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
        #[cfg(target_os = "macos")]
        let base = home_dir().join("Library/Application Support");
        #[cfg(not(target_os = "macos"))]
        let base = std::env::temp_dir();
        let test = SessionEnvironment::is_test();
        let mut data = base.join("Vmux");
        if let Some(override_dir) = std::env::var_os("VMUX_DATA_DIR")
            && !override_dir.is_empty()
        {
            data = PathBuf::from(override_dir);
            assert!(data.is_absolute(), "VMUX_DATA_DIR must be an absolute path");
        }
        if test {
            data = data.join("test");
        }
        if data.is_relative() {
            data = std::env::current_dir()
                .expect("current directory unavailable")
                .join(data);
        }
        Self {
            build: build_profile(),
            profile: Profile::current(),
            data,
            test: test || std::env::var_os("VMUX_DATA_DIR").is_some_and(|path| !path.is_empty()),
        }
    }

    pub fn shared_data(&self) -> PathBuf {
        self.data.clone()
    }

    pub fn runtime_data(&self) -> PathBuf {
        match self.build {
            "release" | "local" => self.data.clone(),
            other => self.data.join(other),
        }
    }

    pub fn application_data(&self) -> PathBuf {
        self.data.clone()
    }

    pub fn config(&self) -> PathBuf {
        if self.test {
            self.data.join("config")
        } else {
            home_dir().join(".vmux")
        }
    }

    pub fn tool_home(&self) -> PathBuf {
        if self.test {
            self.profile().join("home")
        } else {
            home_dir()
        }
    }

    pub fn isolated(&self) -> bool {
        self.test
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
        let directory = self.cef_directory();
        directory
            .canonicalize()
            .unwrap_or(directory)
            .to_str()
            .map(str::to_owned)
    }

    fn cef_directory(&self) -> PathBuf {
        let profile = self.profile();
        if self.test || self.build == "release" {
            return profile;
        }
        profile.join("cef").join("local")
    }

    pub fn prepare_browser_profile(&self) -> std::io::Result<()> {
        let destination = self.cef_directory();
        self.validate_browser_directory(&destination)?;
        if self.test || self.build == "release" {
            return Ok(());
        }
        if let Ok(metadata) = destination.symlink_metadata() {
            if metadata.file_type().is_symlink() && !destination.exists() {
                std::fs::remove_file(&destination)?;
            } else {
                return Ok(());
            }
        }
        let Some(parent) = destination.parent() else {
            return Ok(());
        };
        if !parent.exists() {
            return Ok(());
        }
        let mut source = None;
        let mut newest = std::time::SystemTime::UNIX_EPOCH;
        for entry in std::fs::read_dir(parent)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || !entry.file_name().to_string_lossy().starts_with("local-")
            {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            if source.is_none() || modified > newest {
                newest = modified;
                source = Some(entry.path());
            }
        }
        if let Some(source) = source {
            #[cfg(unix)]
            std::os::unix::fs::symlink(source.file_name().unwrap(), destination)?;
        }
        Ok(())
    }

    fn validate_browser_directory(&self, directory: &Path) -> std::io::Result<()> {
        let mut existing = directory;
        while !existing.exists() {
            let Some(parent) = existing.parent() else {
                return Ok(());
            };
            existing = parent;
        }
        if self.data.exists()
            && !existing
                .canonicalize()?
                .starts_with(self.data.canonicalize()?)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Vmux browser profile resolves outside VMUX_DATA_DIR; copy its contents instead of linking to live data",
            ));
        }
        Ok(())
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
    fn isolated_settings_resolve_only_inside_the_fixture_directory() {
        let fixture = tempfile::tempdir().unwrap();
        let paths = ProfilePaths {
            build: "dev",
            profile: Profile::named("onboarding"),
            data: fixture.path().to_path_buf(),
            test: true,
        };
        let config = fixture.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let settings = config.join("settings.ron");
        std::fs::write(&settings, "(appearance: (locale: \"ja\"))").unwrap();
        assert_eq!(paths.settings(), settings);
        for path in [
            paths.projects(),
            paths.agents(),
            paths.lsp(),
            paths.tool_home(),
            paths.profile(),
        ] {
            assert!(path.starts_with(fixture.path()), "{path:?}");
        }
    }

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
    fn local_and_dev_reuse_browser_cookies_without_mixing_named_profiles() {
        let directory = tempfile::tempdir().unwrap();
        let local = ProfilePaths {
            build: "local",
            profile: Profile::named("personal"),
            data: directory.path().to_path_buf(),
            test: false,
        };
        let mut dev = local.clone();
        dev.build = "dev";
        std::fs::create_dir_all(local.cef_directory()).unwrap();
        std::fs::write(local.cef_directory().join("Cookies"), b"persisted session").unwrap();
        assert_eq!(
            std::fs::read(dev.cef_directory().join("Cookies")).unwrap(),
            b"persisted session"
        );
        dev.profile = Profile::named("work");
        assert!(!dev.cef_directory().join("Cookies").exists());
    }

    #[test]
    fn browser_profile_adopts_legacy_local_data_once_without_overwriting_it() {
        let directory = tempfile::tempdir().unwrap();
        let paths = ProfilePaths {
            build: "dev",
            profile: Profile::named("personal"),
            data: directory.path().to_path_buf(),
            test: false,
        };
        let legacy = paths.profile().join("cef/local-worktree-a");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("Cookies"), b"existing login").unwrap();
        paths.prepare_browser_profile().unwrap();
        assert_eq!(
            std::fs::read(paths.cef_directory().join("Cookies")).unwrap(),
            b"existing login"
        );
        let other = paths.profile().join("cef/local-worktree-b");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("Cookies"), b"other login").unwrap();
        paths.prepare_browser_profile().unwrap();
        assert_eq!(
            std::fs::read(paths.cef_directory().join("Cookies")).unwrap(),
            b"existing login"
        );
        assert_eq!(
            std::fs::read(legacy.join("Cookies")).unwrap(),
            b"existing login"
        );
        assert_eq!(
            std::fs::read(other.join("Cookies")).unwrap(),
            b"other login"
        );
    }

    #[cfg(unix)]
    #[test]
    fn copied_browser_profile_cannot_link_back_to_live_data() {
        let live = tempfile::tempdir().unwrap();
        let copy = tempfile::tempdir().unwrap();
        let paths = ProfilePaths {
            build: "dev",
            profile: Profile::named("personal"),
            data: copy.path().to_path_buf(),
            test: false,
        };
        std::fs::write(live.path().join("Cookies"), b"live login").unwrap();
        std::fs::create_dir_all(paths.profile().join("cef")).unwrap();
        std::os::unix::fs::symlink(live.path(), paths.cef_directory()).unwrap();
        let error = paths.prepare_browser_profile().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read(live.path().join("Cookies")).unwrap(),
            b"live login"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_browser_profile_link_recovers_without_blocking_startup() {
        let directory = tempfile::tempdir().unwrap();
        let paths = ProfilePaths {
            build: "dev",
            profile: Profile::named("personal"),
            data: directory.path().to_path_buf(),
            test: false,
        };
        std::fs::create_dir_all(paths.profile().join("cef")).unwrap();
        std::os::unix::fs::symlink("local-removed", paths.cef_directory()).unwrap();
        paths.prepare_browser_profile().unwrap();
        assert!(paths.cef_directory().symlink_metadata().is_err());
        std::fs::create_dir_all(paths.profile().join("cef/local-existing")).unwrap();
        std::os::unix::fs::symlink("local-removed", paths.cef_directory()).unwrap();
        paths.prepare_browser_profile().unwrap();
        assert!(paths.cef_directory().is_dir());
        assert_eq!(
            std::fs::read_link(paths.cef_directory()).unwrap(),
            PathBuf::from("local-existing")
        );
    }

    #[test]
    fn copied_store_and_runtime_are_separate_from_normal_data() {
        let directory = tempfile::tempdir().unwrap();
        let local = ProfilePaths {
            build: "local",
            profile: Profile::named("personal"),
            data: directory.path().join("normal"),
            test: false,
        };
        let mut dev = local.clone();
        dev.build = "dev";
        std::fs::write(local.store().join("store.ron"), b"workspace").unwrap();
        assert_eq!(
            std::fs::read(dev.store().join("store.ron")).unwrap(),
            b"workspace"
        );
        assert_ne!(local.runtime_data(), dev.runtime_data());
        dev.data = directory.path().join("copy");
        assert!(!dev.store().join("store.ron").exists());
        assert!(dev.runtime_data().starts_with(&dev.data));
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
