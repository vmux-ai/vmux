use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentListCommands, AgentNotify, AgentQueryResult, AgentRequest, ClientMessage, JsonValue,
};
use vmux_ecs::JsonArguments;
use vmux_ecs::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchSet, ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet,
    UnclaimedToolInvocation,
};

#[vmux_api::agent]
pub struct AgentInvokeCommand {
    pub id: String,
    #[rkyv(attr(allow(dead_code)))]
    pub args: JsonValue,
}

pub struct CommandToolPlugin;

impl Plugin for CommandToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_systems(Update, (open, notify, dispatch).in_set(ToolDispatchSet))
            .add_systems(
                Update,
                answer.in_set(ToolQueryRouteSet).after(ServiceMessageSet),
            );
    }
}

fn dispatch(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments), Added<UnclaimedToolInvocation>>,
) {
    for (entity, name, arguments) in &calls {
        commands
            .entity(entity)
            .insert(ToolCommand(AgentRequest::encode(&AgentInvokeCommand {
                id: name.as_str().to_string(),
                args: JsonValue::from(arguments.0.clone()),
            })));
    }
}

fn answer(
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
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(request.request_id, Err(error.to_string())),
            )));
            continue;
        }
        let mut tools = commands
            .iter()
            .filter_map(crate::CommandDefinition::agent_tool)
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let result = serde_json::to_string(&tools).map_err(|error| error.to_string());
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.request_id, result),
        )));
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenCommandBarTool {
    mode: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotifyTool {
    title: Option<String>,
    body: Option<String>,
}

fn open(
    mut commands: Commands,
    requests: Query<(Entity, &OpenCommandBarTool), AddedTool<OpenCommandBarTool>>,
) {
    for (entity, args) in &requests {
        let result = match args.mode.as_deref().unwrap_or("default") {
            "default" => Ok("command_bar_open"),
            "commands" => Ok("command_bar_open_commands"),
            "path" => Ok("command_bar_open_path"),
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

fn notify(mut commands: Commands, requests: Query<(Entity, &NotifyTool), AddedTool<NotifyTool>>) {
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
    use vmux_ecs::JsonArguments;
    use vmux_tool::{
        ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation, ToolRegistryPlugin,
    };

    struct CommandToolFixture;

    impl CommandToolFixture {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins((ToolRegistryPlugin, CommandToolPlugin));
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
            ("default", "command_bar_open"),
            ("commands", "command_bar_open_commands"),
            ("path", "command_bar_open_path"),
        ] {
            assert_eq!(
                CommandToolFixture::dispatch("open_command_bar", serde_json::json!({"mode": mode}),),
                AgentRequest::encode(&AgentInvokeCommand {
                    id: id.to_string(),
                    args: JsonValue::Object(Vec::new()),
                })
            );
        }
        assert!(
            CommandToolFixture::dispatch("open_command_bar", serde_json::json!({"mode": "other"}),)
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
