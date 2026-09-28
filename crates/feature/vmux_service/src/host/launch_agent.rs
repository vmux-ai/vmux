use std::path::PathBuf;

use vmux_core::service::ServicePaths;
use vmux_profile::git_hash;

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
        assert!(local.starts_with("ai.vmux.service."));
        assert_ne!(local, "ai.vmux.service.local");
    }

    #[test]
    fn plist_path_lives_in_user_launchagents() {
        let path = LaunchAgent::for_profile("dev").plist_path();
        let path = path.to_string_lossy();
        assert!(path.contains("Library/LaunchAgents"));
        assert!(path.ends_with("ai.vmux.service.dev.plist"));
    }
}
