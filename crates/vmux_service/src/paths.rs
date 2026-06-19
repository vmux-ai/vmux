use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use vmux_core::profile::shared_data_dir;

/// Profile this build was compiled for ("release", "local", or "dev").
pub fn current_profile() -> &'static str {
    env!("VMUX_BUILD_PROFILE")
}

/// Directory for service runtime files (socket, pid, identity). Nested under the
/// profile-specific data dir so `dev` builds stay isolated under `Vmux/dev`.
pub fn service_dir() -> PathBuf {
    shared_data_dir().join("services")
}

fn profile_file(ext: &str) -> PathBuf {
    service_dir().join(format!("vmux-{}.{ext}", current_profile()))
}

/// Path to the per-profile Unix domain socket.
pub fn socket_path() -> PathBuf {
    profile_file("sock")
}

/// Path to the per-profile PID file.
pub fn pid_path() -> PathBuf {
    profile_file("pid")
}

/// Path to the per-profile service executable identity file.
pub fn identity_path() -> PathBuf {
    profile_file("identity")
}

/// Path to the per-profile service stdout/stderr capture log. Lives alongside
/// the rotated application logs in `log_dir`, not in `service_dir`.
pub fn log_path() -> PathBuf {
    log_dir().join(format!("vmux-{}.log", current_profile()))
}

/// Directory for application log files (separate from runtime files in
/// `service_dir`). Nested under the profile-specific data dir so `dev` builds
/// stay isolated under `Vmux/dev`.
pub fn log_dir() -> PathBuf {
    shared_data_dir().join("logs")
}

/// Path to today's unified log file. Matches the filename the tracing-appender
/// DAILY rotation writes (`vmux-{profile}.{YYYY-MM-DD}.log`, UTC date), so the
/// daemon, the desktop file layer, and the panic hook all target the same file.
pub fn current_log_file() -> PathBuf {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    log_dir().join(format!("vmux-{}.{date}.log", current_profile()))
}

/// LaunchAgent label for the given profile.
///
/// `release` drops the suffix; `local` expands to the build-time git SHA so
/// each per-SHA local install registers a distinct background service. All
/// other profiles (including `dev`) keep the literal profile name as suffix.
pub fn launchd_label(profile: &str) -> String {
    match profile {
        "release" => "ai.vmux.service".to_string(),
        "local" => format!("ai.vmux.service.{}", env!("VMUX_GIT_HASH")),
        _ => format!("ai.vmux.service.{profile}"),
    }
}

/// Path to the LaunchAgent plist for the given profile.
pub fn plist_path(profile: &str) -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", launchd_label(profile)))
}

/// Path to the daemon binary, resolved as a sibling of the current executable.
/// Used by both the daemon (where current_exe IS the daemon) and the GUI/CLI
/// (where it points to the daemon binary alongside them) so identity checks
/// agree on the same target file.
pub fn daemon_binary_path() -> std::io::Result<PathBuf> {
    Ok(daemon_binary_path_for_exe(&std::env::current_exe()?))
}

fn daemon_binary_path_for_exe(exe: &Path) -> PathBuf {
    if matches!(
        exe.file_name().and_then(|n| n.to_str()),
        Some("vmux_service" | "Vmux Service")
    ) {
        return exe.to_path_buf();
    }

    if let Some(root) = crate::bundle::bundle_root_for(exe) {
        return root
            .join("Contents")
            .join("Library")
            .join("LoginItems")
            .join("Vmux Service.app")
            .join("Contents")
            .join("MacOS")
            .join("Vmux Service");
    }

    let mut p = exe.to_path_buf();
    p.pop();
    p.push("vmux_service");
    p
}

/// Identity for the daemon binary. Changes when the binary path, size,
/// or modification timestamp changes. Computed from `daemon_binary_path()`
/// so the daemon and its clients agree on the same target.
pub fn current_executable_identity() -> std::io::Result<String> {
    executable_identity_for_path(&daemon_binary_path()?)
}

/// Write the daemon binary's identity into the per-profile identity file.
pub fn write_service_identity() -> std::io::Result<()> {
    std::fs::write(identity_path(), current_executable_identity()?)
}

pub(crate) fn executable_identity_for_path(path: &Path) -> std::io::Result<String> {
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(format!(
        "{}\n{}\n{modified}",
        path.display(),
        metadata.len()
    ))
}

