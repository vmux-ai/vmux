use super::*;
use vmux_service::protocol::SharedMessage;

fn init_worktree_test_repo() -> tempfile::TempDir {
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
    repo
}

#[test]
fn create_worktree_precedes_and_gates_sibling_self_commands() {
    let anchor = ProcessId::new();
    let create = ServiceAgentCommand::CreateWorktreeOnBranch {
        anchor,
        branch: "feature/test".into(),
    };
    let sibling = ServiceAgentCommand::OpenBeside {
        anchor,
        direction: None,
        url: "https://example.com".into(),
        focus: false,
    };
    assert!(self_command_priority(&create) < self_command_priority(&sibling));
    let failed = std::collections::HashSet::from([anchor]);
    assert!(!self_command_blocked_by_worktree_failure(&create, &failed));
    assert!(self_command_blocked_by_worktree_failure(&sibling, &failed));
}

#[test]
fn workspace_selection_continuations_resume_original_request() {
    let ready = git_workspace_ready_continuation(Path::new("/repo/dashboard"));
    let plain = plain_workspace_ready_continuation(Path::new("/tmp/demo"));
    let cancelled = failed_workspace_continuation("The user cancelled project selection");

    assert!(ready.contains("same conversation"));
    assert!(ready.contains("Git project /repo/dashboard is ready"));
    assert!(ready.contains("Immediately before the first edit"));
    assert!(ready.contains("create_worktree"));
    assert!(plain.contains("Project /tmp/demo is ready without Git"));
    assert!(plain.contains("Do not call create_worktree"));
    assert!(cancelled.contains("Do not retry automatically"));
}

#[test]
fn selected_agent_choice_resumes_session() {
    let mut app = App::new();
    app.add_observer(handle_agent_choice_selected);
    let session = app.world_mut().spawn_empty().id();
    let webview = app
        .world_mut()
        .spawn(PendingAgentChoice {
            session_entity: session,
            action: PendingAgentChoiceAction::Resume,
            question: "Mode?".into(),
            options: vec!["Fast".into(), "Safe".into()],
        })
        .id();

    app.world_mut()
        .trigger(AgentChoiceSelected { webview, index: 1 });
    app.update();

    let continuation = app
        .world()
        .get::<PendingAgentContinuation>(session)
        .unwrap();
    assert!(continuation.0.contains("Safe"));
    assert!(app.world().get::<PendingAgentChoice>(webview).is_none());
}

#[test]
fn initialize_git_choice_uses_new_project_root_directly() {
    let workspace = tempfile::tempdir().unwrap();
    let workspace_path = workspace.path().canonicalize().unwrap();
    let mut app = App::new();
    app.add_observer(handle_agent_choice_selected);
    let session = app.world_mut().spawn_empty().id();
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Project".into(),
            startup_dir: Some(workspace_path.to_string_lossy().into_owned()),
        })
        .id();
    let webview = app
        .world_mut()
        .spawn(PendingAgentChoice {
            session_entity: session,
            action: PendingAgentChoiceAction::InitializeGit {
                tab_entity: tab,
                workspace: workspace_path.clone(),
            },
            question: INITIALIZE_GIT_QUESTION.into(),
            options: INITIALIZE_GIT_OPTIONS
                .into_iter()
                .map(str::to_string)
                .collect(),
        })
        .id();

    app.world_mut()
        .trigger(AgentChoiceSelected { webview, index: 0 });
    app.update();

    assert!(workspace_path.join(".git").is_dir());
    assert!(app.world().get::<RepositoryNeedsWorktree>(tab).is_none());
    assert!(
        app.world()
            .get::<PendingAgentContinuation>(session)
            .unwrap()
            .0
            .contains("Do not call create_worktree")
    );
}

#[test]
fn cli_workspace_continuation_queues_terminal_prompt_without_service_wait() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, send_pending_agent_continuations);
    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Codex,
            },
            PendingAgentContinuation("continue original request".to_string()),
        ))
        .id();

    app.update();

    assert!(
        app.world()
            .get::<PendingAgentContinuation>(entity)
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<vmux_terminal::BufferedAgentPrompt>(entity)
            .unwrap(),
        &vmux_terminal::BufferedAgentPrompt {
            text: "continue original request".to_string(),
            submit: true,
        }
    );
}

#[test]
fn chat_workspace_continuation_is_private_same_session_input() {
    assert!(matches!(
        chat_agent_continuation_message("sid-1", "continue original request"),
        ClientMessage::Shared(SharedMessage::Agent {
            sid,
            action: vmux_wire::protocol::AgentAction::Input { text, context, .. },
        })
            if sid == "sid-1"
                && text.is_empty()
                && context.as_deref() == Some("continue original request")
    ));
}

#[test]
fn worktree_activation_rebinds_existing_acp_session_without_replacing_view() {
    use bevy::ecs::system::RunSystemOnce;

    let repo = init_worktree_test_repo();
    let project_dir = repo.path().canonicalize().unwrap();
    let managed_root = tempfile::tempdir().unwrap();
    let activation = vmux_layout::worktree::create_worktree_for_branch_blocking(
        &project_dir,
        "feature/fun-terminal",
        managed_root.path(),
    )
    .unwrap();
    let execution_dir = activation.execution_dir.clone();
    let anchor = ProcessId::new();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn((
            vmux_layout::tab::Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            },
            PendingAgentProject(project_dir.clone()),
        ))
        .id();
    let pane = app.world_mut().spawn(ChildOf(tab)).id();
    let stack = app
        .world_mut()
        .spawn((
            crate::client::acp::AcpSession {
                agent_id: "claude".into(),
                sid: "routing-session".into(),
                cwd: process_cwd(),
                anchor,
                resume: None,
            },
            vmux_core::AgentWorkingDir(process_cwd().to_string_lossy().into_owned()),
            ChildOf(pane),
        ))
        .id();
    let view = app
        .world_mut()
        .spawn((crate::chat_page::AgentChatView, anchor, ChildOf(stack)))
        .id();

    let project_for_system = project_dir.clone();
    let rebind = app
        .world_mut()
        .run_system_once(
            move |mut tabs: Query<&mut vmux_layout::tab::Tab>,
                  mut sessions: Query<&mut crate::client::acp::AcpSession>,
                  child_of: Query<&ChildOf>,
                  mut commands: Commands| {
                activate_agent_worktree(
                    tab,
                    view,
                    &project_for_system,
                    activation.clone(),
                    &mut tabs,
                    &mut sessions,
                    &child_of,
                    &mut commands,
                )
            },
        )
        .unwrap()
        .unwrap()
        .1
        .unwrap();

    let tab_state = app.world().get::<vmux_layout::tab::Tab>(tab).unwrap();
    assert_eq!(
        tab_state.startup_dir.as_deref(),
        Some(execution_dir.to_string_lossy().as_ref())
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(tab)
            .unwrap()
            .project_dir,
        project_dir.to_string_lossy()
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(tab)
            .unwrap()
            .branch,
        "feature/fun-terminal"
    );
    assert!(
        app.world()
            .get::<vmux_layout::worktree::TabWorktreeReady>(tab)
            .is_some()
    );
    assert!(app.world().get::<PendingAgentProject>(tab).is_none());
    let session = app
        .world()
        .get::<crate::client::acp::AcpSession>(stack)
        .unwrap();
    assert_eq!(session.sid, "routing-session");
    assert_eq!(session.anchor, anchor);
    assert_eq!(session.cwd, execution_dir);
    assert_eq!(
        app.world()
            .get::<vmux_core::AgentWorkingDir>(stack)
            .unwrap()
            .0,
        execution_dir.to_string_lossy()
    );
    assert_eq!(app.world().get::<ChildOf>(view).unwrap().parent(), stack);
    assert!(
        app.world()
            .get::<crate::chat_page::AgentChatView>(view)
            .is_some()
    );
    assert!(matches!(
        rebind,
        ClientMessage::RebindAcpWorkspace { sid, cwd }
            if sid == "routing-session" && cwd == execution_dir.to_string_lossy()
    ));
}

#[test]
fn selected_workspace_binds_repository_without_eager_worktree_creation() {
    use bevy::ecs::system::RunSystemOnce;

    let repo = init_worktree_test_repo();
    let project_dir = repo.path().canonicalize().unwrap();
    let external_root = tempfile::tempdir().unwrap();
    let external = external_root.path().join("existing");
    vmux_git::worktree::worktree_add(&project_dir, &external, "feature/existing", "main").unwrap();
    let external = external.canonicalize().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let linked_tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Existing".into(),
            startup_dir: None,
        })
        .id();
    let linked_agent = app.world_mut().spawn(ChildOf(linked_tab)).id();
    let external_for_system = external.clone();
    let linked_execution = app
        .world_mut()
        .run_system_once(
            move |mut tabs: Query<&mut vmux_layout::tab::Tab>,
                  mut sessions: Query<&mut crate::client::acp::AcpSession>,
                  child_of: Query<&ChildOf>,
                  mut commands: Commands| {
                activate_selected_workspace(
                    linked_tab,
                    linked_agent,
                    &external_for_system,
                    &mut tabs,
                    &mut sessions,
                    &child_of,
                    &mut commands,
                )
            },
        )
        .unwrap()
        .unwrap()
        .0;

    assert_eq!(linked_execution, external);
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(linked_tab)
            .is_none()
    );
    assert!(
        app.world()
            .get::<RepositoryNeedsWorktree>(linked_tab)
            .is_none()
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(linked_tab)
            .unwrap()
            .project_dir,
        external.to_string_lossy()
    );
    assert_eq!(
        vmux_git::worktree::worktree_list(&project_dir)
            .unwrap()
            .len(),
        2
    );

    let managed_tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Managed".into(),
            startup_dir: None,
        })
        .id();
    let managed_agent = app.world_mut().spawn(ChildOf(managed_tab)).id();
    let project_for_system = project_dir.clone();
    let managed_execution = app
        .world_mut()
        .run_system_once(
            move |mut tabs: Query<&mut vmux_layout::tab::Tab>,
                  mut sessions: Query<&mut crate::client::acp::AcpSession>,
                  child_of: Query<&ChildOf>,
                  mut commands: Commands| {
                activate_selected_workspace(
                    managed_tab,
                    managed_agent,
                    &project_for_system,
                    &mut tabs,
                    &mut sessions,
                    &child_of,
                    &mut commands,
                )
            },
        )
        .unwrap()
        .unwrap()
        .0;

    assert_eq!(managed_execution, project_dir);
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(managed_tab)
            .is_none()
    );
    assert!(
        app.world()
            .get::<RepositoryNeedsWorktree>(managed_tab)
            .is_some()
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(managed_tab)
            .unwrap()
            .project_dir,
        project_dir.to_string_lossy()
    );
    assert_eq!(
        vmux_git::worktree::worktree_list(&project_dir)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn selected_workspace_binds_non_git_directory_without_worktree() {
    use bevy::ecs::system::RunSystemOnce;

    let directory = tempfile::tempdir().unwrap();
    let selected = directory.path().canonicalize().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Create".into(),
            startup_dir: None,
        })
        .id();
    let agent = app.world_mut().spawn(ChildOf(tab)).id();
    let selected_for_system = selected.clone();

    let (execution_dir, _, kind) = app
        .world_mut()
        .run_system_once(
            move |mut tabs: Query<&mut vmux_layout::tab::Tab>,
                  mut sessions: Query<&mut crate::client::acp::AcpSession>,
                  child_of: Query<&ChildOf>,
                  mut commands: Commands| {
                activate_selected_workspace(
                    tab,
                    agent,
                    &selected_for_system,
                    &mut tabs,
                    &mut sessions,
                    &child_of,
                    &mut commands,
                )
            },
        )
        .unwrap()
        .unwrap();

    assert_eq!(execution_dir, selected);
    assert_eq!(kind, SelectedWorkspaceKind::Plain);
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(tab)
            .unwrap()
            .project_dir,
        selected.to_string_lossy()
    );
    assert!(app.world().get::<RepositoryNeedsWorktree>(tab).is_none());
}

#[test]
fn worktree_candidates_resolve_known_path_and_offer_create_when_ambiguous() {
    let repo = init_worktree_test_repo();
    let project_dir = repo.path().canonicalize().unwrap();
    let roots = tempfile::tempdir().unwrap();
    let first = roots.path().join("first");
    let second = roots.path().join("second");
    vmux_git::worktree::worktree_add(&project_dir, &first, "feature/first", "main").unwrap();
    vmux_git::worktree::worktree_add(&project_dir, &second, "feature/second", "main").unwrap();

    let candidates = existing_worktree_candidates(&project_dir).unwrap();
    let resolved = resolve_requested_worktree(&project_dir, &first).unwrap();
    let message = ambiguous_worktree_message(&candidates);

    assert_eq!(candidates.len(), 2);
    assert_eq!(resolved.branch, "feature/first");
    assert!(message.contains("1. Create new worktree"));
    assert!(message.contains("feature/first"));
    assert!(message.contains("feature/second"));
    assert!(message.contains("create=true"));
}

fn swap_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_core::agent::SwapStackSession>()
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_swap_stack_session);
    app
}

fn spawn_stack_child(app: &mut App) -> (Entity, Entity) {
    let stack = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn(ChildOf(stack)).id();
    (stack, child)
}

#[test]
fn invalid_swap_target_preserves_current_stack_child() {
    let mut app = swap_test_app();
    let (stack, child) = spawn_stack_child(&mut app);
    app.world_mut()
        .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
        .write(vmux_core::agent::SwapStackSession {
            stack,
            target_url: "not-an-agent-url".to_string(),
            cwd: std::path::PathBuf::from("/work"),
            handoff: None,
        });

    app.update();

    assert!(app.world().get_entity(child).is_ok());
}

