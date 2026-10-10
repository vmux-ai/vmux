use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::{Mutex, oneshot};
use vmux_api::protocol::{
    AGENT_QUERY_TIMEOUT, AGENT_REQUEST_TIMEOUT, AgentCommandResult, AgentRequest, AgentRequestId,
    ProcessId, ServiceMessage,
};

pub type AgentCommandResponses = PendingResponses<AgentRequestId, AgentCommandResult>;
pub type AgentQueryResponses = PendingResponses<AgentRequestId, ServiceMessage>;
pub type AgentToolResponses = PendingResponses<AgentRequestId, (String, bool)>;

const NO_AGENT_SUBSCRIBER: &str = "no desktop subscribed to agent commands";

pub struct PendingResponses<K, V> {
    entries: Arc<Mutex<HashMap<K, oneshot::Sender<V>>>>,
}

impl<K, V> Clone for PendingResponses<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
        }
    }
}

impl<K, V> Default for PendingResponses<K, V> {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<K, V> PendingResponses<K, V>
where
    K: Copy + Eq + Hash,
{
    async fn request(
        &self,
        id: K,
        timeout: Duration,
        publish: impl FnOnce() -> bool,
        unavailable: &'static str,
        timed_out: &'static str,
    ) -> Result<V, String> {
        let (sender, receiver) = oneshot::channel();
        self.entries.lock().await.insert(id, sender);
        if !publish() {
            self.entries.lock().await.remove(&id);
            return Err(unavailable.to_string());
        }
        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                self.entries.lock().await.remove(&id);
                Err(timed_out.to_string())
            }
        }
    }

    async fn resolve(&self, id: K, response: V) -> bool {
        let Some(sender) = self.entries.lock().await.remove(&id) else {
            return false;
        };
        sender.send(response).is_ok()
    }
}

#[derive(Clone)]
pub struct AgentBroker {
    outbound: broadcast::Sender<ServiceMessage>,
    commands: AgentCommandResponses,
    queries: AgentQueryResponses,
    tools: AgentToolResponses,
}

impl AgentBroker {
    pub fn new(
        outbound: broadcast::Sender<ServiceMessage>,
        commands: AgentCommandResponses,
        queries: AgentQueryResponses,
        tools: AgentToolResponses,
    ) -> Self {
        Self {
            outbound,
            commands,
            queries,
            tools,
        }
    }

    pub async fn command(
        &self,
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        request: AgentRequest,
    ) -> Result<AgentCommandResult, String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        self.commands
            .request(
                request_id,
                AGENT_REQUEST_TIMEOUT,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentRequest {
                            request_id,
                            anchor,
                            request,
                        })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent command timed out",
            )
            .await
    }

    pub async fn query(
        &self,
        request_id: AgentRequestId,
        query: AgentRequest,
    ) -> Result<ServiceMessage, String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        self.queries
            .request(
                request_id,
                AGENT_QUERY_TIMEOUT,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentQuery { request_id, query })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent query timed out",
            )
            .await
    }

    pub async fn resolve_command(
        &self,
        request_id: AgentRequestId,
        result: AgentCommandResult,
    ) -> bool {
        self.commands.resolve(request_id, result).await
    }

    pub async fn resolve_query(&self, request_id: AgentRequestId, result: ServiceMessage) -> bool {
        self.queries.resolve(request_id, result).await
    }

    pub async fn resolve_tool(&self, request_id: AgentRequestId, content: String, is_error: bool) {
        self.tools.resolve(request_id, (content, is_error)).await;
    }
}
