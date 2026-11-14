use crate::host::event::{
    AcpAgentInfo, AcpSessionConfigSelectionResult, AcpSessionConfigSnapshot, AcpSessionCreated,
    AcpTerminalCreated, AcpWorkspaceChanged, AgentApprovalResolved, AgentDelta,
    AgentMessagesSnapshot, AgentRequestInput, AgentRunStatusChanged, AgentToolCallRequest,
    CommandOrigin,
};
use bevy::prelude::*;
use vmux_api::protocol::{AgentRunStatus, ClientMessage, JsonValue};
use vmux_api::session::{AssistantBlock, Message};
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

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
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
            .add_systems(Last, forward_command_responses)
            .add_service_message::<InboundAgentRequest>()
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
            .add_service_message::<AcpTerminalCreated>();
    }
}

fn route_messages(
    mut inbound: MessageReader<AgentMessagesSnapshot>,
    mut snapshots: MessageWriter<SnapshotReceived>,
    sessions: Query<(Entity, &SessionId), With<Session>>,
    transcripts: Transcripts,
    mut commands: Commands,
) {
    for inbound in inbound.read() {
        let Some((session, _)) = sessions
            .iter()
            .find(|(_, session_id)| session_id.0 == inbound.sid)
        else {
            continue;
        };
        commands
            .entity(session)
            .remove::<vmux_session::TranscriptLoading>();
        if inbound.messages.is_empty() && !transcripts.get(session).messages.is_empty() {
            continue;
        }
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
    mut sessions: Query<(&SessionId, &mut RunState), With<Session>>,
) {
    for inbound in inbound.read() {
        let args = serde_json::Value::try_from(&inbound.args)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
        for (session_id, mut state) in &mut sessions {
            if session_id.0 != inbound.sid {
                continue;
            }
            *state = RunState::AwaitingApproval {
                call_id: inbound.call_id.clone(),
                name: inbound.name.clone(),
                args: args.clone(),
            };
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
    mut sessions: Query<
        (Entity, &SessionId, &mut RunState, &mut PromptQueue),
        (
            With<Session>,
            Without<super::navigation_driver::RetryRequested>,
        ),
    >,
    mut commands: Commands,
) {
    for inbound in inbound.read() {
        for (session, session_id, mut state, mut queue) in &mut sessions {
            if session_id.0 != inbound.sid {
                continue;
            }
            match &inbound.status {
                AgentRunStatus::Starting => *state = RunState::Preparing,
                AgentRunStatus::Idle => {
                    *state = RunState::Idle;
                    commands
                        .entity(session)
                        .remove::<vmux_session::SetupPending>();
                }
                AgentRunStatus::Streaming => {
                    *state = RunState::Streaming;
                    commands
                        .entity(session)
                        .remove::<vmux_session::SetupPending>();
                }
                AgentRunStatus::AuthenticationRequired { methods, pending } => {
                    *state = RunState::AuthenticationRequired {
                        methods: methods.as_ref().clone(),
                        pending: *pending,
                    };
                    commands
                        .entity(session)
                        .remove::<vmux_session::TranscriptLoading>();
                }
                AgentRunStatus::Interrupted => {
                    *state = RunState::Idle;
                    if !queue.flush_pending() {
                        queue.paused = true;
                    }
                }
                AgentRunStatus::Errored(message) => {
                    commands
                        .entity(session)
                        .remove::<vmux_session::TranscriptLoading>();
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
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::protocol::{AgentRequest, AgentRequestId, ServiceMessage};
    use vmux_ecs::service::ServiceInbound;
    use vmux_team::AgentRenameProfile;

    #[test]
    fn promptless_setup_retains_recovery_until_the_agent_is_ready() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(Plugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("setup".into()),
                RunState::Idle,
                PromptQueue::default(),
                vmux_session::SetupPending,
            ))
            .id();
        for status in [
            AgentRunStatus::Starting,
            AgentRunStatus::AuthenticationRequired {
                methods: Box::new(Vec::new()),
                pending: false,
            },
            AgentRunStatus::Errored("setup failed".into()),
        ] {
            app.world_mut().write_message(AgentRunStatusChanged {
                sid: "setup".into(),
                status,
            });
            app.update();
            assert!(
                app.world()
                    .get::<vmux_session::SetupPending>(session)
                    .is_some()
            );
        }
        assert!(matches!(
            app.world().get::<RunState>(session),
            Some(RunState::Errored(error)) if error == "setup failed"
        ));
        app.world_mut().write_message(AgentRunStatusChanged {
            sid: "setup".into(),
            status: AgentRunStatus::Idle,
        });
        app.update();
        assert!(matches!(
            app.world().get::<RunState>(session),
            Some(RunState::Idle)
        ));
        assert!(
            app.world()
                .get::<vmux_session::SetupPending>(session)
                .is_none()
        );
        assert!(
            app.world_mut()
                .run_system_once(move |queues: vmux_session::QueuedPrompts| queues.empty(session))
                .unwrap()
        );
    }

    #[test]
    fn cancelled_setup_rejects_late_status_until_explicit_reattachment() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(Plugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("cancelled".into()),
                RunState::Errored("ACP setup cancelled".into()),
                PromptQueue::default(),
                super::super::navigation_driver::RetryRequested,
            ))
            .id();
        for status in [
            AgentRunStatus::Starting,
            AgentRunStatus::Idle,
            AgentRunStatus::AuthenticationRequired {
                methods: Box::new(Vec::new()),
                pending: false,
            },
        ] {
            app.world_mut().write_message(AgentRunStatusChanged {
                sid: "cancelled".into(),
                status,
            });
            app.update();
            assert!(
                matches!(app.world().get::<RunState>(session), Some(RunState::Errored(error)) if error == "ACP setup cancelled")
            );
        }
        app.world_mut()
            .entity_mut(session)
            .remove::<super::super::navigation_driver::RetryRequested>();
        app.world_mut().write_message(AgentRunStatusChanged {
            sid: "cancelled".into(),
            status: AgentRunStatus::Starting,
        });
        app.update();
        assert!(matches!(
            app.world().get::<RunState>(session),
            Some(RunState::Preparing)
        ));
    }

    #[test]
    fn routes_agent_requests_without_terminal_ownership() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(super::Plugin);
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

    #[test]
    fn message_snapshot_finishes_transcript_loading() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(super::Plugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("session-1".into()),
                vmux_session::TranscriptLoading,
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<AgentMessagesSnapshot>>()
            .write(AgentMessagesSnapshot {
                sid: "session-1".into(),
                messages: vec![Message::user("hello")],
            });

        app.update();

        assert!(
            !app.world()
                .entity(session)
                .contains::<vmux_session::TranscriptLoading>()
        );
        let snapshots = app
            .world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].session.0, "session-1");
    }

    #[test]
    fn empty_snapshot_preserves_persisted_transcript() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(super::Plugin);
        let session_id = SessionId("session-1".into());
        let session = app
            .world_mut()
            .spawn((Session, session_id.clone(), vmux_session::TranscriptLoading))
            .id();
        app.world_mut().spawn((
            vmux_session::ConversationEvent,
            vmux_session::EventIdentity {
                event_id: vmux_api::session::EventId::new("event-1"),
                actor_id: vmux_api::session::MemberId::local(&session_id),
                client_op_id: None,
                sequence: 1,
                reply_to: None,
            },
            vmux_session::MessageContent(Message::user("persisted")),
            vmux_ecs::CreatedAt(1),
            ChildOf(session),
        ));
        app.world_mut()
            .resource_mut::<Messages<AgentMessagesSnapshot>>()
            .write(AgentMessagesSnapshot {
                sid: session_id.0,
                messages: Vec::new(),
            });

        app.update();

        let snapshots = app
            .world_mut()
            .resource_mut::<Messages<SnapshotReceived>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(snapshots.is_empty());
        assert!(
            !app.world()
                .entity(session)
                .contains::<vmux_session::TranscriptLoading>()
        );
    }

    #[test]
    fn failed_resume_releases_loading_and_reports_the_error() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(super::Plugin);
        let session = app
            .world_mut()
            .spawn((
                Session,
                SessionId("session-1".into()),
                RunState::Idle,
                PromptQueue::default(),
                vmux_session::TranscriptLoading,
            ))
            .id();
        app.world_mut().write_message(AgentRunStatusChanged {
            sid: "session-1".into(),
            status: AgentRunStatus::Errored("history unavailable".into()),
        });
        app.update();
        assert!(
            !app.world()
                .entity(session)
                .contains::<vmux_session::TranscriptLoading>()
        );
        assert!(
            matches!(app.world().get::<RunState>(session), Some(RunState::Errored(error)) if error == "history unavailable")
        );
    }
}
