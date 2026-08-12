//! Commands an agent aims at itself: its own pane, its own tab, its own worktree.
//!
//! These are ordered and gated rather than dispatched as they arrive — a worktree has to exist
//! before anything runs inside it, so a failed `create_worktree` blocks its siblings for that
//! frame instead of letting them run against the wrong directory.

use std::path::Path;

use bevy::prelude::*;
use vmux_command::WriteAppCommands;
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentCommand as ServiceAgentCommand, ClientMessage, ProcessId};
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::{
    AgentRunTerminal, ProcessExited, ServiceMessageSet, Terminal, TerminalStackSpawnRequest,
};

use crate::events::AgentCommandRequest;
use crate::session::AgentSession;

use super::command::requested_focus_for_origin;
use super::follow::file_touch_url;
use super::run_terminal::{
    AgentCwd, AgentPane, AgentTerminalRegions, PendingRunTerminalSpawn, PendingRunTerminalSpawns,
    RunCommand, RunPlacementPolicy, RunTerminal, RunTerminalBucketPanes, RunTerminalCandidate,
};
use super::workspace::{
    AgentTabWorktreeContext, PendingAgentChoice, PendingAgentChoiceAction, PendingWorkspacePicker,
    USER_CHOICE_REQUESTED, WORKSPACE_SELECTION_PENDING, WORKSPACE_SELECTION_REQUESTED,
    WorkspacePickerContext, activate_agent_directory, activate_agent_worktree,
    ambiguous_worktree_message, existing_worktree_candidates, resolve_requested_worktree,
    workspace_path_task, workspace_picker_task,
};

pub(super) struct SelfCommandPlugin;

impl Plugin for SelfCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_agent_self_commands
                .in_set(WriteAppCommands)
                .after(ServiceMessageSet)
                .after(vmux_layout::worktree::TabDirectoryRebindSet)
                .before(vmux_terminal::plugin::respond_terminal_stack_spawn),
        );
    }
}

fn resolve_self_pane(
    anchor: ProcessId,
    agent_terms: &Query<(Entity, &ProcessId, &ChildOf)>,
    child_of_q: &Query<&ChildOf>,
) -> Option<(Entity, Entity)> {
    use bevy::ecs::relationship::Relationship;
    let (term, _, term_co) = agent_terms.iter().find(|(_, pid, _)| **pid == anchor)?;
    let stack = term_co.get();
    let pane = child_of_q.get(stack).ok()?.get();
    Some((term, pane))
}