#[test]
fn unconfigured_acp_swap_target_preserves_current_stack_child() {
    let mut app = swap_test_app();
    let (stack, child) = spawn_stack_child(&mut app);
    app.world_mut()
        .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
        .write(vmux_core::agent::SwapStackSession {
            stack,
            target_url: "vmux://agent/not-configured/sid-1".to_string(),
            cwd: std::path::PathBuf::from("/work"),
            handoff: None,
        });

    app.update();

    assert!(app.world().get_entity(child).is_ok());
}

#[test]
fn cross_agent_swap_attaches_fresh_target_with_imported_history() {
    let mut app = swap_test_app();
    let (stack, _child) = spawn_stack_child(&mut app);
    let messages = vec![crate::Message::user("fix auth")];
    app.world_mut()
        .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
        .write(vmux_core::agent::SwapStackSession {
            stack,
            target_url: "vmux://agent/claude".to_string(),
            cwd: std::path::PathBuf::from("/source/work"),
            handoff: Some(vmux_core::agent::StackSessionHandoff {
                source_agent: "Codex".into(),
                source_kind: AgentKind::Codex,
                source_sid: "cx-1".into(),
                messages_json: serde_json::to_string(&messages).unwrap(),
                context: "prior conversation".into(),
                truncated: false,
            }),
        });

    app.update();

    let session = app.world().get::<crate::AcpSession>(stack).unwrap();
    assert_eq!(session.agent_id, "claude");
    assert_eq!(session.cwd, std::path::PathBuf::from("/source/work"));
    assert!(session.resume.is_none());
    let imported = app
        .world()
        .get::<crate::handoff::ImportedConversation>(stack)
        .unwrap();
    assert_eq!(imported.source_agent, "Codex");
    assert_eq!(imported.messages, messages);
    let pending = app
        .world()
        .get::<crate::handoff::PendingHandoff>(stack)
        .unwrap();
    assert_eq!(pending.context, "prior conversation");
    assert!(!pending.sent);
}

#[test]
fn acp_swap_resets_install_marker() {
    let mut app = swap_test_app();
    let (stack, _child) = spawn_stack_child(&mut app);
    app.world_mut()
        .entity_mut(stack)
        .insert(crate::client::acp::AcpInstallStarted);
    app.world_mut()
        .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
        .write(vmux_core::agent::SwapStackSession {
            stack,
            target_url: "vmux://agent/codex/session-2".to_string(),
            cwd: std::path::PathBuf::from("/work"),
            handoff: None,
        });

    app.update();

    assert!(
        app.world()
            .get::<crate::client::acp::AcpInstallStarted>(stack)
            .is_none()
    );
    let session = app.world().get::<crate::AcpSession>(stack).unwrap();
    assert_eq!(session.resume.as_deref(), Some("session-2"));
}

#[test]
fn acp_attach_gives_profile_agent_and_icon() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>();
    let stack = app.world_mut().spawn_empty().id();

    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut mt: ResMut<Assets<WebviewExtendStandardMaterial>>| {
                attach_acp_agent_to_stack(
                    stack,
                    "mistral-vibe",
                    "Mistral Vibe",
                    "sid-1",
                    std::path::Path::new("/tmp"),
                    Some("https://cdn.example/vibe.svg"),
                    None,
                    &mut commands,
                    &mut meshes,
                    &mut mt,
                );
            },
        )
        .unwrap();

    let world = app.world();
    let profile = world
        .get::<vmux_core::team::Profile>(stack)
        .expect("profile");
    assert_eq!(profile.name, "Mistral Vibe");
    let agent = world.get::<vmux_core::team::Agent>(stack).expect("agent");
    assert_eq!(agent.sid, "sid-1");
    assert_eq!(agent.kind, None);
    let meta = world.get::<PageMetadata>(stack).expect("meta");
    assert_eq!(meta.icon.favicon_url(), "https://cdn.example/vibe.svg");
}

#[test]
fn acp_icon_for_id_reads_catalog() {
    use crate::acp_registry::{Distribution, RegistryAgent};
    let catalog = crate::client::acp::AcpCatalog {
        agents: vec![
            RegistryAgent {
                id: "mistral-vibe".to_string(),
                name: "Mistral Vibe".to_string(),
                version: None,
                description: None,
                icon: Some("https://cdn.example/vibe.svg".to_string()),
                repository: None,
                distribution: Distribution::default(),
            },
            RegistryAgent {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                version: None,
                description: None,
                icon: Some("https://cdn.example/claude.svg".to_string()),
                repository: None,
                distribution: Distribution::default(),
            },
        ],
    };
    assert_eq!(
        acp_icon_for_id(Some(&catalog), "mistral-vibe").as_deref(),
        Some("https://cdn.example/vibe.svg")
    );
    assert_eq!(
        acp_icon_for_id(Some(&catalog), "claude").as_deref(),
        Some("https://cdn.example/claude.svg")
    );
    assert_eq!(acp_icon_for_id(Some(&catalog), "absent"), None);
    assert_eq!(acp_icon_for_id(None, "mistral-vibe"), None);
}

#[test]
fn acp_profile_name_prefers_registry_then_config_then_id() {
    use crate::acp_registry::{Distribution, RegistryAgent};
    use vmux_setting::AcpAgentConfig;

    let mut config = AcpAgentConfig {
        id: "claude".into(),
        name: "Configured Claude".into(),
        command: "npx".into(),
        args: vec![],
        env: vec![],
        cwd: None,
        version: None,
    };
    let catalog = crate::client::acp::AcpCatalog {
        agents: vec![RegistryAgent {
            id: "claude-acp".into(),
            name: "Claude".into(),
            version: None,
            description: None,
            icon: None,
            repository: None,
            distribution: Distribution::default(),
        }],
    };

    assert_eq!(
        acp_profile_name_for_id(&config.id, Some(&config), Some(&catalog)),
        "Claude"
    );
    assert_eq!(
        acp_profile_name_for_id(&config.id, Some(&config), None),
        "Configured Claude"
    );
    config.name = "   ".into();
    assert_eq!(
        acp_profile_name_for_id(&config.id, Some(&config), None),
        "claude"
    );
}

#[test]
fn acp_target_id_accepts_registry_alias_config() {
    let config = vmux_setting::AcpAgentConfig {
        id: "claude-acp".into(),
        name: "Claude".into(),
        command: "npx".into(),
        args: vec![],
        env: vec![],
        cwd: None,
        version: None,
    };

    assert_eq!(
        acp_target_id_for_kind(AgentKind::Claude, &[config], None).as_deref(),
        Some("claude-acp")
    );
}

#[test]
fn resume_in_acp_command_swaps_current_cli_stack() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::agent::SwapStackSession>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_resume_in_acp);
    let stack = app.world_mut().spawn_empty().id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((
        Terminal,
        anchor,
        ChildOf(stack),
        AgentSession {
            kind: AgentKind::Claude,
        },
        SessionId("session-7".into()),
        TerminalLaunch {
            command: "claude".into(),
            args: vec![],
            cwd: "/workspace/project".into(),
            env: vec![],
            kind: vmux_terminal::launch::TerminalKind::Claude,
        },
    ));
    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::ResumeInAcp { anchor },
        });

    app.update();

    let swaps: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::agent::SwapStackSession>>()
        .drain()
        .collect();
    assert_eq!(swaps.len(), 1);
    assert_eq!(swaps[0].stack, stack);
    assert_eq!(swaps[0].target_url, "vmux://agent/claude/session-7");
    assert_eq!(swaps[0].cwd, PathBuf::from("/workspace/project"));
    assert!(swaps[0].handoff.is_none());
}
use vmux_layout::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use vmux_setting::{BrowserSettings, ShortcutSettings};
use vmux_terminal::Terminal;

#[test]
fn file_touch_url_builds_goto_fragment() {
    assert_eq!(
        file_touch_url("/a/b.rs", None, None, None),
        "file:///a/b.rs"
    );
    assert_eq!(
        file_touch_url("/a/b.rs", Some(10), None, None),
        "file:///a/b.rs#L10"
    );
    assert_eq!(
        file_touch_url("/a/b.rs", Some(10), Some(5), Some(12)),
        "file:///a/b.rs#L10:5-12"
    );
}

fn file_touch_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_editor::FileViewModeRequest>()
        .add_message::<vmux_layout::worktree::TabDirectoryObserved>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_agent_file_touch);
    app
}

fn spawn_file_touch_layout(app: &mut App, old_url: &str, dirty: bool) -> (ProcessId, Entity) {
    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let agent_pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let agent_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((anchor, ChildOf(agent_stack)));
    let file_pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let file_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(file_pane)))
        .id();
    app.world_mut().spawn((
        vmux_core::PageMetadata {
            url: old_url.to_string(),
            ..default()
        },
        vmux_git::GitDiffSource { dirty, ..default() },
        ChildOf(file_stack),
    ));
    (anchor, file_stack)
}

fn send_file_touch(
    app: &mut App,
    anchor: ProcessId,
    path: &str,
    kind: vmux_service::protocol::FileTouchKind,
) {
    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::FileTouched {
                anchor,
                path: path.to_string(),
                line: None,
                col: None,
                end_col: None,
                kind,
            },
        });
}

fn send_file_read(app: &mut App, anchor: ProcessId, path: &str) {
    send_file_touch(
        app,
        anchor,
        path,
        vmux_service::protocol::FileTouchKind::Read,
    );
}

fn send_file_edit(app: &mut App, anchor: ProcessId, path: &str) {
    send_file_touch(
        app,
        anchor,
        path,
        vmux_service::protocol::FileTouchKind::Edit,
    );
}

#[test]
fn file_read_replaces_clean_follow_stack() {
    let mut app = file_touch_test_app();
    let (anchor, file_stack) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
    send_file_read(&mut app, anchor, "/repo/new.rs");

    app.update();

    let opens: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
        .drain()
        .collect();
    assert_eq!(opens.len(), 1);
    assert!(matches!(
        opens[0].target,
        vmux_core::PageOpenTarget::Stack(stack) if stack == file_stack
    ));
    assert_eq!(opens[0].url, "file:///repo/new.rs");
    let beside = app
        .world_mut()
        .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
        .drain()
        .count();
    assert_eq!(beside, 0);
}

#[test]
fn file_read_replaces_clean_follow_stack_across_nested_split() {
    let mut app = file_touch_test_app();
    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let root = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: vmux_layout::pane::PaneSplitDirection::Row,
            },
            ChildOf(tab),
        ))
        .id();
    let agent_pane = app.world_mut().spawn((Pane, ChildOf(root))).id();
    let agent_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((anchor, ChildOf(agent_stack)));
    let nested = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: vmux_layout::pane::PaneSplitDirection::Column,
            },
            ChildOf(root),
        ))
        .id();
    app.world_mut().spawn((Pane, ChildOf(nested)));
    let file_pane = app.world_mut().spawn((Pane, ChildOf(nested))).id();
    let file_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(file_pane)))
        .id();
    app.world_mut().spawn((
        vmux_core::PageMetadata {
            url: "file:///repo/old.rs".into(),
            ..default()
        },
        vmux_git::GitDiffSource::default(),
        ChildOf(file_stack),
    ));
    send_file_read(&mut app, anchor, "/repo/new.rs");

    app.update();

    let opens: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
        .drain()
        .collect();
    assert_eq!(opens.len(), 1);
    assert!(matches!(
        opens[0].target,
        vmux_core::PageOpenTarget::Stack(stack) if stack == file_stack
    ));
    assert_eq!(opens[0].url, "file:///repo/new.rs");
}

#[test]
fn file_search_forwards_results_to_editor() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_editor::GlobalSearchRequest>()
        .add_systems(Update, handle_agent_file_search);
    let anchor = ProcessId::new();
    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::FileSearch {
                anchor,
                root: "/repo".into(),
                query: "needle".into(),
                matches: vec![vmux_service::protocol::FileSearchMatch {
                    path: "/repo/src/main.rs".into(),
                    line: 9,
                    col: 4,
                    end_col: 10,
                    preview: "let needle = true;".into(),
                }],
            },
        });

    app.update();

    let requests: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_editor::GlobalSearchRequest>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target_path, PathBuf::from("/repo/src/main.rs"));
    assert_eq!(requests[0].query, "needle");
    assert_eq!(requests[0].matches[0].line, 9);
}

#[test]
fn same_frame_file_reads_replace_once_with_last_touch() {
    let mut app = file_touch_test_app();
    let (anchor, file_stack) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
    send_file_read(&mut app, anchor, "/repo/first.rs");
    send_file_read(&mut app, anchor, "/repo/second.rs");
    send_file_read(&mut app, anchor, "/repo/first.rs");

    app.update();

    let opens: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
        .drain()
        .collect();
    assert_eq!(opens.len(), 1);
    assert!(matches!(
        opens[0].target,
        vmux_core::PageOpenTarget::Stack(stack) if stack == file_stack
    ));
    assert_eq!(opens[0].url, "file:///repo/first.rs");
    let view_modes = app
        .world_mut()
        .resource_mut::<Messages<vmux_editor::FileViewModeRequest>>()
        .drain()
        .count();
    assert_eq!(view_modes, 0);
}

#[test]
fn same_frame_file_edits_open_each_distinct_file_as_tabs() {
    let mut app = file_touch_test_app();
    let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", false);
    send_file_edit(&mut app, anchor, "/repo/first.rs");
    send_file_edit(&mut app, anchor, "/repo/second.rs");
    send_file_edit(&mut app, anchor, "/repo/first.rs");

    app.update();

    let opens = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
        .drain()
        .count();
    assert_eq!(opens, 0);
    let beside: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
        .drain()
        .collect();
    assert_eq!(beside.len(), 2);
    assert_eq!(beside[0].url, "file:///repo/first.rs");
    assert_eq!(beside[1].url, "file:///repo/second.rs");
    let view_modes: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_editor::FileViewModeRequest>>()
        .drain()
        .collect();
    assert_eq!(
        view_modes,
        vec![vmux_editor::FileViewModeRequest(
            vmux_core::event::FileViewMode::Diff
        )]
    );
}

