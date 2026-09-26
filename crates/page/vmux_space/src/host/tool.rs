use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentCommand, AgentQuery, AgentSpaceCommand};
use vmux_core::JsonArguments;
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
    ToolRequestSet,
};

pub struct SpaceToolPlugin;

impl Plugin for SpaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::<SpaceTool>::new(include_str!(
            "tool.ron"
        )))
        .add_systems(Update, parse.in_set(ToolRequestSet))
        .add_systems(
            Update,
            (list_spaces, create, rename, delete).in_set(ToolDispatchSet),
        );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SpaceTool {
    ListSpaces,
    CreateSpace,
    RenameSpace,
    DeleteSpace,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSpaceArgs {
    name: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameSpaceArgs {
    space_id: String,
    name: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteSpaceArgs {
    space_id: String,
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &SpaceTool), AddedTool<SpaceTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        let parsed = match tool {
            SpaceTool::ListSpaces => continue,
            SpaceTool::CreateSpace => {
                arguments
                    .parse::<CreateSpaceArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    })
            }
            SpaceTool::RenameSpace => {
                arguments
                    .parse::<RenameSpaceArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    })
            }
            SpaceTool::DeleteSpace => {
                arguments
                    .parse::<DeleteSpaceArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    })
            }
        };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn list_spaces(mut commands: Commands, calls: Query<(Entity, &SpaceTool), AddedTool<SpaceTool>>) {
    for (request, tool) in &calls {
        if *tool != SpaceTool::ListSpaces {
            continue;
        }
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::ListSpaces)));
    }
}

fn create(
    mut commands: Commands,
    requests: Query<(Entity, &CreateSpaceArgs), AddedTool<CreateSpaceArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::SpaceCommand(
                AgentSpaceCommand::Create {
                    name: args.name.clone().filter(|name| !name.trim().is_empty()),
                },
            ))));
    }
}

fn rename(
    mut commands: Commands,
    requests: Query<(Entity, &RenameSpaceArgs), AddedTool<RenameSpaceArgs>>,
) {
    for (entity, args) in &requests {
        let command = if args.space_id.trim().is_empty() {
            Err("rename_space.space_id is empty".to_string())
        } else if args.name.trim().is_empty() {
            Err("rename_space.name is empty".to_string())
        } else {
            Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Rename {
                space_id: args.space_id.clone(),
                name: args.name.clone(),
            }))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn delete(
    mut commands: Commands,
    requests: Query<(Entity, &DeleteSpaceArgs), AddedTool<DeleteSpaceArgs>>,
) {
    for (entity, args) in &requests {
        let space_id = &args.space_id;
        let command = if space_id.trim().is_empty() {
            Err("delete_space.space_id is empty".to_string())
        } else {
            Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Delete {
                space_id: space_id.clone(),
            }))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
