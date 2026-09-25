use bevy::prelude::*;
use crossbeam_channel::Receiver;
use vmux_core::LastActivatedAt;
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_layout::pane::{PlacementCtx, resolve_spiral_pane};
use vmux_layout::stack::stack_bundle;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ClientMessage, SharedMessage};
use vmux_terminal::reattach_terminal_bundle;

use crate::events::AgentApprovalRequest;
use crate::handoff::{ImportedConversation, PendingHandoff};
use crate::run_state::AgentRunState;
use vmux_session::{AcpSession, AgentApprovalPolicy, PromptQueue};

pub struct AcpAgentPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AcpModelInfoSet;

impl Plugin for AcpAgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::acp_tool::AcpToolPlugin)
            .init_resource::<AcpCatalog>()
            .add_message::<vmux_service::agent_events::PageAgentInfo>()
            .add_message::<vmux_service::agent_events::PageAgentWorkspaceChanged>()
            .add_message::<vmux_service::agent_events::PageAgentModelInfo>()
            .add_message::<vmux_service::agent_events::PageAgentModelSelectionResult>()
            .add_message::<vmux_service::agent_events::PageAgentModeInfo>()
            .add_message::<vmux_service::agent_events::PageAgentModeSelectionResult>()
            .add_message::<vmux_service::agent_events::PageAgentSessionCreated>()
            .add_message::<vmux_service::agent_events::PageAgentAcpTerminalCreated>()
            .add_systems(Startup, start_catalog_fetch)
            .add_systems(
                Update,
                (
                    send_acp_input,
                    receive_catalog,
                    apply_acp_agent_info,
                    apply_acp_workspace_changed,
                    (
                        apply_acp_model_info.in_set(AcpModelInfoSet),
                        apply_acp_model_selection_result,
                        apply_acp_mode_info,
                        apply_acp_mode_selection_result,
                    )
                        .chain(),
                    apply_acp_session_created,
                    apply_acp_terminal_created,
                ),
            )
            .add_observer(close_acp_session_on_remove)
            .add_observer(auto_allow_acp_approval);
    }
}

