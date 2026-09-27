use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentNotify};
use vmux_core::agent::AgentCommandResponse;

use crate::host::event::CommandOrigin;

use super::{AgentReply, CommandSet};

pub(super) struct ApplicationCommandPlugin;

impl Plugin for ApplicationCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentNotifyRequest>()
            .add_systems(Update, notify.in_set(CommandSet::Commands));
    }
}

#[derive(Message, Clone)]
pub(super) struct AgentNotifyRequest {
    pub(super) reply: AgentReply,
    pub(super) origin: CommandOrigin,
    pub(super) payload: AgentNotify,
}

fn notify(
    mut requests: MessageReader<AgentNotifyRequest>,
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
                    title: request.payload.title.clone(),
                    body: request.payload.body.clone(),
                });
                AgentCommandResult::Ok
            }
            None => AgentCommandResult::Error("notify: caller not found".to_string()),
        };
        responses.write(request.reply.response(result));
    }
}
