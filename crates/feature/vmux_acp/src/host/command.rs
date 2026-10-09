use crate::host::event::CommandOrigin;
use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentNotify};
use vmux_command::WriteCommandRequests;
use vmux_ecs::agent::{AgentCommandResponse, AgentReply, AgentRequestInput};
use vmux_ecs::service::ServiceMessageSet;

mod operation;
mod tool_call;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CommandSet {
    ToolCalls,
    Commands,
}

pub(super) fn add(app: &mut App) {
    app.add_message::<AgentCommandResponse>()
        .configure_sets(
            Update,
            (CommandSet::ToolCalls, CommandSet::Commands)
                .chain()
                .in_set(WriteCommandRequests)
                .after(ServiceMessageSet),
        )
        .add_systems(Update, notify.in_set(CommandSet::Commands));
    operation::add(app);
    tool_call::add(app);
}

fn notify(
    mut requests: MessageReader<AgentRequestInput>,
    agents: Query<(Entity, &vmux_ecs::team::Agent, Option<&vmux_ecs::ProcessId>)>,
    user: Query<Entity, With<vmux_ecs::team::User>>,
    mut attention: MessageWriter<vmux_ecs::notify::AgentAttention>,
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
                attention.write(vmux_ecs::notify::AgentAttention {
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
