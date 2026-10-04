use std::path::PathBuf;

use crate::{Profile, ProfilePaths, build_profile};

#[derive(Clone, Debug)]
pub struct ServicePaths {
    build: &'static str,
    profile: String,
}

impl ServicePaths {
    pub fn current() -> Self {
        Self {
            build: build_profile(),
            profile: Profile::current().into_id(),
        }
    }

    pub fn build_profile() -> &'static str {
        build_profile()
    }

    pub fn dir() -> PathBuf {
        ProfilePaths::current().shared_data().join("services")
    }

    pub fn log_dir() -> PathBuf {
        ProfilePaths::current().shared_data().join("logs")
    }

    pub fn shell_integration_dir() -> PathBuf {
        ProfilePaths::current()
            .shared_data()
            .join("shell-integration")
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

    fn file_name(&self, extension: &str) -> String {
        format!("{}.{extension}", self.stem())
    }

    fn runtime_file(&self, extension: &str) -> PathBuf {
        Self::dir().join(self.file_name(extension))
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

    pub fn relay_token(&self) -> PathBuf {
        self.service.runtime_file("remote-token")
    }

    pub fn authorizations(&self) -> PathBuf {
        self.service.runtime_file("remote-authorizations")
    }

    pub fn state(&self) -> PathBuf {
        self.service.runtime_file("remote-state")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_profile_is_compile_env() {
        let profile = ServicePaths::build_profile();
        assert!(!profile.is_empty());
        assert!(matches!(profile, "release" | "local" | "dev"));
    }

    #[test]
    fn socket_path_includes_profile_suffix() {
        let socket = ServicePaths::current().socket();
        let name = socket.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("vmux-"));
        assert!(name.ends_with(".sock"));
        assert!(name.contains(ServicePaths::build_profile()));
    }

    #[test]
    fn remote_token_uses_profile_file_name() {
        let path = RemotePaths::current().relay_token();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("remote-token")
        );
    }

    #[test]
    fn remote_authorizations_use_profile_file_name() {
        let path = RemotePaths::current().authorizations();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("remote-authorizations")
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
        for path in [paths.pid(), paths.identity(), paths.log()] {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with(&suffix),
                "expected {name} to start with {suffix}"
            );
        }
    }

    #[test]
    fn service_and_log_dirs_nest_under_profile_data_dir() {
        let base = ProfilePaths::current().shared_data();
        assert_eq!(ServicePaths::dir(), base.join("services"));
        assert_eq!(ServicePaths::log_dir(), base.join("logs"));
    }

    #[test]
    fn log_path_lives_in_log_dir_not_service_dir() {
        let paths = ServicePaths::current();
        let path = paths.log();
        assert_eq!(path.parent().unwrap(), ServicePaths::log_dir());
        assert_ne!(path.parent().unwrap(), ServicePaths::dir());
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            paths.file_name("log")
        );
    }

    #[test]
    fn current_log_file_lives_in_log_dir_with_profile_and_date() {
        let path = ServicePaths::current().current_log();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!("vmux-{}.", ServicePaths::build_profile())),
            "got {name}"
        );
        assert!(name.ends_with(".log"), "got {name}");
        assert_eq!(path.parent().unwrap(), ServicePaths::log_dir());
        assert!(
            ServicePaths::log_dir().ends_with("logs"),
            "got {}",
            ServicePaths::log_dir().display()
        );
    }
}
