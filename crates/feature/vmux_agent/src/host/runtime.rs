use bevy::prelude::*;
use crossbeam_channel::Receiver;
use vmux_api::protocol::{AcpSessionConfig, ApprovalDecision, ClientMessage, SharedMessage};
#[cfg(test)]
use vmux_ecs::ProcessId;
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_ecs::team::Profile;
use vmux_ecs::{LastActivatedAt, PageMetadata};
use vmux_git::worktree::ValidatedLinkedWorkspace;
use vmux_layout::pane::PanePlacement;
use vmux_layout::stack::Stack;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::TabWorktreeReady;
use vmux_terminal::ReattachedTerminalBundle;

use super::handoff::PendingHandoff;
use crate::host::acp::registry::{Registry, RegistryAgent};
use crate::host::acp::{AcpLaunchStarted, AcpToolPlugin};
use crate::host::event::{
    AgentApprovalRequest, UiAgentAcpTerminalCreated, UiAgentInfo,
    UiAgentSessionConfigSelectionResult, UiAgentSessionConfigState, UiAgentSessionCreated,
    UiAgentWorkspaceChanged,
};
use crate::policy::{AcpWorkspacePolicy, AgentPolicyPlugin};
use vmux_chat::host::{ChatView, ImportedConversation};
use vmux_session::AgentRunState;
use vmux_session::{AcpSession, AgentApprovalPolicy, PromptQueue};

pub struct AgentRuntimePlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AcpSessionConfigSet;

impl Plugin for AgentRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FeaturePlugin::<crate::Feature>::default(),
            AgentPolicyPlugin,
        ))
        .add_message::<ServiceRequest>()
        .add_plugins(AcpToolPlugin)
        .add_message::<UiAgentInfo>()
        .add_message::<UiAgentWorkspaceChanged>()
        .add_message::<UiAgentSessionConfigState>()
        .add_message::<UiAgentSessionConfigSelectionResult>()
        .add_message::<UiAgentSessionCreated>()
        .add_message::<UiAgentAcpTerminalCreated>()
        .add_systems(Startup, fetch_catalog)
        .add_systems(
            Update,
            (
                send_input,
                receive_catalog,
                (
                    apply_info,
                    apply_workspace,
                    (apply_config.in_set(AcpSessionConfigSet), apply_selection).chain(),
                    apply_session,
                    apply_terminal,
                )
                    .after(ServiceMessageSet),
            ),
        )
        .add_observer(close_on_remove)
        .add_observer(auto_allow);
    }
}

