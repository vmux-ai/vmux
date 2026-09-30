use std::collections::HashSet;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_api::protocol::{AgentRequest, AgentRequestId, ClientMessage, ServiceMessage};
use vmux_core::service::{
    ServiceMessagePlugin, ServiceMessageSet, ServiceMessageVariant, ServiceRequest,
};

#[vmux_api::agent(Copy, Eq)]
pub struct AgentWorkingDirectory {
    pub anchor: vmux_api::ProcessId,
}

pub struct ToolQueryPlugin;

impl Plugin for ToolQueryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ServiceMessagePlugin::<ToolQueryRequest>::default())
            .add_message::<ToolQueryRequest>()
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

#[derive(Message)]
pub struct ToolQueryMessage<T> {
    pub request_id: AgentRequestId,
    pub payload: T,
}

pub trait ToolQueryAppExt {
    fn add_tool_query<T>(&mut self) -> &mut Self
    where
        T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync;
}

impl ToolQueryAppExt for App {
    fn add_tool_query<T>(&mut self) -> &mut Self
    where
        T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync,
    {
        if !self.is_plugin_added::<ToolQueryPlugin>() {
            self.add_plugins(ToolQueryPlugin);
        }
        self.add_message::<ToolQueryMessage<T>>()
            .add_systems(Update, route_tool_queries::<T>.in_set(ToolQueryRouteSet))
    }
}

fn route_tool_queries<T>(
    mut queries: MessageReader<ToolQueryRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut routed: MessageWriter<ToolQueryMessage<T>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) where
    T: vmux_api::AgentRequestContract + serde::de::DeserializeOwned + Send + Sync,
{
    for request in queries.read() {
        match request.query.decode::<T>() {
            Ok(Some(payload)) => {
                handled.write(ToolQueryHandled(request.request_id));
                routed.write(ToolQueryMessage {
                    request_id: request.request_id,
                    payload,
                });
            }
            Ok(None) => {}
            Err(message) => {
                handled.write(ToolQueryHandled(request.request_id));
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryError {
                    request_id: request.request_id,
                    message,
                }));
            }
        }
    }
}

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
    use vmux_api::ProcessId;
    use vmux_api::protocol::ServiceMessage;
    use vmux_core::service::ServiceInbound;

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

    #[test]
    fn typed_queries_are_decoded_without_a_central_catalog() {
        let mut app = App::new();
        app.add_tool_query::<AgentWorkingDirectory>();
        let request_id = AgentRequestId([3; 16]);
        let anchor = ProcessId([7; 16]);
        app.world_mut().write_message(ToolQueryRequest {
            request_id,
            query: AgentRequest::encode(&AgentWorkingDirectory { anchor }).unwrap(),
        });

        app.update();

        let routed = app
            .world_mut()
            .resource_mut::<Messages<ToolQueryMessage<AgentWorkingDirectory>>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(routed.len(), 1);
        assert_eq!(routed[0].request_id, request_id);
        assert_eq!(routed[0].payload.anchor, anchor);
    }

    #[test]
    fn service_queries_enter_through_the_tool_boundary() {
        let mut app = App::new();
        app.add_plugins(ToolQueryPlugin);
        let request_id = AgentRequestId([4; 16]);
        let anchor = ProcessId([8; 16]);
        app.world_mut()
            .write_message(ServiceInbound(ServiceMessage::AgentQuery {
                request_id,
                query: AgentRequest::encode(&AgentWorkingDirectory { anchor }).unwrap(),
            }));

        app.update();

        let queries = app
            .world_mut()
            .resource_mut::<Messages<ToolQueryRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].request_id, request_id);
        assert_eq!(
            queries[0].query.decode::<AgentWorkingDirectory>().unwrap(),
            Some(AgentWorkingDirectory { anchor })
        );
    }
}
