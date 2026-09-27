use bevy::prelude::*;
use vmux_api::protocol::{AgentCommand as ServiceAgentCommand, SharedAgentCommand};
use vmux_service::client::ServiceRequest;

use crate::host::event::{AgentCommandRequest, CommandOrigin};

use super::application::{
    AgentFocusPaneRequest, AgentNotifyRequest, AgentRenameProfileRequest, AgentUpdateLayoutRequest,
    AgentUpdateSettingsRequest,
};
use super::browser::{
    AgentBrowserGoBackRequest, AgentBrowserGoForwardRequest, AgentBrowserHistorySearchRequest,
    AgentBrowserInstallExtensionRequest, AgentBrowserNavigateRequest, AgentOpenInNewStackRequest,
};
use super::operation::{
    AgentFileSearchRequest, AgentFileTouchedRequest, AgentInvokeCommandRequest, AgentListRequest,
    AgentNewChatRequest, AgentTurnEndedRequest,
};
use super::terminal::{AgentNewTerminalTabRequest, AgentRunShellRequest, AgentTerminalSendRequest};
use super::{AgentReply, CommandSet};

pub(super) struct DispatchPlugin;

impl Plugin for DispatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_systems(
                Update,
                (
                    route_terminal_operations,
                    route_browser_operations,
                    route_application_operations,
                )
                    .in_set(CommandSet::Dispatch),
            )
            .add_systems(
                Update,
                route_remaining_operations.in_set(CommandSet::Dispatch),
            );
    }
}

fn route_terminal_operations(
    mut commands: MessageReader<AgentCommandRequest>,
    mut new_terminal_tab: MessageWriter<AgentNewTerminalTabRequest>,
    mut run_shell: MessageWriter<AgentRunShellRequest>,
    mut terminal_send: MessageWriter<AgentTerminalSendRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            ServiceAgentCommand::NewTerminalTab(payload) => {
                new_terminal_tab.write(AgentNewTerminalTabRequest {
                    reply,
                    activate: !request.origin.is_agent(),
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::RunShell(payload) => {
                run_shell.write(AgentRunShellRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::TerminalSend(payload) => {
                terminal_send.write(AgentTerminalSendRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn route_browser_operations(
    mut commands: MessageReader<AgentCommandRequest>,
    mut navigate: MessageWriter<AgentBrowserNavigateRequest>,
    mut install_extension: MessageWriter<AgentBrowserInstallExtensionRequest>,
    mut go_back: MessageWriter<AgentBrowserGoBackRequest>,
    mut go_forward: MessageWriter<AgentBrowserGoForwardRequest>,
    mut search_history: MessageWriter<AgentBrowserHistorySearchRequest>,
    mut open_in_new_stack: MessageWriter<AgentOpenInNewStackRequest>,
) {
    for request in commands.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            ServiceAgentCommand::BrowserNavigate(payload) => {
                navigate.write(AgentBrowserNavigateRequest {
                    reply,
                    origin: request.origin.clone(),
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BrowserInstallExtension(payload) => {
                install_extension.write(AgentBrowserInstallExtensionRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BrowserGoBack(payload) => {
                go_back.write(AgentBrowserGoBackRequest {
                    reply,
                    origin: request.origin.clone(),
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BrowserGoForward(payload) => {
                go_forward.write(AgentBrowserGoForwardRequest {
                    reply,
                    origin: request.origin.clone(),
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::BrowserHistorySearch(payload) => {
                search_history.write(AgentBrowserHistorySearchRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::OpenInNewStack(payload) => {
                open_in_new_stack.write(AgentOpenInNewStackRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            _ => {}
        }
    }
}

fn route_application_operations(
    mut commands: MessageReader<AgentCommandRequest>,
    mut notify: MessageWriter<AgentNotifyRequest>,
    mut focus_pane: MessageWriter<AgentFocusPaneRequest>,
    mut rename_profile: MessageWriter<AgentRenameProfileRequest>,
    mut update_settings: MessageWriter<AgentUpdateSettingsRequest>,
    mut update_layout: MessageWriter<AgentUpdateLayoutRequest>,
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
            ServiceAgentCommand::FocusPane(payload) => {
                focus_pane.write(AgentFocusPaneRequest {
                    reply,
                    allowed: !request.origin.is_agent(),
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::RenameProfile(payload) => {
                rename_profile.write(AgentRenameProfileRequest {
                    reply,
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::UpdateSettings(payload) => {
                update_settings.write(AgentUpdateSettingsRequest {
                    reply,
                    from_agent: request.origin.is_agent(),
                    payload: payload.clone(),
                });
            }
            ServiceAgentCommand::UpdateLayout(payload) => {
                update_layout.write(AgentUpdateLayoutRequest {
                    reply,
                    from_agent: request.origin.is_agent(),
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
    mut invoke: MessageWriter<AgentInvokeCommandRequest>,
    mut file_touched: MessageWriter<AgentFileTouchedRequest>,
    mut file_search: MessageWriter<AgentFileSearchRequest>,
    mut turn_ended: MessageWriter<AgentTurnEndedRequest>,
    mut new_chat: MessageWriter<AgentNewChatRequest>,
    mut list_agents: MessageWriter<AgentListRequest>,
) {
    for request in requests.read() {
        let reply = AgentReply::new(request.request_id);
        match &request.command {
            ServiceAgentCommand::InvokeCommand(payload) => {
                invoke.write(AgentInvokeCommandRequest {
                    reply,
                    origin: request.origin.clone(),
                    payload: payload.clone(),
                });
            }
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
                    _payload: payload.clone(),
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
