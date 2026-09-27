use std::path::PathBuf;

use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommandResult, AgentNewTerminalTab, AgentRunShell, AgentTerminalSend,
};
use vmux_layout::{
    pane::{Pane, PaneSplit},
    stack::FocusedStack,
};
use vmux_service::client::ServiceRequest;
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;
use vmux_terminal::TerminalStackSpawnRequest;

use crate::host::valid_cwd;

use super::{AgentReply, CommandSet};

pub(super) struct TerminalCommandPlugin;

impl Plugin for TerminalCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentNewTerminalTabRequest>()
            .add_message::<AgentRunShellRequest>()
            .add_message::<AgentTerminalSendRequest>()
            .add_systems(
                Update,
                (open_terminal_tab, run_shell, send_to_terminal).in_set(CommandSet::Commands),
            );
    }
}

#[derive(Message, Clone)]
pub(crate) struct ProcessStackSpawnRequest {
    pub(crate) pane: Entity,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) activate: bool,
}

#[derive(Message, Clone)]
pub(super) struct AgentNewTerminalTabRequest {
    pub(super) reply: AgentReply,
    pub(super) activate: bool,
    pub(super) payload: AgentNewTerminalTab,
}

#[derive(Message, Clone)]
pub(super) struct AgentRunShellRequest {
    pub(super) reply: AgentReply,
    pub(super) payload: AgentRunShell,
}

#[derive(Message, Clone)]
pub(super) struct AgentTerminalSendRequest {
    pub(super) reply: AgentReply,
    pub(super) payload: AgentTerminalSend,
}

fn open_terminal_tab(
    mut requests: MessageReader<AgentNewTerminalTabRequest>,
    focus: Res<FocusedStack>,
    panes: Query<Entity, (With<Pane>, Without<PaneSplit>)>,
    active_space: Option<Res<ActiveSpace>>,
    settings: Res<AppSettings>,
    mut terminal_spawn: MessageWriter<TerminalStackSpawnRequest>,
    mut process_spawn: MessageWriter<ProcessStackSpawnRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = match focus.pane.filter(|pane| panes.contains(*pane)) {
            None => AgentCommandResult::Error("no active pane".to_string()),
            Some(pane) => match valid_cwd(&request.payload.cwd) {
                Err(message) => AgentCommandResult::Error(message),
                Ok(cwd) => {
                    let cwd = cwd.or_else(|| {
                        active_space
                            .as_ref()
                            .and_then(|space| settings.startup_dir(&space.record.id))
                    });
                    if request.payload.command.trim().is_empty() {
                        terminal_spawn.write(TerminalStackSpawnRequest {
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
    mut run: MessageWriter<vmux_terminal::RunShellRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let mode = match request.payload.mode {
            vmux_api::protocol::AgentShellMode::Active => vmux_terminal::ShellMode::Active,
            vmux_api::protocol::AgentShellMode::NewTab => vmux_terminal::ShellMode::NewTab,
        };
        run.write(vmux_terminal::RunShellRequest {
            command: request.payload.command.clone(),
            cwd: request.payload.cwd.clone(),
            mode,
        });
        responses.write(request.reply.ok());
    }
}

fn send_to_terminal(
    mut requests: MessageReader<AgentTerminalSendRequest>,
    mut send: MessageWriter<vmux_terminal::TerminalSendRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        send.write(vmux_terminal::TerminalSendRequest {
            text: request.payload.text.clone(),
            terminal: request.payload.terminal.clone(),
        });
        responses.write(request.reply.ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::AgentSessionPlugin;
    use crate::host::test_support::test_settings;
    use vmux_api::protocol::{AgentCommand, AgentRequestId, ProcessId};
    use vmux_terminal::Terminal;

    #[derive(Resource, Default)]
    struct CapturedTerminalSends(Vec<vmux_terminal::TerminalSendRequest>);

    impl CapturedTerminalSends {
        fn capture(
            mut requests: MessageReader<vmux_terminal::TerminalSendRequest>,
            mut captured: ResMut<Self>,
        ) {
            captured.0.extend(requests.read().cloned());
        }
    }

    #[test]
    fn terminal_send_writes_raw_text_to_active_terminal() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
            vmux_terminal::TerminalRequestPlugin,
        ))
        .add_message::<vmux_setting::SettingsWriteRequest>()
        .add_message::<vmux_space::SpaceCreateRequest>()
        .add_message::<vmux_space::SpaceRenameRequest>()
        .add_message::<vmux_space::SpaceDeleteRequest>()
        .add_message::<vmux_history::query::HistoryOpenIntent>()
        .init_resource::<CapturedTerminalSends>()
        .add_systems(
            Update,
            CapturedTerminalSends::capture.after(CommandSet::Commands),
        )
        .insert_resource(FocusedStack::default())
        .insert_resource(test_settings());

        let pane = app.world_mut().spawn(Pane).id();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::stack_bundle())
            .insert(ChildOf(pane))
            .id();
        let _terminal = app
            .world_mut()
            .spawn((Terminal, ProcessId::new()))
            .insert(ChildOf(stack))
            .id();

        app.world_mut().resource_mut::<FocusedStack>().pane = Some(pane);
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(stack);

        app.world_mut()
            .resource_mut::<Messages<crate::host::event::AgentCommandRequest>>()
            .write(crate::host::event::AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: crate::host::event::CommandOrigin::User,
                command: AgentCommand::TerminalSend(AgentTerminalSend {
                    text: "ls".to_string(),
                    terminal: None,
                }),
            });

        app.update();
        app.update();

        let captured = &app.world().resource::<CapturedTerminalSends>().0;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].text, "ls");
        assert_eq!(captured[0].terminal, None);
    }
}