impl AcpWorkspaceState {
    fn context<'a>(self, policy: &'a AcpWorkspacePolicy) -> Option<&'a str> {
        match self {
            Self::Bound => None,
            Self::Unbound => Some(&policy.unbound),
            Self::PendingWorktree => Some(&policy.pending_worktree),
            Self::RepositoryNeedsWorktree => Some(&policy.repository_needs_worktree),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcpWorkspaceState {
    Bound,
    Unbound,
    PendingWorktree,
    RepositoryNeedsWorktree,
}

fn ancestor_acp_workspace_state(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&Tab>,
    workspaces: &Query<(), With<TabWorkspace>>,
    pending_projects: &Query<(), With<vmux_space::PendingProject>>,
    repositories_needing_worktrees: &Query<(), With<vmux_space::RepositoryNeedsWorktree>>,
) -> Option<AcpWorkspaceState> {
    let mut current = entity;
    loop {
        if let Ok(tab) = tabs.get(current) {
            let state = match tab.startup_dir.as_deref() {
                Some(_) if repositories_needing_worktrees.contains(current) => {
                    AcpWorkspaceState::RepositoryNeedsWorktree
                }
                Some(_) => AcpWorkspaceState::Bound,
                None if workspaces.contains(current) => AcpWorkspaceState::Bound,
                None if pending_projects.contains(current) => AcpWorkspaceState::PendingWorktree,
                None => AcpWorkspaceState::Unbound,
            };
            return Some(state);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn acp_prompt_context(
    policy: &AcpWorkspacePolicy,
    handoff: Option<String>,
    workspace_state: Option<AcpWorkspaceState>,
) -> Option<String> {
    let policy = workspace_state.and_then(|state| state.context(policy));
    match (handoff, policy) {
        (Some(handoff), Some(policy)) => Some(format!("{handoff}\n\n{policy}")),
        (Some(handoff), None) => Some(handoff),
        (None, Some(policy)) => Some(policy.to_string()),
        (None, None) => None,
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct AcpSessionConfigState {
    pub configs: Vec<AcpSessionConfig>,
    pub(crate) pending: Vec<PendingAcpSessionConfig>,
    pub(crate) initial: Vec<InitialAcpSessionConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingAcpSessionConfig {
    pub request_id: u64,
    pub config_id: Option<String>,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InitialAcpSessionConfig {
    pub(crate) config_id: Option<String>,
    pub(crate) value: String,
}

impl AcpSessionConfigState {
    pub fn category(&self, category: &str) -> Option<&AcpSessionConfig> {
        self.configs
            .iter()
            .find(|config| config.category.as_deref() == Some(category))
    }

    pub fn display_value<'a>(&'a self, config: &'a AcpSessionConfig) -> &'a str {
        self.pending
            .iter()
            .find(|pending| pending.config_id == config.config_id)
            .map(|pending| pending.value.as_str())
            .unwrap_or(&config.current_value)
    }

    pub fn display_name<'a>(&'a self, config: &'a AcpSessionConfig) -> &'a str {
        let value = self.display_value(config);
        config
            .values
            .iter()
            .find(|option| option.value == value)
            .map(|option| option.name.as_str())
            .unwrap_or(value)
    }

    pub fn initial_value<'a>(&'a self, config: &'a AcpSessionConfig) -> &'a str {
        self.initial
            .iter()
            .find(|initial| initial.config_id == config.config_id)
            .map(|initial| initial.value.as_str())
            .unwrap_or(&config.current_value)
    }
}

#[derive(Component)]
struct AcpCatalogFetch {
    rx: Receiver<Vec<RegistryAgent>>,
}

fn fetch_catalog(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        let agents = Registry::fetch_blocking()
            .ok()
            .or_else(Registry::cached)
            .map(|r| r.agents)
            .unwrap_or_default();
        let _ = tx.send(agents);
    });
    commands.spawn(AcpCatalogFetch { rx });
}

fn receive_catalog(
    fetches: Query<(Entity, &AcpCatalogFetch)>,
    current: Query<Entity, With<RegistryAgent>>,
    mut commands: Commands,
) {
    let mut received = None;
    for (entity, fetch) in &fetches {
        let Ok(agents) = fetch.rx.try_recv() else {
            continue;
        };
        received = Some(agents);
        commands.entity(entity).despawn();
    }
    let Some(agents) = received else {
        return;
    };
    for entity in &current {
        commands.entity(entity).despawn();
    }
    for agent in agents {
        let name = Name::new(format!("ACP agent {}", agent.id));
        commands.spawn((name, agent));
    }
}

fn apply_info(
    mut reader: MessageReader<UiAgentInfo>,
    mut sessions: Query<(&AcpSession, &mut Profile)>,
) {
    for event in reader.read() {
        let name = event.name.trim();
        if name.is_empty() {
            continue;
        }
        for (session, mut profile) in &mut sessions {
            if session.sid == event.sid && profile.name != name {
                *profile = Profile::registry(name, &session.agent_id);
            }
        }
    }
}

fn validate_acp_workspace(
    event: &UiAgentWorkspaceChanged,
) -> Result<ValidatedLinkedWorkspace, String> {
    vmux_git::worktree::validate_linked_workspace(
        std::path::Path::new(&event.cwd),
        std::path::Path::new(&event.workspace_cwd),
        &event.branch,
    )
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

fn apply_workspace(
    mut reader: MessageReader<UiAgentWorkspaceChanged>,
    mut sessions: Query<(Entity, &mut AcpSession)>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<Tab>>,
    mut tabs: Query<&mut Tab>,
    mut workspaces: Query<&mut TabWorkspace>,
    managed: Query<&TabWorktree>,
    mut commands: Commands,
) {
    for event in reader.read() {
        let Ok(validated) = validate_acp_workspace(event) else {
            bevy::log::warn!(sid = %event.sid, "ignored invalid ACP worktree metadata");
            continue;
        };
        let cwd = validated.cwd;
        let workspace_cwd = validated.workspace_cwd;
        let checkout = validated.checkout;
        for (session_entity, mut session) in &mut sessions {
            if session.sid != event.sid {
                continue;
            }
            let Some(tab_entity) = ancestor_tab(session_entity, &child_of, &tab_entities) else {
                continue;
            };
            session.cwd.clone_from(&cwd);
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

fn apply_config(
    mut reader: MessageReader<UiAgentSessionConfigState>,
    mut sessions: Query<(Entity, &AcpSession, Option<&mut AcpSessionConfigState>)>,
    mut commands: Commands,
) {
    for event in reader.read() {
        for (entity, session, current) in &mut sessions {
            if session.sid != event.sid {
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

fn apply_selection(
    mut reader: MessageReader<UiAgentSessionConfigSelectionResult>,
    mut sessions: Query<(&AcpSession, &mut AcpSessionConfigState)>,
) {
    for event in reader.read() {
        for (session, mut state) in &mut sessions {
            if session.sid != event.sid {
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
    sessions: Query<(&AcpSession, &AgentApprovalPolicy)>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let request = trigger.event();
    let Ok((session, policy)) = sessions.get(request.session) else {
        return;
    };
    if !policy.allows(&request.name) {
        return;
    }
    service_requests.write(ServiceRequest(ClientMessage::Shared(
        SharedMessage::AgentApprove {
            sid: session.sid.clone(),
            call_id: request.call_id.clone(),
            decision: ApprovalDecision::AllowAlways,
        },
    )));
}

#[allow(clippy::type_complexity)]
fn apply_session(
    mut reader: MessageReader<UiAgentSessionCreated>,
    mut sessions: Query<(Entity, &mut AcpSession, &mut PageMetadata), Without<ChatView>>,
    children: Query<&Children>,
    mut page_meta: Query<&mut PageMetadata, With<ChatView>>,
) {
    for ev in reader.read() {
        for (stack, mut session, mut stack_meta) in &mut sessions {
            if session.sid != ev.sid {
                continue;
            }
            session.resume = Some(ev.acp_session_id.clone());
            let url = format!("vmux://sessions/{}/{}", session.agent_id, ev.acp_session_id);
            if stack_meta.url != url {
                stack_meta.url = url.clone();
            }
            if let Ok(kids) = children.get(stack) {
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

fn apply_terminal(
    mut reader: MessageReader<UiAgentAcpTerminalCreated>,
    sessions: Query<(Entity, &AcpSession)>,
    mut ctx: PanePlacement,
    mut commands: Commands,
) {
    let mut split_batch = std::collections::HashSet::new();
    for ev in reader.read() {
        let Some(stack) = sessions
            .iter()
            .find(|(_, session)| session.sid == ev.sid)
            .map(|(entity, _)| entity)
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

fn send_input(
    mut q: Query<(
        Entity,
        &AcpSession,
        &mut AgentRunState,
        &mut PromptQueue,
        Has<AcpLaunchStarted>,
        Option<&mut PendingHandoff>,
        Option<&mut ImportedConversation>,
    )>,
    child_of: Query<&ChildOf>,
    tabs: Query<&Tab>,
    workspaces: Query<(), With<TabWorkspace>>,
    pending_projects: Query<(), With<vmux_space::PendingProject>>,
    repositories_needing_worktrees: Query<(), With<vmux_space::RepositoryNeedsWorktree>>,
    policy: Single<&AcpWorkspacePolicy>,
    modes: Option<Single<&crate::host::model_selection::AgentModeSelections>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, session, mut state, mut queue, install_started, mut pending, mut imported) in
        &mut q
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
        let workspace_state = ancestor_acp_workspace_state(
            entity,
            &child_of,
            &tabs,
            &workspaces,
            &pending_projects,
            &repositories_needing_worktrees,
        );
        let context = acp_prompt_context(&policy, handoff, workspace_state);
        let preferred_mode = modes
            .as_ref()
            .map(|modes| modes.selected_for(&session.agent_id).to_string())
            .filter(|mode| !mode.is_empty());
        service_requests.write(ServiceRequest(
            SharedMessage::AgentInput {
                sid: session.sid.clone(),
                text,
                context,
                attachments: prompt.attachments,
                preferred_mode,
            }
            .into(),
        ));
        *state = AgentRunState::Streaming;
    }
}

fn acp_prompt_dispatch_ready(
    state: &AgentRunState,
    queue: &PromptQueue,
    install_started: bool,
) -> bool {
    install_started && queue.ready(matches!(state, AgentRunState::Idle))
}

fn close_on_remove(
    trigger: On<Remove, AcpSession>,
    sessions: Query<&AcpSession>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let Ok(session) = sessions.get(trigger.event_target()) else {
        return;
    };
    service_requests.write(ServiceRequest(ClientMessage::CloseAgentSession {
        sid: session.sid.clone(),
    }));
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::protocol::AcpSessionConfigValue;
    use vmux_layout::pane::Pane;

    use super::*;

    #[test]
    fn auto_approval_targets_requested_session_and_call() {
        let mut app = App::new();
        app.add_message::<ServiceRequest>().add_observer(auto_allow);
        let session = AcpSession {
            agent_id: "claude".into(),
            sid: "s1".into(),
            cwd: "/tmp".into(),
            anchor: ProcessId::new(),
            resume: None,
        };
        let mut policy = AgentApprovalPolicy::default();
        policy.allow("run");
        let session = app.world_mut().spawn((session, policy)).id();
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
    fn catalog_fetch_entity_is_consumed_after_delivery() {
        let mut app = App::new();
        app.add_systems(Update, receive_catalog);
        let (tx, rx) = crossbeam_channel::unbounded();
        let fetch = app.world_mut().spawn(AcpCatalogFetch { rx }).id();
        tx.send(vec![RegistryAgent {
            id: "agent".into(),
            name: "Agent".into(),
            version: None,
            description: None,
            icon: None,
            distribution: crate::host::acp::registry::Distribution::default(),
        }])
        .unwrap();

        app.update();

        assert!(app.world().get_entity(fetch).is_err());
        let mut agents = app.world_mut().query::<&RegistryAgent>();
        assert_eq!(agents.single(app.world()).unwrap().id, "agent");
    }

    #[test]
    fn unbound_workspace_context_requires_project_selection_before_file_access() {
        let policy = AcpWorkspacePolicy::bundled();
        let context = acp_prompt_context(&policy, None, Some(AcpWorkspaceState::Unbound)).unwrap();

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
        let policy = AcpWorkspacePolicy::bundled();
        let context = acp_prompt_context(
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
        let policy = AcpWorkspacePolicy::bundled();
        let context = acp_prompt_context(
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
        let policy = AcpWorkspacePolicy::bundled();
        assert_eq!(
            acp_prompt_context(
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
        let stack = app.world_mut().spawn(ChildOf(tab)).id();
        let state = |world: &mut World| {
            world
                .run_system_once(
                    move |child_of: Query<&ChildOf>,
                          tabs: Query<&Tab>,
                          workspaces: Query<(), With<TabWorkspace>>,
                          pending: Query<(), With<vmux_space::PendingProject>>,
                          needs_worktree: Query<
                        (),
                        With<vmux_space::RepositoryNeedsWorktree>,
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
        vmux_git::worktree::worktree_add(repo.path(), &worktree, "vibe/quiet-amber-wolf", "main")
            .unwrap();
        let project_dir = repo.path().canonicalize().unwrap();
        let worktree_dir = worktree.canonicalize().unwrap();
        let mut app = App::new();
        app.add_message::<crate::host::event::UiAgentWorkspaceChanged>()
            .add_systems(Update, apply_workspace);
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
            .spawn((
                AcpSession {
                    agent_id: "mistral-vibe".into(),
                    sid: "matching-sid".into(),
                    cwd: project_dir.clone(),
                    anchor: ProcessId::new(),
                    resume: None,
                },
                ChildOf(tab),
            ))
            .id();
        let unrelated_tab = app
            .world_mut()
            .spawn(Tab {
                name: "unrelated".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            })
            .id();
        app.world_mut()
            .resource_mut::<Messages<crate::host::event::UiAgentWorkspaceChanged>>()
            .write(crate::host::event::UiAgentWorkspaceChanged {
                sid: "matching-sid".into(),
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
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AgentRuntimePlugin);
        let matching = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "antigravity".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: ProcessId::new(),
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
                    anchor: ProcessId::new(),
                    resume: None,
                },
                Profile::registry("Claude", "claude"),
            ))
            .id();

        app.world_mut().write_message(UiAgentInfo {
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

        app.world_mut().write_message(UiAgentInfo {
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
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AgentRuntimePlugin);
        let matching = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "claude".into(),
                sid: "s1".into(),
                cwd: "/tmp".into(),
                anchor: ProcessId::new(),
                resume: None,
            })
            .id();
        let unrelated = app
            .world_mut()
            .spawn(AcpSession {
                agent_id: "codex".into(),
                sid: "s2".into(),
                cwd: "/tmp".into(),
                anchor: ProcessId::new(),
                resume: None,
            })
            .id();

        app.world_mut().write_message(UiAgentSessionConfigState {
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
        app.add_message::<UiAgentSessionConfigState>()
            .add_message::<UiAgentSessionConfigSelectionResult>()
            .add_systems(Update, (apply_config, apply_selection).chain());
        let entity = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: ProcessId::new(),
                    resume: None,
                },
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

        app.world_mut().write_message(UiAgentSessionConfigState {
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
            .write_message(UiAgentSessionConfigSelectionResult {
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
            .write_message(UiAgentSessionConfigSelectionResult {
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
            .write_message(UiAgentSessionConfigSelectionResult {
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
        app.add_message::<UiAgentAcpTerminalCreated>()
            .add_systems(Update, apply_terminal);
        let tab = app.world_mut().spawn(Tab::bundle()).id();
        let pane = app.world_mut().spawn((Pane::bundle(), ChildOf(tab))).id();
        let agent = app
            .world_mut()
            .spawn((
                Stack::bundle(),
                LastActivatedAt(10),
                ChildOf(pane),
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: ProcessId::new(),
                    resume: None,
                },
            ))
            .id();
        app.world_mut().entity_mut(agent).insert(PageMetadata {
            url: "vmux://sessions/claude".into(),
            ..default()
        });
        app.world_mut().write_message(UiAgentAcpTerminalCreated {
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
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AgentRuntimePlugin);
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
