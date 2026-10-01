use bevy::prelude::*;
#[cfg(test)]
use bevy_cef::prelude::HostWindow;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{AgentQueryResult, AgentRequest, AgentSpace, ClientMessage};
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_ecs::{Active, Order, ProcessAnchor};
use vmux_layout::space::{Space, SpaceId};
use vmux_layout::window::{FocusedWindow, WindowHierarchy};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolQuery, ToolQueryHandled,
    ToolQueryRequest, ToolQueryRouteSet,
};

use super::{
    AgentChooseWorkspace, AgentChooseWorkspaceAtPath, AgentCreateWorktreeOnBranch, AgentListSpaces,
    AgentPrepareWorktree, AgentSpaceCreate, AgentSpaceDelete, AgentSpaceRename,
};
use crate::model::SpaceRecord;

pub struct SpaceToolPlugin;

impl Plugin for SpaceToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .bind_tool::<ListSpacesArgs>()
            .bind_tool::<CreateSpaceArgs>()
            .bind_tool::<RenameSpaceArgs>()
            .bind_tool::<DeleteSpaceArgs>()
            .bind_tool::<SelectProjectArgs>()
            .bind_tool::<CreateWorktreeArgs>()
            .add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_systems(
                Update,
                (
                    list_spaces,
                    create,
                    rename,
                    delete,
                    select_project,
                    create_worktree,
                )
                    .in_set(ToolDispatchSet),
            )
            .add_systems(
                Update,
                answer_queries
                    .in_set(ToolQueryRouteSet)
                    .after(ServiceMessageSet),
            );
    }
}

