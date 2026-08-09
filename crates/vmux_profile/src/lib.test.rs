use super::*;

#[test]
fn recording_dir_is_nested_under_profile() {
    assert_eq!(
        recording_dir_for(std::path::Path::new("/data/Vmux"), "personal"),
        PathBuf::from("/data/Vmux/profiles/personal/recording")
    );
}

#[test]
fn recording_dir_test_profile_is_nested() {
    assert_eq!(
        recording_dir_for(std::path::Path::new("/data/Vmux"), "test"),
        PathBuf::from("/data/Vmux/profiles/test/recording")
    );
}

#[test]
fn sanitize_profile_keeps_safe_and_defaults_empty() {
    assert_eq!(sanitize_profile("test"), "test");
    assert_eq!(sanitize_profile("Test"), "test");
    assert_eq!(sanitize_profile(""), "personal");
    assert_eq!(sanitize_profile("  "), "personal");
    assert_eq!(sanitize_profile("a/b"), "a-b");
    assert_eq!(sanitize_profile("../evil"), "evil");
}

#[test]
fn store_dir_is_profile_agnostic_base() {
    let base = std::path::Path::new("/data/Vmux/dev");
    assert_eq!(
        store_dir_for(base, "personal"),
        PathBuf::from("/data/Vmux/dev")
    );
    assert_eq!(
        store_dir_for(base, "gregor"),
        PathBuf::from("/data/Vmux/dev")
    );
}

#[test]
fn is_test_session_reads_env() {
    let prev = std::env::var("VMUX_TEST").ok();
    unsafe { std::env::set_var("VMUX_TEST", "1") };
    assert!(is_test_session());
    unsafe { std::env::remove_var("VMUX_TEST") };
    assert!(!is_test_session());
    if let Some(p) = prev {
        unsafe { std::env::set_var("VMUX_TEST", p) };
    }
}

#[test]
fn display_name_uses_config_or_capitalized_id() {
    assert_eq!(display_name_from(None, "personal", false), "Personal");
    assert_eq!(
        display_name_from(Some("Junichi"), "personal", false),
        "Junichi"
    );
    assert_eq!(
        display_name_from(Some("Junichi"), "personal", true),
        "Personal"
    );
    assert_eq!(display_name_from(Some("  "), "gregor", false), "Gregor");
}

#[test]
fn spaces_root_is_profile_agnostic() {
    let data = std::path::Path::new("/data/Vmux");
    assert_eq!(
        spaces_root_for(data, "personal"),
        PathBuf::from("/data/Vmux/spaces")
    );
    assert_eq!(
        spaces_root_for(data, "gregor"),
        PathBuf::from("/data/Vmux/spaces")
    );
}

#[test]
fn active_profile_name_reads_and_sanitizes_env() {
    let prev = std::env::var("VMUX_PROFILE").ok();
    unsafe { std::env::set_var("VMUX_PROFILE", "Test/X") };
    assert_eq!(active_profile_name(), "test-x");
    unsafe { std::env::remove_var("VMUX_PROFILE") };
    assert_eq!(active_profile_name(), "personal");
    if let Some(p) = prev {
        unsafe { std::env::set_var("VMUX_PROFILE", p) };
    }
}

#[test]
fn data_dir_suffix_maps_each_profile() {
    assert_eq!(data_dir_suffix_for("release"), PathBuf::from("Vmux"));
    assert_eq!(data_dir_suffix_for("local"), PathBuf::from("Vmux"));
    assert_eq!(
        data_dir_suffix_for("dev"),
        PathBuf::from("Vmux").join("dev")
    );
    assert_eq!(
        data_dir_suffix_for("custom"),
        PathBuf::from("Vmux").join("custom"),
    );
}

#[test]
fn local_and_release_share_one_space() {
    assert_eq!(data_dir_suffix_for("local"), data_dir_suffix_for("release"));
}

#[test]
fn test_sessions_use_mock_keychain() {
    assert_eq!(
        cef_keychain_switches_for(true),
        ["use-mock-keychain"].as_slice()
    );
}

#[test]
fn interactive_sessions_use_real_keychain() {
    assert!(cef_keychain_switches_for(false).is_empty());
}

#[test]
fn dev_lives_under_the_release_space() {
    let release = data_dir_suffix_for("release");
    let dev = data_dir_suffix_for("dev");
    assert!(dev.starts_with(&release));
    assert_ne!(dev, release);
    assert_eq!(dev.file_name().unwrap(), "dev");
}

#[test]
fn shared_data_dir_ends_with_profile_suffix() {
    assert!(shared_data_dir().ends_with(data_dir_suffix()));
}

