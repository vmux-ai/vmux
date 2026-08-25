use std::path::PathBuf;
use vmux_profile::{active_profile_name, build_profile, git_hash, shared_data_dir};

#[derive(Clone, Debug)]
pub struct ServicePaths {
    build: &'static str,
    profile: String,
}

impl ServicePaths {
    pub fn current() -> Self {
        Self {
            build: build_profile(),
            profile: active_profile_name(),
        }
    }

    pub fn build_profile() -> &'static str {
        build_profile()
    }

    pub fn dir() -> PathBuf {
        shared_data_dir().join("services")
    }

    pub fn log_dir() -> PathBuf {
        shared_data_dir().join("logs")
    }

    pub fn shell_integration_dir() -> PathBuf {
        shared_data_dir().join("shell-integration")
    }

    pub fn socket(&self) -> PathBuf {
        self.runtime_file("sock")
    }

    pub fn pid(&self) -> PathBuf {
        self.runtime_file("pid")
    }

    pub fn identity(&self) -> PathBuf {
        self.runtime_file("identity")
    }

    pub fn log(&self) -> PathBuf {
        Self::log_dir().join(self.file_name("log"))
    }

    pub fn current_log(&self) -> PathBuf {
        let date = chrono::Utc::now().format("%Y-%m-%d");
        Self::log_dir().join(format!("{}.{date}.log", self.stem()))
    }

    pub fn remote(&self) -> RemotePaths {
        RemotePaths {
            service: self.clone(),
        }
    }

    fn stem(&self) -> String {
        if self.profile == "personal" {
            format!("vmux-{}", self.build)
        } else {
            format!("vmux-{}-{}", self.build, self.profile)
        }
    }

    fn file_name(&self, ext: &str) -> String {
        format!("{}.{ext}", self.stem())
    }

    fn runtime_file(&self, ext: &str) -> PathBuf {
        Self::dir().join(self.file_name(ext))
    }
}

#[derive(Clone, Debug)]
pub struct RemotePaths {
    service: ServicePaths,
}

impl RemotePaths {
    pub fn current() -> Self {
        ServicePaths::current().remote()
    }

    pub fn token(&self) -> PathBuf {
        self.service.runtime_file("remote-token")
    }

    pub fn state(&self) -> PathBuf {
        self.service.runtime_file("remote-state")
    }

    pub fn paired(&self) -> PathBuf {
        self.service.runtime_file("remote-paired")
    }

    pub fn certificate(&self) -> PathBuf {
        self.service.runtime_file("remote-cert")
    }

    pub fn key(&self) -> PathBuf {
        self.service.runtime_file("remote-key")
    }

    pub fn fingerprint(&self) -> PathBuf {
        self.service.runtime_file("remote-fingerprint")
    }

    pub fn relay_device(&self) -> PathBuf {
        self.service.runtime_file("remote-device")
    }

    pub fn relay_url(&self) -> PathBuf {
        self.service.runtime_file("remote-relay-url")
    }

    pub fn relay_registration(&self) -> PathBuf {
        self.service.runtime_file("remote-relay-registration")
    }
}

pub struct RemoteToken(pub String);

impl RemoteToken {
    const MIN_LEN: usize = 32;

