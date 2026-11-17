use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
#[cfg(test)]
use vmux_api::protocol::{AgentRequest, AgentRequestId};
use vmux_command::WriteCommandRequests;
use vmux_ecs::agent::CommandOrigin;
use vmux_ecs::agent::{
    AgentRequestBlocked, AgentRequestDispatchPlugin, AgentRequestDispatchSet, AgentRequestGate,
    AgentRequestGateFailed, AgentRequestInput, AgentRequestPrerequisiteSet, AgentRequestReleaseSet,
};
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_session::host::USER_CHOICE_REQUESTED;
use vmux_setting::{AppSettings, StartupDir};

use super::agent::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktree,
    AgentCreateWorktreeOnBranch, AgentPrepareWorktree,
};
use super::workspace::{
    AgentWorkspacePicker, AgentWorkspaceState, ExistingWorktreeCandidates, PendingWorkspacePicker,
    WORKSPACE_SELECTION_PENDING, WORKSPACE_SELECTION_REQUESTED,
};

use vmux_api::protocol::{AgentCommandResult, ClientMessage};

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
        if !app.is_plugin_added::<AgentRequestDispatchPlugin>() {
            app.add_plugins(AgentRequestDispatchPlugin);
        }
        if !app.is_plugin_added::<vmux_layout::worktree::CheckoutPlugin>() {
            app.add_plugins(vmux_layout::worktree::CheckoutPlugin);
        }
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentRequestBlocked>()
            .add_message::<ServiceRequest>()
            .add_systems(
                Update,
                accept
                    .before(AgentRequestDispatchSet)
                    .after(AgentRequestReleaseSet),
            )
            .add_systems(
                Update,
                (
                    (
                        choose,
                        prepare,
                        prepare_candidate,
                        create,
                        named,
                        submit_create,
                        submit_branch,
                    )
                        .chain(),
                    (retire, applied, commit, finish, reply).chain(),
                )
                    .chain()
                    .in_set(AgentWorkspaceRequestSet)
                    .in_set(AgentRequestPrerequisiteSet)
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet)
                    .after(vmux_layout::worktree::TabDirectoryRebindSet),
            );
    }
}

#[derive(Component)]
struct WorkspaceOperation;

#[derive(Component)]
struct WaitingCheckout {
    checkout: Entity,
    tab: Entity,
    agent: Entity,
    project: PathBuf,
    revision: u64,
}

#[derive(Component)]
struct PlainReply;

#[derive(Component)]
struct AppliedCheckout;

#[derive(Component)]
struct WorkspaceFailure(String);

#[derive(Component)]
struct PrepareInput(AgentPrepareWorktree);
#[derive(Component)]
struct CreateInput(AgentCreateWorktree);
#[derive(Component)]
struct NamedInput(AgentCreateWorktreeOnBranch);
#[derive(Component)]
struct ChoiceInput(WorkspaceChoice);
#[derive(Component)]
struct CandidateIntent {
    agent: Entity,
    tab: Entity,
    project: PathBuf,
}

#[derive(Component)]
struct CreateIntent {
    tab: Entity,
    agent: Entity,
    project: PathBuf,
    hint: String,
}
#[derive(Component)]
struct BranchIntent {
    tab: Entity,
    agent: Entity,
    project: PathBuf,
    branch: String,
}
#[derive(Component)]
struct WorkspaceResult(AgentCommandResult);

#[derive(bevy::ecs::system::SystemParam)]
struct Actors<'w, 's> {
    terms: Query<'w, 's, (Entity, &'static ProcessId, &'static ChildOf)>,
    parents: Query<'w, 's, &'static ChildOf>,
    tabs: Query<'w, 's, Entity, With<vmux_layout::tab::Tab>>,
}

