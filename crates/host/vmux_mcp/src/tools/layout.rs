use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, layout};

pub(super) struct LayoutToolsPlugin;

impl Plugin for LayoutToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Layout))
            .add_systems(Update, (read_layout, update_layout).in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LayoutTool {
    ReadLayout,
    UpdateLayout,
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<LayoutTool>::from_ron(include_str!("layout.ron"));
    tools.spawn_manifest(manifest);
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

fn update_layout(mut commands: Commands, calls: ToolCalls<LayoutTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let layout = call.parse::<layout::LayoutSnapshot>("update_layout")?;
        Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
            layout,
        }))
    }

    for (request, call, _) in calls.matching(LayoutTool::UpdateLayout) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}
