use bevy::prelude::*;
use vmux_service::events::AgentQueryRequest;
use vmux_service::protocol::{AgentQuery, AgentQueryResult, ClientMessage};
use vmux_terminal::ServiceClient;

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    service: Option<Res<ServiceClient>>,
    settings: Res<vmux_settings::AppSettings>,
    mut layout_snapshot_writer: MessageWriter<vmux_layout::reconcile::LayoutSnapshotRequest>,
) {
    let Some(service) = service else { return };

    for request in reader.read() {
        match request.query {
            AgentQuery::ReadLayout => {
                layout_snapshot_writer.write(vmux_layout::reconcile::LayoutSnapshotRequest {
                    request_id: request.request_id.0,
                });
            }
            AgentQuery::GetSettings => {
                let result = AgentQueryResult::Settings(vmux_settings::serialize_settings_to_json(
                    &settings,
                ));
                service.0.send(ClientMessage::AgentQueryResponse {
                    request_id: request.request_id,
                    result,
                });
            }
        }
    }
}
