use super::{
    DispatchTarget, NextToolOrder, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest,
    ToolRegistrationSet, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentCommand, AgentQuery, AgentSpaceCommand};

pub(super) struct SpaceToolPlugin;

impl Plugin for SpaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Space))
            .add_systems(Update, parse.in_set(ToolRequestSet))
            .add_systems(
                Update,
                (list_spaces, create, rename, delete).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SpaceTool {
    ListSpaces,
    CreateSpace,
    RenameSpace,
    DeleteSpace,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSpaceArgs {
    name: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameSpaceArgs {
    space_id: String,
    name: String,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteSpaceArgs {
    space_id: String,
}

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<SpaceTool>::from_ron(include_str!("space.ron"))
        .spawn(&mut commands, &mut next_order);
}

fn parse(mut commands: Commands, calls: ToolCalls<SpaceTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            SpaceTool::ListSpaces => {}
            SpaceTool::CreateSpace => call.parse_into::<CreateSpaceArgs>(request, &mut commands),
            SpaceTool::RenameSpace => call.parse_into::<RenameSpaceArgs>(request, &mut commands),
            SpaceTool::DeleteSpace => call.parse_into::<DeleteSpaceArgs>(request, &mut commands),
        }
    }
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

fn create(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<CreateSpaceArgs>),
        Added<ParsedToolCall<CreateSpaceArgs>>,
    >,
) {
    for (entity, request) in &requests {
        request.finish(
            entity,
            &mut commands,
            Ok(DispatchTarget::Command(AgentCommand::SpaceCommand(
                AgentSpaceCommand::Create {
                    name: request
                        .args()
                        .name
                        .clone()
                        .filter(|name| !name.trim().is_empty()),
                },
            ))),
        );
    }
}

fn rename(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<RenameSpaceArgs>),
        Added<ParsedToolCall<RenameSpaceArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let args = request.args();
        let target = if args.space_id.trim().is_empty() {
            Err("rename_space.space_id is empty".to_string())
        } else if args.name.trim().is_empty() {
            Err("rename_space.name is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(AgentCommand::SpaceCommand(
                AgentSpaceCommand::Rename {
                    space_id: args.space_id.clone(),
                    name: args.name.clone(),
                },
            )))
        };
        request.finish(entity, &mut commands, target);
    }
}

fn delete(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<DeleteSpaceArgs>),
        Added<ParsedToolCall<DeleteSpaceArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let space_id = &request.args().space_id;
        let target = if space_id.trim().is_empty() {
            Err("delete_space.space_id is empty".to_string())
        } else {
            Ok(DispatchTarget::Command(AgentCommand::SpaceCommand(
                AgentSpaceCommand::Delete {
                    space_id: space_id.clone(),
                },
            )))
        };
        request.finish(entity, &mut commands, target);
    }
}
