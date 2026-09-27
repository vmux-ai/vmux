use bevy::prelude::*;
use vmux_api::protocol::{
    AgentBrowserNavigate, AgentCommand as ServiceAgentCommand, AgentCommandResult,
    AgentOpenInNewStack, AgentRequestId, SharedAgentCommand,
};
use vmux_command::{CommandDefinition, CommandInvocation};
use vmux_service::client::ServiceRequest;

use crate::host::event::{AgentCommandRequest, CommandOrigin};

use super::application::{
    AgentFocusPaneRequest, AgentNotifyRequest, AgentRenameProfileRequest,
    AgentUpdateLayoutRequest, AgentUpdateSettingsRequest,
};
use super::browser::{
    AgentBrowserGoBackRequest, AgentBrowserGoForwardRequest, AgentBrowserHistorySearchRequest,
    AgentBrowserInstallExtensionRequest, AgentBrowserNavigateRequest, AgentOpenInNewStackRequest,
};
use super::terminal::{
    AgentNewTerminalTabRequest, AgentRunShellRequest, AgentTerminalSendRequest,
};
use super::{AgentReply, CommandSet};

pub(super) struct DispatchPlugin;

impl Plugin for DispatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_systems(
                Update,
                forward_history_open_intent.in_set(CommandSet::History),
            )
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
                (invoke_commands, handle_shared_commands).in_set(CommandSet::Commands),
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

fn invoke_commands(
    mut requests: MessageReader<AgentCommandRequest>,
    command_definitions: Query<&CommandDefinition>,
    mut command_invocations: MessageWriter<CommandInvocation>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_api::protocol::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = match &request.command {
            ServiceAgentCommand::FileTouched(_)
            | ServiceAgentCommand::FileSearch(_)
            | ServiceAgentCommand::TurnEnded(_) => AgentCommandResult::Ok,
            ServiceAgentCommand::InvokeCommand(command) => {
                let args = match vmux_core::JsonArguments::try_from(&command.args) {
                    Ok(args) => args.0,
                    Err(message) => {
                        service_requests.write(ServiceRequest(
                            request.response(AgentCommandResult::Error(message)),
                        ));
                        continue;
                    }
                };
                let caller = match &request.origin {
                    CommandOrigin::Agent {
                        anchor: Some(pid),
                        ..
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
                let Some(definition) = command_definitions
                    .iter()
                    .find(|definition| definition.matches(&command.id))
                else {
                    service_requests.write(ServiceRequest(request.response(
                        AgentCommandResult::Error(format!("unknown app command: {}", command.id)),
                    )));
                    continue;
                };
                let invocation = if request.origin.is_agent() {
                    definition.agent_invocation(caller, args)
                } else {
                    definition.user_invocation(caller, args)
                };
                match invocation {
                    Ok(invocation) => {
                        command_invocations.write(invocation);
                        AgentCommandResult::Ok
                    }
                    Err(message) => AgentCommandResult::Error(message),
                }
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn handle_shared_commands(
    mut requests: MessageReader<AgentCommandRequest>,
    command_bar: Res<vmux_command::snapshot::CommandBarProjection>,
    contributed_pages: Query<&vmux_command::snapshot::ContributedPage>,
    mut new_tabs: MessageWriter<vmux_layout::NewTabRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = match &request.command {
            ServiceAgentCommand::Shared(SharedAgentCommand::NewAgentChat {
                prompt,
                agent_url,
                ..
            }) => match vmux_command::snapshot::ContributedPage::prompt_url(
                &contributed_pages,
                agent_url.as_deref(),
            ) {
                Some(url) => {
                    new_tabs.write(vmux_layout::NewTabRequest {
                        url,
                        pending_prompt: Some(prompt.clone()),
                    });
                    AgentCommandResult::Ok
                }
                None => AgentCommandResult::Error("no agent is installed".to_string()),
            },
            ServiceAgentCommand::Shared(SharedAgentCommand::ListAgents) => {
                match serde_json::to_string(&remote_agents(&command_bar.agents)) {
                    Ok(json) => AgentCommandResult::Text(json),
                    Err(error) => AgentCommandResult::Error(format!("list_agents: {error}")),
                }
            }
            _ => continue,
        };
        service_requests.write(ServiceRequest(request.response(result)));
    }
}

fn remote_agents(
    snapshot: &vmux_command::snapshot::CommandBarAgentsSnapshot,
) -> Vec<vmux_api::room::RemoteAgent> {
    snapshot
        .acp
        .iter()
        .map(|agent| vmux_api::room::RemoteAgent {
            id: agent.id.clone(),
            name: agent.name.clone(),
            url: agent.url.clone(),
            icon: agent.icon.clone(),
        })
        .chain(
            snapshot
                .providers
                .iter()
                .map(|agent| vmux_api::room::RemoteAgent {
                    id: agent.id.clone(),
                    name: format!("{} (CLI)", agent.name),
                    url: agent.url.clone(),
                    icon: agent.icon.clone(),
                }),
        )
        .collect()
}

fn forward_history_open_intent(
    mut intents: MessageReader<vmux_history::query::HistoryOpenIntent>,
    mut requests: MessageWriter<AgentCommandRequest>,
) {
    for intent in intents.read() {
        let command = if intent.in_new_stack {
            ServiceAgentCommand::OpenInNewStack(AgentOpenInNewStack {
                url: intent.url.clone(),
            })
        } else {
            ServiceAgentCommand::BrowserNavigate(AgentBrowserNavigate {
                url: intent.url.clone(),
                pane: None,
            })
        };
        requests.write(AgentCommandRequest {
            request_id: AgentRequestId::new(),
            origin: CommandOrigin::User,
            command,
        });
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
