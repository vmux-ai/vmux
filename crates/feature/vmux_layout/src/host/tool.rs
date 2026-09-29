use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentCommandResult, AgentInvokeCommand, AgentReadLayout, AgentRequest, AgentRequestId,
    AgentUpdateLayout, ClientMessage, JsonValue, layout,
};
use vmux_core::ProcessAnchor;
use vmux_core::service::{ServiceMessageSet, ServiceRequest};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
    ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet,
};

use crate::apply::{LayoutApplyResponse, LayoutSnapshotRequest, LayoutSnapshotResponse};

pub struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("../feature.ron"),
            "default",
        ))
        .register_tool::<ReadLayoutArgs>("read_layout")
        .register_tool::<UpdateLayoutArgs>("update_layout")
        .register_tool::<SelectTabArgs>("select_tab")
        .add_message::<ToolQueryRequest>()
        .add_message::<ToolQueryHandled>()
        .add_message::<ServiceRequest>()
        .add_message::<LayoutSnapshotRequest>()
        .add_message::<LayoutSnapshotResponse>()
        .add_message::<LayoutApplyResponse>()
        .add_systems(
            Update,
            (read_layout, update_layout, select_tab).in_set(ToolDispatchSet),
        )
        .add_systems(
            Update,
            route_layout_queries
                .in_set(ToolQueryRouteSet)
                .after(ServiceMessageSet),
        )
        .add_systems(
            Update,
            (
                forward_layout_apply_responses,
                forward_layout_snapshot_responses,
            ),
        );
    }
}

fn route_layout_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    mut snapshots: MessageWriter<LayoutSnapshotRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        if request.query.id != AgentReadLayout::ID {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        match serde_json::from_slice::<AgentReadLayout>(&request.query.body) {
            Ok(query) => {
                snapshots.write(LayoutSnapshotRequest {
                    request_id: request.request_id.0,
                    anchor: query.anchor,
                });
            }
            Err(error) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                    request_id: request.request_id,
                    message: error.to_string(),
                }));
            }
        }
    }
}

fn forward_layout_apply_responses(
    mut responses: MessageReader<LayoutApplyResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => AgentCommandResult::Layout(snapshot),
            Err(message) => AgentCommandResult::Error(message),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        }));
    }
}

fn forward_layout_snapshot_responses(
    mut responses: MessageReader<LayoutSnapshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        service_requests.write(ServiceRequest(ClientMessage::AgentLayoutResult {
            request_id: AgentRequestId(response.request_id),
            result: Ok(response.snapshot.clone()),
        }));
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
            .insert(ToolQuery(AgentRequest::encode(&AgentReadLayout {
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
            .insert(ToolCommand(AgentRequest::encode(&AgentUpdateLayout {
                layout: args.0.clone(),
            })));
    }
}

fn select_tab(
    mut commands: Commands,
    requests: Query<(Entity, &SelectTabArgs), AddedTool<SelectTabArgs>>,
) {
    for (entity, args) in &requests {
        let index = args.index;
        let command = if (1..=8).contains(&index) {
            AgentRequest::encode(&AgentInvokeCommand {
                id: format!("tab_select_{index}"),
                args: JsonValue::Object(Vec::new()),
            })
        } else {
            Err(format!(
                "select_tab.index must be between 1 and 8, got {index}"
            ))
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}
