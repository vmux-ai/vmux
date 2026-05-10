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
fn run_mac_uses_signed_debug_binary() {
    let makefile = include_str!("../../../Makefile");

    assert!(makefile.contains("run-mac: build-mac-debug"));
    assert!(makefile.contains("exec env -u CEF_PATH ./target/debug/vmux_desktop"));
    assert!(makefile.contains("sign-mac-debug"));
    assert!(makefile.contains("identity=\"$$(./scripts/ensure-local-codesign-identity.sh)\" &&"));
}

#[test]
fn local_package_uses_stable_bundle_name() {
    let package_script = include_str!("../../../scripts/package.sh");

    assert!(package_script.contains("PRODUCT_NAME=\"Vmux Local\""));
    assert!(!package_script.contains("PRODUCT_NAME=\"Vmux ($GIT_HASH)\""));
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
fn debug_signing_uses_default_keychain_directly() {
    let signing_script = include_str!("../../../scripts/sign-debug-mac.sh");

    assert!(signing_script.contains("CODESIGN_KEYCHAIN"));
    assert!(signing_script.contains("--keychain"));
}

#[test]
fn debug_and_local_share_main_bundle_identifier() {
    let signing_script = include_str!("../../../scripts/sign-debug-mac.sh");
    let package_script = include_str!("../../../scripts/package.sh");

    assert!(signing_script.contains("APP_IDENTIFIER=\"ai.vmux.desktop.local\""));
    assert!(package_script.contains("BUNDLE_ID=\"ai.vmux.desktop.local\""));
    assert!(!signing_script.contains("ai.vmux.desktop.dev."));
}
