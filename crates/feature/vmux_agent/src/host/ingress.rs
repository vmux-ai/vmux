use crate::host::event::{
    AgentApprovalRequest, AgentRequestInput, AgentToolCallRequest, CommandOrigin,
    UiAgentAcpTerminalCreated, UiAgentInfo, UiAgentSessionConfigSelectionResult,
    UiAgentSessionConfigState, UiAgentSessionCreated, UiAgentWorkspaceChanged,
};
use bevy::prelude::*;
use vmux_api::protocol::ClientMessage;
use vmux_ecs::agent::AgentCommandResponse;
use vmux_ecs::service::{
    ServiceConnected, ServiceMessageAppExt, ServiceMessageSet, ServiceRequest,
};
use vmux_session::AcpSession;

#[vmux_api::service_message(AgentRequest)]
struct InboundAgentRequest {
    request_id: vmux_api::protocol::AgentRequestId,
    anchor: Option<vmux_api::ProcessId>,
    request: vmux_api::protocol::AgentRequest,
}

#[vmux_api::service_message(SharedEvent::AgentAwaitingApproval)]
struct InboundAgentAwaitingApproval {
    sid: String,
    call_id: String,
    name: String,
}

pub(crate) struct AgentIngressPlugin;

impl Plugin for AgentIngressPlugin {
    fn build(&self, app: &mut App) {
        app.add_service_message::<InboundAgentRequest>()
            .add_service_message::<AgentToolCallRequest>()
            .add_service_message::<InboundAgentAwaitingApproval>()
            .add_service_message::<UiAgentInfo>()
            .add_service_message::<UiAgentWorkspaceChanged>()
            .add_service_message::<UiAgentSessionConfigState>()
            .add_service_message::<UiAgentSessionConfigSelectionResult>()
            .add_service_message::<UiAgentSessionCreated>()
            .add_service_message::<UiAgentAcpTerminalCreated>()
            .add_message::<ServiceRequest>()
            .add_message::<AgentCommandResponse>()
            .add_message::<AgentRequestInput>()
            .add_systems(
                Update,
                (
                    subscribe_commands,
                    (route_requests, route_approval_requests).in_set(ServiceMessageSet),
                ),
            )
            .add_systems(Last, forward_command_responses);
    }
}

fn subscribe_commands(
    connected: Query<(), Added<ServiceConnected>>,
    mut requests: MessageWriter<ServiceRequest>,
) {
    if connected.is_empty() {
        return;
    }
    requests.write(ServiceRequest(ClientMessage::SubscribeAgentCommands));
}

fn forward_command_responses(
    mut responses: MessageReader<AgentCommandResponse>,
    mut requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: response.request_id,
            result: response.result.clone(),
        }));
    }
}

fn route_requests(
    mut inbound: MessageReader<InboundAgentRequest>,
    mut requests: MessageWriter<AgentRequestInput>,
) {
    for inbound in inbound.read() {
        requests.write(AgentRequestInput {
            request_id: inbound.request_id,
            origin: CommandOrigin::Agent {
                sid: None,
                anchor: inbound.anchor,
            },
            request: inbound.request.clone(),
        });
    }
}

fn route_approval_requests(
    mut inbound: MessageReader<InboundAgentAwaitingApproval>,
    sessions: Query<(Entity, &AcpSession)>,
    mut commands: Commands,
) {
    for inbound in inbound.read() {
        let Some(session) = sessions
            .iter()
            .find_map(|(entity, session)| (session.sid == inbound.sid).then_some(entity))
        else {
            continue;
        };
        commands.trigger(AgentApprovalRequest {
            session,
            call_id: inbound.call_id.clone(),
            name: inbound.name.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::protocol::{AgentRequest, AgentRequestId, ServiceMessage};
    use vmux_ecs::service::ServiceInbound;
    use vmux_space::AgentRenameProfile;

    #[test]
    fn routes_agent_requests_without_terminal_ownership() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AgentIngressPlugin));
        let request_id = AgentRequestId([7; 16]);
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::AgentRequest {
                request_id,
                anchor: None,
                request: AgentRequest::encode(&AgentRenameProfile {
                    name: "Profile".into(),
                })
                .unwrap(),
            }));
        app.update();

        let commands = app
            .world_mut()
            .resource_mut::<Messages<AgentRequestInput>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].request_id, request_id);
    }
}
