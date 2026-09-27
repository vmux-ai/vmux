use bevy::prelude::*;
use vmux_api::protocol::{AgentCommand as ServiceAgentCommand, SharedAgentCommand};

use crate::host::event::AgentCommandRequest;

use super::application::AgentNotifyRequest;
use super::operation::{
    AgentFileSearchRequest, AgentFileTouchedRequest, AgentListRequest, AgentNewChatRequest,
    AgentTurnEndedRequest,
};
use super::{AgentReply, CommandSet};

pub(super) struct DispatchPlugin;

impl Plugin for DispatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            route_application_operations.in_set(CommandSet::Dispatch),
        )
        .add_systems(
            Update,
            route_remaining_operations.in_set(CommandSet::Dispatch),
        );
    }
}

fn route_application_operations(
    mut commands: MessageReader<AgentCommandRequest>,
    mut notify: MessageWriter<AgentNotifyRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            ServiceAgentCommand::Notify(payload) => {
                notify.write(AgentNotifyRequest {
                    reply,
                    origin: request.origin.clone(),
                    payload: payload.clone(),
                });
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn route_remaining_operations(
    mut requests: MessageReader<AgentCommandRequest>,
    mut file_touched: MessageWriter<AgentFileTouchedRequest>,
    mut file_search: MessageWriter<AgentFileSearchRequest>,
    mut turn_ended: MessageWriter<AgentTurnEndedRequest>,
    mut new_chat: MessageWriter<AgentNewChatRequest>,
    mut list_agents: MessageWriter<AgentListRequest>,
) {
    for request in requests.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            ServiceAgentCommand::FileTouched(payload) => {
                file_touched.write(AgentFileTouchedRequest {
                    reply,
                    _payload: payload.clone(),
                });
            }
            ServiceAgentCommand::FileSearch(payload) => {
                file_search.write(AgentFileSearchRequest {
                    reply,
                    _payload: payload.clone(),
                });
            }
            ServiceAgentCommand::TurnEnded(payload) => {
                turn_ended.write(AgentTurnEndedRequest {
                    reply,
                    _payload: *payload,
                });
            }
            ServiceAgentCommand::Shared(SharedAgentCommand::NewAgentChat {
                prompt,
                agent_url,
                ..
            }) => {
                new_chat.write(AgentNewChatRequest {
                    reply,
                    prompt: prompt.clone(),
                    agent_url: agent_url.clone(),
                });
            }
            ServiceAgentCommand::Shared(SharedAgentCommand::ListAgents) => {
                list_agents.write(AgentListRequest { reply });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::event::CommandOrigin;
    use vmux_api::protocol::ProcessId;

    #[test]
    fn agent_origin_clears_requested_focus() {
        let origin = CommandOrigin::Agent {
            sid: Some("s1".into()),
            anchor: Some(ProcessId::new()),
        };

        assert!(!origin.allows_focus(true));
        assert!(!origin.allows_focus(false));
    }

    #[test]
    fn user_origin_keeps_requested_focus() {
        assert!(CommandOrigin::User.allows_focus(true));
        assert!(!CommandOrigin::User.allows_focus(false));
    }

    #[test]
    fn command_arguments_reject_non_object_json() {
        assert!(vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::Null).is_err());
        assert!(
            vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::Array(Vec::new()))
                .is_err()
        );
        assert!(
            vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::Number("x".to_string()))
                .is_err()
        );
        assert_eq!(
            vmux_core::JsonArguments::try_from(&vmux_api::json::JsonValue::from(
                serde_json::json!({ "url": "https://example.com" })
            ))
            .unwrap()
            .0,
            serde_json::json!({ "url": "https://example.com" })
        );
    }
}
