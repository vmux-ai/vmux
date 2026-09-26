use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{
    AgentCommand, AgentQuery, AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename,
};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

pub struct SpaceToolPlugin;

impl Plugin for SpaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::new(include_str!("tool.ron")))
            .register_tool::<ListSpacesArgs>("list_spaces")
            .register_tool::<CreateSpaceArgs>("create_space")
            .register_tool::<RenameSpaceArgs>("rename_space")
            .register_tool::<DeleteSpaceArgs>("delete_space")
            .add_systems(
                Update,
                (list_spaces, create, rename, delete).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListSpacesArgs {}

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

fn list_spaces(mut commands: Commands, calls: Query<Entity, AddedTool<ListSpacesArgs>>) {
    for request in &calls {
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
            .insert(ToolCommand(Ok(AgentCommand::SpaceCreate(
                AgentSpaceCreate {
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
            Ok(AgentCommand::SpaceRename(AgentSpaceRename {
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
            Ok(AgentCommand::SpaceDelete(AgentSpaceDelete {
                space_id: space_id.clone(),
            }))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
