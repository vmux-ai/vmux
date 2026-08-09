use super::*;

fn s(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

#[test]
fn auto_approval_message_targets_requested_session_and_call() {
    let session = AcpSession {
        agent_id: "claude".into(),
        sid: "s1".into(),
        cwd: "/tmp".into(),
        anchor: vmux_core::ProcessId::new(),
        resume: None,
    };
    let mut policy = AgentApprovalPolicy::default();
    policy.allow("run");
    let request = AgentApprovalRequest {
        session: Entity::PLACEHOLDER,
        call_id: "call-1".into(),
        name: "run".into(),
        args: serde_json::json!({}),
    };

    assert!(matches!(
        acp_auto_approval_message(&session, &policy, &request),
        Some(ClientMessage::Shared(SharedMessage::Agent {
            sid,
            action: vmux_service::protocol::AgentAction::Approve { call_id, decision },
        }))
            if sid == "s1"
                && call_id == "call-1"
                && decision == vmux_service::protocol::ApprovalDecision::AllowAlways
    ));
}

#[test]
fn queued_prompt_waits_for_acp_install_start() {
    let mut queue = PromptQueue::default();
    queue.enqueue("hello".to_string());

    assert!(!acp_prompt_dispatch_ready(
        &AgentRunState::Idle,
        &queue,
        false
    ));
    assert!(acp_prompt_dispatch_ready(
        &AgentRunState::Idle,
        &queue,
        true
    ));
    assert!(!acp_prompt_dispatch_ready(
        &AgentRunState::Installing {
            pct: None,
            message: "Preparing agent…".to_string(),
        },
        &queue,
        true
    ));
}

#[test]
fn unbound_workspace_context_allows_reading_before_project_setup() {
    let context = acp_prompt_context(None, Some(AcpWorkspaceState::Unbound)).unwrap();

    assert!(context.contains("Read-only inspection"));
    assert!(context.contains("Never call select_project or create_worktree"));
    assert!(context.contains("select_project"));
    assert!(context.contains("request_user_choice"));
    assert!(context.contains("~/.vmux/workspace/<remote-host>"));
    assert!(context.contains("~/.vmux/workspace/local/<project>"));
    assert!(context.contains("create the empty directory"));
    assert!(context.contains("use the new project root directly"));
    assert!(context.contains("Do not search the user's home directory"));
    assert!(context.contains("project picker"));
}

#[test]
fn repository_context_defers_worktree_until_mutation() {
    let context =
        acp_prompt_context(None, Some(AcpWorkspaceState::RepositoryNeedsWorktree)).unwrap();

    assert!(context.contains("Reading and inspection are allowed"));
    assert!(context.contains("Never call create_worktree"));
    assert!(context.contains("Immediately before the first edit"));
    assert!(context.contains("create_worktree"));
    assert!(context.contains("request_user_choice"));
    assert!(context.contains("Never run git worktree add"));
}

#[test]
fn pending_worktree_context_requires_waiting_for_activation() {
    let context = acp_prompt_context(
        Some("prior conversation".into()),
        Some(AcpWorkspaceState::PendingWorktree),
    )
    .unwrap();

    assert!(context.starts_with("prior conversation\n\n"));
    assert!(context.contains("activation is pending"));
    assert!(context.contains("Wait for vmux"));
    assert!(context.contains("before inspecting"));
}

#[test]
fn bound_workspace_keeps_only_handoff_context() {
    assert_eq!(
        acp_prompt_context(
            Some("prior conversation".into()),
            Some(AcpWorkspaceState::Bound),
        )
        .as_deref(),
        Some("prior conversation")
    );
}

#[test]
fn ancestor_workspace_state_tracks_pending_and_bound_tab() {
    use bevy::ecs::system::RunSystemOnce;

    let mut app = App::new();
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Tab 1".into(),
            startup_dir: None,
        })
        .id();
    let stack = app.world_mut().spawn(ChildOf(tab)).id();
    let state = |world: &mut World| {
        world
                .run_system_once(
                    move |child_of: Query<&ChildOf>,
                          tabs: Query<&vmux_layout::tab::Tab>,
                          workspaces: Query<(), With<vmux_layout::tab::TabWorkspace>>,
                          pending: Query<(), With<crate::plugin::PendingAgentProject>>,
                          needs_worktree: Query<
                        (),
                        With<crate::plugin::RepositoryNeedsWorktree>,
                    >| {
                        ancestor_acp_workspace_state(
                            stack,
                            &child_of,
                            &tabs,
                            &workspaces,
                            &pending,
                            &needs_worktree,
                        )
                    },
                )
                .unwrap()
    };

    assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Unbound));
    app.world_mut()
        .entity_mut(tab)
        .insert(crate::plugin::PendingAgentProject("/repo".into()));
    assert_eq!(
        state(app.world_mut()),
        Some(AcpWorkspaceState::PendingWorktree)
    );
    app.world_mut().entity_mut(tab).insert((
        vmux_layout::tab::Tab {
            name: "Tab 1".into(),
            startup_dir: Some("/repo".into()),
        },
        crate::plugin::RepositoryNeedsWorktree,
    ));
    assert_eq!(
        state(app.world_mut()),
        Some(AcpWorkspaceState::RepositoryNeedsWorktree)
    );
    app.world_mut()
        .entity_mut(tab)
        .insert(vmux_layout::tab::TabWorkspace {
            project_dir: "/repo".into(),
        })
        .remove::<crate::plugin::RepositoryNeedsWorktree>();
    assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Bound));
}