impl Actors<'_, '_> {
    fn resolve(&self, anchor: ProcessId) -> Result<(Entity, Entity), String> {
        for (agent, process, parent) in &self.terms {
            if *process != anchor {
                continue;
            }
            let mut current = parent.parent();
            loop {
                if self.tabs.contains(current) {
                    return Ok((agent, current));
                }
                current = self
                    .parents
                    .get(current)
                    .map_err(|_| "no tab for agent".to_string())?
                    .parent();
            }
        }
        Err("agent pane not found".into())
    }
}

fn accept(
    mut requests: MessageReader<AgentRequestInput>,
    gates: Query<&AgentRequestGate>,
    mut commands: Commands,
) {
    let mut accepted = std::collections::HashSet::new();
    for request in requests.read() {
        let Some(anchor) = workspace_request_anchor(request) else {
            continue;
        };
        let sid = match &request.origin {
            CommandOrigin::Agent { sid, .. } => sid.clone(),
            _ => None,
        };
        if gates.iter().any(|gate| {
            gate.anchor == anchor
                || sid
                    .as_ref()
                    .is_some_and(|sid| gate.sid.as_ref() == Some(sid))
        }) || !accepted.insert(anchor)
        {
            continue;
        }
        let mut operation = commands.spawn((
            Name::new("Agent workspace prerequisite"),
            WorkspaceOperation,
            vmux_ecs::Operation::bundle("agent.workspace")
                .expect("fixed workspace kind")
                .running(),
            AgentRequestGate {
                anchor,
                sid,
                request_id: request.request_id,
                active: false,
            },
        ));
        if let Some(choice) = WorkspaceChoice::decode(request) {
            operation.insert(ChoiceInput(choice));
        } else if let Ok(Some(payload)) = request.decode::<AgentPrepareWorktree>() {
            operation.insert(PrepareInput(payload));
        } else if let Ok(Some(payload)) = request.decode::<AgentCreateWorktree>() {
            operation.insert(CreateInput(payload));
        } else if let Ok(Some(payload)) = request.decode::<AgentCreateWorktreeOnBranch>() {
            operation.insert(NamedInput(payload));
        }
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

fn choose(
    requests: Query<(Entity, &AgentRequestGate, &ChoiceInput)>,
    actors: Actors,
    picker: AgentWorkspacePicker,
    mut commands: Commands,
) {
    let mut pending = picker.pending_tabs();
    for (operation, gate, input) in &requests {
        if !gate.active {
            continue;
        }
        let (agent, tab) = match actors.resolve(input.0.anchor) {
            Ok(target) => target,
            Err(error) => {
                commands
                    .entity(operation)
                    .insert(WorkspaceResult(AgentCommandResult::Error(error)))
                    .remove::<ChoiceInput>();
                continue;
            }
        };
        let Some(session) = picker.session(agent, &actors.parents) else {
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Error(
                    "agent session not found".into(),
                )))
                .remove::<ChoiceInput>();
            continue;
        };
        let result = if picker.choice_pending(agent) {
            AgentCommandResult::Text(USER_CHOICE_REQUESTED.into())
        } else if !pending.insert(tab) {
            AgentCommandResult::Text(WORKSPACE_SELECTION_PENDING.into())
        } else {
            let selected = input
                .0
                .path
                .as_ref()
                .and_then(|path| Path::new(path).canonicalize().ok())
                .filter(|path| path.is_dir());
            let task = match selected {
                Some(path) if picker.trusted(&path) => picker.accept(path),
                path => picker.choose(path),
            };
            commands.spawn(PendingWorkspacePicker::new(tab, agent, session, task));
            AgentCommandResult::Text(WORKSPACE_SELECTION_REQUESTED.into())
        };
        commands
            .entity(operation)
            .insert(WorkspaceResult(result))
            .remove::<ChoiceInput>();
    }
}

