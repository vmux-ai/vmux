mod application;
mod browser;
mod dispatch;
mod operation;
mod tool_call;

use bevy::prelude::*;
use vmux_command::WriteCommandRequests;
use vmux_core::agent::{AgentCommandResponse, AgentReply};
use vmux_terminal::ServiceMessageSet;

pub(crate) use application::RenameProfileRequest;
pub(crate) struct CommandPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CommandSet {
    History,
    ToolCalls,
    Dispatch,
    Commands,
}

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentCommandResponse>()
            .configure_sets(
                Update,
                (
                    CommandSet::History,
                    CommandSet::ToolCalls,
                    CommandSet::Dispatch,
                    CommandSet::Commands,
                )
                    .chain()
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet),
            )
            .add_plugins((
                application::ApplicationCommandPlugin,
                browser::BrowserCommandPlugin,
                dispatch::DispatchPlugin,
                operation::AgentOperationPlugin,
                tool_call::ToolCallPlugin,
            ));
    }
}
