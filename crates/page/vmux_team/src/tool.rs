use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::AgentCommand;
use vmux_core::JsonArguments;
use vmux_mcp::tool::{
    AddedTool, McpToolPlugin, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolRequestSet,
};

pub struct TeamToolPlugin;

impl Plugin for TeamToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<TeamTool>::new(include_str!("tool.ron")))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(Update, rename_profile.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TeamTool {
    RenameProfile,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameProfileArgs {
    name: String,
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments), AddedTool<TeamTool>>,
) {
    for (entity, name, arguments) in &calls {
        match arguments.parse::<RenameProfileArgs>(name.as_str()) {
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
}

fn rename_profile(
    mut commands: Commands,
    requests: Query<(Entity, &RenameProfileArgs), AddedTool<RenameProfileArgs>>,
) {
    for (entity, args) in &requests {
        let name = args.name.trim();
        let command = if name.is_empty() {
            Err("rename_profile.name is empty".to_string())
        } else {
            Ok(AgentCommand::RenameProfile {
                name: name.to_string(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
