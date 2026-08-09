use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use vmux_profile::{active_profile_name, build_profile, git_hash, shared_data_dir};

/// Profile this build was compiled for ("release", "local", or "dev").
pub fn current_profile() -> &'static str {
    build_profile()
}

/// Directory for service runtime files (socket, pid, identity). Nested under the
/// profile-specific data dir so `dev` builds stay isolated under `Vmux/dev`.
pub fn service_dir() -> PathBuf {
    shared_data_dir().join("services")
}

fn profile_file_stem(build: &str, profile: &str) -> String {
    if profile == "personal" {
        format!("vmux-{build}")
    } else {
        format!("vmux-{build}-{profile}")
    }
}

fn profile_file_name(build: &str, profile: &str, ext: &str) -> String {
    format!("{}.{ext}", profile_file_stem(build, profile))
}

fn profile_file(ext: &str) -> PathBuf {
    let profile = active_profile_name();
    service_dir().join(profile_file_name(current_profile(), &profile, ext))
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

/// Path to the bearer token accepted by the local mobile remote server.
pub fn remote_token_path() -> PathBuf {
    profile_file("remote-token")
}

/// Path to the desired Remote exposure state.
pub fn remote_state_path() -> PathBuf {
    profile_file("remote-state")
}

/// Path to the marker written after a phone first authenticates successfully.
pub fn remote_paired_path() -> PathBuf {
    profile_file("remote-paired")
}

/// Path to the self-signed certificate the QUIC listener presents.
///
/// Persisted rather than minted per launch: the pairing link records its fingerprint, so a fresh
/// certificate on every start would silently unpair every phone.
pub fn remote_cert_path() -> PathBuf {
    profile_file("remote-cert")
}

/// Path to the private key for [`remote_cert_path`]. Written at mode `0600`.
pub fn remote_key_path() -> PathBuf {
    profile_file("remote-key")
}

/// Path to the fingerprint of [`remote_cert_path`], as the pairing link spells it.
///
/// Derived from the certificate, but written down because the CLI builds pairing links too and
/// hashing a PEM would cost it the whole QUIC stack as a dependency for one digest.
pub fn remote_fingerprint_path() -> PathBuf {
    profile_file("remote-fingerprint")
}

/// Path to the stable relay device id for the active build and profile.
pub fn remote_relay_device_path() -> PathBuf {
    profile_file("remote-device")
}

/// Path to the resolved relay URL for the active build and profile.
///
/// The desktop app writes this so the daemon can read it: launchd starts the daemon, so it does
/// not inherit the environment the app was launched with.
pub fn remote_relay_url_path() -> PathBuf {
    profile_file("remote-relay-url")
}

/// The hosted relay, used when nothing overrides it.
pub const DEFAULT_RELAY_URL: &str = "https://relay.vmux.ai";

/// The relay `VMUX_REMOTE_RELAY_URL` asks for, or the hosted one when it is unset.
pub fn relay_url_from_env() -> String {
    resolve_relay_url(std::env::var("VMUX_REMOTE_RELAY_URL").ok().as_deref())
}

/// Decide the relay from what the environment said, if anything.
///
/// Every pairing goes through a relay now, so a blank value means "use the hosted one" rather
/// than "turn the relay off". There is no off: a desktop sits behind NAT and is unreachable
/// without one.
pub fn resolve_relay_url(from_env: Option<&str>) -> String {
    let asked = from_env.unwrap_or(DEFAULT_RELAY_URL);
    normalize_relay_url(asked).unwrap_or_else(|| DEFAULT_RELAY_URL.to_string())
}

/// A relay URL trimmed to canonical form, or `None` when blank.
pub fn normalize_relay_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Path to the UDP port the relay allocated this desktop.
///
/// Written by the daemon when it registers, read by the app when it builds a pairing link — the
/// port is the relay's to choose, and the link has no other way to learn it.
pub fn remote_relay_port_path() -> PathBuf {
    profile_file("remote-relay-port")
}

/// Stable loopback port for the active build and profile.
pub fn remote_port() -> u16 {
    let build = current_profile();
    let profile = active_profile_name();
    if build == "release" && profile == "personal" {
        return 54_821;
    }
    let hash = format!("{build}:{profile}")
        .bytes()
        .fold(5381_u32, |hash, byte| {
            hash.wrapping_mul(33).wrapping_add(u32::from(byte))
        });
    54_822 + (hash % 1_000) as u16
}

/// Path to the per-profile service stdout/stderr capture log. Lives alongside
/// the rotated application logs in `log_dir`, not in `service_dir`.
pub fn log_path() -> PathBuf {
    let profile = active_profile_name();
    log_dir().join(profile_file_name(current_profile(), &profile, "log"))
}

/// Directory for application log files (separate from runtime files in
/// `service_dir`). Nested under the profile-specific data dir so `dev` builds
/// stay isolated under `Vmux/dev`.
pub fn log_dir() -> PathBuf {
    shared_data_dir().join("logs")
}

pub fn shell_integration_dir() -> PathBuf {
    shared_data_dir().join("shell-integration")
}

/// Path to today's unified log file. Matches the filename the tracing-appender
/// DAILY rotation writes (`vmux-{profile}.{YYYY-MM-DD}.log`, UTC date), so the
/// daemon, the desktop file layer, and the panic hook all target the same file.
pub fn current_log_file() -> PathBuf {
    let date = chrono::Utc::now().format("%Y-%m-%d");
    let profile = active_profile_name();
    log_dir().join(format!(
        "{}.{date}.log",
        profile_file_stem(current_profile(), &profile)
    ))
}

/// LaunchAgent label for the given profile.
///
/// `release` drops the suffix; `local` expands to the build-time git SHA so
/// each per-SHA local install registers a distinct background service. All
/// other profiles (including `dev`) keep the literal profile name as suffix.
pub fn launchd_label(profile: &str) -> String {
    match profile {
        "release" => "ai.vmux.service".to_string(),
        "local" => format!("ai.vmux.service.{}", git_hash()),
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

#[doc(hidden)]
pub fn service_identity_matches(recorded: &str, current: &str) -> bool {
    recorded.trim() == current.trim()
}

#[cfg(test)]
#[path = "paths.test.rs"]
mod tests;
