use super::*;
use vmux_core::agent::AgentKind;

/// What the contributing crate publishes, so the prompt tests run the real path.
fn contributions(agents: &CommandBarAgentsSnapshot) -> CommandBarContributions {
    CommandBarContributions {
        pages: agents.launcher_pages(),
        ..Default::default()
    }
}

#[test]
fn agents_snapshot_default_is_empty() {
    let s = CommandBarAgentsSnapshot::default();
    assert!(s.providers.is_empty());
    assert!(s.strategies.is_empty());
    assert!(s.acp.is_empty());
    assert!(s.recent.is_empty());
}

#[test]
fn prompt_prefers_most_recent_installed_agent() {
    let snapshot = CommandBarAgentsSnapshot {
        recent: vec![AgentPromptTarget::Cli(AgentKind::Codex)],
        providers: vec![AgentProviderSummary {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            url: "vmux://agent/codex/cli".to_string(),
            icon: String::new(),
        }],
        acp: vec![AgentProviderSummary {
            id: "claude-acp".to_string(),
            name: "Claude Agent".to_string(),
            url: "vmux://agent/claude".to_string(),
            icon: String::new(),
        }],
        ..Default::default()
    };

    assert_eq!(
        contributions(&snapshot).prompt_url(None).as_deref(),
        Some("vmux://agent/codex/cli")
    );
}

#[test]
fn prompt_falls_back_to_installed_agent() {
    let snapshot = CommandBarAgentsSnapshot {
        acp: vec![AgentProviderSummary {
            id: "claude-acp".to_string(),
            name: "Claude Agent".to_string(),
            url: "vmux://agent/claude".to_string(),
            icon: String::new(),
        }],
        ..Default::default()
    };

    assert_eq!(
        contributions(&snapshot).prompt_url(None).as_deref(),
        Some("vmux://agent/claude")
    );
    assert_eq!(
        contributions(&CommandBarAgentsSnapshot::default()).prompt_url(None),
        None
    );
}

#[test]
fn prompt_uses_selected_installed_agent_and_rejects_stale_url() {
    let snapshot = CommandBarAgentsSnapshot {
        providers: vec![AgentProviderSummary {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            url: "vmux://agent/codex/cli".to_string(),
            icon: String::new(),
        }],
        acp: vec![AgentProviderSummary {
            id: "claude-acp".to_string(),
            name: "Claude Agent".to_string(),
            url: "vmux://agent/claude".to_string(),
            icon: String::new(),
        }],
        recent: vec![AgentPromptTarget::Cli(AgentKind::Codex)],
        ..Default::default()
    };

    assert_eq!(
        contributions(&snapshot)
            .prompt_url(Some("vmux://agent/claude"))
            .as_deref(),
        Some("vmux://agent/claude")
    );
    assert_eq!(
        contributions(&snapshot)
            .prompt_url(Some("vmux://agent/uninstalled"))
            .as_deref(),
        Some("vmux://agent/codex/cli")
    );
}

#[test]
fn launcher_pages_lists_only_snapshot_agents_in_recent_order() {
    let snapshot = CommandBarAgentsSnapshot {
        providers: vec![AgentProviderSummary {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            url: "vmux://agent/codex/cli".to_string(),
            icon: String::new(),
        }],
        acp: vec![AgentProviderSummary {
            id: "claude-acp".to_string(),
            name: "Claude Agent".to_string(),
            url: "vmux://agent/claude".to_string(),
            icon: "https://cdn.example/claude-acp.svg".to_string(),
        }],
        recent: vec![
            AgentPromptTarget::Cli(AgentKind::Codex),
            AgentPromptTarget::Acp {
                id: "claude".to_string(),
            },
        ],
        ..Default::default()
    };
    let pages = snapshot.launcher_pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].id, "codex");
    assert_eq!(pages[0].page.url, "vmux://agent/codex/cli");
    assert_eq!(pages[0].page.title, "Codex (CLI)");
    assert_eq!(pages[0].page.host, "agent");
    assert_eq!(pages[1].page.title, "Claude Agent");
    assert!(matches!(
        pages[1].page.icon,
        vmux_core::PageIcon::Favicon(ref u) if u == "https://cdn.example/claude-acp.svg"
    ));
}

#[test]
fn terminals_snapshot_default_is_empty() {
    let s = CommandBarTerminalsSnapshot::default();
    assert!(s.pid_to_entity.is_empty());
    assert!(s.agent_session_to_entity.is_empty());
}

#[test]
fn pages_snapshot_collects_only_command_bar_pages() {
    let mut app = App::new();
    app.init_resource::<CommandBarPagesSnapshot>()
        .add_systems(Update, update_pages_snapshot);
    app.world_mut().spawn(PageManifest {
        host: "settings",
        title: "Settings",
        keywords: &["preferences"],
        icon: Some(vmux_core::BuiltinIcon::Settings),
        command_bar: true,
    });
    app.world_mut().spawn(PageManifest {
        host: "layout",
        title: "Layout",
        keywords: &[],
        icon: None,
        command_bar: false,
    });

    app.update();

    let snap = app.world().resource::<CommandBarPagesSnapshot>();
    assert_eq!(snap.pages.len(), 1);
    assert_eq!(snap.pages[0].host, "settings");
    assert_eq!(snap.pages[0].url, "vmux://settings/");
}
