//! Verify cargo-packager metadata embeds vmux_service in the bundle.

#[test]
fn packager_binaries_include_vmux_service() {
    let toml = include_str!("../Cargo.toml");
    assert!(
        toml.contains(r#"path = "vmux_service""#),
        "packager metadata must include vmux_service so it lands in Vmux.app/Contents/MacOS/"
    );
}

#[test]
fn before_packaging_command_builds_vmux_service() {
    let toml = include_str!("../Cargo.toml");
    let line = toml
        .lines()
        .find(|l| l.starts_with("before-packaging-command"))
        .expect("before-packaging-command line present");
    assert!(
        line.contains("vmux_service"),
        "before-packaging-command must build vmux_service: {line}"
    );
}
