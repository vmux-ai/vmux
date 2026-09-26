use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{
    AgentCommand, AgentInvokeCommand, AgentQuery, AgentUpdateLayout, JsonValue, layout,
};
use vmux_core::{JsonArguments, ProcessAnchor};
use vmux_tool::{
    AddedTool, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolKindManifestPlugin, ToolQuery,
    ToolRequestSet,
};

pub struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolKindManifestPlugin::<LayoutTool>::new(include_str!(
            "tool.ron"
        )))
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

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &LayoutTool), AddedTool<LayoutTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        let parsed = match tool {
            LayoutTool::ReadLayout => continue,
            LayoutTool::UpdateLayout => {
                arguments
                    .parse::<UpdateLayoutArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    })
            }
            LayoutTool::SelectTab => arguments.parse::<SelectTabArgs>(name.as_str()).map(|args| {
                commands.entity(request).insert(args);
            }),
        };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn read_layout(
    mut commands: Commands,
    calls: Query<(Entity, &LayoutTool, Option<&ProcessAnchor>), AddedTool<LayoutTool>>,
) {
    for (request, tool, anchor) in &calls {
        if *tool != LayoutTool::ReadLayout {
            continue;
        }
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::ReadLayout {
                anchor: anchor.map(|anchor| anchor.0),
            })));
    }
}

fn update_layout(
    mut commands: Commands,
    requests: Query<(Entity, &UpdateLayoutArgs), AddedTool<UpdateLayoutArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(Ok(AgentCommand::UpdateLayout(
                AgentUpdateLayout {
                    layout: args.0.clone(),
                },
            ))));
    }
}

fn select_tab(
    mut commands: Commands,
    requests: Query<(Entity, &SelectTabArgs), AddedTool<SelectTabArgs>>,
) {
    for (entity, args) in &requests {
        let index = args.index;
        let command = if (1..=8).contains(&index) {
            Ok(AgentCommand::InvokeCommand(AgentInvokeCommand {
                id: format!("tab_select_{index}"),
                args: JsonValue::Object(Vec::new()),
            }))
        } else {
            Err(format!(
                "select_tab.index must be between 1 and 8, got {index}"
            ))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
