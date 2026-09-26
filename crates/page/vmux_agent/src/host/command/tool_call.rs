use bevy::prelude::*;
use vmux_service::client::ServiceRequest;
use vmux_service::protocol::{AgentRequestId, ClientMessage};

use crate::events::{AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, CommandOrigin};

use super::CommandSet;

pub(super) struct ToolCallPlugin;

impl Plugin for ToolCallPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServiceRequest>()
            .add_systems(
                Update,
                handle_agent_tool_calls
                    .in_set(CommandSet::ToolCalls)
                    .before(vmux_tool::ToolResolveSet),
            )
            .add_systems(
                Update,
                (
                    finish_agent_tool_commands,
                    finish_agent_tool_queries,
                    fail_agent_tool_calls,
                )
                    .after(vmux_tool::ToolDispatchFlush)
                    .before(CommandSet::Commands),
            );
    }
}

#[derive(Component)]
struct PendingAgentToolCall {
    request_id: AgentRequestId,
    sid: String,
}

fn handle_agent_tool_calls(
    mut commands: Commands,
    mut reader: MessageReader<AgentToolCallRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let arguments = match vmux_core::JsonArguments::try_from(&request.args) {
            Ok(arguments) => arguments,
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentToolResult {
                    request_id: request.request_id,
                    content: message,
                    is_error: true,
                }));
                continue;
            }
        };
        commands.spawn((
            Name::new(request.name.clone()),
            arguments,
            vmux_tool::ToolInvocation,
            vmux_tool::ToolCommandFallback,
            PendingAgentToolCall {
                request_id: request.request_id,
                sid: request.sid.clone(),
            },
        ));
    }
}

fn finish_agent_tool_commands(
    mut commands: Commands,
    calls: Query<
        (Entity, &PendingAgentToolCall, &vmux_tool::ToolCommand),
        Added<vmux_tool::ToolCommand>,
    >,
    mut command_writer: MessageWriter<AgentCommandRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, pending, result) in &calls {
        match &result.0 {
            Ok(command) => {
                command_writer.write(AgentCommandRequest {
                    request_id: pending.request_id,
                    origin: CommandOrigin::Agent {
                        sid: Some(pending.sid.clone()),
                        anchor: None,
                    },
                    command: command.clone(),
                });
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentToolResult {
                    request_id: pending.request_id,
                    content: message.clone(),
                    is_error: true,
                }));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn finish_agent_tool_queries(
    mut commands: Commands,
    calls: Query<
        (Entity, &PendingAgentToolCall, &vmux_tool::ToolQuery),
        Added<vmux_tool::ToolQuery>,
    >,
    mut query_writer: MessageWriter<AgentQueryRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, pending, result) in &calls {
        match &result.0 {
            Ok(query) => {
                query_writer.write(AgentQueryRequest {
                    request_id: pending.request_id,
                    query: query.clone(),
                });
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentToolResult {
                    request_id: pending.request_id,
                    content: message.clone(),
                    is_error: true,
                }));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn fail_agent_tool_calls(
    mut commands: Commands,
    calls: Query<
        (Entity, &PendingAgentToolCall, &vmux_tool::ToolDispatchError),
        Added<vmux_tool::ToolDispatchError>,
    >,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, pending, error) in &calls {
        service_requests.write(ServiceRequest(ClientMessage::AgentToolResult {
            request_id: pending.request_id,
            content: error.message().to_string(),
            is_error: true,
        }));
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::AgentQuery;

    #[derive(Resource, Default)]
    struct CapturedAgentQueries(Vec<AgentQuery>);

    impl CapturedAgentQueries {
        fn read(mut requests: MessageReader<AgentQueryRequest>, mut captured: ResMut<Self>) {
            for request in requests.read() {
                captured.0.push(request.query.clone());
            }
        }
    }

    #[test]
    fn agent_tools_dispatch_through_the_owning_world() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::CaptureToolPlugin, ToolCallPlugin))
            .add_message::<AgentToolCallRequest>()
            .add_message::<AgentCommandRequest>()
            .add_message::<AgentQueryRequest>()
            .init_resource::<CapturedAgentQueries>()
            .add_systems(
                Update,
                CapturedAgentQueries::read.after(CommandSet::Commands),
            );
        app.update();

        app.world_mut()
            .resource_mut::<Messages<AgentToolCallRequest>>()
            .write(AgentToolCallRequest {
                request_id: AgentRequestId::new(),
                sid: "agent".to_string(),
                name: "screenshot".to_string(),
                args: vmux_api::json::JsonValue::from(serde_json::json!({})),
            });
        app.update();

        assert!(matches!(
            app.world().resource::<CapturedAgentQueries>().0.as_slice(),
            [AgentQuery::Screenshot { pane: None }]
        ));
    }
}
