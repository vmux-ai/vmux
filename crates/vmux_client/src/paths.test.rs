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
fn remote_port_is_stable_and_non_privileged() {
    let port = remote_port();
    assert!((54_821..=55_821).contains(&port));
    assert_eq!(port, remote_port());
}

#[test]
fn remote_token_uses_profile_file_name() {
    let path = remote_token_path();
    assert_eq!(
        path.extension().and_then(|value| value.to_str()),
        Some("remote-token")
    );
}

#[test]
fn profile_file_name_suffixes_only_non_personal() {
    assert_eq!(
        profile_file_name("dev", "personal", "sock"),
        "vmux-dev.sock"
    );
    assert_eq!(
        profile_file_name("dev", "test", "sock"),
        "vmux-dev-test.sock"
    );
    assert_eq!(
        profile_file_name("release", "test", "log"),
        "vmux-release-test.log"
    );
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
    let base = shared_data_dir();
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
        profile_file_name(current_profile(), &active_profile_name(), "log")
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

/// There is no way to ask for no relay: a desktop behind NAT is unreachable without one, so
/// a blank setting falls back to the hosted relay rather than disabling pairing.
#[test]
fn a_blank_relay_setting_falls_back_to_the_hosted_one() {
    for (from_env, expected) in [
        (None, DEFAULT_RELAY_URL),
        (Some(""), DEFAULT_RELAY_URL),
        (Some("   "), DEFAULT_RELAY_URL),
        (
            Some("https://relay.example.com/"),
            "https://relay.example.com",
        ),
        (Some("  https://localhost:8788  "), "https://localhost:8788"),
    ] {
        assert_eq!(
            resolve_relay_url(from_env),
            expected,
            "from_env = {from_env:?}"
        );
    }
}
