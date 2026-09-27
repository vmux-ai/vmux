mod application;
mod browser;
mod dispatch;
mod layout;
mod operation;
mod terminal;
mod tool_call;

use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentRequestId, ClientMessage};
use vmux_command::WriteCommandRequests;
use vmux_service::client::ServiceRequest;
use vmux_terminal::ServiceMessageSet;

pub(crate) use application::{FocusPaneRequest, RenameProfileRequest};
pub(crate) use terminal::ProcessStackSpawnRequest;

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
        app.configure_sets(
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
            layout::LayoutCommandPlugin,
            operation::AgentOperationPlugin,
            terminal::TerminalCommandPlugin,
            tool_call::ToolCallPlugin,
        ));
    }
}

#[derive(Clone, Copy)]
struct AgentReply {
    request_id: AgentRequestId,
}

impl AgentReply {
    fn new(request_id: AgentRequestId) -> Self {
        Self { request_id }
    }

    fn response(self, result: AgentCommandResult) -> ServiceRequest {
        ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: self.request_id,
            result,
        })
    }

    fn ok(self) -> ServiceRequest {
        self.response(AgentCommandResult::Ok)
    }
}