    pub fn wait(timeout: std::time::Duration) -> std::io::Result<Self> {
        let path = RemotePaths::current().token();
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Ok(token) = std::fs::read_to_string(&path) {
                let token = token.trim();
                if token.len() >= Self::MIN_LEN {
                    return Ok(Self(token.to_string()));
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("remote token not created: {}", path.display()),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

#[derive(Clone, Debug)]
pub struct LaunchAgent {
    profile: String,
}

impl LaunchAgent {
    pub fn current() -> Self {
        Self::for_profile(ServicePaths::build_profile())
    }

    pub fn for_profile(profile: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
        }
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn label(&self) -> String {
        match self.profile.as_str() {
            "release" => "ai.vmux.service".to_string(),
            "local" => format!("ai.vmux.service.{}", git_hash()),
            profile => format!("ai.vmux.service.{profile}"),
        }
    }

    pub fn plist_path(&self) -> PathBuf {
        let home = std::env::var_os("HOME").expect("HOME not set");
        PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", self.label()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_profile_is_compile_env() {
        let p = ServicePaths::build_profile();
        assert!(!p.is_empty());
        assert!(matches!(p, "release" | "local" | "dev"));
    }

    #[test]
    fn launchd_label_includes_profile() {
        assert_eq!(
            LaunchAgent::for_profile("dev").label(),
            "ai.vmux.service.dev"
        );
        assert_eq!(
            LaunchAgent::for_profile("release").label(),
            "ai.vmux.service"
        );
        let local = LaunchAgent::for_profile("local").label();
        assert!(
            local.starts_with("ai.vmux.service."),
            "expected local label to start with 'ai.vmux.service.', got {local}"
        );
        assert_ne!(
            local, "ai.vmux.service.local",
            "local profile should expand to per-SHA label, not literal 'local'"
        );
    }

    #[test]
    fn socket_path_includes_profile_suffix() {
        let s = ServicePaths::current().socket();
        let name = s.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("vmux-"));
        assert!(name.ends_with(".sock"));
        assert!(name.contains(ServicePaths::build_profile()));
    }

    #[test]
    fn remote_token_uses_profile_file_name() {
        let path = RemotePaths::current().token();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("remote-token")
        );
    }

    #[test]
    fn profile_file_name_suffixes_only_non_personal() {
        let personal = ServicePaths {
            build: "dev",
            profile: "personal".to_string(),
        };
        let test_dev = ServicePaths {
            build: "dev",
            profile: "test".to_string(),
        };
        let test_release = ServicePaths {
            build: "release",
            profile: "test".to_string(),
        };

        assert_eq!(personal.file_name("sock"), "vmux-dev.sock");
        assert_eq!(test_dev.file_name("sock"), "vmux-dev-test.sock");
        assert_eq!(test_release.file_name("log"), "vmux-release-test.log");
    }

    #[test]
    fn pid_log_identity_paths_share_profile_suffix() {
        let paths = ServicePaths::current();
        let suffix = format!("vmux-{}", ServicePaths::build_profile());
        for p in [paths.pid(), paths.identity(), paths.log()] {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with(&suffix),
                "expected {name} to start with {suffix}"
            );
        }
    }

    #[test]
    fn service_and_log_dirs_nest_under_profile_data_dir() {
        let base = shared_data_dir();
        assert_eq!(ServicePaths::dir(), base.join("services"));
        assert_eq!(ServicePaths::log_dir(), base.join("logs"));
    }

    #[test]
    fn log_path_lives_in_log_dir_not_service_dir() {
        let paths = ServicePaths::current();
        let p = paths.log();
        assert_eq!(p.parent().unwrap(), ServicePaths::log_dir());
        assert_ne!(p.parent().unwrap(), ServicePaths::dir());
        assert_eq!(
            p.file_name().unwrap().to_string_lossy(),
            paths.file_name("log")
        );
    }

    #[test]
    fn current_log_file_lives_in_log_dir_with_profile_and_date() {
        let p = ServicePaths::current().current_log();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!("vmux-{}.", ServicePaths::build_profile())),
            "got {name}"
        );
        assert!(name.ends_with(".log"), "got {name}");
        assert_eq!(p.parent().unwrap(), ServicePaths::log_dir());
        assert!(
            ServicePaths::log_dir().ends_with("logs"),
            "got {}",
            ServicePaths::log_dir().display()
        );
    }

    #[test]
    fn plist_path_lives_in_user_launchagents() {
        let p = LaunchAgent::for_profile("dev").plist_path();
        let s = p.to_string_lossy();
        assert!(s.contains("Library/LaunchAgents"));
        assert!(s.ends_with("ai.vmux.service.dev.plist"));
    }
}
