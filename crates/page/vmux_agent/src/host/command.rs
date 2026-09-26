mod dispatch;
mod tool_call;

use bevy::prelude::*;
use vmux_command::WriteCommandRequests;
use vmux_terminal::ServiceMessageSet;

pub(crate) use dispatch::{FocusPaneRequest, ProcessStackSpawnRequest, RenameProfileRequest};

pub(crate) struct CommandPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CommandSet {
    History,
    ToolCalls,
    Commands,
}

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                CommandSet::History,
                CommandSet::ToolCalls,
                CommandSet::Commands,
            )
                .chain()
                .in_set(WriteCommandRequests)
                .after(ServiceMessageSet),
        )
        .add_plugins((dispatch::DispatchPlugin, tool_call::ToolCallPlugin));
    }
}
