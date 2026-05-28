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
