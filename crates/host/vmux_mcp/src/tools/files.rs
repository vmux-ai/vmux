use super::{
    ProtocolTool, ToolCalls, ToolDispatchSet, ToolExecution, ToolManifest, ToolRegistrationSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs, World};

pub(super) struct FileToolsPlugin;

impl Plugin for FileToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Files))
            .add_systems(Update, (read_file, grep).in_set(ToolDispatchSet));
    }
}

#[derive(Component)]
struct ReadFile;

#[derive(Component)]
struct Grep;

fn register(world: &mut World) {
    let mut tools = ToolManifest::from_ron(include_str!("files.ron"));
    tools.system(world, "read_file", ReadFile);
    tools.system(world, "grep", Grep);
    tools.finish();
}

fn read_file(mut commands: Commands, calls: ToolCalls<ReadFile>) {
    for (request, call, _) in calls.iter() {
        call.finish(
            request,
            &mut commands,
            Ok(ToolExecution::Protocol {
                tool: ProtocolTool::ReadFile,
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            }),
        );
    }
}

fn grep(mut commands: Commands, calls: ToolCalls<Grep>) {
    for (request, call, _) in calls.iter() {
        call.finish(
            request,
            &mut commands,
            Ok(ToolExecution::Protocol {
                tool: ProtocolTool::Grep,
                arguments: call.arguments.clone(),
                anchor: call.anchor,
            }),
        );
    }
}
