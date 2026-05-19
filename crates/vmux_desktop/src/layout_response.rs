use bevy::prelude::*;
use vmux_layout::reconcile::{LayoutApplyResponse, LayoutSnapshotResponse};
use vmux_service::protocol::{AgentCommandResult, AgentQueryResult, AgentRequestId, ClientMessage};

use crate::terminal::ServiceClient;

pub(crate) struct LayoutResponseForwarderPlugin;

impl Plugin for LayoutResponseForwarderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                forward_layout_apply_responses,
                forward_layout_snapshot_responses,
            ),
        );
    }
}

fn forward_layout_apply_responses(
    mut reader: MessageReader<LayoutApplyResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        let result = match response.result.clone() {
            Ok(snapshot) => AgentCommandResult::Layout(snapshot),
            Err(message) => AgentCommandResult::Error(message),
        };
        service.0.send(ClientMessage::AgentCommandResponse {
            request_id: AgentRequestId(response.request_id),
            result,
        });
    }
}

fn forward_layout_snapshot_responses(
    mut reader: MessageReader<LayoutSnapshotResponse>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else { return };
    for response in reader.read() {
        service.0.send(ClientMessage::AgentQueryResponse {
            request_id: AgentRequestId(response.request_id),
            result: AgentQueryResult::Layout(response.snapshot.clone()),
        });
    }
}
