use super::{
    ProtocolTool, ToolCalls, ToolDispatchSet, ToolExecution, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};

pub(super) struct FileToolsPlugin;

impl Plugin for FileToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Files))
            .add_systems(Update, (read_file, grep).in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FileTool {
    ReadFile,
    Grep,
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<FileTool>::from_ron(include_str!("files.ron"));
    tools.spawn_manifest(manifest);
}

fn read_file(mut commands: Commands, calls: ToolCalls<FileTool>) {
    for (request, call, _) in calls.matching(FileTool::ReadFile) {
        call.finish(
            request,
            &mut commands,
            Ok(ToolExecution::Protocol {
                tool: ProtocolTool::ReadFile,
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            }),
        );
    }
}

fn grep(mut commands: Commands, calls: ToolCalls<FileTool>) {
    for (request, call, _) in calls.matching(FileTool::Grep) {
        call.finish(
            request,
            &mut commands,
            Ok(ToolExecution::Protocol {
                tool: ProtocolTool::Grep,
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            }),
        );
    }
}