#[test]
fn acp_workspace_update_rebinds_only_matching_tab() {
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
    git(&["add", "seed.txt"]);
    git(&["commit", "-qm", "init"]);
    let worktree_parent = tempfile::tempdir().unwrap();
    let worktree = worktree_parent.path().join("quiet-amber-wolf");
    vmux_git::worktree::worktree_add(repo.path(), &worktree, "vibe/quiet-amber-wolf", "main")
        .unwrap();
    let project_dir = repo.path().canonicalize().unwrap();
    let worktree_dir = worktree.canonicalize().unwrap();
    let mut app = App::new();
    app.add_message::<vmux_service::agent_events::PageAgentWorkspaceChanged>()
        .add_systems(Update, apply_acp_workspace_changed);
    let tab = app
        .world_mut()
        .spawn((
            vmux_layout::tab::Tab {
                name: "matching".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            },
            vmux_layout::tab::TabWorkspace {
                project_dir: project_dir.to_string_lossy().into_owned(),
            },
        ))
        .id();
    let session = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "mistral-vibe".into(),
                sid: "matching-sid".into(),
                cwd: project_dir.clone(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            ChildOf(tab),
        ))
        .id();
    let unrelated_tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "unrelated".into(),
            startup_dir: Some(project_dir.to_string_lossy().into_owned()),
        })
        .id();
    app.world_mut()
        .resource_mut::<Messages<vmux_service::agent_events::PageAgentWorkspaceChanged>>()
        .write(vmux_service::agent_events::PageAgentWorkspaceChanged {
            sid: "matching-sid".into(),
            name: "quiet-amber-wolf".into(),
            branch: "vibe/quiet-amber-wolf".into(),
            cwd: worktree_dir.to_string_lossy().into_owned(),
            workspace_cwd: project_dir.to_string_lossy().into_owned(),
        });

    app.update();

    assert_eq!(
        app.world().get::<AcpSession>(session).unwrap().cwd,
        worktree_dir
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir
            .as_deref(),
        Some(worktree_dir.to_string_lossy().as_ref())
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::Tab>(unrelated_tab)
            .unwrap()
            .startup_dir
            .as_deref(),
        Some(project_dir.to_string_lossy().as_ref())
    );
}

#[test]
fn login_env_reaches_agent_and_overrides_base() {
    let base = vec![s("MISTRAL_API_KEY", ""), s("KEEP", "1")];
    let login = vec![s("MISTRAL_API_KEY", "real-key"), s("PATH", "/login/bin")];
    let env = build_agent_env(base, &login, None);
    assert!(
        env.contains(&s("MISTRAL_API_KEY", "real-key")),
        "login-shell API key must win over the empty registry value: {env:?}"
    );
    assert!(env.contains(&s("KEEP", "1")));
    assert!(env.contains(&s("PATH", "/login/bin")));
}

#[test]
fn managed_bin_prepends_to_login_path_not_process_path() {
    let login = vec![s("PATH", "/login/bin")];
    let env = build_agent_env(Vec::new(), &login, Some("/managed/node/bin".to_string()));
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.as_str());
    assert_eq!(path, Some("/managed/node/bin:/login/bin"));
}

#[test]
fn apply_path_prepend_prefers_env_path_over_process() {
    let env = apply_path_prepend(vec![s("PATH", "/from/login")], Some("/managed".to_string()));
    assert_eq!(
        env.iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str()),
        Some("/managed:/from/login")
    );
}

