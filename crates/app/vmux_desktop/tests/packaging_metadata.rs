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
    assert!(script.contains(r#"$release_dir/Vmux Service"#));
    assert!(
        !script.contains("target/release/vmux_desktop target/release/Vmux"),
        "must not copy the GUI binary over the vmux name (case-insensitive clash)"
    );
}

#[cfg(unix)]
#[test]
fn package_binary_paths_honor_target_dir_and_triple() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    let temp = tempfile::tempdir().expect("tempdir");
    let cargo = temp.path().join("cargo");
    let target = temp.path().join("custom target");
    let cache = temp.path().join("cef sdk");
    let log = temp.path().join("cargo.log");
    let dist = temp.path().join("web dist");
    fs::create_dir_all(dist.join("assets")).expect("create web assets");
    fs::create_dir_all(dist.join("wasm")).expect("create wasm assets");
    fs::write(dist.join("index.html"), "").expect("write index");
    fs::write(dist.join(".dx-profile"), "release").expect("write profile");
    fs::write(dist.join("assets/app.js"), "").expect("write js");
    fs::write(dist.join("wasm/app.wasm"), "").expect("write wasm");
    fs::write(
        dist.join(".bundle-stamp"),
        concat!(
            "a4d451ec23463726f72c43d64c710968f6b602cd653b4de8adee1b556240a829  .dx-profile\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/app.js\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  index.html\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  wasm/app.wasm\n",
        ),
    )
    .expect("write bundle stamp");
    fs::write(
        &cargo,
        r#"#!/usr/bin/env bash
set -euo pipefail
profile=release
while [[ "$#" -gt 0 ]]; do
    if [[ "$1" == "--profile" ]]; then
        profile="$2"
        shift 2
    else
        shift
    fi
done
out="$CARGO_TARGET_DIR/${CARGO_BUILD_TARGET:-}/$profile"
mkdir -p "$out"
if [[ "$profile" == "cef-helper" ]]; then
    : > "$out/bevy_cef_debug_render_process"
else
    : > "$out/vmux_service"
fi
printf '%s\n' "${CEF_PATH:-}" >> "$FAKE_CARGO_LOG"
"#,
    )
    .expect("write fake cargo");
    let mut permissions = fs::metadata(&cargo)
        .expect("fake cargo metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&cargo, permissions).expect("make fake cargo executable");

    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/build-package-binaries.sh");
    let status = Command::new("bash")
        .arg(script)
        .env("CI", "true")
        .env("CARGO_BIN", &cargo)
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_BUILD_TARGET", "aarch64-apple-darwin")
        .env("VMUX_CEF_SDK_CACHE", &cache)
        .env("VMUX_WEB_BUNDLE_DIST", &dist)
        .env("FAKE_CARGO_LOG", &log)
        .status()
        .expect("run package binary build");
    assert!(status.success());

    let release = target.join("aarch64-apple-darwin/release");
    assert!(release.join("bevy_cef_debug_render_process").is_file());
    assert!(release.join("Vmux Service").is_file());
    let cargo_log = fs::read_to_string(log).expect("read cargo log");
    let cef_paths = cargo_log.lines().collect::<Vec<_>>();
    let expected = cache.display().to_string();
    assert!(cef_paths.len() >= 2);
    assert!(cef_paths.iter().all(|path| *path == expected));
}

#[test]
fn packaging_scripts_share_resolved_release_dir() {
    let paths = include_str!("../../../scripts/cargo-target-paths.sh");
    let build = include_str!("../../../scripts/build-package-binaries.sh");
    let package = include_str!("../../../scripts/package.sh");
    let inject = include_str!("../../../scripts/inject-cef.sh");
    let before_each = include_str!("../../../scripts/before-each-package.sh");
    let signing = include_str!("../../../scripts/sign-and-notarize.sh");

    assert!(paths.contains("CARGO_TARGET_DIR"));
    assert!(paths.contains("CARGO_BUILD_TARGET"));
    assert!(build.contains("vmux_cargo_profile_dir \"$ROOT\" release"));
    assert!(package.contains("VMUX_CARGO_RELEASE_DIR"));
    assert!(package.contains("packager_args+=(--target \"$CARGO_BUILD_TARGET\")"));
    assert!(inject.contains("VMUX_CARGO_RELEASE_DIR"));
    assert!(inject.contains("$RELEASE_DIR/bevy_cef_debug_render_process"));
    assert!(before_each.contains("$release_dir/Vmux.app"));
    assert!(signing.contains("$(dirname \"$APP_BUNDLE\")"));
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
