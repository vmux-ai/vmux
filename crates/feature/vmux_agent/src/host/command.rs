mod application;
mod operation;
mod tool_call;

use bevy::prelude::*;
use vmux_command::WriteCommandRequests;
use vmux_core::agent::AgentCommandResponse;
use vmux_terminal::ServiceMessageSet;

pub(crate) struct CommandPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CommandSet {
    ToolCalls,
    Commands,
}

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentCommandResponse>()
            .configure_sets(
                Update,
                (CommandSet::ToolCalls, CommandSet::Commands)
                    .chain()
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet),
            )
            .add_plugins((
                application::ApplicationCommandPlugin,
                operation::AgentOperationPlugin,
                tool_call::ToolCallPlugin,
            ));
    }
}
