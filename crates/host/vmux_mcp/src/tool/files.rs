use super::{
    McpToolPlugin, ToolCall, ToolCalls, ToolDispatchError, ToolDispatchResult, ToolDispatchSet,
    ToolRequestSet,
};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::ProcessId;

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

#[derive(Component)]
pub(crate) struct ReadFileExecution {
    pub(crate) path: String,
    pub(crate) offset: Option<std::num::NonZeroU32>,
    pub(crate) limit: Option<usize>,
    pub(crate) anchor: Option<ProcessId>,
}

#[derive(Component)]
pub(crate) struct GrepExecution {
    pub(crate) query: String,
    pub(crate) path: Option<String>,
    pub(crate) anchor: Option<ProcessId>,
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
    protocol_requests: Query<(), With<crate::protocol_runtime::McpRequest>>,
) {
    for (entity, call, args) in &requests {
        if protocol_requests.contains(entity) {
            commands.entity(entity).insert(ReadFileExecution {
                path: args.path.clone(),
                offset: args.offset,
                limit: args.limit,
                anchor: call.anchor,
            });
        } else {
            commands.entity(entity).insert(ToolDispatchError(format!(
                "tool {} requires MCP protocol context",
                call.name
            )));
        }
    }
}

fn grep(
    mut commands: Commands,
    requests: Query<(Entity, &ToolCall, &GrepArgs), Added<GrepArgs>>,
    protocol_requests: Query<(), With<crate::protocol_runtime::McpRequest>>,
) {
    for (entity, call, args) in &requests {
        if protocol_requests.contains(entity) {
            commands.entity(entity).insert(GrepExecution {
                query: args.query.clone(),
                path: args.path.clone(),
                anchor: call.anchor,
            });
        } else {
            commands.entity(entity).insert(ToolDispatchError(format!(
                "tool {} requires MCP protocol context",
                call.name
            )));
        }
    }
}