#[test]
fn file_read_preserves_dirty_follow_stack() {
    let mut app = file_touch_test_app();
    let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/old.rs", true);
    send_file_read(&mut app, anchor, "/repo/new.rs");

    app.update();

    let opens = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
        .drain()
        .count();
    assert_eq!(opens, 0);
    let beside: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
        .drain()
        .collect();
    assert_eq!(beside.len(), 1);
    assert_eq!(beside[0].url, "file:///repo/new.rs");
}

#[test]
fn file_read_does_not_reload_matching_dirty_page() {
    let mut app = file_touch_test_app();
    let (anchor, _) = spawn_file_touch_layout(&mut app, "file:///repo/current.rs", true);
    send_file_read(&mut app, anchor, "/repo/current.rs");

    app.update();

    let opens = app
        .world_mut()
        .resource_mut::<Messages<vmux_core::PageOpenRequest>>()
        .drain()
        .count();
    let beside = app
        .world_mut()
        .resource_mut::<Messages<vmux_layout::OpenBesideRequest>>()
        .drain()
        .count();
    assert_eq!((opens, beside), (0, 0));
}

#[test]
fn skill_file_read_does_not_open_follow_pane() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_editor::FileViewModeRequest>()
        .add_message::<vmux_layout::worktree::TabDirectoryObserved>()
        .insert_resource(test_settings())
        .add_systems(Update, handle_agent_file_touch);

    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((anchor, ChildOf(stack)));

    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::FileTouched {
                anchor,
                path: "/Users/me/.agents/skills/caveman/SKILL.md".into(),
                line: None,
                col: None,
                end_col: None,
                kind: vmux_service::protocol::FileTouchKind::Read,
            },
        });

    app.update();

    let previews = app
        .world()
        .resource::<Messages<vmux_layout::OpenBesideRequest>>();
    let mut preview_cursor = previews.get_cursor();
    assert_eq!(preview_cursor.read(previews).count(), 0);
    let observations = app
        .world()
        .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
    let mut observation_cursor = observations.get_cursor();
    assert_eq!(observation_cursor.read(observations).count(), 0);
}

#[test]
fn file_touch_emits_tab_directory_observation_when_file_follow_is_disabled() {
    let mut settings = test_settings();
    settings.agent.follow_files = false;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_editor::FileViewModeRequest>()
        .add_message::<vmux_layout::worktree::TabDirectoryObserved>()
        .insert_resource(settings)
        .add_systems(Update, handle_agent_file_touch);

    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((anchor, ChildOf(stack)));
    let path = std::env::temp_dir().join("vmux-observed-file.rs");

    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::FileTouched {
                anchor,
                path: path.to_string_lossy().into_owned(),
                line: None,
                col: None,
                end_col: None,
                kind: vmux_service::protocol::FileTouchKind::Read,
            },
        });

    app.update();

    let messages = app
        .world()
        .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
    let mut cursor = messages.get_cursor();
    let observations: Vec<_> = cursor.read(messages).cloned().collect();
    assert_eq!(
        observations,
        vec![vmux_layout::worktree::TabDirectoryObserved {
            tab,
            path,
            kind: vmux_layout::worktree::TabDirectoryObservationKind::Read,
        }]
    );
    let previews = app
        .world()
        .resource::<Messages<vmux_layout::OpenBesideRequest>>();
    let mut preview_cursor = previews.get_cursor();
    assert_eq!(
        preview_cursor.read(previews).count(),
        0,
        "file-follow setting still controls preview panes"
    );
}

#[test]
fn file_touch_rejects_command_anchor_mismatched_with_origin() {
    let mut settings = test_settings();
    settings.agent.follow_files = false;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_editor::FileViewModeRequest>()
        .add_message::<vmux_layout::worktree::TabDirectoryObserved>()
        .insert_resource(settings)
        .add_systems(Update, handle_agent_file_touch);

    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    let command_anchor = ProcessId::new();
    app.world_mut().spawn((command_anchor, ChildOf(stack)));
    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(ProcessId::new()),
            },
            command: ServiceAgentCommand::FileTouched {
                anchor: command_anchor,
                path: std::env::temp_dir()
                    .join("vmux-mismatched-anchor.rs")
                    .to_string_lossy()
                    .into_owned(),
                line: None,
                col: None,
                end_col: None,
                kind: vmux_service::protocol::FileTouchKind::Read,
            },
        });

    app.update();

    let messages = app
        .world()
        .resource::<Messages<vmux_layout::worktree::TabDirectoryObserved>>();
    let mut cursor = messages.get_cursor();
    assert_eq!(cursor.read(messages).count(), 0);
}

#[test]
fn edit_file_touch_rebinds_tab_in_same_frame() {
    #[derive(Resource)]
    struct RunTab(Entity);

    #[derive(Resource, Default)]
    struct CapturedRunCwd(Option<PathBuf>);

    fn capture_run_cwd(
        mut reader: MessageReader<AgentCommandRequest>,
        run_tab: Res<RunTab>,
        tabs: Query<&vmux_layout::tab::Tab>,
        mut captured: ResMut<CapturedRunCwd>,
    ) {
        for request in reader.read() {
            if matches!(request.command, ServiceAgentCommand::Run { .. }) {
                let tab = tabs.get(run_tab.0).unwrap();
                captured.0 = run_terminal_cwd(tab.startup_dir.as_deref(), None).ok();
            }
        }
    }

    struct TestRepo(PathBuf);

    impl TestRepo {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn init_repo(name: &str) -> TestRepo {
        let path = std::env::temp_dir().join(format!(
            "vmux-agent-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = TestRepo(path);
        git(repo.path(), &["init", "-q", "-b", "main"]);
        git(repo.path(), &["config", "user.email", "t@example.com"]);
        git(repo.path(), &["config", "user.name", "Test"]);
        git(repo.path(), &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
        git(repo.path(), &["add", "seed.txt"]);
        git(repo.path(), &["commit", "-qm", "init"]);
        repo
    }

    let current = init_repo("current");
    let observed = init_repo("observed");
    let expected = observed
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut settings = test_settings();
    settings.agent.follow_files = false;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, vmux_layout::worktree::WorktreePlugin))
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_editor::FileViewModeRequest>()
        .init_resource::<CapturedRunCwd>()
        .insert_resource(settings)
        .add_systems(
            Update,
            (
                handle_agent_file_touch.before(vmux_layout::worktree::TabDirectoryRebindSet),
                capture_run_cwd.after(vmux_layout::worktree::TabDirectoryRebindSet),
            ),
        );
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "test".into(),
            startup_dir: Some(current.path().to_string_lossy().into_owned()),
        })
        .id();
    app.insert_resource(RunTab(tab));
    let pane = app.world_mut().spawn((Pane, ChildOf(tab))).id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((anchor, ChildOf(stack)));
    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::FileTouched {
                anchor,
                path: observed
                    .path()
                    .join("seed.txt")
                    .to_string_lossy()
                    .into_owned(),
                line: None,
                col: None,
                end_col: None,
                kind: vmux_service::protocol::FileTouchKind::Edit,
            },
        });
    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::Run {
                anchor,
                command: "pwd".into(),
                direction: vmux_service::protocol::AgentPaneDirection::Right,
                focus: false,
                beside: None,
                mode: vmux_service::protocol::PlacementMode::Auto,
                terminal: None,
                done_marker: None,
            },
        });

    app.update();

    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir
            .as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(
        app.world().resource::<CapturedRunCwd>().0.as_deref(),
        Some(observed.path().canonicalize().unwrap().as_path())
    );
}

fn bell_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_core::notify::BellReceived>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, agent_bell_to_attention);
    app
}

fn spawn_agent_with_pid(app: &mut App, pid: vmux_service::protocol::ProcessId) -> Entity {
    app.world_mut()
        .spawn((
            vmux_core::team::Agent {
                sid: "s".to_string(),
                kind: Some(vmux_core::agent::AgentKind::Claude),
            },
            pid,
        ))
        .id()
}

fn attentions(app: &App) -> Vec<Entity> {
    let messages = app
        .world()
        .resource::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>();
    let mut cursor = messages.get_cursor();
    cursor.read(messages).map(|a| a.entity).collect()
}

fn turn_end_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<AgentCommandRequest>()
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_systems(Update, handle_agent_turn_ended);
    app
}

fn send_turn_ended(app: &mut App, anchor: vmux_service::protocol::ProcessId) {
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: vmux_service::protocol::AgentRequestId::new(),
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: Some(anchor),
            },
            command: ServiceAgentCommand::TurnEnded { anchor },
        });
}

#[test]
fn turn_ended_resolves_to_agent_attention() {
    let mut app = turn_end_test_app();
    let pid = vmux_service::protocol::ProcessId::new();
    let agent = spawn_agent_with_pid(&mut app, pid);
    send_turn_ended(&mut app, pid);
    app.update();
    assert_eq!(attentions(&app), vec![agent]);
}

#[test]
fn turn_ended_unknown_anchor_emits_nothing() {
    let mut app = turn_end_test_app();
    let _agent = spawn_agent_with_pid(&mut app, vmux_service::protocol::ProcessId::new());
    send_turn_ended(&mut app, vmux_service::protocol::ProcessId::new());
    app.update();
    assert!(attentions(&app).is_empty());
}

#[test]
fn bell_resolves_to_agent_attention() {
    use vmux_service::protocol::ProcessId;
    let mut app = bell_test_app();
    let pid = ProcessId::new();
    let agent = spawn_agent_with_pid(&mut app, pid);
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::BellReceived>>()
        .write(vmux_core::notify::BellReceived { process_id: pid });
    app.update();
    assert_eq!(attentions(&app), vec![agent]);
}

#[test]
fn bell_unknown_process_id_emits_nothing() {
    use vmux_service::protocol::ProcessId;
    let mut app = bell_test_app();
    let _agent = spawn_agent_with_pid(&mut app, ProcessId::new());
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::BellReceived>>()
        .write(vmux_core::notify::BellReceived {
            process_id: ProcessId::new(),
        });
    app.update();
    assert!(attentions(&app).is_empty());
}

fn done_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_core::notify::AgentAttention>()
        .add_message::<vmux_core::notify::OsNotify>()
        .init_resource::<vmux_layout::stack::FocusedStack>()
        .add_systems(Update, (mark_agent_done, clear_agent_done));
    app
}

fn spawn_agent_in_stack(app: &mut App) -> (Entity, Entity) {
    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::Stack::default())
        .id();
    let agent = app
        .world_mut()
        .spawn((
            vmux_core::team::Profile::agent(vmux_core::agent::AgentKind::Claude),
            ChildOf(stack),
        ))
        .id();
    (agent, stack)
}

fn set_window(app: &mut App, focused: bool) {
    app.world_mut().spawn((
        Window {
            focused,
            visible: true,
            ..default()
        },
        bevy::window::PrimaryWindow,
    ));
}

fn os_notify_count(app: &App) -> usize {
    let messages = app
        .world()
        .resource::<bevy::ecs::message::Messages<vmux_core::notify::OsNotify>>();
    let mut cursor = messages.get_cursor();
    cursor.read(messages).count()
}

fn send_attention(app: &mut App, entity: Entity) {
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
        .write(vmux_core::notify::AgentAttention {
            entity,
            title: None,
            body: None,
        });
}

#[test]
fn done_notifies_and_marks_when_backgrounded() {
    let mut app = done_test_app();
    let (agent, _stack) = spawn_agent_in_stack(&mut app);
    set_window(&mut app, false);
    send_attention(&mut app, agent);
    app.update();
    assert!(
        app.world()
            .get::<vmux_core::notify::AgentDoneUnseen>(agent)
            .is_some()
    );
    assert_eq!(os_notify_count(&app), 1);
}

#[test]
fn focused_child_agent_does_not_notify_or_mark() {
    let mut app = done_test_app();
    let (agent, stack) = spawn_agent_in_stack(&mut app);
    set_window(&mut app, true);
    app.world_mut()
        .resource_mut::<vmux_layout::stack::FocusedStack>()
        .stack = Some(stack);
    app.update();
    send_attention(&mut app, agent);
    app.update();
    assert!(
        app.world()
            .get::<vmux_core::notify::AgentDoneUnseen>(agent)
            .is_none(),
        "focused agent has no unseen marker"
    );
    assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
}

#[test]
fn focused_stack_agent_does_not_notify_or_mark() {
    let mut app = done_test_app();
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::Stack::default(),
            vmux_core::team::Profile::agent(vmux_core::agent::AgentKind::Claude),
        ))
        .id();
    set_window(&mut app, true);
    app.world_mut()
        .resource_mut::<vmux_layout::stack::FocusedStack>()
        .stack = Some(stack);
    app.update();
    send_attention(&mut app, stack);
    app.update();
    assert!(
        app.world()
            .get::<vmux_core::notify::AgentDoneUnseen>(stack)
            .is_none(),
        "focused stack agent has no unseen marker"
    );
    assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
}

#[test]
fn clear_removes_marker_from_focused_stack_agent() {
    let mut app = done_test_app();
    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::Stack::default())
        .id();
    set_window(&mut app, true);
    app.world_mut()
        .entity_mut(stack)
        .insert(vmux_core::notify::AgentDoneUnseen);
    app.update();
    assert!(
        app.world()
            .get::<vmux_core::notify::AgentDoneUnseen>(stack)
            .is_some()
    );
    app.world_mut()
        .resource_mut::<vmux_layout::stack::FocusedStack>()
        .stack = Some(stack);
    app.update();
    assert!(
        app.world()
            .get::<vmux_core::notify::AgentDoneUnseen>(stack)
            .is_none()
    );
}

