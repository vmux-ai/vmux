use super::*;

fn registry_agent(id: &str, name: &str) -> crate::acp_registry::RegistryAgent {
    crate::acp_registry::RegistryAgent {
        id: id.to_string(),
        name: name.to_string(),
        version: None,
        description: None,
        icon: None,
        repository: None,
        distribution: crate::acp_registry::Distribution::default(),
    }
}

#[test]
fn writes_empty_snapshot_when_no_resources() {
    let mut app = App::new();
    app.init_resource::<CommandBarAgentsSnapshot>()
        .add_systems(Update, update_agents_snapshot);
    app.update();
    let snap = app.world().resource::<CommandBarAgentsSnapshot>();
    assert!(snap.providers.is_empty());
    assert!(snap.strategies.is_empty());
}

#[test]
fn agent_sessions_snapshot_starts_empty() {
    let mut app = App::new();
    app.init_resource::<CommandBarTerminalsSnapshot>()
        .add_systems(Update, update_agent_sessions_snapshot);
    app.update();
    let snap = app.world().resource::<CommandBarTerminalsSnapshot>();
    assert!(snap.agent_session_to_entity.is_empty());
}

#[test]
fn cli_snapshot_only_contains_ready_providers() {
    let mut app = App::new();
    app.init_resource::<CommandBarAgentsSnapshot>()
        .add_systems(Update, update_agents_snapshot);
    app.world_mut().spawn((
        AgentProviderTargetKind(vmux_core::agent::AgentKind::Codex),
        Name::new("Codex"),
        Ready,
    ));
    app.world_mut().spawn((
        AgentProviderTargetKind(vmux_core::agent::AgentKind::Claude),
        Name::new("Claude"),
    ));

    app.update();

    let providers = &app.world().resource::<CommandBarAgentsSnapshot>().providers;
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id, "codex");
}

#[test]
fn installed_unconfigured_acp_is_in_snapshot() {
    let catalog = vec![registry_agent("new-agent-acp", "New Agent")];

    let agents = acp_agent_summaries(&catalog, |_| true);

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "new-agent-acp");
    assert_eq!(agents[0].url, "vmux://agent/new-agent");
}

#[test]
fn uninstalled_acp_is_not_in_snapshot() {
    let catalog = vec![
        registry_agent("installed", "Installed"),
        registry_agent("available", "Available"),
    ];

    let agents = acp_agent_summaries(&catalog, |agent| agent.id == "installed");

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "installed");
}

#[test]
fn recent_agents_are_deduped_and_sorted_by_last_use() {
    let mut app = App::new();
    app.init_resource::<CommandBarAgentsSnapshot>()
        .add_systems(Update, update_recent_agents);
    let cli_stack = app.world_mut().spawn(LastActivatedAt(20)).id();
    app.world_mut().spawn((
        vmux_core::agent::AgentSession {
            kind: vmux_core::agent::AgentKind::Codex,
        },
        ChildOf(cli_stack),
    ));
    app.world_mut().spawn((
        crate::client::acp::AcpSession {
            agent_id: "claude".to_string(),
            sid: "acp-session".to_string(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_service::protocol::ProcessId::new(),
            resume: None,
        },
        LastActivatedAt(30),
    ));
    app.world_mut().spawn((
        crate::client::acp::AcpSession {
            agent_id: "claude-acp".to_string(),
            sid: "older-acp-session".to_string(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_service::protocol::ProcessId::new(),
            resume: None,
        },
        LastActivatedAt(10),
    ));

    app.update();

    assert_eq!(
        app.world().resource::<CommandBarAgentsSnapshot>().recent,
        vec![
            AgentPromptTarget::Acp {
                id: "claude".to_string(),
            },
            AgentPromptTarget::Cli(vmux_core::agent::AgentKind::Codex),
        ]
    );

    let mut q = app
        .world_mut()
        .query_filtered::<Entity, With<crate::client::acp::AcpSession>>();
    let acp_sessions: Vec<_> = q.iter(app.world()).collect();
    for session in acp_sessions {
        app.world_mut().despawn(session);
    }
    app.update();

    assert_eq!(
        app.world().resource::<CommandBarAgentsSnapshot>().recent,
        vec![
            AgentPromptTarget::Acp {
                id: "claude".to_string(),
            },
            AgentPromptTarget::Cli(vmux_core::agent::AgentKind::Codex),
        ]
    );
}

#[test]
fn closed_codex_acp_stays_ahead_of_older_claude_cli() {
    let mut app = App::new();
    app.init_resource::<CommandBarAgentsSnapshot>()
        .add_systems(Update, update_recent_agents);
    let cli_stack = app.world_mut().spawn(LastActivatedAt(20)).id();
    app.world_mut().spawn((
        vmux_core::agent::AgentSession {
            kind: vmux_core::agent::AgentKind::Claude,
        },
        ChildOf(cli_stack),
    ));
    app.world_mut().spawn(ArchivedPage {
        url: "vmux://agent/codex-acp/session-1".to_string(),
        closed_at: 30,
        ..default()
    });

    app.update();

    assert_eq!(
        app.world().resource::<CommandBarAgentsSnapshot>().recent,
        vec![
            AgentPromptTarget::Acp {
                id: "codex".to_string(),
            },
            AgentPromptTarget::Cli(vmux_core::agent::AgentKind::Claude),
        ]
    );
}

#[test]
fn equal_recent_agent_times_fall_back_to_name() {
    let mut app = App::new();
    app.init_resource::<CommandBarAgentsSnapshot>()
        .add_systems(Update, update_recent_agents);
    app.world_mut().spawn((
        crate::client::acp::AcpSession {
            agent_id: "claude-acp".to_string(),
            sid: "acp-session".to_string(),
            cwd: std::path::PathBuf::new(),
            anchor: vmux_service::protocol::ProcessId::new(),
            resume: None,
        },
        LastActivatedAt(10),
    ));
    let cli_stack = app.world_mut().spawn(LastActivatedAt(10)).id();
    app.world_mut().spawn((
        vmux_core::agent::AgentSession {
            kind: vmux_core::agent::AgentKind::Codex,
        },
        ChildOf(cli_stack),
    ));

    app.update();

    assert_eq!(
        app.world().resource::<CommandBarAgentsSnapshot>().recent,
        vec![
            AgentPromptTarget::Acp {
                id: "claude".to_string(),
            },
            AgentPromptTarget::Cli(vmux_core::agent::AgentKind::Codex),
        ]
    );
}
