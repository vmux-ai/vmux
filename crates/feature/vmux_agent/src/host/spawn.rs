use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use std::sync::atomic::{AtomicU64, Ordering};
use vmux_api::protocol::{ClientMessage, ProcessId};
use vmux_command::WriteCommandRequests;
use vmux_core::KeyboardOwner;
use vmux_core::agent::{RestartAgentPty, SpawnAgentInStackRequest};
use vmux_core::service::{ServiceConnected, ServiceMessageSet, ServiceRequest};
use vmux_core::{PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled};
use vmux_layout::pane::ForcePaneClose;
use vmux_setting::AppSettings;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::{ProcessExited, TerminalGridSize, new_terminal_bundle_with_cwd};

use crate::session::CliSessionSources;
use crate::session::{AgentSession, AgentSessionExited, PendingAgentSession, SessionId};

use super::cli::AgentExecutables;
use super::launch::{AgentLaunchRequest, AgentRestartRequest, PreparedAgentLaunch};
use super::page_open::{
    attach_agent_spawn_error_to_stack, attach_cli_setup_to_stack, cli_initial_prompt,
};

pub(super) struct SpawnPlugin;

pub(super) struct SpawnRequestsPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct SpawnRequestSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PrepareAgentLaunchSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ApplyAgentLaunchSet;

impl Plugin for SpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_plugins(SpawnRequestsPlugin)
            .configure_sets(
                Update,
                (SpawnRequestSet, PrepareAgentLaunchSet, ApplyAgentLaunchSet).chain(),
            )
            .add_systems(
                Update,
                detect_exit
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet),
            )
            .add_systems(
                Update,
                (
                    handle_restart_agent_pty
                        .in_set(SpawnRequestSet)
                        .before(ServiceMessageSet),
                    drain_agent_restarts
                        .in_set(ApplyAgentLaunchSet)
                        .before(ServiceMessageSet),
                ),
            );
    }
}

impl Plugin for SpawnRequestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnAgentInStackRequest>()
            .add_systems(Update, handle_spawn_agent_requests.in_set(SpawnRequestSet))
            .add_systems(Update, drain_agent_launches.in_set(ApplyAgentLaunchSet));
    }
}

#[allow(clippy::type_complexity)]
fn detect_exit(
    mut commands: Commands,
    mut writer: MessageWriter<AgentSessionExited>,
    mut q: Query<
        (Entity, Option<&vmux_terminal::pid::Pid>, &mut PageMetadata),
        (
            With<AgentSession>,
            With<ProcessExited>,
            Without<PendingAgentRestart>,
        ),
    >,
    child_of: Query<&ChildOf>,
) {
    use bevy::ecs::relationship::Relationship;
    for (entity, pid, mut meta) in &mut q {
        commands
            .entity(entity)
            .remove::<AgentSession>()
            .remove::<SessionId>()
            .remove::<PendingAgentSession>()
            .remove::<vmux_core::team::Agent>()
            .remove::<vmux_core::team::Profile>();
        let pane = child_of
            .get(entity)
            .ok()
            .map(Relationship::get)
            .and_then(|stack| child_of.get(stack).ok())
            .map(Relationship::get);
        match pane {
            Some(pane) => {
                commands.entity(pane).insert(ForcePaneClose);
            }
            None => {
                let next = match pid {
                    Some(vmux_terminal::pid::Pid(p)) => {
                        format!("{}{p}", vmux_terminal::event::TERMINAL_PAGE_URL)
                    }
                    None => vmux_terminal::event::TERMINAL_PAGE_URL.to_string(),
                };
                if meta.url != next {
                    meta.url = next;
                }
            }
        }
        writer.write(AgentSessionExited { entity });
    }
}

pub(crate) type PendingPageOpen = (
    Without<PageOpenHandled>,
    Without<PageOpenDeferred>,
    Without<PageOpenError>,
);

#[derive(Component)]
struct PendingAgentLaunch {
    request: SpawnAgentInStackRequest,
    process_id: ProcessId,
    generation: AgentLaunchGeneration,
}

#[derive(Component)]
pub(super) struct AgentLaunchTask(pub(super) Task<Result<PreparedAgentLaunch, String>>);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct AgentLaunchGeneration(u64);

