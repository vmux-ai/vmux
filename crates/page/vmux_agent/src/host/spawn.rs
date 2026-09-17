use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use std::sync::atomic::{AtomicU64, Ordering};
use vmux_command::WriteAppCommands;
use vmux_core::KeyboardOwner;
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest, RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_core::{LastActivatedAt, PageMetadata, PageOpenDeferred, PageOpenError, PageOpenHandled};
use vmux_layout::event::TERMINAL_PAGE_URL;
use vmux_layout::pane::ForcePaneClose;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ClientMessage, ProcessId};
use vmux_setting::AppSettings;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::{
    ProcessExited, ServiceMessageSet, TerminalGridSize, new_terminal_bundle_with_cwd,
};

use crate::session::{AgentSession, AgentSessionExited, PendingAgentSession, SessionId};
use crate::strategy::AgentStrategies;

use super::attach::attach_page_agent_to_stack;
use super::command::ProcessStackSpawnRequest;
use super::page_open::{
    attach_agent_spawn_error_to_stack, attach_cli_setup_to_stack, clear_stack_children,
    cli_initial_prompt,
};
use super::provider::{AgentExecutableOverride, resolve_agent_executable};

pub(super) struct SpawnPlugin;

impl Plugin for SpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            detect_agent_session_process_exit
                .in_set(WriteAppCommands)
                .after(ServiceMessageSet)
                .after(super::query::handle_agent_queries),
        )
        .add_systems(
            Update,
            (
                (handle_spawn_agent_requests, drain_agent_launches).chain(),
                respond_process_stack_spawn.after(super::command::handle_agent_commands),
                (handle_restart_agent_pty, drain_agent_restarts)
                    .chain()
                    .before(ServiceMessageSet),
                respond_page_agent_attach,
                respond_page_agent_spawn_stack,
                respond_page_agent_spawn_default,
                respond_page_agent_attach_default,
            ),
        );
    }
}

fn respond_process_stack_spawn(
    mut reader: MessageReader<ProcessStackSpawnRequest>,
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let stack_ts = if request.activate {
            LastActivatedAt::now()
        } else {
            LastActivatedAt(0)
        };
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                stack_ts,
                ChildOf(request.pane),
            ))
            .id();
        let title = std::path::Path::new(&request.command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&request.command)
            .to_string();
        commands.entity(stack).insert(PageMetadata {
            url: TERMINAL_PAGE_URL.to_string(),
            title,
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let launch = vmux_terminal::launch::TerminalLaunch {
            command: request.command.clone(),
            args: request.args.clone(),
            cwd: request.cwd.to_string_lossy().to_string(),
            env: request.env.clone(),
            kind: vmux_terminal::launch::TerminalKind::Plain,
        };
        let term = commands
            .spawn((
                new_terminal_bundle_with_cwd(&settings, Some(&request.cwd)),
                ChildOf(stack),
            ))
            .id();
        commands.entity(term).insert((launch, KeyboardOwner));
    }
}

#[allow(clippy::type_complexity)]
fn detect_agent_session_process_exit(
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
    task: Task<Result<crate::launch::PreparedAgentLaunch, String>>,
}

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
struct PendingAgentRestart {
    task: Task<Result<RestartedAgentLaunch, String>>,
}

type RestartedAgentLaunch = (
    ProcessId,
    String,
    Vec<String>,
    String,
    Vec<(String, String)>,
    u64,
);

pub(super) fn handle_spawn_agent_requests(
    mut reader: MessageReader<SpawnAgentInStackRequest>,
    settings: Res<AppSettings>,
    strategies: Option<Res<AgentStrategies>>,
    models: Option<Res<crate::chat::model::AgentModelSelections>>,
    exec_override: Option<Res<AgentExecutableOverride>>,
    children_q: Query<&Children>,
    mut metadata: Query<&mut PageMetadata>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for req in reader.read() {
        let Some(strategies) = strategies.as_deref() else {
            let message = "agent strategies not registered; cannot spawn agent";
            bevy::log::warn!("{message}");
            attach_agent_spawn_error_to_stack(
                req.stack,
                req.kind,
                message,
                &children_q,
                &mut commands,
            );
            continue;
        };
        let Some(exe_path) = resolve_agent_executable(req.kind, exec_override.as_deref()) else {
            attach_cli_setup_to_stack(req.kind, req.stack, &children_q, &mut commands);
            continue;
        };
        let process_id = ProcessId::new();
        let effort_key = format!("cli:{}", req.kind.as_url_segment());
        let effort = settings.agent.effort_for(&effort_key).map(str::to_string);
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
        let task_request = request.clone();
        let strategies = strategies.clone();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = crate::build_agent_launch(
                task_request.kind,
                &task_request.cwd,
                task_request.session_id.as_deref(),
                &strategies,
                &exe_path,
                process_id,
                effort.as_deref(),
                model.as_deref(),
            );
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands.spawn(PendingAgentLaunch {
            request,
            process_id,
            generation,
            task,
        });
    }
}

