use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentInvokeCommand, AgentListCommands, AgentNotify, AgentRequest, ClientMessage, JsonValue,
};
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQueryHandled,
    ToolQueryRequest, ToolQueryRouteSet,
};

pub struct CommandToolPlugin;

impl Plugin for CommandToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("../feature.ron"),
            "default",
        ))
        .register_tool::<OpenCommandBarArgs>("open_command_bar")
        .register_tool::<NotifyArgs>("notify")
        .add_message::<ToolQueryRequest>()
        .add_message::<ToolQueryHandled>()
        .add_message::<ServiceRequest>()
        .add_systems(Update, (open_command_bar, notify).in_set(ToolDispatchSet))
        .add_systems(
            Update,
            answer_command_queries
                .in_set(ToolQueryRouteSet)
                .after(ServiceMessageSet),
        );
    }
}

fn answer_command_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    commands: Query<&crate::CommandDefinition>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        if request.query.id != AgentListCommands::ID {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        if let Err(error) = serde_json::from_slice::<AgentListCommands>(&request.query.body) {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                request_id: request.request_id,
                message: error.to_string(),
            }));
            continue;
        }
        let mut tools = commands
            .iter()
            .filter_map(crate::CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandsResult {
            request_id: request.request_id,
            result: Ok(tools),
        }));
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCommandBarArgs {
    mode: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotifyArgs {
    title: Option<String>,
    body: Option<String>,
}

fn open_command_bar(
    mut commands: Commands,
    requests: Query<(Entity, &OpenCommandBarArgs), AddedTool<OpenCommandBarArgs>>,
) {
    for (entity, args) in &requests {
        let result = match args.mode.as_deref().unwrap_or("default") {
            "default" => Ok("browser_open_command_bar"),
            "commands" => Ok("browser_open_commands"),
            "path" => Ok("browser_open_path_bar"),
            other => Err(format!("unknown command bar mode: {other}")),
        };
        let command = result.and_then(|id| {
            AgentRequest::encode(&AgentInvokeCommand {
                id: id.to_string(),
                args: JsonValue::Object(Vec::new()),
            })
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn notify(mut commands: Commands, requests: Query<(Entity, &NotifyArgs), AddedTool<NotifyArgs>>) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(AgentRequest::encode(&AgentNotify {
                title: args.title.clone(),
                body: args.body.clone(),
            })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::JsonArguments;
    use vmux_tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    struct CommandToolFixture;

    impl CommandToolFixture {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(CommandToolPlugin);
            app.update();
            app
        }

        fn definitions() -> Vec<String> {
            let mut app = Self::app();
            let request = app.world_mut().spawn(ToolCatalogRequest).id();
            app.update();
            app.world_mut()
                .entity_mut(request)
                .take::<ToolCatalog>()
                .unwrap()
                .0
                .into_iter()
                .map(|definition| definition.name)
                .collect()
        }

        fn dispatch(name: &str, arguments: serde_json::Value) -> Result<AgentRequest, String> {
            let mut app = Self::app();
            let request = app
                .world_mut()
                .spawn((
                    Name::new(name.to_string()),
                    JsonArguments(arguments),
                    ToolInvocation,
                ))
                .id();
            app.update();
            if let Some(command) = app.world_mut().entity_mut(request).take::<ToolCommand>() {
                return command.0;
            }
            let error = app
                .world_mut()
                .entity_mut(request)
                .take::<ToolDispatchError>()
                .unwrap();
            Err(error.message().to_string())
        }
    }

    #[test]
    fn manifest_registers_command_tools() {
        assert_eq!(
            CommandToolFixture::definitions(),
            ["open_command_bar", "notify"]
        );
    }

    #[test]
    fn open_command_bar_dispatches_each_mode() {
        for (mode, id) in [
            ("default", "browser_open_command_bar"),
            ("commands", "browser_open_commands"),
            ("path", "browser_open_path_bar"),
        ] {
            assert_eq!(
                CommandToolFixture::dispatch("open_command_bar", serde_json::json!({"mode": mode})),
                AgentRequest::encode(&AgentInvokeCommand {
                    id: id.to_string(),
                    args: JsonValue::Object(Vec::new()),
                })
            );
        }
        assert!(
            CommandToolFixture::dispatch("open_command_bar", serde_json::json!({"mode": "other"}))
                .is_err()
        );
    }

    #[test]
    fn notify_dispatches_optional_content() {
        assert_eq!(
            CommandToolFixture::dispatch(
                "notify",
                serde_json::json!({"title": "done", "body": "built X"}),
            ),
            AgentRequest::encode(&AgentNotify {
                title: Some("done".to_string()),
                body: Some("built X".to_string()),
            })
        );
        assert_eq!(
            CommandToolFixture::dispatch("notify", serde_json::json!({})),
            AgentRequest::encode(&AgentNotify {
                title: None,
                body: None,
            })
        );
    }
}