#[test]
fn completed_install_progress_describes_agent_startup() {
    assert_eq!(
        display_install_progress(InstallPhase::Done, Some(100), "ready"),
        (None, "Starting agent…".to_string())
    );
    assert_eq!(
        display_install_progress(InstallPhase::Downloading, Some(42), "downloading"),
        (Some(42), "downloading".to_string())
    );
    assert_eq!(ready_agent_message(None), "Starting agent…");
    assert_eq!(
        ready_agent_message(Some("session-1")),
        "Loading session history…"
    );
}

#[test]
fn live_acp_identity_updates_only_matching_profile() {
    use vmux_core::team::Profile;
    use vmux_service::agent_events::PageAgentInfo;

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_plugins(AcpAgentPlugin)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();
    let matching = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "antigravity".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            Profile::registry("Configured", "antigravity"),
        ))
        .id();
    let unrelated = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "claude".into(),
                sid: "s2".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            Profile::registry("Claude", "claude"),
        ))
        .id();

    app.world_mut().write_message(PageAgentInfo {
        sid: "s1".into(),
        name: "Antigravity".into(),
    });
    app.update();

    assert_eq!(
        app.world().get::<Profile>(matching).unwrap().name,
        "Antigravity"
    );
    assert_eq!(
        app.world().get::<Profile>(unrelated).unwrap().name,
        "Claude"
    );

    app.world_mut().write_message(PageAgentInfo {
        sid: "s1".into(),
        name: "   ".into(),
    });
    app.update();

    assert_eq!(
        app.world().get::<Profile>(matching).unwrap().name,
        "Antigravity"
    );
}

#[test]
fn live_acp_model_info_updates_only_matching_session() {
    use vmux_service::agent_events::PageAgentModelInfo;
    use vmux_service::protocol::AcpModelOption;

    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_plugins(AcpAgentPlugin)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();
    let matching = app
        .world_mut()
        .spawn(AcpSession {
            agent_id: "claude".into(),
            sid: "s1".into(),
            cwd: "/tmp".into(),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        })
        .id();
    let unrelated = app
        .world_mut()
        .spawn(AcpSession {
            agent_id: "codex".into(),
            sid: "s2".into(),
            cwd: "/tmp".into(),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        })
        .id();

    app.world_mut().write_message(PageAgentModelInfo {
        sid: "s1".into(),
        config_id: "model".into(),
        current_model_id: "sonnet".into(),
        models: vec![AcpModelOption {
            id: "sonnet".into(),
            name: "Claude Sonnet".into(),
            description: None,
        }],
    });
    app.update();

    let state = app.world().get::<AcpModelState>(matching).unwrap();
    assert_eq!(state.current_name(), "Claude Sonnet");
    assert!(state.pending.is_none());
    assert!(app.world().get::<AcpModelState>(unrelated).is_none());
}

#[test]
fn model_results_preserve_latest_pending_selection() {
    use vmux_service::agent_events::{PageAgentModelInfo, PageAgentModelSelectionResult};
    use vmux_service::protocol::AcpModelOption;

    let models = vec![
        AcpModelOption {
            id: "default".into(),
            name: "Default".into(),
            description: None,
        },
        AcpModelOption {
            id: "opus".into(),
            name: "Opus".into(),
            description: None,
        },
        AcpModelOption {
            id: "fable".into(),
            name: "Fable".into(),
            description: None,
        },
    ];
    let mut app = App::new();
    app.add_message::<PageAgentModelInfo>()
        .add_message::<PageAgentModelSelectionResult>()
        .add_systems(
            Update,
            (apply_acp_model_info, apply_acp_model_selection_result).chain(),
        );
    let entity = app
        .world_mut()
        .spawn((
            AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
            AcpModelState {
                config_id: "model".into(),
                current_model_id: "default".into(),
                pending: Some(PendingAcpModelSelection {
                    request_id: 2,
                    model_id: "fable".into(),
                }),
                models: models.clone(),
            },
        ))
        .id();

    app.world_mut().write_message(PageAgentModelInfo {
        sid: "s1".into(),
        config_id: "model".into(),
        current_model_id: "opus".into(),
        models: models.clone(),
    });
    app.update();

    let state = app.world().get::<AcpModelState>(entity).unwrap();
    assert_eq!(state.current_model_id, "opus");
    assert_eq!(
        state.pending.as_ref().map(|pending| pending.request_id),
        Some(2)
    );
    assert_eq!(state.current_name(), "Fable");

    app.world_mut()
        .write_message(PageAgentModelSelectionResult {
            sid: "s1".into(),
            request_id: 1,
            model_id: "fable".into(),
            succeeded: false,
        });
    app.update();
    assert_eq!(
        app.world()
            .get::<AcpModelState>(entity)
            .unwrap()
            .pending
            .as_ref()
            .map(|pending| pending.request_id),
        Some(2)
    );

    app.world_mut()
        .write_message(PageAgentModelSelectionResult {
            sid: "s1".into(),
            request_id: 2,
            model_id: "fable".into(),
            succeeded: false,
        });
    app.update();
    let state = app.world().get::<AcpModelState>(entity).unwrap();
    assert!(state.pending.is_none());
    assert_eq!(state.current_name(), "Opus");

    {
        let mut state = app.world_mut().get_mut::<AcpModelState>(entity).unwrap();
        state.pending = Some(PendingAcpModelSelection {
            request_id: 3,
            model_id: "fable".into(),
        });
    }
    app.world_mut()
        .write_message(PageAgentModelSelectionResult {
            sid: "s1".into(),
            request_id: 3,
            model_id: "fable".into(),
            succeeded: true,
        });
    app.update();
    let state = app.world().get::<AcpModelState>(entity).unwrap();
    assert_eq!(state.current_model_id, "fable");
    assert!(state.pending.is_none());
}

