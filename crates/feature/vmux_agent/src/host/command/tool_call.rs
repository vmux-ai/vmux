use bevy::prelude::*;
use vmux_api::protocol::{AgentRequestId, ClientMessage};
use vmux_ecs::JsonArguments;
use vmux_ecs::service::ServiceRequest;
use vmux_tool::{
    ToolCommand, ToolCommandFallback, ToolDispatchError, ToolDispatchFlush, ToolInvocation,
    ToolQuery, ToolQueryRequest, ToolResolveSet,
};

use crate::host::event::{AgentRequestInput, AgentToolCallRequest, CommandOrigin};

use super::CommandSet;

pub(super) fn add(app: &mut App) {
    app.add_message::<ServiceRequest>()
        .add_systems(
            Update,
            (handle_tool_calls, ApplyDeferred)
                .chain()
                .in_set(CommandSet::ToolCalls)
                .before(ToolResolveSet),
        )
        .add_systems(
            Update,
            (forward_commands, forward_queries, report_errors)
                .after(ToolDispatchFlush)
                .before(CommandSet::Commands),
        );
}

#[derive(Component)]
struct PendingAgentToolCall {
    request_id: AgentRequestId,
    sid: String,
}

fn handle_tool_calls(
    mut commands: Commands,
    mut reader: MessageReader<AgentToolCallRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in reader.read() {
        let arguments = match JsonArguments::try_from(&request.args) {
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
            ToolInvocation,
            ToolCommandFallback,
            PendingAgentToolCall {
                request_id: request.request_id,
                sid: request.sid.clone(),
            },
        ));
    }
}

fn forward_commands(
    mut commands: Commands,
    calls: Query<(Entity, &PendingAgentToolCall, &ToolCommand), Added<ToolCommand>>,
    mut request_writer: MessageWriter<AgentRequestInput>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, pending, result) in &calls {
        match &result.0 {
            Ok(request) => {
                request_writer.write(AgentRequestInput {
                    request_id: pending.request_id,
                    origin: CommandOrigin::Agent {
                        sid: Some(pending.sid.clone()),
                        anchor: None,
                    },
                    request: request.clone(),
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

fn forward_queries(
    mut commands: Commands,
    calls: Query<(Entity, &PendingAgentToolCall, &ToolQuery), Added<ToolQuery>>,
    mut query_writer: MessageWriter<ToolQueryRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for (entity, pending, result) in &calls {
        match &result.0 {
            Ok(query) => {
                query_writer.write(ToolQueryRequest {
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

fn report_errors(
    mut commands: Commands,
    calls: Query<(Entity, &PendingAgentToolCall, &ToolDispatchError), Added<ToolDispatchError>>,
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
    use serde::Deserialize;
    use vmux_api::protocol::AgentRequest;
    use vmux_ecs::manifest::FeaturePlugin;
    use vmux_tool::{AddedTool, ToolDispatchSet, ToolQuery, ToolRegistryPlugin};

    #[vmux_tool::input]
    #[derive(Component, Deserialize)]
    struct TestQueryInput {}

    struct TestFeature;

    impl vmux_ecs::manifest::FeatureManifestSource for TestFeature {
        const SOURCE: &'static str =
            r#"(tools: [(name: "test_query", description: "test", input_schema: (type: Object))])"#;
    }

    #[derive(Resource, Default)]
    struct CapturedAgentQueries(Vec<AgentRequest>);

    fn capture(
        mut requests: MessageReader<ToolQueryRequest>,
        mut captured: ResMut<CapturedAgentQueries>,
    ) {
        for request in requests.read() {
            captured.0.push(request.query.clone());
        }
    }

    fn create_test_query(
        mut commands: Commands,
        requests: Query<Entity, AddedTool<TestQueryInput>>,
    ) {
        for request in &requests {
            commands.entity(request).insert(ToolQuery(Ok(AgentRequest {
                id: "test_query@1".to_string(),
                body: Vec::new(),
            })));
        }
    }

    #[test]
    fn agent_tools_dispatch_through_the_owning_world() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            FeaturePlugin::<TestFeature>::default(),
            ToolRegistryPlugin,
        ))
        .add_message::<AgentToolCallRequest>()
        .add_message::<AgentRequestInput>()
        .add_message::<ToolQueryRequest>()
        .init_resource::<CapturedAgentQueries>()
        .add_systems(
            Update,
            (
                create_test_query.in_set(ToolDispatchSet),
                capture.after(CommandSet::Commands),
            ),
        );
        add(&mut app);
        app.update();

        app.world_mut()
            .resource_mut::<Messages<AgentToolCallRequest>>()
            .write(AgentToolCallRequest {
                request_id: AgentRequestId::new(),
                sid: "agent".to_string(),
                name: "test_query".to_string(),
                args: vmux_api::json::JsonValue::from(serde_json::json!({})),
            });
        app.update();

        let captured = app.world().resource::<CapturedAgentQueries>();
        assert_eq!(captured.0.len(), 1);
        assert_eq!(captured.0[0].id, "test_query@1");
    }
}
