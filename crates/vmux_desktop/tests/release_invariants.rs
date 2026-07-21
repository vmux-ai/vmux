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
        makefile.contains(
            "dev: ensure-native-deps $(DEV_WEB_TARGET) ensure-codesign-deps install-debug-render-process"
        )
    );
    assert!(makefile.contains("./scripts/sign-dev-mac.sh"));
    assert!(makefile.contains(
        "DYLD_LIBRARY_PATH=\"$$dylib_path\" VMUX_PROFILE=\"$(VMUX_PROFILE)\" VMUX_TEST=\"$(VMUX_TEST)\" ./target/debug/vmux_desktop"
    ));
    assert!(makefile.contains("identity=\"$$(./scripts/ensure-local-codesign-identity.sh)\" &&"));
    assert!(!makefile.contains("run-mac:"));
    assert!(!makefile.contains("build-mac-debug"));
    assert!(!makefile.contains("sign-mac-debug"));
    assert!(!makefile.contains("package-local-mac"));
    assert!(!makefile.contains("package-release-mac"));
}

#[test]
fn test_app_marks_test_session() {
    let makefile = include_str!("../../../Makefile");
    assert!(makefile.contains("test-app:"));
    assert!(makefile.contains("$(MAKE) dev VMUX_PROFILE=gregor VMUX_TEST=1"));
}

#[test]
fn dev_target_keeps_service_out_of_desktop_dynamic_linking_build() {
    let makefile = include_str!("../../../Makefile");

    assert!(makefile.contains("$(CARGO_WITH_CEF_CACHE) build -p vmux_service -p vmux_cli"));
    assert!(
        makefile.contains("$(CARGO_WITH_CEF_CACHE) build -p vmux_desktop $(VMUX_DESKTOP_FEATURES)")
    );
    assert!(
        makefile.contains("VMUX_DESKTOP_FEATURES ?= --no-default-features --features dev,full")
    );
    assert!(makefile.contains(
        "$(MAKE) dev VMUX_BUILD_WEB=0 VMUX_DESKTOP_FEATURES=\"--no-default-features --features dev,full\""
    ));
    assert!(makefile.contains("dev-player:"));
    assert!(!makefile.contains("build -p vmux_desktop -p vmux_cli -p vmux_service --features dev"));
}

#[test]
fn local_cargo_builds_share_cef_sdk_and_sccache() {
    let makefile = include_str!("../../../Makefile");
    let wrapper = include_str!("../../../scripts/cargo-with-cef-cache.sh");

    assert!(makefile.contains("./scripts/cargo-with-cef-cache.sh"));
    assert!(wrapper.contains("seed-worktree-target.sh\" --if-needed"));
    assert!(wrapper.contains("VMUX_CEF_SDK_CACHE"));
    assert!(wrapper.contains("Library/Caches/Vmux/cef-sdk"));
    assert!(wrapper.contains("CEF_PATH=\"$cef_cache\""));
    assert!(wrapper.contains("command -v sccache"));
    assert!(wrapper.contains("VMUX_DISABLE_SCCACHE"));
    assert!(wrapper.contains("building without cache"));
    assert!(wrapper.contains("CMAKE_C_COMPILER_LAUNCHER"));
    assert!(wrapper.contains("CMAKE_CXX_COMPILER_LAUNCHER"));
}

