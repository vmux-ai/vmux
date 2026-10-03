use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use vmux_api::protocol::AgentAttachment;
use vmux_chat::host::{ChatView, ImportedConversation, SessionManagerView};
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::host::persistence::PageRestore;
use vmux_ecs::terminal::TerminalLaunch;
use vmux_ecs::{
    Cwd, EntityTarget, PageIcon, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled,
    PageOpenSet, PageOpenTask, PendingPrompt, PendingPromptAttachments,
};
use vmux_layout::space::FocusedSpace;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::{PageOpenWaitForWorktree, TabWorktreePending, TabWorktreeReady};
use vmux_session::{
    AcpSessionId, AgentConversationTitle, AgentId, Cleanup, PromptQueue, Route, Session, SessionId,
};
use vmux_setting::AppSettings;
use vmux_start::{StartInlineTransition, StartInlineTransitionView};
use vmux_ui::i18n::translate;

use super::attach::AcpAgentAttachment;
use crate::host::acp::registry::RegistryAgent;
use vmux_terminal::AgentCwd;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

#[derive(SystemParam)]
struct AcpCatalog<'w, 's> {
    agents: Query<'w, 's, &'static RegistryAgent>,
}

impl AcpCatalog<'_, '_> {
    fn agent(&self, id: &str) -> Option<&RegistryAgent> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    fn installed_id(&self) -> Option<String> {
        self.agents
            .iter()
            .find(|agent| agent.is_installed())
            .map(|agent| agent.id.clone())
    }

    fn icon(&self, id: &str) -> Option<String> {
        self.agent(id).and_then(|agent| agent.icon.clone())
    }

    fn profile_name(&self, id: &str, config: Option<&vmux_setting::AcpAgentConfig>) -> String {
        self.agent(id)
            .map(|agent| agent.name.trim())
            .filter(|name| !name.is_empty())
            .or_else(|| {
                let name = config?.name.trim();
                (!name.is_empty()).then_some(name)
            })
            .unwrap_or(id)
            .to_string()
    }
}

