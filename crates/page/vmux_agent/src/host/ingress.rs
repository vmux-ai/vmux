use crate::events::{
    AgentCommandRequest, AgentToolCallRequest, CommandOrigin, PageAgentAcpTerminalCreated,
    PageAgentApprovalResolved, PageAgentAwaitingApproval, PageAgentDelta, PageAgentInfo,
    PageAgentModeInfo, PageAgentModeSelectionResult, PageAgentModelInfo,
    PageAgentModelSelectionResult, PageAgentRunStatus, PageAgentSessionCreated, PageAgentSnapshot,
    PageAgentWorkspaceChanged,
};
use bevy::prelude::*;
use vmux_api::protocol::{ServiceMessage, SharedEvent};
use vmux_service::client::ServiceInbound;
use vmux_terminal::ServiceMessageSet;

pub(crate) struct AgentIngressPlugin;

impl Plugin for AgentIngressPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceInbound>()
            .add_message::<AgentCommandRequest>()
            .add_message::<AgentToolCallRequest>()
            .add_message::<PageAgentDelta>()
            .add_message::<PageAgentRunStatus>()
            .add_message::<PageAgentAwaitingApproval>()
            .add_message::<PageAgentApprovalResolved>()
            .add_message::<PageAgentSnapshot>()
            .add_message::<PageAgentInfo>()
            .add_message::<PageAgentWorkspaceChanged>()
            .add_message::<PageAgentModelInfo>()
            .add_message::<PageAgentModelSelectionResult>()
            .add_message::<PageAgentModeInfo>()
            .add_message::<PageAgentModeSelectionResult>()
            .add_message::<PageAgentSessionCreated>()
            .add_message::<PageAgentAcpTerminalCreated>()
            .add_systems(Update, route_service_messages.in_set(ServiceMessageSet));
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct AgentIngressWriters<'w> {
    commands: MessageWriter<'w, AgentCommandRequest>,
    tool_calls: MessageWriter<'w, AgentToolCallRequest>,
    deltas: MessageWriter<'w, PageAgentDelta>,
    run_statuses: MessageWriter<'w, PageAgentRunStatus>,
    approvals: MessageWriter<'w, PageAgentAwaitingApproval>,
    approval_resolutions: MessageWriter<'w, PageAgentApprovalResolved>,
    snapshots: MessageWriter<'w, PageAgentSnapshot>,
    agent_info: MessageWriter<'w, PageAgentInfo>,
    workspace_changes: MessageWriter<'w, PageAgentWorkspaceChanged>,
    model_info: MessageWriter<'w, PageAgentModelInfo>,
    model_selection_results: MessageWriter<'w, PageAgentModelSelectionResult>,
    mode_info: MessageWriter<'w, PageAgentModeInfo>,
    mode_selection_results: MessageWriter<'w, PageAgentModeSelectionResult>,
    session_created: MessageWriter<'w, PageAgentSessionCreated>,
    terminal_created: MessageWriter<'w, PageAgentAcpTerminalCreated>,
}

