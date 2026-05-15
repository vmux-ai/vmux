//! macOS LaunchAgent integration for vmux_service.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Render the LaunchAgent plist XML for a profile.
pub fn generate_plist(profile: &str, binary_path: &Path, log_path: &Path) -> String {
    let label = crate::launchd_label(profile);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{binary}</string>
  </array>
  <key>RunAtLoad</key>
  <false/>
  <key>KeepAlive</key>
  <dict>
    <key>Crashed</key>
    <true/>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>VMUX_BUILD_PROFILE</key>
    <string>{profile}</string>
  </dict>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#,
        label = label,
        binary = binary_path.display(),
        log = log_path.display(),
        profile = profile,
    )
}

/// Write the plist for `profile` pointing at `binary_path`.
pub fn install(profile: &str, binary_path: &Path) -> std::io::Result<PathBuf> {
    let plist = crate::plist_path(profile);
    std::fs::create_dir_all(crate::service_dir())?;
    let log = crate::log_path();
    reconcile_plist_at(&plist, profile, binary_path, &log)?;
    bootstrap(&plist)?;
    Ok(plist)
}

fn reconcile_plist_at(
    plist: &Path,
    profile: &str,
    binary_path: &Path,
    log_path: &Path,
) -> std::io::Result<bool> {
    let desired = generate_plist(profile, binary_path, log_path);
    let current = match std::fs::read_to_string(plist) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if current.as_deref() == Some(desired.as_str()) {
        return Ok(false);
    }
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(plist, desired)?;
    Ok(true)
}

/// Remove the plist and unload from launchd.
pub fn uninstall(profile: &str) -> std::io::Result<()> {
    let plist = crate::plist_path(profile);
    if plist.exists() {
        let _ = bootout(profile);
        std::fs::remove_file(&plist)?;
    }
    Ok(())
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

/// `launchctl bootstrap gui/<uid> <plist>`.
pub fn bootstrap(plist: &Path) -> std::io::Result<()> {
    let uid = current_uid();
    let status = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}")])
        .arg(plist)
        .status()?;
    if !status.success() {
        tracing::warn!(code = ?status.code(), "launchctl bootstrap exited nonzero");
    }
    Ok(())
}

/// `launchctl bootout gui/<uid>/<label>`.
pub fn bootout(profile: &str) -> std::io::Result<()> {
    let uid = current_uid();
    let label = crate::launchd_label(profile);
    let status = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{label}")])
        .status()?;
    if !status.success() {
        tracing::warn!(code = ?status.code(), "launchctl bootout exited nonzero");
    }
    Ok(())
}

/// `launchctl kickstart -k gui/<uid>/<label>` -- restart cleanly.
pub fn kickstart(label: &str) -> std::io::Result<()> {
    let uid = current_uid();
    let status = Command::new("launchctl")
        .args(["kickstart", "-k", &format!("gui/{uid}/{label}")])
        .status()?;
    if !status.success() {
        tracing::warn!(code = ?status.code(), "launchctl kickstart exited nonzero");
    }
    Ok(())
}

