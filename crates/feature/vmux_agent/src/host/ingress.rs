use crate::host::event::{
    AcpAgentInfo, AcpSessionConfigSelectionResult, AcpSessionConfigSnapshot, AcpSessionCreated,
    AcpTerminalCreated, AcpWorkspaceChanged, AgentApprovalRequest, AgentApprovalResolved,
    AgentDelta, AgentMessagesSnapshot, AgentRequestInput, AgentRunStatusChanged,
    AgentToolCallRequest, CommandOrigin,
};
use bevy::prelude::*;
use vmux_api::conversation::{AssistantBlock, Message};
use vmux_api::protocol::{AgentRunStatus, ClientMessage, JsonValue};
use vmux_ecs::agent::AgentCommandResponse;
use vmux_ecs::service::{
    ServiceConnected, ServiceMessageAppExt, ServiceMessageSet, ServiceRequest,
};
use vmux_session::{PromptQueue, RunState, Session, SessionId, SnapshotReceived, Transcripts};

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
        .add_message::<SnapshotReceived>()
        .add_systems(
            Update,
            (
                subscribe_commands,
                (
                    route_requests,
                    route_approval_requests,
                    project_deltas,
                    route_messages,
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
    mut inbound: MessageReader<AgentMessagesSnapshot>,
    mut snapshots: MessageWriter<SnapshotReceived>,
) {
    for inbound in inbound.read() {
        snapshots.write(SnapshotReceived {
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
    mut sessions: Query<(Entity, &SessionId, &mut RunState), With<Session>>,
    mut commands: Commands,
) {
    for inbound in inbound.read() {
        let args = serde_json::Value::try_from(&inbound.args)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
        for (session, session_id, mut state) in &mut sessions {
            if session_id.0 != inbound.sid {
                continue;
            }
            *state = RunState::AwaitingApproval {
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
    sessions: Query<(Entity, &SessionId), With<Session>>,
    transcripts: Transcripts,
    mut snapshots: MessageWriter<SnapshotReceived>,
) {
    let mut pending = std::collections::HashMap::<Entity, (SessionId, Vec<Message>)>::new();
    for inbound in inbound.read() {
        let Some((session, session_id)) = sessions
            .iter()
            .find(|(_, session_id)| session_id.0 == inbound.sid)
        else {
            continue;
        };
        let (_, messages) = pending
            .entry(session)
            .or_insert_with(|| (session_id.clone(), transcripts.get(session).messages));
        match messages.last_mut() {
            Some(Message::Assistant { blocks }) => match blocks.last_mut() {
                Some(AssistantBlock::Text(text)) => text.push_str(&inbound.text),
                _ => blocks.push(AssistantBlock::Text(inbound.text.clone())),
            },
            _ => messages.push(Message::Assistant {
                blocks: vec![AssistantBlock::Text(inbound.text.clone())],
            }),
        }
    }
    for (_, (session, messages)) in pending {
        snapshots.write(SnapshotReceived { session, messages });
    }
}

fn project_statuses(
    mut inbound: MessageReader<AgentRunStatusChanged>,
    mut sessions: Query<(&SessionId, &mut RunState, &mut PromptQueue), With<Session>>,
) {
    for inbound in inbound.read() {
        for (session_id, mut state, mut queue) in &mut sessions {
            if session_id.0 != inbound.sid {
                continue;
            }
            match &inbound.status {
                AgentRunStatus::Idle => *state = RunState::Idle,
                AgentRunStatus::Streaming => *state = RunState::Streaming,
                AgentRunStatus::Interrupted => {
                    *state = RunState::Idle;
                    if !queue.flush_pending() {
                        queue.paused = true;
                    }
                }
                AgentRunStatus::Errored(message) => {
                    if queue.flush_pending() {
                        *state = RunState::Idle;
                    } else {
                        *state = RunState::Errored(message.clone());
                    }
                }
            }
        }
    }
}

fn project_approval_resolutions(
    mut inbound: MessageReader<AgentApprovalResolved>,
    mut sessions: Query<(&SessionId, &mut RunState), With<Session>>,
) {
    for inbound in inbound.read() {
        for (session_id, mut state) in &mut sessions {
            if session_id.0 == inbound.sid
                && matches!(
                    &*state,
                    RunState::AwaitingApproval { call_id, .. } if call_id == &inbound.call_id
                )
            {
                *state = RunState::Streaming;
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