impl AgentLaunchGeneration {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl PendingAgentLaunch {
    fn is_current(&self, current: AgentLaunchGeneration, metadata: Option<&PageMetadata>) -> bool {
        if current != self.generation {
            return false;
        }
        let Some(metadata) = metadata else {
            return true;
        };
        matches!(
            crate::AgentUrl::parse(&metadata.url),
            Some(crate::AgentUrl::Cli { kind, .. }) if kind == self.request.kind
        )
    }
}

#[derive(Component)]
struct PendingAgentRestart;

#[derive(Component)]
pub(super) struct AgentRestartTask(pub(super) Task<Result<PreparedAgentLaunch, String>>);

fn handle_spawn_agent_requests(
    mut reader: MessageReader<SpawnAgentInStackRequest>,
    settings: Res<AppSettings>,
    sources: CliSessionSources,
    models: Option<Single<&crate::host::model_selection::AgentModelSelections>>,
    executables: AgentExecutables,
    mut metadata: Query<&mut PageMetadata>,
    mut commands: Commands,
) {
    for req in reader.read() {
        let Some(source) = sources.get(req.kind) else {
            let message = "CLI session source not registered; cannot spawn agent";
            bevy::log::warn!("{message}");
            attach_agent_spawn_error_to_stack(req.stack, req.kind, message, &mut commands);
            continue;
        };
        let Some(exe_path) = executables.resolve(req.kind) else {
            attach_cli_setup_to_stack(req.kind, req.stack, &mut commands);
            continue;
        };
        let process_id = ProcessId::new();
        let effort_key = format!("cli:{}", req.kind.as_url_segment());
        let effort = settings.agent.effort_for(&effort_key).map(str::to_string);
        let shell =
            vmux_terminal::agent_run::AgentTerminalShell::configured(&settings).into_string();
        let model = models
            .as_deref()
            .map(|models| models.selected_for(&effort_key).to_string())
            .filter(|model| !model.is_empty());
        if let Ok(mut metadata) = metadata.get_mut(req.stack) {
            metadata.url = crate::AgentUrl::Cli {
                kind: req.kind,
                sid: req
                    .session_id
                    .clone()
                    .unwrap_or_else(|| crate::url::CLI_FRESH_SID.to_string()),
            }
            .format();
        }
        let generation = AgentLaunchGeneration::next();
        commands.entity(req.stack).insert(generation);
        let request = req.clone();
        commands.spawn((
            PendingAgentLaunch {
                request,
                process_id,
                generation,
            },
            AgentLaunchRequest {
                cwd: req.cwd.clone(),
                shell,
                session_id: req.session_id.clone(),
                executable: exe_path,
                anchor: process_id,
                effort,
                model,
                kind: source.kind,
            },
        ));
    }
}

fn drain_agent_launches(
    mut pending: Query<(Entity, &PendingAgentLaunch, &mut AgentLaunchTask)>,
    settings: Res<AppSettings>,
    entities: Query<()>,
    stacks: Query<(&AgentLaunchGeneration, Option<&PageMetadata>)>,
    mut spawn_requests: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
) {
    for (entity, pending, mut task) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).despawn();
        let request = &pending.request;
        if !entities.contains(request.stack) {
            continue;
        }
        let Ok((generation, metadata)) = stacks.get(request.stack) else {
            continue;
        };
        if !pending.is_current(*generation, metadata) {
            continue;
        }
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(error) => {
                bevy::log::warn!("agent spawn ({:?}) failed: {error}", request.kind);
                attach_agent_spawn_error_to_stack(
                    request.stack,
                    request.kind,
                    &error,
                    &mut commands,
                );
                commands
                    .entity(request.stack)
                    .remove::<AgentLaunchGeneration>();
                continue;
            }
        };
        let validation = vmux_core::profile::mcp_credentials::McpCredentialAccess::with_revision(
            prepared.mcp_revision,
            || {
                commands.entity(request.stack).despawn_children();
                let terminal = commands
                    .spawn((
                        new_terminal_bundle_with_cwd(&settings, Some(&request.cwd)),
                        ChildOf(request.stack),
                    ))
                    .id();
                commands.entity(terminal).insert(KeyboardOwner).insert((
                    prepared.launch,
                    AgentSession { kind: request.kind },
                    pending.process_id,
                    vmux_core::team::Profile::agent(request.kind),
                    vmux_core::team::Agent {
                        sid: request.session_id.clone().unwrap_or_default(),
                        kind: Some(request.kind),
                    },
                ));
                if let Some(id) = request.session_id.clone() {
                    commands.entity(terminal).insert(SessionId(id));
                } else {
                    commands.entity(terminal).insert(PendingAgentSession {
                        kind: request.kind,
                        spawn_time: std::time::SystemTime::now(),
                        cwd: request.cwd.clone(),
                    });
                }
                if let Some(prompt) = cli_initial_prompt(
                    request.kind,
                    request.initial_prompt.as_deref(),
                    &request.initial_attachments,
                ) {
                    commands
                        .entity(terminal)
                        .insert(vmux_terminal::PromptCapture {
                            draft: prompt,
                            skipped: false,
                        });
                }
                commands.entity(request.stack).remove::<(
                    vmux_core::PendingPrompt,
                    vmux_core::PendingPromptAttachments,
                    AgentLaunchGeneration,
                )>();
            },
        );
        match validation {
            Ok(Some(())) => {}
            Ok(None) => {
                spawn_requests.write(request.clone());
            }
            Err(error) => {
                bevy::log::warn!("agent spawn validation failed: {error}");
                attach_agent_spawn_error_to_stack(
                    request.stack,
                    request.kind,
                    &error,
                    &mut commands,
                );
                commands
                    .entity(request.stack)
                    .remove::<AgentLaunchGeneration>();
            }
        }
    }
}

