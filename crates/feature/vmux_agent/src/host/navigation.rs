use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_chat::host::{ChatView, ImportedConversation};
use vmux_ecs::agent::SwapStackSession;
use vmux_ecs::persistence::PageRestore;
use vmux_ecs::profile::Projects;
use vmux_ecs::terminal::TerminalLaunch;
use vmux_ecs::{
    Cwd, EntityTarget, PageIcon, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled,
    PageOpenSet, PageOpenTask, PendingPrompt, PendingPromptAttachments,
};
use vmux_layout::space::FocusedSpace;
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::{PageOpenWaitForWorktree, TabWorktreePending, TabWorktreeReady};
use vmux_session::AcpSession;
use vmux_setting::AppSettings;
use vmux_start::{StartInlineTransition, StartInlineTransitionView};
use vmux_ui::i18n::translate;

use super::attach::AcpAgentAttachment;
use super::navigation_driver::{AcpCatalog, AgentPageOpenWorkspace, PageOpener};
#[cfg(test)]
use crate::host::acp::registry::RegistryAgent;
use vmux_terminal::AgentCwd;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

pub(super) fn add(app: &mut App) {
    app.add_systems(Update, swap).add_systems(
        Update,
        (release_transition, prepare, open)
            .chain()
            .in_set(PageOpenSet::HandleKnownPages),
    );
}

#[derive(Component)]
struct PreparingAgentChatView;

#[derive(Component)]
struct AwaitingAgentTransitionPaint;

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
        if Route::parse(&task.url).is_none() {
            continue;
        }
        let Some(tab_entity) = ancestor_tab_entity(task.stack, &child_of, &tabs) else {
            continue;
        };
        if !preparing_by_stack.contains_key(&task.stack)
            && let Ok(transition) = transitions.get(task.stack)
            && let Some(target) = AcpRoute::parse(&task.url)
        {
            let (url, title) = match target {
                AcpRoute::AcpDefault => (
                    vmux_api::VmuxRoute::SESSIONS_ROOT.to_string(),
                    "Agent".to_string(),
                ),
                AcpRoute::Acp { id, sid } => {
                    let url = match sid {
                        Some(sid) => {
                            format!("{}{id}/{sid}", vmux_api::VmuxRoute::SESSIONS_ROOT)
                        }
                        None => format!("{}{id}", vmux_api::VmuxRoute::SESSIONS_ROOT),
                    };
                    (url, id.to_string())
                }
            };
            let view = transition.webview;
            commands
                .entity(view)
                .insert((
                    PageMetadata {
                        url,
                        title,
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
    projects: Projects,
) {
    let tasks: Vec<(Entity, PageOpenTask, bool)> = open_q
        .p0()
        .iter()
        .map(|(entity, task, restoring)| (entity, task.clone(), restoring))
        .collect();
    for (entity, task, restoring) in tasks {
        if Route::parse(&task.url).is_none() {
            continue;
        }
        let tab = workspace.tab(task.stack);
        let tab_dir = tab
            .as_ref()
            .and_then(|(_, startup_dir)| startup_dir.clone());
        let space_startup_dir = workspace.startup_dir(task.stack, &opener.settings);
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

        commands
            .entity(session_entity)
            .remove::<AcpSessionId>()
            .remove::<vmux_ecs::ProcessAnchor>()
            .remove::<crate::host::acp::AcpLaunchStarted>()
            .remove::<vmux_session::ApprovalPolicy>()
            .remove::<vmux_session::RunState>()
            .remove::<ImportedConversation>()
            .remove::<super::handoff::PendingHandoff>()
            .remove::<vmux_ecs::team::Agent>()
            .remove::<vmux_ecs::team::Profile>();
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
}