#[test]
fn screenshot_response_maps_ok_and_err() {
    let ok = screenshot_response_to_query_result(&Ok(ScreenshotImage {
        path: "/tmp/a.png".into(),
        png: vec![9, 8, 7],
        width: 10,
        height: 20,
    }));
    assert!(matches!(
        ok,
        AgentQueryResult::Image { path, png, width, height }
            if path == "/tmp/a.png" && png == vec![9, 8, 7] && width == 10 && height == 20
    ));

    let err = screenshot_response_to_query_result(&Err("nope".to_string()));
    assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
}

#[test]
fn record_start_response_maps_ok_and_err() {
    let ok = record_start_response_to_query_result(&Ok(120));
    assert!(matches!(ok, AgentQueryResult::Text(t) if t.contains("120")));
    let err = record_start_response_to_query_result(&Err("nope".to_string()));
    assert!(matches!(err, AgentQueryResult::Error(m) if m == "nope"));
}

#[test]
fn record_stop_response_maps_ok_and_err() {
    let ok = record_stop_response_to_query_result(&Ok(RecordingInfo {
        mp4_path: "/tmp/x.mp4".into(),
        gif_path: None,
        duration_ms: 1000,
        bytes: 42,
        auto_stopped: false,
    }));
    assert!(matches!(ok, AgentQueryResult::Recording { mp4_path, .. } if mp4_path == "/tmp/x.mp4"));
    let err = record_stop_response_to_query_result(&Err("boom".to_string()));
    assert!(matches!(err, AgentQueryResult::Error(m) if m == "boom"));
}

pub(super) fn test_settings() -> AppSettings {
    AppSettings {
        browser: BrowserSettings {
            startup_url: "about:blank".to_string(),
            ..Default::default()
        },
        layout: LayoutSettings {
            radius: 0.0,
            window: WindowSettings { padding: 0.0 },
            pane: PaneSettings { gap: 0.0 },
            side_sheet: SideSheetSettings::default(),
            focus_ring: FocusRingSettings::default(),
        },
        shortcuts: ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        agent: vmux_setting::AgentSettings::default(),
        spaces: Default::default(),
        recording: Default::default(),
        editor: Default::default(),
        appearance: Default::default(),
    }
}

#[test]
fn blank_cwd_is_accepted() {
    assert_eq!(valid_cwd("").unwrap(), None);
}

#[test]
fn restart_rebuilds_args_with_new_anchor() {
    let temp = std::env::temp_dir().join(format!("vmux-restart-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(temp.join("Cargo.toml"), b"[workspace]\n").unwrap();
    let launch = TerminalLaunch {
        command: "/usr/local/bin/claude".into(),
        args: vec!["--mcp-config".into(), "OLD".into()],
        cwd: temp.to_string_lossy().to_string(),
        env: vec![],
        kind: vmux_core::terminal::TerminalKind::Claude,
    };
    let new_id = ProcessId::new();
    let (args, _env) = rebuilt_args_env_for_restart(
        &launch,
        &crate::client::cli::claude::ClaudeStrategy,
        None,
        new_id,
    );
    let _ = std::fs::remove_dir_all(&temp);
    let joined = args.join(" ");
    assert!(joined.contains("--anchor"), "args carry --anchor: {joined}");
    assert!(joined.contains(&new_id.to_string()), "anchor is the new id");
    assert!(
        !args
            .windows(2)
            .any(|pair| pair[0] == "--mcp-config" && pair[1] == "OLD"),
        "old args replaced"
    );
}

#[test]
fn deep_link_focuses_existing_claude_tab() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<crate::session::AgentSessionToEntity>()
        .add_systems(Update, crate::session::track_session_id_inserts);

    let entity = app
        .world_mut()
        .spawn((
            AgentSession {
                kind: AgentKind::Claude,
            },
            SessionId("dl-1".into()),
        ))
        .id();

    app.update();

    let map = app
        .world()
        .resource::<crate::session::AgentSessionToEntity>();
    assert_eq!(
        map.0.get(&(AgentKind::Claude, "dl-1".into())),
        Some(&entity)
    );
}

#[test]
fn agent_plugin_registers_all_three_provider_entries() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        vmux_command::CommandPlugin,
        AgentSessionPlugin,
    ));
    app.world_mut().run_schedule(Startup);
    let mut q = app.world_mut().query::<&AgentProviderTargetKind>();
    let ids: std::collections::HashSet<&'static str> =
        q.iter(app.world()).map(|p| p.0.as_url_segment()).collect();
    for id in ["vibe", "claude", "codex"] {
        assert!(ids.contains(id), "missing provider: {id}");
    }
}

#[test]
fn agent_plugin_registers_three_cli_strategies() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        vmux_command::CommandPlugin,
        AgentSessionPlugin,
    ));
    let strategies = app.world().resource::<AgentStrategies>();
    assert!(strategies.get_cli(AgentKind::Vibe).is_some());
    assert!(strategies.get_cli(AgentKind::Claude).is_some());
    assert!(strategies.get_cli(AgentKind::Codex).is_some());
}

#[test]
fn update_settings_via_apply_mutates_resource_and_returns_ron() {
    let mut settings = test_settings();
    let ron_bytes = vmux_setting::apply_settings_update(
        &mut settings,
        "browser.startup_url",
        serde_json::json!("https://example.com/custom"),
    )
    .expect("apply ok");
    assert_eq!(settings.browser.startup_url, "https://example.com/custom");
    // sparse RON includes only sections that differ from the embedded
    // defaults; this override differs, so it appears.
    assert!(ron_bytes.contains("https://example.com/custom"));
}

#[test]
fn run_placement_override_settings_update_rejects_agents_and_allows_users() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        vmux_command::CommandPlugin,
        AgentSessionPlugin,
    ))
    .add_message::<vmux_layout::BrowserNavigateRequest>()
    .add_message::<vmux_layout::BrowserGoBackRequest>()
    .add_message::<vmux_layout::BrowserGoForwardRequest>()
    .add_message::<vmux_layout::OpenInNewStackRequest>()
    .add_message::<vmux_layout::ExtensionInstallRequest>()
    .add_message::<vmux_layout::OpenBesideRequest>()
    .add_message::<vmux_layout::apply::LayoutApplyRequest>()
    .add_message::<vmux_layout::apply::LayoutApplyResponse>()
    .add_message::<vmux_layout::apply::LayoutSnapshotRequest>()
    .add_message::<vmux_layout::apply::LayoutSnapshotResponse>()
    .add_message::<vmux_terminal::TerminalSendRequest>()
    .add_message::<vmux_terminal::RunShellRequest>()
    .add_message::<vmux_setting::SettingsWriteRequest>()
    .add_message::<vmux_space::SpaceCommandRequest>()
    .add_message::<vmux_history::query::HistoryOpenIntent>()
    .insert_resource(FocusedStack::default())
    .insert_resource(test_settings())
    .init_resource::<Assets<Mesh>>()
    .init_resource::<Assets<WebviewExtendStandardMaterial>>();

    let mut agent_value = serde_json::to_value(vmux_setting::AgentSettings::default()).unwrap();
    agent_value["allow_run_placement_override"] = serde_json::json!(true);
    for (path, value_json) in [
        (
            "agent.allow_run_placement_override",
            serde_json::json!(true).to_string(),
        ),
        ("agent", agent_value.to_string()),
    ] {
        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: Some("test-agent".to_string()),
                    anchor: None,
                },
                command: ServiceAgentCommand::UpdateSettings {
                    path: path.to_string(),
                    value_json,
                },
            });
        app.update();
        assert!(
            !app.world()
                .resource::<AppSettings>()
                .agent
                .allow_run_placement_override,
            "agent update unexpectedly enabled override through {path}"
        );
    }

    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::User,
            command: ServiceAgentCommand::UpdateSettings {
                path: "agent.allow_run_placement_override".to_string(),
                value_json: serde_json::json!(true).to_string(),
            },
        });
    app.update();
    assert!(
        app.world()
            .resource::<AppSettings>()
            .agent
            .allow_run_placement_override
    );
}

#[test]
fn terminal_send_writes_raw_text_to_active_terminal() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        vmux_command::CommandPlugin,
        AgentSessionPlugin,
    ))
    .add_message::<vmux_layout::BrowserNavigateRequest>()
    .add_message::<vmux_layout::BrowserGoBackRequest>()
    .add_message::<vmux_layout::BrowserGoForwardRequest>()
    .add_message::<vmux_layout::OpenInNewStackRequest>()
    .add_message::<vmux_layout::ExtensionInstallRequest>()
    .add_message::<vmux_layout::OpenBesideRequest>()
    .add_message::<vmux_layout::apply::LayoutApplyRequest>()
    .add_message::<vmux_layout::apply::LayoutApplyResponse>()
    .add_message::<vmux_layout::apply::LayoutSnapshotRequest>()
    .add_message::<vmux_layout::apply::LayoutSnapshotResponse>()
    .add_message::<vmux_terminal::TerminalSendRequest>()
    .add_message::<vmux_terminal::RunShellRequest>()
    .add_message::<vmux_setting::SettingsWriteRequest>()
    .add_message::<vmux_space::SpaceCommandRequest>()
    .add_message::<vmux_history::query::HistoryOpenIntent>()
    .add_systems(Update, vmux_terminal::handle_terminal_send_requests)
    .insert_resource(FocusedStack::default())
    .insert_resource(test_settings())
    .init_resource::<Assets<Mesh>>()
    .init_resource::<Assets<WebviewExtendStandardMaterial>>();

    let pane = app.world_mut().spawn(Pane).id();
    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .insert(ChildOf(pane))
        .id();
    let terminal = app
        .world_mut()
        .spawn((Terminal, ProcessId::new()))
        .insert(ChildOf(stack))
        .id();

    app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
    app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

    app.world_mut()
        .resource_mut::<Messages<AgentCommandRequest>>()
        .write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::User,
            command: ServiceAgentCommand::TerminalSend {
                text: "ls".to_string(),
                terminal: None,
            },
        });

    app.update();
    app.update();

    let pending = app
        .world()
        .get::<vmux_terminal::PendingTerminalInput>(terminal)
        .expect("PendingTerminalInput inserted");
    assert_eq!(pending.data, b"ls".to_vec());
}

#[test]
fn missing_vibe_cli_shows_setup_page_at_vibe_url() {
    let mut app = App::new();
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(VibeStrategy));
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(strategies)
        .insert_resource(AgentExecutableOverride(std::collections::HashMap::from([
            (AgentKind::Vibe, false),
        ])))
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (handle_agent_page_open, handle_spawn_agent_requests).chain(),
        );

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    let child = app.world_mut().spawn(ChildOf(stack)).id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/vibe/".to_string(),
        request_id: None,
    });

    app.update();
    app.update();

    assert!(app.world().get_entity(child).is_err());
    let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
    assert_eq!(stack_meta.url, "vmux://agent/vibe/setup");
    assert_eq!(stack_meta.title, "Set up Vibe CLI");
    let mut browsers = app
        .world_mut()
        .query_filtered::<(&PageMetadata, &ChildOf), With<vmux_layout::Browser>>();
    let metas: Vec<PageMetadata> = browsers
        .iter(app.world())
        .filter(|(_, child_of)| child_of.parent() == stack)
        .map(|(meta, _)| meta.clone())
        .collect();
    assert_eq!(metas.len(), 1);
    assert_eq!(metas[0].title, "Set up Vibe CLI");
    assert_eq!(metas[0].url, "vmux://agent/vibe/setup");
}

#[test]
fn missing_claude_or_codex_cli_shows_setup_page() {
    for (kind, segment) in [(AgentKind::Claude, "claude"), (AgentKind::Codex, "codex")] {
        // Isolate the legacy CLI path: ACP now shadows claude/codex single-segment URLs.
        let mut settings = test_settings();
        settings.agent.acp.clear();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(AgentStrategies::default())
            .insert_resource(AgentExecutableOverride(std::collections::HashMap::from([
                (kind, false),
            ])))
            .insert_resource(settings)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(
                Update,
                (handle_agent_page_open, handle_spawn_agent_requests).chain(),
            );

        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: format!("vmux://agent/{segment}/"),
            request_id: None,
        });

        app.update();
        app.update();

        let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(stack_meta.url, format!("vmux://agent/{segment}/setup"));
        assert_eq!(
            stack_meta.title,
            format!("Set up {} CLI", kind.display_name())
        );
    }
}

#[test]
fn registry_acp_opens_without_settings_entry() {
    use crate::acp_registry::{Distribution, RegistryAgent};

    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(crate::client::acp::AcpCatalog {
            agents: vec![RegistryAgent {
                id: "custom-acp".to_string(),
                name: "Custom ACP".to_string(),
                version: None,
                description: None,
                icon: Some("https://cdn.example/custom.svg".to_string()),
                repository: None,
                distribution: Distribution::default(),
            }],
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/custom".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let session = app
        .world()
        .get::<crate::client::acp::AcpSession>(stack)
        .unwrap();
    assert_eq!(session.agent_id, "custom");
    let meta = app.world().get::<PageMetadata>(stack).unwrap();
    assert_eq!(meta.url, "vmux://agent/custom");
    assert_eq!(meta.title, "Custom ACP");
    assert_eq!(meta.icon.favicon_url(), "https://cdn.example/custom.svg");
}

#[test]
fn explicit_setup_url_attaches_setup_page() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(AgentStrategies::default())
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/codex/setup".to_string(),
        request_id: None,
    });

    app.update();
    app.update();

    let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
    assert_eq!(stack_meta.url, "vmux://agent/codex/setup");
    assert_eq!(stack_meta.title, "Set up Codex CLI");
}

