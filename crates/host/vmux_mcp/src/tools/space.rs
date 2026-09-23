use super::{
    DispatchTarget, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet, ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::AgentQuery;

pub(super) struct SpaceToolPlugin;

impl Plugin for SpaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Space))
            .add_systems(Update, list_spaces.in_set(ToolDispatchSet));
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SpaceTool {
    ListSpaces,
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<SpaceTool>::from_ron(include_str!("space.ron"));
    tools.spawn_manifest(manifest);
}

fn list_spaces(mut commands: Commands, calls: ToolCalls<SpaceTool>) {
    for (request, call, _) in calls.matching(SpaceTool::ListSpaces) {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::ListSpaces)),
        );
    }
}
