#[test]
fn embedded_plist_uses_bundle_program_relative_path() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(
        xml.contains("<key>BundleProgram</key>"),
        "embedded plist must use BundleProgram, not ProgramArguments"
    );
    assert!(
        xml.contains("Contents/MacOS/vmux_service"),
        "BundleProgram path must be Contents/MacOS/vmux_service"
    );
    assert!(
        !xml.contains("/usr/local/") && !xml.contains("$HOME"),
        "embedded plist must not reference absolute paths outside the bundle"
    );
}

#[test]
fn embedded_plist_keeps_alive_on_crash() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(xml.contains("<key>KeepAlive</key>"));
    assert!(xml.contains("<key>Crashed</key>"));
}

#[test]
fn embedded_plist_label_matches_release_profile() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(
        xml.contains("<string>ai.vmux.service</string>"),
        "release builds must use the suffix-less label"
    );
}

#[test]
fn embedded_plist_sets_build_profile_release() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(xml.contains("<key>VMUX_BUILD_PROFILE</key>"));
    assert!(xml.contains("{{PROFILE}}") || xml.contains("<string>release</string>"));
}

#[test]
fn embedded_plist_associates_with_parent_bundle() {
    let xml = include_str!("../../../packaging/macos/ai.vmux.service.plist");
    assert!(
        xml.contains("<key>AssociatedBundleIdentifiers</key>"),
        "embedded plist must declare AssociatedBundleIdentifiers so Login Items \
         displays the helper grouped under Vmux.app with the app icon/name"
    );
    assert!(
        xml.contains("<string>ai.vmux.desktop</string>"),
        "AssociatedBundleIdentifiers must include the parent app bundle id ai.vmux.desktop"
    );
}
