use bevy::prelude::*;
use vmux_service::client::ServiceClient;
use vmux_service::protocol::{AgentRequestId, ClientMessage};

use crate::events::{AgentCommandRequest, AgentQueryRequest, AgentToolCallRequest, CommandOrigin};

use super::{CommandArguments, CommandSet};

pub(super) struct ToolCallPlugin;

impl Plugin for ToolCallPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_agent_tool_calls
                .in_set(CommandSet::ToolCalls)
                .before(vmux_mcp::tool::ToolRequestSet),
        )
        .add_systems(
            Update,
            (
                finish_agent_tool_commands,
                finish_agent_tool_queries,
                fail_agent_tool_calls,
            )
                .after(vmux_mcp::tool::ToolDispatchFlush)
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
    tools: vmux_mcp::tool::ToolRegistry,
    service: Option<Res<ServiceClient>>,
) {
    for request in reader.read() {
        let arguments = match CommandArguments::try_from(&request.args) {
            Ok(arguments) => arguments.0,
            Err(message) => {
                if let Some(service) = service.as_ref() {
                    service.0.send(ClientMessage::AgentToolResult {
                        request_id: request.request_id,
                        content: message,
                        is_error: true,
                    });
                }
                continue;
            }
        };
        match tools.call(
            &request.name,
            arguments,
            None,
            "",
            vmux_mcp::tool::ToolCallPolicy::agent(),
        ) {
            Ok(call) => {
                commands.spawn((
                    call,
                    PendingAgentToolCall {
                        request_id: request.request_id,
                        sid: request.sid.clone(),
                    },
                ));
            }
            Err(message) => {
                if let Some(service) = service.as_ref() {
                    service.0.send(ClientMessage::AgentToolResult {
                        request_id: request.request_id,
                        content: message,
                        is_error: true,
                    });
                }
            }
        }
    }
}

fn finish_agent_tool_commands(
    mut commands: Commands,
    calls: Query<
        (Entity, &PendingAgentToolCall, &vmux_mcp::tool::ToolCommand),
        Added<vmux_mcp::tool::ToolCommand>,
    >,
    mut command_writer: MessageWriter<AgentCommandRequest>,
    service: Option<Res<ServiceClient>>,
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
                if let Some(service) = service.as_ref() {
                    service.0.send(ClientMessage::AgentToolResult {
                        request_id: pending.request_id,
                        content: message.clone(),
                        is_error: true,
                    });
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

fn finish_agent_tool_queries(
    mut commands: Commands,
    calls: Query<
        (Entity, &PendingAgentToolCall, &vmux_mcp::tool::ToolQuery),
        Added<vmux_mcp::tool::ToolQuery>,
    >,
    mut query_writer: MessageWriter<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
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
                if let Some(service) = service.as_ref() {
                    service.0.send(ClientMessage::AgentToolResult {
                        request_id: pending.request_id,
                        content: message.clone(),
                        is_error: true,
                    });
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

fn fail_agent_tool_calls(
    mut commands: Commands,
    calls: Query<
        (
            Entity,
            &PendingAgentToolCall,
            &vmux_mcp::tool::ToolDispatchError,
        ),
        Added<vmux_mcp::tool::ToolDispatchError>,
    >,
    service: Option<Res<ServiceClient>>,
) {
    for (entity, pending, error) in &calls {
        if let Some(service) = service.as_ref() {
            service.0.send(ClientMessage::AgentToolResult {
                request_id: pending.request_id,
                content: error.message().to_string(),
                is_error: true,
            });
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_service::protocol::AgentCommand;

    #[derive(Resource, Default)]
    struct CapturedAgentCommands(Vec<AgentCommand>);

    impl CapturedAgentCommands {
        fn read(mut requests: MessageReader<AgentCommandRequest>, mut captured: ResMut<Self>) {
            for request in requests.read() {
                captured.0.push(request.command.clone());
            }
        }
    }

    #[test]
    fn agent_tools_dispatch_through_the_owning_world() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_mcp::tool::BuiltinToolPlugin,
            ToolCallPlugin,
        ))
        .add_message::<AgentToolCallRequest>()
        .add_message::<AgentCommandRequest>()
        .add_message::<AgentQueryRequest>()
        .init_resource::<CapturedAgentCommands>()
        .add_systems(
            Update,
            CapturedAgentCommands::read.after(vmux_mcp::tool::ToolDispatchFlush),
        );
        app.update();

        app.world_mut()
            .resource_mut::<Messages<AgentToolCallRequest>>()
            .write(AgentToolCallRequest {
                request_id: AgentRequestId::new(),
                sid: "agent".to_string(),
                name: "notify".to_string(),
                args: vmux_api::json::JsonValue::from(serde_json::json!({"body": "done"})),
            });
        app.update();

        assert!(matches!(
            app.world().resource::<CapturedAgentCommands>().0.as_slice(),
            [AgentCommand::Notify {
                title: None,
                body: Some(body),
            }] if body == "done"
        ));
    }
}
