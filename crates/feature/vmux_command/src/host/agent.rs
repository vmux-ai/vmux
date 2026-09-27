use bevy::prelude::*;
use vmux_api::protocol::{AgentCommandResult, AgentInvokeCommand};
use vmux_core::agent::{AgentCommandResponse, AgentRequestInput, CommandOrigin};

use crate::{CommandDefinition, CommandInvocation};

pub(super) struct AgentCommandPlugin;

impl Plugin for AgentCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentRequestInput>()
            .add_message::<AgentCommandResponse>()
            .add_systems(Update, invoke_command);
    }
}

fn invoke_command(
    mut requests: MessageReader<AgentRequestInput>,
    definitions: Query<&CommandDefinition>,
    mut invocations: MessageWriter<CommandInvocation>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_api::protocol::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut responses: MessageWriter<AgentCommandResponse>,
) {
    for request in requests.read() {
        let Ok(Some(payload)) = request.decode::<AgentInvokeCommand>() else {
            continue;
        };
        let args = match vmux_core::JsonArguments::try_from(&payload.args) {
            Ok(args) => args.0,
            Err(message) => {
                responses.write(AgentCommandResponse {
                    request_id: request.request_id,
                    result: AgentCommandResult::Error(message),
                });
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
            .find(|definition| definition.matches(&payload.id))
        else {
            responses.write(AgentCommandResponse {
                request_id: request.request_id,
                result: AgentCommandResult::Error(format!("unknown app command: {}", payload.id)),
            });
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
                responses.write(AgentCommandResponse {
                    request_id: request.request_id,
                    result: AgentCommandResult::Ok,
                });
            }
            Err(message) => {
                responses.write(AgentCommandResponse {
                    request_id: request.request_id,
                    result: AgentCommandResult::Error(message),
                });
            }
        }
    }
}