fn drain_agent_launches(
    mut pending: Query<(Entity, &mut PendingAgentLaunch)>,
    settings: Res<AppSettings>,
    children_q: Query<&Children>,
    entities: Query<()>,
    stacks: Query<(&AgentLaunchGeneration, Option<&PageMetadata>)>,
    mut spawn_requests: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut pending {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
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
                    &children_q,
                    &mut commands,
                );
                commands
                    .entity(request.stack)
                    .remove::<AgentLaunchGeneration>();
                continue;
            }
        };
        let validation = vmux_core::profile::mcp_credentials::McpOauthCredentials::with_revision(
            prepared.mcp_revision,
            || {
                clear_stack_children(request.stack, &children_q, &mut commands);
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
                    &children_q,
                    &mut commands,
                );
                commands
                    .entity(request.stack)
                    .remove::<AgentLaunchGeneration>();
            }
        }
    }
}

fn respond_page_agent_attach(
    mut reader: MessageReader<PageAgentAttachRequest>,
    mut commands: Commands,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping page attach");
            continue;
        };
        let _ = attach_page_agent_to_stack(
            req.stack,
            &req.provider,
            &req.model,
            &req.sid,
            &mut commands,
            idx,
            &kind_q,
        );
    }
}

fn respond_page_agent_spawn_stack(
    mut reader: MessageReader<PageAgentSpawnStackRequest>,
    mut commands: Commands,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping page spawn");
            continue;
        };
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt::now(),
                ChildOf(req.pane),
            ))
            .id();
        let _ = attach_page_agent_to_stack(
            stack,
            &req.provider,
            &req.model,
            &req.sid,
            &mut commands,
            idx,
            &kind_q,
        );
    }
}

fn respond_page_agent_spawn_default(
    mut reader: MessageReader<PageAgentSpawnDefaultRequest>,
    mut commands: Commands,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping default page spawn");
            continue;
        };
        let Some(p) = crate::providers::resolve_default_app_provider() else {
            bevy::log::warn!(
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
            );
            continue;
        };
        let sid = uuid::Uuid::new_v4().to_string();
        let stack = commands
            .spawn((
                vmux_layout::stack::stack_bundle(),
                LastActivatedAt::now(),
                ChildOf(req.pane),
            ))
            .id();
        if attach_page_agent_to_stack(
            stack,
            p.provider,
            p.default_model,
            &sid,
            &mut commands,
            idx,
            &kind_q,
        )
        .is_none()
        {
            bevy::log::warn!(
                "page agent stack spawn failed: no strategy registered for {}/{}",
                p.provider,
                p.default_model
            );
        }
    }
}

fn respond_page_agent_attach_default(
    mut reader: MessageReader<PageAgentAttachDefaultRequest>,
    mut commands: Commands,
    idx: Option<Res<crate::client::page::strategy_index::PageStrategyIndex>>,
    kind_q: Query<&crate::client::page::strategy_components::StrategyKind>,
) {
    for req in reader.read() {
        let Some(idx) = idx.as_deref() else {
            bevy::log::warn!("page strategy index not registered; skipping default page attach");
            continue;
        };
        let Some(p) = crate::providers::resolve_default_app_provider() else {
            bevy::log::warn!(
                "no default Page agent provider available (set MISTRAL_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY)"
            );
            continue;
        };
        let sid = uuid::Uuid::new_v4().to_string();
        if attach_page_agent_to_stack(
            req.stack,
            p.provider,
            p.default_model,
            &sid,
            &mut commands,
            idx,
            &kind_q,
        )
        .is_none()
        {
            bevy::log::warn!(
                "attach_page_agent_to_stack returned None: no strategy registered for {}/{}",
                p.provider,
                p.default_model
            );
        }
    }
}

fn rebuilt_args_env_for_restart(
    launch: &TerminalLaunch,
    strategy: &dyn crate::client::cli::strategy::CliAgentStrategy,
    session_id: Option<&str>,
    new_id: ProcessId,
) -> Result<(Vec<String>, Vec<(String, String)>, u64), String> {
    for _ in 0..3 {
        let mcp_revision =
            vmux_core::profile::mcp_credentials::McpOauthCredentials::stable_revision()?;
        let mcp_cfg =
            crate::mcp::resolve(std::path::Path::new(&launch.cwd), new_id, strategy.kind())?;
        let args = strategy.build_args(&mcp_cfg, session_id);
        let fresh = strategy.build_env(&mcp_cfg);
        let fresh_keys: std::collections::HashSet<String> =
            fresh.iter().map(|(k, _)| k.clone()).collect();
        let mut env: Vec<(String, String)> = launch
            .env
            .iter()
            .filter(|(k, _)| {
                !fresh_keys.contains(k)
                    && !crate::managed_mcp::McpAuthorization::is_environment_variable(k)
            })
            .cloned()
            .collect();
        env.extend(fresh);
        if vmux_core::profile::mcp_credentials::McpOauthCredentials::revision() != mcp_revision {
            continue;
        }
        return Ok((args, env, mcp_revision));
    }
    Err("MCP configuration changed repeatedly while preparing the agent restart".to_string())
}

