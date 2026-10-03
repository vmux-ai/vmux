use bevy::prelude::*;
use vmux_api::protocol::{AcpSessionConfig, ApprovalDecision, ClientMessage, SharedMessage};
#[cfg(test)]
use vmux_ecs::ProcessId;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_ecs::team::Profile;
use vmux_ecs::{Cwd, EntityTarget, LastActivatedAt, PageMetadata, ProcessAnchor};
use vmux_git::worktree::ValidatedLinkedWorkspace;
use vmux_layout::pane::PanePlacement;
use vmux_layout::stack::Stack;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::TabWorktreeReady;
use vmux_terminal::ReattachedTerminalBundle;

use super::handoff::PendingHandoff;
#[cfg(test)]
use super::runtime_driver::AcpWorkspaceState;
use super::runtime_driver::PromptWorkspace;
use crate::host::acp::AcpLaunchStarted;
use crate::host::event::{
    AcpAgentInfo, AcpSessionConfigSelectionResult, AcpSessionConfigSnapshot, AcpSessionCreated,
    AcpTerminalCreated, AcpWorkspaceChanged, AgentApprovalRequest,
};
use crate::policy::AcpWorkspacePolicy;
use vmux_chat::host::{ChatView, ImportedConversation};
use vmux_session::{
    AcpSessionId, AgentId, ApprovalPolicy, PromptQueue, Route, RunState, Session, SessionId,
};

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AcpSessionConfigSet;

