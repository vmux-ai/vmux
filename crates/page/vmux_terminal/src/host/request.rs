use bevy::prelude::*;
use vmux_service::protocol::ProcessId;

use crate::host::plugin::{PendingTerminalInput, ServiceMessageSet, TerminalStackSpawnRequest};
use crate::host::process_index::TerminalProcessIndex;
use crate::{ProcessExited, Terminal};

pub struct TerminalRequestPlugin;

impl Plugin for TerminalRequestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::contract::TerminalContractPlugin,
            crate::host::process_index::TerminalProcessIndexPlugin,
        ))
        .add_systems(
            Update,
            (handle_terminal_send_requests, handle_run_shell_requests).after(ServiceMessageSet),
        );
    }
}

#[derive(Message, Clone)]
pub struct TerminalSendRequest {
    pub text: String,
    pub terminal: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellMode {
    NewTab,
    Active,
}

#[derive(Message, Clone)]
pub struct RunShellRequest {
    pub command: String,
    pub cwd: String,
    pub mode: ShellMode,
}

fn handle_terminal_send_requests(
    mut reader: MessageReader<TerminalSendRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    process_index: Res<TerminalProcessIndex>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let TerminalSendRequest { text, terminal } = request.clone();

        let target = if let Some(target) = terminal.as_deref() {
            crate::target::parse_terminal_target(target, &process_index, &terminals)
        } else {
            crate::target::active_terminal_for_tab(focus.stack, &terminals)
        };
        let Some(terminal) = target else {
            continue;
        };
        commands.entity(terminal).insert(PendingTerminalInput {
            data: text.into_bytes(),
        });
    }
}

fn handle_run_shell_requests(
    mut reader: MessageReader<RunShellRequest>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    panes: Query<
        Entity,
        (
            With<vmux_layout::pane::Pane>,
            Without<vmux_layout::pane::PaneSplit>,
        ),
    >,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut commands: Commands,
    mut terminal_stack_spawns: Option<MessageWriter<TerminalStackSpawnRequest>>,
) {
    for request in reader.read() {
        let RunShellRequest { command, cwd, mode } = request.clone();
        let input = crate::shell_input::shell_command_input(&command);
        if matches!(mode, ShellMode::Active)
            && let Some(terminal) = crate::target::active_terminal_for_tab(focus.stack, &terminals)
        {
            commands
                .entity(terminal)
                .insert(PendingTerminalInput { data: input });
            continue;
        }
        let Some(terminal_stack_spawns) = terminal_stack_spawns.as_mut() else {
            continue;
        };
        let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane)) else {
            continue;
        };
        let Ok(cwd) = vmux_space::cwd::valid_cwd(&cwd) else {
            continue;
        };
        terminal_stack_spawns.write(TerminalStackSpawnRequest {
            pane,
            cwd,
            shell: None,
            agent_run: false,
            pending_input: Some(input),
            process_id: None,
            activate: true,
        });
    }
}