fn handle_restart_agent_pty(
    mut reader: MessageReader<RestartAgentPty>,
    q: Query<
        (Option<&TerminalLaunch>, &AgentSession, Option<&SessionId>),
        Without<PendingAgentRestart>,
    >,
    service: Option<Res<ServiceClient>>,
    strategies: Option<Res<AgentStrategies>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(_service) = service else {
        for _ in reader.read() {}
        return;
    };
    for msg in reader.read() {
        let Ok((launch, session, session_id)) = q.get(msg.entity) else {
            continue;
        };
        let launch = launch.cloned();
        let kind = session.kind;
        let session_id = session_id.map(|session_id| session_id.0.clone());
        let new_id = ProcessId::new();
        let strategies = strategies.as_deref().cloned();
        let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = match launch {
                Some(launch) => {
                    let Some(strategy) = strategies
                        .as_ref()
                        .and_then(|strategies| strategies.get_cli(kind))
                    else {
                        return Err(format!("CLI strategy not registered for {kind:?}"));
                    };
                    let (args, env, mcp_revision) = rebuilt_args_env_for_restart(
                        &launch,
                        strategy,
                        session_id.as_deref(),
                        new_id,
                    )?;
                    Ok((new_id, launch.command, args, launch.cwd, env, mcp_revision))
                }
                None => Err("agent launch configuration is unavailable".to_string()),
            };
            if let Some(wake) = wake {
                let _ = wake.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(msg.entity)
            .insert(PendingAgentRestart { task });
    }
}

fn drain_agent_restarts(
    mut q: Query<(
        Entity,
        &mut ProcessId,
        Option<&mut TerminalLaunch>,
        Option<&TerminalGridSize>,
        &mut PendingAgentRestart,
    )>,
    service: Option<Res<ServiceClient>>,
    mut restart_requests: MessageWriter<RestartAgentPty>,
    mut commands: Commands,
) {
    let Some(service) = service else { return };
    for (entity, mut pid, mut launch, grid, mut pending) in &mut q {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let (new_id, command, args, cwd, env, mcp_revision) = match result {
            Ok(launch) => launch,
            Err(error) => {
                bevy::log::warn!("agent restart preparation failed: {error}");
                commands.entity(entity).remove::<PendingAgentRestart>();
                continue;
            }
        };
        let (cols, rows) = grid.map(|grid| (grid.cols, grid.rows)).unwrap_or((80, 24));
        let validation = vmux_core::profile::mcp_credentials::McpOauthCredentials::with_revision(
            mcp_revision,
            || {
                service
                    .0
                    .send(ClientMessage::KillProcess { process_id: *pid });
                service.0.send(ClientMessage::CreateProcess {
                    process_id: new_id,
                    command,
                    args: args.clone(),
                    cwd,
                    env: env.clone(),
                    cols,
                    rows,
                });
            },
        );
        match validation {
            Ok(Some(())) => {}
            Ok(None) => {
                commands.entity(entity).remove::<PendingAgentRestart>();
                restart_requests.write(RestartAgentPty { entity });
                continue;
            }
            Err(error) => {
                bevy::log::warn!("agent restart validation failed: {error}");
                commands.entity(entity).remove::<PendingAgentRestart>();
                continue;
            }
        }
        *pid = new_id;
        if let Some(launch) = launch.as_mut() {
            launch.args = args;
            launch.env = env;
        }
        vmux_terminal::plugin::mark_terminal_restarting(&mut commands, entity);
        commands
            .entity(entity)
            .remove::<ProcessExited>()
            .remove::<PendingAgentRestart>();
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
    pub(crate) fn restart_rebuilds_args_with_new_anchor() {
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
        let (args, _env, _) = rebuilt_args_env_for_restart(
            &launch,
            &crate::client::cli::claude::ClaudeStrategy,
            None,
            new_id,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&temp);
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

        let (_, env, _) = rebuilt_args_env_for_restart(
            &launch,
            &crate::client::cli::codex::CodexStrategy,
            None,
            ProcessId::new(),
        )
        .unwrap();

        let _ = std::fs::remove_dir_all(&temp);
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
            rebuilt_args_env_for_restart(
                &launch,
                &crate::client::cli::codex::CodexStrategy,
                None,
                ProcessId::new(),
            )
            .is_err()
        );
    }
}
