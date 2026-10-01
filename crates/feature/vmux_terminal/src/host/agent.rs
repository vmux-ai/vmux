use std::path::PathBuf;

use bevy::prelude::*;
use vmux_api::protocol::AgentCommandResult;
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
};
use vmux_core::{KeyboardOwner, LastActivatedAt, PageMetadata};
use vmux_layout::pane::{Pane, PaneSplit};
use vmux_layout::stack::FocusedStack;
use vmux_setting::AppSettings;

#[vmux_api::contract(Copy, Eq)]
pub enum AgentShellMode {
    NewTab,
    Active,
}

#[vmux_api::agent]
pub struct AgentNewTerminalTab {
    pub cwd: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[vmux_api::agent]
pub struct AgentRunShell {
    pub command: String,
    pub cwd: String,
    pub mode: AgentShellMode,
}

#[vmux_api::agent]
pub struct AgentTerminalSend {
    pub text: String,
    pub terminal: Option<String>,
}

pub(super) struct AgentTerminalPlugin;

impl Plugin for AgentTerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(super::agent_run::AgentRunPlugin)
            .add_agent_request::<AgentNewTerminalTab>()
            .add_agent_request::<AgentRunShell>()
            .add_agent_request::<AgentTerminalSend>()
            .add_message::<ProcessStackSpawnRequest>()
            .add_systems(
                Update,
                (open_tab, run_shell, send_to)
                    .after(AgentRequestRouteSet)
                    .before(respond_process_stack_spawn),
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

fn open_tab(
    mut requests: MessageReader<AgentRequestMessage<AgentNewTerminalTab>>,
    focus: FocusedStack,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_space: vmux_layout::space::FocusedSpace,
    settings: Res<AppSettings>,
    mut terminal_spawn: MessageWriter<super::TerminalStackSpawnRequest>,
    mut process_spawn: MessageWriter<ProcessStackSpawnRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let activate = !request.origin.is_agent();
        let result = match focus.pane.filter(|pane| panes.contains(*pane)) {
            None => AgentCommandResult::Error("no active pane".to_string()),
            Some(pane) => match vmux_space::cwd::valid_cwd(&request.payload.cwd) {
                Err(message) => AgentCommandResult::Error(message),
                Ok(cwd) => {
                    let cwd = cwd.or_else(|| {
                        active_space
                            .id()
                            .and_then(|space_id| settings.startup_dir(space_id))
                    });
                    if request.payload.command.trim().is_empty() {
                        terminal_spawn.write(super::TerminalStackSpawnRequest {
                            pane,
                            cwd,
                            shell: None,
                            agent_run: false,
                            pending_input: None,
                            process_id: None,
                            activate,
                        });
                        AgentCommandResult::Ok
                    } else if let Some(cwd) = cwd {
                        process_spawn.write(ProcessStackSpawnRequest {
                            pane,
                            command: request.payload.command.clone(),
                            args: request.payload.args.clone(),
                            cwd,
                            env: request.payload.env.clone(),
                            activate,
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
    mut requests: MessageReader<AgentRequestMessage<AgentRunShell>>,
    mut run: MessageWriter<super::RunShellRequest>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let mode = match request.payload.mode {
            AgentShellMode::Active => super::ShellMode::Active,
            AgentShellMode::NewTab => super::ShellMode::NewTab,
        };
        run.write(super::RunShellRequest {
            command: request.payload.command.clone(),
            cwd: request.payload.cwd.clone(),
            mode,
        });
        responses.write(request.reply.ok());
    }
}

fn send_to(
    mut requests: MessageReader<AgentRequestMessage<AgentTerminalSend>>,
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
            url: crate::TerminalPlugin::URL.to_string(),
            title,
            bg_color: Some(vmux_layout::event::TERMINAL_CEF_BG_COLOR.to_string()),
            ..default()
        });
        let launch = crate::launch::TerminalLaunch {
            command: request.command.clone(),
            args: request.args.clone(),
            cwd: request.cwd.to_string_lossy().to_string(),
            env: request.env.clone(),
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
