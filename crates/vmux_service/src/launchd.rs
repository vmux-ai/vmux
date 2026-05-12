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
    <key>VMUX_PROFILE</key>
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
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(crate::service_dir())?;

    let xml = generate_plist(profile, binary_path, &crate::log_path());
    std::fs::write(&plist, xml)?;
    bootstrap(&plist)?;
    Ok(plist)
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

/// `launchctl bootstrap gui/<uid> <plist>`.
pub fn bootstrap(plist: &Path) -> std::io::Result<()> {
    let uid = unsafe { libc::getuid() };
    Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}")])
        .arg(plist)
        .status()?;
    Ok(())
}

/// `launchctl bootout gui/<uid>/<label>`.
pub fn bootout(profile: &str) -> std::io::Result<()> {
    let uid = unsafe { libc::getuid() };
    let label = crate::launchd_label(profile);
    Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{label}")])
        .status()?;
    Ok(())
}

/// `launchctl kickstart -k gui/<uid>/<label>` -- restart cleanly.
pub fn kickstart(profile: &str) -> std::io::Result<()> {
    let uid = unsafe { libc::getuid() };
    let label = crate::launchd_label(profile);
    Command::new("launchctl")
        .args(["kickstart", "-k", &format!("gui/{uid}/{label}")])
        .status()?;
    Ok(())
}

/// Make sure the daemon is installed and running. Idempotent.
/// `binary_path` is the daemon executable (resolved by the caller).
pub fn ensure_running(profile: &str, binary_path: &Path) -> std::io::Result<()> {
    let plist = crate::plist_path(profile);
    if !plist.exists() {
        install(profile, binary_path)?;
    }
    kickstart(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        assert!(xml.contains("<key>VMUX_PROFILE</key>"));
        assert!(xml.contains("<string>dev</string>"));
        assert!(xml.contains("<key>RunAtLoad</key>\n  <false/>"));
        assert!(xml.contains("<key>KeepAlive</key>"));
        assert!(xml.contains("<key>Crashed</key>\n    <true/>"));
    }
}