#[test]
fn first_local_agent_open_creates_and_reuses_one_tab_worktree() {
    let repo = init_worktree_test_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_layout::worktree::ManagedWorktreeRoot(
            managed_root.path().to_path_buf(),
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (prepare_agent_tab_worktrees, handle_agent_page_open).chain(),
        );
    let project_dir = repo.path().canonicalize().unwrap();
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Feature".into(),
            startup_dir: Some(project_dir.to_string_lossy().into_owned()),
        })
        .id();
    let first_stack = app.world_mut().spawn(ChildOf(tab)).id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack: first_stack,
        url: "vmux://agent/claude/cli".to_string(),
        request_id: None,
    });

    app.update();

    let first_dir = PathBuf::from(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir
            .as_deref()
            .unwrap(),
    );
    assert!(first_dir.starts_with(managed_root.path().canonicalize().unwrap()));
    let canonical_first_dir = first_dir.canonicalize().unwrap();
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(tab)
            .is_some()
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(tab)
            .unwrap()
            .project_dir,
        project_dir.to_string_lossy()
    );
    assert_eq!(
        vmux_git::worktree::worktree_list(repo.path())
            .unwrap()
            .len(),
        2
    );
    let first_spawns: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(first_spawns.len(), 1);
    assert_eq!(first_spawns[0].cwd, canonical_first_dir);

    let second_stack = app.world_mut().spawn(ChildOf(tab)).id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack: second_stack,
        url: "vmux://agent/codex/cli".to_string(),
        request_id: None,
    });
    app.update();

    assert_eq!(
        vmux_git::worktree::worktree_list(repo.path())
            .unwrap()
            .len(),
        2
    );
    let second_dir = Path::new(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir
            .as_deref()
            .unwrap(),
    )
    .canonicalize()
    .unwrap();
    assert_eq!(second_dir, canonical_first_dir);
}

#[test]
fn explicit_work_here_decision_skips_managed_worktree() {
    let repo = init_worktree_test_repo();
    let project_dir = repo.path().canonicalize().unwrap();
    let managed_root = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_layout::worktree::ManagedWorktreeRoot(
            managed_root.path().to_path_buf(),
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (prepare_agent_tab_worktrees, handle_agent_page_open).chain(),
        );
    let tab = app
        .world_mut()
        .spawn((
            vmux_layout::tab::Tab {
                name: "Dashboard".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            },
            vmux_layout::tab::TabWorkspace {
                project_dir: project_dir.to_string_lossy().into_owned(),
            },
            vmux_layout::tab::TabDirDecided,
        ))
        .id();
    let stack = app.world_mut().spawn(ChildOf(tab)).id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude/cli".to_string(),
        request_id: None,
    });

    app.update();

    assert_eq!(
        vmux_git::worktree::worktree_list(repo.path())
            .unwrap()
            .len(),
        1
    );
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(tab)
            .is_none()
    );
    let spawns: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].cwd, project_dir);
}

#[test]
fn local_agent_open_preserves_existing_linked_worktree() {
    let repo = init_worktree_test_repo();
    let linked = repo.path().join(".worktrees/existing");
    vmux_git::worktree::worktree_add(repo.path(), &linked, "existing", "main").unwrap();
    let linked = linked.canonicalize().unwrap();
    let managed_root = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_layout::worktree::ManagedWorktreeRoot(
            managed_root.path().to_path_buf(),
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (prepare_agent_tab_worktrees, handle_agent_page_open).chain(),
        );
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Existing".into(),
            startup_dir: Some(linked.to_string_lossy().into_owned()),
        })
        .id();
    let stack = app.world_mut().spawn(ChildOf(tab)).id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude/cli".to_string(),
        request_id: None,
    });

    app.update();

    assert_eq!(
        vmux_git::worktree::worktree_list(repo.path())
            .unwrap()
            .len(),
        2
    );
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(tab)
            .is_none()
    );
    let spawns: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].cwd, linked);
}

#[test]
fn browser_only_tab_creates_no_worktree() {
    let repo = init_worktree_test_repo();
    let managed_root = tempfile::tempdir().unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(vmux_layout::worktree::ManagedWorktreeRoot(
            managed_root.path().to_path_buf(),
        ))
        .add_systems(Update, prepare_agent_tab_worktrees);
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Browser".into(),
            startup_dir: Some(repo.path().to_string_lossy().into_owned()),
        })
        .id();
    let stack = app.world_mut().spawn(ChildOf(tab)).id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "https://example.com".to_string(),
        request_id: None,
    });

    app.update();

    assert_eq!(
        vmux_git::worktree::worktree_list(repo.path())
            .unwrap()
            .len(),
        1
    );
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorktree>(tab)
            .is_none()
    );
}

#[test]
fn agent_tab_without_workspace_starts_in_home_without_binding_tab() {
    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Tab 1".into(),
            startup_dir: None,
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::PendingPrompt("Show me something fun in terminal".into()),
            ChildOf(tab),
        ))
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/codex/cli".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    let spawns: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].cwd, process_cwd());
    assert_eq!(
        spawns[0].initial_prompt.as_deref(),
        Some("Show me something fun in terminal")
    );
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(tab)
            .is_none()
    );
}

#[test]
fn acp_tab_without_workspace_attaches_once_without_setup_page() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "Tab 1".into(),
            startup_dir: None,
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::PendingPrompt("Show me something fun in terminal".into()),
            ChildOf(tab),
        ))
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude".to_string(),
        request_id: None,
    });

    app.update();

    let session = app
        .world()
        .get::<crate::client::acp::AcpSession>(stack)
        .unwrap();
    assert_eq!(session.cwd, process_cwd());
    assert_eq!(
        app.world()
            .get::<crate::components::PromptQueue>(stack)
            .unwrap()
            .items
            .front()
            .map(|item| item.text.as_str()),
        Some("Show me something fun in terminal")
    );
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(tab)
            .is_none()
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<&ChildOf, With<crate::chat_page::AgentChatView>>()
            .iter(app.world())
            .filter(|child_of| child_of.parent() == stack)
            .count(),
        1
    );
}

#[test]
fn inline_start_transition_reuses_the_existing_webview() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::PendingPrompt("keep this prompt".to_string()),
            vmux_core::PendingPromptAttachments(vec![AgentAttachment {
                path: "/tmp/reference.png".to_string(),
                name: "reference.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 42,
            }]),
        ))
        .id();
    let webview = app
        .world_mut()
        .spawn((
            vmux_layout::Browser,
            bevy_cef::prelude::WebviewSource::new("vmux://start/"),
            PageMetadata {
                url: "vmux://start/".to_string(),
                title: "Start".to_string(),
                ..default()
            },
            vmux_layout::start::StartInlineTransitionView,
            ChildOf(stack),
        ))
        .id();
    app.world_mut()
        .entity_mut(stack)
        .insert(vmux_layout::start::StartInlineTransition { webview });
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude".to_string(),
        request_id: None,
    });

    app.update();

    assert!(app.world().get_entity(webview).is_ok());
    assert!(
        app.world()
            .get::<crate::chat_page::AgentChatView>(webview)
            .is_some()
    );
    assert!(
        matches!(
            app.world()
                .get::<bevy_cef::prelude::WebviewSource>(webview),
            Some(bevy_cef::prelude::WebviewSource::Url(url)) if url == "vmux://start/"
        ),
        "the existing document remains loaded"
    );
    assert_eq!(
        app.world().get::<PageMetadata>(webview).unwrap().url,
        "vmux://agent/claude"
    );
    let queue = app
        .world()
        .get::<crate::components::PromptQueue>(stack)
        .unwrap();
    assert_eq!(
        queue.items.front().map(|item| item.text.as_str()),
        Some("keep this prompt")
    );
    assert_eq!(
        queue
            .items
            .front()
            .and_then(|item| item.attachments.first())
            .map(|attachment| attachment.path.as_str()),
        Some("/tmp/reference.png")
    );
    assert!(app.world().get::<vmux_core::PendingPrompt>(stack).is_none());
    assert!(
        app.world()
            .get::<vmux_core::PendingPromptAttachments>(stack)
            .is_none()
    );
    assert!(
        app.world()
            .get::<vmux_layout::start::StartInlineTransition>(stack)
            .is_none()
    );
}

#[test]
fn acp_open_discards_missing_restored_tab_workspace() {
    let missing = std::env::temp_dir().join(format!(
        "vmux-missing-restored-workspace-{}",
        uuid::Uuid::new_v4()
    ));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(
            Update,
            (prepare_agent_tab_worktrees, handle_agent_page_open).chain(),
        );
    let stale = missing.to_string_lossy().into_owned();
    let tab = app
        .world_mut()
        .spawn((
            vmux_layout::tab::Tab {
                name: "Tab 1".into(),
                startup_dir: Some(stale.clone()),
            },
            vmux_layout::tab::TabWorkspace { project_dir: stale },
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/codex".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    assert!(app.world().get::<PageOpenHandled>(task).is_some());
    assert!(app.world().get::<PageOpenError>(task).is_none());
    assert_eq!(
        app.world()
            .get::<crate::client::acp::AcpSession>(stack)
            .unwrap()
            .cwd,
        process_cwd()
    );
    assert_eq!(
        app.world()
            .get::<vmux_layout::tab::Tab>(tab)
            .unwrap()
            .startup_dir,
        None
    );
    assert!(
        app.world()
            .get::<vmux_layout::tab::TabWorkspace>(tab)
            .is_none()
    );
}

#[test]
fn fresh_claude_page_uses_space_startup_dir() {
    let dir = std::env::temp_dir().join(format!("vmux-startup-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut settings = test_settings();
    // Isolate the legacy CLI path: ACP now shadows the `claude` single-segment URL.
    settings.agent.acp.clear();
    settings.spaces.insert(
        "space-1".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(dir.to_string_lossy().into()),
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace {
            record: vmux_space::model::SpaceRecord {
                id: "space-1".into(),
                name: "Space 1".into(),
                profile: "Personal".into(),
            },
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude/".to_string(),
        request_id: None,
    });

    app.update();

    let spawns: Vec<SpawnAgentInStackRequest> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(spawns.len(), 1, "one agent spawn emitted");
    assert_eq!(spawns[0].kind, AgentKind::Claude);
    assert_eq!(
        spawns[0].cwd, dir,
        "claude page cwd resolves to space startup_dir"
    );
}

#[test]
fn restored_agent_tab_uses_ancestor_space_startup_dir() {
    let active_dir = tempfile::tempdir().unwrap();
    let restored_dir = tempfile::tempdir().unwrap();
    let mut settings = test_settings();
    settings.agent.acp.clear();
    settings.spaces.insert(
        "active".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(active_dir.path().to_string_lossy().into()),
        },
    );
    settings.spaces.insert(
        "restored".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(restored_dir.path().to_string_lossy().into()),
        },
    );
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace {
            record: vmux_space::model::SpaceRecord {
                id: "active".into(),
                name: "Active".into(),
                profile: "Personal".into(),
            },
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let space = app
        .world_mut()
        .spawn((
            vmux_layout::space::Space,
            vmux_layout::space::SpaceId("restored".into()),
        ))
        .id();
    let tab = app
        .world_mut()
        .spawn((
            vmux_layout::tab::Tab {
                name: "Legacy".into(),
                startup_dir: None,
            },
            ChildOf(space),
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude/cli".to_string(),
        request_id: None,
    });

    app.update();

    let spawns: Vec<SpawnAgentInStackRequest> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].cwd, restored_dir.path());
}

#[test]
fn fresh_cli_page_forwards_pending_prompt() {
    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::PendingPrompt("fix the tests".to_string()),
        ))
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/codex/cli".to_string(),
        request_id: None,
    });

    app.update();

    let spawns: Vec<SpawnAgentInStackRequest> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].kind, AgentKind::Codex);
    assert_eq!(spawns[0].initial_prompt.as_deref(), Some("fix the tests"));
}

#[test]
fn cli_initial_prompt_waits_for_terminal_readiness() {
    let mut strategies = AgentStrategies::default();
    strategies.register_cli(Box::new(crate::client::cli::codex::CodexStrategy));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(strategies)
        .insert_resource(AgentExecutableOverride(std::collections::HashMap::from([
            (AgentKind::Codex, true),
        ])))
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_spawn_agent_requests);
    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    app.world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .write(SpawnAgentInStackRequest {
            kind: AgentKind::Codex,
            cwd: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
            session_id: None,
            stack,
            initial_prompt: Some("@asdfas".to_string()),
            initial_attachments: Vec::new(),
        });

    app.update();
    app.update();

    let mut terminals = app.world_mut().query_filtered::<(
        &vmux_terminal::PromptCapture,
        Has<vmux_terminal::BufferedAgentPrompt>,
    ), With<Terminal>>();
    let (capture, buffered) = terminals.single(app.world()).unwrap();
    assert_eq!(capture.draft, "@asdfas");
    assert!(!capture.skipped);
    assert!(!buffered);
}

#[test]
fn cli_initial_prompt_keeps_media_paths() {
    let attachments = vec![AgentAttachment {
        path: "/tmp/reference image.png".to_string(),
        name: "reference image.png".to_string(),
        mime_type: "image/png".to_string(),
        size: 42,
    }];

    assert_eq!(
        cli_initial_prompt(AgentKind::Codex, Some("describe this"), &attachments).as_deref(),
        Some("describe this /tmp/reference image.png")
    );
    assert_eq!(
        cli_initial_prompt(AgentKind::Vibe, Some("describe this"), &attachments).as_deref(),
        Some("describe this @'/tmp/reference image.png'")
    );
}