fn route_service_messages(
    mut inbound: MessageReader<ServiceInbound>,
    mut writers: AgentIngressWriters,
) {
    for inbound in inbound.read() {
        match &inbound.0 {
            ServiceMessage::AgentCommand {
                request_id,
                anchor,
                command,
            } => {
                writers.commands.write(AgentCommandRequest {
                    request_id: *request_id,
                    origin: CommandOrigin::Agent {
                        sid: None,
                        anchor: *anchor,
                    },
                    command: command.clone(),
                });
            }
            ServiceMessage::AgentToolCall {
                request_id,
                sid,
                name,
                args,
            } => {
                writers.tool_calls.write(AgentToolCallRequest {
                    request_id: *request_id,
                    sid: sid.clone(),
                    name: name.clone(),
                    args: args.clone(),
                });
            }
            ServiceMessage::Shared(SharedEvent::AgentDelta { sid, text }) => {
                writers.deltas.write(PageAgentDelta {
                    sid: sid.clone(),
                    text: text.clone(),
                });
            }
            ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged { sid, status }) => {
                writers.run_statuses.write(PageAgentRunStatus {
                    sid: sid.clone(),
                    status: status.clone(),
                });
            }
            ServiceMessage::Shared(SharedEvent::AgentAwaitingApproval {
                sid,
                call_id,
                name,
                args,
            }) => {
                let args = serde_json::Value::try_from(args)
                    .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
                writers.approvals.write(PageAgentAwaitingApproval {
                    sid: sid.clone(),
                    call_id: call_id.clone(),
                    name: name.clone(),
                    args,
                });
            }
            ServiceMessage::Shared(SharedEvent::AgentApprovalResolved { sid, call_id }) => {
                writers
                    .approval_resolutions
                    .write(PageAgentApprovalResolved {
                        sid: sid.clone(),
                        call_id: call_id.clone(),
                    });
            }
            ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot { sid, messages }) => {
                writers.snapshots.write(PageAgentSnapshot {
                    sid: sid.clone(),
                    messages: messages.clone(),
                });
            }
            ServiceMessage::Shared(SharedEvent::AcpAgentInfo { sid, name }) => {
                writers.agent_info.write(PageAgentInfo {
                    sid: sid.clone(),
                    name: name.clone(),
                });
            }
            ServiceMessage::Shared(SharedEvent::AcpWorkspaceChanged {
                sid,
                name,
                branch,
                cwd,
                workspace_cwd,
            }) => {
                writers.workspace_changes.write(PageAgentWorkspaceChanged {
                    sid: sid.clone(),
                    name: name.clone(),
                    branch: branch.clone(),
                    cwd: cwd.clone(),
                    workspace_cwd: workspace_cwd.clone(),
                });
            }
            ServiceMessage::Shared(SharedEvent::AcpModelInfo {
                sid,
                config_id,
                current_model_id,
                models,
            }) => {
                writers.model_info.write(PageAgentModelInfo {
                    sid: sid.clone(),
                    config_id: config_id.clone(),
                    current_model_id: current_model_id.clone(),
                    models: models.clone(),
                });
            }
            ServiceMessage::AcpModelSelectionResult {
                sid,
                request_id,
                model_id,
                succeeded,
            } => {
                writers
                    .model_selection_results
                    .write(PageAgentModelSelectionResult {
                        sid: sid.clone(),
                        request_id: *request_id,
                        model_id: model_id.clone(),
                        succeeded: *succeeded,
                    });
            }
            ServiceMessage::AcpModeInfo {
                sid,
                config_id,
                current_mode_id,
                modes,
            } => {
                writers.mode_info.write(PageAgentModeInfo {
                    sid: sid.clone(),
                    config_id: config_id.clone(),
                    current_mode_id: current_mode_id.clone(),
                    modes: modes.clone(),
                });
            }
            ServiceMessage::AcpModeSelectionResult {
                sid,
                request_id,
                mode_id,
                succeeded,
            } => {
                writers
                    .mode_selection_results
                    .write(PageAgentModeSelectionResult {
                        sid: sid.clone(),
                        request_id: *request_id,
                        mode_id: mode_id.clone(),
                        succeeded: *succeeded,
                    });
            }
            ServiceMessage::AcpSessionCreated {
                sid,
                acp_session_id,
            } => {
                writers.session_created.write(PageAgentSessionCreated {
                    sid: sid.clone(),
                    acp_session_id: acp_session_id.clone(),
                });
            }
            ServiceMessage::AcpTerminalCreated {
                sid,
                terminal_id,
                process_id,
                command,
                args,
                cwd,
            } => {
                writers.terminal_created.write(PageAgentAcpTerminalCreated {
                    sid: sid.clone(),
                    terminal_id: terminal_id.clone(),
                    process_id: *process_id,
                    command: command.clone(),
                    args: args.clone(),
                    cwd: cwd.clone(),
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::protocol::{AgentCommand, AgentRequestId};

    #[test]
    fn routes_agent_messages_without_terminal_ownership() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AgentIngressPlugin));
        let request_id = AgentRequestId([7; 16]);
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::AgentCommand {
                request_id,
                anchor: None,
                command: AgentCommand::RenameProfile(vmux_api::protocol::AgentRenameProfile {
                    name: "Profile".into(),
                }),
            }));
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::Shared(
                SharedEvent::AgentDelta {
                    sid: "session".into(),
                    text: "hello".into(),
                },
            )));

        app.update();

        let commands = app
            .world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .drain()
            .collect::<Vec<_>>();
        let deltas = app
            .world_mut()
            .resource_mut::<Messages<PageAgentDelta>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].request_id, request_id);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].sid, "session");
        assert_eq!(deltas[0].text, "hello");
    }
}
