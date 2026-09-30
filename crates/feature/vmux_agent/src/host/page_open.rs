use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use vmux_api::protocol::AgentAttachment;
use vmux_chat::host::ChatView;
use vmux_core::KeyboardOwner;
#[cfg(test)]
use vmux_core::PageOpenId;
use vmux_core::agent::{
    AgentKind, AgentSession as CoreAgentSession, SessionId as CoreSessionId,
    SpawnAgentInStackRequest, SwapStackSession,
};
use vmux_core::host::persistence::PageRestore;
use vmux_core::terminal::TerminalLaunch;
use vmux_core::{
    PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
    PendingPrompt, PendingPromptAttachments,
};
#[cfg(test)]
use vmux_git::worktree::worktree_list;
use vmux_layout::Browser as LayoutBrowser;
#[cfg(test)]
use vmux_layout::space::CurrentSpace;
use vmux_layout::space::FocusedSpace;
#[cfg(test)]
use vmux_layout::space::{Space, SpaceId};
#[cfg(test)]
use vmux_layout::stack::stack_bundle;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::{ManagedWorktreeRoot, TabWorktreeActivation, TabWorktreeReady};
use vmux_session::{AcpSession, AgentConversationTitle, PromptQueue};
use vmux_setting::AppSettings;
#[cfg(test)]
use vmux_setting::SpaceOverrides;
#[cfg(test)]
use vmux_space::model::SpaceRecord;
#[cfg(test)]
use vmux_space::spaces::space_profile_bundle;
use vmux_start::{StartInlineTransition, StartInlineTransitionView};

use super::attach::{
    AcpAgentAttachment, AgentStrategies, PageAgentAttachment, acp_icon_for_id,
    acp_profile_name_for_id, acp_registry_agent_for_id,
};
use super::spawn::PendingPageOpen;
use crate::acp_registry::RegistryAgent;
use vmux_terminal::agent_run::AgentCwd;

pub(super) struct PageOpenPlugin;

impl Plugin for PageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_swap_stack_session.before(super::spawn::SpawnRequestSet),
        )
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
            crate::AgentUrl::Page {
                provider, model, ..
            } => Some(Self {
                url: format!("vmux://sessions/{provider}"),
                title: format!("{provider}/{model}"),
            }),
            crate::AgentUrl::PageDefault => {
                let provider = crate::host::provider::resolve_default_app_provider()?;
                Some(Self {
                    url: format!("vmux://sessions/{}", provider.provider),
                    title: format!("{}/{}", provider.provider, provider.default_model),
                })
            }
            crate::AgentUrl::Acp { id, sid } => {
                let id = RegistryAgent::url_id(&id);
                let url = match sid {
                    Some(sid) => format!("vmux://sessions/{id}/{sid}"),
                    None => format!("vmux://sessions/{id}"),
                };
                Some(Self {
                    url,
                    title: id.to_string(),
                })
            }
            crate::AgentUrl::Cli { .. } => None,
        }
    }
}

