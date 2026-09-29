use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentCommandResult, AgentInvokeCommand, AgentOpenBeside, AgentPaneDirection, AgentReadLayout,
    AgentRequest, AgentRequestId, AgentUpdateLayout, ClientMessage, JsonValue, layout,
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
        .register_tool::<OpenPageArgs>("open_page")
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
            (open_page, read_layout, update_layout, select_tab).in_set(ToolDispatchSet),
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

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PaneDirection {
    Top,
    Right,
    Bottom,
    Left,
}

impl From<PaneDirection> for AgentPaneDirection {
    fn from(value: PaneDirection) -> Self {
        match value {
            PaneDirection::Top => Self::Top,
            PaneDirection::Right => Self::Right,
            PaneDirection::Bottom => Self::Bottom,
            PaneDirection::Left => Self::Left,
        }
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenPageArgs {
    url: String,
    direction: Option<PaneDirection>,
    #[serde(default)]
    focus: bool,
}

fn open_page(
    mut commands: Commands,
    requests: Query<(Entity, &Name, Option<&ProcessAnchor>, &OpenPageArgs), Added<OpenPageArgs>>,
) {
    for (entity, name, anchor, args) in &requests {
        let command = ProcessAnchor::required(anchor, name.as_str()).and_then(|anchor| {
            if args.url.trim().is_empty() {
                return Err("open_page.url is empty".to_string());
            }
            AgentRequest::encode(&AgentOpenBeside {
                anchor,
                direction: args.direction.map(Into::into),
                url: args.url.clone(),
                focus: args.focus,
            })
        });
        commands.entity(entity).insert(ToolCommand(command));
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
