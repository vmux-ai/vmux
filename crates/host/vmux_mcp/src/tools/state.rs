use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs, World};
use vmux_client::protocol::{AgentCommand, AgentQuery};

pub(super) struct StateToolsPlugin;

impl Plugin for StateToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::State))
            .add_systems(
                Update,
                (read_layout, update_layout, get_settings, list_spaces).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Component)]
struct ReadLayout;

#[derive(Component)]
struct UpdateLayout;

#[derive(Component)]
struct GetSettings;

#[derive(Component)]
struct ListSpaces;

fn register(world: &mut World) {
    let mut tools = ToolManifest::from_ron(include_str!("state.ron"));
    tools.system(world, "read_layout", ReadLayout);
    tools.system(world, "update_layout", UpdateLayout);
    tools.system(world, "get_settings", GetSettings);
    tools.system(world, "list_spaces", ListSpaces);
    tools.finish();
}

fn read_layout(mut commands: Commands, calls: ToolCalls<ReadLayout>) {
    for (request, call, _) in calls.iter() {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::ReadLayout {
                anchor: call.anchor,
            })),
        );
    }
}

fn update_layout(mut commands: Commands, calls: ToolCalls<UpdateLayout>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let layout = serde_json::from_value(call.arguments.clone())
            .map_err(|error| format!("update_layout: invalid layout payload: {error}"))?;
        Ok(DispatchTarget::Command(AgentCommand::UpdateLayout {
            layout,
        }))
    }

    for (request, call, _) in calls.iter() {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn get_settings(mut commands: Commands, calls: ToolCalls<GetSettings>) {
    for (request, call, _) in calls.iter() {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::GetSettings)),
        );
    }
}

fn list_spaces(mut commands: Commands, calls: ToolCalls<ListSpaces>) {
    for (request, call, _) in calls.iter() {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::ListSpaces)),
        );
    }
}
