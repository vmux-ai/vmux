use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentCommand, JsonValue};
use vmux_core::JsonArguments;
use vmux_mcp::tool::{
    AddedTool, McpToolPlugin, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolRequestSet,
};

pub struct CommandToolPlugin;

impl Plugin for CommandToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<CommandTool>::new(include_str!("tool.ron")))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(Update, (open_command_bar, notify).in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CommandTool {
    OpenCommandBar,
    Notify,
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

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &CommandTool), AddedTool<CommandTool>>,
) {
    for (entity, name, arguments, tool) in &calls {
        match tool {
            CommandTool::OpenCommandBar => {
                match arguments.parse::<OpenCommandBarArgs>(name.as_str()) {
                    Ok(args) => {
                        commands.entity(entity).insert(args);
                    }
                    Err(message) => {
                        commands
                            .entity(entity)
                            .insert(ToolDispatchError::new(message));
                    }
                }
            }
            CommandTool::Notify => match arguments.parse::<NotifyArgs>(name.as_str()) {
                Ok(args) => {
                    commands.entity(entity).insert(args);
                }
                Err(message) => {
                    commands
                        .entity(entity)
                        .insert(ToolDispatchError::new(message));
                }
            },
        }
    }
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
        let command = result.map(|id| AgentCommand::InvokeCommand {
            id: id.to_string(),
            args: JsonValue::Object(Vec::new()),
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn notify(mut commands: Commands, requests: Query<(Entity, &NotifyArgs), AddedTool<NotifyArgs>>) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::Notify {
                title: args.title.clone(),
                body: args.body.clone(),
            })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_mcp::tool::{ToolCatalog, ToolCatalogRequest, ToolDispatchError, ToolInvocation};

    impl CommandTool {
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

        fn dispatch(name: &str, arguments: serde_json::Value) -> Result<AgentCommand, String> {
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
        assert_eq!(CommandTool::definitions(), ["open_command_bar", "notify"]);
    }

    #[test]
    fn open_command_bar_dispatches_each_mode() {
        for (mode, id) in [
            ("default", "browser_open_command_bar"),
            ("commands", "browser_open_commands"),
            ("path", "browser_open_path_bar"),
        ] {
            assert_eq!(
                CommandTool::dispatch("open_command_bar", serde_json::json!({"mode": mode})),
                Ok(AgentCommand::InvokeCommand {
                    id: id.to_string(),
                    args: JsonValue::Object(Vec::new()),
                })
            );
        }
        assert!(
            CommandTool::dispatch("open_command_bar", serde_json::json!({"mode": "other"}))
                .is_err()
        );
    }

    #[test]
    fn notify_dispatches_optional_content() {
        assert_eq!(
            CommandTool::dispatch(
                "notify",
                serde_json::json!({"title": "done", "body": "built X"}),
            ),
            Ok(AgentCommand::Notify {
                title: Some("done".to_string()),
                body: Some("built X".to_string()),
            })
        );
        assert_eq!(
            CommandTool::dispatch("notify", serde_json::json!({})),
            Ok(AgentCommand::Notify {
                title: None,
                body: None,
            })
        );
    }
}
