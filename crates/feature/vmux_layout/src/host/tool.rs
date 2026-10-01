use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentCommandResult, AgentQueryResult, AgentRequest, AgentRequestId, ClientMessage, JsonValue,
    layout,
};
use vmux_command::AgentInvokeCommand;
use vmux_ecs::ProcessAnchor;
use vmux_ecs::host::manifest::FeaturePlugin;
use vmux_ecs::service::{ServiceMessageSet, ServiceRequest};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolQuery, ToolQueryHandled,
    ToolQueryRequest, ToolQueryRouteSet,
};

use super::agent::{AgentOpenBeside, AgentPaneDirection, AgentReadLayout, AgentUpdateLayout};
use crate::apply::{LayoutApplyResponse, LayoutSnapshotRequest, LayoutSnapshotResponse};

pub struct LayoutToolPlugin;

impl Plugin for LayoutToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .bind_tool::<OpenPageArgs>()
            .bind_tool::<ReadLayoutArgs>()
            .bind_tool::<UpdateLayoutArgs>()
            .bind_tool::<SelectTabArgs>()
            .add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .add_message::<LayoutSnapshotRequest>()
            .add_message::<LayoutSnapshotResponse>()
            .add_message::<LayoutApplyResponse>()
            .add_systems(
                Update,
                (open_page, read, update, select_tab).in_set(ToolDispatchSet),
            )
            .add_systems(
                Update,
                route_queries
                    .in_set(ToolQueryRouteSet)
                    .after(ServiceMessageSet),
            )
            .add_systems(Update, (forward_apply_responses, forward_snapshots));
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

#[vmux_tool::input]
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

fn route_queries(
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
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                    AgentQueryResult::text(request.request_id, Err(error.to_string())),
                )));
            }
        }
    }
}

fn forward_apply_responses(
    mut responses: MessageReader<LayoutApplyResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => match serde_json::to_string(&snapshot) {
                Ok(json) => AgentCommandResult::Text(json),
                Err(error) => AgentCommandResult::Error(error.to_string()),
            },
            Err(message) => AgentCommandResult::Error(message),
        };
        service_requests.write(ServiceRequest(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        }));
    }
}

fn forward_snapshots(
    mut responses: MessageReader<LayoutSnapshotResponse>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for response in responses.read() {
        let result = serde_json::to_string(&response.snapshot).map_err(|error| error.to_string());
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(AgentRequestId(response.request_id), result),
        )));
    }
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadLayoutArgs {}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectTabArgs {
    index: u8,
}

#[vmux_tool::input]
#[derive(Component, Deserialize)]
#[serde(transparent)]
struct UpdateLayoutArgs(layout::LayoutSnapshot);

fn read(
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

fn update(
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
