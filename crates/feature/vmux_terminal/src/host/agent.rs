use std::path::PathBuf;

use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommand, AgentCommandResult, AgentNewTerminalTab, AgentRunShell, AgentTerminalSend,
};
use vmux_core::agent::{AgentCommandRequest, AgentCommandResponse, AgentReply};
use vmux_core::{KeyboardOwner, LastActivatedAt, PageMetadata};
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::FocusedStack;
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;

pub(super) struct AgentTerminalPlugin;

impl Plugin for AgentTerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentCommandRequest>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentNewTerminalTabRequest>()
            .add_message::<AgentRunShellRequest>()
            .add_message::<AgentTerminalSendRequest>()
            .add_message::<ProcessStackSpawnRequest>()
            .add_systems(
                Update,
                (
                    route_terminal_commands,
                    (open_terminal_tab, run_shell, send_to_terminal),
                    respond_process_stack_spawn,
                )
                    .chain(),
            );
    }
}

#[derive(Message, Clone)]
struct ProcessStackSpawnRequest {
    pane: Entity,
    command: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: Vec<(String, String)>,
    activate: bool,
}

#[derive(Message, Clone)]
struct AgentNewTerminalTabRequest {
    reply: AgentReply,
    activate: bool,
    payload: AgentNewTerminalTab,
}

#[derive(Message, Clone)]
struct AgentRunShellRequest {
    reply: AgentReply,
    payload: AgentRunShell,
}

#[derive(Message, Clone)]
struct AgentTerminalSendRequest {
    reply: AgentReply,
    payload: AgentTerminalSend,
}

fn route_terminal_commands(
    mut commands: MessageReader<AgentCommandRequest>,
    mut new_terminal_tab: MessageWriter<AgentNewTerminalTabRequest>,
    mut run_shell: MessageWriter<AgentRunShellRequest>,
    mut terminal_send: MessageWriter<AgentTerminalSendRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            AgentCommand::NewTerminalTab(payload) => {
                new_terminal_tab.write(AgentNewTerminalTabRequest {
                    reply,
                    activate: !request.origin.is_agent(),
                    payload: payload.clone(),
                });
            }
            AgentCommand::RunShell(payload) => {
                run_shell.write(AgentRunShellRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            AgentCommand::TerminalSend(payload) => {
                terminal_send.write(AgentTerminalSendRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            _ => {}
        }
    }
}

fn open_terminal_tab(
    mut requests: MessageReader<AgentNewTerminalTabRequest>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_space: Option<Res<ActiveSpace>>,
    settings: Res<AppSettings>,
    mut terminal_spawn: MessageWriter<super::TerminalStackSpawnRequest>,
    mut process_spawn: MessageWriter<ProcessStackSpawnRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let result = match focus.pane.filter(|pane| panes.contains(*pane)) {
            None => AgentCommandResult::Error("no active pane".to_string()),
            Some(pane) => match vmux_space::cwd::valid_cwd(&request.payload.cwd) {
                Err(message) => AgentCommandResult::Error(message),
                Ok(cwd) => {
                    let cwd = cwd.or_else(|| {
                        active_space
                            .as_ref()
                            .and_then(|space| settings.startup_dir(&space.record.id))
                    });
                    if request.payload.command.trim().is_empty() {
                        terminal_spawn.write(super::TerminalStackSpawnRequest {
                            pane,
                            cwd,
                            shell: None,
                            agent_run: false,
                            pending_input: None,
                            process_id: None,
                            activate: request.activate,
                        });
                        AgentCommandResult::Ok
                    } else if let Some(cwd) = cwd {
                        process_spawn.write(ProcessStackSpawnRequest {
                            pane,
                            command: request.payload.command.clone(),
                            args: request.payload.args.clone(),
                            cwd,
                            env: request.payload.env.clone(),
                            activate: request.activate,
                        });
                        AgentCommandResult::Ok
                    } else {
                        AgentCommandResult::Error(
                            "project directory is required to run a command".to_string(),
                        )
                    }
                }
            },
        };
        responses.write(request.reply.response(result));
    }
}

fn run_shell(
    mut requests: MessageReader<AgentRunShellRequest>,
    mut run: MessageWriter<super::RunShellRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let mode = match request.payload.mode {
            vmux_api::protocol::AgentShellMode::Active => super::ShellMode::Active,
            vmux_api::protocol::AgentShellMode::NewTab => super::ShellMode::NewTab,
        };
        run.write(super::RunShellRequest {
            command: request.payload.command.clone(),
            cwd: request.payload.cwd.clone(),
            mode,
        });
        responses.write(request.reply.ok());
    }
}

fn send_to_terminal(
    mut requests: MessageReader<AgentTerminalSendRequest>,
    mut send: MessageWriter<super::TerminalSendRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        send.write(super::TerminalSendRequest {
            text: request.payload.text.clone(),
            terminal: request.payload.terminal.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn respond_process_stack_spawn(
    mut requests: MessageReader<ProcessStackSpawnRequest>,
    settings: Res<AppSettings>,
    mut commands: Commands,
) {
    for request in requests.read() {
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
            .and_then(|name| name.to_str())
            .unwrap_or(&request.command)
            .to_string();
        commands.entity(stack).insert(PageMetadata {
            url: crate::event::TERMINAL_PAGE_URL.to_string(),
            title,
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let launch = crate::launch::TerminalLaunch {
            command: request.command.clone(),
            args: request.args.clone(),
            cwd: request.cwd.to_string_lossy().to_string(),
            env: request.env.clone(),
            kind: crate::launch::TerminalKind::Plain,
        };
        let terminal = commands
            .spawn((
                super::new_terminal_bundle_with_cwd(&settings, Some(&request.cwd)),
                ChildOf(stack),
            ))
            .id();
        commands.entity(terminal).insert((launch, KeyboardOwner));
    }
}