fn ancestor_self_tab(
    pane: Entity,
    tabs: &Query<&mut vmux_layout::tab::Tab>,
    child_of: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = pane;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

pub(crate) fn ancestor_acp_stack(
    entity: Entity,
    sessions: &Query<&mut crate::client::acp::AcpSession>,
    child_of: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if sessions.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn ancestor_agent_session(
    entity: Entity,
    acp_sessions: &Query<&mut crate::client::acp::AcpSession>,
    page_sessions: &Query<&vmux_session::AgentSession>,
    cli_sessions: &Query<&AgentSession>,
    child_of: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if acp_sessions.contains(current)
            || page_sessions.contains(current)
            || cli_sessions.contains(current)
        {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

pub(crate) fn rebind_acp_workspace(
    stack: Entity,
    cwd: &Path,
    sessions: &mut Query<&mut crate::client::acp::AcpSession>,
    commands: &mut Commands,
) -> Option<ClientMessage> {
    let Ok(mut session) = sessions.get_mut(stack) else {
        return None;
    };
    session.cwd = cwd.to_path_buf();
    let cwd = cwd.to_string_lossy().into_owned();
    commands
        .entity(stack)
        .insert(vmux_core::AgentWorkingDir(cwd.clone()));
    Some(ClientMessage::RebindAcpWorkspace {
        sid: session.sid.clone(),
        cwd,
    })
}

fn self_command_anchor(command: &ServiceAgentCommand) -> Option<ProcessId> {
    match command {
        ServiceAgentCommand::OpenBeside { anchor, .. }
        | ServiceAgentCommand::Run { anchor, .. }
        | ServiceAgentCommand::RunWithPlacementOverride { anchor, .. }
        | ServiceAgentCommand::CreateWorktree { anchor }
        | ServiceAgentCommand::ChooseWorkspace { anchor }
        | ServiceAgentCommand::ChooseWorkspaceAtPath { anchor, .. }
        | ServiceAgentCommand::PrepareWorktree { anchor, .. }
        | ServiceAgentCommand::RequestUserChoice { anchor, .. }
        | ServiceAgentCommand::SetConversationTitle { anchor, .. }
        | ServiceAgentCommand::SearchKnowledge { anchor, .. }
        | ServiceAgentCommand::ReadKnowledge { anchor, .. }
        | ServiceAgentCommand::WriteKnowledge { anchor, .. }
        | ServiceAgentCommand::CreateWorktreeOnBranch { anchor, .. } => Some(*anchor),
        _ => None,
    }
}

fn self_command_priority(command: &ServiceAgentCommand) -> u8 {
    if matches!(
        command,
        ServiceAgentCommand::CreateWorktree { .. }
            | ServiceAgentCommand::ChooseWorkspace { .. }
            | ServiceAgentCommand::ChooseWorkspaceAtPath { .. }
            | ServiceAgentCommand::PrepareWorktree { .. }
            | ServiceAgentCommand::RequestUserChoice { .. }
            | ServiceAgentCommand::SetConversationTitle { .. }
            | ServiceAgentCommand::SearchKnowledge { .. }
            | ServiceAgentCommand::ReadKnowledge { .. }
            | ServiceAgentCommand::WriteKnowledge { .. }
            | ServiceAgentCommand::CreateWorktreeOnBranch { .. }
    ) {
        0
    } else {
        1
    }
}

fn self_command_blocked_by_worktree_failure(
    command: &ServiceAgentCommand,
    failed: &std::collections::HashSet<ProcessId>,
) -> bool {
    !matches!(
        command,
        ServiceAgentCommand::CreateWorktree { .. }
            | ServiceAgentCommand::ChooseWorkspace { .. }
            | ServiceAgentCommand::ChooseWorkspaceAtPath { .. }
            | ServiceAgentCommand::PrepareWorktree { .. }
            | ServiceAgentCommand::RequestUserChoice { .. }
            | ServiceAgentCommand::SetConversationTitle { .. }
            | ServiceAgentCommand::SearchKnowledge { .. }
            | ServiceAgentCommand::ReadKnowledge { .. }
            | ServiceAgentCommand::WriteKnowledge { .. }
            | ServiceAgentCommand::CreateWorktreeOnBranch { .. }
    ) && self_command_anchor(command).is_some_and(|anchor| failed.contains(&anchor))
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentSelfCommandWriters<'w> {
    open_beside: MessageWriter<'w, vmux_layout::OpenBesideRequest>,
    terminal_stack_spawn: MessageWriter<'w, TerminalStackSpawnRequest>,
    terminal_reinput: MessageWriter<'w, vmux_terminal::TerminalReinputRequest>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_agent_self_commands(
    mut reader: MessageReader<AgentCommandRequest>,
    agent_terms: Query<(Entity, &ProcessId, &ChildOf)>,
    term_pids: Query<(Entity, &ProcessId), With<Terminal>>,
    run_terms: Query<
        (Entity, &ProcessId, &TerminalLaunch, Has<AgentRunTerminal>),
        (
            With<Terminal>,
            Without<AgentSession>,
            Without<ProcessExited>,
        ),
    >,
    launch_q: Query<&TerminalLaunch>,
    mut acp_sessions: Query<&mut crate::client::acp::AcpSession>,
    ctx: vmux_layout::pane::PlacementCtx,
    mut writers: AgentSelfCommandWriters,
    mut commands: Commands,
    service: Option<Res<ServiceClient>>,
    active_space: Option<Res<ActiveSpace>>,
    settings: Res<AppSettings>,
    mut regions: ResMut<AgentTerminalRegions>,
    mut spawn_counter: ResMut<vmux_layout::pane::SpawnCounter>,
    mut tab_worktree: AgentTabWorktreeContext,
    mut workspace_picker: WorkspacePickerContext,
) {
    use vmux_service::protocol::{AgentCommandResult, ClientMessage};
    let Some(service) = service else {
        for _ in reader.read() {}
        return;
    };
    let managed_root = tab_worktree
        .managed_root
        .as_deref()
        .cloned()
        .unwrap_or_default()
        .0;
    // Anchors split during this batch. Several `run`s dispatched in one tick all
    // resolve to the same agent pane; the first splits it, the rest must extend
    // that split rather than re-split the leaf (which would orphan empty panes).
    let mut split_this_batch: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    let mut worktree_created_this_batch: std::collections::HashMap<Entity, String> =
        std::collections::HashMap::new();
    let mut terminal_spawns: Vec<TerminalStackSpawnRequest> = Vec::new();
    let mut pending_run_spawns = PendingRunTerminalSpawns::default();
    let mut failed_worktree_anchors = std::collections::HashSet::new();
    let mut workspace_picker_tabs: std::collections::HashSet<Entity> = workspace_picker
        .pickers
        .iter()
        .map(|picker| picker.tab_entity)
        .collect();
    let mut requests: Vec<_> = reader.read().collect();
    requests.sort_by_key(|request| self_command_priority(&request.command));
    for request in requests {
        let request_anchor = self_command_anchor(&request.command);
        if self_command_blocked_by_worktree_failure(&request.command, &failed_worktree_anchors) {
            service.0.send(ClientMessage::AgentCommandResponse {
                request_id: request.request_id,
                result: AgentCommandResult::Error(
                    "Skipped because worktree activation did not complete.".to_string(),
                ),
            });
            continue;
        }
        let result = match &request.command {
            ServiceAgentCommand::OpenBeside {
                anchor,
                direction,
                url,
                focus,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("self process not found".to_string()),
                Some((_, pane)) => {
                    let focus = requested_focus_for_origin(&request.origin, *focus);
                    writers.open_beside.write(vmux_layout::OpenBesideRequest {
                        pane,
                        direction: direction.as_ref().map(AgentPane::direction),
                        url: url.clone(),
                        request_id: request.request_id.0,
                        focus,
                    });
                    AgentCommandResult::Ok
                }
            },
            ServiceAgentCommand::Run {
                anchor,
                command,
                direction,
                focus,
                beside,
                mode,
                terminal,
                done_marker,
            }
            | ServiceAgentCommand::RunWithPlacementOverride {
                anchor,
                command,
                direction,
                focus,
                beside,
                mode,
                terminal,
                done_marker,
            } => 'run: {
                let placement_override = matches!(
                    &request.command,
                    ServiceAgentCommand::RunWithPlacementOverride { .. }
                ) || beside.is_some()
                    || *mode != vmux_service::protocol::PlacementMode::Auto
                    || *direction != vmux_service::protocol::AgentPaneDirection::Right;
                if let Err(error) = RunPlacementPolicy::new(placement_override).validate(&settings)
                {
                    break 'run AgentCommandResult::Error(error.to_string());
                }
                let focus = requested_focus_for_origin(&request.origin, *focus);
                let run = RunCommand::new(command, done_marker.as_deref());
                match terminal {
                    Some(pid) => match RunTerminal::new(*pid).launch(&term_pids, &launch_q) {
                        Ok(launch) => {
                            run.queue(&mut writers.terminal_reinput, *pid, &launch);
                            AgentCommandResult::Text(pid.to_string())
                        }
                        Err(error) => AgentCommandResult::Error(error),
                    },
                    None => 'spawn: {
                        let Some((agent_term, self_pane)) =
                            resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q)
                        else {
                            break 'spawn AgentCommandResult::Error(
                                "self process not found".to_string(),
                            );
                        };
                        let tab_cwd = {
                            let mut current = self_pane;
                            loop {
                                if let Ok(tab) = tab_worktree.tabs.get(current) {
                                    break tab.startup_dir.clone();
                                }
                                match ctx.child_of_q.get(current) {
                                    Ok(child_of) => current = child_of.parent(),
                                    Err(_) => break None,
                                }
                            }
                        };
                        let agent_cwd = launch_q
                            .get(agent_term)
                            .ok()
                            .map(|launch| launch.cwd.clone())
                            .or_else(|| {
                                let stack =
                                    ancestor_acp_stack(agent_term, &acp_sessions, &ctx.child_of_q)?;
                                acp_sessions
                                    .get(stack)
                                    .ok()
                                    .map(|session| session.cwd.to_string_lossy().into_owned())
                            });
                        let cwd = match AgentCwd::of_tab(tab_cwd.as_deref())
                            .or_agent_launch(agent_cwd.as_deref())
                        {
                            Ok(cwd) => cwd,
                            Err(message) => break 'spawn AgentCommandResult::Error(message),
                        };
                        let candidates = RunTerminalCandidate::collect(
                            self_pane,
                            &run_terms,
                            &ctx.child_of_q,
                            &ctx.tab_q,
                            &ctx.seq_q,
                            &cwd,
                        );
                        let terminal_bucket_panes = RunTerminalBucketPanes::collect(
                            self_pane,
                            &ctx.child_of_q,
                            &ctx.tab_q,
                            &ctx.leaf_panes,
                            &ctx.pane_children,
                            &ctx.tab_filter,
                            &ctx.page_q,
                            &ctx.seq_q,
                        );
                        if beside.is_none()
                            && *mode == vmux_service::protocol::PlacementMode::Auto
                            && let Some(pid) = pending_run_spawns.append_input(
                                *anchor,
                                &mut terminal_spawns,
                                &cwd,
                                run,
                            )
                        {
                            break 'spawn AgentCommandResult::Text(pid.to_string());
                        }
                        if beside.is_none()
                            && *mode == vmux_service::protocol::PlacementMode::Auto
                            && let Some(candidate) =
                                regions.choose_reusable_terminal(*anchor, self_pane, &candidates)
                        {
                            let Ok(launch) = launch_q.get(candidate.terminal) else {
                                break 'spawn AgentCommandResult::Error(format!(
                                    "run terminal launch not found: {}",
                                    candidate.pid
                                ));
                            };
                            run.queue(&mut writers.terminal_reinput, candidate.pid, launch);
                            regions.run_terminals.insert(*anchor, candidate.pid);
                            regions.run_panes.insert(*anchor, candidate.pane);
                            AgentPane::new(candidate.pane).touch_spawn_seq(
                                &mut commands,
                                &mut spawn_counter,
                                &ctx.seq_q,
                            );
                            if focus {
                                candidate.focus(&mut commands, &ctx.child_of_q, &ctx.tab_q);
                            }
                            break 'spawn AgentCommandResult::Text(candidate.pid.to_string());
                        }
                        // Resolve an explicit `beside` anchor up front (errors if stale).
                        let beside_pane = match beside {
                            Some(pid) => {
                                match RunTerminal::new(*pid).pane(&term_pids, &ctx.child_of_q) {
                                    Some(pane) => Some(pane),
                                    None => {
                                        break 'spawn AgentCommandResult::Error(format!(
                                            "run.beside page not found: {pid}"
                                        ));
                                    }
                                }
                            }
                            None => None,
                        };
                        let (shell, data) = run.for_new_terminal(&settings);
                        if let Err(error) = shell.validate() {
                            break 'spawn AgentCommandResult::Error(error);
                        }
                        let shell = shell.into_string();

                        use vmux_service::protocol::PlacementMode;
                        let target_pane = match (beside_pane, *mode) {
                            (anchor_pane, PlacementMode::Split) => {
                                let bucket_pane = if anchor_pane.is_none() {
                                    regions
                                        .choose_bucket_pane(*anchor, self_pane, &candidates)
                                        .filter(|pane| terminal_bucket_panes.contains(*pane))
                                        .or_else(|| terminal_bucket_panes.newest(self_pane))
                                } else {
                                    None
                                };
                                if let Some(pane) = bucket_pane {
                                    pane
                                } else {
                                    let anchor_pane = anchor_pane.unwrap_or_else(|| {
                                        vmux_layout::pane::resolve_split_anchor_pane(
                                            self_pane, &ctx,
                                        )
                                    });
                                    AgentPane::new(anchor_pane).split_off(
                                        &mut commands,
                                        direction,
                                        focus,
                                        &ctx.pane_children,
                                        &ctx.tab_filter,
                                        &ctx.split_dir_q,
                                        &mut split_this_batch,
                                    )
                                }
                            }
                            (Some(pane), _) => pane,
                            (None, _) => vmux_layout::pane::resolve_spiral_pane(
                                &mut commands,
                                self_pane,
                                TERMINAL_PAGE_URL,
                                focus,
                                &mut split_this_batch,
                                &ctx,
                            ),
                        };
                        AgentPane::new(target_pane).touch_spawn_seq(
                            &mut commands,
                            &mut spawn_counter,
                            &ctx.seq_q,
                        );
                        let new_pid = ProcessId::new();
                        let request_index = terminal_spawns.len();
                        terminal_spawns.push(TerminalStackSpawnRequest {
                            pane: target_pane,
                            cwd: Some(cwd),
                            shell: Some(shell.clone()),
                            agent_run: true,
                            pending_input: Some(data),
                            process_id: Some(new_pid),
                            activate: focus,
                        });
                        regions.run_panes.insert(*anchor, target_pane);
                        if beside.is_none() && *mode != vmux_service::protocol::PlacementMode::Split
                        {
                            regions.run_terminals.insert(*anchor, new_pid);
                            pending_run_spawns.insert(
                                *anchor,
                                PendingRunTerminalSpawn {
                                    pid: new_pid,
                                    request_index,
                                    shell,
                                },
                            );
                        }
                        AgentCommandResult::Text(new_pid.to_string())
                    }
                }
            }
            ServiceAgentCommand::RequestUserChoice {
                anchor,
                question,
                options,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((agent_entity, _)) => {
                    let Some(session_entity) = ancestor_agent_session(
                        agent_entity,
                        &acp_sessions,
                        &workspace_picker.page_sessions,
                        &workspace_picker.cli_sessions,
                        &ctx.child_of_q,
                    ) else {
                        service.0.send(ClientMessage::AgentCommandResponse {
                            request_id: request.request_id,
                            result: AgentCommandResult::Error(
                                "agent session not found".to_string(),
                            ),
                        });
                        continue;
                    };
                    if workspace_picker.choices.get(agent_entity).is_ok() {
                        AgentCommandResult::Text(USER_CHOICE_REQUESTED.to_string())
                    } else if workspace_picker.chat_views.contains(agent_entity) {
                        commands
                            .entity(agent_entity)
                            .insert(PendingAgentChoice {
                                session_entity,
                                action: PendingAgentChoiceAction::Resume,
                                question: question.clone(),
                                options: options.clone(),
                            })
                            .remove::<crate::plugin::chat::ChatSynced>();
                        AgentCommandResult::Text(USER_CHOICE_REQUESTED.to_string())
                    } else {
                        AgentCommandResult::Error(
                            "Native choice prompts require the chat agent view; ask the user with the same numbered options in the current terminal session."
                                .to_string(),
                        )
                    }
                }
            },
            ServiceAgentCommand::SetConversationTitle { anchor, title } => {
                match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                    None => AgentCommandResult::Error("agent pane not found".to_string()),
                    Some((agent_entity, _)) => {
                        let Some(session_entity) = ancestor_agent_session(
                            agent_entity,
                            &acp_sessions,
                            &workspace_picker.page_sessions,
                            &workspace_picker.cli_sessions,
                            &ctx.child_of_q,
                        ) else {
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error(
                                    "agent session not found".to_string(),
                                ),
                            });
                            continue;
                        };
                        let title = title.trim().to_string();
                        if title.is_empty() {
                            AgentCommandResult::Error("conversation title is empty".to_string())
                        } else if let Ok(mut current) =
                            workspace_picker.conversation_titles.get_mut(session_entity)
                        {
                            current.0 = title;
                            AgentCommandResult::Ok
                        } else {
                            commands
                                .entity(session_entity)
                                .insert(vmux_session::AgentConversationTitle(title));
                            AgentCommandResult::Ok
                        }
                    }
                }
            }
            ServiceAgentCommand::SearchKnowledge {
                anchor,
                query,
                limit,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some(_) => match tab_worktree.knowledge_index.as_deref() {
                    Some(index) if index.loaded() => {
                        let matches = index.search(query, usize::from(*limit));
                        if matches.is_empty() {
                            AgentCommandResult::Text(format!(
                                "No Knowledge matches for: {}",
                                query.trim()
                            ))
                        } else {
                            let root = index.root();
                            let text = matches
                                .into_iter()
                                .map(|item| {
                                    let path = item
                                        .path
                                        .strip_prefix(root)
                                        .unwrap_or(&item.path)
                                        .to_string_lossy()
                                        .replace('\\', "/");
                                    format!(
                                        "{}:{}: {} — {}",
                                        path,
                                        item.line + 1,
                                        item.title,
                                        item.preview
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            AgentCommandResult::Text(text)
                        }
                    }
                    Some(_) => AgentCommandResult::Error(
                        "Knowledge index is still loading; retry shortly.".to_string(),
                    ),
                    None => AgentCommandResult::Error(
                        "Knowledge is unavailable in this vmux session.".to_string(),
                    ),
                },
            },
            ServiceAgentCommand::ReadKnowledge {
                anchor,
                path,
                line,
                limit,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some(_) => match tab_worktree.knowledge_index.as_deref() {
                    Some(index) if index.loaded() => match index.note_by_query(path) {
                        Some((note_path, title, text)) => {
                            let lines = text.lines().collect::<Vec<_>>();
                            let start = line.saturating_sub(1) as usize;
                            if start >= lines.len() && !lines.is_empty() {
                                AgentCommandResult::Error(format!(
                                    "Knowledge line {} exceeds note length {}",
                                    line,
                                    lines.len()
                                ))
                            } else {
                                let end = start.saturating_add(*limit as usize).min(lines.len());
                                let source = note_path
                                    .strip_prefix(index.root())
                                    .unwrap_or(&note_path)
                                    .to_string_lossy()
                                    .replace('\\', "/");
                                let body = lines[start..end]
                                    .iter()
                                    .enumerate()
                                    .map(|(offset, value)| {
                                        format!("{} | {}", start + offset + 1, value)
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                AgentCommandResult::Text(format!(
                                    "Source: {source}\nTitle: {title}\nLines {}-{}\n\n{body}",
                                    start + 1,
                                    end
                                ))
                            }
                        }
                        None => AgentCommandResult::Error(format!(
                            "Knowledge note not found: {}",
                            path.trim()
                        )),
                    },
                    Some(_) => AgentCommandResult::Error(
                        "Knowledge index is still loading; retry shortly.".to_string(),
                    ),
                    None => AgentCommandResult::Error(
                        "Knowledge is unavailable in this vmux session.".to_string(),
                    ),
                },
            },
            ServiceAgentCommand::WriteKnowledge {
                anchor,
                path,
                title,
                content,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((_, pane)) => {
                    match vmux_core::knowledge::KnowledgeVault::user().write_note(
                        path.as_deref(),
                        title,
                        content,
                    ) {
                        Ok(path) => {
                            writers.open_beside.write(vmux_layout::OpenBesideRequest {
                                pane,
                                direction: None,
                                url: file_touch_url(&path.to_string_lossy(), None, None, None),
                                request_id: request.request_id.0,
                                focus: false,
                            });
                            AgentCommandResult::Text(format!("Knowledge saved: {}", path.display()))
                        }
                        Err(error) => AgentCommandResult::Error(error),
                    }
                }
            },
            ServiceAgentCommand::ChooseWorkspace { anchor }
            | ServiceAgentCommand::ChooseWorkspaceAtPath { anchor, .. } => {
                match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                    None => AgentCommandResult::Error("agent pane not found".to_string()),
                    Some((agent_entity, pane)) => {
                        let Some(tab_entity) =
                            ancestor_self_tab(pane, &tab_worktree.tabs, &ctx.child_of_q)
                        else {
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error("no tab for agent".to_string()),
                            });
                            continue;
                        };
                        let Some(session_entity) = ancestor_agent_session(
                            agent_entity,
                            &acp_sessions,
                            &workspace_picker.page_sessions,
                            &workspace_picker.cli_sessions,
                            &ctx.child_of_q,
                        ) else {
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error(
                                    "agent session not found".to_string(),
                                ),
                            });
                            continue;
                        };
                        if workspace_picker.choices.get(agent_entity).is_ok() {
                            AgentCommandResult::Text(USER_CHOICE_REQUESTED.to_string())
                        } else if !workspace_picker_tabs.insert(tab_entity) {
                            AgentCommandResult::Text(WORKSPACE_SELECTION_PENDING.to_string())
                        } else if let ServiceAgentCommand::ChooseWorkspaceAtPath { path, .. } =
                            &request.command
                            && let Ok(selected) = Path::new(path).canonicalize()
                            && selected.is_dir()
                        {
                            commands.spawn(PendingWorkspacePicker {
                                tab_entity,
                                agent_entity,
                                session_entity,
                                task: workspace_path_task(
                                    selected,
                                    workspace_picker.proxy.as_deref(),
                                ),
                            });
                            AgentCommandResult::Text(WORKSPACE_SELECTION_REQUESTED.to_string())
                        } else {
                            commands.spawn(PendingWorkspacePicker {
                                tab_entity,
                                agent_entity,
                                session_entity,
                                task: workspace_picker_task(workspace_picker.proxy.as_deref()),
                            });
                            AgentCommandResult::Text(WORKSPACE_SELECTION_REQUESTED.to_string())
                        }
                    }
                }
            }
            ServiceAgentCommand::PrepareWorktree {
                anchor,
                path,
                task,
                create,
            } => match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((agent_entity, pane)) => {
                    let Some(tab_entity) =
                        ancestor_self_tab(pane, &tab_worktree.tabs, &ctx.child_of_q)
                    else {
                        service.0.send(ClientMessage::AgentCommandResponse {
                            request_id: request.request_id,
                            result: AgentCommandResult::Error("no tab for agent".to_string()),
                        });
                        continue;
                    };
                    let current_dir = tab_worktree.tabs.get(tab_entity).ok().and_then(|tab| {
                        AgentCwd::of_tab(tab.startup_dir.as_deref())
                            .stored()
                            .ok()
                            .flatten()
                    });
                    if let Some(current_dir) = current_dir.as_deref()
                        && vmux_git::worktree::is_linked_worktree(current_dir)
                    {
                        AgentCommandResult::Text(current_dir.to_string_lossy().into_owned())
                    } else {
                        let project_dir = tab_worktree
                            .workspaces
                            .get(tab_entity)
                            .ok()
                            .and_then(|workspace| {
                                AgentCwd::of_tab(Some(&workspace.project_dir))
                                    .stored()
                                    .ok()
                                    .flatten()
                            })
                            .or_else(|| {
                                tab_worktree
                                    .pending_projects
                                    .get(tab_entity)
                                    .ok()
                                    .map(|project| project.0.clone())
                            })
                            .or(current_dir);
                        let Some(project_dir) = project_dir else {
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error(
                                    "No Git project selected. Complete select_project and initialize Git first."
                                        .to_string(),
                                ),
                            });
                            continue;
                        };
                        if vmux_git::worktree::checkout_info(&project_dir).is_err() {
                            AgentCommandResult::Text(project_dir.to_string_lossy().into_owned())
                        } else {
                            let candidate = if *create {
                                Ok(None)
                            } else {
                                match path.as_deref() {
                                    Some(path) => {
                                        resolve_requested_worktree(&project_dir, Path::new(path))
                                            .map(Some)
                                    }
                                    None => match existing_worktree_candidates(&project_dir) {
                                        Ok(candidates) if candidates.is_empty() => Ok(None),
                                        Ok(candidates) if candidates.len() == 1 => {
                                            Ok(candidates.into_iter().next())
                                        }
                                        Ok(candidates) => {
                                            Err(ambiguous_worktree_message(&candidates))
                                        }
                                        Err(error) => Err(error),
                                    },
                                }
                            };
                            match candidate {
                                Err(error) => AgentCommandResult::Error(error),
                                Ok(Some(candidate)) => match activate_agent_directory(
                                    tab_entity,
                                    agent_entity,
                                    &project_dir,
                                    &candidate.execution_dir,
                                    &mut tab_worktree.tabs,
                                    &mut acp_sessions,
                                    &ctx.child_of_q,
                                    &mut commands,
                                ) {
                                    Ok(rebind) => {
                                        if let Some(message) = rebind {
                                            service.0.send(message);
                                        }
                                        AgentCommandResult::Text(format!(
                                            "Worktree ready: {}\nContinue the original request immediately in this directory. Do not stop after setup or search for optional tools.",
                                            candidate.execution_dir.display()
                                        ))
                                    }
                                    Err(error) => AgentCommandResult::Error(error),
                                },
                                Ok(None) => {
                                    let name = task
                                        .as_deref()
                                        .filter(|task| !task.trim().is_empty())
                                        .map(str::to_string)
                                        .or_else(|| {
                                            tab_worktree
                                                .tabs
                                                .get(tab_entity)
                                                .ok()
                                                .map(|tab| tab.name.clone())
                                        })
                                        .unwrap_or_else(|| "task".to_string());
                                    let slug_hint = vmux_layout::worktree::tab_worktree_slug_hint(
                                        &name,
                                        &project_dir,
                                    );
                                    match vmux_layout::worktree::create_worktree_blocking(
                                        &project_dir,
                                        &slug_hint,
                                        &managed_root,
                                    ) {
                                        Ok(activation) => match activate_agent_worktree(
                                            tab_entity,
                                            agent_entity,
                                            &project_dir,
                                            activation,
                                            &mut tab_worktree.tabs,
                                            &mut acp_sessions,
                                            &ctx.child_of_q,
                                            &mut commands,
                                        ) {
                                            Ok((execution_dir, rebind)) => {
                                                if let Some(message) = rebind {
                                                    service.0.send(message);
                                                }
                                                worktree_created_this_batch.insert(
                                                    tab_entity,
                                                    vmux_git::worktree::head_ref(&execution_dir)
                                                        .unwrap_or_default(),
                                                );
                                                AgentCommandResult::Text(format!(
                                                    "Worktree ready: {}\nContinue the original request immediately in this directory. Do not stop after setup or search for optional tools.",
                                                    execution_dir.display()
                                                ))
                                            }
                                            Err(error) => AgentCommandResult::Error(error),
                                        },
                                        Err(error) => AgentCommandResult::Error(error),
                                    }
                                }
                            }
                        }
                    }
                }
            },
            ServiceAgentCommand::CreateWorktree { anchor } => {
                match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                    None => AgentCommandResult::Error("agent pane not found".to_string()),
                    Some((_, pane)) => {
                        let mut cur = pane;
                        let tab_e = loop {
                            if tab_worktree.tabs.get(cur).is_ok() {
                                break Some(cur);
                            }
                            match ctx.child_of_q.get(cur) {
                                Ok(co) => cur = co.parent(),
                                Err(_) => break None,
                            }
                        };
                        match tab_e {
                            None => AgentCommandResult::Error("no tab for agent".to_string()),
                            Some(tab_e)
                                if tab_worktree.worktrees.get(tab_e).is_ok()
                                    || worktree_created_this_batch.contains_key(&tab_e) =>
                            {
                                let tab_dir = tab_worktree
                                    .tabs
                                    .get(tab_e)
                                    .ok()
                                    .and_then(|t| t.startup_dir.clone());
                                match AgentCwd::of_tab(tab_dir.as_deref()).stored() {
                                    Ok(Some(path)) => AgentCommandResult::Text(
                                        path.to_string_lossy().into_owned(),
                                    ),
                                    Ok(None) => AgentCommandResult::Error(
                                        "tab project directory is missing".to_string(),
                                    ),
                                    Err(message) => AgentCommandResult::Error(message),
                                }
                            }
                            Some(tab_e) => {
                                let tab_dir = tab_worktree
                                    .tabs
                                    .get(tab_e)
                                    .ok()
                                    .and_then(|t| t.startup_dir.clone());
                                let name = tab_worktree
                                    .tabs
                                    .get(tab_e)
                                    .map(|t| t.name.clone())
                                    .unwrap_or_default();
                                match AgentCwd::of_tab(tab_dir.as_deref()).stored() {
                                    Err(message) => AgentCommandResult::Error(message),
                                    Ok(stored) => 'create_worktree: {
                                        let configured_dir =
                                            active_space.as_deref().and_then(|space| {
                                                vmux_setting::resolve_startup_dir(
                                                    &settings,
                                                    &space.record.id,
                                                )
                                            });
                                        let workspace_dir =
                                            tab_worktree.workspaces.get(tab_e).ok().and_then(
                                                |workspace| {
                                                    AgentCwd::of_tab(Some(&workspace.project_dir))
                                                        .stored()
                                                        .ok()
                                                        .flatten()
                                                },
                                            );
                                        let Some(current_dir) = stored
                                            .or(configured_dir)
                                            .or_else(|| workspace_dir.clone())
                                        else {
                                            break 'create_worktree AgentCommandResult::Error(
                                                "tab project directory is missing".to_string(),
                                            );
                                        };
                                        if vmux_git::worktree::is_linked_worktree(&current_dir) {
                                            AgentCommandResult::Text(
                                                current_dir.to_string_lossy().into_owned(),
                                            )
                                        } else {
                                            let base_dir = workspace_dir
                                                .unwrap_or_else(|| current_dir.clone());
                                            if tab_worktree.workspaces.get(tab_e).is_err() {
                                                commands.entity(tab_e).insert(
                                                    vmux_layout::tab::TabWorkspace {
                                                        project_dir: base_dir
                                                            .to_string_lossy()
                                                            .into_owned(),
                                                    },
                                                );
                                            }
                                            let slug_hint =
                                                vmux_layout::worktree::tab_worktree_slug_hint(
                                                    &name, &base_dir,
                                                );
                                            match vmux_layout::worktree::create_worktree_blocking(
                                                &base_dir,
                                                &slug_hint,
                                                &managed_root,
                                            ) {
                                                Ok(activation) => {
                                                    let branch = activation.metadata.branch.clone();
                                                    let path = activation
                                                        .execution_dir
                                                        .to_string_lossy()
                                                        .into_owned();
                                                    if let Ok(mut t) =
                                                        tab_worktree.tabs.get_mut(tab_e)
                                                    {
                                                        t.startup_dir = Some(path.clone());
                                                    }
                                                    commands
                                                        .entity(tab_e)
                                                        .insert((
                                                            activation.metadata,
                                                            activation.ready,
                                                        ))
                                                        .remove::<
                                                            vmux_layout::tab::TabWorktreeUnavailable,
                                                        >();
                                                    worktree_created_this_batch
                                                        .insert(tab_e, branch);
                                                    AgentCommandResult::Text(path)
                                                }
                                                Err(e) => AgentCommandResult::Error(e),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ServiceAgentCommand::CreateWorktreeOnBranch { anchor, branch } => {
                match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                    None => AgentCommandResult::Error("agent pane not found".to_string()),
                    Some((agent_entity, pane)) => {
                        let Some(tab_entity) =
                            ancestor_self_tab(pane, &tab_worktree.tabs, &ctx.child_of_q)
                        else {
                            failed_worktree_anchors.insert(*anchor);
                            service.0.send(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error("no tab for agent".to_string()),
                            });
                            continue;
                        };
                        let existing_branch = tab_worktree
                            .worktrees
                            .get(tab_entity)
                            .ok()
                            .map(|worktree| worktree.branch.clone())
                            .or_else(|| worktree_created_this_batch.get(&tab_entity).cloned());
                        if let Some(existing_branch) = existing_branch {
                            if existing_branch != *branch {
                                AgentCommandResult::Error(format!(
                                    "Tab already has a worktree on branch {existing_branch}; requested {branch}"
                                ))
                            } else {
                                let path = tab_worktree
                                    .tabs
                                    .get(tab_entity)
                                    .ok()
                                    .and_then(|tab| tab.startup_dir.clone());
                                match path {
                                    Some(path) => AgentCommandResult::Text(path),
                                    None => AgentCommandResult::Error(
                                        "tab worktree directory is missing".to_string(),
                                    ),
                                }
                            }
                        } else {
                            let base_dir = tab_worktree
                                .pending_projects
                                .get(tab_entity)
                                .ok()
                                .map(|project| project.0.clone())
                                .or_else(|| {
                                    tab_worktree.workspaces.get(tab_entity).ok().and_then(
                                        |workspace| {
                                            AgentCwd::of_tab(Some(&workspace.project_dir))
                                                .stored()
                                                .ok()
                                                .flatten()
                                        },
                                    )
                                });
                            let Some(base_dir) = base_dir else {
                                failed_worktree_anchors.insert(*anchor);
                                service.0.send(ClientMessage::AgentCommandResponse {
                                    request_id: request.request_id,
                                    result: AgentCommandResult::Error(
                                        "No project selected. Call select_project first."
                                            .to_string(),
                                    ),
                                });
                                continue;
                            };
                            match vmux_layout::worktree::create_worktree_for_branch_blocking(
                                &base_dir,
                                branch,
                                &managed_root,
                            ) {
                                Ok(activation) => match activate_agent_worktree(
                                    tab_entity,
                                    agent_entity,
                                    &base_dir,
                                    activation,
                                    &mut tab_worktree.tabs,
                                    &mut acp_sessions,
                                    &ctx.child_of_q,
                                    &mut commands,
                                ) {
                                    Ok((execution_dir, rebind)) => {
                                        if let Some(message) = rebind {
                                            service.0.send(message);
                                        }
                                        worktree_created_this_batch
                                            .insert(tab_entity, branch.clone());
                                        let path = execution_dir.to_string_lossy().into_owned();
                                        AgentCommandResult::Text(format!(
                                            "Worktree ready: {path}\nContinue the original request immediately in this directory. Do not stop after setup or search for optional tools."
                                        ))
                                    }
                                    Err(error) => AgentCommandResult::Error(error),
                                },
                                Err(error) => AgentCommandResult::Error(error),
                            }
                        }
                    }
                }
            }
            _ => continue,
        };
        if matches!(
            (&request.command, &result),
            (
                ServiceAgentCommand::CreateWorktree { .. }
                    | ServiceAgentCommand::CreateWorktreeOnBranch { .. }
                    | ServiceAgentCommand::PrepareWorktree { .. },
                AgentCommandResult::Error(_)
            )
        ) && let Some(anchor) = request_anchor
        {
            failed_worktree_anchors.insert(anchor);
        }
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: request.request_id,
            result,
        });
    }
    for spawn in terminal_spawns {
        writers.terminal_stack_spawn.write(spawn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::{IntoSystemSet, NodeId, Schedules, SystemSet};

    /// A `run` command writes a [`TerminalStackSpawnRequest`] that the terminal crate turns into a
    /// pane in the same frame. Lose the edge and the spawn slips a frame behind the agent command
    /// that asked for it, so the schedule is asked directly rather than trusted to the order this
    /// plugin happens to be added in.
    #[test]
    fn agent_run_spawns_terminal_before_next_agent_command_frame() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SelfCommandPlugin));

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();

        let self_commands = graph
            .systems_in_set(handle_agent_self_commands.into_system_set().intern())
            .expect("handle_agent_self_commands is registered")
            .first()
            .copied()
            .expect("handle_agent_self_commands is registered");
        let terminal_spawn = graph
            .system_sets
            .get_key(
                vmux_terminal::plugin::respond_terminal_stack_spawn
                    .into_system_set()
                    .intern(),
            )
            .expect("the ordering names respond_terminal_stack_spawn");

        assert!(
            graph
                .dependency()
                .graph()
                .contains_edge(NodeId::System(self_commands), NodeId::Set(terminal_spawn)),
            "run terminal spawn requests must materialize before the next agent command frame"
        );
    }

    #[test]
    pub(crate) fn create_worktree_precedes_and_gates_sibling_self_commands() {
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
}
