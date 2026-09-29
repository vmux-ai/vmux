use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::UiInput;
use vmux_api::protocol::ClientMessage;
use vmux_chat::event::ChatChoiceSelected;
use vmux_chat::host::{ChatSynced, ChatView, PendingAgentChoice};
use vmux_command::WriteCommandRequests;
use vmux_core::agent::{AgentContinuationRequest, AgentSessionRoot};
use vmux_core::service::{ServiceConnected, ServiceMessageSet, ServiceRequest};
#[cfg(test)]
use vmux_core::{AgentWorkingDir, agent::AgentKind};
use vmux_git::worktree::{
    CheckoutInfo, is_linked_worktree, repository_init, worktree_registrations,
};
#[cfg(test)]
use vmux_git::worktree::{worktree_add, worktree_list};
use vmux_layout::tab::{Tab, TabDirDecided, TabWorkspace, TabWorktree, TabWorktreeUnavailable};
use vmux_layout::worktree::{
    ManagedWorktreeRoot, TabWorktreeActivation, TabWorktreeReady, is_generated_tab_name,
};
use vmux_session::{AcpSession, AgentRunState, AgentSession as PageAgentSession};
use vmux_terminal::BufferedAgentPrompt;

use crate::session::AgentSession;

use super::self_command::{ancestor_acp_stack, rebind_acp_workspace};
use vmux_core::profile::ProjectsDirectory;

pub(super) struct WorkspacePlugin;

impl Plugin for WorkspacePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_message::<AgentContinuationRequest>()
            .add_observer(initialize_git_agent_choice)
            .add_systems(
                Update,
                (
                    drain_workspace_picker_tasks.after(super::self_command::SelfCommandSet),
                    queue_agent_continuations,
                    send_pending_agent_continuations.in_set(super::AgentContinuationSet),
                )
                    .chain()
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet),
            );
    }
}

pub(crate) const WORKSPACE_SELECTION_REQUESTED: &str = "Project selection requested. Stop this turn and wait. vmux will resume this same conversation after the user chooses or cancels.";

pub(crate) const WORKSPACE_SELECTION_PENDING: &str = "Project selection is already pending. Stop this turn and wait. vmux will resume this same conversation after the user chooses or cancels.";

const INITIALIZE_GIT_QUESTION: &str = "Initialize Git repository?";

const INITIALIZE_GIT_OPTIONS: [&str; 2] = ["Initialize Git", "Not now"];

