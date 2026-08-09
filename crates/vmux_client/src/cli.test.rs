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
