use crate::host::event::{
    AcpAgentInfo, AcpSessionConfigSelectionResult, AcpSessionConfigSnapshot, AcpSessionCreated,
    AcpTerminalCreated, AcpWorkspaceChanged, AgentApprovalRequest, AgentApprovalResolved,
    AgentDelta, AgentMessagesSnapshot, AgentRequestInput, AgentRunStatusChanged,
    AgentToolCallRequest, CommandOrigin,
};
use bevy::prelude::*;
use vmux_api::protocol::{AgentRunStatus, ClientMessage, JsonValue};
use vmux_api::room::{AssistantBlock, Message};
use vmux_ecs::agent::AgentCommandResponse;
use vmux_ecs::service::{
    ServiceConnected, ServiceMessageAppExt, ServiceMessageSet, ServiceRequest,
};
use vmux_session::{AcpSession, AgentMessageTimes, AgentMessages, AgentRunState, PromptQueue};

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
    args: JsonValue,
}

pub(super) fn add(app: &mut App) {
    app.add_service_message::<InboundAgentRequest>()
        .add_service_message::<AgentToolCallRequest>()
        .add_service_message::<AgentDelta>()
        .add_service_message::<AgentRunStatusChanged>()
        .add_service_message::<InboundAgentAwaitingApproval>()
        .add_service_message::<AgentApprovalResolved>()
        .add_service_message::<AgentMessagesSnapshot>()
        .add_service_message::<AcpAgentInfo>()
        .add_service_message::<AcpWorkspaceChanged>()
        .add_service_message::<AcpSessionConfigSnapshot>()
        .add_service_message::<AcpSessionConfigSelectionResult>()
        .add_service_message::<AcpSessionCreated>()
        .add_service_message::<AcpTerminalCreated>()
        .add_message::<ServiceRequest>()
        .add_message::<AgentCommandResponse>()
        .add_message::<AgentRequestInput>()
        .add_systems(
            Update,
            (
                subscribe_commands,
                (
                    route_requests,
                    route_approval_requests,
                    project_deltas,
                    project_snapshots,
                    project_statuses,
                    project_approval_resolutions,
                )
                    .chain()
                    .in_set(ServiceMessageSet),
            ),
        )
        .add_systems(Last, forward_command_responses);
}

fn route_messages(
    mut inbound: MessageReader<UiAgentMessagesSnapshot>,
    mut snapshots: MessageWriter<vmux_session::SnapshotReceived>,
) {
    for inbound in inbound.read() {
        snapshots.write(vmux_session::SnapshotReceived {
            session: SessionId(inbound.sid.clone()),
            messages: inbound.messages.clone(),
        });
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
    mut sessions: Query<(Entity, &AcpSession, &mut AgentRunState)>,
    mut commands: Commands,
) {
    for inbound in inbound.read() {
        let args = serde_json::Value::try_from(&inbound.args)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
        for (session, acp, mut state) in &mut sessions {
            if acp.sid != inbound.sid {
                continue;
            }
            *state = AgentRunState::AwaitingApproval {
                call_id: inbound.call_id.clone(),
                name: inbound.name.clone(),
                args: args.clone(),
            };
            commands.trigger(AgentApprovalRequest {
                session,
                call_id: inbound.call_id.clone(),
                name: inbound.name.clone(),
            });
        }
    }
}

fn project_deltas(
    mut inbound: MessageReader<AgentDelta>,
    mut sessions: Query<(&AcpSession, &mut AgentMessages, &mut AgentMessageTimes)>,
) {
    for inbound in inbound.read() {
        for (session, mut messages, mut times) in &mut sessions {
            if session.sid != inbound.sid {
                continue;
            }
            let before = messages.0.clone();
            match messages.0.last_mut() {
                Some(Message::Assistant { blocks }) => match blocks.last_mut() {
                    Some(AssistantBlock::Text(text)) => text.push_str(&inbound.text),
                    _ => blocks.push(AssistantBlock::Text(inbound.text.clone())),
                },
                _ => messages.0.push(Message::Assistant {
                    blocks: vec![AssistantBlock::Text(inbound.text.clone())],
                }),
            }
            times.reconcile(&before, &messages.0);
        }
    }
}

fn project_snapshots(
    mut inbound: MessageReader<AgentMessagesSnapshot>,
    mut sessions: Query<(&AcpSession, &mut AgentMessages, &mut AgentMessageTimes)>,
) {
    for inbound in inbound.read() {
        for (session, mut messages, mut times) in &mut sessions {
            if session.sid != inbound.sid {
                continue;
            }
            times.reconcile(&messages.0, &inbound.messages);
            messages.0.clone_from(&inbound.messages);
        }
    }
}

fn project_statuses(
    mut inbound: MessageReader<AgentRunStatusChanged>,
    mut sessions: Query<(&AcpSession, &mut AgentRunState, &mut PromptQueue)>,
) {
    for inbound in inbound.read() {
        for (session, mut state, mut queue) in &mut sessions {
            if session.sid != inbound.sid {
                continue;
            }
            match &inbound.status {
                AgentRunStatus::Idle => *state = AgentRunState::Idle,
                AgentRunStatus::Streaming => *state = AgentRunState::Streaming,
                AgentRunStatus::Interrupted => {
                    *state = AgentRunState::Idle;
                    if !queue.flush_pending() {
                        queue.paused = true;
                    }
                }
                AgentRunStatus::Errored(message) => {
                    if queue.flush_pending() {
                        *state = AgentRunState::Idle;
                    } else {
                        *state = AgentRunState::Errored(message.clone());
                    }
                }
            }
        }
    }
}

fn project_approval_resolutions(
    mut inbound: MessageReader<AgentApprovalResolved>,
    mut sessions: Query<(&AcpSession, &mut AgentRunState)>,
) {
    for inbound in inbound.read() {
        for (session, mut state) in &mut sessions {
            if session.sid == inbound.sid
                && matches!(
                    &*state,
                    AgentRunState::AwaitingApproval { call_id, .. } if call_id == &inbound.call_id
                )
            {
                *state = AgentRunState::Streaming;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::protocol::{AgentRequest, AgentRequestId, ServiceMessage};
    use vmux_ecs::service::ServiceInbound;
    use vmux_team::AgentRenameProfile;

    #[test]
    fn routes_agent_requests_without_terminal_ownership() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        add(&mut app);
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
