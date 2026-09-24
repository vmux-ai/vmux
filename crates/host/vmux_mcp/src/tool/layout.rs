use super::{
    DispatchTarget, NextToolOrder, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest,
    ToolRegistrationSet, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue, layout};

pub(super) struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Layout))
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

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<LayoutTool>::from_ron(include_str!("layout.ron"))
        .spawn(&mut commands, &mut next_order);
}

fn parse(mut commands: Commands, calls: ToolCalls<LayoutTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            LayoutTool::ReadLayout => {}
            LayoutTool::UpdateLayout => call.parse_into::<UpdateLayoutArgs>(request, &mut commands),
            LayoutTool::SelectTab => call.parse_into::<SelectTabArgs>(request, &mut commands),
        }
    }
}

fn read_layout(mut commands: Commands, calls: ToolCalls<LayoutTool>) {
    for (request, call, _) in calls.matching(LayoutTool::ReadLayout) {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::ReadLayout {
                anchor: call.anchor,
            })),
        );
    }
}

fn update_layout(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<UpdateLayoutArgs>),
        Added<ParsedToolCall<UpdateLayoutArgs>>,
    >,
) {
    for (entity, request) in &requests {
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
                layout: request.args().0.clone(),
            })),
        );
    }
}

fn select_tab(
    mut commands: Commands,
    requests: Query<(Entity, &ParsedToolCall<SelectTabArgs>), Added<ParsedToolCall<SelectTabArgs>>>,
) {
    for (entity, request) in &requests {
        let index = request.args().index;
        let target = if (1..=8).contains(&index) {
            Ok(DispatchTarget::Command(AgentCommand::InvokeCommand {
                id: format!("tab_select_{index}"),
                args: JsonValue::Object(Vec::new()),
            }))
        } else {
            Err(format!(
                "select_tab.index must be between 1 and 8, got {index}"
            ))
        };
        request.finish(entity, &mut commands, target);
    }
}
