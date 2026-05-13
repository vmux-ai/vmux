//! Implementation of `vmux service ...` subcommands.

#[cfg(target_os = "macos")]
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub profile: String,
    pub pid: Option<i32>,
    pub uptime: Option<Duration>,
    pub socket: std::path::PathBuf,
    pub identity_short: Option<String>,
    pub process_count: Option<u32>,
}

pub fn format_status(s: &StatusInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("profile     {}\n", s.profile));
    out.push_str(&format!(
        "pid         {}\n",
        s.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!(
        "uptime      {}\n",
        s.uptime.map(format_uptime).unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!("socket      {}\n", s.socket.display()));
    out.push_str(&format!(
        "identity    {}\n",
        s.identity_short.clone().unwrap_or_else(|| "-".into())
    ));
    out.push_str(&format!(
        "processes   {}\n",
        s.process_count
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".into())
    ));
    out
}

fn format_uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (h, rem) = (s / 3600, s % 3600);
    let (m, sec) = (rem / 60, rem % 60);
    if h > 0 {
        format!("{h}h {m}m {sec}s")
    } else if m > 0 {
        format!("{m}m {sec}s")
    } else {
        format!("{sec}s")
    }
}

fn read_pid() -> Option<i32> {
    std::fs::read_to_string(crate::pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn read_identity_short() -> Option<String> {
    std::fs::read_to_string(crate::identity_path())
        .ok()
        .map(|s| {
            let mut hash: u64 = 5381;
            for b in s.trim().bytes() {
                hash = hash.wrapping_mul(33).wrapping_add(b as u64);
            }
            let folded = (hash as u32) ^ ((hash >> 32) as u32);
            format!("{folded:08x}")
        })
}

fn live_status() -> Option<(u64, u32)> {
    live_status_inner().ok().flatten()
}

fn live_status_inner() -> std::io::Result<Option<(u64, u32)>> {
    use crate::protocol::{ClientMessage, ServiceMessage};
    let stream = std::os::unix::net::UnixStream::connect(crate::socket_path())?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut stream = stream;
    crate::write_message_blocking!(&mut stream, &ClientMessage::Status)?;
    let mut reader = std::io::BufReader::new(&mut stream);
    let msg = crate::read_message_blocking!(&mut reader, ServiceMessage)?;
    Ok(match msg {
        Some(ServiceMessage::StatusResponse {
            uptime_secs,
            process_count,
        }) => Some((uptime_secs, process_count)),
        _ => None,
    })
}

pub fn cmd_status() -> std::io::Result<i32> {
    let pid = read_pid();
    let live = live_status();
    let info = StatusInfo {
        profile: crate::current_profile().to_string(),
        pid,
        uptime: live.map(|(s, _)| Duration::from_secs(s)),
        socket: crate::socket_path(),
        identity_short: read_identity_short(),
        process_count: live.map(|(_, c)| c),
    };
    print!("{}", format_status(&info));
    Ok(if live.is_some() { 0 } else { 1 })
}

#[cfg(target_os = "macos")]
pub fn cmd_install(binary_path: &Path) -> std::io::Result<i32> {
    let profile = crate::current_profile();
    let plist = crate::launchd::install(profile, binary_path)?;
    println!("installed: {}", plist.display());
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_uninstall() -> std::io::Result<i32> {
    let profile = crate::current_profile();
    crate::launchd::uninstall(profile)?;
    println!("uninstalled: {}", crate::plist_path(profile).display());
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_start(binary_path: &Path) -> std::io::Result<i32> {
    let profile = crate::current_profile();
    crate::launchd::ensure_running(profile, binary_path)?;
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_stop() -> std::io::Result<i32> {
    let profile = crate::current_profile();
    crate::launchd::bootout(profile)?;
    Ok(0)
}

#[cfg(target_os = "macos")]
pub fn cmd_restart(binary_path: &Path) -> std::io::Result<i32> {
    let profile = crate::current_profile();
    let _ = crate::launchd::bootout(profile);
    crate::launchd::ensure_running(profile, binary_path)?;
    Ok(0)
}

pub fn cmd_logs(follow: bool) -> std::io::Result<i32> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new("tail");
    if follow {
        cmd.arg("-f");
    }
    cmd.arg(crate::log_path());
    let err = cmd.exec();
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn format_uptime_formats_segments() {
        assert_eq!(format_uptime(Duration::from_secs(0)), "0s");
        assert_eq!(format_uptime(Duration::from_secs(45)), "45s");
        assert_eq!(format_uptime(Duration::from_secs(75)), "1m 15s");
        assert_eq!(format_uptime(Duration::from_secs(3601)), "1h 0m 1s");
    }

    #[test]
    fn format_status_renders_all_fields() {
        let info = StatusInfo {
            profile: "dev".into(),
            pid: Some(12345),
            uptime: Some(Duration::from_secs(60)),
            socket: PathBuf::from("/tmp/vmux-dev.sock"),
            identity_short: Some("abcd1234".into()),
            process_count: Some(2),
        };
        let out = format_status(&info);
        assert!(out.contains("profile     dev"));
        assert!(out.contains("pid         12345"));
        assert!(out.contains("uptime      1m 0s"));
        assert!(out.contains("socket      /tmp/vmux-dev.sock"));
        assert!(out.contains("identity    abcd1234"));
        assert!(out.contains("processes   2"));
    }

    #[test]
    fn format_status_renders_dashes_when_unknown() {
        let info = StatusInfo {
            profile: "dev".into(),
            pid: None,
            uptime: None,
            socket: PathBuf::from("/tmp/vmux-dev.sock"),
            identity_short: None,
            process_count: None,
        };
        let out = format_status(&info);
        assert!(out.contains("pid         -"));
        assert!(out.contains("uptime      -"));
        assert!(out.contains("identity    -"));
        assert!(out.contains("processes   -"));
    }
}