fn handle_restart_agent_pty(
    mut reader: MessageReader<RestartAgentPty>,
    settings: Res<AppSettings>,
    q: Query<
        (Option<&TerminalLaunch>, &AgentSession, Option<&SessionId>),
        Without<PendingAgentRestart>,
    >,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
) {
    if connected.is_none() {
        for _ in reader.read() {}
        return;
    }
    for msg in reader.read() {
        let Ok((launch, session, session_id)) = q.get(msg.entity) else {
            continue;
        };
        let launch = launch.cloned();
        let kind = session.kind;
        let session_id = session_id.map(|session_id| session_id.0.clone());
        let new_id = ProcessId::new();
        let shell =
            vmux_terminal::agent_run::AgentTerminalShell::configured(&settings).into_string();
        let Some(launch) = launch else {
            bevy::log::warn!("agent launch configuration is unavailable");
            continue;
        };
        commands.entity(msg.entity).insert((
            PendingAgentRestart,
            AgentRestartRequest {
                launch,
                shell,
                session_id,
                anchor: new_id,
                kind,
            },
        ));
    }
}

fn drain_agent_restarts(
    mut q: Query<(
        Entity,
        &mut ProcessId,
        Option<&mut TerminalLaunch>,
        Option<&TerminalGridSize>,
        &AgentRestartRequest,
        &mut AgentRestartTask,
    )>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut restart_requests: MessageWriter<RestartAgentPty>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_none() {
        return;
    }
    for (entity, mut pid, mut launch, grid, request, mut task) in &mut q {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(error) => {
                bevy::log::warn!("agent restart preparation failed: {error}");
                commands
                    .entity(entity)
                    .remove::<(PendingAgentRestart, AgentRestartRequest, AgentRestartTask)>();
                continue;
            }
        };
        let new_id = request.anchor;
        let command = prepared.launch.command;
        let args = prepared.launch.args;
        let cwd = prepared.launch.cwd;
        let env = prepared.launch.env;
        let mcp_revision = prepared.mcp_revision;
        let (cols, rows) = grid.map(|grid| (grid.cols, grid.rows)).unwrap_or((80, 24));
        let validation = vmux_core::profile::mcp_credentials::McpCredentialAccess::with_revision(
            mcp_revision,
            || (),
        );
        match validation {
            Ok(Some(())) => {
                service_requests.write(ServiceRequest(ClientMessage::KillProcess {
                    process_id: *pid,
                }));
                service_requests.write(ServiceRequest(ClientMessage::CreateProcess {
                    process_id: new_id,
                    command,
                    args: args.clone(),
                    cwd,
                    env: env.clone(),
                    cols,
                    rows,
                }));
            }
            Ok(None) => {
                commands
                    .entity(entity)
                    .remove::<(PendingAgentRestart, AgentRestartRequest, AgentRestartTask)>();
                restart_requests.write(RestartAgentPty { entity });
                continue;
            }
            Err(error) => {
                bevy::log::warn!("agent restart validation failed: {error}");
                commands
                    .entity(entity)
                    .remove::<(PendingAgentRestart, AgentRestartRequest, AgentRestartTask)>();
                continue;
            }
        }
        *pid = new_id;
        if let Some(launch) = launch.as_mut() {
            launch.args = args;
            launch.env = env;
        }
        commands.trigger(vmux_terminal::TerminalRestartRequest { terminal: entity });
        commands.entity(entity).remove::<ProcessExited>().remove::<(
            PendingAgentRestart,
            AgentRestartRequest,
            AgentRestartTask,
        )>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::schedule::{IntoSystemSet, NodeId, Schedules, SystemSet};

    #[test]
    fn agent_restart_runs_before_terminal_service_messages() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SpawnPlugin));

        let mut schedules = app.world_mut().remove_resource::<Schedules>().unwrap();
        let mut update = schedules.remove(Update).unwrap();
        update.initialize(app.world_mut()).unwrap();
        let graph = update.graph();

        let service_messages = graph
            .system_sets
            .get_key(ServiceMessageSet.intern())
            .expect("the ordering names ServiceMessageSet");

        for (system, message) in [
            (
                handle_restart_agent_pty.into_system_set().intern(),
                "restart preparation must run before terminal input flush",
            ),
            (
                drain_agent_restarts.into_system_set().intern(),
                "restart process commands must run before terminal input flush",
            ),
        ] {
            let restart = graph
                .systems_in_set(system)
                .expect("restart system is registered")
                .first()
                .copied()
                .expect("restart system is registered");

            assert!(
                graph
                    .dependency()
                    .graph()
                    .contains_edge(NodeId::System(restart), NodeId::Set(service_messages)),
                "{message}"
            );
        }
    }

    #[test]
    fn restart_rebuilds_args_with_new_anchor() {
        let temp = std::env::temp_dir().join(format!("vmux-restart-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("Cargo.toml"), b"[workspace]\n").unwrap();
        let launch = TerminalLaunch {
            command: "/usr/local/bin/claude".into(),
            args: vec!["--mcp-config".into(), "OLD".into()],
            cwd: temp.to_string_lossy().to_string(),
            env: vec![],
            kind: vmux_core::terminal::TerminalKind::Claude,
        };
        let new_id = ProcessId::new();
        let prepared = crate::host::cli::prepare_restart_for_test::<
            crate::host::cli::claude::ClaudeLaunch,
        >(&AgentRestartRequest {
            launch,
            shell: "/bin/zsh".to_string(),
            session_id: None,
            anchor: new_id,
            kind: crate::AgentKind::Claude,
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&temp);
        let args = prepared.launch.args;
        let joined = args.join(" ");
        assert!(joined.contains("--anchor"), "args carry --anchor: {joined}");
        assert!(joined.contains(&new_id.to_string()), "anchor is the new id");
        assert!(
            !args
                .windows(2)
                .any(|pair| pair[0] == "--mcp-config" && pair[1] == "OLD"),
            "old args replaced"
        );
    }

    #[test]
    fn restart_removes_stale_managed_oauth_environment() {
        let temp = std::env::temp_dir().join(format!("vmux-restart-env-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("Cargo.toml"), b"[workspace]\n").unwrap();
        let launch = TerminalLaunch {
            command: "/usr/local/bin/codex".into(),
            args: Vec::new(),
            cwd: temp.to_string_lossy().to_string(),
            env: vec![("VMUX_MCP_OAUTH_6C696E656172".into(), "stale".into())],
            kind: vmux_core::terminal::TerminalKind::Codex,
        };

        let prepared = crate::host::cli::prepare_restart_for_test::<
            crate::host::cli::codex::CodexLaunch,
        >(&AgentRestartRequest {
            launch,
            shell: "/bin/zsh".to_string(),
            session_id: None,
            anchor: ProcessId::new(),
            kind: crate::AgentKind::Codex,
        })
        .unwrap();

        let _ = std::fs::remove_dir_all(&temp);
        let env = prepared.launch.env;
        assert!(
            !env.iter()
                .any(|(name, _)| name.starts_with("VMUX_MCP_OAUTH_"))
        );
    }

    #[test]
    fn restart_aborts_when_mcp_resolution_fails() {
        let launch = TerminalLaunch {
            command: "/usr/local/bin/codex".into(),
            args: vec!["stale".into()],
            cwd: "/vmux-test-missing-workspace".into(),
            env: vec![("VMUX_MCP_OAUTH_6C696E656172".into(), "stale".into())],
            kind: vmux_core::terminal::TerminalKind::Codex,
        };

        assert!(
            crate::host::cli::prepare_restart_for_test::<crate::host::cli::codex::CodexLaunch>(
                &AgentRestartRequest {
                    launch,
                    shell: "/bin/zsh".to_string(),
                    session_id: None,
                    anchor: ProcessId::new(),
                    kind: crate::AgentKind::Codex,
                },
            )
            .is_err()
        );
    }
}
