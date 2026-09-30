use bevy::prelude::*;
#[cfg(test)]
use vmux_api::protocol::ClientMessage;
use vmux_api::protocol::SharedMessage;
use vmux_command::WriteCommandRequests;
use vmux_core::agent::{AgentContinuationRequest, AgentSession};
use vmux_core::service::{ServiceConnected, ServiceMessageSet, ServiceRequest};
use vmux_session::{AcpSession, AgentRunState};
use vmux_terminal::BufferedAgentPrompt;

pub(super) struct AgentContinuationPlugin;

impl Plugin for AgentContinuationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentContinuationRequest>()
            .add_message::<ServiceRequest>()
            .add_systems(
                Update,
                (queue_agent_continuations, send_continuations)
                    .chain()
                    .in_set(WriteCommandRequests)
                    .after(ServiceMessageSet),
            );
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct PendingAgentContinuation(String);

fn queue_agent_continuations(
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
        Option<&AcpSession>,
        Option<&AgentSession>,
        Option<&mut AgentRunState>,
    )>,
    connected: Option<Single<(), With<ServiceConnected>>>,
    mut commands: Commands,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, continuation, acp, cli, state) in &mut sessions {
        if cli.is_some() {
            commands
                .entity(entity)
                .insert(BufferedAgentPrompt {
                    text: continuation.0.clone(),
                    submit: true,
                })
                .remove::<PendingAgentContinuation>();
            continue;
        }
        if connected.is_none() {
            continue;
        }
        let (Some(session), Some(mut state)) = (acp, state) else {
            continue;
        };
        if !matches!(*state, AgentRunState::Idle | AgentRunState::Errored(_)) {
            continue;
        }
        service_requests.write(ServiceRequest(
            SharedMessage::AgentInput {
                sid: session.sid.clone(),
                text: String::new(),
                context: Some(continuation.0.clone()),
                attachments: Vec::new(),
                preferred_mode: None,
            }
            .into(),
        ));
        *state = AgentRunState::Streaming;
        commands.entity(entity).remove::<PendingAgentContinuation>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::agent::AgentKind;

    #[test]
    fn cli_workspace_continuation_queues_terminal_prompt_without_service_wait() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<ServiceRequest>()
            .add_systems(Update, send_continuations);
        let entity = app
            .world_mut()
            .spawn((
                AgentSession {
                    kind: AgentKind::Codex,
                },
                PendingAgentContinuation("continue original request".to_string()),
            ))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<PendingAgentContinuation>(entity)
                .is_none()
        );
        assert_eq!(
            app.world().get::<BufferedAgentPrompt>(entity).unwrap(),
            &BufferedAgentPrompt {
                text: "continue original request".to_string(),
                submit: true,
            }
        );
    }

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
