mod operation;
mod tool_call;

use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentNotify};
use vmux_command::WriteCommandRequests;
use vmux_core::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};
use vmux_core::service::ServiceMessageSet;

use crate::host::event::CommandOrigin;

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
            .add_plugins((operation::AgentOperationPlugin, tool_call::ToolCallPlugin))
            .add_systems(Update, notify.in_set(CommandSet::Commands));
    }
}

fn notify(
    mut requests: MessageReader<AgentRequestInput>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_api::protocol::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(payload)) = request.decode::<AgentNotify>() else {
            continue;
        };
        let caller = match &request.origin {
            CommandOrigin::Agent {
                anchor: Some(pid), ..
            } => agents
                .iter()
                .find(|(_, _, process)| process.is_some_and(|process| process == pid))
                .map(|(entity, _, _)| entity),
            CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => agents
                .iter()
                .find(|(_, agent, _)| &agent.sid == sid)
                .map(|(entity, _, _)| entity),
            CommandOrigin::User => user.single().ok(),
            _ => None,
        };
        let result = match caller {
            Some(caller) => {
                attention.write(vmux_core::notify::AgentAttention {
                    entity: caller,
                    title: payload.title.clone(),
                    body: payload.body.clone(),
                });
                AgentCommandResult::Ok
            }
            None => AgentCommandResult::Error("notify: caller not found".to_string()),
        };
        responses.write(AgentReply::new(request.request_id).response(result));
    }
}