#[test]
fn managed_data_uses_build_agnostic_application_support_root() {
    let shared = shared_data_dir();
    let managed = application_data_dir();
    assert!(shared.starts_with(&managed));
    if matches!(build_profile(), "release" | "local") {
        assert_eq!(shared, managed);
    } else {
        assert_eq!(shared.parent(), Some(managed.as_path()));
    }
}

#[test]
fn space_dir_is_under_vmux_spaces() {
    assert_eq!(
        space_dir_path(std::path::Path::new("/data/Vmux"), "personal", "work"),
        PathBuf::from("/data/Vmux/spaces/work")
    );
}

#[test]
fn migrate_relocates_nested_spaces_and_recording() {
    let home = std::env::temp_dir().join(format!("vmux-migrate-{}", std::process::id()));
    let managed_data = home.join("data/Vmux");
    let data = managed_data.join("dev");
    let _ = std::fs::remove_dir_all(&home);
    let nested_space = home
        .join(".vmux")
        .join("profiles")
        .join("personal")
        .join("spaces")
        .join("space-1");
    std::fs::create_dir_all(&nested_space).unwrap();
    std::fs::write(nested_space.join("space.ron"), b"x").unwrap();
    let legacy_rec = home.join(".vmux").join("recording");
    std::fs::create_dir_all(&legacy_rec).unwrap();
    std::fs::write(legacy_rec.join("a.mp4"), b"y").unwrap();
    let agents = home.join(".vmux/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("registry.json"), b"{}").unwrap();
    let gregor = home.join(".vmux/profiles/gregor/recording");
    std::fs::create_dir_all(&gregor).unwrap();
    std::fs::write(gregor.join("b.mp4"), b"z").unwrap();

    migrate_legacy_personal_layout_in(&home, &data, &managed_data);

    assert!(
        space_dir_path(&data, "personal", "space-1")
            .join("space.ron")
            .exists()
    );
    assert!(
        !home
            .join(".vmux")
            .join("profiles")
            .join("personal")
            .join("spaces")
            .exists()
    );
    assert!(!legacy_rec.exists());
    assert!(recording_dir_for(&data, "personal").join("a.mp4").exists());
    assert!(managed_data.join("agents/registry.json").exists());
    assert!(data.join("profiles/gregor/recording/b.mp4").exists());
    assert!(!home.join(".vmux/profiles").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn migrate_keeps_existing_agnostic_spaces() {
    let home = std::env::temp_dir().join(format!("vmux-migrate-noop-{}", std::process::id()));
    let data = home.join("data/Vmux");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(
        home.join(".vmux")
            .join("profiles")
            .join("personal")
            .join("spaces"),
    )
    .unwrap();
    let target = spaces_root_for(&data, "personal");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"keep").unwrap();

    migrate_legacy_personal_layout_in(&home, &data, &data);

    assert!(target.join("keep.txt").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn cleanup_removes_empty_legacy_space_dirs_and_preserves_files() {
    let home = std::env::temp_dir().join(format!("vmux-prune-{}", std::process::id()));
    let data = home.join("data/Vmux");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(space_dir_path(&data, "personal", "org/empty")).unwrap();
    std::fs::create_dir_all(space_dir_path(&data, "personal", "solo")).unwrap();
    std::fs::create_dir_all(space_dir_path(&data, "personal", "keep")).unwrap();
    std::fs::write(
        space_dir_path(&data, "personal", "keep").join("f.txt"),
        b"x",
    )
    .unwrap();

    prune_empty_legacy_space_dirs_in(&data);

    assert!(!space_dir_path(&data, "personal", "org/empty").exists());
    assert!(!space_dir_path(&data, "personal", "org").exists());
    assert!(!space_dir_path(&data, "personal", "solo").exists());
    assert!(space_dir_path(&data, "personal", "keep").is_dir());
    let _ = std::fs::remove_dir_all(&home);
}
#[test]
fn settings_live_in_dot_vmux_not_data_dir() {
    for candidate in settings_path_candidates() {
        assert!(candidate.starts_with(config_dir()));
        assert!(!candidate.starts_with(shared_data_dir()));
    }
}

#[test]
fn settings_candidates_prefer_per_build_override_then_shared() {
    let base = PathBuf::from("/base");
    assert_eq!(
        settings_candidates_in(&base, None),
        vec![base.join("settings.ron")]
    );
    assert_eq!(
        settings_candidates_in(&base, Some("dev")),
        vec![
            base.join("dev").join("settings.ron"),
            base.join("settings.ron"),
        ]
    );
}