const UNBOUND_WORKSPACE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): This tab starts in ~/.vmux/projects and has no selected project. Before accessing project files or running project commands, call select_project with the known project path or without a path to open the picker. Paths inside ~/.vmux/projects are selected immediately; paths outside it require explicit user approval in the native picker. For a new project, do not ask the user to invent a folder location. First call request_user_choice with two concrete options: create the project at a suggested path under ~/.vmux/projects, or choose an existing project. Use ~/.vmux/projects/<remote-host>/<organization>/<repository> when a remote is known and ~/.vmux/projects/local/<project> otherwise. If the user chooses creation, use run only to create the empty directory, then call select_project with that path. vmux will offer Git initialization and use the new project root directly; never call create_worktree for that new project. Do not search the user's home directory. General questions and self-contained terminal demonstrations may use the current directory without selecting a project.";
const PENDING_WORKTREE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): Project activation is pending. Do not access project paths directly or run git worktree add yourself. Wait for vmux to finish preparing the selected project before inspecting, editing, testing, or running it.";
const REPOSITORY_WORKTREE_CONTEXT: &str = "VMUX HOST POLICY (mandatory): The selected project is a Git repository, but this tab is not isolated. Reading and inspection are allowed without a worktree. Never call create_worktree for requests that only read, show, search, or explain existing files. Immediately before the first edit, write, test, build, or other mutation, call create_worktree. It reuses a known linked worktree, automatically uses one unambiguous existing worktree, or creates one when none exists. If it reports multiple candidates, ask the user with request_user_choice to choose an existing path or Create new worktree, then call create_worktree again with path or create=true. Never run git worktree add yourself.";

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
    tabs: &Query<&vmux_layout::tab::Tab>,
    workspaces: &Query<(), With<vmux_layout::tab::TabWorkspace>>,
    pending_projects: &Query<(), With<crate::host::PendingAgentProject>>,
    repositories_needing_worktrees: &Query<(), With<crate::host::RepositoryNeedsWorktree>>,
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
    handoff: Option<String>,
    workspace_state: Option<AcpWorkspaceState>,
) -> Option<String> {
    let policy = match workspace_state {
        Some(AcpWorkspaceState::Unbound) => Some(UNBOUND_WORKSPACE_CONTEXT),
        Some(AcpWorkspaceState::PendingWorktree) => Some(PENDING_WORKTREE_CONTEXT),
        Some(AcpWorkspaceState::RepositoryNeedsWorktree) => Some(REPOSITORY_WORKTREE_CONTEXT),
        Some(AcpWorkspaceState::Bound) | None => None,
    };
    match (handoff, policy) {
        (Some(handoff), Some(policy)) => Some(format!("{handoff}\n\n{policy}")),
        (Some(handoff), None) => Some(handoff),
        (None, Some(policy)) => Some(policy.to_string()),
        (None, None) => None,
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AcpModelState {
    pub config_id: String,
    pub current_model_id: String,
    pub default_model_id: String,
    pub(crate) pending: Option<PendingAcpModelSelection>,
    pub models: Vec<vmux_service::protocol::AcpModelOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingAcpModelSelection {
    pub request_id: u64,
    pub model_id: String,
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AcpModeState {
    pub config_id: String,
    pub current_mode_id: String,
    pub(crate) pending: Option<PendingAcpModeSelection>,
    pub modes: Vec<vmux_service::protocol::AcpModeOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingAcpModeSelection {
    pub request_id: u64,
    pub mode_id: String,
}

impl AcpModeState {
    pub fn display_mode_id(&self) -> &str {
        self.pending
            .as_ref()
            .map(|pending| pending.mode_id.as_str())
            .unwrap_or(&self.current_mode_id)
    }

    pub fn current_name(&self) -> &str {
        self.modes
            .iter()
            .find(|mode| mode.id == self.display_mode_id())
            .map(|mode| mode.name.as_str())
            .unwrap_or_else(|| self.display_mode_id())
    }
}

impl AcpModelState {
    pub fn display_model_id(&self) -> &str {
        self.pending
            .as_ref()
            .map(|pending| pending.model_id.as_str())
            .unwrap_or(&self.current_model_id)
    }

    pub fn current_name(&self) -> &str {
        self.models
            .iter()
            .find(|model| model.id == self.display_model_id())
            .map(|model| model.name.as_str())
            .unwrap_or_else(|| self.display_model_id())
    }
}

#[derive(Resource, Default)]
pub struct AcpCatalog {
    pub agents: Vec<crate::acp_registry::RegistryAgent>,
}

#[derive(Component)]
struct AcpCatalogFetch {
    rx: Receiver<Vec<crate::acp_registry::RegistryAgent>>,
}

fn start_catalog_fetch(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        let agents = crate::acp_registry::fetch_blocking()
            .ok()
            .or_else(crate::acp_registry::load_cached)
            .map(|r| r.agents)
            .unwrap_or_default();
        let _ = tx.send(agents);
    });
    commands.spawn(AcpCatalogFetch { rx });
}

fn receive_catalog(
    fetches: Query<(Entity, &AcpCatalogFetch)>,
    mut catalog: ResMut<AcpCatalog>,
    mut commands: Commands,
) {
    for (entity, fetch) in &fetches {
        let Ok(agents) = fetch.rx.try_recv() else {
            continue;
        };
        catalog.agents = agents;
        commands.entity(entity).despawn();
    }
}

fn apply_acp_agent_info(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentInfo>,
    mut sessions: Query<(&AcpSession, &mut vmux_core::team::Profile)>,
) {
    for event in reader.read() {
        let name = event.name.trim();
        if name.is_empty() {
            continue;
        }
        for (session, mut profile) in &mut sessions {
            if session.sid == event.sid && profile.name != name {
                *profile = vmux_core::team::Profile::registry(name, &session.agent_id);
            }
        }
    }
}

fn validate_acp_workspace(
    event: &vmux_service::agent_events::PageAgentWorkspaceChanged,
) -> Result<vmux_git::worktree::ValidatedLinkedWorkspace, String> {
    vmux_git::worktree::validate_linked_workspace(
        std::path::Path::new(&event.cwd),
        std::path::Path::new(&event.workspace_cwd),
        &event.branch,
    )
}

fn ancestor_tab(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(), With<vmux_layout::tab::Tab>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn apply_acp_workspace_changed(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentWorkspaceChanged>,
    mut sessions: Query<(Entity, &mut AcpSession)>,
    child_of: Query<&ChildOf>,
    tab_entities: Query<(), With<vmux_layout::tab::Tab>>,
    mut tabs: Query<&mut vmux_layout::tab::Tab>,
    mut workspaces: Query<&mut vmux_layout::tab::TabWorkspace>,
    managed: Query<&vmux_layout::tab::TabWorktree>,
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
                commands
                    .entity(tab_entity)
                    .insert(vmux_layout::tab::TabWorkspace {
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
                .insert(vmux_layout::tab::TabDirDecided)
                .remove::<vmux_layout::tab::TabWorktreeUnavailable>();
            if !keeps_managed {
                entity
                    .remove::<vmux_layout::tab::TabWorktree>()
                    .remove::<vmux_layout::worktree::TabWorktreeReady>();
            } else if let Ok(ready) = vmux_layout::worktree::TabWorktreeReady::new(
                &cwd,
                &workspace_project_dir,
                managed.get(tab_entity).unwrap(),
                &checkout,
            ) {
                entity.insert(ready);
            } else {
                entity.remove::<vmux_layout::worktree::TabWorktreeReady>();
            }
        }
    }
}

fn apply_acp_model_info(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentModelInfo>,
    mut sessions: Query<(Entity, &AcpSession, Option<&mut AcpModelState>)>,
    mut commands: Commands,
) {
    for event in reader.read() {
        for (entity, session, current) in &mut sessions {
            if session.sid != event.sid {
                continue;
            }
            if event.config_id.is_empty() || event.models.is_empty() {
                if current.is_some() {
                    commands.entity(entity).remove::<AcpModelState>();
                }
                continue;
            }
            if let Some(mut current) = current {
                let pending = current.pending.take();
                let default_model_id = match current.default_model_id.is_empty() {
                    true => event.current_model_id.clone(),
                    false => current.default_model_id.clone(),
                };
                *current = AcpModelState {
                    config_id: event.config_id.clone(),
                    current_model_id: event.current_model_id.clone(),
                    default_model_id,
                    pending,
                    models: event.models.clone(),
                };
            } else {
                commands.entity(entity).insert(AcpModelState {
                    config_id: event.config_id.clone(),
                    current_model_id: event.current_model_id.clone(),
                    default_model_id: event.current_model_id.clone(),
                    pending: None,
                    models: event.models.clone(),
                });
            }
        }
    }
}

fn apply_acp_model_selection_result(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentModelSelectionResult>,
    mut sessions: Query<(&AcpSession, &mut AcpModelState)>,
) {
    for event in reader.read() {
        for (session, mut state) in &mut sessions {
            if session.sid == event.sid
                && state.pending.as_ref().is_some_and(|pending| {
                    pending.request_id == event.request_id && pending.model_id == event.model_id
                })
            {
                if event.succeeded {
                    state.current_model_id.clone_from(&event.model_id);
                }
                state.pending = None;
            }
        }
    }
}

fn apply_acp_mode_info(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentModeInfo>,
    mut sessions: Query<(Entity, &AcpSession, Option<&mut AcpModeState>)>,
    mut commands: Commands,
) {
    for event in reader.read() {
        for (entity, session, current) in &mut sessions {
            if session.sid != event.sid {
                continue;
            }
            if event.modes.is_empty() {
                if current.is_some() {
                    commands.entity(entity).remove::<AcpModeState>();
                }
                continue;
            }
            if let Some(mut current) = current {
                let pending = current.pending.take();
                *current = AcpModeState {
                    config_id: event.config_id.clone(),
                    current_mode_id: event.current_mode_id.clone(),
                    pending,
                    modes: event.modes.clone(),
                };
            } else {
                commands.entity(entity).insert(AcpModeState {
                    config_id: event.config_id.clone(),
                    current_mode_id: event.current_mode_id.clone(),
                    pending: None,
                    modes: event.modes.clone(),
                });
            }
        }
    }
}

fn apply_acp_mode_selection_result(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentModeSelectionResult>,
    mut sessions: Query<(&AcpSession, &mut AcpModeState)>,
) {
    for event in reader.read() {
        for (session, mut state) in &mut sessions {
            if session.sid == event.sid
                && state.pending.as_ref().is_some_and(|pending| {
                    pending.request_id == event.request_id && pending.mode_id == event.mode_id
                })
            {
                if event.succeeded {
                    state.current_mode_id.clone_from(&event.mode_id);
                }
                state.pending = None;
            }
        }
    }
}

fn acp_auto_approval_message(
    session: &AcpSession,
    policy: &AgentApprovalPolicy,
    request: &AgentApprovalRequest,
) -> Option<ClientMessage> {
    policy.allows(&request.name).then(|| {
        ClientMessage::Shared(SharedMessage::agent(
            session.sid.clone(),
            vmux_service::protocol::AgentRequest::Approve {
                call_id: request.call_id.clone(),
                decision: vmux_service::protocol::ApprovalDecision::AllowAlways,
            },
        ))
    })
}

fn auto_allow_acp_approval(
    trigger: On<AgentApprovalRequest>,
    sessions: Query<(&AcpSession, &AgentApprovalPolicy)>,
    service: Option<Res<ServiceClient>>,
) {
    let request = trigger.event();
    let Ok((session, policy)) = sessions.get(request.session) else {
        return;
    };
    let Some(message) = acp_auto_approval_message(session, policy, request) else {
        return;
    };
    let Some(service) = service else {
        warn!(sid = %session.sid, call_id = %request.call_id, "auto-approval waiting for service connection");
        return;
    };
    service.0.send(message);
}

#[allow(clippy::type_complexity)]
fn apply_acp_session_created(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentSessionCreated>,
    mut sessions: Query<
        (
            Entity,
            &mut AcpSession,
            &mut vmux_core::PageMetadata,
            Option<&ImportedConversation>,
        ),
        Without<vmux_layout::Browser>,
    >,
    children: Query<&Children>,
    mut browser_meta: Query<&mut vmux_core::PageMetadata, With<vmux_layout::Browser>>,
) {
    for ev in reader.read() {
        for (stack, mut session, mut stack_meta, imported) in &mut sessions {
            if session.sid != ev.sid {
                continue;
            }
            session.resume = Some(ev.acp_session_id.clone());
            if let Some(imported) = imported
                && imported.first_prompt.is_some()
                && let Err(err) =
                    crate::handoff::save(&session.agent_id, &ev.acp_session_id, imported)
            {
                bevy::log::warn!("acp: failed to persist handoff metadata: {err}");
            }
            let url = format!("vmux://sessions/{}/{}", session.agent_id, ev.acp_session_id);
            if stack_meta.url != url {
                stack_meta.url = url.clone();
            }
            if let Ok(kids) = children.get(stack) {
                for kid in kids.iter() {
                    if let Ok(mut meta) = browser_meta.get_mut(kid)
                        && meta.url != url
                    {
                        meta.url = url.clone();
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_acp_terminal_created(
    mut reader: MessageReader<vmux_service::agent_events::PageAgentAcpTerminalCreated>,
    sessions: Query<(Entity, &AcpSession)>,
    ctx: PlacementCtx,
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
        let target_pane = resolve_spiral_pane(
            &mut commands,
            agent_pane,
            TERMINAL_PAGE_URL,
            false,
            &mut split_batch,
            &ctx,
        );
        let tab = commands
            .spawn((stack_bundle(), LastActivatedAt(0), ChildOf(target_pane)))
            .id();
        commands.spawn((
            reattach_terminal_bundle(ev.process_id),
            vmux_terminal::RetainOnProcessExit,
            ChildOf(tab),
        ));
    }
}

fn send_acp_input(
    mut q: Query<(
        Entity,
        &AcpSession,
        &mut AgentRunState,
        &mut PromptQueue,
        Has<crate::acp_tool::AcpLaunchStarted>,
        Option<&mut PendingHandoff>,
        Option<&mut ImportedConversation>,
    )>,
    child_of: Query<&ChildOf>,
    tabs: Query<&vmux_layout::tab::Tab>,
    workspaces: Query<(), With<vmux_layout::tab::TabWorkspace>>,
    pending_projects: Query<(), With<crate::host::PendingAgentProject>>,
    repositories_needing_worktrees: Query<(), With<crate::host::RepositoryNeedsWorktree>>,
    modes: Option<Res<crate::chat::model::AgentModeSelections>>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
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
        let handoff = pending
            .as_deref_mut()
            .and_then(PendingHandoff::context_for_send);
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
        let context = acp_prompt_context(handoff, workspace_state);
        let preferred_mode = modes
            .as_deref()
            .map(|modes| modes.selected_for(&session.agent_id).to_string())
            .filter(|mode| !mode.is_empty());
        service.0.send(ClientMessage::agent_input_with_mode(
            session.sid.clone(),
            text,
            context,
            prompt.attachments,
            preferred_mode,
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

fn close_acp_session_on_remove(
    trigger: On<Remove, AcpSession>,
    sessions: Query<&AcpSession>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    let Ok(session) = sessions.get(trigger.event_target()) else {
        return;
    };
    service.0.send(ClientMessage::ClosePageAgent {
        sid: session.sid.clone(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
                request: vmux_service::protocol::AgentRequest::Approve { call_id, decision },
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
    fn catalog_fetch_entity_is_consumed_after_delivery() {
        let mut app = App::new();
        app.init_resource::<AcpCatalog>()
            .add_systems(Update, receive_catalog);
        let (tx, rx) = crossbeam_channel::unbounded();
        let fetch = app.world_mut().spawn(AcpCatalogFetch { rx }).id();
        tx.send(vec![crate::acp_registry::RegistryAgent {
            id: "agent".into(),
            name: "Agent".into(),
            version: None,
            description: None,
            icon: None,
            repository: None,
            distribution: crate::acp_registry::Distribution::default(),
        }])
        .unwrap();

        app.update();

        assert!(app.world().get_entity(fetch).is_err());
        assert_eq!(app.world().resource::<AcpCatalog>().agents[0].id, "agent");
    }

    #[test]
    fn unbound_workspace_context_requires_project_selection_before_file_access() {
        let context = acp_prompt_context(None, Some(AcpWorkspaceState::Unbound)).unwrap();

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
                          pending: Query<(), With<crate::host::PendingAgentProject>>,
                          needs_worktree: Query<
                        (),
                        With<crate::host::RepositoryNeedsWorktree>,
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
            .insert(crate::host::PendingAgentProject("/repo".into()));
        assert_eq!(
            state(app.world_mut()),
            Some(AcpWorkspaceState::PendingWorktree)
        );
        app.world_mut().entity_mut(tab).insert((
            vmux_layout::tab::Tab {
                name: "Tab 1".into(),
                startup_dir: Some("/repo".into()),
            },
            crate::host::RepositoryNeedsWorktree,
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
            .remove::<crate::host::RepositoryNeedsWorktree>();
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
    fn live_acp_identity_updates_only_matching_profile() {
        use vmux_core::team::Profile;
        use vmux_service::agent_events::PageAgentInfo;

        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin);
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
            .add_plugins(AcpAgentPlugin);
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
                    default_model_id: "default".into(),
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
    fn mode_results_preserve_latest_pending_selection() {
        use vmux_service::agent_events::{PageAgentModeInfo, PageAgentModeSelectionResult};
        use vmux_service::protocol::AcpModeOption;

        let modes = vec![
            AcpModeOption {
                id: "ask".into(),
                name: "Ask".into(),
                description: None,
            },
            AcpModeOption {
                id: "auto".into(),
                name: "Auto Allow".into(),
                description: None,
            },
        ];
        let mut app = App::new();
        app.add_message::<PageAgentModeInfo>()
            .add_message::<PageAgentModeSelectionResult>()
            .add_systems(
                Update,
                (apply_acp_mode_info, apply_acp_mode_selection_result).chain(),
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
                AcpModeState {
                    config_id: String::new(),
                    current_mode_id: "ask".into(),
                    pending: Some(PendingAcpModeSelection {
                        request_id: 2,
                        mode_id: "auto".into(),
                    }),
                    modes: modes.clone(),
                },
            ))
            .id();

        app.world_mut().write_message(PageAgentModeInfo {
            sid: "s1".into(),
            config_id: String::new(),
            current_mode_id: "ask".into(),
            modes,
        });
        app.world_mut().write_message(PageAgentModeSelectionResult {
            sid: "s1".into(),
            request_id: 1,
            mode_id: "auto".into(),
            succeeded: false,
        });
        app.update();

        let state = app.world().get::<AcpModeState>(entity).unwrap();
        assert_eq!(state.display_mode_id(), "auto");
        assert_eq!(
            state.pending.as_ref().map(|pending| pending.request_id),
            Some(2)
        );

        app.world_mut().write_message(PageAgentModeSelectionResult {
            sid: "s1".into(),
            request_id: 2,
            mode_id: "auto".into(),
            succeeded: true,
        });
        app.update();

        let state = app.world().get::<AcpModeState>(entity).unwrap();
        assert_eq!(state.current_mode_id, "auto");
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
            .add_systems(Update, apply_acp_terminal_created);
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
                url: "vmux://sessions/claude".into(),
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
    fn plugin_builds_and_runs_without_panic() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .add_plugins(AcpAgentPlugin);
        app.world_mut().spawn(AcpSession {
            agent_id: "vibe-acp".to_string(),
            sid: "s1".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            anchor: vmux_core::ProcessId::new(),
            resume: None,
        });
        app.update();
    }
}