#[test]
fn acp_terminal_stack_does_not_take_focus_from_agent() {
    use vmux_layout::pane::leaf_pane_bundle;
    use vmux_layout::stack::Stack;
    use vmux_layout::tab::tab_bundle;
    use vmux_service::agent_events::PageAgentAcpTerminalCreated;

    let mut app = App::new();
    app.add_message::<PageAgentAcpTerminalCreated>()
        .add_systems(Update, apply_acp_terminal_created)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();
    let tab = app.world_mut().spawn(tab_bundle()).id();
    let pane = app
        .world_mut()
        .spawn((leaf_pane_bundle(), ChildOf(tab)))
        .id();
    let agent = app
        .world_mut()
        .spawn((
            stack_bundle(),
            LastActivatedAt(10),
            ChildOf(pane),
            AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: vmux_core::ProcessId::new(),
                resume: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(agent)
        .insert(vmux_core::PageMetadata {
            url: "vmux://agent/claude".into(),
            ..default()
        });
    app.world_mut().write_message(PageAgentAcpTerminalCreated {
        sid: "s1".into(),
        terminal_id: "terminal-1".into(),
        process_id: vmux_core::ProcessId::new(),
        command: "echo".into(),
        args: vec!["hi".into()],
        cwd: Some("/tmp".into()),
    });

    app.update();

    let stack_times = {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(Entity, &LastActivatedAt), With<Stack>>();
        query
            .iter(world)
            .map(|(entity, activated)| (entity, activated.0))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        stack_times
            .iter()
            .find(|(entity, _)| *entity == agent)
            .map(|(_, activated)| *activated),
        Some(10)
    );
    assert_eq!(
        stack_times
            .iter()
            .find(|(entity, _)| *entity != agent)
            .map(|(_, activated)| *activated),
        Some(0)
    );
}

#[test]
fn codex_acp_routes_shell_commands_through_vmux_run() {
    for agent_id in ["codex", "codex-acp"] {
        let env = apply_agent_compatibility_env(agent_id, Vec::new());
        let config = env
            .iter()
            .find(|(key, _)| key == "CODEX_CONFIG")
            .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
            .expect("codex ACP compatibility config");

        assert_eq!(config["features"]["shell_tool"], false);
        assert_eq!(config["features"]["unified_exec"], false);
        assert_eq!(config["tools"]["web_search"], false);
        assert_eq!(config["approvals_reviewer"], "user");
        assert_eq!(config["mcp_servers"]["vmux"]["tool_timeout_sec"], 660);
        assert_eq!(
            config["features"]["code_mode"]["direct_only_tool_namespaces"],
            serde_json::json!([crate::client::cli::codex::DIRECT_ONLY_NAMESPACE])
        );
        assert!(
            config["developer_instructions"]
                .as_str()
                .unwrap()
                .contains("mcp__vmux__run")
        );
        let instructions = config["developer_instructions"].as_str().unwrap();
        assert!(instructions.contains("mcp__vmux__set_conversation_title"));
        assert!(instructions.contains("first tool of the turn"));
        assert!(instructions.contains("raw first prompt as a provisional title"));
        assert!(instructions.contains("topic materially changes"));
        assert!(instructions.contains("same-topic follow-ups"));
        assert!(instructions.contains("never needs user permission"));
        assert!(instructions.contains("mcp__vmux__browser_snapshot"));
        assert!(instructions.contains("page already visible beside you"));
    }
}

#[test]
fn codex_acp_disables_session_skill_files() {
    let mut config = serde_json::json!({
        "skills": {
            "config": [
                {"path": "/tmp/knowledge/alpha/SKILL.md", "enabled": true},
                {"path": "/tmp/other", "enabled": true}
            ]
        }
    })
    .as_object()
    .unwrap()
    .clone();
    disable_codex_skills(
        &mut config,
        &[
            std::path::PathBuf::from("/tmp/knowledge/alpha/SKILL.md"),
            std::path::PathBuf::from("/tmp/knowledge/beta/SKILL.md"),
        ],
    );

    assert_eq!(config["skills"]["config"][0]["enabled"], false);
    assert_eq!(config["skills"]["config"][1]["enabled"], true);
    assert_eq!(
        config["skills"]["config"][2],
        serde_json::json!({"path": "/tmp/knowledge/beta/SKILL.md", "enabled": false})
    );
}

#[test]
fn claude_acp_extends_mcp_tool_timeout() {
    for agent_id in ["claude", "claude-acp"] {
        let env = apply_agent_compatibility_env(agent_id, vec![s("MCP_TOOL_TIMEOUT", "60000")]);
        assert_eq!(
            env.iter()
                .find(|(key, _)| key == "MCP_TOOL_TIMEOUT")
                .map(|(_, value)| value.as_str()),
            Some("660000")
        );
    }
}

#[test]
fn vibe_acp_routes_shell_commands_through_vmux_run() {
    let env = apply_agent_compatibility_env(
        "mistral-vibe",
        vec![
            s("VIBE_DISABLED_TOOLS", r#"["from-env"]"#),
            s(
                "VIBE_MCP_SERVERS",
                r#"[{"name":"from-env","transport":"stdio","command":"env-command"}]"#,
            ),
        ],
    );
    let disabled = env
        .iter()
        .find(|(key, _)| key == "VIBE_DISABLED_TOOLS")
        .map(|(_, value)| serde_json::from_str::<Vec<String>>(value).unwrap())
        .expect("Vibe ACP disabled tools");

    assert_eq!(disabled, vec!["from-env", "bash"]);
    let mcp_servers = env
        .iter()
        .find(|(key, _)| key == "VIBE_MCP_SERVERS")
        .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
        .expect("Vibe ACP MCP servers");
    assert_eq!(mcp_servers[0]["name"], "from-env");
}

#[test]
fn vibe_acp_discards_invalid_mcp_environment() {
    let env =
        apply_agent_compatibility_env("mistral-vibe", vec![s("VIBE_MCP_SERVERS", "not-json")]);

    assert!(env.iter().all(|(key, _)| key != "VIBE_MCP_SERVERS"));
}

#[test]
fn codex_acp_preserves_existing_config() {
    let env = apply_agent_compatibility_env(
        "codex",
        vec![s(
            "CODEX_CONFIG",
            r#"{"model":"gpt-test","features":{"custom_feature":true,"code_mode":{"custom_setting":"keep"}}}"#,
        )],
    );
    let config = env
        .iter()
        .find(|(key, _)| key == "CODEX_CONFIG")
        .map(|(_, value)| serde_json::from_str::<serde_json::Value>(value).unwrap())
        .unwrap();

    assert_eq!(config["model"], "gpt-test");
    assert_eq!(config["features"]["custom_feature"], true);
    assert_eq!(config["features"]["code_mode"]["custom_setting"], "keep");
    assert_eq!(config["features"]["shell_tool"], false);
}

#[test]
fn codex_acp_reports_discarded_invalid_config() {
    let (_, invalid_json) = parse_codex_config(Some("{not-json"));
    assert!(invalid_json.unwrap().contains("invalid JSON"));

    let (_, non_object) = parse_codex_config(Some("[]"));
    assert!(non_object.unwrap().contains("not a JSON object"));
}

#[test]
fn plugin_builds_and_runs_without_panic() {
    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .add_plugins(AcpAgentPlugin)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();
    app.world_mut().spawn(AcpSession {
        agent_id: "vibe-acp".to_string(),
        sid: "s1".to_string(),
        cwd: std::path::PathBuf::from("/tmp"),
        anchor: vmux_core::ProcessId::new(),
        resume: None,
    });
    app.update();
}