pub(crate) fn service_identity_matches(recorded: &str, current: &str) -> bool {
    recorded.trim() == current.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn executable_identity_changes_when_file_changes() {
        let path = std::env::temp_dir().join(format!("vmux-identity-test-{}", std::process::id()));
        {
            let mut file = std::fs::File::create(&path).expect("create identity test file");
            file.write_all(b"old").expect("write old identity bytes");
        }
        let old_identity = executable_identity_for_path(&path).expect("old identity");

        std::thread::sleep(std::time::Duration::from_millis(2));
        {
            let mut file = std::fs::File::create(&path).expect("rewrite identity test file");
            file.write_all(b"newer").expect("write new identity bytes");
        }
        let new_identity = executable_identity_for_path(&path).expect("new identity");
        let _ = std::fs::remove_file(&path);

        assert_ne!(old_identity, new_identity);
    }

    #[test]
    fn bundled_main_app_resolves_named_service_app_executable() {
        let exe = PathBuf::from("/Applications/Vmux.app/Contents/MacOS/Vmux");

        assert_eq!(
            daemon_binary_path_for_exe(&exe),
            PathBuf::from(
                "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service"
            )
        );
    }

    #[test]
    fn bundled_service_app_resolves_to_self() {
        let exe = PathBuf::from(
            "/Applications/Vmux.app/Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service",
        );

        assert_eq!(daemon_binary_path_for_exe(&exe), exe);
    }

    #[test]
    fn unbundled_debug_app_resolves_legacy_service_binary() {
        let exe = PathBuf::from("/Users/x/repo/target/debug/vmux_desktop");

        assert_eq!(
            daemon_binary_path_for_exe(&exe),
            PathBuf::from("/Users/x/repo/target/debug/vmux_service")
        );
    }

    #[test]
    fn service_identity_match_requires_exact_record() {
        assert!(service_identity_matches("a\n1\n2\n", "a\n1\n2"));
        assert!(!service_identity_matches("a\n1\n2", "a\n1\n3"));
    }

    #[test]
    fn current_profile_is_compile_env() {
        let p = current_profile();
        assert!(!p.is_empty());
        assert!(matches!(p, "release" | "local" | "dev"));
    }

    #[test]
    fn launchd_label_includes_profile() {
        assert_eq!(launchd_label("dev"), "ai.vmux.service.dev");
        assert_eq!(launchd_label("release"), "ai.vmux.service");
        let local = launchd_label("local");
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
        let s = socket_path();
        let name = s.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("vmux-"));
        assert!(name.ends_with(".sock"));
        assert!(name.contains(current_profile()));
    }

    #[test]
    fn pid_log_identity_paths_share_profile_suffix() {
        let suffix = format!("vmux-{}", current_profile());
        for p in [pid_path(), identity_path(), log_path()] {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with(&suffix),
                "expected {name} to start with {suffix}"
            );
        }
    }

    #[test]
    fn service_and_log_dirs_nest_under_profile_data_dir() {
        let base = vmux_core::profile::shared_data_dir();
        assert_eq!(service_dir(), base.join("services"));
        assert_eq!(log_dir(), base.join("logs"));
    }

    #[test]
    fn log_path_lives_in_log_dir_not_service_dir() {
        let p = log_path();
        assert_eq!(p.parent().unwrap(), log_dir());
        assert_ne!(p.parent().unwrap(), service_dir());
        assert_eq!(
            p.file_name().unwrap().to_string_lossy(),
            format!("vmux-{}.log", current_profile())
        );
    }

    #[test]
    fn current_log_file_lives_in_log_dir_with_profile_and_date() {
        let p = current_log_file();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!("vmux-{}.", current_profile())),
            "got {name}"
        );
        assert!(name.ends_with(".log"), "got {name}");
        assert_eq!(p.parent().unwrap(), log_dir());
        assert!(log_dir().ends_with("logs"), "got {}", log_dir().display());
    }

    #[test]
    fn plist_path_lives_in_user_launchagents() {
        let p = plist_path("dev");
        let s = p.to_string_lossy();
        assert!(s.contains("Library/LaunchAgents"));
        assert!(s.ends_with("ai.vmux.service.dev.plist"));
    }
}