/// Make sure the daemon is installed and running. Idempotent.
/// `binary_path` is the daemon executable (resolved by the caller).
pub fn ensure_running(profile: &str, binary_path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(crate::service_dir())?;
    let plist = crate::plist_path(profile);
    let log = crate::log_path();
    let rewrote = reconcile_plist_at(&plist, profile, binary_path, &log)?;
    if rewrote {
        let _ = bootout(profile);
    }
    bootstrap(&plist)?;
    kickstart(&crate::launchd_label(profile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_plist(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vmux-launchd-test-{}-{nanos}-{tag}.plist",
            std::process::id(),
        ))
    }

    #[test]
    fn generated_plist_contains_label_binary_log_profile() {
        let xml = generate_plist(
            "dev",
            &PathBuf::from("/usr/local/bin/vmux_service"),
            &PathBuf::from("/tmp/vmux-dev.log"),
        );
        assert!(xml.contains("<string>ai.vmux.service.dev</string>"));
        assert!(xml.contains("<string>/usr/local/bin/vmux_service</string>"));
        assert!(xml.contains("<string>/tmp/vmux-dev.log</string>"));
        assert!(xml.contains("<key>VMUX_BUILD_PROFILE</key>"));
        assert!(xml.contains("<string>dev</string>"));
        assert!(xml.contains("<key>RunAtLoad</key>\n  <false/>"));
        assert!(xml.contains("<key>KeepAlive</key>"));
        assert!(xml.contains("<key>Crashed</key>\n    <true/>"));
    }

    #[test]
    fn reconcile_plist_at_writes_when_missing() {
        let plist = temp_plist("missing");
        let _ = std::fs::remove_file(&plist);
        let bin = PathBuf::from("/usr/local/bin/vmux_service");
        let log = PathBuf::from("/tmp/vmux-dev.log");

        let rewrote = reconcile_plist_at(&plist, "dev", &bin, &log).expect("reconcile");

        assert!(rewrote, "expected reconcile to report write");
        let on_disk = std::fs::read_to_string(&plist).expect("plist exists");
        assert_eq!(on_disk, generate_plist("dev", &bin, &log));
        let _ = std::fs::remove_file(&plist);
    }

    #[test]
    fn reconcile_plist_at_rewrites_when_binary_path_drifts() {
        let plist = temp_plist("binary-drift");
        let log = PathBuf::from("/tmp/vmux-dev.log");
        let old_bin = PathBuf::from("/old/worktree/target/debug/vmux_service");
        let new_bin = PathBuf::from("/new/worktree/target/debug/vmux_service");
        std::fs::write(&plist, generate_plist("dev", &old_bin, &log)).expect("seed plist");

        let rewrote = reconcile_plist_at(&plist, "dev", &new_bin, &log).expect("reconcile");

        assert!(rewrote, "expected reconcile to rewrite drifted plist");
        let on_disk = std::fs::read_to_string(&plist).expect("plist exists");
        assert!(
            on_disk.contains("/new/worktree/target/debug/vmux_service"),
            "expected new binary path in {on_disk}"
        );
        assert!(
            !on_disk.contains("/old/worktree/target/debug/vmux_service"),
            "expected old binary path gone from {on_disk}"
        );
        let _ = std::fs::remove_file(&plist);
    }

    #[test]
    fn reconcile_plist_at_rewrites_when_env_var_key_drifts() {
        let plist = temp_plist("env-drift");
        let bin = PathBuf::from("/usr/local/bin/vmux_service");
        let log = PathBuf::from("/tmp/vmux-dev.log");
        let legacy_xml =
            generate_plist("dev", &bin, &log).replace("VMUX_BUILD_PROFILE", "VMUX_PROFILE");
        std::fs::write(&plist, &legacy_xml).expect("seed legacy plist");

        let rewrote = reconcile_plist_at(&plist, "dev", &bin, &log).expect("reconcile");

        assert!(
            rewrote,
            "expected reconcile to rewrite legacy env-var plist"
        );
        let on_disk = std::fs::read_to_string(&plist).expect("plist exists");
        assert!(
            on_disk.contains("<key>VMUX_BUILD_PROFILE</key>"),
            "expected new env var key in {on_disk}"
        );
        assert!(
            !on_disk.contains("<key>VMUX_PROFILE</key>"),
            "expected legacy env var key gone from {on_disk}"
        );
        let _ = std::fs::remove_file(&plist);
    }

    #[test]
    fn reconcile_plist_at_no_op_when_matching() {
        let plist = temp_plist("match");
        let bin = PathBuf::from("/usr/local/bin/vmux_service");
        let log = PathBuf::from("/tmp/vmux-dev.log");
        let xml = generate_plist("dev", &bin, &log);
        std::fs::write(&plist, &xml).expect("seed matching plist");
        let mtime_before = std::fs::metadata(&plist)
            .expect("metadata")
            .modified()
            .expect("mtime");

        std::thread::sleep(std::time::Duration::from_millis(10));
        let rewrote = reconcile_plist_at(&plist, "dev", &bin, &log).expect("reconcile");

        assert!(!rewrote, "expected reconcile to skip matching plist");
        let mtime_after = std::fs::metadata(&plist)
            .expect("metadata")
            .modified()
            .expect("mtime");
        assert_eq!(
            mtime_before, mtime_after,
            "plist should not have been touched"
        );
        let _ = std::fs::remove_file(&plist);
    }
}
