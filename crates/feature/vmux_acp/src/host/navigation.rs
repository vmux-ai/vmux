use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::persistence::PageRestore;
use vmux_ecs::profile::Projects;
use vmux_ecs::terminal::TerminalLaunch;
use vmux_ecs::{
    Cwd, EntityTarget, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled, PageOpenSet,
    PageOpenTask, PendingPrompt, PendingPromptAttachments, UiState, UiStateWriter,
};
use vmux_layout::space::FocusedSpace;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::{PageOpenWaitForWorktree, TabWorktreePending, TabWorktreeReady};
use vmux_session::event::{ChatSnapshot, ChatTranscriptState};
use vmux_session::host::{ImportedConversation, SessionManagerView, SessionView};
use vmux_session::state::{ChatUiState, SessionDetailUiState};
use vmux_session::{AgentId, Cleanup, PromptQueue, Route, Session, SessionId};
use vmux_setting::AppSettings;
use vmux_start::{StartInlineTransition, StartInlineTransitionView};
use vmux_ui::i18n::translate;

use super::attach::AcpAgentAttachment;
use super::navigation_driver::{
    AcpCatalog, AgentPageOpenWorkspace, PageOpenCompletion, PageOpener,
};
use crate::AcpSessionId;
#[cfg(test)]
use crate::host::acp::registry::RegistryAgent;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type ReadyPageOpen = (
    Without<PageOpenHandled>,
    Without<PageOpenError>,
    Without<PageOpenDeferred>,
);

pub(super) fn add(app: &mut App) {
    app.init_resource::<vmux_git::GitRuntime>()
        .add_observer(cleanup)
        .add_systems(Update, swap)
        .add_systems(
            Update,
            (
                poll_workspace_checks,
                start_workspace_checks,
                release_transition,
                discard_stale_inline_transitions,
                prepare,
                open,
            )
                .chain()
                .in_set(PageOpenSet::HandleKnownPages)
                .after(vmux_layout::warm_page::PrewarmPageOpenSet),
        )
        .add_systems(
            Update,
            discard_stale_inline_transitions.after(PageOpenSet::Respond),
        );
}

#[derive(Component)]
pub(super) struct PreparingAgentChatView;

#[derive(Component)]
struct AwaitingAgentTransitionPaint;

#[derive(Component, Clone)]
pub(super) struct InlineTransitionSnapshot {
    metadata: Option<PageMetadata>,
    session_view: bool,
    session_manager_view: bool,
    transition_view: bool,
    hosts_launcher: bool,
    page_ready: bool,
    chat_state: Option<Option<ChatUiState>>,
    detail_state: Option<Option<SessionDetailUiState>>,
}

#[derive(Component, Clone)]
struct WorkspaceCheckRequest {
    tab: Entity,
    project_dir: String,
    startup_dir: Option<String>,
}

impl WorkspaceCheckRequest {
    fn needs_worktree(&self) -> bool {
        let linked = self
            .startup_dir
            .as_deref()
            .map(Path::new)
            .and_then(|path| path.canonicalize().ok())
            .as_deref()
            .is_some_and(vmux_git::worktree::CheckoutInfo::is_linked);
        !linked && vmux_git::worktree::CheckoutInfo::try_from(Path::new(&self.project_dir)).is_ok()
    }
}

#[derive(Component)]
struct WorkspaceCheckTask {
    request: WorkspaceCheckRequest,
    task: vmux_git::GitTask<bool>,
}

struct AgentChatTarget {
    url: String,
    title: String,
}

impl AgentChatTarget {
    fn parse(url: &str) -> Option<Self> {
        match Route::parse(url)? {
            Route::Manager => Some(Self {
                url: Route::requested_agent(url)
                    .map(|agent| Route::manager_for_agent(&agent))
                    .unwrap_or_else(|| Route::Manager.url()),
                title: translate("sessions-title"),
            }),
            Route::Session(id) => Some(Self {
                url: Route::Session(id.clone()).url(),
                title: id.0,
            }),
        }
    }
}