#[test]
fn fresh_acp_page_queues_pending_prompt() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::PendingPrompt("ship it".to_string()),
        ))
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude".to_string(),
        request_id: None,
    });

    app.update();

    let queue = app
        .world()
        .get::<crate::components::PromptQueue>(stack)
        .unwrap();
    assert_eq!(
        queue.items.front().map(|item| item.text.as_str()),
        Some("ship it")
    );
    assert_eq!(
        app.world()
            .get::<crate::components::AgentConversationTitle>(stack),
        Some(&crate::components::AgentConversationTitle("ship it".into()))
    );
    assert!(app.world().get::<vmux_core::PendingPrompt>(stack).is_none());
}

#[test]
fn fresh_claude_page_prefers_ancestor_tab_startup_dir() {
    let space_dir = std::env::temp_dir().join(format!("vmux-space-dir-{}", std::process::id()));
    let tab_dir = std::env::temp_dir().join(format!("vmux-tab-dir-{}", std::process::id()));
    std::fs::create_dir_all(&space_dir).unwrap();
    std::fs::create_dir_all(&tab_dir).unwrap();

    let mut settings = test_settings();
    settings.agent.acp.clear();
    settings.spaces.insert(
        "space-1".into(),
        vmux_setting::SpaceOverrides {
            startup_url: None,
            startup_dir: Some(space_dir.to_string_lossy().into()),
        },
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .insert_resource(vmux_space::spaces::ActiveSpace {
            record: vmux_space::model::SpaceRecord {
                id: "space-1".into(),
                name: "Space 1".into(),
                profile: "Personal".into(),
            },
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);

    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "t".into(),
            startup_dir: Some(tab_dir.to_string_lossy().into()),
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/claude/".to_string(),
        request_id: None,
    });

    app.update();

    let spawns: Vec<SpawnAgentInStackRequest> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    let canonical_tab_dir = tab_dir.canonicalize().unwrap();
    let _ = std::fs::remove_dir_all(&space_dir);
    let _ = std::fs::remove_dir_all(&tab_dir);
    assert_eq!(spawns.len(), 1);
    assert_eq!(
        spawns[0].cwd, canonical_tab_dir,
        "claude page cwd resolves to ancestor tab startup_dir"
    );
}

#[test]
fn fresh_claude_page_rejects_invalid_stored_tab_startup_dir() {
    let mut settings = test_settings();
    settings.agent.acp.clear();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(settings)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);
    let tab = app
        .world_mut()
        .spawn(vmux_layout::tab::Tab {
            name: "t".into(),
            startup_dir: Some("/no/such/vmux-tab-workspace".into()),
        })
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(tab)))
        .id();
    let task = app
        .world_mut()
        .spawn(PageOpenTask {
            id: vmux_core::PageOpenId::new(),
            stack,
            url: "vmux://agent/claude/".to_string(),
            request_id: None,
        })
        .id();

    app.update();

    let spawns: Vec<SpawnAgentInStackRequest> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert!(spawns.is_empty());
    assert!(app.world().get::<PageOpenError>(task).is_some());
}

#[test]
fn bare_agent_open_skips_when_stack_already_has_same_agent() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<SpawnAgentInStackRequest>()
        .insert_resource(test_settings())
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<WebviewExtendStandardMaterial>>()
        .add_systems(Update, handle_agent_page_open);

    let stack = app
        .world_mut()
        .spawn(vmux_layout::stack::stack_bundle())
        .id();
    // Stack already hosts a live vibe agent.
    app.world_mut().spawn((
        ChildOf(stack),
        vmux_core::agent::AgentSession {
            kind: AgentKind::Vibe,
        },
    ));
    app.world_mut().spawn(PageOpenTask {
        id: vmux_core::PageOpenId::new(),
        stack,
        url: "vmux://agent/vibe/".to_string(),
        request_id: None,
    });

    app.update();

    let spawns: Vec<SpawnAgentInStackRequest> = app
        .world_mut()
        .resource_mut::<Messages<SpawnAgentInStackRequest>>()
        .drain()
        .collect();
    assert_eq!(
        spawns.len(),
        0,
        "bare agent open must not spawn a second agent when the stack already has one"
    );
}

#[test]
fn run_terminal_cwd_prefers_tab_dir() {
    let tab_dir = std::env::temp_dir().join(format!("vmux-tab-cwd-{}", std::process::id()));
    let agent_dir = std::env::temp_dir().join(format!("vmux-agent-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&tab_dir).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    let canonical_tab_dir = tab_dir.canonicalize().unwrap();
    assert_eq!(
        run_terminal_cwd(
            Some(tab_dir.to_string_lossy().as_ref()),
            Some(agent_dir.to_string_lossy().as_ref()),
        )
        .unwrap(),
        canonical_tab_dir
    );
    let _ = std::fs::remove_dir_all(&agent_dir);
    let _ = std::fs::remove_dir_all(&tab_dir);
}

#[test]
fn run_terminal_launch_must_match_rebound_cwd_for_reuse() {
    let current = std::env::temp_dir().join(format!("vmux-current-cwd-{}", std::process::id()));
    let stale = std::env::temp_dir().join(format!("vmux-stale-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&current).unwrap();
    std::fs::create_dir_all(&stale).unwrap();
    assert!(run_terminal_launch_matches_cwd(
        current.to_string_lossy().as_ref(),
        &current,
    ));
    assert!(!run_terminal_launch_matches_cwd(
        stale.to_string_lossy().as_ref(),
        &current,
    ));
    let _ = std::fs::remove_dir_all(&stale);
    let _ = std::fs::remove_dir_all(&current);
}

#[test]
fn run_terminal_cwd_inherits_agent_launch_dir() {
    let dir = std::env::temp_dir().join(format!("vmux-run-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let got = run_terminal_cwd(None, Some(&dir.to_string_lossy())).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, dir);
}

#[test]
fn run_terminal_cwd_requires_tab_or_agent_workspace() {
    assert!(run_terminal_cwd(None, Some("")).is_err());
    assert!(run_terminal_cwd(None, None).is_err());
}

#[test]
fn run_terminal_cwd_rejects_invalid_stored_tab_directory() {
    let agent_dir = std::env::temp_dir();

    assert!(run_terminal_cwd(Some("/no/such/vmux-tab-workspace"), agent_dir.to_str()).is_err());
}

#[test]
fn run_terminal_cwd_rejects_relative_stored_tab_directory() {
    assert!(run_terminal_cwd(Some("."), None).is_err());
}

#[test]
fn command_with_marker_is_shell_aware() {
    // The completion marker is an invisible OSC escape
    // (ESC ] 6973 ; token ; exit BEL), consumed by the terminal parser so it
    // never renders. nushell aborts `;` on failure, so it wraps in try/catch
    // and reads the exit code from the caught error.
    assert_eq!(
        command_with_marker("/opt/homebrew/bin/nu", "ls", "abc"),
        "$env.GIT_PAGER = \"cat\"; $env.PAGER = \"cat\"; $env.LESS = \"FRX\"; try { ls; print -rn $\"\\u{1b}]6973;abc;($env.LAST_EXIT_CODE)\\u{7}\" } catch { |e| print -rn $\"\\u{1b}]6973;abc;($e.exit_code? | default 1)\\u{7}\" }"
    );
    assert_eq!(
        command_with_marker("/usr/local/bin/fish", "ls", "abc"),
        "set -gx GIT_PAGER cat; set -gx PAGER cat; set -gx LESS FRX; ls; set __vmux_status $status; printf '\\033]6973;abc;%s\\007' $__vmux_status"
    );
    assert_eq!(
        command_with_marker("/bin/zsh", "ls", "abc"),
        "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
    );
    // Unknown shells fall back to posix syntax.
    assert_eq!(
        command_with_marker("/usr/bin/xonsh", "ls", "abc"),
        "export GIT_PAGER=cat PAGER=cat LESS=FRX; ls; __vmux_status=\"$?\"; printf '\\033]6973;abc;%s\\007' \"$__vmux_status\""
    );
}

#[test]
fn run_command_line_noop_when_token_absent() {
    assert_eq!(run_command_line("ls -la", None, "/bin/zsh"), "ls -la");
}

#[test]
fn run_command_line_embeds_marker_when_token_present() {
    let out = run_command_line("ls -la", Some("tok9"), "/bin/zsh");
    assert!(out.contains("ls -la"), "got: {out}");
    assert!(out.contains("]6973;tok9;"), "got: {out}");
    assert!(
        !out.contains("__VMUX_DONE_"),
        "marker must be invisible: {out}"
    );
}

#[test]
fn new_agent_run_terminal_uses_configured_shell_for_launch_and_input() {
    let mut settings = test_settings();
    settings.terminal = Some(vmux_setting::TerminalSettings {
        default_theme: "default".to_string(),
        themes: vec![vmux_setting::TerminalTheme {
            name: "default".to_string(),
            color_scheme: "catppuccin-mocha".to_string(),
            font_family: "JetBrainsMono Nerd Font".to_string(),
            font_size: 14.0,
            line_height: 1.2,
            padding: 4.0,
            cursor_style: "block".to_string(),
            cursor_blink: true,
            shell: "/opt/homebrew/bin/nu".to_string(),
        }],
        ..Default::default()
    });

    let (shell, input) = new_run_terminal_command(&settings, "cd /tmp", Some("tok9"));

    assert_eq!(shell, "/opt/homebrew/bin/nu");
    let input = String::from_utf8(input).unwrap();
    assert!(input.contains("try { cd /tmp;"), "got: {input}");
    assert!(input.contains("]6973;tok9;"), "got: {input}");
    assert!(input.ends_with('\r'));
    assert!(!input.contains("export GIT_PAGER"), "got: {input}");
}

#[test]
fn new_agent_run_terminal_rejects_missing_configured_shell() {
    let shell = "/definitely/missing/vmux-terminal-shell";

    assert_eq!(
        validate_agent_terminal_shell(shell),
        Err(format!(
            "terminal shell not found or not executable: {shell}"
        ))
    );
}

#[test]
fn existing_agent_run_terminal_uses_launch_shell_for_input() {
    let launch = TerminalLaunch {
        command: "/usr/local/bin/fish".to_string(),
        args: vec![],
        cwd: String::new(),
        env: vec![],
        kind: vmux_terminal::launch::TerminalKind::Plain,
    };

    let input = terminal_run_command_input("pwd", Some("tok2"), &launch);
    let input = String::from_utf8(input).unwrap();

    assert!(input.contains("set __vmux_status $status"), "got: {input}");
    assert!(input.contains("]6973;tok2;"), "got: {input}");
    assert!(input.ends_with('\r'));
}

#[test]
fn explicit_run_terminal_errors_distinguish_missing_page_and_launch() {
    use bevy::ecs::system::RunSystemOnce;

    let mut app = App::new();
    let terminal_pid = ProcessId::new();
    let missing_pid = ProcessId::new();
    app.world_mut().spawn((Terminal, terminal_pid));

    let (missing_page, missing_launch) = app
        .world_mut()
        .run_system_once(
            move |terminals: Query<(Entity, &ProcessId), With<Terminal>>,
                  launches: Query<&TerminalLaunch>| {
                (
                    explicit_run_terminal_launch(missing_pid, &terminals, &launches).unwrap_err(),
                    explicit_run_terminal_launch(terminal_pid, &terminals, &launches).unwrap_err(),
                )
            },
        )
        .unwrap();

    assert_eq!(
        missing_page,
        format!("run.terminal page not found: {missing_pid}")
    );
    assert_eq!(
        missing_launch,
        format!("run terminal launch not found: {terminal_pid}")
    );
}

#[test]
fn existing_agent_run_terminal_routes_input_through_terminal_queue() {
    #[derive(Resource)]
    struct Input {
        process_id: ProcessId,
        launch: TerminalLaunch,
    }

    #[derive(Resource, Default)]
    struct Captured(Vec<vmux_terminal::TerminalReinputRequest>);

    fn emit(input: Res<Input>, mut writer: MessageWriter<vmux_terminal::TerminalReinputRequest>) {
        queue_terminal_run_command_input(
            &mut writer,
            input.process_id,
            "pwd",
            Some("tok4"),
            &input.launch,
        );
    }

    fn capture(
        mut reader: MessageReader<vmux_terminal::TerminalReinputRequest>,
        mut captured: ResMut<Captured>,
    ) {
        captured.0.extend(reader.read().cloned());
    }

    let process_id = ProcessId::new();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_terminal::TerminalReinputRequest>()
        .insert_resource(Input {
            process_id,
            launch: TerminalLaunch {
                command: "/usr/local/bin/fish".to_string(),
                args: vec![],
                cwd: String::new(),
                env: vec![],
                kind: vmux_terminal::launch::TerminalKind::Plain,
            },
        })
        .init_resource::<Captured>()
        .add_systems(Update, (emit, capture).chain());

    app.update();

    let captured = &app.world().resource::<Captured>().0;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].process_id, process_id);
    let input = String::from_utf8(captured[0].data.clone()).unwrap();

    assert!(input.contains("set __vmux_status $status"), "got: {input}");
    assert!(input.contains("]6973;tok4;"), "got: {input}");
    assert!(input.ends_with('\r'));
}

#[test]
fn agent_origin_clears_requested_focus() {
    let origin = CommandOrigin::Agent {
        sid: Some("s1".into()),
        anchor: Some(ProcessId::new()),
    };

    assert!(!requested_focus_for_origin(&origin, true));
    assert!(!requested_focus_for_origin(&origin, false));
}

#[test]
fn user_origin_keeps_requested_focus() {
    assert!(requested_focus_for_origin(&CommandOrigin::User, true));
    assert!(!requested_focus_for_origin(&CommandOrigin::User, false));
}

#[test]
fn agent_layout_snapshot_keeps_current_focus() {
    use vmux_service::protocol::layout::{Focus, LayoutNode, LayoutSnapshot, Tab};
    let mut snapshot = LayoutSnapshot {
        tabs: vec![
            Tab {
                id: Some("tab:9".into()),
                name: "Agent".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:8".into()),
                    is_zoomed: false,
                    stacks: vec![],
                },
            },
            Tab {
                id: Some("tab:1".into()),
                name: "User".into(),
                is_active: false,
                root: LayoutNode::Pane {
                    id: Some("pane:2".into()),
                    is_zoomed: false,
                    stacks: vec![],
                },
            },
        ],
        focused: Focus {
            tab: Some("tab:9".into()),
            pane: Some("pane:8".into()),
            stack: None,
        },
    };
    let focus = FocusedStack {
        tab: Some(Entity::from_bits(1)),
        pane: Some(Entity::from_bits(2)),
        stack: Some(Entity::from_bits(3)),
    };

    preserve_current_focus_in_layout_snapshot(&mut snapshot, &focus);

    assert_eq!(snapshot.focused.tab.as_deref(), Some("tab:1"));
    assert_eq!(snapshot.focused.pane.as_deref(), Some("pane:2"));
    assert_eq!(snapshot.focused.stack.as_deref(), Some("stack:3"));
    assert!(!snapshot.tabs[0].is_active);
    assert!(snapshot.tabs[1].is_active);
}

#[test]
fn agent_app_command_filter_blocks_focus_changers() {
    assert!(!agent_may_dispatch_app_command(&AppCommand::Browser(
        vmux_command::BrowserCommand::Open(vmux_command::OpenCommand::InNewStack { url: None }),
    )));
    assert!(!agent_may_dispatch_app_command(&AppCommand::Browser(
        vmux_command::BrowserCommand::Bar(vmux_command::BrowserBarCommand::OpenCommandBar),
    )));
    assert!(!agent_may_dispatch_app_command(&AppCommand::Terminal(
        vmux_command::TerminalCommand::Next,
    )));
    assert!(agent_may_dispatch_app_command(&AppCommand::Terminal(
        vmux_command::TerminalCommand::Clear,
    )));
}

#[test]
fn agent_run_spawns_terminal_before_next_agent_command_frame() {
    let source = include_str!("plugin.rs");
    let non_test_source = source
        .split("#[cfg(test)]")
        .next()
        .expect("non-test source");
    let start = non_test_source
        .find("handle_agent_self_commands")
        .expect("handle_agent_self_commands registered");
    assert!(
        non_test_source[start..]
            .contains(".before(vmux_terminal::plugin::respond_terminal_stack_spawn)"),
        "run terminal spawn requests must materialize before the next agent command frame"
    );
}

#[test]
fn agent_restart_runs_before_terminal_service_messages() {
    let source = include_str!("plugin.rs");
    let non_test_source = source
        .split("#[cfg(test)]")
        .next()
        .expect("non-test source");

    assert!(
        non_test_source.contains("handle_restart_agent_pty.before(ServiceMessageSet)"),
        "restart state commands must apply before terminal input flush"
    );
}

#[derive(Resource)]
struct RunTerminalCandidateInput {
    agent_pane: Entity,
    desired_cwd: PathBuf,
}

#[derive(Resource, Default)]
struct RunTerminalCandidateOutput(Vec<RunTerminalCandidate>);

fn collect_run_terminal_candidates(
    input: Res<RunTerminalCandidateInput>,
    terminals: Query<
        (Entity, &ProcessId, &TerminalLaunch, Has<AgentRunTerminal>),
        (
            With<Terminal>,
            Without<AgentSession>,
            Without<ProcessExited>,
        ),
    >,
    child_of_q: Query<&ChildOf>,
    tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
    seq_q: Query<&vmux_layout::pane::SpawnSeq>,
    mut out: ResMut<RunTerminalCandidateOutput>,
) {
    out.0 = run_terminal_candidates(
        input.agent_pane,
        &terminals,
        &child_of_q,
        &tab_q,
        &seq_q,
        &input.desired_cwd,
    );
}

#[test]
fn run_terminal_candidates_fail_closed_when_agent_tab_missing() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<RunTerminalCandidateOutput>()
        .add_systems(Update, collect_run_terminal_candidates);

    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let terminal_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(7), ChildOf(tab)))
        .id();
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(terminal_pane)))
        .id();
    let desired_cwd = std::env::temp_dir();
    app.world_mut().spawn((
        Terminal,
        ProcessId::new(),
        AgentRunTerminal,
        TerminalLaunch {
            command: "/bin/zsh".to_string(),
            args: vec![],
            cwd: desired_cwd.to_string_lossy().into_owned(),
            env: vec![],
            kind: vmux_terminal::launch::TerminalKind::Plain,
        },
        ChildOf(stack),
    ));
    let agent_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(9)))
        .id();

    app.insert_resource(RunTerminalCandidateInput {
        agent_pane,
        desired_cwd,
    });
    app.update();

    assert!(
        app.world()
            .resource::<RunTerminalCandidateOutput>()
            .0
            .is_empty(),
        "unresolved agent tab must not match terminals from other tabs"
    );
}