pub(super) fn add(app: &mut App) {
    app.add_plugins(FeaturePlugin::<crate::Feature>::default());
    crate::policy::add(app);
    crate::host::acp::add_registry(app);
    crate::host::acp::add(app);
    app.add_message::<ServiceRequest>()
        .add_message::<AcpAgentInfo>()
        .add_message::<AcpWorkspaceChanged>()
        .add_message::<AcpSessionConfigSnapshot>()
        .add_message::<AcpSessionConfigSelectionResult>()
        .add_message::<AcpSessionCreated>()
        .add_message::<AcpTerminalCreated>()
        .add_systems(
            Update,
            (
                input.after(ServiceMessageSet),
                (
                    info,
                    workspace,
                    (config.in_set(AcpSessionConfigSet), selection).chain(),
                    session,
                    terminal,
                )
                    .after(ServiceMessageSet),
            ),
        )
        .add_observer(close_on_remove)
        .add_observer(auto_allow);
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpSessionConfigState {
    pub configs: Vec<AcpSessionConfig>,
    pub(super) pending: Vec<PendingAcpSessionConfig>,
    pub(super) initial: Vec<InitialAcpSessionConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingAcpSessionConfig {
    pub request_id: u64,
    pub config_id: Option<String>,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InitialAcpSessionConfig {
    pub(super) config_id: Option<String>,
    pub(super) value: String,
}

fn info(mut reader: MessageReader<AcpAgentInfo>, mut sessions: Query<(&AcpSession, &mut Profile)>) {
    for event in reader.read() {
        let name = event.name.trim();
        if name.is_empty() {
            continue;
        }
        for (session_id, agent_id, mut profile) in &mut sessions {
            if session_id.0 == event.sid && profile.name != name {
                *profile = Profile::registry(name, &agent_id.0);
            }
        }
    }
}

fn ancestor_tab(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(), With<Tab>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn workspace(
    mut reader: MessageReader<AcpWorkspaceChanged>,
    mut sessions: Query<(Entity, &mut AcpSession)>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<Tab>>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    managed: Query<&TabWorktree>,
    mut commands: Commands,
) {
    for event in reader.read() {
        let Ok(validated) = ValidatedLinkedWorkspace::new(
            std::path::Path::new(&event.cwd),
            std::path::Path::new(&event.workspace_cwd),
            &event.branch,
        ) else {
            bevy::log::warn!(sid = %event.sid, "ignored invalid ACP worktree metadata");
            continue;
        };
        let cwd = validated.cwd;
        let workspace_cwd = validated.workspace_cwd;
        let checkout = validated.checkout;
        for (session_entity, session_id, mut session_cwd) in &mut sessions {
            if session_id.0 != event.sid {
                continue;
            }
            let Some(stack) = session_views
                .iter()
                .find(|(_, target)| target.entity() == session_entity)
                .map(|(stack, _)| stack)
            else {
                continue;
            };
            let Some(tab_entity) = ancestor_tab(stack, &child_of, &tab_entities) else {
                continue;
            };
            session_cwd.0.clone_from(&cwd);
            if let Ok(mut tab) = tabs.get_mut(tab_entity) {
                tab.startup_dir = Some(cwd.to_string_lossy().into_owned());
            }
            let workspace_project_dir = workspace_cwd.to_string_lossy().into_owned();
            if let Ok(mut workspace) = workspaces.get_mut(tab_entity) {
                workspace.project_dir.clone_from(&workspace_project_dir);
            } else {
                commands.entity(tab_entity).insert(TabWorkspace {
                    project_dir: workspace_project_dir.clone(),
                });
            }
            let keeps_managed = managed.get(tab_entity).ok().is_some_and(|metadata| {
                metadata.branch == event.branch
                    && std::path::Path::new(&metadata.checkout_dir)
                        .canonicalize()
                        .ok()
                        .as_ref()
                        == Some(&checkout.root)
            });
            let mut entity = commands.entity(tab_entity);
            entity
                .insert(TabDirDecided)
                .remove::<TabWorktreeUnavailable>();
            if !keeps_managed {
                entity.remove::<TabWorktree>().remove::<TabWorktreeReady>();
            } else if let Ok(ready) = TabWorktreeReady::new(
                &cwd,
                &workspace_project_dir,
                managed.get(tab_entity).unwrap(),
                &checkout,
            ) {
                entity.insert(ready);
            } else {
                entity.remove::<TabWorktreeReady>();
            }
        }
    }
}

fn config(
    mut reader: MessageReader<AcpSessionConfigSnapshot>,
    mut sessions: Query<(Entity, &AcpSession, Option<&mut AcpSessionConfigState>)>,
    mut commands: Commands,
) {
    for event in reader.read() {
        for (entity, session_id, current) in &mut sessions {
            if session_id.0 != event.sid {
                continue;
            }
            if event.configs.is_empty() {
                if current.is_some() {
                    commands.entity(entity).remove::<AcpSessionConfigState>();
                }
                continue;
            }
            if let Some(mut current) = current {
                current.pending.retain(|pending| {
                    event.configs.iter().any(|config| {
                        config.config_id == pending.config_id
                            && config
                                .values
                                .iter()
                                .any(|option| option.value == pending.value)
                    })
                });
                for config in &event.configs {
                    if !current
                        .initial
                        .iter()
                        .any(|initial| initial.config_id == config.config_id)
                    {
                        current.initial.push(InitialAcpSessionConfig {
                            config_id: config.config_id.clone(),
                            value: config.current_value.clone(),
                        });
                    }
                }
                current.configs.clone_from(&event.configs);
            } else {
                let initial = event
                    .configs
                    .iter()
                    .map(|config| InitialAcpSessionConfig {
                        config_id: config.config_id.clone(),
                        value: config.current_value.clone(),
                    })
                    .collect();
                commands.entity(entity).insert(AcpSessionConfigState {
                    configs: event.configs.clone(),
                    pending: Vec::new(),
                    initial,
                });
            }
        }
    }
}

fn selection(
    mut reader: MessageReader<AcpSessionConfigSelectionResult>,
    mut sessions: Query<(&AcpSession, &mut AcpSessionConfigState)>,
) {
    for event in reader.read() {
        for (session_id, mut state) in &mut sessions {
            if session_id.0 != event.sid {
                continue;
            }
            let Some(index) = state.pending.iter().position(|pending| {
                pending.request_id == event.request_id
                    && pending.config_id == event.config_id
                    && pending.value == event.value
            }) else {
                continue;
            };
            state.pending.remove(index);
            if event.succeeded
                && let Some(config) = state
                    .configs
                    .iter_mut()
                    .find(|config| config.config_id == event.config_id)
            {
                config.current_value.clone_from(&event.value);
            }
        }
    }
}

fn auto_allow(
    trigger: On<AgentApprovalRequest>,
    sessions: Query<(&SessionId, &ApprovalPolicy), With<Session>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let request = trigger.event();
    let Ok((session_id, policy)) = sessions.get(request.session) else {
        return;
    };
    if !policy.allows(&request.name) {
        return;
    }
    service_requests.write(ServiceRequest(ClientMessage::Shared(
        SharedMessage::AgentApprove {
            sid: session_id.0.clone(),
            call_id: request.call_id.clone(),
            decision: ApprovalDecision::AllowAlways,
        },
    )));
}

#[allow(clippy::type_complexity)]
fn session(
    mut reader: MessageReader<AcpSessionCreated>,
    mut sessions: Query<(Entity, &mut AcpSession, &mut PageMetadata), Without<ChatView>>,
    children: Query<&Children>,
    mut page_meta: Query<&mut PageMetadata, With<ChatView>>,
    mut commands: Commands,
) {
    for ev in reader.read() {
        for (session_entity, session_id) in &sessions {
            if session_id.0 != ev.sid {
                continue;
            }
            commands
                .entity(session_entity)
                .insert(AcpSessionId(ev.acp_session_id.clone()));
            let url = Route::Session(session_id.clone()).url();
            for (target, kids) in &stacks {
                if target.entity() != session_entity {
                    continue;
                }
                for kid in kids.iter() {
                    if let Ok(mut meta) = page_meta.get_mut(kid)
                        && meta.url != url
                    {
                        meta.url = url.clone();
                    }
                }
            }
        }
    }
}

fn terminal(
    mut reader: MessageReader<AcpTerminalCreated>,
    sessions: Query<(Entity, &AcpSession)>,
    mut ctx: PanePlacement,
    mut commands: Commands,
) {
    let mut split_batch = std::collections::HashSet::new();
    for ev in reader.read() {
        let Some(session_entity) = sessions
            .iter()
            .find(|(_, session_id)| session_id.0 == ev.sid)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        let Some(stack) = stacks
            .iter()
            .find(|(_, target)| target.entity() == session_entity)
            .map(|(stack, _)| stack)
        else {
            continue;
        };
        let Ok(agent_pane) = ctx.child_of_q.get(stack).map(|child_of| child_of.parent()) else {
            continue;
        };
        let target_pane = ctx.resolve_spiral(
            agent_pane,
            vmux_terminal::TerminalPlugin::URL,
            false,
            &mut split_batch,
        );
        let tab = commands
            .spawn((Stack::bundle(), LastActivatedAt(0), ChildOf(target_pane)))
            .id();
        commands.spawn((
            ReattachedTerminalBundle::new(ev.process_id),
            vmux_terminal::RetainOnProcessExit,
            ChildOf(tab),
        ));
    }
}

fn input(
    mut q: Query<(
        Entity,
        &SessionId,
        &AgentId,
        &mut RunState,
        &mut PromptQueue,
        Has<AcpLaunchStarted>,
        Option<&mut PendingHandoff>,
        Option<&mut ImportedConversation>,
    )>,
    workspace: PromptWorkspace,
    policy: Single<&AcpWorkspacePolicy>,
    modes: Option<Single<&crate::host::model_selection::AgentModeSelections>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (
        entity,
        session_id,
        agent_id,
        mut state,
        mut queue,
        install_started,
        mut pending,
        mut imported,
    ) in &mut q
    {
        if !acp_prompt_dispatch_ready(&state, &queue, install_started) {
            continue;
        }
        let Some(prompt) = queue.take_next() else {
            continue;
        };
        let text = prompt.text;
        let handoff = pending.as_deref_mut().and_then(|pending| {
            if pending.sent {
                return None;
            }
            pending.sent = true;
            Some(pending.context.clone())
        });
        if handoff.is_some()
            && let Some(imported) = imported.as_deref_mut()
            && imported.first_prompt.is_none()
        {
            imported.first_prompt = Some(text.clone());
        }
        let workspace_state = workspace.state(entity);
        let context = PromptWorkspace::prompt(&policy, handoff, workspace_state);
        let preferred_mode = modes
            .as_ref()
            .and_then(|modes| modes.by_agent.get(&session.agent_id))
            .map(|memory| memory.selected.clone())
            .filter(|mode| !mode.is_empty());
        service_requests.write(ServiceRequest(
            SharedMessage::AgentInput {
                sid: session_id.0.clone(),
                text,
                context,
                attachments: prompt.attachments,
                preferred_mode,
            }
            .into(),
        ));
        *state = RunState::Streaming;
    }
}

fn acp_prompt_dispatch_ready(state: &RunState, queue: &PromptQueue, install_started: bool) -> bool {
    install_started && queue.ready(matches!(state, RunState::Idle))
}

fn close_on_remove(
    trigger: On<Remove, ProcessAnchor>,
    sessions: Query<&SessionId, With<Session>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(session_id) = sessions.get(trigger.event_target()) else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::CloseAgentSession {
        sid: session_id.0.clone(),
    }));
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::protocol::AcpSessionConfigValue;
    use vmux_layout::pane::Pane;

    use super::*;

    struct TestSession;

    impl TestSession {
        fn bundle(agent: &str, sid: &str, cwd: impl Into<std::path::PathBuf>) -> impl Bundle {
            (
                Session,
                SessionId(sid.into()),
                AgentId(agent.into()),
                Cwd(cwd.into()),
                ProcessAnchor(ProcessId::new()),
            )
        }
    }

    #[test]
    fn auto_approval_targets_requested_session_and_call() {
        let mut app = App::new();
        app.add_message::<ServiceRequest>().add_observer(auto_allow);
        let mut policy = ApprovalPolicy::default();
        policy.allow("run");
        let session = app
            .world_mut()
            .spawn((TestSession::bundle("claude", "s1", "/tmp"), policy))
            .id();
        app.world_mut().trigger(AgentApprovalRequest {
            session,
            call_id: "call-1".into(),
            name: "run".into(),
        });
        app.update();
        let requests = app
            .world_mut()
            .resource_mut::<Messages<ServiceRequest>>()
            .drain()
            .collect::<Vec<_>>();

        assert!(matches!(
            requests.as_slice(),
            [ServiceRequest(ClientMessage::Shared(SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            }))]
                if sid == "s1"
                    && call_id == "call-1"
                    && *decision == ApprovalDecision::AllowAlways
        ));
    }

    #[test]
    fn queued_prompt_waits_for_acp_install_start() {
        let mut queue = PromptQueue::default();
        queue.enqueue("hello".to_string());

        assert!(!acp_prompt_dispatch_ready(&RunState::Idle, &queue, false));
        assert!(acp_prompt_dispatch_ready(&RunState::Idle, &queue, true));
        assert!(!acp_prompt_dispatch_ready(
            &RunState::Errored("failed".into()),
            &queue,
            true
        ));
    }

    #[test]
    fn unbound_workspace_context_requires_project_selection_before_file_access() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context =
            PromptWorkspace::prompt(&policy, None, Some(AcpWorkspaceState::Unbound)).unwrap();

        assert!(context.contains("Before accessing project files"));
        assert!(context.contains("select_project"));
        assert!(context.contains("request_user_choice"));
        assert!(context.contains("explicit user approval"));
        assert!(context.contains("~/.vmux/projects/<remote-host>"));
        assert!(context.contains("~/.vmux/projects/local/<project>"));
        assert!(context.contains("create the empty directory"));
        assert!(context.contains("use the new project root directly"));
        assert!(context.contains("Do not search the user's home directory"));
        assert!(context.contains("open the picker"));
    }

    #[test]
    fn repository_context_defers_worktree_until_mutation() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context = PromptWorkspace::prompt(
            &policy,
            None,
            Some(AcpWorkspaceState::RepositoryNeedsWorktree),
        )
        .unwrap();

        assert!(context.contains("Reading and inspection are allowed"));
        assert!(context.contains("Never call create_worktree"));
        assert!(context.contains("Immediately before the first edit"));
        assert!(context.contains("create_worktree"));
        assert!(context.contains("request_user_choice"));
        assert!(context.contains("Never run git worktree add"));
    }

    #[test]
    fn pending_worktree_context_requires_waiting_for_activation() {
        let policy = crate::policy_driver::PolicyDriver::bundled();
        let context = PromptWorkspace::prompt(
            &policy,
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
        let policy = crate::policy_driver::PolicyDriver::bundled();
        assert_eq!(
            PromptWorkspace::prompt(
                &policy,
                Some("prior conversation".into()),
                Some(AcpWorkspaceState::Bound),
            )
            .as_deref(),
            Some("prior conversation")
        );
    }

    #[test]
    fn ancestor_workspace_state_tracks_pending_and_bound_tab() {
        let mut app = App::new();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            })
            .id();
        let session = app.world_mut().spawn(Session).id();
        app.world_mut()
            .spawn((EntityTarget::<Session>::new(session), ChildOf(tab)));
        let state = |world: &mut World| {
            world
                .run_system_once(move |workspace: PromptWorkspace| workspace.state(session))
                .unwrap()
        };

        assert_eq!(state(app.world_mut()), Some(AcpWorkspaceState::Unbound));
        app.world_mut()
            .entity_mut(tab)
            .insert(vmux_space::PendingProject("/repo".into()));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::PendingWorktree)
        );
        app.world_mut().entity_mut(tab).insert((
            Tab {
                name: "Tab 1".into(),
                startup_dir: Some("/repo".into()),
            },
            vmux_space::RepositoryNeedsWorktree,
        ));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::RepositoryNeedsWorktree)
        );
        app.world_mut()
            .entity_mut(tab)
            .insert(TabWorkspace {
                project_dir: "/repo".into(),
            })
            .remove::<vmux_space::RepositoryNeedsWorktree>();
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
        vmux_git::worktree::CheckoutInfo::try_from(repo.path())
            .unwrap()
            .add_worktree(&worktree, "vibe/quiet-amber-wolf", "main")
            .unwrap();
        let project_dir = repo.path().canonicalize().unwrap();
        let worktree_dir = worktree.canonicalize().unwrap();
        let mut app = App::new();
        app.add_message::<crate::host::event::AcpWorkspaceChanged>()
            .add_systems(Update, workspace);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "matching".into(),
                    startup_dir: Some(project_dir.to_string_lossy().into_owned()),
                },
                TabWorkspace {
                    project_dir: project_dir.to_string_lossy().into_owned(),
                },
            ))
            .id();
        let session = app
            .world_mut()
            .spawn(TestSession::bundle(
                "mistral-vibe",
                "matching-sid",
                project_dir.clone(),
            ))
            .id();
        app.world_mut()
            .spawn((EntityTarget::<Session>::new(session), ChildOf(tab)));
        let unrelated_tab = app
            .world_mut()
            .spawn(Tab {
                name: "unrelated".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            })
            .id();
        app.world_mut()
            .resource_mut::<Messages<crate::host::event::AcpWorkspaceChanged>>()
            .write(crate::host::event::AcpWorkspaceChanged {
                sid: "matching-sid".into(),
                branch: "vibe/quiet-amber-wolf".into(),
                cwd: worktree_dir.to_string_lossy().into_owned(),
                workspace_cwd: project_dir.to_string_lossy().into_owned(),
            });

        app.update();

        assert_eq!(app.world().get::<Cwd>(session).unwrap().0, worktree_dir);
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(worktree_dir.to_string_lossy().as_ref())
        );
        assert_eq!(
            app.world()
                .get::<Tab>(unrelated_tab)
                .unwrap()
                .startup_dir
                .as_deref(),
            Some(project_dir.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn live_acp_identity_updates_only_matching_profile() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        add(&mut app);
        let matching = app
            .world_mut()
            .spawn((
                TestSession::bundle("antigravity", "s1", "/tmp"),
                Profile::registry("Configured", "antigravity"),
            ))
            .id();
        let unrelated = app
            .world_mut()
            .spawn((
                TestSession::bundle("claude", "s2", "/tmp"),
                Profile::registry("Claude", "claude"),
            ))
            .id();

        app.world_mut().write_message(AcpAgentInfo {
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

        app.world_mut().write_message(AcpAgentInfo {
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
    fn live_acp_config_state_updates_only_matching_session() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        add(&mut app);
        let matching = app
            .world_mut()
            .spawn(TestSession::bundle("claude", "s1", "/tmp"))
            .id();
        let unrelated = app
            .world_mut()
            .spawn(TestSession::bundle("codex", "s2", "/tmp"))
            .id();

        app.world_mut().write_message(AcpSessionConfigSnapshot {
            sid: "s1".into(),
            configs: vec![AcpSessionConfig {
                config_id: Some("model".into()),
                name: "Model".into(),
                description: None,
                category: Some("model".into()),
                current_value: "sonnet".into(),
                values: vec![AcpSessionConfigValue {
                    value: "sonnet".into(),
                    name: "Claude Sonnet".into(),
                    description: None,
                    group: None,
                }],
            }],
        });
        app.update();

        let state = app.world().get::<AcpSessionConfigState>(matching).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(state.display_name(model), "Claude Sonnet");
        assert!(state.pending.is_empty());
        assert!(
            app.world()
                .get::<AcpSessionConfigState>(unrelated)
                .is_none()
        );
    }

    #[test]
    fn config_results_preserve_latest_pending_selection() {
        let values = ["default", "opus", "fable"]
            .into_iter()
            .map(|value| AcpSessionConfigValue {
                value: value.into(),
                name: value.into(),
                description: None,
                group: None,
            })
            .collect::<Vec<_>>();
        let mut app = App::new();
        app.add_message::<AcpSessionConfigSnapshot>()
            .add_message::<AcpSessionConfigSelectionResult>()
            .add_systems(Update, (config, selection).chain());
        let entity = app
            .world_mut()
            .spawn((
                TestSession::bundle("claude", "s1", "/tmp"),
                AcpSessionConfigState {
                    configs: vec![AcpSessionConfig {
                        config_id: Some("model".into()),
                        name: "Model".into(),
                        description: None,
                        category: Some("model".into()),
                        current_value: "default".into(),
                        values: values.clone(),
                    }],
                    pending: vec![PendingAcpSessionConfig {
                        request_id: 2,
                        config_id: Some("model".into()),
                        value: "fable".into(),
                    }],
                    initial: vec![InitialAcpSessionConfig {
                        config_id: Some("model".into()),
                        value: "default".into(),
                    }],
                },
            ))
            .id();

        app.world_mut().write_message(AcpSessionConfigSnapshot {
            sid: "s1".into(),
            configs: vec![AcpSessionConfig {
                config_id: Some("model".into()),
                name: "Model".into(),
                description: None,
                category: Some("model".into()),
                current_value: "opus".into(),
                values,
            }],
        });
        app.update();

        let state = app.world().get::<AcpSessionConfigState>(entity).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(model.current_value, "opus");
        assert_eq!(state.pending[0].request_id, 2);
        assert_eq!(state.display_name(model), "fable");

        app.world_mut()
            .write_message(AcpSessionConfigSelectionResult {
                sid: "s1".into(),
                request_id: 1,
                config_id: Some("model".into()),
                value: "fable".into(),
                succeeded: false,
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<AcpSessionConfigState>(entity)
                .unwrap()
                .pending[0]
                .request_id,
            2
        );

        app.world_mut()
            .write_message(AcpSessionConfigSelectionResult {
                sid: "s1".into(),
                request_id: 2,
                config_id: Some("model".into()),
                value: "fable".into(),
                succeeded: false,
            });
        app.update();
        let state = app.world().get::<AcpSessionConfigState>(entity).unwrap();
        let model = state.category("model").unwrap();
        assert!(state.pending.is_empty());
        assert_eq!(state.display_name(model), "opus");

        {
            let mut state = app
                .world_mut()
                .get_mut::<AcpSessionConfigState>(entity)
                .unwrap();
            state.pending.push(PendingAcpSessionConfig {
                request_id: 3,
                config_id: Some("model".into()),
                value: "fable".into(),
            });
        }
        app.world_mut()
            .write_message(AcpSessionConfigSelectionResult {
                sid: "s1".into(),
                request_id: 3,
                config_id: Some("model".into()),
                value: "fable".into(),
                succeeded: true,
            });
        app.update();
        let state = app.world().get::<AcpSessionConfigState>(entity).unwrap();
        let model = state.category("model").unwrap();
        assert_eq!(model.current_value, "fable");
        assert!(state.pending.is_empty());
    }

    #[test]
    fn acp_terminal_stack_does_not_take_focus_from_agent() {
        let mut app = App::new();
        app.add_message::<AcpTerminalCreated>()
            .add_systems(Update, terminal);
        let tab = app.world_mut().spawn(Tab::bundle()).id();
        let pane = app.world_mut().spawn((Pane::bundle(), ChildOf(tab))).id();
        let session = app
            .world_mut()
            .spawn(TestSession::bundle("claude", "s1", "/tmp"))
            .id();
        let agent = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                LastActivatedAt(10),
                ChildOf(pane),
                EntityTarget::<Session>::new(session),
            ))
            .id();
        app.world_mut().entity_mut(agent).insert(PageMetadata {
            url: "vmux://sessions/claude".into(),
            ..default()
        });
        app.world_mut().write_message(AcpTerminalCreated {
            sid: "s1".into(),
            process_id: ProcessId::new(),
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
    fn plugin_builds_and_runs_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        add(&mut app);
        app.world_mut().spawn(AcpSession {
            agent_id: "vibe-acp".to_string(),
            sid: "s1".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            anchor: ProcessId::new(),
            resume: None,
        });
        app.update();
    }
}
