use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentInvokeCommand};
use vmux_core::agent::{
    AgentCommandResponse, AgentRequestAppExt, AgentRequestMessage, AgentRequestRouteSet,
    CommandOrigin,
};

use crate::{CommandDefinition, CommandInvocation};

pub(super) struct AgentCommandPlugin;

impl Plugin for AgentCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_request::<AgentInvokeCommand>()
            .add_systems(Update, invoke_command.after(AgentRequestRouteSet));
    }
}

fn invoke_command(
    mut requests: MessageReader<AgentRequestMessage<AgentInvokeCommand>>,
    definitions: Query<&CommandDefinition>,
    mut invocations: MessageWriter<CommandInvocation>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_core::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let args = match vmux_core::JsonArguments::try_from(&request.payload.args) {
            Ok(args) => args.0,
            Err(message) => {
                responses.write(request.reply.response(AgentCommandResult::Error(message)));
                continue;
            }
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
        }
        .unwrap_or(Entity::PLACEHOLDER);
        let Some(definition) = definitions
            .iter()
            .find(|definition| definition.matches(&request.payload.id))
        else {
            responses.write(request.reply.response(AgentCommandResult::Error(format!(
                "unknown app command: {}",
                request.payload.id
            ))));
            continue;
        };
        let result = if request.origin.is_agent() {
            definition.agent_invocation(caller, args)
        } else {
            definition.user_invocation(caller, args)
        };
        match result {
            Ok(invocation) => {
                invocations.write(invocation);
                responses.write(request.reply.ok());
            }
            Err(message) => {
                responses.write(request.reply.response(AgentCommandResult::Error(message)));
            }
        }
    }
}