fn prepare(
    requests: Query<(Entity, &AgentRequestGate, &PrepareInput), Without<CandidateIntent>>,
    actors: Actors,
    workspace: AgentWorkspaceState,
    mut commands: Commands,
) {
    for (operation, gate, input) in &requests {
        if !gate.active {
            continue;
        }
        let (agent, tab) = match actors.resolve(input.0.anchor) {
            Ok(target) => target,
            Err(error) => {
                commands
                    .entity(operation)
                    .insert(WorkspaceResult(AgentCommandResult::Error(error)))
                    .remove::<PrepareInput>();
                continue;
            }
        };
        let current = workspace.tabs.get(tab).ok().and_then(|tab| {
            WorkspaceDirectory::stored(tab.startup_dir.as_deref())
                .ok()
                .flatten()
        });
        if let Some(current) = &current
            && vmux_git::worktree::CheckoutInfo::is_linked(current)
        {
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Text(
                    current.to_string_lossy().into_owned(),
                )))
                .remove::<PrepareInput>();
            continue;
        }
        let project = workspace
            .workspaces
            .get(tab)
            .ok()
            .and_then(|workspace| {
                WorkspaceDirectory::stored(Some(&workspace.project_dir))
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                workspace
                    .pending_projects
                    .get(tab)
                    .ok()
                    .map(|project| project.0.clone())
            })
            .or(current);
        let Some(project) = project else {
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Error(
                    "No Git project selected. Complete select_project and initialize Git first."
                        .into(),
                )))
                .remove::<PrepareInput>();
            continue;
        };
        if vmux_git::worktree::CheckoutInfo::try_from(project.as_path()).is_err() {
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Text(
                    project.to_string_lossy().into_owned(),
                )))
                .remove::<PrepareInput>();
            continue;
        }
        commands.entity(operation).insert(CandidateIntent {
            agent,
            tab,
            project,
        });
    }
}

fn prepare_candidate(
    requests: Query<(Entity, &PrepareInput, &CandidateIntent)>,
    mut workspace: AgentWorkspaceState,
    mut service: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for (operation, input, candidate) in &requests {
        let agent = candidate.agent;
        let tab = candidate.tab;
        let project = candidate.project.clone();
        let candidate = if input.0.create {
            Ok(None)
        } else {
            match input.0.path.as_deref() {
                Some(path) => {
                    ExistingWorktreeCandidates::resolve(&project, Path::new(path)).map(Some)
                }
                None => ExistingWorktreeCandidates::for_project(&project)
                    .and_then(ExistingWorktreeCandidates::automatic),
            }
        };
        match candidate {
            Ok(Some(candidate)) => {
                let result = match workspace.activate_directory(
                    tab,
                    agent,
                    &project,
                    &candidate.execution_dir,
                    &mut commands,
                ) {
                    Ok(rebind) => {
                        if let Some(message) = rebind {
                            service.write(ServiceRequest(message));
                        }
                        AgentCommandResult::Text(format!(
                            "Worktree ready: {}\nContinue the original request immediately in this directory. Do not stop after setup or search for optional tools.",
                            candidate.execution_dir.display()
                        ))
                    }
                    Err(error) => AgentCommandResult::Error(error),
                };
                commands.entity(operation).insert(WorkspaceResult(result));
            }
            Ok(None) => {
                let name = input
                    .0
                    .task
                    .as_deref()
                    .filter(|task| !task.trim().is_empty())
                    .map(str::to_string)
                    .or_else(|| workspace.tabs.get(tab).ok().map(|tab| tab.name.clone()))
                    .unwrap_or_else(|| "task".into());
                let hint = vmux_layout::worktree::WorktreeName::hint(&name, &project);
                commands.entity(operation).insert(CreateIntent {
                    tab,
                    agent,
                    project,
                    hint,
                });
            }
            Err(error) => {
                commands
                    .entity(operation)
                    .insert(WorkspaceResult(AgentCommandResult::Error(error)));
            }
        }
        commands
            .entity(operation)
            .remove::<(PrepareInput, CandidateIntent)>();
    }
}

