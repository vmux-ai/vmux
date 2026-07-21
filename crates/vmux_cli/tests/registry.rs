#[cfg(unix)]
#[test]
fn registry_adopt_and_apply_manage_home_links() {
    use assert_cmd::Command;
    use predicates::prelude::*;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config = home.join(".config/nushell/config.nu");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "echo hi").unwrap();

    let mut adopt = Command::cargo_bin("vmux").unwrap();
    adopt
        .env("HOME", &home)
        .args([
            "registry",
            "adopt",
            config.to_str().unwrap(),
            "--package",
            "shell",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            ".vmux/registry/dotfiles/shell/.config/nushell/config.nu",
        ));

    assert!(config.symlink_metadata().unwrap().file_type().is_symlink());
    std::fs::remove_file(&config).unwrap();

    let mut apply = Command::cargo_bin("vmux").unwrap();
    apply
        .env("HOME", &home)
        .args(["registry", "apply"])
        .assert()
        .success()
        .stdout(predicate::str::contains("linked 1 file(s)"));

    assert!(config.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "echo hi");
}
