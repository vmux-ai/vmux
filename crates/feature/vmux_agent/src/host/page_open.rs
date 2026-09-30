use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use vmux_api::protocol::AgentAttachment;
use vmux_chat::host::{ChatView, ImportedConversation};
use vmux_core::agent::SwapStackSession;
use vmux_core::host::persistence::PageRestore;
use vmux_core::terminal::TerminalLaunch;
use vmux_core::{
    PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
    PendingPrompt, PendingPromptAttachments,
};
use vmux_layout::space::FocusedSpace;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::{ManagedWorktreeRoot, TabWorktreeActivation, TabWorktreeReady};
use vmux_session::{AcpSession, AgentConversationTitle, PromptQueue};
use vmux_setting::AppSettings;
use vmux_start::{StartInlineTransition, StartInlineTransitionView};

use super::attach::{
    AcpAgentAttachment, acp_icon_for_id, acp_profile_name_for_id, acp_registry_agent_for_id,
};
use crate::acp_registry::RegistryAgent;
use vmux_terminal::agent_run::AgentCwd;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

pub(super) struct PageOpenPlugin;

impl Plugin for PageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_swap_stack_session)
            .add_systems(
                Update,
                (
                    release_agent_transition_paint,
                    prepare_agent_tab_worktrees,
                    start_agent_tab_worktrees,
                    drain_agent_tab_worktrees,
                    handle_agent_page_open,
                )
                    .chain()
                    .in_set(PageOpenSet::HandleKnownPages),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentPageOpenWorkspace<'w, 's> {
    active_space: FocusedSpace<'w, 's>,
    tabs: Query<'w, 's, &'static Tab>,
    hierarchy: vmux_layout::space::SpaceHierarchy<'w, 's>,
}

impl AgentPageOpenWorkspace<'_, '_> {
    fn startup_dir(
        &self,
        entity: Entity,
        settings: &AppSettings,
    ) -> Option<vmux_setting::StartupDir> {
        let space_id = self
            .hierarchy
            .id(entity)
            .or_else(|| self.active_space.id().map(str::to_string))?;
        vmux_setting::StartupDir::resolve(settings, &space_id, None)
    }
}

#[derive(Component)]
struct PendingAgentWorktree {
    tab: Entity,
    tab_name: String,
    startup_dir: Option<String>,
    workspace: TabWorkspace,
    metadata: TabWorktree,
    managed_root: PathBuf,
}

#[derive(Component)]
struct AgentWorktreeTask(Task<Result<TabWorktreeActivation, String>>);

#[derive(Component, Clone, Copy)]
struct AwaitingAgentWorktree {
    pending: Entity,
}

#[derive(Component)]
struct PreparingAgentChatView;

#[derive(Component)]
struct AwaitingAgentTransitionPaint;

struct AgentChatTarget {
    url: String,
    title: String,
}

impl AgentChatTarget {
    fn parse(url: &str) -> Option<Self> {
        match crate::AgentUrl::parse(url)? {
            crate::AgentUrl::AcpDefault => Some(Self {
                url: "vmux://sessions/".to_string(),
                title: "Agent".to_string(),
            }),
            crate::AgentUrl::Acp { id, sid } => {
                let url = match sid {
                    Some(sid) => format!("vmux://sessions/{id}/{sid}"),
                    None => format!("vmux://sessions/{id}"),
                };
                Some(Self {
                    url,
                    title: id.to_string(),
                })
            }
        }
    }
}

fn agent_url_uses_local_workspace(url: &str) -> bool {
    crate::AgentUrl::parse(url).is_some()
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

fn ancestor_agent_tab(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&Tab>,
) -> Option<(Entity, Option<String>)> {
    let mut current = entity;
    loop {
        if let Ok(tab) = tabs.get(current) {
            return Some((current, tab.startup_dir.clone()));
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn prepare_agent_tab_worktrees(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    pending: Query<(Entity, &PendingAgentWorktree)>,
    transitions: Query<&StartInlineTransition>,
    preparing_views: Query<(Entity, &ChildOf), With<PreparingAgentChatView>>,
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
    managed_root: Option<Res<ManagedWorktreeRoot>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let managed_root = managed_root.as_deref().cloned().unwrap_or_default().0;
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let mut pending_by_tab: std::collections::HashMap<Entity, Entity> = pending
        .iter()
        .map(|(entity, pending)| (pending.tab, entity))
        .collect();
    let mut preparing_by_stack: std::collections::HashMap<Entity, Entity> = preparing_views
        .iter()
        .map(|(entity, child_of)| (child_of.parent(), entity))
        .collect();
    let mut opened_stacks = std::collections::HashSet::new();
    for (task_entity, task) in &tasks {
        if !agent_url_uses_local_workspace(&task.url) {
            continue;
        }
        let Some(tab_entity) = ancestor_tab_entity(task.stack, &child_of, &tabs) else {
            continue;
        };
        if !preparing_by_stack.contains_key(&task.stack)
            && let Ok(transition) = transitions.get(task.stack)
            && let Some(target) = AgentChatTarget::parse(&task.url)
        {
            let view = transition.webview;
            commands
                .entity(view)
                .insert((
                    PageMetadata {
                        url: target.url,
                        title: target.title,
                        bg_color: None,
                        ..default()
                    },
                    ChatView,
                    PreparingAgentChatView,
                ))
                .remove::<(
                    StartInlineTransitionView,
                    vmux_core::launcher::HostsLauncher,
                    vmux_core::page::PageReady,
                )>();
            commands
                .entity(task.stack)
                .insert(StartInlineTransition { webview: view });
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
        if let Some(pending) = pending_by_tab.get(&tab_entity).copied() {
            commands
                .entity(task_entity)
                .insert((PageOpenDeferred, AwaitingAgentWorktree { pending }));
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
            && AgentCwd::from_tab(tab.startup_dir.as_deref())
                .stored()
                .ok()
                .flatten()
                .is_none()
            && AgentCwd::from_tab(Some(&workspace.project_dir))
                .stored()
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
        let pending_worktree = if let Some(metadata) = metadata {
            commands
                .entity(tab_entity)
                .remove::<vmux_space::RepositoryNeedsWorktree>();
            if ready.is_some_and(|ready| ready.is_current(&tab, &workspace, metadata)) {
                None
            } else {
                Some(PendingAgentWorktree {
                    tab: tab_entity,
                    tab_name: tab.name.clone(),
                    startup_dir: tab.startup_dir.clone(),
                    workspace: workspace.clone(),
                    metadata: metadata.clone(),
                    managed_root: managed_root.clone(),
                })
            }
        } else {
            let current_dir = tab
                .startup_dir
                .as_deref()
                .map(Path::new)
                .and_then(|path| path.canonicalize().ok());
            let needs_worktree = decided.is_none()
                && !current_dir
                    .as_deref()
                    .is_some_and(vmux_git::worktree::is_linked_worktree)
                && vmux_git::worktree::CheckoutInfo::try_from(Path::new(&workspace.project_dir))
                    .is_ok();
            let mut entity = commands.entity(tab_entity);
            if needs_worktree {
                entity.insert(vmux_space::RepositoryNeedsWorktree);
            } else {
                entity.remove::<vmux_space::RepositoryNeedsWorktree>();
            }
            None
        };
        let Some(pending_worktree) = pending_worktree else {
            commands
                .entity(tab_entity)
                .remove::<TabWorktreeUnavailable>();
            continue;
        };
        let pending = commands.spawn(pending_worktree).id();
        pending_by_tab.insert(tab_entity, pending);
        commands
            .entity(task_entity)
            .insert((PageOpenDeferred, AwaitingAgentWorktree { pending }));
    }
}

fn start_agent_tab_worktrees(
    pending: Query<(Entity, &PendingAgentWorktree), Without<AgentWorktreeTask>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, pending) in &pending {
        let tab = Tab {
            name: pending.tab_name.clone(),
            startup_dir: pending.startup_dir.clone(),
        };
        let workspace = pending.workspace.clone();
        let metadata = pending.metadata.clone();
        let managed_root = pending.managed_root.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = vmux_layout::worktree::ensure_tab_worktree_available(
                &tab,
                &workspace,
                &metadata,
                &managed_root,
            );
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.entity(entity).insert(AgentWorktreeTask(task));
    }
}

fn release_agent_transition_paint(
    waiting: Query<Entity, With<AwaitingAgentTransitionPaint>>,
    mut commands: Commands,
) {
    for entity in &waiting {
        commands
            .entity(entity)
            .remove::<(PageOpenDeferred, AwaitingAgentTransitionPaint)>();
    }
}

fn drain_agent_tab_worktrees(
    mut pending: Query<(Entity, &PendingAgentWorktree, &mut AgentWorktreeTask)>,
    mut tabs: Query<(&mut Tab, Option<&TabWorkspace>, Option<&TabWorktree>)>,
    waiting: Query<(Entity, &AwaitingAgentWorktree, &PageOpenTask)>,
    mut commands: Commands,
) {
    for (pending_entity, pending, mut task) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        let outcome = match tabs.get_mut(pending.tab) {
            Err(_) => Some(Err("agent tab no longer exists".to_string())),
            Ok((tab, workspace, metadata))
                if tab.startup_dir != pending.startup_dir
                    || workspace != Some(&pending.workspace)
                    || metadata != Some(&pending.metadata) =>
            {
                None
            }
            Ok((mut tab, _, metadata)) => match result {
                Ok(activation) => {
                    tab.startup_dir = Some(activation.execution_dir.to_string_lossy().into_owned());
                    let mut entity = commands.entity(pending.tab);
                    if metadata != Some(&activation.metadata) {
                        entity.insert(activation.metadata);
                    }
                    entity
                        .insert(activation.ready)
                        .remove::<TabWorktreeUnavailable>();
                    Some(Ok(()))
                }
                Err(message) => {
                    commands
                        .entity(pending.tab)
                        .insert(TabWorktreeUnavailable {
                            message: message.clone(),
                        })
                        .remove::<TabWorktreeReady>();
                    Some(Err(message))
                }
            },
        };
        let mut resumed = std::collections::HashSet::new();
        for (task_entity, waiting, task) in &waiting {
            if waiting.pending != pending_entity {
                continue;
            }
            let duplicate = !resumed.insert((task.stack, task.url.clone()));
            let mut entity = commands.entity(task_entity);
            entity.remove::<(AwaitingAgentWorktree, PageOpenDeferred)>();
            if let Some(Err(message)) = outcome.as_ref() {
                entity.insert(PageOpenError {
                    message: message.clone(),
                });
            }
            if duplicate {
                entity.insert(PageOpenHandled);
            }
        }
        commands.entity(pending_entity).despawn();
    }
}

fn handle_agent_page_open(
    mut open_q: ParamSet<(
        Query<(Entity, &PageOpenTask, Has<PageRestore>), PendingPageOpen>,
        Query<(&PendingPrompt, Option<&PendingPromptAttachments>)>,
    )>,
    acp_sessions: Query<&AcpSession>,
    child_of_q: Query<&ChildOf>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    workspace: AgentPageOpenWorkspace,
    catalog: Option<Single<&crate::runtime::AcpCatalog>>,
    transitions: Query<&StartInlineTransition>,
    launches: Query<&TerminalLaunch>,
) {
    let catalog = catalog.as_ref().map(|catalog| **catalog);
    let tasks: Vec<(Entity, PageOpenTask, bool)> = open_q
        .p0()
        .iter()
        .map(|(entity, task, restoring)| (entity, task.clone(), restoring))
        .collect();
    for (entity, task, restoring) in tasks {
        if !vmux_api::VmuxRoute::parse(&task.url).is_some_and(|route| route.is_agent()) {
            continue;
        }
        let tab = ancestor_agent_tab(task.stack, &child_of_q, &workspace.tabs);
        let tab_dir = tab
            .as_ref()
            .and_then(|(_, startup_dir)| startup_dir.clone());
        let space_startup_dir = workspace.startup_dir(task.stack, &settings);
        let restored_cwd = restoring
            .then(|| launches.get(task.stack).ok())
            .flatten()
            .map(|launch| PathBuf::from(&launch.cwd));
        let default_cwd = if let Some(cwd) = restored_cwd {
            cwd
        } else {
            match AgentCwd::from_tab(tab_dir.as_deref()).stored() {
                Ok(Some(path)) => path,
                Ok(None) => match space_startup_dir {
                    Some(dir) => dir.path,
                    None => match AgentCwd::projects() {
                        Ok(path) => path,
                        Err(message) => {
                            commands.entity(entity).insert(PageOpenError { message });
                            continue;
                        }
                    },
                },
                Err(message) => {
                    commands.entity(entity).insert(PageOpenError { message });
                    continue;
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
            .filter(|_| vmux_start::supports_inline_agent_transition(&task.url));
        match handle_agent_page_open_task(
            &task,
            initial_prompt,
            initial_attachments,
            transition_webview,
            &acp_sessions,
            &mut commands,
            &default_cwd,
            &settings.agent.acp,
            catalog,
        ) {
            Ok(()) => {
                commands.entity(entity).insert(PageOpenHandled);
                if let Some(webview) = transition_webview {
                    commands.entity(webview).remove::<PreparingAgentChatView>();
                }
                commands
                    .entity(task.stack)
                    .remove::<StartInlineTransition>();
            }
            Err(message) => {
                commands.entity(entity).insert(PageOpenError { message });
            }
        }
    }
}

fn handle_swap_stack_session(
    mut reader: MessageReader<SwapStackSession>,
    settings: Res<AppSettings>,
    catalog: Option<Single<&crate::runtime::AcpCatalog>>,
    mut commands: Commands,
) {
    let catalog = catalog.as_ref().map(|catalog| **catalog);
    for ev in reader.read() {
        let target = match crate::AgentUrl::parse(&ev.target_url) {
            Some(target @ crate::AgentUrl::Acp { .. }) => target,
            other => {
                bevy::log::warn!("swap: unsupported target url {other:?} ({})", ev.target_url);
                continue;
            }
        };
        if let crate::AgentUrl::Acp { id, .. } = &target
            && !settings.agent.acp.iter().any(|cfg| cfg.id == *id)
            && acp_registry_agent_for_id(catalog, id).is_none()
        {
            bevy::log::warn!("swap: ACP agent unavailable for '{id}'");
            continue;
        }
        let imported = ev.handoff.as_ref().map(|handoff| {
            (
                ImportedConversation {
                    source_agent: handoff.source_agent.clone(),
                    source_sid: handoff.source_sid.clone(),
                    messages: handoff.messages.clone(),
                    truncated: handoff.truncated,
                    first_prompt: None,
                },
                crate::handoff::PendingHandoff {
                    context: handoff.context.clone(),
                    sent: false,
                },
            )
        });

        commands
            .entity(ev.stack)
            .remove::<AcpSession>()
            .remove::<crate::acp_tool::AcpLaunchStarted>()
            .remove::<crate::AgentMessages>()
            .remove::<crate::AgentApprovalPolicy>()
            .remove::<vmux_session::AgentRunState>()
            .remove::<ImportedConversation>()
            .remove::<crate::handoff::PendingHandoff>()
            .remove::<vmux_core::AgentWorkingDir>()
            .remove::<vmux_core::team::Agent>()
            .remove::<vmux_core::team::Profile>();
        commands.entity(ev.stack).despawn_children();

        match target {
            crate::AgentUrl::Acp { id, sid } => {
                let cfg = settings.agent.acp.iter().find(|cfg| cfg.id == id);
                let routing_sid = uuid::Uuid::new_v4().to_string();
                let icon = acp_icon_for_id(catalog, &id);
                let name = acp_profile_name_for_id(&id, cfg, catalog);
                let request = AcpAgentAttachment::new(id, name, routing_sid, ev.cwd.clone())
                    .icon(icon)
                    .resume(sid);
                commands.entity(ev.stack).insert(request);
                if let Some((imported, pending)) = imported {
                    commands.entity(ev.stack).insert((imported, pending));
                }
            }
            _ => unreachable!(),
        }
    }
}

fn handle_agent_page_open_task(
    task: &PageOpenTask,
    initial_prompt: Option<String>,
    initial_attachments: Vec<AgentAttachment>,
    transition_webview: Option<Entity>,
    acp_sessions: &Query<&AcpSession>,
    commands: &mut Commands,
    default_cwd: &Path,
    acp_configs: &[vmux_setting::AcpAgentConfig],
    catalog: Option<&crate::runtime::AcpCatalog>,
) -> Result<(), String> {
    let target = match crate::AgentUrl::parse(&task.url) {
        Some(crate::AgentUrl::AcpDefault) => {
            let id = acp_configs
                .first()
                .map(|config| config.id.clone())
                .or_else(|| {
                    catalog.and_then(|catalog| {
                        catalog
                            .agents
                            .iter()
                            .find(|agent| RegistryAgent::is_installed(agent))
                            .map(|agent| agent.id.clone())
                    })
                })
                .ok_or_else(|| "no ACP agent is configured or installed".to_string())?;
            crate::AgentUrl::Acp { id, sid: None }
        }
        Some(target) => target,
        None => return Err(format!("malformed agent URL '{}'", task.url)),
    };
    match target {
        crate::AgentUrl::Acp { id, sid } => {
            let cfg = acp_configs.iter().find(|config| config.id == id);
            if cfg.is_none() && acp_registry_agent_for_id(catalog, &id).is_none() {
                return Err(format!("ACP agent unavailable for '{id}'"));
            }
            if acp_sessions
                .get(task.stack)
                .is_ok_and(|session| session.agent_id == id)
            {
                return Ok(());
            }
            if transition_webview.is_none() {
                commands.entity(task.stack).despawn_children();
            }
            let routing_sid = uuid::Uuid::new_v4().to_string();
            let icon = acp_icon_for_id(catalog, &id);
            let name = acp_profile_name_for_id(&id, cfg, catalog);
            let request = AcpAgentAttachment::new(id, name, routing_sid, default_cwd.to_path_buf())
                .icon(icon)
                .resume(sid)
                .webview(transition_webview);
            commands.entity(task.stack).insert(request);
            insert_initial_prompt_queue(task.stack, initial_prompt, initial_attachments, commands);
            Ok(())
        }
        crate::AgentUrl::AcpDefault => unreachable!(),
    }
}

fn insert_initial_prompt_queue(
    stack: Entity,
    initial_prompt: Option<String>,
    initial_attachments: Vec<AgentAttachment>,
    commands: &mut Commands,
) {
    let prompt = initial_prompt.unwrap_or_default();
    if prompt.trim().is_empty() && initial_attachments.is_empty() {
        return;
    }
    if let Some(title) = vmux_session::provisional_conversation_title(&prompt) {
        commands.entity(stack).insert(AgentConversationTitle(title));
    }
    let mut queue = PromptQueue::default();
    queue.enqueue_with_attachments(prompt, initial_attachments);
    commands
        .entity(stack)
        .insert(queue)
        .remove::<(PendingPrompt, PendingPromptAttachments)>();
}