fn create(
    requests: Query<(Entity, &AgentRequestGate, &CreateInput)>,
    actors: Actors,
    workspace: AgentWorkspaceState,
    active: vmux_layout::space::FocusedSpace,
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    for (operation, gate, input) in &requests {
        if !gate.active {
            continue;
        }
        let (agent, tab) = match actors.resolve(input.0.anchor) {
            Ok(target) => target,
            Err(error) => {
                commands
                    .entity(operation)
                    .insert(WorkspaceResult(AgentCommandResult::Error(error)))
                    .remove::<CreateInput>();
                continue;
            }
        };
        let current = workspace.tabs.get(tab).ok().and_then(|tab| {
            WorkspaceDirectory::stored(tab.startup_dir.as_deref())
                .ok()
                .flatten()
        });
        if workspace.worktrees.get(tab).is_ok()
            || current
                .as_ref()
                .is_some_and(|current| vmux_git::worktree::CheckoutInfo::is_linked(current))
        {
            let result = current
                .map(|path| AgentCommandResult::Text(path.to_string_lossy().into_owned()))
                .unwrap_or_else(|| {
                    AgentCommandResult::Error("tab worktree directory is missing".into())
                });
            commands
                .entity(operation)
                .insert(WorkspaceResult(result))
                .remove::<CreateInput>();
            continue;
        }
        let configured = active.id().and_then(|space| settings.startup_dir(space));
        let selected = workspace.workspaces.get(tab).ok().and_then(|workspace| {
            WorkspaceDirectory::stored(Some(&workspace.project_dir))
                .ok()
                .flatten()
        });
        let Some(current) = current.or(configured).or_else(|| selected.clone()) else {
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Error(
                    "tab project directory is missing".into(),
                )))
                .remove::<CreateInput>();
            continue;
        };
        let project = selected.unwrap_or(current);
        let name = workspace
            .tabs
            .get(tab)
            .map(|tab| tab.name.clone())
            .unwrap_or_default();
        let hint = vmux_layout::worktree::WorktreeName::hint(&name, &project);
        commands
            .entity(operation)
            .insert((
                PlainReply,
                CreateIntent {
                    tab,
                    agent,
                    project,
                    hint,
                },
            ))
            .remove::<CreateInput>();
    }
}

fn named(
    requests: Query<(Entity, &AgentRequestGate, &NamedInput)>,
    actors: Actors,
    workspace: AgentWorkspaceState,
    mut commands: Commands,
) {
    for (operation, gate, input) in &requests {
        if !gate.active {
            continue;
        }
        let (agent, tab) = match actors.resolve(input.0.anchor) {
            Ok(target) => target,
            Err(error) => {
                commands
                    .entity(operation)
                    .insert(WorkspaceResult(AgentCommandResult::Error(error)))
                    .remove::<NamedInput>();
                continue;
            }
        };
        let branch = input.0.branch.trim();
        if let Ok(worktree) = workspace.worktrees.get(tab) {
            let result = if worktree.branch != branch {
                AgentCommandResult::Error(format!(
                    "Tab already has a worktree on branch {}; requested {branch}",
                    worktree.branch
                ))
            } else {
                workspace
                    .tabs
                    .get(tab)
                    .ok()
                    .and_then(|tab| tab.startup_dir.clone())
                    .map(AgentCommandResult::Text)
                    .unwrap_or_else(|| {
                        AgentCommandResult::Error("tab worktree directory is missing".into())
                    })
            };
            commands
                .entity(operation)
                .insert(WorkspaceResult(result))
                .remove::<NamedInput>();
            continue;
        }
        let project = input
            .0
            .project
            .as_deref()
            .and_then(|path| WorkspaceDirectory::stored(Some(path)).ok().flatten())
            .or_else(|| {
                workspace
                    .pending_projects
                    .get(tab)
                    .ok()
                    .map(|project| project.0.clone())
            })
            .or_else(|| {
                workspace.workspaces.get(tab).ok().and_then(|workspace| {
                    WorkspaceDirectory::stored(Some(&workspace.project_dir))
                        .ok()
                        .flatten()
                })
            });
        match project {
            Some(project) => {
                commands.entity(operation).insert(BranchIntent {
                    tab,
                    agent,
                    project,
                    branch: branch.into(),
                });
            }
            None => {
                commands
                    .entity(operation)
                    .insert(WorkspaceResult(AgentCommandResult::Error(
                        "No project selected. Call select_project first.".into(),
                    )));
            }
        }
        commands.entity(operation).remove::<NamedInput>();
    }
}

