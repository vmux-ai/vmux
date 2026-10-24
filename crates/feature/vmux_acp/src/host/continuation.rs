use bevy::prelude::*;
#[cfg(test)]
use vmux_api::protocol::ClientMessage;
use vmux_api::protocol::SharedMessage;
use vmux_command::WriteCommandRequests;
use vmux_ecs::agent::AgentContinuationRequest;
use vmux_ecs::service::{ServiceConnected, ServiceMessageSet, ServiceRequest};
use vmux_session::{RunState, SessionId};

pub(super) fn add(app: &mut App) {
    app.add_message::<AgentContinuationRequest>()
        .add_message::<ServiceRequest>()
        .add_systems(
            Update,
            (queue_continuations, send_continuations)
                .chain()
                .in_set(WriteCommandRequests)
                .after(ServiceMessageSet),
        );
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct PendingAgentContinuation(String);

fn queue_continuations(
    mut requests: MessageReader<AgentContinuationRequest>,
    mut commands: Commands,
) {
    for request in requests.read() {
        commands
            .entity(request.session)
            .insert(PendingAgentContinuation(request.context.clone()));
    }
}

fn send_continuations(
    mut sessions: Query<(
        Entity,
        &PendingAgentContinuation,
        Option<&SessionId>,
        Option<&mut RunState>,
    )>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, continuation, acp, state) in &mut sessions {
        if connected.is_none() {
            continue;
        }
        let (Some(session_id), Some(mut state)) = (acp, state) else {
            continue;
        };
        if !matches!(*state, RunState::Idle | RunState::Errored(_)) {
            continue;
        }
        service_requests.write(ServiceRequest(
            SharedMessage::AgentInput {
                sid: session_id.0.clone(),
                text: String::new(),
                context: Some(continuation.0.clone()),
                attachments: Vec::new(),
                preferred_mode: None,
            }
            .into(),
        ));
        *state = RunState::Streaming;
        commands.entity(entity).remove::<PendingAgentContinuation>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_workspace_continuation_is_private_same_session_input() {
        let message: ClientMessage = SharedMessage::AgentInput {
            sid: "sid-1".to_string(),
            text: String::new(),
            context: Some("continue original request".to_string()),
            attachments: Vec::new(),
            preferred_mode: None,
        }
        .into();

        assert!(matches!(
            message,
            ClientMessage::Shared(SharedMessage::AgentInput {
                sid,
                text,
                context,
                ..
            }) if sid == "sid-1"
                && text.is_empty()
                && context.as_deref() == Some("continue original request")
        ));
    }
}
