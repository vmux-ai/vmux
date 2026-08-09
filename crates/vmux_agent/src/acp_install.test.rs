use super::*;

fn npx_agent(id: &str) -> RegistryAgent {
    RegistryAgent {
        id: id.to_string(),
        name: id.to_string(),
        version: Some("1.0.0".to_string()),
        description: None,
        icon: None,
        repository: None,
        distribution: acp_registry::Distribution {
            binary: None,
            npx: Some(acp_registry::PackageDist {
                package: format!("@example/{id}"),
                args: vec![],
                env: Default::default(),
            }),
            uvx: None,
        },
    }
}

#[test]
fn package_spec_pins_version_when_present() {
    assert_eq!(package_spec("@scope/pkg", None), "@scope/pkg");
    assert_eq!(
        package_spec("@scope/pkg", Some("1.2.3")),
        "@scope/pkg@1.2.3"
    );
    assert_eq!(package_spec("pkg", Some("  ")), "pkg");
    assert_eq!(package_spec("pkg", Some("1.0.0")), "pkg@1.0.0");
}

#[test]
fn package_spec_replaces_a_baked_registry_version() {
    // The registry may ship a versioned package; pinning must replace, not append.
    assert_eq!(
        package_spec("@scope/pkg@1.1.9", Some("1.1.8")),
        "@scope/pkg@1.1.8"
    );
    assert_eq!(package_spec("pkg@1.1.9", Some("1.1.8")), "pkg@1.1.8");
    // No pin keeps the registry's package (including its baked version) untouched.
    assert_eq!(package_spec("@scope/pkg@1.1.9", None), "@scope/pkg@1.1.9");
    assert_eq!(package_base("@scope/pkg"), "@scope/pkg");
}

#[test]
fn cmd_basename_strips_prefix_and_dirs() {
    assert_eq!(cmd_basename("./vibe"), "vibe");
    assert_eq!(cmd_basename("vibe"), "vibe");
    assert_eq!(cmd_basename("./bin/agent"), "agent");
}

#[test]
fn archive_filename_takes_last_segment() {
    assert_eq!(
        archive_filename("https://x/y/vibe-darwin-arm64.tar.gz"),
        "vibe-darwin-arm64.tar.gz"
    );
    assert_eq!(archive_filename("https://x/y/bin.zip?token=1"), "bin.zip");
}

#[test]
fn acp_registry_suffix_is_omitted_from_agent_urls() {
    assert_eq!(agent_url_id("codex-acp"), "codex");
    assert_eq!(agent_url_id("custom-acp"), "custom");
    assert_eq!(agent_url_id("mistral-vibe"), "mistral-vibe");
}

#[test]
fn agent_ids_match_url_and_registry_forms() {
    assert!(agent_ids_match("codex", "codex-acp"));
    assert!(agent_ids_match("custom", "custom-acp"));
    assert!(agent_ids_match("vibe", "mistral-vibe"));
    assert!(!agent_ids_match("codex", "custom-acp"));
}

#[test]
fn resolved_cmd_path_by_archive_kind() {
    let pkg = Path::new("/pkg");
    let tar = BinaryTarget {
        archive: "https://x/a.tar.gz".into(),
        cmd: "./bin/agent".into(),
        args: vec![],
        env: Default::default(),
    };
    assert_eq!(
        resolved_cmd_path(pkg, &tar, "a.tar.gz"),
        Path::new("/pkg/bin/agent")
    );
    let gz = BinaryTarget {
        archive: "https://x/a.gz".into(),
        cmd: "./agent".into(),
        args: vec![],
        env: Default::default(),
    };
    assert_eq!(resolved_cmd_path(pkg, &gz, "a.gz"), Path::new("/pkg/agent"));
}

#[test]
fn shared_node_does_not_mark_every_npx_agent_installed() {
    let root = std::env::temp_dir().join(format!(
        "vmux-acp-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let node = node_bindir(&root).unwrap().join("node");
    std::fs::create_dir_all(node.parent().unwrap()).unwrap();
    std::fs::write(&node, b"").unwrap();
    let installed = npx_agent("installed-agent");
    let available = npx_agent("available-agent");

    assert!(!is_agent_installed_at(&root, &installed));
    assert!(!is_agent_installed_at(&root, &available));

    write_agent_receipt(&root, &installed, None).unwrap();

    assert!(is_agent_installed_at(&root, &installed));
    assert!(!is_agent_installed_at(&root, &available));

    uninstall_at(&root, &installed.id).unwrap();

    assert!(!is_agent_installed_at(&root, &installed));
    assert!(node.exists());
    std::fs::remove_dir_all(root).unwrap();
}