fn submit_create(
    requests: Query<(Entity, &CreateIntent)>,
    root: Option<Res<vmux_layout::worktree::ManagedWorktreeRoot>>,
    revisions: Query<&vmux_layout::worktree::CheckoutTargetRevision>,
    mut creates: MessageWriter<vmux_layout::worktree::CreateCheckout>,
    mut commands: Commands,
) {
    for (operation, input) in &requests {
        let revision = revisions
            .get(input.tab)
            .map_or(1, |current| current.0.wrapping_add(1).max(1));
        let checkout = commands
            .spawn((
                Name::new("Agent generated checkout"),
                vmux_ecs::Operation::bundle("agent.checkout.create").expect("fixed checkout kind"),
                vmux_ecs::OperationTarget::new(input.tab),
                vmux_ecs::OperationTargetRevision(revision),
            ))
            .id();
        creates.write(vmux_layout::worktree::CreateCheckout {
            operation: checkout,
            directory: input.project.clone(),
            managed_root: root.as_deref().cloned().unwrap_or_default().0,
            slug_hint: input.hint.clone(),
        });
        commands
            .entity(input.tab)
            .insert(vmux_layout::worktree::CheckoutTargetRevision(revision));
        commands
            .entity(operation)
            .insert(WaitingCheckout {
                checkout,
                tab: input.tab,
                agent: input.agent,
                project: input.project.clone(),
                revision,
            })
            .remove::<CreateIntent>();
    }
}

fn submit_branch(
    requests: Query<(Entity, &BranchIntent)>,
    root: Option<Res<vmux_layout::worktree::ManagedWorktreeRoot>>,
    revisions: Query<&vmux_layout::worktree::CheckoutTargetRevision>,
    mut creates: MessageWriter<vmux_layout::worktree::CreateBranchCheckout>,
    mut commands: Commands,
) {
    for (operation, input) in &requests {
        let revision = revisions
            .get(input.tab)
            .map_or(1, |current| current.0.wrapping_add(1).max(1));
        let checkout = commands
            .spawn((
                Name::new("Agent named checkout"),
                vmux_ecs::Operation::bundle("agent.checkout.branch").expect("fixed checkout kind"),
                vmux_ecs::OperationTarget::new(input.tab),
                vmux_ecs::OperationTargetRevision(revision),
            ))
            .id();
        creates.write(vmux_layout::worktree::CreateBranchCheckout {
            operation: checkout,
            directory: input.project.clone(),
            managed_root: root.as_deref().cloned().unwrap_or_default().0,
            branch: input.branch.clone(),
        });
        commands
            .entity(input.tab)
            .insert(vmux_layout::worktree::CheckoutTargetRevision(revision));
        commands
            .entity(operation)
            .insert(WaitingCheckout {
                checkout,
                tab: input.tab,
                agent: input.agent,
                project: input.project.clone(),
                revision,
            })
            .remove::<BranchIntent>();
    }
}

fn applied(
    mut prepared: MessageReader<vmux_layout::worktree::CheckoutPrepared>,
    resolved: Query<&vmux_layout::worktree::PreparedCheckout>,
    requests: Query<(Entity, &WaitingCheckout)>,
    revisions: Query<&vmux_layout::worktree::CheckoutTargetRevision>,
    mut workspace: AgentWorkspaceState,
    mut service: MessageWriter<ServiceRequest>,
    mut commands: Commands,
) {
    for prepared in prepared.read() {
        let Ok(checkout) = resolved.get(prepared.operation) else {
            continue;
        };
        for (operation, waiting) in &requests {
            if waiting.checkout != prepared.operation {
                continue;
            }
            if !revisions
                .get(waiting.tab)
                .is_ok_and(|revision| revision.0 == waiting.revision)
            {
                commands.entity(operation).insert(WorkspaceFailure(
                    "workspace selection changed before activation".into(),
                ));
                continue;
            }
            match workspace.activate_worktree(
                waiting.tab,
                waiting.agent,
                &waiting.project,
                checkout.activation.clone(),
                &mut commands,
            ) {
                Ok((_, rebind)) => {
                    if let Some(message) = rebind {
                        service.write(ServiceRequest(message));
                    }
                    commands.entity(operation).insert(AppliedCheckout);
                }
                Err(error) => {
                    commands.entity(operation).insert(WorkspaceFailure(error));
                }
            }
        }
    }
}