pub struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(cleanup)
            .add_systems(Update, swap)
            .add_systems(
                Update,
                (release_transition, prepare, open)
                    .chain()
                    .in_set(PageOpenSet::HandleKnownPages),
            );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentPageOpenWorkspace<'w, 's> {
    active_space: FocusedSpace<'w, 's>,
    child_of: Query<'w, 's, &'static ChildOf>,
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

    fn tab(&self, entity: Entity) -> Option<(Entity, Option<String>)> {
        let mut current = entity;
        loop {
            if let Ok(tab) = self.tabs.get(current) {
                return Some((current, tab.startup_dir.clone()));
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }
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
    let mut opened_stacks = std::collections::HashSet::new();
    for (task_entity, task) in &tasks {
        let Some(route) = Route::parse(&task.url) else {
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
                    vmux_ecs::launcher::HostsLauncher,
                    vmux_ecs::page::PageReady,
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
        if matches!(route, Route::Manager) {
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
            let current_dir = tab
                .startup_dir
                .as_deref()
                .map(Path::new)
                .and_then(|path| path.canonicalize().ok());
            let needs_worktree = decided.is_none()
                && !current_dir
                    .as_deref()
                    .is_some_and(vmux_git::worktree::CheckoutInfo::is_linked)
                && vmux_git::worktree::CheckoutInfo::try_from(Path::new(&workspace.project_dir))
                    .is_ok();
            let mut entity = commands.entity(tab_entity);
            if needs_worktree {
                entity.insert(vmux_space::RepositoryNeedsWorktree);
            } else {
                entity.remove::<vmux_space::RepositoryNeedsWorktree>();
            }
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
        Query<(Entity, &PageOpenTask, Has<PageRestore>), PendingPageOpen>,
        Query<(&PendingPrompt, Option<&PendingPromptAttachments>)>,
    )>,
    workspace: AgentPageOpenWorkspace,
    mut opener: PageOpener,
    transitions: Query<&StartInlineTransition>,
    launches: Query<&TerminalLaunch>,
) {
    let tasks: Vec<(Entity, PageOpenTask, bool)> = open_q
        .p0()
        .iter()
        .map(|(entity, task, restoring)| (entity, task.clone(), restoring))
        .collect();
    for (entity, task, restoring) in tasks {
        let Some(route) = Route::parse(&task.url) else {
            continue;
        };
        let default_cwd = if matches!(route, Route::Manager) {
            PathBuf::new()
        } else {
            let tab = workspace.tab(task.stack);
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
                match AgentCwd::from_tab(tab_dir.as_deref()).stored() {
                    Ok(Some(path)) => path,
                    Ok(None) => match space_startup_dir {
                        Some(dir) => dir.path,
                        None => match AgentCwd::projects() {
                            Ok(path) => path,
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
            &task,
            initial_prompt,
            initial_attachments,
            transition_webview,
            &default_cwd,
        ) {
            Ok(()) => {
                opener.commands.entity(entity).insert(PageOpenHandled);
                if let Some(webview) = transition_webview {
                    opener
                        .commands
                        .entity(webview)
                        .remove::<PreparingAgentChatView>();
                }
                opener
                    .commands
                    .entity(task.stack)
                    .remove::<StartInlineTransition>();
            }
            Err(message) => {
                opener
                    .commands
                    .entity(entity)
                    .insert(PageOpenError { message });
            }
        }
    }
}

fn swap(
    mut reader: MessageReader<SwapStackSession>,
    settings: Res<AppSettings>,
    catalog: AcpCatalog,
    targets: Query<&EntityTarget<Session>>,
    sessions: Query<(&SessionId, &Name, &Cwd), With<Session>>,
    mut commands: Commands,
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

        SessionRuntime::detach(&mut commands.entity(session_entity));
        commands.entity(ev.stack).despawn_children();
        let cwd = if ev.cwd.as_os_str().is_empty() {
            cwd.0.clone()
        } else {
            ev.cwd.clone()
        };
        commands
            .entity(session_entity)
            .insert((AgentId(id.to_string()), Cwd(cwd.clone())));
        let cfg = settings.agent.acp.iter().find(|cfg| cfg.id == id);
        let icon = catalog.icon(id);
        let agent_name = catalog.profile_name(id, cfg);
        commands.spawn(AcpAgentAttachment::new(
            session_entity,
            ev.stack,
            id,
            session_name.as_str(),
            agent_name,
            session_id.0.clone(),
            cwd,
            icon,
            None,
        ));
        if let Some((imported, pending)) = imported {
            commands.entity(session_entity).insert((imported, pending));
        }
    }
}

struct SessionRuntime;

impl SessionRuntime {
    fn detach(entity: &mut EntityCommands) {
        entity.remove::<(
            AcpSessionId,
            vmux_ecs::ProcessAnchor,
            crate::host::acp::AcpLaunchStarted,
            crate::host::runtime::AcpSessionConfigState,
            crate::host::run_state_kind::LastRunStateKind,
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

    fn cleanup(entity: &mut EntityCommands) {
        Self::detach(entity);
        entity.remove::<PromptQueue>();
    }
}

fn cleanup(
    trigger: On<Cleanup>,
    stacks: Query<(Entity, &EntityTarget<Session>)>,
    mut opens: MessageWriter<vmux_ecs::PageOpenRequest>,
    mut commands: Commands,
) {
    let session = trigger.event_target();
    SessionRuntime::cleanup(&mut commands.entity(session));
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

#[derive(SystemParam)]
struct PageOpener<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            Entity,
            &'static SessionId,
            &'static Name,
            &'static Cwd,
            Option<&'static AgentId>,
            Option<&'static AcpSessionId>,
            Option<&'static vmux_ecs::ProcessAnchor>,
        ),
        With<Session>,
    >,
    children: Query<'w, 's, &'static Children>,
    queues: Query<'w, 's, &'static mut PromptQueue>,
    commands: Commands<'w, 's>,
    settings: Res<'w, AppSettings>,
    catalog: AcpCatalog<'w, 's>,
}

impl PageOpener<'_, '_> {
    fn apply(
        &mut self,
        task: &PageOpenTask,
        initial_prompt: Option<String>,
        initial_attachments: Vec<AgentAttachment>,
        transition_webview: Option<Entity>,
        default_cwd: &Path,
    ) -> Result<(), String> {
        let target = match Route::parse(&task.url) {
            Some(target) => target,
            None => return Err(format!("malformed agent URL '{}'", task.url)),
        };
        match target {
            Route::Manager => {
                let url = Route::requested_agent(&task.url)
                    .map(|agent| Route::manager_for_agent(&agent))
                    .unwrap_or_else(|| Route::Manager.url());
                self.open_manager(task.stack, transition_webview, &url);
                Ok(())
            }
            Route::Session(id) => {
                let Some((
                    session_entity,
                    session_name,
                    stored_cwd,
                    selected_agent,
                    resume,
                    active,
                )) = self
                    .sessions
                    .iter()
                    .find(|(_, candidate, ..)| *candidate == &id)
                    .map(|(entity, _, name, cwd, agent, resume, active)| {
                        (
                            entity,
                            name.as_str().to_string(),
                            cwd.0.clone(),
                            agent.map(|agent| agent.0.clone()),
                            resume.map(|resume| resume.0.clone()),
                            active.map(|active| active.0),
                        )
                    })
                else {
                    return Err(format!("Session '{}' was not found", id.0));
                };
                let agent_id = selected_agent
                    .or_else(|| {
                        self.settings
                            .agent
                            .acp
                            .first()
                            .map(|config| config.id.clone())
                    })
                    .or_else(|| self.catalog.installed_id())
                    .ok_or_else(|| "no ACP agent is configured or installed".to_string())?;
                let config = self
                    .settings
                    .agent
                    .acp
                    .iter()
                    .find(|config| config.id == agent_id);
                if config.is_none() && self.catalog.agent(&agent_id).is_none() {
                    return Err(format!("ACP agent unavailable for '{agent_id}'"));
                }
                let icon = self.catalog.icon(&agent_id);
                let agent_name = self.catalog.profile_name(&agent_id, config);
                let cwd = if stored_cwd.as_os_str().is_empty() {
                    self.commands
                        .entity(session_entity)
                        .insert(Cwd(default_cwd.to_path_buf()));
                    default_cwd.to_path_buf()
                } else {
                    stored_cwd
                };
                self.commands.entity(task.stack).insert((
                    EntityTarget::<Session>::new(session_entity),
                    PageMetadata {
                        url: Route::Session(id.clone()).url(),
                        title: session_name.clone(),
                        bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
                        icon: icon.clone().map(PageIcon::favicon).unwrap_or_default(),
                    },
                ));
                self.replace_children(task.stack, transition_webview);
                self.enqueue_initial_prompt(
                    session_entity,
                    task.stack,
                    initial_prompt,
                    initial_attachments,
                );
                if let Some(anchor) = active {
                    self.open_session_view(
                        task.stack,
                        session_entity,
                        &session_name,
                        &agent_id,
                        anchor,
                        transition_webview,
                    );
                    return Ok(());
                }
                let request = AcpAgentAttachment::new(
                    session_entity,
                    task.stack,
                    agent_id.clone(),
                    &session_name,
                    agent_name,
                    id.0.clone(),
                    cwd,
                    icon,
                    resume,
                );
                self.commands
                    .entity(session_entity)
                    .insert(AgentId(agent_id));
                let operation = self.commands.spawn(request).id();
                if let Some(webview) = transition_webview {
                    self.commands
                        .entity(operation)
                        .insert(EntityTarget::<ChatView>::new(webview));
                }
                Ok(())
            }
        }
    }

    fn open_manager(&mut self, stack: Entity, transition_webview: Option<Entity>, url: &str) {
        let title = translate("sessions-title");
        self.replace_children(stack, transition_webview);
        self.commands
            .entity(stack)
            .remove::<EntityTarget<Session>>()
            .remove::<vmux_command::CommandBarWorkDirectory>()
            .insert(PageMetadata {
                url: url.to_string(),
                title: title.clone(),
                bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
                icon: default(),
            });
        let view = transition_webview.unwrap_or_else(|| {
            self.commands
                .spawn((
                    vmux_layout::Browser::native_page(url, &title),
                    SessionManagerView,
                    ChildOf(stack),
                ))
                .id()
        });
        self.commands.entity(view).remove::<ChatView>().insert((
            PageMetadata {
                url: url.to_string(),
                title,
                bg_color: None,
                icon: default(),
            },
            SessionManagerView,
        ));
    }

    fn open_session_view(
        &mut self,
        stack: Entity,
        session: Entity,
        name: &str,
        agent_id: &str,
        anchor: vmux_ecs::ProcessId,
        transition_webview: Option<Entity>,
    ) {
        let url = self
            .sessions
            .get(session)
            .map(|(_, id, ..)| Route::Session(id.clone()).url())
            .unwrap_or_else(|_| Route::Manager.url());
        let view = transition_webview.unwrap_or_else(|| {
            self.commands
                .spawn((
                    vmux_layout::Browser::native_page(&url, name),
                    ChatView,
                    ChildOf(stack),
                    anchor,
                ))
                .id()
        });
        self.commands.entity(view).insert((
            PageMetadata {
                url,
                title: name.to_string(),
                bg_color: None,
                icon: self
                    .catalog
                    .icon(agent_id)
                    .map(PageIcon::favicon)
                    .unwrap_or_default(),
            },
            ChatView,
            anchor,
        ));
        self.commands.entity(view).remove::<SessionManagerView>();
        self.commands
            .entity(stack)
            .insert(EntityTarget::<Session>::new(session));
    }

    fn replace_children(&mut self, stack: Entity, keep: Option<Entity>) {
        let Ok(children) = self.children.get(stack) else {
            return;
        };
        for child in children.iter() {
            if Some(child) != keep {
                self.commands.entity(child).despawn();
            }
        }
    }

    fn enqueue_initial_prompt(
        &mut self,
        session: Entity,
        stack: Entity,
        initial_prompt: Option<String>,
        initial_attachments: Vec<AgentAttachment>,
    ) {
        let prompt = initial_prompt.unwrap_or_default();
        if prompt.trim().is_empty() && initial_attachments.is_empty() {
            return;
        }
        if let Some(title) = AgentConversationTitle::from_prompt(&prompt) {
            self.commands.entity(session).insert(title);
        }
        if let Ok(mut queue) = self.queues.get_mut(session) {
            queue.enqueue_with_attachments(prompt, initial_attachments);
        } else {
            let mut queue = PromptQueue::default();
            queue.enqueue_with_attachments(prompt, initial_attachments);
            self.commands.entity(session).insert(queue);
        }
        self.commands
            .entity(stack)
            .remove::<(PendingPrompt, PendingPromptAttachments)>();
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
    fn manager_target_preserves_requested_agent() {
        let url = Route::manager_for_agent(&AgentId("codex".into()));
        let target = AgentChatTarget::parse(&url).unwrap();

        assert_eq!(target.url, url);
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
                crate::host::run_state_kind::LastRunStateKind::default(),
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
            .run_system_once(move |mut commands: Commands| {
                SessionRuntime::cleanup(&mut commands.entity(session));
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
                .get::<crate::host::run_state_kind::LastRunStateKind>(session)
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
            .run_system_once(move |mut commands: Commands| {
                SessionRuntime::detach(&mut commands.entity(session));
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
