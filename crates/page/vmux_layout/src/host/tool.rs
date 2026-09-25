use vmux_mcp::tool::{
    McpToolPlugin, ToolCall, ToolCalls, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolQuery,
    ToolRequestSet,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentCommand, AgentQuery, JsonValue, layout};

pub struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<LayoutTool>::new(include_str!("tool.ron")))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(
                Update,
                (read_layout, update_layout, select_tab).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LayoutTool {
    ReadLayout,
    UpdateLayout,
    SelectTab,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectTabArgs {
    index: u8,
}

#[derive(Component, Deserialize)]
#[serde(transparent)]
struct UpdateLayoutArgs(layout::LayoutSnapshot);

fn parse(mut commands: Commands, calls: ToolCalls<LayoutTool>) {
    for (request, call, tool) in calls.iter() {
        let parsed = match tool {
            LayoutTool::ReadLayout => continue,
            LayoutTool::UpdateLayout => call.parse::<UpdateLayoutArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            LayoutTool::SelectTab => call.parse::<SelectTabArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands.entity(request).insert(ToolDispatchError::new(message));
        }
    }
}

fn read_layout(mut commands: Commands, calls: ToolCalls<LayoutTool>) {
    for (request, call, _) in calls.matching(LayoutTool::ReadLayout) {
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::ReadLayout {
                anchor: call.anchor(),
            })));
    }
}

fn update_layout(
    mut commands: Commands,
    requests: Query<(Entity, &UpdateLayoutArgs), (With<ToolCall>, Added<UpdateLayoutArgs>)>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::UpdateLayout {
                layout: args.0.clone(),
            })));
    }
}

fn select_tab(
    mut commands: Commands,
    requests: Query<(Entity, &SelectTabArgs), (With<ToolCall>, Added<SelectTabArgs>)>,
) {
    for (entity, args) in &requests {
        let index = args.index;
        let command = if (1..=8).contains(&index) {
            Ok(AgentCommand::InvokeCommand {
                id: format!("tab_select_{index}"),
                args: JsonValue::Object(Vec::new()),
            })
        } else {
            Err(format!(
                "select_tab.index must be between 1 and 8, got {index}"
            ))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
