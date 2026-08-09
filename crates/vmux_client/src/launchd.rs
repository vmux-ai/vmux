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
    std::fs::create_dir_all(crate::log_dir())?;
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
    std::fs::create_dir_all(crate::log_dir())?;
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
#[path = "launchd.test.rs"]
mod tests;
