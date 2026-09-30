use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_api::BinEvent;
use vmux_api::protocol::ProcessId;
#[cfg(test)]
use vmux_api::protocol::{AgentRequest, AgentRequestId};
use vmux_chat::host::USER_CHOICE_REQUESTED;
use vmux_command::WriteCommandRequests;
#[cfg(test)]
use vmux_core::agent::CommandOrigin;
use vmux_core::agent::{AgentRequestBlocked, AgentRequestInput, AgentRequestPrerequisiteSet};
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_setting::{AppSettings, StartupDir};

use super::agent::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentPrepareWorktree,
};
use super::workspace::{
    AgentWorkspacePicker, AgentWorkspaceState, ExistingWorktreeCandidates, PendingWorkspacePicker,
    WORKSPACE_SELECTION_PENDING, WORKSPACE_SELECTION_REQUESTED, workspace_path_task,
    workspace_picker_task,
};
use vmux_core::profile::ProjectsDirectory;

struct WorkspaceDirectory;

impl WorkspaceDirectory {
    fn stored(path: Option<&str>) -> Result<Option<PathBuf>, String> {
        let Some(path) = path else {
            return Ok(None);
        };
        StartupDir::from_tab(path).map(|directory| Some(directory.path))
    }
}

pub(super) struct AgentWorkspaceRequestPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct AgentWorkspaceRequestSet;

impl Plugin for AgentWorkspaceRequestPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentRequestBlocked>()
            .add_message::<ServiceRequest>()
            .add_systems(
                Update,
                handle_agent_workspace_requests
                    .in_set(AgentWorkspaceRequestSet)
                    .in_set(AgentRequestPrerequisiteSet)
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet)
                    .after(vmux_layout::worktree::TabDirectoryRebindSet),
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

