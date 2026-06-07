// Lock-in tests for the v0.0.4 vibe-keychain fix.
// Verify that runtime keychain ACL mutation is gone, that local + debug builds
// share a stable codesigning identity, and that the local app and debug binary
// share the bundle identifier so Chromium safe-storage ACL covers both flows.

#[test]
fn startup_does_not_mutate_chromium_safe_storage_acl() {
    let source = include_str!("../src/main.rs");
    let symbol = ["ensure_chromium_safe_storage", "_acl("].concat();

    assert!(!source.contains(&symbol));
}

#[test]
fn dev_target_signs_then_runs_debug_binary() {
    let makefile = include_str!("../../../Makefile");

    assert!(makefile.contains(".DEFAULT_GOAL := dev"));
    assert!(
        makefile.contains("dev: ensure-mac-deps ensure-codesign-deps install-debug-render-process")
    );
    assert!(makefile.contains("./scripts/sign-dev-mac.sh"));
    assert!(makefile.contains("DYLD_LIBRARY_PATH=\"$$dylib_path\" ./target/debug/vmux_desktop"));
    assert!(makefile.contains("identity=\"$$(./scripts/ensure-local-codesign-identity.sh)\" &&"));
    assert!(!makefile.contains("run-mac:"));
    assert!(!makefile.contains("build-mac-debug"));
    assert!(!makefile.contains("sign-mac-debug"));
    assert!(!makefile.contains("package-local-mac"));
    assert!(!makefile.contains("package-release-mac"));
}

#[test]
fn dev_target_keeps_service_out_of_desktop_dynamic_linking_build() {
    let makefile = include_str!("../../../Makefile");

    assert!(
        makefile.contains("env -u CEF_PATH \"$(CARGO_BIN)\" build -p vmux_service -p vmux_cli")
    );
    assert!(
        makefile.contains("env -u CEF_PATH \"$(CARGO_BIN)\" build -p vmux_desktop --features dev")
    );
    assert!(!makefile.contains("build -p vmux_desktop -p vmux_cli -p vmux_service --features dev"));
}

#[test]
fn dev_target_stops_existing_debug_desktop_before_cef_initialize() {
    let makefile = include_str!("../../../Makefile");
    let stop_idx = makefile
        .find("target/debug/vmux_desktop")
        .expect("debug desktop stop");
    let run_idx = makefile
        .find("exec env -u CEF_PATH")
        .expect("debug desktop run");

    assert!(makefile.contains("pkill -f \"target/debug/vmux_desktop\""));
    assert!(makefile.contains("pkill -f \"bevy_cef_debug_render_process\""));
    assert!(stop_idx < run_idx);
}

#[test]
fn local_package_uses_per_sha_bundle_name() {
    let package_script = include_str!("../../../scripts/package.sh");

    assert!(package_script.contains("PRODUCT_NAME=\"Vmux ($SHA)\""));
    assert!(package_script.contains("BUNDLE_ID=\"ai.vmux.desktop.$SHA\""));
    assert!(!package_script.contains("PRODUCT_NAME=\"Vmux Local\""));
}

#[test]
fn local_package_only_builds_app_bundle() {
    let package_script = include_str!("../../../scripts/package.sh");

    assert!(package_script.contains("cargo packager --release --formats app"));
    assert!(package_script.contains("if [[ \"$PROFILE\" == \"local\" ]]"));
}

#[test]
fn cef_injection_uses_ci_cached_framework_path() {
    let inject_script = include_str!("../../../scripts/inject-cef.sh");

    assert!(inject_script.contains("--cef-framework \"$CEF_FRAMEWORK\""));
    assert!(inject_script.contains("CEF_FRAMEWORK=\"${CEF_FRAMEWORK:-${HOME}/.local/share/Chromium Embedded Framework.framework}\""));
}

#[test]
fn local_signing_uses_stable_codesigning_identity() {
    let signing_script = include_str!("../../../scripts/ensure-local-codesign-identity.sh");

    assert!(signing_script.contains("Vmux Dev"));
    assert!(!signing_script.contains("Vmux Development"));
    assert!(!signing_script.contains("Vmux Local Development"));
    assert!(signing_script.contains("awk -F'\"'"));
    assert!(signing_script.contains("security list-keychains -d user -s"));
    assert!(signing_script.contains("security import"));
    assert!(signing_script.contains("-keypbe PBE-SHA1-3DES"));
    assert!(signing_script.contains("-certpbe PBE-SHA1-3DES"));
    assert!(signing_script.contains("-macalg sha1"));
    assert!(signing_script.contains("security add-trusted-cert"));
    assert!(signing_script.contains("security set-key-partition-list"));
    assert!(signing_script.contains("could not pre-authorize codesign key access"));
    assert!(signing_script.contains("security find-identity -v -p codesigning"));
}

#[test]
fn dev_signing_uses_default_keychain_directly() {
    let signing_script = include_str!("../../../scripts/sign-dev-mac.sh");

    assert!(signing_script.contains("CODESIGN_KEYCHAIN"));
    assert!(signing_script.contains("--keychain"));
}

