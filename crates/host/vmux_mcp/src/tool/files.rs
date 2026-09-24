use super::{
    NextToolOrder, ProtocolTool, RegisterTools, ToolCall, ToolCalls, ToolDispatchSet,
    ToolExecution, ToolManifest, ToolOutcome, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

pub(super) struct FileToolPlugin;

impl Plugin for FileToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(RegisterTools))
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
    requests: Query<(Entity, &ToolCall, &ReadFileArgs), Added<ReadFileArgs>>,
) {
    for (entity, call, args) in &requests {
        let result = serde_json::to_value(args)
            .map_err(|error| format!("MCP tool arguments must serialize: {error}"))
            .map(|arguments| ToolExecution::Protocol {
                tool: ProtocolTool::ReadFile,
                arguments,
                anchor: call.anchor,
            });
        commands.entity(entity).insert(ToolOutcome(result));
    }
}

fn grep(mut commands: Commands, requests: Query<(Entity, &ToolCall, &GrepArgs), Added<GrepArgs>>) {
    for (entity, call, args) in &requests {
        let result = serde_json::to_value(args)
            .map_err(|error| format!("MCP tool arguments must serialize: {error}"))
            .map(|arguments| ToolExecution::Protocol {
                tool: ProtocolTool::Grep,
                arguments,
                anchor: call.anchor,
            });
        commands.entity(entity).insert(ToolOutcome(result));
    }
}
