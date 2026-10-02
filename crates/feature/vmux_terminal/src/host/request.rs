use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_ecs::ProcessId;
use vmux_ecs::service::ServiceMessageSet;

use crate::host::input_queue::{InputQueuePlugin, QueueTerminalInput, TerminalProcessIndex};
use crate::host::plugin::TerminalStackSpawnRequest;
use crate::{ProcessExited, Terminal};

pub struct TerminalRequestPlugin;

impl Plugin for TerminalRequestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::contract::TerminalContractPlugin);
        if !app.is_plugin_added::<InputQueuePlugin>() {
            app.add_plugins(InputQueuePlugin);
        }
        app.add_systems(Update, (send, shell).after(ServiceMessageSet));
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

impl RunShellRequest {
    fn input(&self) -> Vec<u8> {
        let mut data = self.command.as_bytes().to_vec();
        data.push(b'\r');
        data
    }
}

fn send(
    mut reader: MessageReader<TerminalSendRequest>,
    focus: vmux_layout::stack::FocusedStack,
    process_index: Single<&TerminalProcessIndex>,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut terminal_inputs: MessageWriter<QueueTerminalInput>,
) {
    for request in reader.read() {
        let TerminalSendRequest { text, terminal } = request.clone();
        let mut target_entity = None;
        if let Some(target) = terminal.as_deref() {
            if let Ok(process_id) = target.parse::<ProcessId>()
                && let Some(entity) = process_index.get(&process_id)
                && terminals.contains(entity)
            {
                target_entity = Some(entity);
            } else if let Ok(bits) = target.parse::<u64>()
                && let Some(entity) = Entity::try_from_bits(bits)
                && terminals.contains(entity)
            {
                target_entity = Some(entity);
            }
        } else if let Some(stack) = focus.stack {
            for (entity, _, child_of) in &terminals {
                if child_of.get() == stack {
                    target_entity = Some(entity);
                    break;
                }
            }
        }
        let target = target_entity;
        let Some(terminal) = target else {
            continue;
        };
        terminal_inputs.write(QueueTerminalInput {
            terminal,
            data: text.into_bytes(),
        });
    }
}

fn shell(
    mut reader: MessageReader<RunShellRequest>,
    focus: vmux_layout::stack::FocusedStack,
    panes: Query<
        Entity,
        (
            With<vmux_layout::pane::Pane>,
            Without<vmux_layout::pane::PaneSplit>,
        ),
    >,
    terminals: Query<(Entity, &ProcessId, &ChildOf), (With<Terminal>, Without<ProcessExited>)>,
    mut terminal_inputs: MessageWriter<QueueTerminalInput>,
    mut terminal_stack_spawns: Option<MessageWriter<TerminalStackSpawnRequest>>,
) {
    for request in reader.read() {
        let input = request.input();
        let RunShellRequest { cwd, mode, .. } = request.clone();
        if matches!(mode, ShellMode::Active) {
            let mut active_terminal = None;
            if let Some(stack) = focus.stack {
                for (terminal, _, child_of) in &terminals {
                    if child_of.get() == stack {
                        active_terminal = Some(terminal);
                        break;
                    }
                }
            }
            if let Some(terminal) = active_terminal {
                terminal_inputs.write(QueueTerminalInput {
                    terminal,
                    data: input,
                });
                continue;
            }
        }
        let Some(terminal_stack_spawns) = terminal_stack_spawns.as_mut() else {
            continue;
        };
        let Some(pane) = focus.pane.filter(|pane| panes.contains(*pane)) else {
            continue;
        };
        let Ok(cwd) = vmux_space::WorkspaceCwd::try_from(cwd.as_str()) else {
            continue;
        };
        let cwd = cwd.into_path();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_input_appends_carriage_return() {
        let request = RunShellRequest {
            command: "echo hi".into(),
            cwd: String::new(),
            mode: ShellMode::NewTab,
        };

        assert_eq!(request.input(), b"echo hi\r".to_vec());
    }
}
