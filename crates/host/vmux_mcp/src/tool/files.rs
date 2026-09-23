use super::{
    ProtocolTool, ToolCall, ToolCalls, ToolDispatchSet, ToolExecution, ToolManifest,
    ToolRegistrationSet, ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};

pub(super) struct FileToolPlugin;

impl Plugin for FileToolPlugin {
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadFileArgs {
    path: String,
    offset: Option<std::num::NonZeroU32>,
    limit: Option<usize>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GrepArgs {
    query: String,
    path: Option<String>,
}

fn read_file(mut commands: Commands, calls: ToolCalls<FileTool>) {
    for (request, call, _) in calls.matching(FileTool::ReadFile) {
        let result = call
            .parse::<ReadFileArgs>("read_file")
            .and_then(ToolCall::arguments)
            .map(|arguments| ToolExecution::Protocol {
                tool: ProtocolTool::ReadFile,
                arguments,
                anchor: call.anchor,
            });
        call.finish(request, &mut commands, result);
    }
}

fn grep(mut commands: Commands, calls: ToolCalls<FileTool>) {
    for (request, call, _) in calls.matching(FileTool::Grep) {
        let result = call
            .parse::<GrepArgs>("grep")
            .and_then(ToolCall::arguments)
            .map(|arguments| ToolExecution::Protocol {
                tool: ProtocolTool::Grep,
                arguments,
                anchor: call.anchor,
            });
        call.finish(request, &mut commands, result);
    }
}
