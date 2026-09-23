use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, AgentSpaceCommand};

pub(super) struct SpaceToolPlugin;

impl Plugin for SpaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Space))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSpaceArgs {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameSpaceArgs {
    space_id: String,
    name: String,
}

impl RenameSpaceArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if self.space_id.trim().is_empty() {
            return Err("rename_space.space_id is empty".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("rename_space.name is empty".to_string());
        }
        Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Rename {
            space_id: self.space_id,
            name: self.name,
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteSpaceArgs {
    space_id: String,
}

impl DeleteSpaceArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if self.space_id.trim().is_empty() {
            return Err("delete_space.space_id is empty".to_string());
        }
        Ok(AgentCommand::SpaceCommand(AgentSpaceCommand::Delete {
            space_id: self.space_id,
        }))
    }
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<SpaceTool>::from_ron(include_str!("space.ron"));
    tools.spawn_manifest(manifest);
}

fn list_spaces(mut commands: Commands, calls: ToolCalls<SpaceTool>) {
    for (request, call, _) in calls.matching(SpaceTool::ListSpaces) {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::ListSpaces)),
        );
    }
}

fn create(mut commands: Commands, calls: ToolCalls<SpaceTool>) {
    for (request, call, _) in calls.matching(SpaceTool::CreateSpace) {
        let result = call
            .parse::<CreateSpaceArgs>("create_space")
            .map(|args| {
                AgentCommand::SpaceCommand(AgentSpaceCommand::Create {
                    name: args.name.filter(|name| !name.trim().is_empty()),
                })
            })
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, result);
    }
}

fn rename(mut commands: Commands, calls: ToolCalls<SpaceTool>) {
    for (request, call, _) in calls.matching(SpaceTool::RenameSpace) {
        let target = call
            .parse::<RenameSpaceArgs>("rename_space")
            .and_then(RenameSpaceArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn delete(mut commands: Commands, calls: ToolCalls<SpaceTool>) {
    for (request, call, _) in calls.matching(SpaceTool::DeleteSpace) {
        let target = call
            .parse::<DeleteSpaceArgs>("delete_space")
            .and_then(DeleteSpaceArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}
