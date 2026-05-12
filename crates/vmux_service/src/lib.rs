pub mod client;
pub mod framing;
pub mod process;
pub mod protocol;
pub mod server;

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Profile this build was compiled for ("release", "local", or "dev").
pub fn current_profile() -> &'static str {
    env!("VMUX_PROFILE")
}

/// Directory for service runtime files (socket, pid, log).
pub fn service_dir() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home).join("Library/Application Support/Vmux/services")
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

/// Path to the per-profile service log file.
pub fn log_path() -> PathBuf {
    profile_file("log")
}

/// LaunchAgent label for the given profile.
pub fn launchd_label(profile: &str) -> String {
    format!("ai.vmux.service.{profile}")
}

/// Path to the LaunchAgent plist for the given profile.
pub fn plist_path(profile: &str) -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME not set");
    PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", launchd_label(profile)))
}

/// Identity for the current executable. Changes when the binary path, size,
/// or modification timestamp changes.
pub fn current_executable_identity() -> std::io::Result<String> {
    executable_identity_for_path(&std::env::current_exe()?)
}

/// Write the current executable identity for a service process.
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
        assert_eq!(launchd_label("release"), "ai.vmux.service.release");
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
    fn plist_path_lives_in_user_launchagents() {
        let p = plist_path("dev");
        let s = p.to_string_lossy();
        assert!(s.contains("Library/LaunchAgents"));
        assert!(s.ends_with("ai.vmux.service.dev.plist"));
    }
}
