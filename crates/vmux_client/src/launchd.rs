//! macOS LaunchAgent integration for vmux_service.
//!
//! The verbs live on [`LaunchAgent`], which names the profile they act on. [`kickstart`] does not:
//! it takes a bare label so the bundled login item, which has no vmux profile of its own, can be
//! restarted the same way.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::{LaunchAgent, ServicePaths};

impl LaunchAgent {
    /// Render this agent's plist XML.
    pub fn plist_xml(&self, binary_path: &Path, log_path: &Path) -> String {
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
            label = self.label(),
            binary = binary_path.display(),
            log = log_path.display(),
            profile = self.profile(),
        )
    }

    /// Write this agent's plist pointing at `binary_path`, and load it.
    pub fn install(&self, binary_path: &Path) -> std::io::Result<PathBuf> {
        let plist = self.plist_path();
        std::fs::create_dir_all(ServicePaths::dir())?;
        std::fs::create_dir_all(ServicePaths::log_dir())?;
        let log = ServicePaths::current().log();
        self.reconcile_plist_at(&plist, binary_path, &log)?;
        bootstrap(&plist)?;
        Ok(plist)
    }

    /// Remove the plist and unload from launchd.
    pub fn uninstall(&self) -> std::io::Result<()> {
        let plist = self.plist_path();
        if plist.exists() {
            let _ = self.bootout();
            std::fs::remove_file(&plist)?;
        }
        Ok(())
    }

    /// `launchctl bootout gui/<uid>/<label>`.
    pub fn bootout(&self) -> std::io::Result<()> {
        let uid = current_uid();
        let label = self.label();
        let status = Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{label}")])
            .status()?;
        if !status.success() {
            tracing::warn!(code = ?status.code(), "launchctl bootout exited nonzero");
        }
        Ok(())
    }

    /// Make sure the daemon is installed and running. Idempotent.
    /// `binary_path` is the daemon executable (resolved by the caller).
    pub fn ensure_running(&self, binary_path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(ServicePaths::dir())?;
        std::fs::create_dir_all(ServicePaths::log_dir())?;
        let plist = self.plist_path();
        let log = ServicePaths::current().log();
        let rewrote = self.reconcile_plist_at(&plist, binary_path, &log)?;
        if rewrote {
            let _ = self.bootout();
        }
        bootstrap(&plist)?;
        kickstart(&self.label())
    }

    fn reconcile_plist_at(
        &self,
        plist: &Path,
        binary_path: &Path,
        log_path: &Path,
    ) -> std::io::Result<bool> {
        let desired = self.plist_xml(binary_path, log_path);
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
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

/// `launchctl bootstrap gui/<uid> <plist>`.
fn bootstrap(plist: &Path) -> std::io::Result<()> {
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

/// `launchctl kickstart -k gui/<uid>/<label>` -- restart cleanly.
///
/// Free rather than a [`LaunchAgent`] method because the bundled login item is kickstarted by its
/// packaged label, which belongs to no profile this build could name.
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
        let xml = LaunchAgent::for_profile("dev").plist_xml(
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
        let agent = LaunchAgent::for_profile("dev");
        let plist = temp_plist("missing");
        let _ = std::fs::remove_file(&plist);
        let bin = PathBuf::from("/usr/local/bin/vmux_service");
        let log = PathBuf::from("/tmp/vmux-dev.log");

        let rewrote = agent
            .reconcile_plist_at(&plist, &bin, &log)
            .expect("reconcile");

        assert!(rewrote, "expected reconcile to report write");
        let on_disk = std::fs::read_to_string(&plist).expect("plist exists");
        assert_eq!(on_disk, agent.plist_xml(&bin, &log));
        let _ = std::fs::remove_file(&plist);
    }

    #[test]
    fn reconcile_plist_at_rewrites_when_binary_path_drifts() {
        let agent = LaunchAgent::for_profile("dev");
        let plist = temp_plist("binary-drift");
        let log = PathBuf::from("/tmp/vmux-dev.log");
        let old_bin = PathBuf::from("/old/worktree/target/debug/vmux_service");
        let new_bin = PathBuf::from("/new/worktree/target/debug/vmux_service");
        std::fs::write(&plist, agent.plist_xml(&old_bin, &log)).expect("seed plist");

        let rewrote = agent
            .reconcile_plist_at(&plist, &new_bin, &log)
            .expect("reconcile");

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
        let agent = LaunchAgent::for_profile("dev");
        let plist = temp_plist("env-drift");
        let bin = PathBuf::from("/usr/local/bin/vmux_service");
        let log = PathBuf::from("/tmp/vmux-dev.log");
        let legacy_xml = agent
            .plist_xml(&bin, &log)
            .replace("VMUX_BUILD_PROFILE", "VMUX_PROFILE");
        std::fs::write(&plist, &legacy_xml).expect("seed legacy plist");

        let rewrote = agent
            .reconcile_plist_at(&plist, &bin, &log)
            .expect("reconcile");

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
        let agent = LaunchAgent::for_profile("dev");
        let plist = temp_plist("match");
        let bin = PathBuf::from("/usr/local/bin/vmux_service");
        let log = PathBuf::from("/tmp/vmux-dev.log");
        let xml = agent.plist_xml(&bin, &log);
        std::fs::write(&plist, &xml).expect("seed matching plist");
        let mtime_before = std::fs::metadata(&plist)
            .expect("metadata")
            .modified()
            .expect("mtime");

        std::thread::sleep(std::time::Duration::from_millis(10));
        let rewrote = agent
            .reconcile_plist_at(&plist, &bin, &log)
            .expect("reconcile");

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