#[test]
fn run_terminal_candidates_require_agent_run_marker() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<RunTerminalCandidateOutput>()
        .add_systems(Update, collect_run_terminal_candidates);
    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let agent_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
        .id();
    let desired_cwd = std::env::temp_dir();
    let agent_pid = ProcessId::new();
    let user_pid = ProcessId::new();
    let mut agent_terminal = None;
    for (sequence, pid, agent_run) in [(2, agent_pid, true), (3, user_pid, false)] {
        let pane = app
            .world_mut()
            .spawn((Pane, vmux_layout::pane::SpawnSeq(sequence), ChildOf(tab)))
            .id();
        let stack = app
            .world_mut()
            .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
            .id();
        let terminal = app
            .world_mut()
            .spawn((
                Terminal,
                pid,
                TerminalLaunch {
                    command: "/bin/zsh".to_string(),
                    args: vec![],
                    cwd: desired_cwd.to_string_lossy().into_owned(),
                    env: vec![],
                    kind: vmux_terminal::launch::TerminalKind::Plain,
                },
                ChildOf(stack),
            ))
            .id();
        if agent_run {
            app.world_mut()
                .entity_mut(terminal)
                .insert(AgentRunTerminal);
            agent_terminal = Some(terminal);
        }
    }

    app.insert_resource(RunTerminalCandidateInput {
        agent_pane,
        desired_cwd,
    });
    app.update();

    let candidates = &app.world().resource::<RunTerminalCandidateOutput>().0;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].pid, agent_pid);
    assert_eq!(candidates[0].terminal, agent_terminal.unwrap());
}

#[test]
fn run_terminal_candidates_exclude_stale_launch_cwd() {
    let current =
        std::env::temp_dir().join(format!("vmux-current-candidate-{}", std::process::id()));
    let stale = std::env::temp_dir().join(format!("vmux-stale-candidate-{}", std::process::id()));
    std::fs::create_dir_all(&current).unwrap();
    std::fs::create_dir_all(&stale).unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<RunTerminalCandidateOutput>()
        .add_systems(Update, collect_run_terminal_candidates);
    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let agent_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
        .id();
    let current_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(2), ChildOf(tab)))
        .id();
    let current_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(current_pane)))
        .id();
    let current_pid = ProcessId::new();
    app.world_mut().spawn((
        Terminal,
        current_pid,
        AgentRunTerminal,
        TerminalLaunch {
            command: "/bin/zsh".into(),
            args: vec![],
            cwd: current.to_string_lossy().into_owned(),
            env: vec![],
            kind: vmux_core::terminal::TerminalKind::Plain,
        },
        ChildOf(current_stack),
    ));
    let stale_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(3), ChildOf(tab)))
        .id();
    let stale_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(stale_pane)))
        .id();
    app.world_mut().spawn((
        Terminal,
        ProcessId::new(),
        AgentRunTerminal,
        TerminalLaunch {
            command: "/bin/zsh".into(),
            args: vec![],
            cwd: stale.to_string_lossy().into_owned(),
            env: vec![],
            kind: vmux_core::terminal::TerminalKind::Plain,
        },
        ChildOf(stale_stack),
    ));
    app.insert_resource(RunTerminalCandidateInput {
        agent_pane,
        desired_cwd: current.clone(),
    });
    app.update();

    let candidates = &app.world().resource::<RunTerminalCandidateOutput>().0;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].pid, current_pid);
    let _ = std::fs::remove_dir_all(&stale);
    let _ = std::fs::remove_dir_all(&current);
}

#[derive(Resource)]
struct RunTerminalBucketPaneInput {
    agent_pane: Entity,
}

#[derive(Resource, Default)]
struct RunTerminalBucketPaneOutput(Vec<Entity>);

fn collect_run_terminal_bucket_panes(
    input: Res<RunTerminalBucketPaneInput>,
    child_of_q: Query<&ChildOf>,
    tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
    leaf_panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    pane_children: Query<&Children, With<Pane>>,
    stack_q: Query<Entity, With<vmux_layout::stack::Stack>>,
    page_q: Query<&PageMetadata, With<vmux_layout::stack::Stack>>,
    seq_q: Query<&vmux_layout::pane::SpawnSeq>,
    mut out: ResMut<RunTerminalBucketPaneOutput>,
) {
    out.0 = run_terminal_bucket_panes(
        input.agent_pane,
        &child_of_q,
        &tab_q,
        &leaf_panes,
        &pane_children,
        &stack_q,
        &page_q,
        &seq_q,
    )
    .into_iter()
    .map(|candidate| candidate.pane)
    .collect();
}

#[test]
fn run_terminal_bucket_panes_include_pure_terminal_layout_panes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<RunTerminalBucketPaneOutput>()
        .add_systems(Update, collect_run_terminal_bucket_panes);

    let tab = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let agent_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(1), ChildOf(tab)))
        .id();
    let terminal_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(3), ChildOf(tab)))
        .id();
    spawn_stack_in_pane(&mut app, terminal_pane, "vmux://terminal/68001");
    let file_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(9), ChildOf(tab)))
        .id();
    spawn_stack_in_pane(&mut app, file_pane, "file:///repo/src/plugin.rs");

    app.insert_resource(RunTerminalBucketPaneInput { agent_pane });
    app.update();

    assert_eq!(
        app.world().resource::<RunTerminalBucketPaneOutput>().0,
        vec![terminal_pane]
    );
}

#[test]
fn pending_run_terminal_spawn_uses_selected_shell() {
    let anchor = ProcessId::new();
    let terminal = ProcessId::new();
    let pane = Entity::from_bits(20);
    let mut pending_spawns = std::collections::HashMap::new();
    pending_spawns.insert(
        anchor,
        PendingRunTerminalSpawn {
            pid: terminal,
            request_index: 0,
            shell: "/opt/homebrew/bin/nu".to_string(),
        },
    );
    let mut terminal_spawns = vec![TerminalStackSpawnRequest {
        pane,
        cwd: Some(std::env::temp_dir()),
        shell: Some("/opt/homebrew/bin/nu".to_string()),
        agent_run: true,
        pending_input: Some(b"one\r".to_vec()),
        process_id: Some(terminal),
        activate: false,
    }];

    let picked = append_pending_run_terminal_input(
        anchor,
        &pending_spawns,
        &mut terminal_spawns,
        &std::env::temp_dir(),
        "pwd",
        Some("tok2"),
    );

    assert_eq!(picked, Some(terminal));
    let input = String::from_utf8(terminal_spawns[0].pending_input.clone().unwrap()).unwrap();
    assert!(input.starts_with("one\r"), "got: {input}");
    assert!(input.contains("try { pwd;"), "got: {input}");
    assert!(input.contains("]6973;tok2;"), "got: {input}");
    assert_eq!(terminal_spawns.len(), 1);
}

#[test]
fn pending_run_terminal_spawn_rejects_changed_cwd() {
    let old_cwd = std::env::temp_dir().join(format!("vmux-old-cwd-{}", std::process::id()));
    let new_cwd = std::env::temp_dir().join(format!("vmux-new-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&old_cwd).unwrap();
    std::fs::create_dir_all(&new_cwd).unwrap();
    let anchor = ProcessId::new();
    let terminal = ProcessId::new();
    let mut pending_spawns = std::collections::HashMap::new();
    pending_spawns.insert(
        anchor,
        PendingRunTerminalSpawn {
            pid: terminal,
            request_index: 0,
            shell: "/opt/homebrew/bin/nu".to_string(),
        },
    );
    let mut terminal_spawns = vec![TerminalStackSpawnRequest {
        pane: Entity::from_bits(20),
        cwd: Some(old_cwd.clone()),
        shell: Some("/opt/homebrew/bin/nu".to_string()),
        agent_run: true,
        pending_input: Some(b"one\r".to_vec()),
        process_id: Some(terminal),
        activate: false,
    }];

    let picked = append_pending_run_terminal_input(
        anchor,
        &pending_spawns,
        &mut terminal_spawns,
        &new_cwd,
        "pwd",
        Some("tok2"),
    );

    let _ = std::fs::remove_dir_all(&old_cwd);
    let _ = std::fs::remove_dir_all(&new_cwd);
    assert_eq!(picked, None);
    assert_eq!(
        terminal_spawns[0].pending_input.as_deref(),
        Some(&b"one\r"[..])
    );
}

#[derive(Resource)]
struct ReusedRunPaneTouchInput {
    pane: Entity,
}

fn touch_reused_run_pane_spawn_seq_test_system(
    input: Res<ReusedRunPaneTouchInput>,
    mut commands: Commands,
    mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
    seq_q: Query<&vmux_layout::pane::SpawnSeq>,
) {
    touch_reused_run_pane_spawn_seq(input.pane, &mut commands, &mut spawn_counter, &seq_q);
}

#[test]
fn reusable_run_pane_touch_refreshes_spawn_seq() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<vmux_layout::pane::SpawnCounter>()
        .add_systems(Update, touch_reused_run_pane_spawn_seq_test_system);

    let reused = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(2)))
        .id();
    app.world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(10)));
    app.insert_resource(ReusedRunPaneTouchInput { pane: reused });
    app.update();

    assert_eq!(
        app.world()
            .get::<vmux_layout::pane::SpawnSeq>(reused)
            .unwrap()
            .0,
        11
    );
}