fn retire(
    requests: Query<(Entity, &AgentRequestGate, &WaitingCheckout)>,
    actors: Actors,
    revisions: Query<&vmux_layout::worktree::CheckoutTargetRevision>,
    mut cancels: MessageWriter<vmux_layout::worktree::CancelCheckout>,
    mut commands: Commands,
) {
    for (operation, gate, waiting) in &requests {
        if actors
            .resolve(gate.anchor)
            .is_ok_and(|(agent, tab)| agent == waiting.agent && tab == waiting.tab)
            && revisions
                .get(waiting.tab)
                .is_ok_and(|revision| revision.0 == waiting.revision)
        {
            continue;
        }
        cancels.write(vmux_layout::worktree::CancelCheckout {
            operation: waiting.checkout,
        });
        commands.entity(operation).insert(WorkspaceFailure(
            "worktree owner or workspace selection changed".into(),
        ));
    }
}

fn commit(
    requests: Query<&WaitingCheckout, Added<AppliedCheckout>>,
    mut commits: MessageWriter<vmux_layout::worktree::CommitCheckout>,
) {
    for waiting in &requests {
        commits.write(vmux_layout::worktree::CommitCheckout {
            operation: waiting.checkout,
            target: waiting.tab,
            revision: waiting.revision,
        });
    }
}

fn finish(
    mut failed: MessageReader<vmux_layout::worktree::CheckoutFailed>,
    requests: Query<(
        Entity,
        &WaitingCheckout,
        Has<PlainReply>,
        Option<&WorkspaceFailure>,
    )>,
    resolved: Query<&vmux_layout::worktree::PreparedCheckout>,
    states: Query<&vmux_ecs::OperationState>,
    mut cancels: MessageWriter<vmux_layout::worktree::CancelCheckout>,
    mut commands: Commands,
) {
    for failure in failed.read() {
        for (operation, waiting, _, _) in &requests {
            if waiting.checkout == failure.operation {
                commands
                    .entity(operation)
                    .insert(WorkspaceFailure(failure.error.clone()));
            }
        }
    }
    for (operation, waiting, plain, error) in &requests {
        if let Some(error) = error {
            cancels.write(vmux_layout::worktree::CancelCheckout {
                operation: waiting.checkout,
            });
            if !states.get(waiting.checkout).is_ok_and(|state| {
                matches!(
                    state,
                    vmux_ecs::OperationState::Succeeded
                        | vmux_ecs::OperationState::Failed
                        | vmux_ecs::OperationState::Canceled
                )
            }) {
                continue;
            }
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Error(error.0.clone())))
                .remove::<WaitingCheckout>();
            continue;
        }
        if states
            .get(waiting.checkout)
            .is_ok_and(|state| *state == vmux_ecs::OperationState::Succeeded)
        {
            let Ok(checkout) = resolved.get(waiting.checkout) else {
                continue;
            };
            let path = checkout
                .activation
                .execution_dir
                .to_string_lossy()
                .into_owned();
            let text = if plain {
                path
            } else {
                format!(
                    "Worktree ready: {path}\nContinue the original request immediately in this directory. Do not stop after setup or search for optional tools."
                )
            };
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Text(text)))
                .remove::<WaitingCheckout>();
        } else if states
            .get(waiting.checkout)
            .is_ok_and(|state| *state == vmux_ecs::OperationState::Canceled)
        {
            commands
                .entity(operation)
                .insert(WorkspaceResult(AgentCommandResult::Error(
                    "worktree activation was canceled".into(),
                )))
                .remove::<WaitingCheckout>();
        }
    }
}