#[derive(Component, Clone, Debug)]
pub(crate) struct PendingAgentProject(pub(crate) PathBuf);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct PendingAgentContinuation(String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct InitializeGitAgentChoice {
    pub(crate) tab_entity: Entity,
    pub(crate) workspace: PathBuf,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct RepositoryNeedsWorktree;

#[derive(Component)]
pub(crate) struct PendingWorkspacePicker {
    pub(crate) tab_entity: Entity,
    pub(crate) agent_entity: Entity,
    pub(crate) session_entity: Entity,
    pub(crate) task: Task<Option<PathBuf>>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentWorkspacePicker<'w, 's> {
    pub(crate) pickers: Query<'w, 's, &'static PendingWorkspacePicker>,
    pub(crate) choices: Query<'w, 's, &'static PendingAgentChoice>,
    pub(crate) session_roots: Query<'w, 's, (), With<AgentSessionRoot>>,
    pub(crate) proxy: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
}

fn initialize_git_agent_choice(
    trigger: On<UiInput<ChatChoiceSelected>>,
    choices: Query<(&PendingAgentChoice, &InitializeGitAgentChoice)>,
    mut continuations: MessageWriter<AgentContinuationRequest>,
    tabs: Query<(), With<Tab>>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok((choice, initialize)) = choices.get(event.webview) else {
        return;
    };
    if choice.options.get(event.payload.index as usize).is_none() {
        return;
    }
    let continuation = if !tabs.contains(initialize.tab_entity) {
        failed_workspace_continuation("The project tab no longer exists")
    } else if event.payload.index == 0 {
        match repository_init(&initialize.workspace) {
            Ok(root) => new_git_workspace_ready_continuation(&root),
            Err(error) => git_initialization_failed_continuation(&initialize.workspace, &error.0),
        }
    } else {
        plain_workspace_ready_continuation(&initialize.workspace)
    };
    continuations.write(AgentContinuationRequest {
        session: choice.session_entity,
        context: continuation,
    });
    commands
        .entity(event.webview)
        .remove::<(PendingAgentChoice, InitializeGitAgentChoice)>()
        .remove::<ChatSynced>();
}

fn queue_agent_continuations(
    mut requests: MessageReader<AgentContinuationRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        commands
            .entity(request.session)
            .insert(PendingAgentContinuation(request.context.clone()));
    }
}

pub(crate) fn workspace_picker_task(
    requested: Option<PathBuf>,
    proxy: Option<&bevy::winit::EventLoopProxyWrapper>,
) -> Task<Option<PathBuf>> {
    let wake = proxy.map(|proxy| (**proxy).clone());
    let initial_dir = requested
        .filter(|path| path.is_dir())
        .or_else(|| {
            ProjectsDirectory::ensure()
                .ok()
                .map(ProjectsDirectory::into_path)
        })
        .or_else(|| std::env::current_dir().ok().filter(|path| path.is_dir()))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"));
    IoTaskPool::get().spawn(async move {
        let selected = rfd::AsyncFileDialog::new()
            .set_title("Choose existing project")
            .set_directory(initial_dir)
            .pick_folder()
            .await
            .map(|handle| handle.path().to_path_buf());
        if let Some(wake) = wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
        selected
    })
}

pub(crate) fn workspace_path_task(
    path: PathBuf,
    proxy: Option<&bevy::winit::EventLoopProxyWrapper>,
) -> Task<Option<PathBuf>> {
    let wake = proxy.map(|proxy| (**proxy).clone());
    IoTaskPool::get().spawn(async move {
        if let Some(wake) = wake {
            let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
        Some(path)
    })
}

fn bind_tab_workspace(tab: &mut Tab, project_dir: &Path, execution_dir: &Path) {
    tab.startup_dir = Some(execution_dir.to_string_lossy().into_owned());
    if is_generated_tab_name(&tab.name)
        && let Some(name) = project_dir.file_name().and_then(|name| name.to_str())
        && !name.is_empty()
    {
        tab.name = name.to_string();
    }
}

fn git_workspace_ready_continuation(path: &Path) -> String {
    format!(
        "VMUX PROJECT SELECTION COMPLETED: Git project {} is ready for reading and inspection. Continue the original user request in this same conversation. Immediately before the first edit, write, test, build, or other mutation, call create_worktree; if it reports multiple candidates, ask the user whether to create or choose an existing worktree.",
        path.display()
    )
}

fn new_git_workspace_ready_continuation(path: &Path) -> String {
    format!(
        "VMUX NEW PROJECT READY: Git project {} is the dedicated project root. Continue the original user request immediately in this directory. Do not call create_worktree for this project.",
        path.display()
    )
}

fn plain_workspace_ready_continuation(path: &Path) -> String {
    format!(
        "VMUX PROJECT SELECTION COMPLETED: Project {} is ready without Git. Continue the original user request in this same conversation. Do not call create_worktree unless Git is initialized later.",
        path.display()
    )
}

fn git_initialization_failed_continuation(path: &Path, error: &str) -> String {
    format!(
        "VMUX GIT INITIALIZATION FAILED: {error}. Project {} remains selected and usable without Git. Continue the original user request in this same conversation. Do not call create_worktree.",
        path.display()
    )
}

fn failed_workspace_continuation(message: &str) -> String {
    format!(
        "VMUX PROJECT SELECTION DID NOT COMPLETE: {message}. Do not retry automatically. Wait for the user to request project selection again."
    )
}

fn chat_agent_continuation_message(sid: &str, context: &str) -> ClientMessage {
    vmux_api::protocol::SharedMessage::AgentInput {
        sid: sid.to_string(),
        text: String::new(),
        context: Some(context.to_string()),
        attachments: Vec::new(),
        preferred_mode: None,
    }
    .into()
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AgentTabWorkspace<'w, 's> {
    pub(crate) tabs: Query<'w, 's, &'static mut Tab>,
    pub(crate) worktrees: Query<'w, 's, &'static TabWorktree>,
    pub(crate) workspaces: Query<'w, 's, &'static TabWorkspace>,
    pub(crate) pending_projects: Query<'w, 's, &'static PendingAgentProject>,
    pub(crate) managed_root: Option<Res<'w, ManagedWorktreeRoot>>,
}

pub(crate) fn activate_agent_worktree(
    tab_entity: Entity,
    agent_entity: Entity,
    project_dir: &Path,
    activation: TabWorktreeActivation,
    tabs: &mut Query<&mut Tab>,
    acp_sessions: &mut Query<&mut AcpSession>,
    child_of: &Query<&ChildOf>,
    commands: &mut Commands,
) -> Result<(PathBuf, Option<ClientMessage>), String> {
    let execution_dir = activation.execution_dir.clone();
    {
        let Ok(mut tab) = tabs.get_mut(tab_entity) else {
            return Err("tab not found".to_string());
        };
        bind_tab_workspace(&mut tab, project_dir, &execution_dir);
    }
    commands
        .entity(tab_entity)
        .insert((
            TabWorkspace {
                project_dir: project_dir.to_string_lossy().into_owned(),
            },
            activation.metadata,
            activation.ready,
            TabDirDecided,
        ))
        .remove::<PendingAgentProject>()
        .remove::<RepositoryNeedsWorktree>()
        .remove::<TabWorktreeUnavailable>();
    let rebind = ancestor_acp_stack(agent_entity, acp_sessions, child_of)
        .and_then(|stack| rebind_acp_workspace(stack, &execution_dir, acp_sessions, commands));
    Ok((execution_dir, rebind))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn activate_agent_directory(
    tab_entity: Entity,
    agent_entity: Entity,
    project_dir: &Path,
    execution_dir: &Path,
    tabs: &mut Query<&mut Tab>,
    acp_sessions: &mut Query<&mut AcpSession>,
    child_of: &Query<&ChildOf>,
    commands: &mut Commands,
) -> Result<Option<ClientMessage>, String> {
    {
        let Ok(mut tab) = tabs.get_mut(tab_entity) else {
            return Err("tab not found".to_string());
        };
        bind_tab_workspace(&mut tab, project_dir, execution_dir);
    }
    commands
        .entity(tab_entity)
        .insert((
            TabWorkspace {
                project_dir: project_dir.to_string_lossy().into_owned(),
            },
            TabDirDecided,
        ))
        .remove::<PendingAgentProject>()
        .remove::<RepositoryNeedsWorktree>()
        .remove::<TabWorktree>()
        .remove::<TabWorktreeReady>()
        .remove::<TabWorktreeUnavailable>();
    Ok(ancestor_acp_stack(agent_entity, acp_sessions, child_of)
        .and_then(|stack| rebind_acp_workspace(stack, execution_dir, acp_sessions, commands)))
}

#[allow(clippy::too_many_arguments)]
fn activate_selected_workspace(
    tab_entity: Entity,
    agent_entity: Entity,
    selected: &Path,
    tabs: &mut Query<&mut Tab>,
    acp_sessions: &mut Query<&mut AcpSession>,
    child_of: &Query<&ChildOf>,
    commands: &mut Commands,
) -> Result<(PathBuf, Option<ClientMessage>, SelectedWorkspaceKind), String> {
    let kind = if selected.join(".git").exists() {
        CheckoutInfo::try_from(selected)
            .map_err(|error| format!("selected project has invalid Git metadata: {}", error.0))?;
        SelectedWorkspaceKind::Git {
            needs_worktree: !is_linked_worktree(selected),
        }
    } else {
        SelectedWorkspaceKind::Plain
    };
    let rebind = activate_agent_directory(
        tab_entity,
        agent_entity,
        selected,
        selected,
        tabs,
        acp_sessions,
        child_of,
        commands,
    )?;
    if matches!(
        kind,
        SelectedWorkspaceKind::Git {
            needs_worktree: true
        }
    ) {
        commands.entity(tab_entity).insert(RepositoryNeedsWorktree);
    }
    Ok((selected.to_path_buf(), rebind, kind))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectedWorkspaceKind {
    Plain,
    Git { needs_worktree: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ExistingWorktreeCandidate {
    pub(super) checkout_dir: PathBuf,
    pub(super) execution_dir: PathBuf,
    pub(super) branch: String,
}

pub(super) fn existing_worktree_candidates(
    project_dir: &Path,
) -> Result<Vec<ExistingWorktreeCandidate>, String> {
    let project_dir = project_dir
        .canonicalize()
        .map_err(|error| format!("invalid project directory: {error}"))?;
    let project_checkout =
        CheckoutInfo::try_from(project_dir.as_path()).map_err(|error| error.0)?;
    let relative_dir = project_dir
        .strip_prefix(&project_checkout.root)
        .map_err(|_| "project directory is outside its checkout".to_string())?;
    let mut candidates = worktree_registrations(&project_checkout.root)
        .map_err(|error| error.0)?
        .into_iter()
        .filter_map(|registration| {
            let branch = registration.branch?;
            let checkout = CheckoutInfo::try_from(registration.path.as_path()).ok()?;
            if checkout.common_dir != project_checkout.common_dir
                || !is_linked_worktree(&checkout.root)
            {
                return None;
            }
            let execution_dir = checkout.root.join(relative_dir).canonicalize().ok()?;
            execution_dir.is_dir().then_some(ExistingWorktreeCandidate {
                checkout_dir: checkout.root,
                execution_dir,
                branch,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.execution_dir.cmp(&right.execution_dir));
    candidates.dedup_by(|left, right| left.execution_dir == right.execution_dir);
    Ok(candidates)
}

pub(super) fn resolve_requested_worktree(
    project_dir: &Path,
    requested: &Path,
) -> Result<ExistingWorktreeCandidate, String> {
    let requested = requested
        .canonicalize()
        .map_err(|error| format!("invalid worktree path: {error}"))?;
    existing_worktree_candidates(project_dir)?
        .into_iter()
        .find(|candidate| {
            requested == candidate.execution_dir
                || requested.starts_with(&candidate.execution_dir)
                || requested == candidate.checkout_dir
                || requested.starts_with(&candidate.checkout_dir)
        })
        .ok_or_else(|| {
            format!(
                "{} is not an existing linked worktree for this repository",
                requested.display()
            )
        })
}

pub(super) fn ambiguous_worktree_message(candidates: &[ExistingWorktreeCandidate]) -> String {
    let existing = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            format!(
                "{}. {} — {}",
                index + 2,
                candidate.branch,
                candidate.execution_dir.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Multiple existing worktrees match this repository. Ask the user with request_user_choice using these options, then call create_worktree again with create=true or the selected path:\n1. Create new worktree\n{existing}"
    )
}

fn drain_workspace_picker_tasks(
    mut pickers: Query<(Entity, &mut PendingWorkspacePicker)>,
    chat_views: Query<(), With<ChatView>>,
    mut tabs: Query<&mut Tab>,
    mut acp_sessions: Query<&mut AcpSession>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_none() {
        return;
    }
    for (picker_entity, mut picker) in &mut pickers {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        let continuation = match selected {
            None => Some(failed_workspace_continuation(
                "The user cancelled project selection",
            )),
            Some(selected) => match selected.canonicalize() {
                Ok(selected) if selected.is_dir() => {
                    if tabs.get(picker.tab_entity).is_err() {
                        Some(failed_workspace_continuation(
                            "The project tab no longer exists",
                        ))
                    } else {
                        match activate_selected_workspace(
                            picker.tab_entity,
                            picker.agent_entity,
                            &selected,
                            &mut tabs,
                            &mut acp_sessions,
                            &child_of,
                            &mut commands,
                        ) {
                            Ok((execution_dir, rebind, kind)) => {
                                if let Some(message) = rebind {
                                    service_requests.write(ServiceRequest(message));
                                }
                                match kind {
                                    SelectedWorkspaceKind::Git { .. } => {
                                        Some(git_workspace_ready_continuation(&execution_dir))
                                    }
                                    SelectedWorkspaceKind::Plain
                                        if chat_views.contains(picker.agent_entity) =>
                                    {
                                        commands
                                            .entity(picker.agent_entity)
                                            .insert((
                                                PendingAgentChoice {
                                                    session_entity: picker.session_entity,
                                                    question: INITIALIZE_GIT_QUESTION.to_string(),
                                                    options: INITIALIZE_GIT_OPTIONS
                                                        .into_iter()
                                                        .map(str::to_string)
                                                        .collect(),
                                                },
                                                InitializeGitAgentChoice {
                                                    tab_entity: picker.tab_entity,
                                                    workspace: execution_dir,
                                                },
                                            ))
                                            .remove::<ChatSynced>();
                                        None
                                    }
                                    SelectedWorkspaceKind::Plain => Some(format!(
                                        "VMUX PROJECT SELECTION COMPLETED: Project {} is ready without Git. Ask the user: \"Initialize Git repository?\" If yes, initialize Git in this exact project and continue directly in the project root without calling create_worktree. If no, continue the original request without a worktree.",
                                        execution_dir.display()
                                    )),
                                }
                            }
                            Err(error) => Some(failed_workspace_continuation(&format!(
                                "The selected project could not be prepared: {error}"
                            ))),
                        }
                    }
                }
                Ok(_) => Some(failed_workspace_continuation(
                    "The selected project is not a directory",
                )),
                Err(error) => Some(failed_workspace_continuation(&format!(
                    "The selected project directory is invalid: {error}"
                ))),
            },
        };
        if let Some(continuation) = continuation {
            commands
                .entity(picker.session_entity)
                .insert(PendingAgentContinuation(continuation));
        }
        commands.entity(picker_entity).despawn();
    }
}

fn send_pending_agent_continuations(
    mut sessions: Query<(
        Entity,
        &PendingAgentContinuation,
        Option<&AcpSession>,
        Option<&PageAgentSession>,
        Option<&AgentSession>,
        Option<&mut AgentRunState>,
    )>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, continuation, acp, page, cli, state) in &mut sessions {
        if cli.is_some() {
            commands
                .entity(entity)
                .insert(BufferedAgentPrompt {
                    text: continuation.0.clone(),
                    submit: true,
                })
                .remove::<PendingAgentContinuation>();
            continue;
        }
        if connected.is_none() {
            continue;
        }
        let sid = acp
            .map(|session| session.sid.as_str())
            .or_else(|| page.map(|session| session.sid.as_str()));
        let (Some(sid), Some(mut state)) = (sid, state) else {
            continue;
        };
        if !matches!(*state, AgentRunState::Idle | AgentRunState::Errored(_)) {
            continue;
        }
        service_requests.write(ServiceRequest(chat_agent_continuation_message(
            sid,
            &continuation.0,
        )));
        *state = AgentRunState::Streaming;
        commands.entity(entity).remove::<PendingAgentContinuation>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::run_terminal::AgentCwd;
    use crate::host::test_support::init_worktree_test_repo;
    use vmux_api::protocol::SharedMessage;
    use vmux_core::ProcessId;

    #[test]
    fn workspace_selection_continuations_resume_original_request() {
        let ready = git_workspace_ready_continuation(Path::new("/repo/dashboard"));
        let plain = plain_workspace_ready_continuation(Path::new("/tmp/demo"));
        let cancelled = failed_workspace_continuation("The user cancelled project selection");

        assert!(ready.contains("same conversation"));
        assert!(ready.contains("Git project /repo/dashboard is ready"));
        assert!(ready.contains("Immediately before the first edit"));
        assert!(ready.contains("create_worktree"));
        assert!(plain.contains("Project /tmp/demo is ready without Git"));
        assert!(plain.contains("Do not call create_worktree"));
        assert!(cancelled.contains("Do not retry automatically"));
    }

    #[test]
    fn initialize_git_choice_uses_new_project_root_directly() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().canonicalize().unwrap();
        let mut app = App::new();
        app.add_message::<AgentContinuationRequest>()
            .add_observer(initialize_git_agent_choice)
            .add_systems(Update, queue_agent_continuations);
        let session = app.world_mut().spawn_empty().id();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Project".into(),
                startup_dir: Some(workspace_path.to_string_lossy().into_owned()),
            })
            .id();
        let webview = app
            .world_mut()
            .spawn((
                PendingAgentChoice {
                    session_entity: session,
                    question: INITIALIZE_GIT_QUESTION.into(),
                    options: INITIALIZE_GIT_OPTIONS
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                },
                InitializeGitAgentChoice {
                    tab_entity: tab,
                    workspace: workspace_path.clone(),
                },
            ))
            .id();

        app.world_mut().trigger(UiInput {
            webview,
            payload: ChatChoiceSelected { index: 0 },
        });
        app.update();

        assert!(workspace_path.join(".git").is_dir());
        assert!(app.world().get::<RepositoryNeedsWorktree>(tab).is_none());
        assert!(
            app.world()
                .get::<PendingAgentContinuation>(session)
                .unwrap()
                .0
                .contains("Do not call create_worktree")
        );
    }

    #[test]
    fn cli_workspace_continuation_queues_terminal_prompt_without_service_wait() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_systems(Update, send_pending_agent_continuations);
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Codex,
                },
                PendingAgentContinuation("continue original request".to_string()),
            ))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<PendingAgentContinuation>(entity)
                .is_none()
        );
        assert_eq!(
            app.world().get::<BufferedAgentPrompt>(entity).unwrap(),
            &BufferedAgentPrompt {
                text: "continue original request".to_string(),
                submit: true,
            }
        );
    }

    #[test]
    fn chat_workspace_continuation_is_private_same_session_input() {
        assert!(matches!(
            chat_agent_continuation_message("sid-1", "continue original request"),
            ClientMessage::Shared(SharedMessage::AgentInput {
                sid,
                text,
                context,
                ..
            })
                if sid == "sid-1"
                    && text.is_empty()
                    && context.as_deref() == Some("continue original request")
        ));
    }

    #[test]
    fn worktree_activation_rebinds_existing_acp_session_without_replacing_view() {
        use bevy::ecs::system::RunSystemOnce;

        let repo = init_worktree_test_repo();
        let project_dir = repo.path().canonicalize().unwrap();
        let managed_root = tempfile::tempdir().unwrap();
        let activation = vmux_layout::worktree::create_worktree_for_branch_blocking(
            &project_dir,
            "feature/fun-terminal",
            managed_root.path(),
        )
        .unwrap();
        let execution_dir = activation.execution_dir.clone();
        let anchor = ProcessId::new();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "Tab 1".into(),
                    startup_dir: None,
                },
                PendingAgentProject(project_dir.clone()),
            ))
            .id();
        let pane = app.world_mut().spawn(ChildOf(tab)).id();
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "routing-session".into(),
                    cwd: AgentCwd::projects().unwrap(),
                    anchor,
                    resume: None,
                },
                AgentWorkingDir(AgentCwd::projects().unwrap().to_string_lossy().into_owned()),
                ChildOf(pane),
            ))
            .id();
        let view = app
            .world_mut()
            .spawn((ChatView, anchor, ChildOf(stack)))
            .id();

        let project_for_system = project_dir.clone();
        let rebind = app
            .world_mut()
            .run_system_once(
                move |mut tabs: Query<&mut Tab>,
                      mut sessions: Query<&mut AcpSession>,
                      child_of: Query<&ChildOf>,
                      mut commands: Commands| {
                    activate_agent_worktree(
                        tab,
                        view,
                        &project_for_system,
                        activation.clone(),
                        &mut tabs,
                        &mut sessions,
                        &child_of,
                        &mut commands,
                    )
                },
            )
            .unwrap()
            .unwrap()
            .1
            .unwrap();

        let tab_state = app.world().get::<Tab>(tab).unwrap();
        assert_eq!(
            tab_state.startup_dir.as_deref(),
            Some(execution_dir.to_string_lossy().as_ref())
        );
        assert_eq!(
            app.world().get::<TabWorkspace>(tab).unwrap().project_dir,
            project_dir.to_string_lossy()
        );
        assert_eq!(
            app.world().get::<TabWorktree>(tab).unwrap().branch,
            "feature/fun-terminal"
        );
        assert!(app.world().get::<TabWorktreeReady>(tab).is_some());
        assert!(app.world().get::<PendingAgentProject>(tab).is_none());
        let session = app.world().get::<AcpSession>(stack).unwrap();
        assert_eq!(session.sid, "routing-session");
        assert_eq!(session.anchor, anchor);
        assert_eq!(session.cwd, execution_dir);
        assert_eq!(
            app.world().get::<AgentWorkingDir>(stack).unwrap().0,
            execution_dir.to_string_lossy()
        );
        assert_eq!(app.world().get::<ChildOf>(view).unwrap().parent(), stack);
        assert!(app.world().get::<ChatView>(view).is_some());
        assert!(matches!(
            rebind,
            ClientMessage::RebindAcpWorkspace { sid, cwd }
                if sid == "routing-session" && cwd == execution_dir.to_string_lossy()
        ));
    }

    #[test]
    fn selected_workspace_binds_repository_without_eager_worktree_creation() {
        use bevy::ecs::system::RunSystemOnce;

        let repo = init_worktree_test_repo();
        let project_dir = repo.path().canonicalize().unwrap();
        let external_root = tempfile::tempdir().unwrap();
        let external = external_root.path().join("existing");
        worktree_add(&project_dir, &external, "feature/existing", "main").unwrap();
        let external = external.canonicalize().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let linked_tab = app
            .world_mut()
            .spawn(Tab {
                name: "Existing".into(),
                startup_dir: None,
            })
            .id();
        let linked_agent = app.world_mut().spawn(ChildOf(linked_tab)).id();
        let external_for_system = external.clone();
        let linked_execution = app
            .world_mut()
            .run_system_once(
                move |mut tabs: Query<&mut Tab>,
                      mut sessions: Query<&mut AcpSession>,
                      child_of: Query<&ChildOf>,
                      mut commands: Commands| {
                    activate_selected_workspace(
                        linked_tab,
                        linked_agent,
                        &external_for_system,
                        &mut tabs,
                        &mut sessions,
                        &child_of,
                        &mut commands,
                    )
                },
            )
            .unwrap()
            .unwrap()
            .0;

        assert_eq!(linked_execution, external);
        assert!(app.world().get::<TabWorktree>(linked_tab).is_none());
        assert!(
            app.world()
                .get::<RepositoryNeedsWorktree>(linked_tab)
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<TabWorkspace>(linked_tab)
                .unwrap()
                .project_dir,
            external.to_string_lossy()
        );
        assert_eq!(worktree_list(&project_dir).unwrap().len(), 2);

        let managed_tab = app
            .world_mut()
            .spawn(Tab {
                name: "Managed".into(),
                startup_dir: None,
            })
            .id();
        let managed_agent = app.world_mut().spawn(ChildOf(managed_tab)).id();
        let project_for_system = project_dir.clone();
        let managed_execution = app
            .world_mut()
            .run_system_once(
                move |mut tabs: Query<&mut Tab>,
                      mut sessions: Query<&mut AcpSession>,
                      child_of: Query<&ChildOf>,
                      mut commands: Commands| {
                    activate_selected_workspace(
                        managed_tab,
                        managed_agent,
                        &project_for_system,
                        &mut tabs,
                        &mut sessions,
                        &child_of,
                        &mut commands,
                    )
                },
            )
            .unwrap()
            .unwrap()
            .0;

        assert_eq!(managed_execution, project_dir);
        assert!(app.world().get::<TabWorktree>(managed_tab).is_none());
        assert!(
            app.world()
                .get::<RepositoryNeedsWorktree>(managed_tab)
                .is_some()
        );
        assert_eq!(
            app.world()
                .get::<TabWorkspace>(managed_tab)
                .unwrap()
                .project_dir,
            project_dir.to_string_lossy()
        );
        assert_eq!(worktree_list(&project_dir).unwrap().len(), 2);
    }

    #[test]
    fn selected_workspace_binds_non_git_directory_without_worktree() {
        use bevy::ecs::system::RunSystemOnce;

        let directory = tempfile::tempdir().unwrap();
        let selected = directory.path().canonicalize().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "Create".into(),
                startup_dir: None,
            })
            .id();
        let agent = app.world_mut().spawn(ChildOf(tab)).id();
        let selected_for_system = selected.clone();

        let (execution_dir, _, kind) = app
            .world_mut()
            .run_system_once(
                move |mut tabs: Query<&mut Tab>,
                      mut sessions: Query<&mut AcpSession>,
                      child_of: Query<&ChildOf>,
                      mut commands: Commands| {
                    activate_selected_workspace(
                        tab,
                        agent,
                        &selected_for_system,
                        &mut tabs,
                        &mut sessions,
                        &child_of,
                        &mut commands,
                    )
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(execution_dir, selected);
        assert_eq!(kind, SelectedWorkspaceKind::Plain);
        assert_eq!(
            app.world().get::<TabWorkspace>(tab).unwrap().project_dir,
            selected.to_string_lossy()
        );
        assert!(app.world().get::<RepositoryNeedsWorktree>(tab).is_none());
    }

    #[test]
    fn worktree_candidates_resolve_known_path_and_offer_create_when_ambiguous() {
        let repo = init_worktree_test_repo();
        let project_dir = repo.path().canonicalize().unwrap();
        let roots = tempfile::tempdir().unwrap();
        let first = roots.path().join("first");
        let second = roots.path().join("second");
        worktree_add(&project_dir, &first, "feature/first", "main").unwrap();
        worktree_add(&project_dir, &second, "feature/second", "main").unwrap();

        let candidates = existing_worktree_candidates(&project_dir).unwrap();
        let resolved = resolve_requested_worktree(&project_dir, &first).unwrap();
        let message = ambiguous_worktree_message(&candidates);

        assert_eq!(candidates.len(), 2);
        assert_eq!(resolved.branch, "feature/first");
        assert!(message.contains("1. Create new worktree"));
        assert!(message.contains("feature/first"));
        assert!(message.contains("feature/second"));
        assert!(message.contains("create=true"));
    }
}