fn answer_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    spaces: Query<(Entity, &SpaceId, &Name, Has<Active>, Option<&Order>), With<Space>>,
    focused_window: FocusedWindow,
    hierarchy: WindowHierarchy,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        if request.query.id != AgentListSpaces::ID {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        if let Err(error) = serde_json::from_slice::<AgentListSpaces>(&request.query.body) {
            service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                AgentQueryResult::text(request.request_id, Err(error.to_string())),
            )));
            continue;
        }
        let mut rows: Vec<(u32, AgentSpace)> = Vec::new();
        for (entity, id, name, is_active, order) in &spaces {
            let local = focused_window
                .entity()
                .is_some_and(|focused| hierarchy.get(entity) == Some(focused));
            let order = order.map(|order| order.0).unwrap_or(u32::MAX);
            if let Some((existing_order, row)) =
                rows.iter_mut().find(|(_, existing)| existing.id == id.0)
            {
                *existing_order = (*existing_order).min(order);
                if local {
                    row.is_active = is_active;
                }
                continue;
            }
            rows.push((
                order,
                AgentSpace {
                    id: id.0.clone(),
                    name: name.to_string(),
                    profile: SpaceRecord::current_profile_name(),
                    is_active: local && is_active,
                },
            ));
        }
        rows.sort_by_key(|(order, _)| *order);
        let rows = rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>();
        let result = serde_json::to_string(&rows).map_err(|error| error.to_string());
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.request_id, result),
        )));
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListSpacesArgs {}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSpaceArgs {
    name: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameSpaceArgs {
    space_id: String,
    name: String,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteSpaceArgs {
    space_id: String,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectProjectArgs {
    path: Option<String>,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateWorktreeArgs {
    branch: Option<String>,
    path: Option<String>,
    task: Option<String>,
    #[serde(default)]
    create: bool,
}

fn list_spaces(mut commands: Commands, calls: Query<Entity, AddedTool<ListSpacesArgs>>) {
    for request in &calls {
        commands
            .entity(request)
            .insert(ToolQuery(AgentRequest::encode(&AgentListSpaces)));
    }
}

fn create(
    mut commands: Commands,
    requests: Query<(Entity, &CreateSpaceArgs), AddedTool<CreateSpaceArgs>>,
) {
    for (entity, args) in &requests {
        commands
            .entity(entity)
            .insert(ToolCommand(AgentRequest::encode(&AgentSpaceCreate {
                name: args.name.clone().filter(|name| !name.trim().is_empty()),
            })));
    }
}

fn rename(
    mut commands: Commands,
    requests: Query<(Entity, &RenameSpaceArgs), AddedTool<RenameSpaceArgs>>,
) {
    for (entity, args) in &requests {
        let command = if args.space_id.trim().is_empty() {
            Err("rename_space.space_id is empty".to_string())
        } else if args.name.trim().is_empty() {
            Err("rename_space.name is empty".to_string())
        } else {
            AgentRequest::encode(&AgentSpaceRename {
                space_id: args.space_id.clone(),
                name: args.name.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn delete(
    mut commands: Commands,
    requests: Query<(Entity, &DeleteSpaceArgs), AddedTool<DeleteSpaceArgs>>,
) {
    for (entity, args) in &requests {
        let space_id = &args.space_id;
        let command = if space_id.trim().is_empty() {
            Err("delete_space.space_id is empty".to_string())
        } else {
            AgentRequest::encode(&AgentSpaceDelete {
                space_id: space_id.clone(),
            })
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn select_project(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &SelectProjectArgs),
        Added<SelectProjectArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| match args
            .path
            .clone()
            .and_then(Trimmed::into_option)
        {
            Some(path) => AgentRequest::encode(&AgentChooseWorkspaceAtPath { anchor, path }),
            None => AgentRequest::encode(&AgentChooseWorkspace { anchor }),
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn create_worktree(
    mut commands: Commands,
    requests: Query<
        (Entity, &Name, Option<&ProcessAnchor>, &CreateWorktreeArgs),
        Added<CreateWorktreeArgs>,
    >,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            if let Some(branch) = args.branch.clone().and_then(Trimmed::into_option) {
                AgentRequest::encode(&AgentCreateWorktreeOnBranch {
                    anchor,
                    branch,
                    project: None,
                })
            } else {
                AgentRequest::encode(&AgentPrepareWorktree {
                    anchor,
                    path: args.path.clone().and_then(Trimmed::into_option),
                    task: args.task.clone().and_then(Trimmed::into_option),
                    create: args.create,
                })
            }
        });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

struct Trimmed;

impl Trimmed {
    fn into_option(value: String) -> Option<String> {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_spaces_are_global_but_active_state_is_window_local() {
        let mut app = App::new();
        app.add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_systems(Update, answer_queries);
        let first_window = app.world_mut().spawn(Window::default()).id();
        let second_window = app.world_mut().spawn((Window::default(), Active)).id();
        let first_root = app.world_mut().spawn(HostWindow(first_window)).id();
        let second_root = app.world_mut().spawn(HostWindow(second_window)).id();
        app.world_mut().spawn((
            Space,
            SpaceId("shared".to_string()),
            Name::new("shared"),
            Active,
            ChildOf(first_root),
        ));
        app.world_mut().spawn((
            Space,
            SpaceId("shared".to_string()),
            Name::new("shared"),
            ChildOf(second_root),
        ));
        app.world_mut().spawn((
            Space,
            SpaceId("local".to_string()),
            Name::new("local"),
            Active,
            ChildOf(second_root),
        ));
        let request_id = vmux_api::protocol::AgentRequestId([1; 16]);
        app.world_mut().write_message(ToolQueryRequest {
            request_id,
            query: AgentRequest::encode(&AgentListSpaces).unwrap(),
        });

        app.update();

        let requests = app.world().resource::<Messages<ServiceRequest>>();
        let mut cursor = requests.get_cursor();
        let response = cursor.read(requests).next().expect("space response");
        let ClientMessage::AgentQueryResult(result) = &response.0 else {
            panic!("expected spaces result");
        };
        let rows = serde_json::from_str::<Vec<AgentSpace>>(&result.content).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            !rows
                .iter()
                .find(|row| row.id == "shared")
                .unwrap()
                .is_active
        );
        assert!(rows.iter().find(|row| row.id == "local").unwrap().is_active);
    }
}