#[derive(Resource)]
struct SplitRunPaneInput {
    pane: Entity,
}

#[derive(Resource, Default)]
struct SplitRunPaneOutput(Option<Entity>);

fn split_run_pane_test_system(
    input: Res<SplitRunPaneInput>,
    mut out: ResMut<SplitRunPaneOutput>,
    mut commands: Commands,
    mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
    pane_children: Query<&Children, With<Pane>>,
    tab_filter: Query<Entity, With<vmux_layout::stack::Stack>>,
    split_dir_q: Query<&PaneSplit>,
    seq_q: Query<&vmux_layout::pane::SpawnSeq>,
) {
    let mut split_batch = std::collections::HashSet::new();
    let target = split_pane_off(
        &mut commands,
        input.pane,
        &vmux_service::protocol::AgentPaneDirection::Bottom,
        false,
        &pane_children,
        &tab_filter,
        &split_dir_q,
        &mut split_batch,
    );
    touch_reused_run_pane_spawn_seq(target, &mut commands, &mut spawn_counter, &seq_q);
    out.0 = Some(target);
}

#[test]
fn split_run_pane_becomes_newest_for_followup_placement() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<vmux_layout::pane::SpawnCounter>()
        .init_resource::<SplitRunPaneOutput>()
        .add_systems(Update, split_run_pane_test_system);

    let tab = app
        .world_mut()
        .spawn((vmux_layout::tab::Tab::default(), LastActivatedAt(1)))
        .id();
    let browser_pane = app
        .world_mut()
        .spawn((Pane, vmux_layout::pane::SpawnSeq(10), ChildOf(tab)))
        .id();
    let browser_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(browser_pane)))
        .id();
    app.world_mut()
        .entity_mut(browser_stack)
        .insert(PageMetadata {
            url: "https://news.ycombinator.com".into(),
            ..default()
        });
    app.insert_resource(SplitRunPaneInput { pane: browser_pane });

    app.update();

    let terminal_pane = app.world().resource::<SplitRunPaneOutput>().0.unwrap();
    let seq = app
        .world()
        .get::<vmux_layout::pane::SpawnSeq>(terminal_pane)
        .expect("split run target gets fresh spawn seq")
        .0;
    assert!(seq > 10, "split run target must become newest");
}

#[derive(Resource)]
struct BrowserPaneClaimInput {
    anchor: ProcessId,
}

#[derive(Resource, Default)]
struct BrowserPaneClaimOutput(Option<Entity>);

fn claim_browser_pane_test_system(
    input: Res<BrowserPaneClaimInput>,
    mut resolve: AgentBrowserResolve,
    mut out: ResMut<BrowserPaneClaimOutput>,
) {
    out.0 = resolve
        .claim_browser_pane(input.anchor)
        .map(|(pane, _)| pane);
}

fn spawn_stack_in_pane(app: &mut App, pane: Entity, url: &str) -> Entity {
    let stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(pane)))
        .id();
    app.world_mut().entity_mut(stack).insert(PageMetadata {
        url: url.to_string(),
        ..default()
    });
    stack
}

fn close_stack_requests(app: &App) -> Vec<Entity> {
    let messages = app
        .world()
        .resource::<bevy::ecs::message::Messages<vmux_layout::CloseStackRequest>>();
    let mut cursor = messages.get_cursor();
    cursor.read(messages).map(|m| m.stack).collect()
}

fn spawn_file_preview_stack(app: &mut App, pane: Entity, ts: i64, url: &str) -> Entity {
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            vmux_core::LastActivatedAt(ts),
            ChildOf(pane),
        ))
        .id();
    app.world_mut().spawn((
        PageMetadata {
            url: url.to_string(),
            ..default()
        },
        ChildOf(stack),
    ));
    stack
}

#[test]
fn tidy_page_on_idle_closes_clean_previews_for_native_chat_cli() {
    let mut settings = test_settings();
    settings.agent.tidy_files_auto = true;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_layout::CloseStackRequest>()
        .add_message::<vmux_core::PageOpenRequest>()
        .add_message::<vmux_layout::OpenBesideRequest>()
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .add_message::<vmux_layout::worktree::TabDirectoryObserved>()
        .insert_resource(settings)
        .add_systems(Update, tidy_page_on_idle);

    let parent = app.world_mut().spawn(vmux_layout::tab::Tab::default()).id();
    let agent_pane = app.world_mut().spawn((Pane, ChildOf(parent))).id();
    let agent_stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            crate::components::AgentSession {
                kind: vmux_core::agent::AgentKind::Claude,
                variant: crate::AgentVariant::Cli,
                sid: "sid-1".to_string(),
                provider: "claude".to_string(),
                model: "cli".to_string(),
            },
            crate::AgentRunState::Streaming,
            ChildOf(agent_pane),
        ))
        .id();
    let file_pane = app.world_mut().spawn((Pane, ChildOf(parent))).id();
    let previews: Vec<Entity> = (0..6)
        .map(|i| {
            spawn_file_preview_stack(&mut app, file_pane, i, &format!("file:///clean/f{i}.rs"))
        })
        .collect();

    app.update();
    assert!(
        close_stack_requests(&app).is_empty(),
        "streaming (not idle) must not tidy"
    );

    *app.world_mut()
        .get_mut::<crate::AgentRunState>(agent_stack)
        .unwrap() = crate::AgentRunState::Idle;
    app.update();

    let mut closed = close_stack_requests(&app);
    closed.sort();
    let mut expected = previews[0..5].to_vec();
    expected.sort();
    assert_eq!(
        closed, expected,
        "clean non-active previews close; the active (max LastActivatedAt) preview is kept"
    );
    assert!(
        !closed.contains(&previews[5]),
        "active preview must be kept"
    );
}

fn browser_claim_app() -> (App, ProcessId, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_layout::active_panes::ActivatePane>()
        .init_resource::<vmux_layout::active_panes::ActivePanes>()
        .init_resource::<BrowserPaneClaimOutput>()
        .add_systems(Update, claim_browser_pane_test_system);
    let split = app
        .world_mut()
        .spawn((
            Pane,
            PaneSplit {
                direction: vmux_layout::pane::PaneSplitDirection::Row,
            },
        ))
        .id();
    let agent_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
    let agent_stack = app
        .world_mut()
        .spawn((vmux_layout::stack::stack_bundle(), ChildOf(agent_pane)))
        .id();
    let anchor = ProcessId::new();
    app.world_mut().spawn((
        Terminal,
        anchor,
        AgentSession {
            kind: AgentKind::Codex,
        },
        ChildOf(agent_stack),
    ));
    app.insert_resource(BrowserPaneClaimInput { anchor });
    (app, anchor, split)
}

#[test]
fn browser_pane_claim_ignores_mixed_file_browser_pane() {
    let (mut app, _anchor, split) = browser_claim_app();
    let mixed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
    spawn_stack_in_pane(&mut app, mixed_pane, "file:///repo/src/main.rs");
    let browser_stack = spawn_stack_in_pane(&mut app, mixed_pane, "https://example.com");
    app.world_mut()
        .entity_mut(browser_stack)
        .insert(vmux_layout::Browser);

    app.update();

    assert_eq!(app.world().resource::<BrowserPaneClaimOutput>().0, None);
}

#[test]
fn browser_pane_claim_prefers_pure_browser_pane_over_mixed_pane() {
    let (mut app, _anchor, split) = browser_claim_app();
    let mixed_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
    spawn_stack_in_pane(&mut app, mixed_pane, "file:///repo/src/main.rs");
    let mixed_browser = spawn_stack_in_pane(&mut app, mixed_pane, "https://mixed.example");
    app.world_mut()
        .entity_mut(mixed_browser)
        .insert(vmux_layout::Browser);
    let pure_pane = app.world_mut().spawn((Pane, ChildOf(split))).id();
    let pure_browser = spawn_stack_in_pane(&mut app, pure_pane, "https://pure.example");
    app.world_mut()
        .entity_mut(pure_browser)
        .insert(vmux_layout::Browser);

    app.update();

    assert_eq!(
        app.world().resource::<BrowserPaneClaimOutput>().0,
        Some(pure_pane)
    );
}

#[test]
fn run_reuses_existing_terminal_when_region_cache_is_empty() {
    let anchor = ProcessId::new();
    let terminal = ProcessId::new();
    let agent_pane = Entity::from_bits(10);
    let terminal_pane = Entity::from_bits(20);
    let regions = AgentTerminalRegions::default();
    let candidates = [RunTerminalCandidate {
        terminal: Entity::from_bits(19),
        pid: terminal,
        stack: Entity::from_bits(21),
        pane: terminal_pane,
        pane_spawn_seq: 7,
    }];

    let picked = choose_reusable_run_terminal(anchor, agent_pane, &regions, &candidates).unwrap();

    assert_eq!(picked.pid, terminal);
    assert_eq!(picked.pane, terminal_pane);
}

#[test]
fn run_placement_policy_rejects_override_by_default() {
    let settings = test_settings();
    assert_eq!(
        validate_run_placement_policy(&settings, true),
        Err("run placement overrides are disabled; omit mode, direction, and beside and retry")
    );
}

#[test]
fn run_placement_policy_allows_bare_run() {
    let settings = test_settings();
    assert_eq!(validate_run_placement_policy(&settings, false), Ok(()));
}

#[test]
fn run_placement_policy_honors_user_opt_out() {
    let mut settings = test_settings();
    settings.agent.allow_run_placement_override = true;
    assert_eq!(validate_run_placement_policy(&settings, true), Ok(()));
}

#[test]
fn run_reuses_cached_terminal_before_newer_terminal_candidates() {
    let anchor = ProcessId::new();
    let cached = ProcessId::new();
    let newer = ProcessId::new();
    let agent_pane = Entity::from_bits(10);
    let cached_pane = Entity::from_bits(20);
    let newer_pane = Entity::from_bits(30);
    let mut regions = AgentTerminalRegions::default();
    regions.run_terminals.insert(anchor, cached);
    regions.run_panes.insert(anchor, cached_pane);
    let candidates = [
        RunTerminalCandidate {
            terminal: Entity::from_bits(19),
            pid: cached,
            stack: Entity::from_bits(21),
            pane: cached_pane,
            pane_spawn_seq: 3,
        },
        RunTerminalCandidate {
            terminal: Entity::from_bits(29),
            pid: newer,
            stack: Entity::from_bits(31),
            pane: newer_pane,
            pane_spawn_seq: 9,
        },
    ];

    let picked = choose_reusable_run_terminal(anchor, agent_pane, &regions, &candidates).unwrap();

    assert_eq!(picked.pid, cached);
    assert_eq!(picked.pane, cached_pane);
}

#[derive(Resource)]
struct ReusedRunTerminalFocusInput {
    candidate: RunTerminalCandidate,
}

fn focus_reused_run_terminal_test_system(
    input: Res<ReusedRunTerminalFocusInput>,
    mut commands: Commands,
    child_of_q: Query<&ChildOf>,
    tab_q: Query<Entity, With<vmux_layout::tab::Tab>>,
) {
    focus_reused_run_terminal(input.candidate, &mut commands, &child_of_q, &tab_q);
}

#[test]
fn reused_run_terminal_focus_activates_stack_pane_and_tab() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Update, focus_reused_run_terminal_test_system);
    let tab = app
        .world_mut()
        .spawn((vmux_layout::tab::Tab::default(), LastActivatedAt(1)))
        .id();
    let pane = app
        .world_mut()
        .spawn((
            Pane,
            vmux_layout::pane::SpawnSeq(7),
            LastActivatedAt(2),
            ChildOf(tab),
        ))
        .id();
    let stack = app
        .world_mut()
        .spawn((
            vmux_layout::stack::stack_bundle(),
            LastActivatedAt(3),
            ChildOf(pane),
        ))
        .id();
    app.insert_resource(ReusedRunTerminalFocusInput {
        candidate: RunTerminalCandidate {
            terminal: Entity::from_bits(4),
            pid: ProcessId::new(),
            stack,
            pane,
            pane_spawn_seq: 7,
        },
    });

    app.update();

    assert!(app.world().get::<LastActivatedAt>(tab).unwrap().0 > 1);
    assert!(app.world().get::<LastActivatedAt>(pane).unwrap().0 > 2);
    assert!(app.world().get::<LastActivatedAt>(stack).unwrap().0 > 3);
}

#[test]
fn split_run_stacks_into_cached_terminal_bucket_pane() {
    let anchor = ProcessId::new();
    let terminal = ProcessId::new();
    let agent_pane = Entity::from_bits(10);
    let terminal_pane = Entity::from_bits(20);
    let mut regions = AgentTerminalRegions::default();
    regions.run_panes.insert(anchor, terminal_pane);
    let candidates = [RunTerminalCandidate {
        terminal: Entity::from_bits(19),
        pid: terminal,
        stack: Entity::from_bits(21),
        pane: terminal_pane,
        pane_spawn_seq: 7,
    }];

    assert_eq!(
        choose_run_terminal_bucket_pane(anchor, agent_pane, &regions, &candidates),
        Some(terminal_pane)
    );
}

#[test]
fn split_run_keeps_cached_terminal_bucket_after_process_exits() {
    let anchor = ProcessId::new();
    let agent_pane = Entity::from_bits(10);
    let terminal_pane = Entity::from_bits(20);
    let mut regions = AgentTerminalRegions::default();
    regions.run_panes.insert(anchor, terminal_pane);
    let candidates = [];

    assert_eq!(
        choose_run_terminal_bucket_pane(anchor, agent_pane, &regions, &candidates),
        Some(terminal_pane)
    );
}
