//! Verify cargo-packager metadata embeds vmux_service in the bundle and that
//! the main executable name does not case-collide with the vmux CLI.

#[test]
fn packager_binaries_avoid_case_insensitive_collision() {
    let toml = include_str!("../Cargo.toml");
    assert!(
        toml.contains(r#"{ path = "vmux_desktop", main = true }"#),
        "main executable must be vmux_desktop so it does not case-collide with the vmux CLI"
    );
    assert!(
        toml.contains(r#"{ path = "vmux" }"#),
        "packager metadata must install the CLI executable as vmux"
    );
    assert!(
        toml.contains(r#"{ path = "Vmux Service" }"#),
        "packager metadata must install the service executable as Vmux Service"
    );
    assert!(
        !toml.contains(r#"{ path = "Vmux", main = true }"#),
        "main executable must not be Vmux (case-insensitive clash with the vmux CLI)"
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
        "before-packaging-command must prepare the bundled binaries: {line}"
    );
    let script = include_str!("../../../scripts/build-package-binaries.sh");
    assert!(script.contains("-p vmux_desktop"));
    assert!(script.contains("-p vmux_cli"));
    assert!(script.contains(r#"target/release/Vmux Service"#));
    assert!(
        !script.contains("target/release/vmux_desktop target/release/Vmux"),
        "must not copy the GUI binary over the vmux name (case-insensitive clash)"
    );
}

#[test]
fn macos_bundle_layout_uses_collision_safe_names() {
    let layout_script = include_str!("../../../scripts/test-bundle-layout.sh");
    let required_block = layout_script.split("FORBIDDEN=(").next().unwrap();
    assert!(required_block.contains("Contents/MacOS/vmux_desktop"));
    assert!(required_block.contains("Contents/MacOS/vmux"));
    assert!(required_block.contains(
        "Contents/Frameworks/vmux_desktop Helper.app/Contents/MacOS/vmux_desktop Helper"
    ));
    assert!(
        required_block
            .contains("Contents/Frameworks/vmux_desktop Helper.app/Contents/Resources/Vmux.icns")
    );
    assert!(
        required_block
            .contains("Contents/Library/LoginItems/Vmux Service.app/Contents/MacOS/Vmux Service")
    );
    assert!(
        required_block
            .contains("Contents/Library/LoginItems/Vmux Service.app/Contents/Resources/Vmux.icns")
    );
    assert!(!required_block.contains("Contents/Frameworks/Vmux Helper.app/"));
}

#[test]
fn cef_injection_uses_named_helper_base_and_icon() {
    let inject_script = include_str!("../../../scripts/inject-cef.sh");
    assert!(inject_script.contains("--bin-name vmux_desktop"));
    assert!(inject_script.contains("vmux_desktop Helper.app"));
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
        after_key
            .trim_start()
            .starts_with("<string>vmux_desktop</string>"),
        "CFBundleExecutable must be vmux_desktop to match the bundled main binary"
    );
}
