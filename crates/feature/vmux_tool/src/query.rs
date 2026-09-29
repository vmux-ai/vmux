use std::collections::HashSet;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::protocol::{AgentRequest, AgentRequestId, ClientMessage, ServiceMessage};
use vmux_core::service::{ServiceMessageSet, ServiceMessageVariant, ServiceRequest};

pub struct ToolQueryPlugin;

impl Plugin for ToolQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ToolQueryRequest>()
            .add_message::<ToolQueryHandled>()
            .add_message::<ServiceRequest>()
            .configure_sets(
                Update,
                (ToolQueryRouteSet, ToolQueryFallbackSet)
                    .chain()
                    .after(ServiceMessageSet),
            )
            .add_systems(Update, reject_unhandled.in_set(ToolQueryFallbackSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToolQueryRouteSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ToolQueryFallbackSet;

#[derive(Message, Clone)]
pub struct ToolQueryRequest {
    pub request_id: AgentRequestId,
    pub query: AgentRequest,
}

impl ServiceMessageVariant for ToolQueryRequest {
    fn from_service_message(message: &ServiceMessage) -> Option<Self> {
        let ServiceMessage::AgentQuery { request_id, query } = message else {
            return None;
        };
        Some(Self {
            request_id: *request_id,
            query: query.clone(),
        })
    }
}

#[derive(Message, Clone, Copy)]
pub struct ToolQueryHandled(pub AgentRequestId);

fn reject_unhandled(
    mut queries: MessageReader<ToolQueryRequest>,
    mut handled: MessageReader<ToolQueryHandled>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    let handled = handled
        .read()
        .map(|handled| handled.0)
        .collect::<HashSet<_>>();
    for request in queries.read() {
        if handled.contains(&request.request_id) {
            continue;
        }
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
            request_id: request.request_id,
            message: format!("unknown agent query: {}", request.query.id),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle_known(
        mut queries: MessageReader<ToolQueryRequest>,
        mut handled: MessageWriter<ToolQueryHandled>,
    ) {
        for request in queries.read() {
            if request.query.id == "known@1" {
                handled.write(ToolQueryHandled(request.request_id));
            }
        }
    }

    #[test]
    fn fallback_rejects_only_unhandled_queries() {
        let mut app = App::new();
        app.add_plugins(ToolQueryPlugin)
            .add_systems(Update, handle_known.in_set(ToolQueryRouteSet));
        let known = AgentRequestId([1; 16]);
        let unknown = AgentRequestId([2; 16]);
        app.world_mut().write_message(ToolQueryRequest {
            request_id: known,
            query: AgentRequest {
                id: "known@1".to_string(),
                body: Vec::new(),
            },
        });
        app.world_mut().write_message(ToolQueryRequest {
            request_id: unknown,
            query: AgentRequest {
                id: "unknown@1".to_string(),
                body: Vec::new(),
            },
        });

        app.update();

        let requests = app.world().resource::<Messages<ServiceRequest>>();
        let mut cursor = requests.get_cursor();
        let responses = cursor.read(requests).collect::<Vec<_>>();
        assert_eq!(responses.len(), 1);
        assert!(matches!(
            &responses[0].0,
            ClientMessage::AgentQueryError { request_id, .. } if *request_id == unknown
        ));
    }
}
