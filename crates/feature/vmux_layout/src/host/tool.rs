use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{
    AgentCommand, AgentInvokeCommand, AgentQuery, AgentUpdateLayout, JsonValue, layout,
};
use vmux_core::ProcessAnchor;
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

pub struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::new(include_str!("tool.ron")))
            .register_tool::<ReadLayoutArgs>("read_layout")
            .register_tool::<UpdateLayoutArgs>("update_layout")
            .register_tool::<SelectTabArgs>("select_tab")
            .add_systems(
                Update,
                (read_layout, update_layout, select_tab).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadLayoutArgs {}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectTabArgs {
    index: u8,
}

#[derive(Component, Deserialize)]
#[serde(transparent)]
struct UpdateLayoutArgs(layout::LayoutSnapshot);

fn read_layout(
    mut commands: Commands,
    calls: Query<(Entity, Option<&ProcessAnchor>), AddedTool<ReadLayoutArgs>>,
) {
    for (request, anchor) in &calls {
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
