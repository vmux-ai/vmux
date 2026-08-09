use super::*;

#[test]
fn catalog_tracks_multiple_webviews() {
    let mut app = App::new();
    app.init_resource::<AgentsPageWebviews>()
        .add_observer(on_catalog_request);
    let first = app.world_mut().spawn_empty().id();
    let second = app.world_mut().spawn_empty().id();
    for webview in [first, second] {
        app.world_mut().trigger(BinReceive {
            webview,
            payload: AgentsCatalogRequest {},
        });
    }

    let webviews = app.world().resource::<AgentsPageWebviews>();
    assert_eq!(webviews.0, HashSet::from([first, second]));
}

#[test]
fn cli_catalog_rows_report_install_state() {
    let rows = cli_agent_entries(|kind| kind == AgentKind::Codex);

    assert_eq!(rows.len(), 3);
    let codex = rows.iter().find(|row| row.id == "cli:codex").unwrap();
    assert_eq!(codex.source, "cli");
    assert_eq!(codex.launch_url, "vmux://agent/codex/cli");
    assert_eq!(codex.status, "installed");
    assert!(!codex.uninstallable);
    assert!(
        rows.iter()
            .filter(|row| row.id != "cli:codex")
            .all(|row| row.status == "available")
    );
}

#[test]
fn upsert_acp_version_updates_match_or_appends() {
    let mut acp = vec![vmux_setting::AcpAgentConfig {
        id: "claude".into(),
        name: "Claude Code".into(),
        command: "npx".into(),
        args: vec![],
        env: vec![],
        cwd: None,
        version: None,
    }];
    upsert_acp_version(&mut acp, "claude-acp", "Claude", Some("1.2.3".into()));
    assert_eq!(acp.len(), 1);
    assert_eq!(acp[0].version.as_deref(), Some("1.2.3"));

    upsert_acp_version(&mut acp, "other-acp", "Other", Some("9.9.9".into()));
    assert_eq!(acp.len(), 2);
    let added = acp.iter().find(|c| c.id == "other").unwrap();
    assert_eq!(added.command, "");
    assert_eq!(added.version.as_deref(), Some("9.9.9"));

    upsert_acp_version(&mut acp, "claude-acp", "Claude", None);
    assert_eq!(acp[0].version, None);
}

#[test]
fn catalog_snapshot_carries_pinned_version() {
    let catalog = AcpCatalog {
        agents: vec![crate::acp_registry::RegistryAgent {
            id: "claude-acp".into(),
            name: "Claude".into(),
            version: None,
            description: None,
            icon: None,
            repository: None,
            distribution: crate::acp_registry::Distribution {
                binary: None,
                npx: Some(crate::acp_registry::PackageDist {
                    package: "@zed-industries/claude-code-acp".into(),
                    args: vec![],
                    env: Default::default(),
                }),
                uvx: None,
            },
        }],
    };
    let acp = vec![vmux_setting::AcpAgentConfig {
        id: "claude".into(),
        name: "Claude Code".into(),
        command: "npx".into(),
        args: vec![],
        env: vec![],
        cwd: None,
        version: Some("0.11.0".into()),
    }];
    let snapshot = catalog_snapshot(
        &catalog,
        &AgentsStatus::default(),
        &acp,
        &AgentVersions::default(),
    );
    let row = snapshot
        .agents
        .iter()
        .find(|a| a.id == "claude-acp")
        .expect("claude row present");
    assert_eq!(row.pinned_version, "0.11.0");
}
