use super::*;

#[test]
fn known_extensions_map_to_servers() {
    assert_eq!(builtin_spec("rs").unwrap().command, "rust-analyzer");
    assert_eq!(builtin_spec("rs").unwrap().language_id, "rust");
    assert_eq!(builtin_spec("tsx").unwrap().language_id, "typescriptreact");
    assert_eq!(builtin_spec("cpp").unwrap().language_id, "cpp");
    assert!(builtin_spec("xyzzy").is_none());
}

#[test]
fn known_extensions_map_to_preferred_packages() {
    for (extension, package) in [
        ("rs", "rust-analyzer"),
        ("py", "pyright"),
        ("tsx", "typescript-language-server"),
        ("go", "gopls"),
        ("cpp", "clangd"),
        ("lua", "lua-language-server"),
        ("rb", "solargraph"),
        ("zig", "zls"),
        ("sh", "bash-language-server"),
        ("json", "json-lsp"),
        ("yaml", "yaml-language-server"),
        ("toml", "taplo"),
        ("md", "marksman"),
        ("java", "jdtls"),
    ] {
        assert_eq!(preferred_package(extension), Some(package));
    }
    assert_eq!(preferred_package("xyzzy"), None);
}

#[test]
fn executable_lookup_finds_a_real_binary() {
    assert!(executable_on_path("cargo"));
    assert!(!executable_on_path("definitely-not-a-real-binary-zzz"));
}

#[test]
fn workspace_root_finds_marker_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    let nested = root.join("crates").join("a").join("src");
    std::fs::create_dir_all(&nested).unwrap();
    let found = workspace_root(&nested, &["Cargo.toml".into(), ".git".into()]);
    assert_eq!(found, root);
}

#[test]
fn workspace_root_falls_back_to_start() {
    let tmp = tempfile::tempdir().unwrap();
    let start = tmp.path().join("no").join("markers");
    std::fs::create_dir_all(&start).unwrap();
    assert_eq!(workspace_root(&start, &["Cargo.toml".into()]), start);
}

#[test]
fn linters_map_by_extension() {
    assert_eq!(linter_for("py").unwrap().command, "ruff");
    assert_eq!(linter_for("ts").unwrap().format, LintFormat::Eslint);
    assert_eq!(linter_for("sh").unwrap().command, "shellcheck");
    assert!(linter_for("rs").is_none());
}

#[test]
fn override_takes_precedence_over_builtin() {
    let mut ov = std::collections::BTreeMap::new();
    ov.insert(
        "rs".to_string(),
        ServerSpec {
            command: "my-ra".into(),
            args: vec![],
            language_id: "rust".into(),
            root_markers: vec![".git".into()],
        },
    );
    assert_eq!(resolve_spec("rs", &ov).unwrap().command, "my-ra");
    assert_eq!(resolve_spec("go", &ov).unwrap().command, "gopls");
    assert!(resolve_spec("zzz", &ov).is_none());
}
