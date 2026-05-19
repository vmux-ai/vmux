use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::agent::{
    AgentSession, McpServerConfig, PendingAgentSession, RestartAgentPty, SessionId,
    SpawnAgentInStackRequest,
};
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{ClientMessage, ProcessId};
use vmux_settings::AppSettings;
use vmux_terminal::launch::TerminalLaunch;
use vmux_terminal::new_terminal_bundle_with_cwd;

use crate::launch::build_agent_launch;
use crate::strategy::AgentStrategies;

pub struct TerminalIntegrationPlugin;

impl Plugin for TerminalIntegrationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_spawn_agent_requests, handle_restart_agent_pty),
        );
    }
}

fn handle_spawn_agent_requests(
    mut reader: MessageReader<SpawnAgentInStackRequest>,
    settings: Res<AppSettings>,
    strategies: Option<Res<AgentStrategies>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for req in reader.read() {
        let Some(strategies) = strategies.as_deref() else {
            bevy::log::warn!("agent strategies not registered; cannot spawn agent");
            continue;
        };
        match build_agent_launch(req.kind, &req.cwd, req.session_id.as_deref(), strategies) {
            Ok(launch) => {
                let terminal = commands
                    .spawn((
                        new_terminal_bundle_with_cwd(
                            &mut meshes,
                            &mut webview_mt,
                            &settings,
                            Some(&req.cwd),
                        ),
                        ChildOf(req.stack),
                    ))
                    .id();
                commands
                    .entity(terminal)
                    .insert(CefKeyboardTarget)
                    .insert((launch, AgentSession { kind: req.kind }));
                if let Some(id) = req.session_id.clone() {
                    commands.entity(terminal).insert(SessionId(id));
                } else {
                    commands.entity(terminal).insert(PendingAgentSession {
                        kind: req.kind,
                        spawn_time: std::time::SystemTime::now(),
                        cwd: req.cwd.clone(),
                    });
                }
            }
            Err(e) => {
                bevy::log::warn!("agent spawn ({:?}) failed: {e}", req.kind);
            }
        }
    }
}

fn handle_restart_agent_pty(
    mut reader: MessageReader<RestartAgentPty>,
    mut q: Query<(
        &mut ProcessId,
        Option<&mut TerminalLaunch>,
        &AgentSession,
        Option<&SessionId>,
    )>,
    service: Option<Res<ServiceClient>>,
    strategies: Option<Res<AgentStrategies>>,
) {
    let Some(service) = service else {
        for _ in reader.read() {}
        return;
    };
    for msg in reader.read() {
        let Ok((mut pid, mut launch, session, session_id)) = q.get_mut(msg.entity) else {
            continue;
        };
        service
            .0
            .send(ClientMessage::KillProcess { process_id: *pid });

        let (command, args, cwd, env) = match launch.as_deref() {
            Some(l) => {
                let mut updated_args = l.args.clone();
                if let Some(strats) = strategies.as_deref()
                    && let Some(strategy) = strats.get_cli(session.kind)
                {
                    let mcp = McpServerConfig {
                        command: l.command.clone(),
                        args: vec![],
                        cwd: None,
                    };
                    updated_args = strategy.build_args(&mcp, session_id.map(|s| s.0.as_str()));
                }
                (
                    l.command.clone(),
                    updated_args,
                    l.cwd.clone(),
                    l.env.clone(),
                )
            }
            None => (String::new(), vec![], String::new(), Vec::new()),
        };

        let new_id = ProcessId::new();
        service.0.send(ClientMessage::CreateProcess {
            process_id: new_id,
            command: command.clone(),
            args: args.clone(),
            cwd: cwd.clone(),
            env: env.clone(),
            cols: 80,
            rows: 24,
        });
        service
            .0
            .send(ClientMessage::AttachProcess { process_id: new_id });

        *pid = new_id;
        if let Some(l) = launch.as_mut() {
            l.args = args;
        }
    }
}
