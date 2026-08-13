//! Where the service keeps its files, and what launchd calls it.

use std::path::PathBuf;
use vmux_profile::{active_profile_name, build_profile, git_hash, shared_data_dir};

/// Every runtime file the service owns, for the build and the profile this process runs as.
///
/// Read from the environment rather than passed in, because the profile is a property of the
/// process: two components disagreeing about it would talk past each other over different sockets.
#[derive(Clone, Debug)]
pub struct ServicePaths {
    build: &'static str,
    profile: String,
}

impl ServicePaths {
    /// The paths this process should use.
    pub fn current() -> Self {
        Self {
            build: build_profile(),
            profile: active_profile_name(),
        }
    }

    /// Profile this build was compiled for ("release", "local", or "dev").
    pub fn build_profile() -> &'static str {
        build_profile()
    }

    /// Directory for service runtime files (socket, pid, identity). Nested under the
    /// profile-specific data dir so `dev` builds stay isolated under `Vmux/dev`.
    pub fn dir() -> PathBuf {
        shared_data_dir().join("services")
    }

    /// Directory for application log files, separate from the runtime files in
    /// [`ServicePaths::dir`]. Nested under the profile-specific data dir so `dev` builds stay
    /// isolated under `Vmux/dev`.
    pub fn log_dir() -> PathBuf {
        shared_data_dir().join("logs")
    }

    /// Directory holding the shell integration snippets a spawned shell sources.
    pub fn shell_integration_dir() -> PathBuf {
        shared_data_dir().join("shell-integration")
    }

    /// The per-profile Unix domain socket.
    pub fn socket(&self) -> PathBuf {
        self.runtime_file("sock")
    }

    /// The per-profile PID file.
    pub fn pid(&self) -> PathBuf {
        self.runtime_file("pid")
    }

    /// The per-profile record of which daemon binary is running, as [`DaemonIdentity`] spells it.
    ///
    /// [`DaemonIdentity`]: crate::daemon::DaemonIdentity
    pub fn identity(&self) -> PathBuf {
        self.runtime_file("identity")
    }

    /// The per-profile service stdout/stderr capture log. Lives alongside the rotated application
    /// logs in [`ServicePaths::log_dir`], not in [`ServicePaths::dir`].
    pub fn log(&self) -> PathBuf {
        Self::log_dir().join(self.file_name("log"))
    }

    /// Today's unified log file. Matches the filename the tracing-appender DAILY rotation writes
    /// (`vmux-{profile}.{YYYY-MM-DD}.log`, UTC date), so the daemon, the desktop file layer, and
    /// the panic hook all target the same file.
    pub fn current_log(&self) -> PathBuf {
        let date = chrono::Utc::now().format("%Y-%m-%d");
        Self::log_dir().join(format!("{}.{date}.log", self.stem()))
    }

    /// The files Remote keeps beside these.
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

/// The files Remote writes, for the build and the profile this process runs as.
///
/// Separate from [`ServicePaths`] because they answer a different question. The socket and the pid
/// file exist whenever the daemon does; these appear only once someone turns Remote on, and every
/// reader has to cope with them being absent.
#[derive(Clone, Debug)]
pub struct RemotePaths {
    service: ServicePaths,
}

impl RemotePaths {
    /// The Remote files this process should use.
    pub fn current() -> Self {
        ServicePaths::current().remote()
    }

    /// The bearer token accepted by the local mobile remote server.
    pub fn token(&self) -> PathBuf {
        self.service.runtime_file("remote-token")
    }

    /// The desired Remote exposure state.
    pub fn state(&self) -> PathBuf {
        self.service.runtime_file("remote-state")
    }

    /// The marker written after a phone first authenticates successfully.
    pub fn paired(&self) -> PathBuf {
        self.service.runtime_file("remote-paired")
    }

    /// The self-signed certificate the QUIC listener presents.
    ///
    /// Persisted rather than minted per launch: the pairing link records its fingerprint, so a
    /// fresh certificate on every start would silently unpair every phone.
    pub fn certificate(&self) -> PathBuf {
        self.service.runtime_file("remote-cert")
    }

    /// The private key for [`RemotePaths::certificate`]. Written at mode `0600`.
    pub fn key(&self) -> PathBuf {
        self.service.runtime_file("remote-key")
    }

    /// The fingerprint of [`RemotePaths::certificate`], as the pairing link spells it.
    ///
    /// Derived from the certificate, but written down because the CLI builds pairing links too and
    /// hashing a PEM would cost it the whole QUIC stack as a dependency for one digest.
    pub fn fingerprint(&self) -> PathBuf {
        self.service.runtime_file("remote-fingerprint")
    }

    /// The stable relay device id for the active build and profile.
    pub fn relay_device(&self) -> PathBuf {
        self.service.runtime_file("remote-device")
    }

    /// The resolved relay URL for the active build and profile.
    ///
    /// The desktop app writes this so the daemon can read it: launchd starts the daemon, so it
    /// does not inherit the environment the app was launched with.
    pub fn relay_url(&self) -> PathBuf {
        self.service.runtime_file("remote-relay-url")
    }

    /// The device id the relay has accepted a registration for, while it holds.
    ///
    /// Distinct from [`RemotePaths::relay_device`], which is this desktop's id whether or not
    /// anything has been registered. The app builds a pairing link from this one, so a link is
    /// never offered for a desktop the relay would not route to.
    pub fn relay_registration(&self) -> PathBuf {
        self.service.runtime_file("remote-relay-registration")
    }
}

/// The secret a phone presents to prove it has been paired.
pub struct RemoteToken(pub String);

impl RemoteToken {
    /// The shortest token the daemon will ever write. Anything shorter is a partial read of a
    /// file being written, not a token.
    const MIN_LEN: usize = 32;

    /// Block until the daemon has written one.
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

/// The LaunchAgent that starts the daemon for one profile.
///
/// Named by a profile rather than reading the current one, because uninstalling a stale install
/// means addressing an agent this build was not compiled for.
///
/// The launchctl verbs — installing, booting out, kickstarting — hang off this too, in the
/// macOS-only `launchd` module.
#[derive(Clone, Debug)]
pub struct LaunchAgent {
    profile: String,
}

impl LaunchAgent {
    /// The agent for the profile this build was compiled for.
    pub fn current() -> Self {
        Self::for_profile(ServicePaths::build_profile())
    }

    /// The agent for a named profile.
    pub fn for_profile(profile: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
        }
    }

    /// The profile this agent runs the daemon as.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// The launchd label.
    ///
    /// `release` drops the suffix; `local` expands to the build-time git SHA so each per-SHA local
    /// install registers a distinct background service. All other profiles (including `dev`) keep
    /// the literal profile name as suffix.
    pub fn label(&self) -> String {
        match self.profile.as_str() {
            "release" => "ai.vmux.service".to_string(),
            "local" => format!("ai.vmux.service.{}", git_hash()),
            profile => format!("ai.vmux.service.{profile}"),
        }
    }

    /// Path to this agent's plist.
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