fn ancestor_agent_session(
    entity: Entity,
    session_roots: &Query<(), With<vmux_core::agent::AgentSessionRoot>>,
    child_of: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if session_roots.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

fn workspace_request_anchor(request: &AgentRequestInput) -> Option<ProcessId> {
    if let Ok(Some(command)) = request.decode::<AgentCreateWorktree>() {
        Some(command.anchor)
    } else if let Some(command) = WorkspaceChoice::decode(request) {
        Some(command.anchor)
    } else if let Ok(Some(command)) = request.decode::<AgentPrepareWorktree>() {
        Some(command.anchor)
    } else if let Ok(Some(command)) = request.decode::<AgentCreateWorktreeOnBranch>() {
        Some(command.anchor)
    } else {
        None
    }
}

struct WorkspaceChoice {
    anchor: ProcessId,
    path: Option<String>,
}

impl WorkspaceChoice {
    fn decode(request: &AgentRequestInput) -> Option<Self> {
        if let Ok(Some(command)) = request.decode::<AgentChooseWorkspace>() {
            return Some(Self {
                anchor: command.anchor,
                path: None,
            });
        }
        let command = request
            .decode::<AgentChooseWorkspaceAtPath>()
            .ok()
            .flatten()?;
        Some(Self {
            anchor: command.anchor,
            path: Some(command.path),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_agent_workspace_requests(
    mut reader: MessageReader<AgentRequestInput>,
    agent_terms: Query<(Entity, &ProcessId, &ChildOf)>,
    ctx: vmux_layout::pane::PanePlacement,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
    mut blocked_requests: MessageWriter<AgentRequestBlocked>,
    active_space: vmux_layout::space::FocusedSpace,
    settings: Res<AppSettings>,
    mut workspace: AgentWorkspaceState,
    workspace_picker: AgentWorkspacePicker,
) {
    use vmux_api::protocol::{AgentCommandResult, ClientMessage};
    let managed_root = workspace
        .managed_root
        .as_deref()
        .cloned()
        .unwrap_or_default()
        .0;
    let mut worktree_created_this_batch: std::collections::HashMap<Entity, String> =
        std::collections::HashMap::new();
    let mut workspace_picker_tabs: std::collections::HashSet<Entity> = workspace_picker
        .pickers
        .iter()
        .map(|picker| picker.tab_entity)
        .collect();
    for request in reader.read() {
        let request_anchor = workspace_request_anchor(request);
        let result = if let Some(command) = WorkspaceChoice::decode(request) {
            let anchor = command.anchor;
            match resolve_self_pane(anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((agent_entity, pane)) => {
                    let Some(tab_entity) =
                        ancestor_self_tab(pane, &workspace.tabs, &ctx.child_of_q)
                    else {
                        service_requests.write(ServiceRequest(
                            ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error("no tab for agent".to_string()),
                            },
                        ));
                        continue;
                    };
                    let Some(session_entity) = ancestor_agent_session(
                        agent_entity,
                        &workspace_picker.session_roots,
                        &ctx.child_of_q,
                    ) else {
                        service_requests.write(ServiceRequest(
                            ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error(
                                    "agent session not found".to_string(),
                                ),
                            },
                        ));
                        continue;
                    };
                    if workspace_picker.choices.get(agent_entity).is_ok() {
                        AgentCommandResult::Text(USER_CHOICE_REQUESTED.to_string())
                    } else if !workspace_picker_tabs.insert(tab_entity) {
                        AgentCommandResult::Text(WORKSPACE_SELECTION_PENDING.to_string())
                    } else if let Some(path) = command.path.as_deref()
                        && let Ok(selected) = Path::new(path).canonicalize()
                        && selected.is_dir()
                    {
                        let trusted = ProjectsDirectory::ensure()
                            .is_ok_and(|projects| projects.contains(&selected));
                        let task = if trusted {
                            workspace_path_task(selected, workspace_picker.proxy.as_deref())
                        } else {
                            workspace_picker_task(Some(selected), workspace_picker.proxy.as_deref())
                        };
                        commands.spawn(PendingWorkspacePicker {
                            tab_entity,
                            agent_entity,
                            session_entity,
                            task,
                        });
                        AgentCommandResult::Text(WORKSPACE_SELECTION_REQUESTED.to_string())
                    } else {
                        commands.spawn(PendingWorkspacePicker {
                            tab_entity,
                            agent_entity,
                            session_entity,
                            task: workspace_picker_task(None, workspace_picker.proxy.as_deref()),
                        });
                        AgentCommandResult::Text(WORKSPACE_SELECTION_REQUESTED.to_string())
                    }
                }
            }
        } else if let Ok(Some(command)) = request.decode::<AgentPrepareWorktree>() {
            let anchor = &command.anchor;
            let path = &command.path;
            let task = &command.task;
            let create = &command.create;
            match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((agent_entity, pane)) => {
                    let Some(tab_entity) =
                        ancestor_self_tab(pane, &workspace.tabs, &ctx.child_of_q)
                    else {
                        service_requests.write(ServiceRequest(
                            ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error("no tab for agent".to_string()),
                            },
                        ));
                        continue;
                    };
                    let current_dir = workspace.tabs.get(tab_entity).ok().and_then(|tab| {
                        WorkspaceDirectory::stored(tab.startup_dir.as_deref())
                            .ok()
                            .flatten()
                    });
                    if let Some(current_dir) = current_dir.as_deref()
                        && vmux_git::worktree::is_linked_worktree(current_dir)
                    {
                        AgentCommandResult::Text(current_dir.to_string_lossy().into_owned())
                    } else {
                        let project_dir = workspace
                            .workspaces
                            .get(tab_entity)
                            .ok()
                            .and_then(|workspace| {
                                WorkspaceDirectory::stored(Some(&workspace.project_dir))
                                    .ok()
                                    .flatten()
                            })
                            .or_else(|| {
                                workspace
                                    .pending_projects
                                    .get(tab_entity)
                                    .ok()
                                    .map(|project| project.0.clone())
                            })
                            .or(current_dir);
                        let Some(project_dir) = project_dir else {
                            service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error(
                                    "No Git project selected. Complete select_project and initialize Git first."
                                        .to_string(),
                                ),
                            }));
                            continue;
                        };
                        if vmux_git::worktree::CheckoutInfo::try_from(project_dir.as_path())
                            .is_err()
                        {
                            AgentCommandResult::Text(project_dir.to_string_lossy().into_owned())
                        } else {
                            let candidate = if *create {
                                Ok(None)
                            } else {
                                match path.as_deref() {
                                    Some(path) => ExistingWorktreeCandidates::resolve(
                                        &project_dir,
                                        Path::new(path),
                                    )
                                    .map(Some),
                                    None => ExistingWorktreeCandidates::for_project(&project_dir)
                                        .and_then(ExistingWorktreeCandidates::automatic),
                                }
                            };
                            match candidate {
                                Err(error) => AgentCommandResult::Error(error),
                                Ok(Some(candidate)) => match workspace.activate_directory(
                                    tab_entity,
                                    agent_entity,
                                    &project_dir,
                                    &candidate.execution_dir,
                                    &mut commands,
                                ) {
                                    Ok(rebind) => {
                                        if let Some(message) = rebind {
                                            service_requests.write(ServiceRequest(message));
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
                                            workspace
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
                                        Ok(activation) => match workspace.activate_worktree(
                                            tab_entity,
                                            agent_entity,
                                            &project_dir,
                                            activation,
                                            &mut commands,
                                        ) {
                                            Ok((execution_dir, rebind)) => {
                                                if let Some(message) = rebind {
                                                    service_requests.write(ServiceRequest(message));
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
            }
        } else if let Ok(Some(command)) = request.decode::<AgentCreateWorktree>() {
            let anchor = &command.anchor;
            match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((_, pane)) => {
                    let mut cur = pane;
                    let tab_e = loop {
                        if workspace.tabs.get(cur).is_ok() {
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
                            if workspace.worktrees.get(tab_e).is_ok()
                                || worktree_created_this_batch.contains_key(&tab_e) =>
                        {
                            let tab_dir = workspace
                                .tabs
                                .get(tab_e)
                                .ok()
                                .and_then(|t| t.startup_dir.clone());
                            match WorkspaceDirectory::stored(tab_dir.as_deref()) {
                                Ok(Some(path)) => {
                                    AgentCommandResult::Text(path.to_string_lossy().into_owned())
                                }
                                Ok(None) => AgentCommandResult::Error(
                                    "tab project directory is missing".to_string(),
                                ),
                                Err(message) => AgentCommandResult::Error(message),
                            }
                        }
                        Some(tab_e) => {
                            let tab_dir = workspace
                                .tabs
                                .get(tab_e)
                                .ok()
                                .and_then(|t| t.startup_dir.clone());
                            let name = workspace
                                .tabs
                                .get(tab_e)
                                .map(|t| t.name.clone())
                                .unwrap_or_default();
                            match WorkspaceDirectory::stored(tab_dir.as_deref()) {
                                Err(message) => AgentCommandResult::Error(message),
                                Ok(stored) => 'create_worktree: {
                                    let configured_dir = active_space
                                        .id()
                                        .and_then(|space_id| settings.startup_dir(space_id));
                                    let workspace_dir = workspace
                                        .workspaces
                                        .get(tab_e)
                                        .ok()
                                        .and_then(|workspace| {
                                            WorkspaceDirectory::stored(Some(&workspace.project_dir))
                                                .ok()
                                                .flatten()
                                        });
                                    let Some(current_dir) =
                                        stored.or(configured_dir).or_else(|| workspace_dir.clone())
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
                                        let base_dir =
                                            workspace_dir.unwrap_or_else(|| current_dir.clone());
                                        if workspace.workspaces.get(tab_e).is_err() {
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
                                                if let Ok(mut t) = workspace.tabs.get_mut(tab_e) {
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
                                                worktree_created_this_batch.insert(tab_e, branch);
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
        } else if let Ok(Some(command)) = request.decode::<AgentCreateWorktreeOnBranch>() {
            let anchor = &command.anchor;
            let branch = &command.branch;
            let project = &command.project;
            match resolve_self_pane(*anchor, &agent_terms, &ctx.child_of_q) {
                None => AgentCommandResult::Error("agent pane not found".to_string()),
                Some((agent_entity, pane)) => {
                    let Some(tab_entity) =
                        ancestor_self_tab(pane, &workspace.tabs, &ctx.child_of_q)
                    else {
                        blocked_requests.write(AgentRequestBlocked {
                            anchor: *anchor,
                            reason: "Skipped because worktree activation did not complete."
                                .to_string(),
                        });
                        service_requests.write(ServiceRequest(
                            ClientMessage::AgentCommandResponse {
                                request_id: request.request_id,
                                result: AgentCommandResult::Error("no tab for agent".to_string()),
                            },
                        ));
                        continue;
                    };
                    let existing_branch = workspace
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
                            let path = workspace
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
                        let base_dir =
                            project
                                .as_ref()
                                .and_then(|picked| {
                                    WorkspaceDirectory::stored(Some(picked)).ok().flatten()
                                })
                                .or_else(|| {
                                    workspace
                                        .pending_projects
                                        .get(tab_entity)
                                        .ok()
                                        .map(|project| project.0.clone())
                                })
                                .or_else(|| {
                                    workspace.workspaces.get(tab_entity).ok().and_then(
                                        |workspace| {
                                            WorkspaceDirectory::stored(Some(&workspace.project_dir))
                                                .ok()
                                                .flatten()
                                        },
                                    )
                                });
                        let Some(base_dir) = base_dir else {
                            blocked_requests.write(AgentRequestBlocked {
                                anchor: *anchor,
                                reason: "Skipped because worktree activation did not complete."
                                    .to_string(),
                            });
                            service_requests.write(ServiceRequest(
                                ClientMessage::AgentCommandResponse {
                                    request_id: request.request_id,
                                    result: AgentCommandResult::Error(
                                        "No project selected. Call select_project first."
                                            .to_string(),
                                    ),
                                },
                            ));
                            continue;
                        };
                        match vmux_layout::worktree::create_worktree_for_branch_blocking(
                            &base_dir,
                            branch,
                            &managed_root,
                        ) {
                            Ok(activation) => match workspace.activate_worktree(
                                tab_entity,
                                agent_entity,
                                &base_dir,
                                activation,
                                &mut commands,
                            ) {
                                Ok((execution_dir, rebind)) => {
                                    if let Some(message) = rebind {
                                        service_requests.write(ServiceRequest(message));
                                    }
                                    worktree_created_this_batch.insert(tab_entity, branch.clone());
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
        } else {
            continue;
        };
        if matches!(result, AgentCommandResult::Error(_))
            && matches!(
                request.request.id.as_str(),
                AgentCreateWorktree::ID
                    | AgentCreateWorktreeOnBranch::ID
                    | AgentPrepareWorktree::ID
            )
            && let Some(anchor) = request_anchor
        {
            blocked_requests.write(AgentRequestBlocked {
                anchor,
                reason: "Skipped because worktree activation did not complete.".to_string(),
            });
        }
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: request.request_id,
            result,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worktree_failures_publish_the_anchor_gate() {
        let anchor = ProcessId::new();
        let create = AgentRequestInput {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::User,
            request: AgentRequest::encode(&AgentCreateWorktreeOnBranch {
                anchor,
                branch: "feature/test".into(),
                project: None,
            })
            .unwrap(),
        };
        assert_eq!(workspace_request_anchor(&create), Some(anchor));
    }
}