#[test]
fn dev_and_local_use_distinct_bundle_identifiers() {
    let signing_script = include_str!("../../../scripts/sign-dev-mac.sh");
    let package_script = include_str!("../../../scripts/package.sh");

    assert!(signing_script.contains("APP_IDENTIFIER=\"ai.vmux.desktop.dev\""));
    assert!(!signing_script.contains("ai.vmux.desktop.local"));
    assert!(package_script.contains("BUNDLE_ID=\"ai.vmux.desktop.$SHA\""));
}

fn workspace_bevy_spec() -> &'static str {
    let manifest = include_str!("../../../Cargo.toml");
    let deps = manifest
        .split("[workspace.dependencies]")
        .nth(1)
        .expect("workspace dependencies block")
        .split("\n\n")
        .next()
        .expect("workspace dependencies content");
    let start = deps.find("bevy = {").expect("workspace bevy dependency");
    let rest = &deps[start..];
    let end = rest
        .find("\nbevy_ecs =")
        .expect("dependency after workspace bevy");

    &rest[..end]
}

fn workspace_bevy_features() -> std::collections::BTreeSet<&'static str> {
    workspace_bevy_spec()
        .split("features = [")
        .nth(1)
        .expect("workspace bevy features")
        .split(']')
        .next()
        .expect("workspace bevy features content")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches(',').trim_matches('"'))
        .collect()
}

#[test]
fn workspace_bevy_uses_explicit_feature_allowlist() {
    let spec = workspace_bevy_spec();

    assert!(spec.contains("default-features = false"));
    assert!(!spec.contains("default-features = true"));

    let expected = [
        "std",
        "multi_threaded",
        "async_executor",
        "bevy_animation",
        "bevy_asset",
        "bevy_log",
        "bevy_winit",
        "bevy_window",
        "bevy_render",
        "bevy_core_pipeline",
        "bevy_pbr",
        "bevy_post_process",
        "bevy_mesh",
        "bevy_sprite",
        "bevy_ui",
        "bevy_ui_render",
        "bevy_image",
        "bevy_scene",
        "bevy_state",
        "bevy_input_focus",
        "bevy_picking",
        "mesh_picking",
        "sprite_picking",
        "ui_picking",
        "custom_cursor",
        "reflect_auto_register",
        "default_font",
        "bevy_camera_controller",
        "free_camera",
        "https",
        "x11",
        "wayland",
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(workspace_bevy_features(), expected);
}

#[test]
fn workspace_bevy_does_not_enable_removed_heavy_features() {
    let features = workspace_bevy_features();

    for feature in [
        "audio",
        "bevy_audio",
        "vorbis",
        "gamepad",
        "bevy_gilrs",
        "bevy_gltf",
        "gltf_animation",
        "morph_animation",
        "ktx2",
        "smaa_luts",
        "tonemapping_luts",
        "sysinfo_plugin",
        "webgl2",
        "default_platform",
        "bevy_text",
        "png",
    ] {
        assert!(
            !features.contains(feature),
            "workspace bevy dependency should not enable feature {feature}"
        );
    }
}

#[test]
fn patched_bevy_cef_does_not_reenable_bevy_default_bundles() {
    fn dependency_block(manifest: &'static str, dependency: &str) -> &'static str {
        let start = manifest
            .find(dependency)
            .unwrap_or_else(|| panic!("dependency block {dependency}"));
        let rest = &manifest[start..];
        let end = rest.find("\n\n").unwrap_or(rest.len());

        &rest[..end]
    }

    for block in [
        dependency_block(
            include_str!("../../../patches/bevy_cef-0.5.2/Cargo.toml"),
            "[dependencies.bevy]",
        ),
        dependency_block(
            include_str!("../../../patches/bevy_cef_core-0.5.2/Cargo.toml"),
            "[dependencies.bevy]",
        ),
        dependency_block(
            include_str!("../../../patches/bevy_cef_core-0.5.2/Cargo.toml"),
            "[dependencies.bevy_winit]",
        ),
    ] {
        assert!(!block.contains("\"picking\""));
        assert!(!block.contains("default-features = true"));
    }
}

#[test]
fn patched_bevy_cef_core_keeps_required_pointer_input_feature() {
    let manifest = include_str!("../../../patches/bevy_cef_core-0.5.2/Cargo.toml");
    let start = manifest
        .find("[dependencies.bevy]")
        .expect("bevy_cef_core bevy dependency");
    let rest = &manifest[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    let bevy_block = &rest[..end];

    assert!(bevy_block.contains("\"bevy_picking\""));
}

#[test]
fn workspace_bevy_winit_does_not_reenable_default_platform() {
    let manifest = include_str!("../../../Cargo.toml");

    assert!(
        manifest.contains("bevy_winit = { version = \"0.19.0-rc.2\", default-features = false")
    );
}
