use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, JsonValue, layout};

pub(super) struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Layout))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectTabArgs {
    index: u8,
}

impl SelectTabArgs {
    fn command(self) -> Result<AgentCommand, String> {
        if !(1..=8).contains(&self.index) {
            return Err(format!(
                "select_tab.index must be between 1 and 8, got {}",
                self.index
            ));
        }
        Ok(AgentCommand::InvokeCommand {
            id: format!("tab_select_{}", self.index),
            args: JsonValue::Object(Vec::new()),
        })
    }
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
    for (request, call, _) in calls.matching(LayoutTool::UpdateLayout) {
        let target = call
            .parse::<layout::LayoutSnapshot>("update_layout")
            .map(|layout| DispatchTarget::Command(AgentCommand::UpdateLayout { layout }));
        call.finish_dispatch(request, &mut commands, target);
    }
}

fn select_tab(mut commands: Commands, calls: ToolCalls<LayoutTool>) {
    for (request, call, _) in calls.matching(LayoutTool::SelectTab) {
        let target = call
            .parse::<SelectTabArgs>("select_tab")
            .and_then(SelectTabArgs::command)
            .map(DispatchTarget::Command);
        call.finish_dispatch(request, &mut commands, target);
    }
}