fn agent_url_uses_local_workspace(url: &str) -> bool {
    if AgentKind::all()
        .into_iter()
        .any(|kind| kind.is_setup_url(url))
    {
        return false;
    }
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
    children_q: Query<&Children>,
    agents: Query<&CoreAgentSession>,
    cli_sessions: Query<(Entity, &CoreAgentSession, &CoreSessionId)>,
    acp_sessions: Query<&AcpSession>,
    child_of_q: Query<&ChildOf>,
    strategies: AgentStrategies,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
    settings: Res<AppSettings>,
    workspace: AgentPageOpenWorkspace,
    catalog: Option<Single<&crate::runtime::acp::AcpCatalog>>,
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
            &children_q,
            &agents,
            &cli_sessions,
            &acp_sessions,
            &strategies,
            &mut spawn_agent,
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
    catalog: Option<Single<&crate::runtime::acp::AcpCatalog>>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
) {
    let catalog = catalog.as_ref().map(|catalog| **catalog);
    for ev in reader.read() {
        let target = match crate::AgentUrl::parse(&ev.target_url) {
            Some(target @ crate::AgentUrl::Cli { .. }) => target,
            Some(target @ crate::AgentUrl::Acp { .. }) => target,
            other => {
                bevy::log::warn!("swap: unsupported target url {other:?} ({})", ev.target_url);
                continue;
            }
        };
        if let crate::AgentUrl::Acp { id, .. } = &target
            && !settings
                .agent
                .acp
                .iter()
                .any(|cfg| RegistryAgent::ids_match(&cfg.id, id))
            && acp_registry_agent_for_id(catalog, id).is_none()
        {
            bevy::log::warn!("swap: ACP agent unavailable for '{id}'");
            continue;
        }
        if ev.handoff.is_some() && !matches!(target, crate::AgentUrl::Acp { .. }) {
            bevy::log::warn!("swap: cross-agent handoff requires an ACP target");
            continue;
        }
        let imported = ev.handoff.as_ref().map(|handoff| {
            (
                crate::handoff::ImportedConversation {
                    source_agent: handoff.source_agent.clone(),
                    source_kind: handoff.source_kind,
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
            .remove::<vmux_session::AgentSession>()
            .remove::<crate::AgentMessages>()
            .remove::<crate::AgentApprovalPolicy>()
            .remove::<vmux_session::AgentRunState>()
            .remove::<crate::handoff::ImportedConversation>()
            .remove::<crate::handoff::PendingHandoff>()
            .remove::<vmux_core::AgentWorkingDir>()
            .remove::<vmux_core::team::Agent>()
            .remove::<vmux_core::team::Profile>();
        commands.entity(ev.stack).despawn_children();

        match target {
            crate::AgentUrl::Cli { kind, sid } => {
                let session_id = (sid != crate::url::CLI_FRESH_SID).then_some(sid);
                spawn_agent.write(SpawnAgentInStackRequest {
                    kind,
                    cwd: ev.cwd.clone(),
                    session_id,
                    stack: ev.stack,
                    initial_prompt: None,
                    initial_attachments: Vec::new(),
                });
            }
            crate::AgentUrl::Acp { id, sid } => {
                let cfg = settings
                    .agent
                    .acp
                    .iter()
                    .find(|cfg| RegistryAgent::ids_match(&cfg.id, &id));
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
    children_q: &Query<&Children>,
    agents: &Query<&CoreAgentSession>,
    cli_sessions: &Query<(Entity, &CoreAgentSession, &CoreSessionId)>,
    acp_sessions: &Query<&AcpSession>,
    strategies: &AgentStrategies,
    spawn_agent: &mut MessageWriter<SpawnAgentInStackRequest>,
    commands: &mut Commands,
    default_cwd: &std::path::Path,
    acp_configs: &[vmux_setting::AcpAgentConfig],
    catalog: Option<&crate::runtime::acp::AcpCatalog>,
) -> Result<(), String> {
    if let Some(kind) = AgentKind::all()
        .into_iter()
        .find(|kind| kind.is_setup_url(&task.url))
    {
        attach_cli_setup_to_stack(kind, task.stack, commands);
        return Ok(());
    }
    match crate::AgentUrl::parse(&task.url) {
        Some(crate::AgentUrl::Page {
            provider,
            model,
            sid,
        }) => {
            let kind = strategies.page_kind(&provider, &model)?;
            if transition_webview.is_none() {
                commands.entity(task.stack).despawn_children();
            }
            let request = PageAgentAttachment::new(kind, provider, model, sid)
                .with_webview(transition_webview);
            commands.entity(task.stack).insert(request);
            insert_initial_prompt_queue(task.stack, initial_prompt, initial_attachments, commands);
            Ok(())
        }
        Some(crate::AgentUrl::PageDefault) => {
            let provider = crate::host::provider::resolve_default_app_provider().ok_or_else(|| {
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
                    .to_string()
            })?;
            let kind = strategies.page_kind(provider.provider, provider.default_model)?;
            let sid = uuid::Uuid::new_v4().to_string();
            if transition_webview.is_none() {
                commands.entity(task.stack).despawn_children();
            }
            let request =
                PageAgentAttachment::new(kind, provider.provider, provider.default_model, sid)
                    .with_webview(transition_webview);
            commands.entity(task.stack).insert(request);
            insert_initial_prompt_queue(task.stack, initial_prompt, initial_attachments, commands);
            Ok(())
        }
        Some(crate::AgentUrl::Cli { kind, sid }) => {
            if sid == crate::url::CLI_FRESH_SID {
                if !stack_has_agent_of_kind(task.stack, kind, children_q, agents) {
                    spawn_agent.write(SpawnAgentInStackRequest {
                        kind,
                        cwd: default_cwd.to_path_buf(),
                        session_id: None,
                        stack: task.stack,
                        initial_prompt,
                        initial_attachments,
                    });
                }
                return Ok(());
            }
            if let Some((entity, _, _)) = cli_sessions
                .iter()
                .find(|(_, session, id)| session.kind == kind && id.0 == sid)
            {
                commands.trigger(vmux_core::ActivateRequest { entity });
                return Ok(());
            }
            spawn_agent.write(SpawnAgentInStackRequest {
                kind,
                cwd: default_cwd.to_path_buf(),
                session_id: Some(sid),
                stack: task.stack,
                initial_prompt,
                initial_attachments,
            });
            Ok(())
        }
        Some(crate::AgentUrl::Acp { id, sid }) => {
            let cfg = acp_configs
                .iter()
                .find(|config| RegistryAgent::ids_match(&config.id, &id));
            if cfg.is_none() && acp_registry_agent_for_id(catalog, &id).is_none() {
                if sid.is_none()
                    && let Some(kind) = AgentKind::from_url_segment(&id)
                {
                    if !stack_has_agent_of_kind(task.stack, kind, children_q, agents) {
                        spawn_agent.write(SpawnAgentInStackRequest {
                            kind,
                            cwd: default_cwd.to_path_buf(),
                            session_id: None,
                            stack: task.stack,
                            initial_prompt,
                            initial_attachments,
                        });
                    }
                    return Ok(());
                }
                return Err(format!("ACP agent unavailable for '{id}'"));
            }
            if acp_sessions
                .get(task.stack)
                .is_ok_and(|session| RegistryAgent::ids_match(&session.agent_id, &id))
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
        None => Err(format!("malformed agent URL '{}'", task.url)),
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

pub(crate) fn cli_initial_prompt(
    kind: AgentKind,
    prompt: Option<&str>,
    attachments: &[AgentAttachment],
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(prompt) = prompt.filter(|prompt| !prompt.trim().is_empty()) {
        parts.push(prompt.to_string());
    }
    parts.extend(attachments.iter().filter_map(|attachment| {
        if attachment.path.is_empty() {
            return None;
        }
        let path = vmux_terminal::image_path_payload(kind == AgentKind::Vibe, &attachment.path);
        Some(if kind == AgentKind::Vibe {
            format!("@{path}")
        } else {
            path
        })
    }));
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn stack_has_agent_of_kind(
    stack: Entity,
    kind: AgentKind,
    children_q: &Query<&Children>,
    agents: &Query<&CoreAgentSession>,
) -> bool {
    children_q
        .get(stack)
        .map(|children| {
            children
                .iter()
                .any(|child| agents.get(child).is_ok_and(|session| session.kind == kind))
        })
        .unwrap_or(false)
}

pub(crate) fn attach_agent_spawn_error_to_stack(
    stack: Entity,
    kind: AgentKind,
    message: &str,
    commands: &mut Commands,
) {
    commands.entity(stack).despawn_children();
    let title = "Agent failed to start";
    let url = format!("vmux://error/agent/{}/", kind.as_url_segment());
    let message = html_escape(message);
    let html = format!(
        "<!doctype html><html><head><meta charset='utf-8'><title>{title}</title><style>html,body{{height:100%;margin:0;background:#101114;color:#e8e8ea;font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif}}main{{height:100%;display:flex;align-items:center;justify-content:center;padding:40px;box-sizing:border-box}}section{{max-width:640px}}h1{{font-size:28px;line-height:1.15;margin:0 0 12px;font-weight:650}}p{{font-size:14px;line-height:1.55;margin:0;color:#a9abb2}}code{{display:block;margin-top:18px;padding:12px;border-radius:6px;background:#1a1c22;color:#d7d8dd;white-space:pre-wrap;word-break:break-word}}</style></head><body><main><section><h1>{title}</h1><p>{}</p><code>{}</code></section></main></body></html>",
        kind.display_name(),
        message
    );
    let data_url = data_url_for_html(&html);
    commands.entity(stack).insert(PageMetadata {
        url,
        title: title.to_string(),
        bg_color: Some("#101114".to_string()),
        ..default()
    });
    let browser = commands
        .spawn((
            LayoutBrowser::new_with_title(&data_url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(KeyboardOwner);
}

pub(crate) fn attach_cli_setup_to_stack(kind: AgentKind, stack: Entity, commands: &mut Commands) {
    commands.entity(stack).despawn_children();
    commands
        .entity(stack)
        .remove::<crate::setup::AgentSetupNavigated>();
    let title = format!("Set up {} CLI", kind.display_name());
    let url = kind.setup_url();
    commands.entity(stack).insert(PageMetadata {
        url: url.clone(),
        title: title.clone(),
        bg_color: Some("#101114".to_string()),
        ..default()
    });
    let browser = commands
        .spawn((
            LayoutBrowser::new_with_title(&url, &title),
            crate::setup::AgentSetupView,
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(KeyboardOwner);
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn data_url_for_html(html: &str) -> String {
    let mut encoded = String::with_capacity(html.len() * 3);
    for byte in html.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::cli::VIBE as VIBE_CLI;
    use crate::host::provider::AgentExecutableOverride;
    use crate::host::spawn::{SpawnPlugin, SpawnRequestSet, SpawnRequestsPlugin};
    use crate::host::test_support::{init_worktree_test_repo, test_settings};
    use vmux_terminal::Terminal;

    pub(crate) fn swap_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SwapStackSession>()
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_swap_stack_session);
        app
    }

    pub(crate) fn spawn_stack_child(app: &mut App) -> (Entity, Entity) {
        let stack = app.world_mut().spawn_empty().id();
        let child = app.world_mut().spawn(ChildOf(stack)).id();
        (stack, child)
    }

    #[test]
    fn invalid_swap_target_preserves_current_stack_child() {
        let mut app = swap_test_app();
        let (stack, child) = spawn_stack_child(&mut app);
        app.world_mut()
            .resource_mut::<Messages<SwapStackSession>>()
            .write(SwapStackSession {
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
            .resource_mut::<Messages<SwapStackSession>>()
            .write(SwapStackSession {
                stack,
                target_url: "vmux://sessions/not-configured/sid-1".to_string(),
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
            .resource_mut::<Messages<SwapStackSession>>()
            .write(SwapStackSession {
                stack,
                target_url: "vmux://sessions/claude".to_string(),
                cwd: std::path::PathBuf::from("/source/work"),
                handoff: Some(vmux_core::agent::StackSessionHandoff {
                    source_agent: "Codex".into(),
                    source_kind: AgentKind::Codex,
                    source_sid: "cx-1".into(),
                    messages: messages.clone(),
                    context: "prior conversation".into(),
                    truncated: false,
                }),
            });

        app.update();

        let session = app.world().get::<AcpSession>(stack).unwrap();
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
            .insert(crate::acp_tool::AcpLaunchStarted);
        app.world_mut()
            .resource_mut::<Messages<SwapStackSession>>()
            .write(SwapStackSession {
                stack,
                target_url: "vmux://sessions/codex/session-2".to_string(),
                cwd: std::path::PathBuf::from("/work"),
                handoff: None,
            });

        app.update();

        assert!(
            app.world()
                .get::<crate::acp_tool::AcpLaunchStarted>(stack)
                .is_none()
        );
        let session = app.world().get::<AcpSession>(stack).unwrap();
        assert_eq!(session.resume.as_deref(), Some("session-2"));
    }

    #[test]
    fn missing_vibe_cli_shows_setup_page_at_vibe_url() {
        let mut app = App::new();
        app.world_mut().spawn(VIBE_CLI);
        app.world_mut()
            .spawn(AgentExecutableOverride(std::collections::HashMap::from([
                (AgentKind::Vibe, false),
            ])));
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .add_plugins(SpawnRequestsPlugin)
            .insert_resource(test_settings())
            .add_systems(Update, handle_agent_page_open.before(SpawnRequestSet));

        let stack = app.world_mut().spawn(stack_bundle()).id();
        let child = app.world_mut().spawn(ChildOf(stack)).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/vibe/".to_string(),
            request_id: None,
        });

        app.update();
        app.update();

        assert!(app.world().get_entity(child).is_err());
        let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(stack_meta.url, "vmux://sessions/vibe/setup");
        assert_eq!(stack_meta.title, "Set up Vibe CLI");
        let mut browsers = app
            .world_mut()
            .query_filtered::<(&PageMetadata, &ChildOf), With<LayoutBrowser>>();
        let metas: Vec<PageMetadata> = browsers
            .iter(app.world())
            .filter(|(_, child_of)| child_of.parent() == stack)
            .map(|(meta, _)| meta.clone())
            .collect();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].title, "Set up Vibe CLI");
        assert_eq!(metas[0].url, "vmux://sessions/vibe/setup");
    }

    #[test]
    fn missing_claude_or_codex_cli_shows_setup_page() {
        for (kind, segment) in [(AgentKind::Claude, "claude"), (AgentKind::Codex, "codex")] {
            let mut settings = test_settings();
            settings.agent.acp.clear();
            let mut app = App::new();
            let strategy = match kind {
                AgentKind::Claude => crate::host::cli::CLAUDE,
                AgentKind::Codex => crate::host::cli::CODEX,
                _ => unreachable!(),
            };
            app.world_mut().spawn(strategy);
            app.world_mut()
                .spawn(AgentExecutableOverride(std::collections::HashMap::from([
                    (kind, false),
                ])));
            app.add_plugins(MinimalPlugins)
                .add_message::<SpawnAgentInStackRequest>()
                .add_plugins(SpawnRequestsPlugin)
                .insert_resource(settings)
                .add_systems(Update, handle_agent_page_open.before(SpawnRequestSet));

            let stack = app.world_mut().spawn(stack_bundle()).id();
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: format!("vmux://sessions/{segment}/"),
                request_id: None,
            });

            app.update();
            app.update();

            let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
            assert_eq!(stack_meta.url, format!("vmux://sessions/{segment}/setup"));
            assert_eq!(
                stack_meta.title,
                format!("Set up {} CLI", kind.display_name())
            );
        }
    }

    #[test]
    fn legacy_registry_acp_url_opens_as_session() {
        use crate::acp_registry::{Distribution, RegistryAgent};

        let mut settings = test_settings();
        settings.agent.acp.clear();
        let mut app = App::new();
        app.world_mut().spawn(crate::runtime::acp::AcpCatalog {
            agents: vec![RegistryAgent {
                id: "custom-acp".to_string(),
                name: "Custom ACP".to_string(),
                version: None,
                description: None,
                icon: Some("https://cdn.example/custom.svg".to_string()),
                repository: None,
                distribution: Distribution::default(),
            }],
        });
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .add_systems(Update, handle_agent_page_open);

        let stack = app.world_mut().spawn(stack_bundle()).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://agent/custom".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let session = app.world().get::<AcpSession>(stack).unwrap();
        assert_eq!(session.agent_id, "custom");
        let meta = app.world().get::<PageMetadata>(stack).unwrap();
        assert_eq!(meta.url, "vmux://sessions/custom");
        assert_eq!(meta.title, "Custom ACP");
        assert_eq!(meta.icon.favicon_url(), "https://cdn.example/custom.svg");
    }

    #[test]
    fn canonical_and_legacy_setup_urls_attach_setup_page() {
        for url in ["vmux://sessions/codex/setup", "vmux://agent/codex/setup"] {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<SpawnAgentInStackRequest>()
                .insert_resource(test_settings())
                .add_systems(Update, handle_agent_page_open);

            let stack = app.world_mut().spawn(stack_bundle()).id();
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: url.to_string(),
                request_id: None,
            });

            app.update();
            app.update();

            let stack_meta = app.world().get::<PageMetadata>(stack).unwrap();
            assert_eq!(stack_meta.url, "vmux://sessions/codex/setup", "{url}");
            assert_eq!(stack_meta.title, "Set up Codex CLI", "{url}");
        }
    }

    #[test]
    fn first_local_agent_open_starts_in_the_repository_without_a_worktree() {
        let repo = init_worktree_test_repo();
        let mut settings = test_settings();
        settings.agent.acp.clear();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .add_systems(
                Update,
                (
                    release_agent_transition_paint,
                    prepare_agent_tab_worktrees,
                    drain_agent_tab_worktrees,
                    handle_agent_page_open,
                )
                    .chain(),
            );
        let project_dir = repo.path().canonicalize().unwrap();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Feature".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            })
            .id();
        let first_stack = app.world_mut().spawn(ChildOf(tab)).id();
        let first_task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack: first_stack,
                url: "vmux://sessions/claude/cli".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenDeferred>(first_task).is_none());
        assert!(app.world().get::<PageOpenHandled>(first_task).is_some());
        assert!(app.world().get::<TabWorktree>(tab).is_none());
        assert_eq!(
            app.world().get::<TabWorkspace>(tab).unwrap().project_dir,
            project_dir.to_string_lossy()
        );
        assert_eq!(worktree_list(repo.path()).unwrap().len(), 1);
        assert!(
            app.world()
                .get::<vmux_space::RepositoryNeedsWorktree>(tab)
                .is_some()
        );
        let first_spawns: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect();
        assert_eq!(first_spawns.len(), 1);
        assert_eq!(first_spawns[0].cwd, project_dir);

        let second_stack = app.world_mut().spawn(ChildOf(tab)).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack: second_stack,
            url: "vmux://sessions/codex/cli".to_string(),
            request_id: None,
        });
        app.update();

        assert_eq!(worktree_list(repo.path()).unwrap().len(), 1);
    }

    #[test]
    fn inline_open_starts_agent_before_worktree_creation() {
        let repo = init_worktree_test_repo();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(test_settings())
            .add_systems(
                Update,
                (
                    release_agent_transition_paint,
                    prepare_agent_tab_worktrees,
                    drain_agent_tab_worktrees,
                    handle_agent_page_open,
                )
                    .chain(),
            );
        let project_dir = repo.path().canonicalize().unwrap();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Feature".into(),
                startup_dir: Some(project_dir.to_string_lossy().into_owned()),
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((
                stack_bundle(),
                PendingPrompt("yo".to_string()),
                ChildOf(tab),
            ))
            .id();
        let start = app
            .world_mut()
            .spawn((
                LayoutBrowser::native_page("vmux://start/", "Start"),
                StartInlineTransitionView,
                ChildOf(stack),
            ))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(StartInlineTransition { webview: start });
        let first = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/claude".to_string(),
                request_id: None,
            })
            .id();
        let second = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/claude".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get_entity(start).is_ok());
        assert!(app.world().get::<PageOpenDeferred>(first).is_some());
        assert!(app.world().get::<PageOpenDeferred>(second).is_some());
        assert!(
            app.world()
                .get::<AwaitingAgentTransitionPaint>(first)
                .is_some()
        );
        assert!(
            app.world()
                .get::<AwaitingAgentTransitionPaint>(second)
                .is_some()
        );
        assert!(app.world().get::<AcpSession>(stack).is_none());
        assert!(
            app.world()
                .get::<vmux_space::RepositoryNeedsWorktree>(tab)
                .is_none()
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<ChatView>, With<PreparingAgentChatView>,)>()
                .iter(app.world())
                .collect::<Vec<_>>(),
            [start]
        );

        app.update();

        assert!(app.world().get::<PageOpenDeferred>(first).is_none());
        assert!(app.world().get::<PageOpenDeferred>(second).is_none());
        assert!(app.world().get::<PageOpenHandled>(first).is_some());
        assert!(app.world().get::<PageOpenHandled>(second).is_some());
        assert!(app.world().get::<AcpSession>(stack).is_some());
        assert!(
            app.world()
                .get::<vmux_space::RepositoryNeedsWorktree>(tab)
                .is_some()
        );
        assert_eq!(worktree_list(repo.path()).unwrap().len(), 1);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<ChatView>>()
                .iter(app.world())
                .collect::<Vec<_>>(),
            [start]
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<(), With<PreparingAgentChatView>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn inline_transition_opens_chat_when_tab_worktree_is_already_pending() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings())
            .add_systems(Update, prepare_agent_tab_worktrees);
        let workspace = TabWorkspace {
            project_dir: "/project".to_string(),
        };
        let metadata = TabWorktree {
            repo_root: "/project".to_string(),
            checkout_dir: "/project/.worktrees/feature".to_string(),
            branch: "feature".to_string(),
            base_ref: "main".to_string(),
        };
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Feature".into(),
                    startup_dir: Some(metadata.checkout_dir.clone()),
                },
                workspace.clone(),
                metadata.clone(),
            ))
            .id();
        let stack = app.world_mut().spawn((stack_bundle(), ChildOf(tab))).id();
        let start = app
            .world_mut()
            .spawn((
                LayoutBrowser::native_page("vmux://start/", "Start"),
                StartInlineTransitionView,
                ChildOf(stack),
            ))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(StartInlineTransition { webview: start });
        app.world_mut().spawn((
            PendingAgentWorktree {
                tab,
                tab_name: "Feature".into(),
                startup_dir: Some(metadata.checkout_dir.clone()),
                workspace,
                metadata,
                managed_root: PathBuf::from("/project/.worktrees"),
            },
            AgentWorktreeTask(
                IoTaskPool::get().spawn(async {
                    future::pending::<Result<TabWorktreeActivation, String>>().await
                }),
            ),
        ));
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/claude".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get_entity(start).is_ok());
        assert!(app.world().get::<PageOpenDeferred>(task).is_some());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<ChatView>, With<PreparingAgentChatView>,)>()
                .iter(app.world())
                .collect::<Vec<_>>(),
            [start]
        );
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
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .add_systems(
                Update,
                (
                    prepare_agent_tab_worktrees,
                    drain_agent_tab_worktrees,
                    handle_agent_page_open,
                )
                    .chain(),
            );
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Dashboard".into(),
                    startup_dir: Some(project_dir.to_string_lossy().into_owned()),
                },
                TabWorkspace {
                    project_dir: project_dir.to_string_lossy().into_owned(),
                },
                TabDirDecided,
            ))
            .id();
        let stack = app.world_mut().spawn(ChildOf(tab)).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude/cli".to_string(),
            request_id: None,
        });

        app.update();

        assert_eq!(worktree_list(repo.path()).unwrap().len(), 1);
        assert!(app.world().get::<TabWorktree>(tab).is_none());
        assert!(
            app.world()
                .get::<vmux_space::RepositoryNeedsWorktree>(tab)
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
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .add_systems(
                Update,
                (
                    prepare_agent_tab_worktrees,
                    drain_agent_tab_worktrees,
                    handle_agent_page_open,
                )
                    .chain(),
            );
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Existing".into(),
                startup_dir: Some(linked.to_string_lossy().into_owned()),
            })
            .id();
        let stack = app.world_mut().spawn(ChildOf(tab)).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude/cli".to_string(),
            request_id: None,
        });

        app.update();

        assert_eq!(worktree_list(repo.path()).unwrap().len(), 2);
        assert!(app.world().get::<TabWorktree>(tab).is_none());
        assert!(
            app.world()
                .get::<vmux_space::RepositoryNeedsWorktree>(tab)
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
            .insert_resource(ManagedWorktreeRoot(managed_root.path().to_path_buf()))
            .add_systems(Update, prepare_agent_tab_worktrees);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Browser".into(),
                startup_dir: Some(repo.path().to_string_lossy().into_owned()),
            })
            .id();
        let stack = app.world_mut().spawn(ChildOf(tab)).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "https://example.com".to_string(),
            request_id: None,
        });

        app.update();

        assert_eq!(worktree_list(repo.path()).unwrap().len(), 1);
        assert!(app.world().get::<TabWorktree>(tab).is_none());
    }

    #[test]
    fn agent_tab_without_workspace_starts_in_projects_without_binding_tab() {
        let mut settings = test_settings();
        settings.agent.acp.clear();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .add_systems(Update, handle_agent_page_open);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((
                stack_bundle(),
                PendingPrompt("Show me something fun in terminal".into()),
                ChildOf(tab),
            ))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/codex/cli".to_string(),
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
        assert_eq!(spawns[0].cwd, AgentCwd::projects().unwrap());
        assert_eq!(
            spawns[0].initial_prompt.as_deref(),
            Some("Show me something fun in terminal")
        );
        assert!(app.world().get::<TabWorkspace>(tab).is_none());
    }

    #[test]
    fn acp_tab_without_workspace_attaches_once_without_setup_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_agent_page_open);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((
                stack_bundle(),
                PendingPrompt("Show me something fun in terminal".into()),
                ChildOf(tab),
            ))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude".to_string(),
            request_id: None,
        });

        app.update();

        let session = app.world().get::<AcpSession>(stack).unwrap();
        assert_eq!(session.cwd, AgentCwd::projects().unwrap());
        assert_eq!(
            app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .items
                .front()
                .map(|item| item.text.as_str()),
            Some("Show me something fun in terminal")
        );
        assert!(app.world().get::<TabWorkspace>(tab).is_none());
        assert_eq!(
            app.world_mut()
                .query_filtered::<&ChildOf, With<ChatView>>()
                .iter(app.world())
                .filter(|child_of| child_of.parent() == stack)
                .count(),
            1
        );
    }

    #[test]
    fn inline_start_transition_navigates_the_launcher_view_and_keeps_the_prompt() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(test_settings())
            .add_systems(Update, handle_agent_page_open);
        let stack = app
            .world_mut()
            .spawn((
                stack_bundle(),
                PendingPrompt("keep this prompt".to_string()),
                PendingPromptAttachments(vec![AgentAttachment {
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
                LayoutBrowser,
                bevy_cef::prelude::WebviewSource::new("vmux://start/"),
                PageMetadata {
                    url: "vmux://start/".to_string(),
                    title: "Start".to_string(),
                    ..default()
                },
                StartInlineTransitionView,
                ChildOf(stack),
            ))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(StartInlineTransition { webview });
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude".to_string(),
            request_id: None,
        });

        app.update();

        assert!(app.world().get_entity(webview).is_ok());
        let mut views = app
            .world_mut()
            .query_filtered::<(Entity, &PageMetadata, &ChildOf), With<ChatView>>();
        let opened: Vec<_> = views.iter(app.world()).collect();
        let [(entity, meta, parent)] = opened.as_slice() else {
            panic!("expected exactly one chat view, got {}", opened.len());
        };
        assert_eq!(*entity, webview);
        assert_eq!(meta.url, "vmux://sessions/claude");
        assert_eq!(parent.parent(), stack);
        let queue = app.world().get::<PromptQueue>(stack).unwrap();
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
        assert!(app.world().get::<PendingPrompt>(stack).is_none());
        assert!(app.world().get::<PendingPromptAttachments>(stack).is_none());
        assert!(app.world().get::<StartInlineTransition>(stack).is_none());
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
            .add_systems(
                Update,
                (
                    prepare_agent_tab_worktrees,
                    drain_agent_tab_worktrees,
                    handle_agent_page_open,
                )
                    .chain(),
            );
        let stale = missing.to_string_lossy().into_owned();
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Tab 1".into(),
                    startup_dir: Some(stale.clone()),
                },
                TabWorkspace { project_dir: stale },
            ))
            .id();
        let stack = app.world_mut().spawn((stack_bundle(), ChildOf(tab))).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/codex".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        assert!(app.world().get::<PageOpenError>(task).is_none());
        assert_eq!(
            app.world().get::<AcpSession>(stack).unwrap().cwd,
            AgentCwd::projects().unwrap()
        );
        assert_eq!(app.world().get::<Tab>(tab).unwrap().startup_dir, None);
        assert!(app.world().get::<TabWorkspace>(tab).is_none());
    }

    #[test]
    fn fresh_claude_page_uses_space_startup_dir() {
        let dir = std::env::temp_dir().join(format!("vmux-startup-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut settings = test_settings();
        settings.agent.acp.clear();
        settings.spaces.insert(
            "space-1".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(dir.to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .add_systems(Update, handle_agent_page_open);
        app.world_mut().spawn((
            space_profile_bundle(&SpaceRecord {
                id: "space-1".into(),
                name: "Space 1".into(),
                profile: "Personal".into(),
            }),
            CurrentSpace,
        ));

        let stack = app.world_mut().spawn(stack_bundle()).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude/".to_string(),
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
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(active_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );
        settings.spaces.insert(
            "restored".into(),
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(restored_dir.path().to_string_lossy().into()),
                ..Default::default()
            },
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .add_systems(Update, handle_agent_page_open);
        app.world_mut().spawn((
            space_profile_bundle(&SpaceRecord {
                id: "active".into(),
                name: "Active".into(),
                profile: "Personal".into(),
            }),
            CurrentSpace,
        ));
        let space = app
            .world_mut()
            .spawn((Space, SpaceId("restored".into())))
            .id();
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Legacy".into(),
                    startup_dir: None,
                },
                ChildOf(space),
            ))
            .id();
        let stack = app.world_mut().spawn((stack_bundle(), ChildOf(tab))).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude/cli".to_string(),
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
            .add_systems(Update, handle_agent_page_open);
        let stack = app
            .world_mut()
            .spawn((stack_bundle(), PendingPrompt("fix the tests".to_string())))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/codex/cli".to_string(),
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
        let mut app = App::new();
        app.world_mut().spawn(crate::host::cli::CODEX);
        app.world_mut()
            .spawn(AgentExecutableOverride(std::collections::HashMap::from([
                (AgentKind::Codex, true),
            ])));
        app.add_plugins((
            MinimalPlugins,
            crate::host::cli::CliLaunchPlugin,
            SpawnPlugin,
        ))
        .add_message::<SpawnAgentInStackRequest>()
        .add_message::<crate::session::AgentSessionExited>()
        .add_message::<vmux_core::agent::RestartAgentPty>()
        .add_message::<vmux_core::agent::PageAgentAttachRequest>()
        .add_message::<vmux_core::agent::PageAgentSpawnStackRequest>()
        .add_message::<vmux_core::agent::PageAgentSpawnDefaultRequest>()
        .add_message::<vmux_core::agent::PageAgentAttachDefaultRequest>()
        .insert_resource(test_settings());
        let stack = app.world_mut().spawn(stack_bundle()).id();
        app.world_mut()
            .entity_mut(stack)
            .get_mut::<PageMetadata>()
            .unwrap()
            .url = "vmux://agent/codex/cli".to_string();
        app.world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .write(SpawnAgentInStackRequest {
                kind: AgentKind::Codex,
                cwd: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
                session_id: None,
                stack,
                initial_prompt: Some("@asdfas".to_string()),
                initial_attachments: Vec::new(),
            });

        for _ in 0..100 {
            app.update();
            if app
                .world_mut()
                .query_filtered::<Entity, With<Terminal>>()
                .iter(app.world())
                .next()
                .is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

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
            .add_systems(Update, handle_agent_page_open);
        let stack = app
            .world_mut()
            .spawn((stack_bundle(), PendingPrompt("ship it".to_string())))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude".to_string(),
            request_id: None,
        });

        app.update();

        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert_eq!(
            queue.items.front().map(|item| item.text.as_str()),
            Some("ship it")
        );
        assert_eq!(
            app.world().get::<AgentConversationTitle>(stack),
            Some(&AgentConversationTitle("ship it".into()))
        );
        assert!(app.world().get::<PendingPrompt>(stack).is_none());
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
            SpaceOverrides {
                startup_url: None,
                startup_dir: Some(space_dir.to_string_lossy().into()),
                ..Default::default()
            },
        );

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<SpawnAgentInStackRequest>()
            .insert_resource(settings)
            .add_systems(Update, handle_agent_page_open);
        app.world_mut().spawn((
            space_profile_bundle(&SpaceRecord {
                id: "space-1".into(),
                name: "Space 1".into(),
                profile: "Personal".into(),
            }),
            CurrentSpace,
        ));

        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "t".into(),
                startup_dir: Some(tab_dir.to_string_lossy().into()),
            })
            .id();
        let stack = app.world_mut().spawn((stack_bundle(), ChildOf(tab))).id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/claude/".to_string(),
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
            .add_systems(Update, handle_agent_page_open);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "t".into(),
                startup_dir: Some("/no/such/vmux-tab-workspace".into()),
            })
            .id();
        let stack = app.world_mut().spawn((stack_bundle(), ChildOf(tab))).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/claude/".to_string(),
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
            .add_systems(Update, handle_agent_page_open);

        let stack = app.world_mut().spawn(stack_bundle()).id();
        app.world_mut().spawn((
            ChildOf(stack),
            CoreAgentSession {
                kind: AgentKind::Vibe,
            },
        ));
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://sessions/vibe/".to_string(),
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
}