fn reply(
    requests: Query<(Entity, &AgentRequestGate, &WorkspaceResult)>,
    mut service: MessageWriter<ServiceRequest>,
    mut blocked: MessageWriter<AgentRequestBlocked>,
    mut failures: MessageWriter<AgentRequestGateFailed>,
    mut commands: Commands,
) {
    for (operation, gate, result) in &requests {
        if matches!(result.0, AgentCommandResult::Error(_)) {
            let reason = "Skipped because worktree activation did not complete.".to_string();
            blocked.write(AgentRequestBlocked {
                anchor: gate.anchor,
                reason: reason.clone(),
            });
            failures.write(AgentRequestGateFailed {
                anchor: gate.anchor,
                reason,
            });
        }
        service.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: gate.request_id,
            result: result.0.clone(),
        }));
        let state = if matches!(result.0, AgentCommandResult::Error(_)) {
            vmux_ecs::OperationState::Failed
        } else {
            vmux_ecs::OperationState::Succeeded
        };
        commands.entity(operation).insert(state).remove::<(
            AgentRequestGate,
            WorkspaceResult,
            WorkspaceOperation,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::agent::{AgentCommandResponse, AgentRequestReady};
    use vmux_ecs::{Cwd, EntityTarget};
    use vmux_session::{Session, SessionId};

    struct WorkspaceFixture {
        app: App,
        root: tempfile::TempDir,
        repository: tempfile::TempDir,
        anchor: ProcessId,
        tab: Entity,
        session: Entity,
        service: bevy::ecs::message::MessageCursor<ServiceRequest>,
        ready: bevy::ecs::message::MessageCursor<AgentRequestReady>,
        responses: bevy::ecs::message::MessageCursor<AgentCommandResponse>,
    }

    impl WorkspaceFixture {
        fn new() -> Self {
            let repository = tempfile::tempdir().unwrap();
            let root = tempfile::tempdir().unwrap();
            for args in [
                vec!["init", "--quiet", "-b", "main"],
                vec![
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "--allow-empty",
                    "--no-gpg-sign",
                    "--quiet",
                    "--message",
                    "initial",
                ],
            ] {
                assert!(
                    std::process::Command::new("git")
                        .current_dir(repository.path())
                        .args(args)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            let project = repository.path().canonicalize().unwrap();
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, AgentWorkspaceRequestPlugin))
                .init_resource::<AppSettings>()
                .insert_resource(vmux_layout::worktree::ManagedWorktreeRoot(
                    root.path().into(),
                ));
            let anchor = ProcessId::new();
            let tab = app
                .world_mut()
                .spawn((
                    vmux_layout::tab::Tab {
                        name: "Task".into(),
                        startup_dir: Some(project.to_string_lossy().into_owned()),
                    },
                    vmux_layout::tab::TabWorkspace {
                        project_dir: project.to_string_lossy().into_owned(),
                    },
                ))
                .id();
            let pane = app.world_mut().spawn(ChildOf(tab)).id();
            let session = app
                .world_mut()
                .spawn((Session, SessionId("routing-session".into()), Cwd(project)))
                .id();
            let stack = app
                .world_mut()
                .spawn((EntityTarget::<Session>::new(session), ChildOf(pane)))
                .id();
            app.world_mut().spawn((anchor, ChildOf(stack)));
            Self {
                app,
                root,
                repository,
                anchor,
                tab,
                session,
                service: Default::default(),
                ready: Default::default(),
                responses: Default::default(),
            }
        }

        fn named(&mut self, branch: &str) -> AgentRequestId {
            let request_id = AgentRequestId::new();
            self.app.world_mut().write_message(AgentRequestInput {
                request_id,
                origin: CommandOrigin::Agent {
                    sid: Some("routing-session".into()),
                    anchor: None,
                },
                request: AgentRequest::encode(&AgentCreateWorktreeOnBranch {
                    anchor: self.anchor,
                    branch: branch.into(),
                    project: None,
                })
                .unwrap(),
            });
            request_id
        }

        fn following(&mut self) -> AgentRequestId {
            let request_id = AgentRequestId::new();
            self.app.world_mut().write_message(AgentRequestInput {
                request_id,
                origin: CommandOrigin::Agent {
                    sid: Some("routing-session".into()),
                    anchor: None,
                },
                request: AgentRequest::encode(&vmux_api::protocol::AgentNotify {
                    title: Some("following".into()),
                    body: None,
                })
                .unwrap(),
            });
            request_id
        }
    }

    #[test]
    fn checkout_commit_and_acp_rebind_precede_reply_and_following_command_delivery() {
        let mut fixture = WorkspaceFixture::new();
        let setup = fixture.named("feature/test");
        let following = fixture.following();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut rebind = false;
        let mut replied = false;
        loop {
            fixture.app.update();
            for request in fixture
                .service
                .read(fixture.app.world().resource::<Messages<ServiceRequest>>())
            {
                match &request.0 {
                    ClientMessage::RebindAcpWorkspace { sid, cwd } => {
                        assert_eq!(sid, "routing-session");
                        assert_eq!(
                            fixture.app.world().get::<Cwd>(fixture.session).unwrap().0,
                            PathBuf::from(cwd)
                        );
                        rebind = true;
                    }
                    ClientMessage::AgentCommandResponse { request_id, result }
                        if *request_id == setup =>
                    {
                        assert!(rebind);
                        assert!(matches!(result, AgentCommandResult::Text(_)));
                        replied = true;
                    }
                    _ => {}
                }
            }
            let delivered = fixture
                .ready
                .read(
                    fixture
                        .app
                        .world()
                        .resource::<Messages<AgentRequestReady>>(),
                )
                .any(|request| request.request_id == following);
            if delivered {
                assert!(replied);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "workspace batch did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let directory = fixture
            .app
            .world()
            .get::<Cwd>(fixture.session)
            .unwrap()
            .0
            .clone();
        assert!(directory.starts_with(fixture.root.path().canonicalize().unwrap()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<vmux_layout::tab::TabWorktree>(fixture.tab)
                .unwrap()
                .branch,
            "feature/test"
        );
        assert_ne!(directory, fixture.repository.path().canonicalize().unwrap());
    }

    #[test]
    fn failed_workspace_prerequisite_denies_following_commands() {
        let mut fixture = WorkspaceFixture::new();
        let setup = fixture.named("invalid branch");
        let following = fixture.following();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut replied = false;
        loop {
            fixture.app.update();
            assert!(
                !fixture
                    .ready
                    .read(
                        fixture
                            .app
                            .world()
                            .resource::<Messages<AgentRequestReady>>()
                    )
                    .any(|request| request.request_id == following)
            );
            for request in fixture
                .service
                .read(fixture.app.world().resource::<Messages<ServiceRequest>>())
            {
                if let ClientMessage::AgentCommandResponse { request_id, result } = &request.0
                    && *request_id == setup
                {
                    assert!(matches!(result, AgentCommandResult::Error(_)));
                    replied = true;
                }
            }
            if let Some(response) = fixture
                .responses
                .read(
                    fixture
                        .app
                        .world()
                        .resource::<Messages<AgentCommandResponse>>(),
                )
                .find(|response| response.request_id == following)
            {
                assert!(replied);
                assert!(matches!(response.result, AgentCommandResult::Error(_)));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "failed workspace batch did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            fixture.app.world().get::<Cwd>(fixture.session).unwrap().0,
            fixture.repository.path().canonicalize().unwrap()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<vmux_layout::tab::TabWorktree>(fixture.tab)
                .is_none()
        );
    }
}
