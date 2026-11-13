use std::{fs, os::unix::fs::PermissionsExt, process::Command};

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

#[cfg(unix)]
#[test]
fn package_binary_paths_honor_target_dir_and_triple() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cargo = temp.path().join("cargo");
    let target = temp.path().join("custom target");
    let cache = temp.path().join("cef sdk");
    let log = temp.path().join("cargo.log");
    let dist = temp.path().join("web dist");
    fs::create_dir_all(dist.join("assets/favicons")).expect("create favicon assets");
    fs::create_dir_all(dist.join("assets/tool-logos")).expect("create tool logo assets");
    fs::create_dir_all(dist.join("wasm")).expect("create wasm assets");
    fs::write(dist.join("index.html"), "").expect("write index");
    fs::write(dist.join(".dx-profile"), "release").expect("write profile");
    fs::write(dist.join("assets/app.js"), "").expect("write js");
    for asset in [
        "assets/index.css",
        "assets/theme.css",
        "assets/favicons/tools.svg",
        "assets/tool-logos/acp.png",
        "assets/tool-logos/homebrew.svg",
        "assets/tool-logos/mcp.png",
        "assets/tool-logos/npm.svg",
    ] {
        fs::write(dist.join(asset), "").expect("write required web asset");
    }
    fs::write(dist.join("wasm/app.wasm"), "").expect("write wasm");
    fs::write(
        dist.join(".bundle-stamp"),
        concat!(
            "a4d451ec23463726f72c43d64c710968f6b602cd653b4de8adee1b556240a829  .dx-profile\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/app.js\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/favicons/tools.svg\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/index.css\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/theme.css\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/tool-logos/acp.png\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/tool-logos/homebrew.svg\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/tool-logos/mcp.png\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  assets/tool-logos/npm.svg\n",
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
package=""
while [[ "$#" -gt 0 ]]; do
    if [[ "$1" == "--profile" ]]; then
        profile="$2"
        shift 2
    elif [[ "$1" == "-p" ]]; then
        package="$2"
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
    case "$package" in
        vmux_cli) printf 'cli' > "$out/vmux" ;;
        vmux_desktop) printf 'desktop' > "$out/vmux_desktop" ;;
        vmux_app) printf 'service' > "$out/vmux_service" ;;
        *) exit 1 ;;
    esac
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
        .join("../../../scripts/build-package-binaries.sh");
    let status = Command::new("bash")
        .arg(&script)
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
    assert_eq!(fs::read_to_string(release.join("vmux")).unwrap(), "cli");
    assert_eq!(
        fs::read_to_string(release.join("vmux_desktop")).unwrap(),
        "desktop"
    );
    assert_eq!(
        fs::read_to_string(release.join("Vmux Service")).unwrap(),
        "service"
    );
    let status = Command::new("bash")
        .arg(&script)
        .env("CI", "true")
        .env("CARGO_BIN", &cargo)
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_BUILD_TARGET", "aarch64-apple-darwin")
        .env("VMUX_CEF_SDK_CACHE", &cache)
        .env("VMUX_WEB_BUNDLE_DIST", &dist)
        .env("FAKE_CARGO_LOG", &log)
        .env("VMUX_HOST_NAME", "Atlas")
        .env("VMUX_HOST_NAMESPACE", "org.example.atlas")
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        fs::read_to_string(release.join("Atlas Service")).unwrap(),
        "service"
    );
    assert_eq!(
        fs::read_to_string(release.join("Vmux Service")).unwrap(),
        "service"
    );
    let cargo_log = fs::read_to_string(log).expect("read cargo log");
    let cef_paths = cargo_log.lines().collect::<Vec<_>>();
    let expected = cache.display().to_string();
    assert!(cef_paths.len() >= 2);
    assert!(cef_paths.iter().all(|path| *path == expected));
}

#[test]
fn macos_bundle_layout_uses_collision_safe_names() {
    let layout_script = include_str!("../../../../scripts/test-bundle-layout.sh");
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
    let inject_script = include_str!("../../../../scripts/inject-cef.sh");
    assert!(inject_script.contains("--bin-name vmux_desktop"));
    assert!(inject_script.contains("vmux_desktop Helper.app"));
    assert!(inject_script.contains("CFBundleIconFile"));
    assert!(inject_script.contains("Vmux.icns"));
}

#[test]
fn generated_info_plist_uses_named_executable() {
    let info_plist = include_str!("../../../../packaging/macos/Info.plist");
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
