use super::{
    NextToolOrder, ParsedToolCall, ProtocolTool, ToolCalls, ToolDispatchSet, ToolExecution,
    ToolManifest, ToolRegistrationSet, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

pub(super) struct FileToolPlugin;

impl Plugin for FileToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Files))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(Update, (read_file, grep).in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FileTool {
    ReadFile,
    Grep,
}

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<FileTool>::from_ron(include_str!("files.ron"))
        .spawn(&mut commands, &mut next_order);
}

#[derive(Component, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadFileArgs {
    path: String,
    offset: Option<std::num::NonZeroU32>,
    limit: Option<usize>,
}

#[derive(Component, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GrepArgs {
    query: String,
    path: Option<String>,
}

fn parse(mut commands: Commands, calls: ToolCalls<FileTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            FileTool::ReadFile => call.parse_into::<ReadFileArgs>(request, &mut commands),
            FileTool::Grep => call.parse_into::<GrepArgs>(request, &mut commands),
        }
    }
}

fn read_file(
    mut commands: Commands,
    requests: Query<(Entity, &ParsedToolCall<ReadFileArgs>), Added<ParsedToolCall<ReadFileArgs>>>,
) {
    for (entity, request) in &requests {
        let result = request
            .serialized_args()
            .map(|arguments| ToolExecution::Protocol {
                tool: ProtocolTool::ReadFile,
                arguments,
                anchor: request.anchor(),
            });
        request.finish_execution(entity, &mut commands, result);
    }
}

fn grep(
    mut commands: Commands,
    requests: Query<(Entity, &ParsedToolCall<GrepArgs>), Added<ParsedToolCall<GrepArgs>>>,
) {
    for (entity, request) in &requests {
        let result = request
            .serialized_args()
            .map(|arguments| ToolExecution::Protocol {
                tool: ProtocolTool::Grep,
                arguments,
                anchor: request.anchor(),
            });
        request.finish_execution(entity, &mut commands, result);
    }
}
