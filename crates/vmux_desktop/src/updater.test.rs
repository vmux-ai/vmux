use super::*;

#[test]
fn progress_step_emits_on_percent_increase() {
    assert_eq!(progress_step(50, 100, 0), Some(50));
    assert_eq!(progress_step(50, 100, 50), None);
    assert_eq!(progress_step(100, 100, 50), Some(100));
}

#[test]
fn progress_step_caps_at_100() {
    assert_eq!(progress_step(250, 100, 0), Some(100));
}

#[test]
fn progress_step_unknown_total_buckets_by_512k() {
    let bucket = 512 * 1024;
    assert_eq!(progress_step(0, 0, 0), None);
    assert_eq!(progress_step(bucket + 1, 0, 0), Some(1));
    assert_eq!(progress_step(bucket + 1, 0, 1), None);
}

#[test]
fn default_endpoint_is_vmux_ai_updates_json() {
    assert_eq!(DEFAULT_ENDPOINT, "https://vmux.ai/updates.json");
}

#[test]
fn default_updater_checks_after_launch_and_hourly() {
    let updater = VmuxUpdaterBuilder::default();

    assert_eq!(updater.initial_delay, Duration::from_secs(5));
    assert_eq!(updater.poll_interval, Duration::from_secs(3600));
}

#[test]
fn manual_update_check_bypasses_auto_update_setting() {
    assert!(should_start_update_check(true, false, false));
}

#[test]
fn automatic_update_check_requires_enabled_setting_and_due_timer() {
    assert!(should_start_update_check(false, true, true));
    assert!(!should_start_update_check(false, false, true));
    assert!(!should_start_update_check(false, true, false));
}

#[test]
fn default_pubkey_uses_runtime_env_first() {
    let pubkey = default_pubkey_from_env(Some("runtime".to_string()), Some("build"));

    assert_eq!(pubkey, "runtime");
}

#[test]
fn default_pubkey_falls_back_to_build_env() {
    let pubkey = default_pubkey_from_env(None, Some("build"));

    assert_eq!(pubkey, "build");
}

#[test]
fn default_pubkey_is_empty_when_env_is_missing() {
    let pubkey = default_pubkey_from_env(None, None);

    assert_eq!(pubkey, "");
}
