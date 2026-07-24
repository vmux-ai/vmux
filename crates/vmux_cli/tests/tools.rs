#[cfg(unix)]
#[test]
fn tools_adopt_and_apply_manage_home_links() {
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
            "tools",
            "adopt",
            config.to_str().unwrap(),
            "--package",
            "shell",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            ".vmux/tools/dotfiles/shell/.config/nushell/config.nu",
        ));

    assert!(config.symlink_metadata().unwrap().file_type().is_symlink());
    std::fs::remove_file(&config).unwrap();

    let mut apply = Command::cargo_bin("vmux").unwrap();
    apply
        .env("HOME", &home)
        .args(["tools", "apply"])
        .assert()
        .success()
        .stdout(predicate::str::contains("linked 1 file(s)"));

    assert!(config.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "echo hi");
}

#[test]
fn tools_import_adopts_existing_manifests() {
    use assert_cmd::Command;
    use predicates::prelude::*;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let brewfile = temp.path().join("Brewfile");
    let mcp = temp.path().join("mcp.json");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(&brewfile, "brew \"ripgrep\"\ncask \"ghostty\"\n").unwrap();
    std::fs::write(
        &mcp,
        r#"{"mcpServers":{"docs":{"url":"https://example.com/mcp"}}}"#,
    )
    .unwrap();

    Command::cargo_bin("vmux")
        .unwrap()
        .env("HOME", &home)
        .args(["tools", "import", "homebrew", brewfile.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 formulae and 1 casks"));
    Command::cargo_bin("vmux")
        .unwrap()
        .env("HOME", &home)
        .args(["tools", "import", "mcp", mcp.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 MCP server"));
    Command::cargo_bin("vmux")
        .unwrap()
        .env("HOME", &home)
        .args(["tools", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("homebrew-formula (1)")
                .and(predicate::str::contains("homebrew-cask (1)"))
                .and(predicate::str::contains("mcp (1)"))
                .and(predicate::str::contains("docs · Http")),
        );
}
