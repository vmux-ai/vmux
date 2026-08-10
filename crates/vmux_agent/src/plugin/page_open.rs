//! Opening a `vmux://agent/...` URL: settling where it runs before anything is spawned.
//!
//! The URL alone does not say which directory the agent gets. That is resolved first — from the
//! tab's bound workspace, its managed worktree, or the space's startup dir — because the answer
//! decides whether the page attaches, shows a setup card, or waits for a worktree to be built.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::{CefKeyboardTarget, WebviewExtendStandardMaterial};
use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
use vmux_core::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_service::protocol::AgentAttachment;
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;

use crate::session::AgentSessionToEntity;

use super::attach::{
    acp_icon_for_id, acp_profile_name_for_id, acp_registry_agent_for_id, attach_acp_agent_to_stack,
    attach_acp_agent_to_stack_with_webview, attach_page_agent_to_stack_with_webview,
};
use super::run_terminal::{process_cwd, stored_tab_cwd};
use super::spawn::PendingPageOpen;

pub(super) struct PageOpenPlugin;

impl Plugin for PageOpenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                handle_swap_stack_session.before(super::spawn::handle_spawn_agent_requests),
                prepare_agent_tab_worktrees
                    .in_set(PageOpenSet::HandleKnownPages)
                    .before(handle_agent_page_open),
                handle_agent_page_open.in_set(PageOpenSet::HandleKnownPages),
            ),
        );
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentPageOpenWorkspace<'w, 's> {
    active_space: Option<Res<'w, ActiveSpace>>,
    tabs: Query<'w, 's, &'static vmux_layout::tab::Tab>,
    spaces: Query<'w, 's, (), With<vmux_layout::space::Space>>,
    space_ids: Query<'w, 's, &'static vmux_layout::space::SpaceId>,
}

pub(crate) fn agent_url_uses_local_workspace(url: &str) -> bool {
    if AgentKind::all()
        .into_iter()
        .any(|kind| url == kind.setup_url())
    {
        return false;
    }
    crate::AgentUrl::parse(url).is_some()
}