#[cfg(unix)]
#[test]
fn cargo_cache_wrapper_allows_nested_package_builds() {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, process::CommandExt},
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let target = temp.path().join("target");
    let locks = temp.path().join("locks");
    let cache = temp.path().join("cef-cache");
    let cargo = temp.path().join("cargo");
    let rustc = temp.path().join("rustc");
    let log = temp.path().join("cargo.log");
    fs::create_dir_all(&target).expect("create target");
    fs::write(
        &cargo,
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-V" ]]; then
    echo "cargo 1.0.0"
    exit 0
fi
printf '%s\n' "$*" >> "$FAKE_CARGO_LOG"
if [[ "${1:-}" == "packager" ]]; then
    "$CARGO_WRAPPER" build -p nested-package-build
fi
"#,
    )
    .expect("write fake cargo");
    fs::write(
        &rustc,
        "#!/usr/bin/env bash\necho 'rustc 1.0.0'\necho 'host: test-host'\n",
    )
    .expect("write fake rustc");
    for executable in [&cargo, &rustc] {
        let mut permissions = fs::metadata(executable)
            .expect("fake executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).expect("make fake executable runnable");
    }

    let wrapper = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/cargo-with-cef-cache.sh");
    let mut command = Command::new("bash");
    command
        .arg(&wrapper)
        .arg("packager")
        .env_remove("CI")
        .env_remove("VMUX_TARGET_LOCK_TARGET")
        .env_remove("VMUX_TARGET_LOCK_OWNER_PID")
        .env("CARGO_BIN", &cargo)
        .env("RUSTC", &rustc)
        .env("CARGO_TARGET_DIR", &target)
        .env("VMUX_TARGET_LOCK_ROOT", &locks)
        .env("VMUX_CEF_SDK_CACHE", &cache)
        .env("VMUX_DISABLE_SCCACHE", "1")
        .env("CARGO_WRAPPER", &wrapper)
        .env("FAKE_CARGO_LOG", &log)
        .process_group(0);
    let mut child = command.spawn().expect("run nested cargo wrapper");
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll nested cargo wrapper") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = Command::new("kill")
                .arg("-TERM")
                .arg(format!("-{}", child.id()))
                .status();
            let _ = child.wait();
            panic!("nested cargo wrapper deadlocked on the target cache lock");
        }
        thread::sleep(Duration::from_millis(100));
    };

    assert!(status.success());
    assert_eq!(
        fs::read_to_string(log).expect("read fake cargo log"),
        "packager\nbuild -p nested-package-build\n"
    );
}

#[test]
fn cef_wheel_forwarding_rejects_invalid_events() {
    let source =
        include_str!("../../../patches/bevy_cef_core-0.5.2/src/browser_process/browsers.rs");
    let sprite_source =
        include_str!("../../../patches/bevy_cef-0.5.2/src/webview/webview_sprite.rs");

    assert!(source.contains("fn cef_mouse_wheel_event"));
    assert!(source.contains("!position.is_finite() || !delta.is_finite()"));
    assert!(source.contains("delta_x == 0 && delta_y == 0"));
    assert!(source.contains("MAX_CEF_WHEEL_DELTA"));
    assert!(sprite_source.contains("With<CefPointerTarget>"));
    assert!(sprite_source.contains("Without<WebviewWindowed>"));
    assert!(sprite_source.contains("let use_targets = webviews_targeted.iter().next().is_some()"));
}

#[test]
fn bookmark_changes_save_without_a_debounce_window() {
    let source = include_str!("../src/bookmark_persistence.rs");

    assert!(!source.contains("Timer::from_seconds"));
    assert!(source.contains("PostUpdate"));
    assert!(source.contains("migrate_legacy_bookmark_order"));
    assert!(source.contains("mark_bookmarks_dirty"));
    assert!(source.contains("autosave_bookmarks"));
    assert!(source.contains("BookmarkOrder"));
}

#[test]
fn worktree_target_seed_uses_copy_on_write_and_relocates_cef_cmake_state() {
    let makefile = include_str!("../../../Makefile");
    let script = include_str!("../../../scripts/seed-worktree-target.sh");

    assert!(makefile.contains("seed-target:"));
    assert!(script.contains("git rev-parse --path-format=absolute --git-common-dir"));
    assert!(script.contains("--if-needed"));
    assert!(script.contains("cp -cR"));
    assert!(script.contains("cp --reflink=always -a"));
    assert!(script.contains("relocate-cef-target.sh"));
    assert!(!script.contains("-path '*/build/cef-dll-sys-*' -o"));
}

