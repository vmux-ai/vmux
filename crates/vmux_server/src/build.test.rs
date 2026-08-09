use super::*;

#[test]
fn profile_tag_maps_release_flag() {
    assert_eq!(profile_tag(true), "release");
    assert_eq!(profile_tag(false), "debug");
}

#[test]
#[should_panic(expected = "VMUX_SKIP_DX_BUILD cannot be used for release builds")]
fn skip_dx_is_rejected_for_release_builds() {
    assert_skip_dx_allowed(true);
}

#[test]
fn mismatch_true_when_marker_missing() {
    assert!(dist_profile_mismatch(None, true));
    assert!(dist_profile_mismatch(None, false));
}

#[test]
fn mismatch_true_when_marker_differs() {
    assert!(dist_profile_mismatch(Some("debug"), true));
    assert!(dist_profile_mismatch(Some("release"), false));
}

#[test]
fn mismatch_false_when_marker_matches() {
    assert!(!dist_profile_mismatch(Some("release"), true));
    assert!(!dist_profile_mismatch(Some("debug"), false));
    assert!(!dist_profile_mismatch(Some("release\n"), true));
}

#[test]
fn bundle_stamp_detects_corruption() {
    let dist = std::env::temp_dir().join(format!("vmux-bundle-stamp-test-{}", std::process::id()));
    fs::create_dir_all(dist.join("wasm")).unwrap();
    fs::create_dir_all(dist.join("assets")).unwrap();
    fs::write(dist.join("index.html"), "index").unwrap();
    fs::write(dist.join(DIST_PROFILE_MARKER), "debug").unwrap();
    fs::write(dist.join("wasm/app.wasm"), "wasm").unwrap();
    fs::write(dist.join("assets/app.js"), "js").unwrap();

    refresh_bundle_stamp(&dist).unwrap();
    assert!(bundle_stamp_matches(&dist));

    fs::write(dist.join("assets/app.js"), "changed").unwrap();
    assert!(!bundle_stamp_matches(&dist));

    let _ = fs::remove_dir_all(dist);
}

#[test]
fn recursive_copy_creates_destination() {
    let root =
        std::env::temp_dir().join(format!("vmux-recursive-copy-test-{}", std::process::id()));
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("font.woff2"), "font").unwrap();

    copy_dir_recursive(&source, &destination).unwrap();

    assert_eq!(
        fs::read_to_string(destination.join("font.woff2")).unwrap(),
        "font"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replacement_copy_removes_destination_only_files() {
    let root =
        std::env::temp_dir().join(format!("vmux-replacement-copy-test-{}", std::process::id()));
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("current.woff2"), "current").unwrap();
    fs::write(destination.join("stale.woff2"), "stale").unwrap();

    replace_dir_recursive(&source, &destination).unwrap();

    assert!(destination.join("current.woff2").is_file());
    assert!(!destination.join("stale.woff2").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tracks_files_under_extra_directories() {
    let root = std::env::temp_dir().join(format!(
        "vmux-page-builder-track-test-{}",
        std::process::id()
    ));
    let manifest_dir = root.join("crates/vmux_server");
    let layout_src = root.join("crates/vmux_layout/src");
    let nested_src = layout_src.join("nested");
    let terminal_fonts = root.join("crates/vmux_terminal/assets/fonts");
    fs::create_dir_all(manifest_dir.join("assets")).unwrap();
    fs::create_dir_all(manifest_dir.join("src")).unwrap();
    fs::create_dir_all(&nested_src).unwrap();
    fs::create_dir_all(&terminal_fonts).unwrap();
    fs::write(manifest_dir.join("Cargo.toml"), "").unwrap();
    fs::write(manifest_dir.join("Dioxus.toml"), "").unwrap();
    fs::write(manifest_dir.join("assets/index.html"), "").unwrap();
    fs::write(manifest_dir.join("assets/index.css"), "").unwrap();
    fs::write(manifest_dir.join("src/lib.rs"), "").unwrap();
    fs::write(layout_src.join("page.rs"), "").unwrap();
    fs::write(nested_src.join("pane.rs"), "").unwrap();
    fs::write(nested_src.join("pane.css"), "").unwrap();
    fs::write(terminal_fonts.join("terminal.woff2"), "").unwrap();

    let tracked = PageBuilder::new(manifest_dir.clone(), "vmux_server", "vmux_server")
        .track_manifest_rel_paths(&["../vmux_layout/src"])
        .copy_manifest_dir_to_dist("../vmux_terminal/assets/fonts", "assets/fonts")
        .tracked_paths();

    let _ = fs::remove_dir_all(&root);
    assert!(tracked.contains(&manifest_dir.join("../vmux_layout/src/page.rs")));
    assert!(tracked.contains(&manifest_dir.join("../vmux_layout/src/nested/pane.rs")));
    assert!(tracked.contains(&manifest_dir.join("../vmux_layout/src/nested/pane.css")));
    assert!(tracked.contains(&manifest_dir.join("../vmux_terminal/assets/fonts/terminal.woff2")));
}
