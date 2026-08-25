use bevy::prelude::*;
use vmux_command::WriteAppCommands;
use vmux_core::KeyboardOwner;
use vmux_core::agent::{
    PageAgentAttachDefaultRequest, PageAgentAttachRequest, PageAgentSpawnDefaultRequest,
    PageAgentSpawnStackRequest, RestartAgentPty, SpawnAgentInStackRequest,
};
use vmux_core::{LastActivatedAt, PageMetadata, PageOpenError, PageOpenHandled};
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
                handle_spawn_agent_requests,
                respond_process_stack_spawn.after(super::command::handle_agent_commands),
                handle_restart_agent_pty.before(ServiceMessageSet),
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
pub fn detect_agent_session_process_exit(
    mut commands: Commands,
    mut writer: MessageWriter<AgentSessionExited>,
    mut q: Query<
        (Entity, Option<&vmux_terminal::pid::Pid>, &mut PageMetadata),
        (With<AgentSession>, With<ProcessExited>),
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

pub(crate) type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

pub(super) fn handle_spawn_agent_requests(
    mut reader: MessageReader<SpawnAgentInStackRequest>,
    settings: Res<AppSettings>,
    strategies: Option<Res<AgentStrategies>>,
    exec_override: Option<Res<AgentExecutableOverride>>,
    children_q: Query<&Children>,
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
        let effort = settings.agent.effort_for(&effort_key);
        match crate::build_agent_launch(
            req.kind,
            &req.cwd,
            req.session_id.as_deref(),
            strategies,
            &exe_path,
            process_id,
            effort,
        ) {
            Ok(launch) => {
                clear_stack_children(req.stack, &children_q, &mut commands);
                let terminal = commands
                    .spawn((
                        new_terminal_bundle_with_cwd(&settings, Some(&req.cwd)),
                        ChildOf(req.stack),
                    ))
                    .id();
                commands.entity(terminal).insert(KeyboardOwner).insert((
                    launch,
                    AgentSession { kind: req.kind },
                    process_id,
                    vmux_core::team::Profile::agent(req.kind),
                    vmux_core::team::Agent {
                        sid: req.session_id.clone().unwrap_or_default(),
                        kind: Some(req.kind),
                    },
                ));
                if let Some(id) = req.session_id.clone() {
                    commands.entity(terminal).insert(SessionId(id));
                } else {
                    commands.entity(terminal).insert(PendingAgentSession {
                        kind: req.kind,
                        spawn_time: std::time::SystemTime::now(),
                        cwd: req.cwd.clone(),
                    });
                }
                if let Some(prompt) = cli_initial_prompt(
                    req.kind,
                    req.initial_prompt.as_deref(),
                    &req.initial_attachments,
                ) {
                    commands
                        .entity(terminal)
                        .insert(vmux_terminal::PromptCapture {
                            draft: prompt,
                            skipped: false,
                        });
                }
                commands.entity(req.stack).remove::<(
                    vmux_core::PendingPrompt,
                    vmux_core::PendingPromptAttachments,
                )>();
            }
            Err(e) => {
                bevy::log::warn!("agent spawn ({:?}) failed: {e}", req.kind);
                attach_agent_spawn_error_to_stack(
                    req.stack,
                    req.kind,
                    &e,
                    &children_q,
                    &mut commands,
                );
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
) -> (Vec<String>, Vec<(String, String)>) {
    let Ok(mcp_cfg) =
        crate::mcp::resolve(std::path::Path::new(&launch.cwd), new_id, strategy.kind())
    else {
        return (launch.args.clone(), launch.env.clone());
    };
    let args = strategy.build_args(&mcp_cfg, session_id);
    let fresh = strategy.build_env(&mcp_cfg);
    let fresh_keys: std::collections::HashSet<String> =
        fresh.iter().map(|(k, _)| k.clone()).collect();
    let mut env: Vec<(String, String)> = launch
        .env
        .iter()
        .filter(|(k, _)| !fresh_keys.contains(k))
        .cloned()
        .collect();
    env.extend(fresh);
    (args, env)
}

fn handle_restart_agent_pty(
    mut reader: MessageReader<RestartAgentPty>,
    mut q: Query<(
        &mut ProcessId,
        Option<&mut TerminalLaunch>,
        &AgentSession,
        Option<&SessionId>,
        Option<&TerminalGridSize>,
    )>,
    service: Option<Res<ServiceClient>>,
    strategies: Option<Res<AgentStrategies>>,
    mut commands: Commands,
) {
    let Some(service) = service else {
        for _ in reader.read() {}
        return;
    };
    for msg in reader.read() {
        let Ok((mut pid, mut launch, session, session_id, grid)) = q.get_mut(msg.entity) else {
            continue;
        };
        service
            .0
            .send(ClientMessage::KillProcess { process_id: *pid });
        let new_id = ProcessId::new();

        let (command, args, cwd, env) = match launch.as_deref() {
            Some(l) => {
                let (rebuilt_args, rebuilt_env) =
                    match strategies.as_deref().and_then(|s| s.get_cli(session.kind)) {
                        Some(strategy) => rebuilt_args_env_for_restart(
                            l,
                            strategy,
                            session_id.map(|s| s.0.as_str()),
                            new_id,
                        ),
                        None => (l.args.clone(), l.env.clone()),
                    };
                (l.command.clone(), rebuilt_args, l.cwd.clone(), rebuilt_env)
            }
            None => (String::new(), vec![], String::new(), Vec::new()),
        };

        let (cols, rows) = grid.map(|g| (g.cols, g.rows)).unwrap_or((80, 24));
        service.0.send(ClientMessage::CreateProcess {
            process_id: new_id,
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env: env.clone(),
            cols,
            rows,
        });

        *pid = new_id;
        vmux_terminal::plugin::mark_terminal_restarting(&mut commands, msg.entity);
        if let Some(l) = launch.as_mut() {
            l.args = args;
            l.env = env;
        }
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

        let restart = graph
            .systems_in_set(handle_restart_agent_pty.into_system_set().intern())
            .expect("handle_restart_agent_pty is registered")
            .first()
            .copied()
            .expect("handle_restart_agent_pty is registered");
        let service_messages = graph
            .system_sets
            .get_key(ServiceMessageSet.intern())
            .expect("the ordering names ServiceMessageSet");

        assert!(
            graph
                .dependency()
                .graph()
                .contains_edge(NodeId::System(restart), NodeId::Set(service_messages)),
            "restart state commands must apply before terminal input flush"
        );
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
        let (args, _env) = rebuilt_args_env_for_restart(
            &launch,
            &crate::client::cli::claude::ClaudeStrategy,
            None,
            new_id,
        );
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
}