pub(crate) fn ancestor_tab_entity(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<(
        Entity,
        &mut vmux_layout::tab::Tab,
        Option<&vmux_layout::tab::TabWorkspace>,
        Option<&vmux_layout::tab::TabWorktree>,
        Option<&vmux_layout::worktree::TabWorktreeReady>,
        Option<&vmux_layout::tab::TabDirDecided>,
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

pub(crate) fn ancestor_agent_tab(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&vmux_layout::tab::Tab>,
) -> Option<(Entity, Option<String>)> {
    let mut current = entity;
    loop {
        if let Ok(tab) = tabs.get(current) {
            return Some((current, tab.startup_dir.clone()));
        }
        current = child_of.get(current).ok()?.parent();
    }
}

pub(crate) fn resolved_space_startup_dir(
    entity: Entity,
    child_of: &Query<&ChildOf>,
    spaces: &Query<(), With<vmux_layout::space::Space>>,
    space_ids: &Query<&vmux_layout::space::SpaceId>,
    settings: &AppSettings,
    active_space: Option<&ActiveSpace>,
) -> Option<(PathBuf, vmux_setting::DirSource)> {
    let space_id = vmux_layout::space::space_id_of(entity, child_of, spaces, space_ids)
        .or_else(|| active_space.map(|space| space.record.id.clone()))?;
    vmux_setting::resolve_startup_dir_for_tab_with_source(settings, &space_id, None)
}

fn prepare_agent_tab_worktrees(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    child_of: Query<&ChildOf>,
    spaces: Query<(), With<vmux_layout::space::Space>>,
    space_ids: Query<&vmux_layout::space::SpaceId>,
    mut tabs: Query<(
        Entity,
        &mut vmux_layout::tab::Tab,
        Option<&vmux_layout::tab::TabWorkspace>,
        Option<&vmux_layout::tab::TabWorktree>,
        Option<&vmux_layout::worktree::TabWorktreeReady>,
        Option<&vmux_layout::tab::TabDirDecided>,
    )>,
    settings: Option<Res<AppSettings>>,
    active_space: Option<Res<ActiveSpace>>,
    managed_root: Option<Res<vmux_layout::worktree::ManagedWorktreeRoot>>,
    mut commands: Commands,
) {
    let managed_root = managed_root.as_deref().cloned().unwrap_or_default().0;
    let mut outcomes: std::collections::HashMap<Entity, Result<(), String>> =
        std::collections::HashMap::new();
    for (task_entity, task) in &tasks {
        if !agent_url_uses_local_workspace(&task.url) {
            continue;
        }
        let Some(tab_entity) = ancestor_tab_entity(task.stack, &child_of, &tabs) else {
            continue;
        };
        let configured_project_dir = settings.as_deref().and_then(|settings| {
            resolved_space_startup_dir(
                task.stack,
                &child_of,
                &spaces,
                &space_ids,
                settings,
                active_space.as_deref(),
            )
            .map(|(path, _)| path.to_string_lossy().into_owned())
        });
        let outcome = if let Some(outcome) = outcomes.get(&tab_entity) {
            outcome.clone()
        } else {
            let outcome = match tabs.get_mut(tab_entity) {
                Err(_) => Ok(()),
                Ok((_, mut tab, workspace, metadata, ready, decided)) => {
                    let has_workspace = workspace.is_some();
                    let workspace = workspace.cloned().unwrap_or_else(|| {
                        let project_dir = metadata
                            .map(|metadata| metadata.repo_root.clone())
                            .filter(|path| !path.is_empty())
                            .or_else(|| tab.startup_dir.clone())
                            .or_else(|| configured_project_dir.clone())
                            .unwrap_or_default();
                        vmux_layout::tab::TabWorkspace { project_dir }
                    });
                    if workspace.project_dir.is_empty() {
                        Ok(())
                    } else if metadata.is_none()
                        && stored_tab_cwd(tab.startup_dir.as_deref())
                            .ok()
                            .flatten()
                            .is_none()
                        && stored_tab_cwd(Some(&workspace.project_dir))
                            .ok()
                            .flatten()
                            .is_none()
                    {
                        tab.startup_dir = None;
                        commands
                            .entity(tab_entity)
                            .remove::<vmux_layout::tab::TabWorkspace>()
                            .remove::<vmux_layout::tab::TabDirDecided>()
                            .remove::<vmux_layout::tab::TabWorktreeUnavailable>();
                        Ok(())
                    } else {
                        if !has_workspace {
                            commands.entity(tab_entity).insert(workspace.clone());
                        }
                        let result = if let Some(metadata) = metadata {
                            if ready
                                .is_some_and(|ready| ready.is_current(&tab, &workspace, metadata))
                            {
                                Ok(())
                            } else {
                                vmux_layout::worktree::ensure_tab_worktree_available(
                                    &tab,
                                    &workspace,
                                    metadata,
                                    &managed_root,
                                )
                                .map(|activation| {
                                    tab.startup_dir = Some(
                                        activation.execution_dir.to_string_lossy().into_owned(),
                                    );
                                    let mut entity = commands.entity(tab_entity);
                                    if metadata != &activation.metadata {
                                        entity.insert(activation.metadata);
                                    }
                                    entity.insert(activation.ready);
                                })
                            }
                        } else if decided.is_some() {
                            Ok(())
                        } else {
                            let current_dir = tab
                                .startup_dir
                                .as_deref()
                                .map(Path::new)
                                .and_then(|path| path.canonicalize().ok());
                            if current_dir
                                .as_deref()
                                .is_some_and(vmux_git::worktree::is_linked_worktree)
                            {
                                Ok(())
                            } else {
                                let project_dir = PathBuf::from(&workspace.project_dir);
                                if vmux_git::worktree::checkout_info(&project_dir).is_err() {
                                    Ok(())
                                } else {
                                    let slug_hint = vmux_layout::worktree::tab_worktree_slug_hint(
                                        &tab.name,
                                        &project_dir,
                                    );
                                    vmux_layout::worktree::create_worktree_blocking(
                                        &project_dir,
                                        &slug_hint,
                                        &managed_root,
                                    )
                                    .map(|activation| {
                                        tab.startup_dir = Some(
                                            activation.execution_dir.to_string_lossy().into_owned(),
                                        );
                                        commands.entity(tab_entity).insert((
                                            activation.metadata,
                                            activation.ready,
                                            vmux_layout::tab::TabDirDecided,
                                        ));
                                    })
                                }
                            }
                        };
                        match result {
                            Ok(()) => {
                                commands
                                    .entity(tab_entity)
                                    .remove::<vmux_layout::tab::TabWorktreeUnavailable>();
                                Ok(())
                            }
                            Err(message) => {
                                commands
                                    .entity(tab_entity)
                                    .insert(vmux_layout::tab::TabWorktreeUnavailable {
                                        message: message.clone(),
                                    })
                                    .remove::<vmux_layout::worktree::TabWorktreeReady>();
                                Err(message)
                            }
                        }
                    }
                }
            };
            outcomes.insert(tab_entity, outcome.clone());
            outcome
        };
        if let Err(message) = outcome {
            commands
                .entity(task_entity)
                .insert(PageOpenError { message });
        }
    }
}

fn handle_agent_page_open(
    mut open_q: ParamSet<(
        Query<(Entity, &PageOpenTask), PendingPageOpen>,
        Query<(
            &vmux_core::PendingPrompt,
            Option<&vmux_core::PendingPromptAttachments>,
        )>,
    )>,
    children_q: Query<&Children>,
    agents: Query<&vmux_core::agent::AgentSession>,
    acp_sessions: Query<&crate::client::acp::AcpSession>,
    child_of_q: Query<&ChildOf>,
    agent_to_entity: Option<Res<AgentSessionToEntity>>,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    settings: Res<AppSettings>,
    workspace: AgentPageOpenWorkspace,
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
    transitions: Query<&vmux_layout::start::StartInlineTransition>,
) {
    let tasks: Vec<(Entity, PageOpenTask)> = open_q
        .p0()
        .iter()
        .map(|(entity, task)| (entity, task.clone()))
        .collect();
    for (entity, task) in tasks {
        if !task.url.starts_with("vmux://agent/") {
            continue;
        }
        let tab = ancestor_agent_tab(task.stack, &child_of_q, &workspace.tabs);
        let tab_dir = tab
            .as_ref()
            .and_then(|(_, startup_dir)| startup_dir.clone());
        let space_startup_dir = resolved_space_startup_dir(
            task.stack,
            &child_of_q,
            &workspace.spaces,
            &workspace.space_ids,
            &settings,
            workspace.active_space.as_deref(),
        );
        let default_cwd = match stored_tab_cwd(tab_dir.as_deref()) {
            Ok(Some(path)) => path,
            Ok(None) => space_startup_dir
                .map(|(path, _)| path)
                .unwrap_or_else(process_cwd),
            Err(message) => {
                commands.entity(entity).insert(PageOpenError { message });
                continue;
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
            .filter(|_| vmux_layout::start::supports_inline_agent_transition(&task.url));
        match handle_agent_page_open_task(
            &task,
            initial_prompt,
            initial_attachments,
            transition_webview,
            &children_q,
            &agents,
            &acp_sessions,
            &child_of_q,
            agent_to_entity.as_deref(),
            idx.as_deref(),
            &kind_q,
            &mut spawn_agent,
            &mut commands,
            &mut meshes,
            &mut webview_mt,
            &default_cwd,
            &settings.agent.acp,
            catalog.as_deref(),
        ) {
            Ok(()) => {
                commands.entity(entity).insert(PageOpenHandled);
                commands
                    .entity(task.stack)
                    .remove::<vmux_layout::start::StartInlineTransition>();
            }
            Err(message) => {
                commands.entity(entity).insert(PageOpenError { message });
            }
        }
    }
}

/// Swap the agent session on a stack in place (see [`vmux_core::agent::SwapStackSession`]).
/// Tears down the current session's stack-level components + panes, then re-attaches the
/// target runtime with an explicit cwd — the shared path for `/resume` and the ACP↔CLI
/// handoff. Unlike the page-open path this always re-attaches (no same-id no-op) and never
/// falls back to `default_cwd`.
fn handle_swap_stack_session(
    mut reader: MessageReader<vmux_core::agent::SwapStackSession>,
    settings: Res<AppSettings>,
    catalog: Option<Res<crate::client::acp::AcpCatalog>>,
    children_q: Query<&Children>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
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
                .any(|cfg| crate::acp_install::agent_ids_match(&cfg.id, id))
            && acp_registry_agent_for_id(catalog.as_deref(), id).is_none()
        {
            bevy::log::warn!("swap: ACP agent unavailable for '{id}'");
            continue;
        }
        if ev.handoff.is_some() && !matches!(target, crate::AgentUrl::Acp { .. }) {
            bevy::log::warn!("swap: cross-agent handoff requires an ACP target");
            continue;
        }
        let imported = match ev.handoff.as_ref() {
            Some(handoff) => {
                let Ok(messages) =
                    serde_json::from_str::<Vec<crate::Message>>(&handoff.messages_json)
                else {
                    bevy::log::warn!("swap: invalid handoff transcript");
                    continue;
                };
                Some((
                    crate::handoff::ImportedConversation {
                        source_agent: handoff.source_agent.clone(),
                        source_kind: handoff.source_kind,
                        source_sid: handoff.source_sid.clone(),
                        messages,
                        truncated: handoff.truncated,
                        first_prompt: None,
                    },
                    crate::handoff::PendingHandoff {
                        context: handoff.context.clone(),
                        sent: false,
                    },
                ))
            }
            None => None,
        };

        // Removing AcpSession fires close_acp_session_on_remove → the daemon session is closed.
        // Children (the Browser/terminal pane) are despawned; a CLI terminal despawn kills its
        // PTY. Stack-level removes are no-ops for a CLI stack (its agent components live on the
        // terminal child).
        commands
            .entity(ev.stack)
            .remove::<crate::client::acp::AcpSession>()
            .remove::<crate::client::acp::AcpInstallStarted>()
            .remove::<crate::components::AgentSession>()
            .remove::<crate::AgentMessages>()
            .remove::<crate::AgentApprovalPolicy>()
            .remove::<crate::AgentRunState>()
            .remove::<crate::handoff::ImportedConversation>()
            .remove::<crate::handoff::PendingHandoff>()
            .remove::<vmux_core::AgentWorkingDir>()
            .remove::<vmux_core::team::Agent>()
            .remove::<vmux_core::team::Profile>();
        clear_stack_children(ev.stack, &children_q, &mut commands);

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
                    .find(|cfg| crate::acp_install::agent_ids_match(&cfg.id, &id));
                let routing_sid = uuid::Uuid::new_v4().to_string();
                let icon = acp_icon_for_id(catalog.as_deref(), &id);
                let name = acp_profile_name_for_id(&id, cfg, catalog.as_deref());
                attach_acp_agent_to_stack(
                    ev.stack,
                    &id,
                    &name,
                    &routing_sid,
                    &ev.cwd,
                    icon.as_deref(),
                    sid.as_deref(),
                    &mut commands,
                    &mut meshes,
                    &mut webview_mt,
                );
                if let Some((imported, pending)) = imported {
                    commands.entity(ev.stack).insert((imported, pending));
                }
            }
            _ => unreachable!(),
        }
    }
}

pub(crate) fn handle_agent_page_open_task(
    task: &PageOpenTask,
    initial_prompt: Option<String>,
    initial_attachments: Vec<AgentAttachment>,
    transition_webview: Option<Entity>,
    children_q: &Query<&Children>,
    agents: &Query<&vmux_core::agent::AgentSession>,
    acp_sessions: &Query<&crate::client::acp::AcpSession>,
    child_of_q: &Query<&ChildOf>,
    agent_to_entity: Option<&AgentSessionToEntity>,
    idx: Option<&crate::client::page::strategy_index::PageStrategyIndex>,
    kind_q: &Query<&crate::client::page::strategy_components::StrategyKind>,
    spawn_agent: &mut MessageWriter<SpawnAgentInStackRequest>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
    default_cwd: &std::path::Path,
    acp_configs: &[vmux_setting::AcpAgentConfig],
    catalog: Option<&crate::client::acp::AcpCatalog>,
) -> Result<(), String> {
    if let Some(kind) = AgentKind::all()
        .into_iter()
        .find(|k| task.url == k.setup_url())
    {
        attach_cli_setup_to_stack(kind, task.stack, children_q, commands, meshes, webview_mt);
        return Ok(());
    }
    match crate::AgentUrl::parse(&task.url) {
        Some(crate::AgentUrl::Page {
            provider,
            model,
            sid,
        }) => {
            if transition_webview.is_none() {
                clear_stack_children(task.stack, children_q, commands);
            }
            let idx = idx.ok_or_else(|| "page strategy index not registered".to_string())?;
            attach_page_agent_to_stack_with_webview(
                task.stack,
                &provider,
                &model,
                &sid,
                transition_webview,
                commands,
                meshes,
                webview_mt,
                idx,
                kind_q,
            )
            .ok_or_else(|| format!("no Page agent strategy registered for {provider}/{model}"))?;
            insert_initial_prompt_queue(task.stack, initial_prompt, initial_attachments, commands);
            Ok(())
        }
        Some(crate::AgentUrl::PageDefault) => {
            let provider = crate::providers::resolve_default_app_provider().ok_or_else(|| {
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
                    .to_string()
            })?;
            let idx = idx.ok_or_else(|| "page strategy index not registered".to_string())?;
            let sid = uuid::Uuid::new_v4().to_string();
            if transition_webview.is_none() {
                clear_stack_children(task.stack, children_q, commands);
            }
            attach_page_agent_to_stack_with_webview(
                task.stack,
                provider.provider,
                provider.default_model,
                &sid,
                transition_webview,
                commands,
                meshes,
                webview_mt,
                idx,
                kind_q,
            )
            .ok_or_else(|| {
                format!(
                    "no Page agent strategy registered for {}/{}",
                    provider.provider, provider.default_model
                )
            })?;
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
            if let Some(map) = agent_to_entity
                && let Some(&entity) = map.0.get(&(kind, sid.clone()))
            {
                vmux_terminal::pid::focus_pane_entity(entity, commands, child_of_q);
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
            // ACP agents own the canonical single-segment names (claude/codex/…) plus the
            // two-segment `<id>/<acp-session-id>` session form.
            let cfg = acp_configs
                .iter()
                .find(|config| crate::acp_install::agent_ids_match(&config.id, &id));
            if cfg.is_none() && acp_registry_agent_for_id(catalog, &id).is_none() {
                // Not an ACP agent. A bare `vmux://agent/<kind>` for a built-in CLI kind falls
                // back to a fresh CLI session (CLI's own url is `<kind>/cli`); this keeps the
                // legacy bare-url entry point (and the missing-binary setup flow) working.
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
            // Already attached to this agent on this stack? A repeat open (or the post-spawn url
            // redirect) is a no-op instead of re-spawning the session.
            if acp_sessions
                .get(task.stack)
                .is_ok_and(|session| crate::acp_install::agent_ids_match(&session.agent_id, &id))
            {
                return Ok(());
            }
            if transition_webview.is_none() {
                clear_stack_children(task.stack, children_q, commands);
            }
            // `sid` (when present) is the agent-assigned ACP session id from a restored url — pass
            // it as the resume target. Fresh opens mint a routing sid and load nothing.
            let routing_sid = uuid::Uuid::new_v4().to_string();
            let icon = acp_icon_for_id(catalog, &id);
            let name = acp_profile_name_for_id(&id, cfg, catalog);
            attach_acp_agent_to_stack_with_webview(
                task.stack,
                &id,
                &name,
                &routing_sid,
                default_cwd,
                icon.as_deref(),
                sid.as_deref(),
                transition_webview,
                commands,
                meshes,
                webview_mt,
            );
            insert_initial_prompt_queue(task.stack, initial_prompt, initial_attachments, commands);
            Ok(())
        }
        None => Err(format!("malformed agent URL '{}'", task.url)),
    }
}

pub(crate) fn insert_initial_prompt_queue(
    stack: Entity,
    initial_prompt: Option<String>,
    initial_attachments: Vec<AgentAttachment>,
    commands: &mut Commands,
) {
    let prompt = initial_prompt.unwrap_or_default();
    if prompt.trim().is_empty() && initial_attachments.is_empty() {
        return;
    }
    if let Some(title) = crate::components::provisional_conversation_title(&prompt) {
        commands
            .entity(stack)
            .insert(crate::components::AgentConversationTitle(title));
    }
    let mut queue = crate::components::PromptQueue::default();
    queue.enqueue_with_attachments(prompt, initial_attachments);
    commands.entity(stack).insert(queue).remove::<(
        vmux_core::PendingPrompt,
        vmux_core::PendingPromptAttachments,
    )>();
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

pub(crate) fn stack_has_agent_of_kind(
    stack: Entity,
    kind: AgentKind,
    children_q: &Query<&Children>,
    agents: &Query<&vmux_core::agent::AgentSession>,
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

pub(crate) fn clear_stack_children(
    stack: Entity,
    children_q: &Query<&Children>,
    commands: &mut Commands,
) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

pub(crate) fn attach_agent_spawn_error_to_stack(
    stack: Entity,
    kind: AgentKind,
    message: &str,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    clear_stack_children(stack, children_q, commands);
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
            vmux_layout::Browser::new_with_title(meshes, webview_mt, &data_url, title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

pub(crate) fn attach_cli_setup_to_stack(
    kind: AgentKind,
    stack: Entity,
    children_q: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    clear_stack_children(stack, children_q, commands);
    commands
        .entity(stack)
        .remove::<crate::vibe::setup::AgentSetupNavigated>();
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
            vmux_layout::Browser::new_with_title(meshes, webview_mt, &url, &title),
            ChildOf(stack),
        ))
        .id();
    commands.entity(browser).insert(CefKeyboardTarget);
}

pub(crate) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn data_url_for_html(html: &str) -> String {
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
    use crate::client::cli::vibe::VibeStrategy;
    use crate::plugin::provider::AgentExecutableOverride;
    use crate::plugin::spawn::handle_spawn_agent_requests;
    use crate::plugin::test_support::{init_worktree_test_repo, test_settings};
    use crate::session::{AgentSession, SessionId};
    use crate::strategy::AgentStrategies;
    use vmux_terminal::Terminal;

    pub(crate) fn swap_test_app() -> App {
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

    pub(crate) fn spawn_stack_child(app: &mut App) -> (Entity, Entity) {
        let stack = app.world_mut().spawn_empty().id();
        let child = app.world_mut().spawn(ChildOf(stack)).id();
        (stack, child)
    }

    #[test]
    pub(crate) fn invalid_swap_target_preserves_current_stack_child() {
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
    pub(crate) fn unconfigured_acp_swap_target_preserves_current_stack_child() {
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
    pub(crate) fn cross_agent_swap_attaches_fresh_target_with_imported_history() {
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
    pub(crate) fn acp_swap_resets_install_marker() {
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
    pub(crate) fn deep_link_focuses_existing_claude_tab() {
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
    pub(crate) fn missing_vibe_cli_shows_setup_page_at_vibe_url() {
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
    pub(crate) fn missing_claude_or_codex_cli_shows_setup_page() {
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
    pub(crate) fn registry_acp_opens_without_settings_entry() {
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
    pub(crate) fn explicit_setup_url_attaches_setup_page() {
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
    pub(crate) fn first_local_agent_open_creates_and_reuses_one_tab_worktree() {
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
    pub(crate) fn explicit_work_here_decision_skips_managed_worktree() {
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
    pub(crate) fn local_agent_open_preserves_existing_linked_worktree() {
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
    pub(crate) fn browser_only_tab_creates_no_worktree() {
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
    pub(crate) fn agent_tab_without_workspace_starts_in_home_without_binding_tab() {
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
    pub(crate) fn acp_tab_without_workspace_attaches_once_without_setup_page() {
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
                .query_filtered::<&ChildOf, With<crate::plugin::chat::AgentChatView>>()
                .iter(app.world())
                .filter(|child_of| child_of.parent() == stack)
                .count(),
            1
        );
    }

    #[test]
    pub(crate) fn inline_start_transition_reuses_the_existing_webview() {
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
                .get::<crate::plugin::chat::AgentChatView>(webview)
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
    pub(crate) fn acp_open_discards_missing_restored_tab_workspace() {
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
    pub(crate) fn fresh_claude_page_uses_space_startup_dir() {
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
    pub(crate) fn restored_agent_tab_uses_ancestor_space_startup_dir() {
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
    pub(crate) fn fresh_cli_page_forwards_pending_prompt() {
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
    pub(crate) fn cli_initial_prompt_waits_for_terminal_readiness() {
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
    pub(crate) fn cli_initial_prompt_keeps_media_paths() {
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
    pub(crate) fn fresh_acp_page_queues_pending_prompt() {
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
    pub(crate) fn fresh_claude_page_prefers_ancestor_tab_startup_dir() {
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
    pub(crate) fn fresh_claude_page_rejects_invalid_stored_tab_startup_dir() {
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
    pub(crate) fn bare_agent_open_skips_when_stack_already_has_same_agent() {
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
}
