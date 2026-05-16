use crate::{agent::AgentQueryRequest, terminal::ServiceClient};
use bevy::prelude::*;
use vmux_service::protocol::{AgentQuery, AgentQueryResult, ClientMessage};

pub(crate) fn handle_agent_queries(
    mut reader: MessageReader<AgentQueryRequest>,
    mut query_result_writer: MessageWriter<crate::agent::AgentQueryResultMessage>,
    service: Option<Res<ServiceClient>>,
    settings: Res<crate::settings::AppSettings>,
    mut layout_snapshot_writer: MessageWriter<vmux_layout::reconcile::LayoutSnapshotRequest>,
) {
    for request in reader.read() {
        match request.query {
            AgentQuery::ReadLayout => {
                layout_snapshot_writer.write(vmux_layout::reconcile::LayoutSnapshotRequest {
                    request_id: request.request_id.0,
                });
            }
            AgentQuery::GetSettings => {
                let result = AgentQueryResult::Settings(
                    crate::settings::serialize_settings_to_json(&settings),
                );
                query_result_writer.write(crate::agent::AgentQueryResultMessage {
                    request_id: request.request_id,
                    result: result.clone(),
                });
                if let Some(service) = service.as_ref() {
                    service.0.send(ClientMessage::AgentQueryResponse {
                        request_id: request.request_id,
                        result,
                    });
                }
            }
        }
    }
}