#[test]
fn cef_target_relocator_rewrites_only_cef_build_state() {
    use std::{fs, process::Command};

    let temp = tempfile::tempdir().expect("tempdir");
    let staging = temp.path().join("target");
    let source = temp.path().join("source-target");
    let original = temp.path().join("original-target");
    let destination = temp.path().join("destination-target");
    let cmake = staging.join("debug/build/cef-dll-sys-test/out/build/CMakeCache.txt");
    let fingerprint = staging
        .join("debug/.fingerprint/cef-dll-sys-test/run-build-script-build-script-build.json");
    let dep = staging.join("debug/deps/cef_dll_sys-test.d");
    let unrelated = staging.join("debug/build/other/output");

    for path in [&cmake, &fingerprint, &dep] {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, format!("{}\n", original.display())).expect("write fixture");
    }
    fs::write(
        &cmake,
        format!(
            "CMAKE_CACHEFILE_DIR:INTERNAL={}/debug/build/cef-dll-sys-test/out/build\n",
            original.display()
        ),
    )
    .expect("write CMake cache fixture");
    fs::create_dir_all(unrelated.parent().expect("parent")).expect("create unrelated parent");
    fs::write(&unrelated, format!("{}\n", source.display())).expect("write unrelated fixture");

    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/relocate-cef-target.sh");
    let status = Command::new("bash")
        .arg(script)
        .arg(&staging)
        .arg(&source)
        .arg(&destination)
        .status()
        .expect("run CEF target relocator");
    assert!(status.success());

    for path in [&cmake, &fingerprint, &dep] {
        assert!(
            fs::read_to_string(path)
                .expect("read relocated fixture")
                .contains(&destination.display().to_string())
        );
    }
    assert_eq!(
        fs::read_to_string(unrelated).expect("read unrelated fixture"),
        format!("{}\n", source.display())
    );
}

#[test]
fn debug_render_process_install_uses_fingerprint_cache() {
    let makefile = include_str!("../../../Makefile");
    let script = include_str!("../../../scripts/install-debug-render-process.sh");

    assert!(makefile.contains("./scripts/install-debug-render-process.sh"));
    assert!(!makefile.contains(
        "$(CARGO_WITH_CEF_CACHE) build -p bevy_cef_debug_render_process --features debug"
    ));
    assert!(script.contains("VMUX_CEF_HELPER_CACHE"));
    assert!(script.contains("hash-object --stdin"));
    assert!(script.contains("CEF debug render process up to date"));
    assert!(script.contains("Installed cached CEF debug render process"));
}

#[test]
fn package_builds_cef_helper_separately_without_lto() {
    let manifest = include_str!("../../../Cargo.toml");
    let core_manifest = include_str!("../../../patches/bevy_cef_core-0.5.2/Cargo.toml");
    let core_source = include_str!("../../../patches/bevy_cef_core-0.5.2/src/lib.rs");
    let handler = include_str!(
        "../../../patches/bevy_cef_core-0.5.2/src/render_process/render_process_handler.rs"
    );
    let helper_manifest =
        include_str!("../../../patches/bevy_cef_debug_render_process-0.5.2/Cargo.toml");
    let script = include_str!("../../../scripts/build-package-binaries.sh");

    assert!(script.contains("build -p vmux_cli --release"));
    assert!(script.contains(
        "build -p vmux_desktop -p vmux_service --release --features vmux_desktop/package"
    ));
    assert!(!script.contains("build -p vmux_desktop -p vmux_cli -p vmux_service --release"));
    assert!(script.contains("build -p bevy_cef_debug_render_process --profile cef-helper"));
    assert!(script.contains("$helper_dir/bevy_cef_debug_render_process"));
    assert!(script.contains("$release_dir/bevy_cef_debug_render_process"));
    assert!(!script.contains("target/cef-helper"));
    assert!(!script.contains("-p vmux_service -p bevy_cef_debug_render_process --release"));
    assert!(manifest.contains("[profile.cef-helper]\ninherits = \"release\"\nlto = \"off\""));
    assert!(core_manifest.contains("default = [\"browser-process\"]"));
    for dependency in [
        "dep:async-channel",
        "dep:bevy",
        "dep:bevy_remote",
        "dep:bevy_winit",
        "dep:raw-window-handle",
        "dep:winit",
        "cef/accelerated_osr",
    ] {
        assert!(core_manifest.contains(dependency));
    }
    assert!(core_source.contains("#[cfg(feature = \"browser-process\")]\nmod browser_process;"));
    assert!(helper_manifest.contains("default-features = false"));
    assert!(helper_manifest.contains("cef = { version = \"148.2.0\", default-features = false }"));
    assert!(!handler.contains("use bevy::"));
    assert!(!handler.contains("use bevy_remote::"));
}

