use super::{
    McpToolPlugin, ProtocolTool, ToolCall, ToolCalls, ToolDispatchResult, ToolDispatchSet,
    ToolExecution, ToolOutcome, ToolRequestSet,
};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

pub(super) struct FileToolPlugin;

impl Plugin for FileToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<FileTool>::new(include_str!("files.ron")))
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
        let parsed = match tool {
            FileTool::ReadFile => call.parse::<ReadFileArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            FileTool::Grep => call.parse::<GrepArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchResult(Err(message)));
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