fn ancestor_tab_entity(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(
        Entity,
        &mut Tab,
        Option<&TabWorkspace>,
        Option<&TabWorktree>,
        Option<&TabWorktreeReady>,
        Option<&TabDirDecided>,
    )>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn prepare(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    pending: Query<(), With<TabWorktreePending>>,
    transitions: Query<&StartInlineTransition>,
    preparing_views: Query<(Entity, &ChildOf), With<PreparingAgentChatView>>,
    manager_views: Query<(Entity, &ChildOf), With<SessionManagerView>>,
    view_states: Query<(
        Option<&PageMetadata>,
        Has<SessionView>,
        Has<SessionManagerView>,
        Has<StartInlineTransitionView>,
        Has<vmux_ecs::launcher::HostsLauncher>,
        Has<vmux_ecs::page::PageReady>,
        Option<&UiState<ChatUiState>>,
        Option<&UiState<SessionDetailUiState>>,
        Has<InlineTransitionSnapshot>,
    )>,
    workspace_checks: Query<(), Or<(With<WorkspaceCheckRequest>, With<WorkspaceCheckTask>)>>,
    child_of: Query<&ChildOf>,
    space_hierarchy: vmux_layout::space::SpaceHierarchy,
    mut tabs: Query<(
        Entity,
        &mut Tab,
        Option<&TabWorkspace>,
        Option<&TabWorktree>,
        Option<&TabWorktreeReady>,
        Option<&TabDirDecided>,
    )>,
    settings: Option<Res<AppSettings>>,
    active_space: FocusedSpace,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let mut preparing_by_stack: std::collections::HashMap<Entity, Entity> = preparing_views
        .iter()
        .map(|(entity, child_of)| (child_of.parent(), entity))
        .collect();
    let manager_by_stack = manager_views
        .iter()
        .map(|(entity, child_of)| (child_of.parent(), entity))
        .collect::<std::collections::HashMap<_, _>>();
    let mut opened_stacks = std::collections::HashSet::new();
    for (task_entity, task) in &tasks {
        let Some(route) = Route::parse(&task.url) else {
            continue;
        };
        let transition = transitions.get(task.stack).ok();
        let transition_view = transition.map(|transition| transition.webview).or_else(|| {
            matches!(route, Route::Session(_))
                .then(|| manager_by_stack.get(&task.stack).copied())
                .flatten()
        });
        let transition_is_stale = transition
            .and_then(|transition| transition.navigation)
            .is_some_and(|navigation| navigation != task.id);
        if (!preparing_by_stack.contains_key(&task.stack) || transition_is_stale)
            && let Some(view) = transition_view
            && let Some(target) = AgentChatTarget::parse(&task.url)
        {
            if let Ok((
                metadata,
                session_view,
                session_manager_view,
                transition_view,
                hosts_launcher,
                page_ready,
                chat_state,
                detail_state,
                snapshot,
            )) = view_states.get(view)
                && !snapshot
            {
                commands.entity(view).insert(InlineTransitionSnapshot {
                    metadata: metadata.cloned(),
                    session_view,
                    session_manager_view,
                    transition_view,
                    hosts_launcher,
                    page_ready,
                    chat_state: chat_state.map(|state| state.current().cloned()),
                    detail_state: detail_state.map(|state| state.current().cloned()),
                });
            }
            commands
                .entity(view)
                .insert((
                    PageMetadata {
                        url: target.url,
                        title: target.title,
                        bg_color: None,
                        ..default()
                    },
                    SessionView,
                    PreparingAgentChatView,
                ))
                .remove::<(
                    SessionManagerView,
                    StartInlineTransitionView,
                    vmux_ecs::launcher::HostsLauncher,
                    vmux_ecs::page::PageReady,
                )>();
            commands.trigger(vmux_ecs::UiStateWrite::<ChatUiState>::from_event(
                view,
                &ChatSnapshot {
                    transcript_loading: true,
                    ..Default::default()
                },
            ));
            commands.trigger(vmux_ecs::UiStateWrite::<ChatUiState>::from_event(
                view,
                &ChatTranscriptState::default(),
            ));
            commands.trigger(vmux_ecs::UiStateWrite::<SessionDetailUiState>::from_event(
                view,
                &SessionDetailUiState::default(),
            ));
            commands.entity(task.stack).insert(StartInlineTransition {
                webview: view,
                navigation: Some(task.id),
            });
            preparing_by_stack.insert(task.stack, view);
            if let Some(wake) = wake.as_ref() {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            opened_stacks.insert(task.stack);
        }
        if opened_stacks.contains(&task.stack) {
            commands
                .entity(task_entity)
                .insert((PageOpenDeferred, AwaitingAgentTransitionPaint));
            continue;
        }
        if matches!(route, Route::Manager) {
            continue;
        }
        if workspace_checks.contains(task_entity) {
            continue;
        }
        let Some(tab_entity) = ancestor_tab_entity(task.stack, &child_of, &tabs) else {
            continue;
        };
        if pending.contains(tab_entity) {
            commands.entity(task_entity).insert((
                PageOpenDeferred,
                PageOpenWaitForWorktree { tab: tab_entity },
            ));
            continue;
        }
        let configured_project_dir = settings.as_deref().and_then(|settings| {
            let space_id = space_hierarchy
                .id(task.stack)
                .or_else(|| active_space.id().map(str::to_string))?;
            vmux_setting::StartupDir::resolve(settings, &space_id, None)
                .map(|dir| dir.path.to_string_lossy().into_owned())
        });
        let Ok((_, mut tab, workspace, metadata, ready, decided)) = tabs.get_mut(tab_entity) else {
            continue;
        };
        let has_workspace = workspace.is_some();
        let workspace = workspace.cloned().unwrap_or_else(|| {
            let project_dir = metadata
                .map(|metadata| metadata.repo_root.clone())
                .filter(|path| !path.is_empty())
                .or_else(|| tab.startup_dir.clone())
                .or_else(|| configured_project_dir.clone())
                .unwrap_or_default();
            TabWorkspace { project_dir }
        });
        if workspace.project_dir.is_empty() {
            continue;
        }
        if metadata.is_none()
            && vmux_space::WorkspaceCwd::from_tab(tab.startup_dir.as_deref())
                .ok()
                .flatten()
                .is_none()
            && vmux_space::WorkspaceCwd::from_tab(Some(&workspace.project_dir))
                .ok()
                .flatten()
                .is_none()
        {
            tab.startup_dir = None;
            commands
                .entity(tab_entity)
                .remove::<TabWorkspace>()
                .remove::<TabDirDecided>()
                .remove::<TabWorktreeUnavailable>()
                .remove::<vmux_space::RepositoryNeedsWorktree>();
            continue;
        }
        if !has_workspace {
            commands.entity(tab_entity).insert(workspace.clone());
        }
        if let Some(metadata) = metadata {
            commands
                .entity(tab_entity)
                .remove::<vmux_space::RepositoryNeedsWorktree>();
            if ready.is_some_and(|ready| ready.is_current(&tab, &workspace, metadata)) {
                commands
                    .entity(tab_entity)
                    .remove::<TabWorktreeUnavailable>();
                continue;
            }
            commands
                .entity(tab_entity)
                .remove::<(TabWorktreeReady, TabWorktreeUnavailable)>()
                .insert(TabWorktreePending);
            commands.entity(task_entity).insert((
                PageOpenDeferred,
                PageOpenWaitForWorktree { tab: tab_entity },
            ));
        } else {
            if decided.is_none() {
                commands.entity(task_entity).insert((
                    PageOpenDeferred,
                    WorkspaceCheckRequest {
                        tab: tab_entity,
                        project_dir: workspace.project_dir,
                        startup_dir: tab.startup_dir.clone(),
                    },
                ));
            }
        }
    }
}

fn start_workspace_checks(
    requests: Query<(Entity, &WorkspaceCheckRequest), Without<WorkspaceCheckTask>>,
    runtime: Res<vmux_git::GitRuntime>,
    wake: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, request) in &requests {
        let worker_request = request.clone();
        let Some(task) = runtime.spawn(wake.clone(), move || worker_request.needs_worktree())
        else {
            break;
        };
        commands.entity(entity).insert(WorkspaceCheckTask {
            request: request.clone(),
            task,
        });
    }
}

fn poll_workspace_checks(
    mut tasks: Query<(Entity, &mut WorkspaceCheckTask)>,
    tabs: Query<(&Tab, &TabWorkspace, Has<TabWorktree>)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(output) = task.task.poll() else {
            continue;
        };
        let current = tabs
            .get(task.request.tab)
            .is_ok_and(|(tab, workspace, worktree)| {
                !worktree
                    && workspace.project_dir == task.request.project_dir
                    && tab.startup_dir == task.request.startup_dir
            });
        let mut request = commands.entity(entity);
        request.remove::<(WorkspaceCheckRequest, WorkspaceCheckTask, PageOpenDeferred)>();
        if !current {
            continue;
        }
        let needs_worktree = output.unwrap_or(false);
        let mut tab = commands.entity(task.request.tab);
        tab.insert(TabDirDecided);
        if needs_worktree {
            tab.insert(vmux_space::RepositoryNeedsWorktree);
        } else {
            tab.remove::<vmux_space::RepositoryNeedsWorktree>();
        }
    }
}

