//! Verify cargo-packager metadata embeds vmux_service in the bundle.

#[test]
fn packager_binaries_use_user_facing_names() {
    let toml = include_str!("../Cargo.toml");
    assert!(
        toml.contains(r#"{ path = "Vmux", main = true }"#),
        "packager metadata must install the app executable as Vmux"
    );
    assert!(
        toml.contains(r#"{ path = "Vmux Service" }"#),
        "packager metadata must install the service executable as Vmux Service"
    );
    assert!(
        !toml.contains(r#"{ path = "vmux_desktop", main = true }"#),
        "packager metadata must not expose vmux_desktop in the app bundle"
    );
    assert!(
        !toml.contains(r#"{ path = "vmux_service" }"#),
        "packager metadata must not expose vmux_service in the app bundle"
    );
}

#[test]
fn before_packaging_command_prepares_named_binaries() {
    let toml = include_str!("../Cargo.toml");
    let line = toml
        .lines()
        .find(|l| l.starts_with("before-packaging-command"))
        .expect("before-packaging-command line present");
    assert!(
        line.contains("scripts/build-package-binaries.sh"),
        "before-packaging-command must prepare user-facing binary names: {line}"
    );
    let script = include_str!("../../../scripts/build-package-binaries.sh");
    assert!(script.contains("target/release/vmux_desktop"));
    assert!(script.contains("target/release/Vmux"));
    assert!(script.contains("target/release/vmux_service"));
    assert!(script.contains("target/release/Vmux Service"));
}

#[test]
fn macos_bundle_scripts_expect_user_facing_helper_names_and_icons() {
    let layout_script = include_str!("../../../scripts/test-bundle-layout.sh");
    let required_block = layout_script.split("FORBIDDEN=(").next().unwrap();
    assert!(required_block.contains("Contents/MacOS/Vmux"));
    assert!(
        required_block.contains("Contents/Frameworks/Vmux Helper.app/Contents/MacOS/Vmux Helper")
    );
    assert!(
        required_block.contains("Contents/Frameworks/Vmux Helper.app/Contents/Resources/Vmux.icns")
    );
    assert!(
        required_block
            .contains("Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service")
    );
    assert!(
        required_block
            .contains("Contents/Library/LoginItems/Vmux Service.app/Contents/Resources/Vmux.icns")
    );
    assert!(!required_block.contains("Contents/MacOS/vmux_desktop"));
    assert!(!required_block.contains("Contents/MacOS/vmux_service"));
    assert!(!required_block.contains("vmux_desktop Helper.app"));
}

#[test]
fn cef_injection_uses_named_helper_base_and_icon() {
    let inject_script = include_str!("../../../scripts/inject-cef.sh");
    assert!(inject_script.contains("--bin-name Vmux"));
    assert!(inject_script.contains("Vmux Helper.app"));
    assert!(inject_script.contains("CFBundleIconFile"));
    assert!(inject_script.contains("Vmux.icns"));
}

#[test]
fn signing_includes_service_helper_app() {
    let signing_script = include_str!("../../../scripts/sign-and-notarize.sh");
    assert!(signing_script.contains("$APP_BUNDLE/Contents/Library"));
    assert!(signing_script.contains("Vmux Service"));
    assert!(signing_script.contains("ai.vmux.service%s"));
}

#[test]
fn generated_info_plist_uses_named_executable() {
    let info_plist = include_str!("../../../packaging/macos/Info.plist");
    let after_key = info_plist
        .split("<key>CFBundleExecutable</key>")
        .nth(1)
        .expect("CFBundleExecutable key");
    assert!(
        after_key.trim_start().starts_with("<string>Vmux</string>"),
        "CFBundleExecutable must be Vmux so helper processes are named Vmux Helper"
    );
}