#[test]
fn macos_ci_shares_cef_sdk_without_installing_unused_render_process() {
    let workflow = include_str!("../../../.github/workflows/ci.yml");

    assert!(workflow.contains("VMUX_CEF_SDK_CACHE: ${{ github.workspace }}/.cache/cef-sdk"));
    assert!(!workflow.contains("- name: Cache CEF SDK"));
    assert!(!workflow.contains("cargo install bevy_cef_render_process"));
}

#[test]
fn cef_debug_loader_requires_debug_feature() {
    let core_source = include_str!("../../../patches/bevy_cef_core-0.5.2/src/lib.rs");
    let feature_gate = "#[cfg(all(target_os = \"macos\", feature = \"debug\"))]";

    assert_eq!(core_source.matches(feature_gate).count(), 2);
}

#[test]
fn dev_target_stops_existing_debug_desktop_before_cef_initialize() {
    let makefile = include_str!("../../../Makefile");
    let stop_idx = makefile
        .find("target/debug/vmux_desktop")
        .expect("debug desktop stop");
    let run_idx = makefile
        .find("exec env -u CEF_PATH DYLD_LIBRARY_PATH")
        .expect("debug desktop run");

    assert!(makefile.contains("pgrep -f \"target/debug/vmux_desktop\""));
    assert!(makefile.contains("pgrep -f \"bevy_cef_debug_render_process\""));
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
fn build_git_env_uses_github_style_short_hash() {
    let source = include_str!("../../build_git_env.rs");

    assert!(source.contains("\"--short=7\""));
    assert!(!source.contains("\"--short\", \"HEAD\""));
}

#[test]
fn local_package_only_builds_app_bundle() {
    let package_script = include_str!("../../../scripts/package.sh");

    assert!(package_script.contains("packager_args=(packager --release)"));
    assert!(package_script.contains("packager_args+=(--formats app)"));
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
    let build_script = include_str!("../../../scripts/build-mac-release.sh");
    let makefile = include_str!("../../../Makefile");

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
    assert!(build_script.contains("ensure-local-codesign-identity.sh"));
    assert!(build_script.contains("SKIP_NOTARIZE=\"${SKIP_NOTARIZE:-1}\""));
    assert!(
        makefile.contains("build-local: ensure-mac-deps ensure-package-deps ensure-codesign-deps")
    );
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
        "bevy_asset",
        "bevy_log",
        "bevy_winit",
        "bevy_window",
        "bevy_render",
        "bevy_core_pipeline",
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
        "bevy_animation",
        "bevy_camera_controller",
        "bevy_pbr",
        "bevy_post_process",
        "free_camera",
        "png",
    ] {
        assert!(
            !features.contains(feature),
            "workspace bevy dependency should not enable feature {feature}"
        );
    }
}

#[test]
fn player_mode_owns_player_only_bevy_features() {
    let desktop = include_str!("../Cargo.toml");
    let layout = include_str!("../../vmux_layout/Cargo.toml");

    assert!(desktop.contains("default = [\"full\"]"));
    let full = desktop
        .split_once("full = [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(features, _)| features)
        .expect("full feature list");
    assert!(full.contains("\"player-mode\""));
    assert!(desktop.contains("player-mode = [\"vmux_layout/player-mode\"]"));
    for feature in [
        "bevy/bevy_animation",
        "bevy/bevy_camera_controller",
        "bevy/bevy_pbr",
        "bevy/bevy_post_process",
        "bevy/free_camera",
        "bevy_cef/pbr",
    ] {
        assert!(layout.contains(feature));
    }
}

#[test]
fn patched_bevy_remote_does_not_pull_bevy_dev_tools() {
    let manifest = include_str!("../../../patches/bevy_remote-0.19.0/Cargo.toml");

    assert!(!manifest.contains("bevy_dev_tools"));
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
fn patched_bevy_cef_sprite_backend_enables_render_support_without_pbr() {
    let manifest = include_str!("../../../patches/bevy_cef-0.5.2/Cargo.toml");
    let start = manifest
        .find("[dependencies.bevy]")
        .expect("bevy_cef bevy dependency");
    let rest = &manifest[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    let bevy_block = &rest[..end];

    assert!(bevy_block.contains("\"bevy_sprite_render\""));
    assert!(!bevy_block.contains("\"bevy_pbr\""));
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

    assert!(manifest.contains("bevy_winit = { version = \"0.19.0\", default-features = false"));
}