fn release_transition(
    waiting: Query<Entity, With<AwaitingAgentTransitionPaint>>,
    mut commands: Commands,
) {
    for entity in &waiting {
        commands
            .entity(entity)
            .remove::<(PageOpenDeferred, AwaitingAgentTransitionPaint)>();
    }
}

fn open(
    mut open_q: ParamSet<(
        Query<(Entity, &PageOpenTask, Has<PageRestore>), ReadyPageOpen>,
        Query<(&PendingPrompt, Option<&PendingPromptAttachments>)>,
    )>,
    workspace: AgentPageOpenWorkspace,
    mut opener: PageOpener,
    transitions: Query<&StartInlineTransition>,
    launches: Query<&TerminalLaunch>,
    projects: Projects,
    navigations: Query<&vmux_ecs::PageNavigation>,
) {
    let tasks: Vec<(Entity, PageOpenTask, bool)> = open_q
        .p0()
        .iter()
        .map(|(entity, task, restoring)| (entity, task.clone(), restoring))
        .collect();
    for (entity, task, restoring) in tasks {
        if !navigations
            .get(task.stack)
            .is_ok_and(|navigation| navigation.matches(task.id))
        {
            continue;
        }
        let Some(route) = Route::parse(&task.url) else {
            continue;
        };
        let default_cwd = if matches!(route, Route::Manager) {
            PathBuf::new()
        } else {
            let tab = opener.tab(task.stack);
            let tab_dir = tab
                .as_ref()
                .and_then(|(_, startup_dir)| startup_dir.clone());
            let space_startup_dir = workspace.startup_dir(task.stack, &opener.settings);
            let restored_cwd = restoring
                .then(|| launches.get(task.stack).ok())
                .flatten()
                .map(|launch| PathBuf::from(&launch.cwd));
            if let Some(cwd) = restored_cwd {
                cwd
            } else {
                match vmux_space::WorkspaceCwd::from_tab(tab_dir.as_deref()) {
                    Ok(Some(path)) => path,
                    Ok(None) => match space_startup_dir {
                        Some(dir) => dir.path,
                        None => match projects.path() {
                            Ok(path) => path.to_path_buf(),
                            Err(message) => {
                                opener
                                    .commands
                                    .entity(entity)
                                    .insert(PageOpenError { message });
                                continue;
                            }
                        },
                    },
                    Err(message) => {
                        opener
                            .commands
                            .entity(entity)
                            .insert(PageOpenError { message });
                        continue;
                    }
                }
            }
        };
        let (initial_prompt, initial_attachments) = open_q
            .p1()
            .get(task.stack)
            .map(|(prompt, attachments)| {
                (
                    Some(prompt.0.clone()),
                    attachments
                        .map(|attachments| attachments.0.clone())
                        .unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        let transition_webview = transitions
            .get(task.stack)
            .ok()
            .map(|transition| transition.webview)
            .filter(|_| {
                vmux_api::VmuxRoute::parse(&task.url)
                    .is_some_and(|route| route.supports_inline_transition())
            });
        match opener.apply(
            entity,
            &task,
            initial_prompt,
            initial_attachments,
            transition_webview,
            &default_cwd,
        ) {
            Ok(completion) => match completion {
                PageOpenCompletion::Complete => {
                    opener.commands.entity(entity).insert(PageOpenHandled);
                    if let Some(webview) = transition_webview {
                        opener
                            .commands
                            .entity(webview)
                            .remove::<(PreparingAgentChatView, InlineTransitionSnapshot)>();
                    }
                    opener
                        .commands
                        .entity(task.stack)
                        .remove::<StartInlineTransition>();
                }
                PageOpenCompletion::PendingAttachment => {
                    opener.commands.entity(entity).insert(PageOpenDeferred);
                }
            },
            Err(message) => {
                opener
                    .commands
                    .entity(entity)
                    .insert(PageOpenError { message });
            }
        }
    }
}

fn discard_stale_inline_transitions(
    transitions: Query<(Entity, &StartInlineTransition)>,
    navigations: Query<&vmux_ecs::PageNavigation>,
    snapshots: Query<&InlineTransitionSnapshot>,
    mut chat_states: UiStateWriter<ChatUiState>,
    mut detail_states: UiStateWriter<SessionDetailUiState>,
    mut commands: Commands,
) {
    for (stack, transition) in &transitions {
        let Some(navigation) = transition.navigation else {
            continue;
        };
        if navigations
            .get(stack)
            .is_ok_and(|current| current.matches(navigation))
        {
            continue;
        }
        commands.entity(stack).remove::<StartInlineTransition>();
        let Ok(snapshot) = snapshots.get(transition.webview) else {
            if let Ok(mut view) = commands.get_entity(transition.webview) {
                view.remove::<(PreparingAgentChatView, StartInlineTransitionView)>();
            }
            continue;
        };
        let snapshot = snapshot.clone();
        match snapshot.chat_state.clone() {
            Some(state) => {
                chat_states.restore(transition.webview, snapshot.metadata.as_ref(), state)
            }
            None => {
                commands
                    .entity(transition.webview)
                    .remove::<UiState<ChatUiState>>();
            }
        }
        match snapshot.detail_state.clone() {
            Some(state) => {
                detail_states.restore(transition.webview, snapshot.metadata.as_ref(), state)
            }
            None => {
                commands
                    .entity(transition.webview)
                    .remove::<UiState<SessionDetailUiState>>();
            }
        }
        restore_components(&snapshot, transition.webview, &mut commands);
    }
}

fn restore_components(snapshot: &InlineTransitionSnapshot, view: Entity, commands: &mut Commands) {
    let mut entity = commands.entity(view);
    match snapshot.metadata.clone() {
        Some(metadata) => {
            entity.insert(metadata);
        }
        None => {
            entity.remove::<PageMetadata>();
        }
    }
    if snapshot.session_view {
        entity.insert(SessionView);
    } else {
        entity.remove::<SessionView>();
    }
    if snapshot.session_manager_view {
        entity.insert(SessionManagerView);
    } else {
        entity.remove::<SessionManagerView>();
    }
    if snapshot.transition_view {
        entity.insert(StartInlineTransitionView);
    } else {
        entity.remove::<StartInlineTransitionView>();
    }
    if snapshot.hosts_launcher {
        entity.insert(vmux_ecs::launcher::HostsLauncher);
    } else {
        entity.remove::<vmux_ecs::launcher::HostsLauncher>();
    }
    if snapshot.page_ready {
        entity.insert(vmux_ecs::page::PageReady);
    } else {
        entity.remove::<vmux_ecs::page::PageReady>();
    }
    entity.remove::<(PreparingAgentChatView, InlineTransitionSnapshot)>();
}

fn swap(
    mut reader: MessageReader<SwapStackSession>,
    settings: Res<AppSettings>,
    catalog: AcpCatalog,
    targets: Query<&EntityTarget<Session>>,
    sessions: Query<(&SessionId, &Name, &Cwd), With<Session>>,
    mut runtime: SessionRuntime,
) {
    for ev in reader.read() {
        let id = ev.target_agent.as_str();
        if !settings.agent.acp.iter().any(|cfg| cfg.id == id) && catalog.agent(id).is_none() {
            bevy::log::warn!("swap: ACP agent unavailable for '{id}'");
            continue;
        };
        let Ok(target) = targets.get(ev.stack) else {
            bevy::log::warn!("swap: stack has no Session target");
            continue;
        };
        let session_entity = target.entity();
        let Ok((session_id, session_name, cwd)) = sessions.get(session_entity) else {
            bevy::log::warn!("swap: Session target is unavailable");
            continue;
        };
        let imported = ev.handoff.as_ref().map(|handoff| {
            (
                ImportedConversation {
                    source_agent: handoff.source_agent.clone(),
                    source_sid: handoff.source_sid.clone(),
                    messages: handoff.messages.clone(),
                    truncated: handoff.truncated,
                    first_prompt: None,
                },
                super::handoff::PendingHandoff {
                    context: handoff.context.clone(),
                    sent: false,
                },
            )
        });

        runtime.detach(session_entity);
        let cwd = if ev.cwd.as_os_str().is_empty() {
            cwd.0.clone()
        } else {
            ev.cwd.clone()
        };
        runtime
            .commands
            .entity(session_entity)
            .insert((AgentId(id.to_string()), Cwd(cwd.clone())));
        let cfg = settings.agent.acp.iter().find(|cfg| cfg.id == id);
        let icon = catalog.icon(id);
        let agent_name = catalog.profile_name(id, cfg);
        let navigation = vmux_ecs::PageOpenId::new();
        runtime
            .commands
            .entity(ev.stack)
            .insert(vmux_ecs::PageNavigation::new(navigation));
        runtime.commands.spawn((
            AcpAgentAttachment::new(
                session_entity,
                ev.stack,
                id,
                session_name.as_str(),
                agent_name,
                session_id.0.clone(),
                cwd,
                icon,
                None,
            ),
            vmux_ecs::PageNavigation::new(navigation),
        ));
        if let Some((imported, pending)) = imported {
            runtime
                .commands
                .entity(session_entity)
                .insert((imported, pending));
        }
    }
}

#[derive(SystemParam)]
struct SessionRuntime<'w, 's> {
    commands: Commands<'w, 's>,
}

impl SessionRuntime<'_, '_> {
    fn detach(&mut self, session: Entity) {
        self.commands.entity(session).remove::<(
            AcpSessionId,
            vmux_ecs::ProcessAnchor,
            crate::host::acp::AcpLaunchStarted,
            crate::host::acp::AcpSpawnQueued,
            crate::host::runtime::AcpSessionConfigState,
            crate::host::toast::ErrorSurfaced,
            vmux_session::ApprovalPolicy,
            vmux_session::RunState,
            vmux_session::AgentTurnMeta,
            ImportedConversation,
            super::handoff::PendingHandoff,
            vmux_ecs::agent::AgentSessionRoot,
            vmux_ecs::team::Agent,
            vmux_ecs::team::Profile,
        )>();
    }

    fn cleanup(&mut self, session: Entity) {
        self.detach(session);
        self.commands.entity(session).remove::<PromptQueue>();
    }
}

fn cleanup(
    trigger: On<Cleanup>,
    stacks: Query<(Entity, &EntityTarget<Session>)>,
    mut opens: MessageWriter<vmux_ecs::PageOpenRequest>,
    mut runtime: SessionRuntime,
) {
    let session = trigger.event_target();
    runtime.cleanup(session);
    for (stack, target) in &stacks {
        if target.entity() == session {
            opens.write(vmux_ecs::PageOpenRequest {
                target: vmux_ecs::PageOpenTarget::Stack(stack),
                url: Route::Manager.url(),
                request_id: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use vmux_setting::AcpAgentConfig;

    use super::*;
    use crate::host::acp::registry::Distribution;

    fn registry_agent(id: &str, name: &str, icon: Option<&str>) -> RegistryAgent {
        RegistryAgent {
            id: id.to_string(),
            name: name.to_string(),
            version: None,
            description: None,
            icon: icon.map(str::to_string),
            distribution: Distribution::default(),
        }
    }

    #[test]
    fn catalog_reads_agent_icons_from_entities() {
        let mut world = World::new();
        world.spawn(registry_agent(
            "mistral-vibe",
            "Mistral Vibe",
            Some("https://cdn.example/vibe.svg"),
        ));
        world.spawn(registry_agent(
            "claude-acp",
            "Claude Agent",
            Some("https://cdn.example/claude.svg"),
        ));

        let icons = world
            .run_system_once(|catalog: AcpCatalog| {
                (
                    catalog.icon("mistral-vibe"),
                    catalog.icon("claude-acp"),
                    catalog.icon("absent"),
                )
            })
            .unwrap();

        assert_eq!(icons.0.as_deref(), Some("https://cdn.example/vibe.svg"));
        assert_eq!(icons.1.as_deref(), Some("https://cdn.example/claude.svg"));
        assert_eq!(icons.2, None);
    }

    #[test]
    fn catalog_prefers_registry_name_then_config_then_id() {
        let mut world = World::new();
        world.spawn(registry_agent("claude-acp", "Claude", None));

        let names = world
            .run_system_once(|catalog: AcpCatalog| {
                let mut config = AcpAgentConfig {
                    id: "claude-acp".into(),
                    name: "Configured Claude".into(),
                    command: "npx".into(),
                    args: vec![],
                    env: vec![],
                    cwd: None,
                    version: None,
                };
                let registry = catalog.profile_name(&config.id, Some(&config));
                let configured = catalog.profile_name("configured", Some(&config));
                config.name = "   ".into();
                let fallback = catalog.profile_name("fallback", Some(&config));
                (registry, configured, fallback)
            })
            .unwrap();

        assert_eq!(names.0, "Claude");
        assert_eq!(names.1, "Configured Claude");
        assert_eq!(names.2, "fallback");
    }

    #[test]
    fn agent_swap_retains_the_previous_surface_until_attachment_handoff() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(AppSettings::default())
            .add_message::<SwapStackSession>()
            .add_systems(Update, swap);
        app.world_mut()
            .spawn(registry_agent("claude-acp", "Claude", None));
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("session-1".into()),
                Name::new("Task"),
                Cwd(PathBuf::from("/tmp")),
                AgentId("codex".into()),
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();
        let previous = app
            .world_mut()
            .spawn((vmux_layout::Browser, ChildOf(stack)))
            .id();
        app.world_mut().write_message(SwapStackSession {
            stack,
            target_agent: "claude-acp".into(),
            cwd: PathBuf::new(),
            handoff: None,
        });

        app.update();

        assert!(app.world().get_entity(previous).is_ok());
        assert!(app.world().get::<vmux_ecs::PageNavigation>(stack).is_some());
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<super::super::attach_driver::PendingAcpAgentAttachment>>(
                )
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn manager_target_preserves_requested_agent() {
        let url = Route::manager_for_agent(&AgentId("codex".into()));
        let target = AgentChatTarget::parse(&url).unwrap();

        assert_eq!(target.url, url);
    }

    #[test]
    fn stale_inline_transition_restores_the_previous_view() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::UiStatePlugin::<ChatUiState>::default(),
            vmux_ecs::UiStatePlugin::<SessionDetailUiState>::default(),
        ))
        .add_systems(Update, discard_stale_inline_transitions);
        let stale = vmux_ecs::PageOpenId::new();
        let current = vmux_ecs::PageOpenId::new();
        let original = PageMetadata {
            url: Route::Manager.url(),
            title: "Sessions".into(),
            ..Default::default()
        };
        let view = app
            .world_mut()
            .spawn((
                PageMetadata {
                    url: Route::Session(SessionId("stale".into())).url(),
                    title: "Stale".into(),
                    ..Default::default()
                },
                SessionView,
                PreparingAgentChatView,
                UiState::<ChatUiState>::default(),
                UiState::<SessionDetailUiState>::default(),
                InlineTransitionSnapshot {
                    metadata: Some(original.clone()),
                    session_view: false,
                    session_manager_view: true,
                    transition_view: true,
                    hosts_launcher: false,
                    page_ready: true,
                    chat_state: None,
                    detail_state: None,
                },
            ))
            .id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_ecs::PageNavigation::new(current),
                StartInlineTransition {
                    webview: view,
                    navigation: Some(stale),
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<StartInlineTransition>(stack).is_none());
        assert_eq!(app.world().get::<PageMetadata>(view), Some(&original));
        assert!(app.world().get::<SessionManagerView>(view).is_some());
        assert!(app.world().get::<SessionView>(view).is_none());
        assert!(app.world().get::<StartInlineTransitionView>(view).is_some());
        assert!(app.world().get::<vmux_ecs::page::PageReady>(view).is_some());
        assert!(app.world().get::<UiState<ChatUiState>>(view).is_none());
        assert!(
            app.world()
                .get::<UiState<SessionDetailUiState>>(view)
                .is_none()
        );
        assert!(app.world().get::<InlineTransitionSnapshot>(view).is_none());
    }

    #[test]
    fn workspace_check_runs_once_before_releasing_page_open() {
        let repository = tempfile::tempdir().unwrap();
        vmux_git::worktree::CheckoutInfo::initialize(repository.path()).unwrap();
        let path = repository.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.init_resource::<vmux_git::GitRuntime>().add_systems(
            Update,
            (poll_workspace_checks, start_workspace_checks).chain(),
        );
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    startup_dir: Some(path.clone()),
                    ..Default::default()
                },
                TabWorkspace {
                    project_dir: path.clone(),
                },
            ))
            .id();
        let request = app
            .world_mut()
            .spawn((
                PageOpenDeferred,
                WorkspaceCheckRequest {
                    tab,
                    project_dir: path.clone(),
                    startup_dir: Some(path),
                },
            ))
            .id();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.world().get::<PageOpenDeferred>(request).is_some() {
            app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }

        assert!(app.world().get::<TabDirDecided>(tab).is_some());
        assert!(
            app.world()
                .get::<vmux_space::RepositoryNeedsWorktree>(tab)
                .is_some()
        );
        assert!(app.world().get::<WorkspaceCheckRequest>(request).is_none());
        assert!(app.world().get::<WorkspaceCheckTask>(request).is_none());
    }

    #[test]
    fn opening_session_rebinds_the_ancestor_tab_workspace() {
        let mut world = World::new();
        world.insert_resource(AppSettings::default());
        world.insert_resource(Time::<()>::default());
        let tab = world
            .spawn((
                Tab {
                    startup_dir: Some("/old".into()),
                    ..Default::default()
                },
                TabWorkspace {
                    project_dir: "/old".into(),
                },
            ))
            .id();
        let stack = world.spawn(ChildOf(tab)).id();

        world
            .run_system_once(move |mut opener: PageOpener| {
                opener.bind_workspace(stack, Path::new("/project"));
            })
            .unwrap();

        assert_eq!(
            world.get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some("/project")
        );
        assert_eq!(
            world.get::<TabWorkspace>(tab).unwrap().project_dir,
            "/project"
        );
    }

    #[test]
    fn runtime_cleanup_removes_every_transient_component() {
        let mut world = World::new();
        let session = world
            .spawn((
                AcpSessionId("acp-session".into()),
                vmux_ecs::ProcessAnchor(vmux_ecs::ProcessId::new()),
                crate::host::acp::AcpLaunchStarted,
                vmux_session::ApprovalPolicy::default(),
                vmux_session::RunState::default(),
                vmux_session::AgentTurnMeta::default(),
                PromptQueue::default(),
                crate::host::runtime::AcpSessionConfigState::default(),
                crate::host::toast::ErrorSurfaced,
                ImportedConversation {
                    source_agent: "codex".into(),
                    source_sid: "source".into(),
                    messages: Vec::new(),
                    truncated: false,
                    first_prompt: None,
                },
                super::super::handoff::PendingHandoff {
                    context: String::new(),
                    sent: false,
                },
                vmux_ecs::agent::AgentSessionRoot,
                vmux_ecs::team::Agent { sid: "sid".into() },
                vmux_ecs::team::Profile::registry("Codex", "codex"),
            ))
            .id();

        world
            .run_system_once(move |mut runtime: SessionRuntime| {
                runtime.cleanup(session);
            })
            .unwrap();
        world.flush();

        assert!(world.get::<AcpSessionId>(session).is_none());
        assert!(world.get::<vmux_ecs::ProcessAnchor>(session).is_none());
        assert!(
            world
                .get::<crate::host::acp::AcpLaunchStarted>(session)
                .is_none()
        );
        assert!(world.get::<vmux_session::ApprovalPolicy>(session).is_none());
        assert!(world.get::<vmux_session::RunState>(session).is_none());
        assert!(world.get::<vmux_session::AgentTurnMeta>(session).is_none());
        assert!(world.get::<PromptQueue>(session).is_none());
        assert!(
            world
                .get::<crate::host::runtime::AcpSessionConfigState>(session)
                .is_none()
        );
        assert!(
            world
                .get::<crate::host::toast::ErrorSurfaced>(session)
                .is_none()
        );
        assert!(world.get::<ImportedConversation>(session).is_none());
        assert!(
            world
                .get::<super::super::handoff::PendingHandoff>(session)
                .is_none()
        );
        assert!(
            world
                .get::<vmux_ecs::agent::AgentSessionRoot>(session)
                .is_none()
        );
        assert!(world.get::<vmux_ecs::team::Agent>(session).is_none());
        assert!(world.get::<vmux_ecs::team::Profile>(session).is_none());
    }

    #[test]
    fn runtime_detach_preserves_queued_prompts() {
        let mut world = World::new();
        let mut queue = PromptQueue::default();
        queue.enqueue("keep me".into());
        let session = world
            .spawn((
                vmux_session::RunState::default(),
                vmux_session::AgentTurnMeta::default(),
                queue,
            ))
            .id();

        world
            .run_system_once(move |mut runtime: SessionRuntime| {
                runtime.detach(session);
            })
            .unwrap();
        world.flush();

        let queue = world.get::<PromptQueue>(session).unwrap();
        assert_eq!(queue.items.front().unwrap().text, "keep me");
        assert!(world.get::<vmux_session::RunState>(session).is_none());
        assert!(world.get::<vmux_session::AgentTurnMeta>(session).is_none());
    }

    #[test]
    fn cleanup_detaches_runtime_and_redirects_open_views() {
        let mut app = App::new();
        app.add_message::<vmux_ecs::PageOpenRequest>()
            .add_observer(cleanup);
        let session = app
            .world_mut()
            .spawn((Session, PromptQueue::default()))
            .id();
        let stack = app
            .world_mut()
            .spawn(EntityTarget::<Session>::new(session))
            .id();

        app.world_mut().trigger(Cleanup { entity: session });
        app.world_mut().flush();

        assert!(app.world().get::<PromptQueue>(session).is_none());
        let requests = app
            .world_mut()
            .resource_mut::<Messages<vmux_ecs::PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, Route::Manager.url());
        assert!(matches!(
            requests[0].target,
            vmux_ecs::PageOpenTarget::Stack(entity) if entity == stack
        ));
    }
}
