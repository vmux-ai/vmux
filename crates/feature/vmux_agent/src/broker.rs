use tokio::sync::broadcast;
use vmux_api::protocol::{
    AGENT_QUERY_TIMEOUT, AGENT_REQUEST_TIMEOUT, AGENT_TOOL_TIMEOUT, AgentCommandResult,
    AgentRequest, AgentRequestId, JsonValue, ProcessId, ServiceMessage,
};
use vmux_core::service::PendingRequests;

pub type AgentCommandResponses = PendingRequests<AgentRequestId, AgentCommandResult>;
pub type AgentQueryResponses = PendingRequests<AgentRequestId, ServiceMessage>;
pub type AgentToolResponses = PendingRequests<AgentRequestId, (String, bool)>;

const NO_AGENT_SUBSCRIBER: &str = "no desktop subscribed to agent commands";

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

    pub async fn tool_call(
        &self,
        request_id: AgentRequestId,
        sid: String,
        name: String,
        args: JsonValue,
    ) -> Result<(String, bool), String> {
        if self.outbound.receiver_count() == 0 {
            return Err(NO_AGENT_SUBSCRIBER.to_string());
        }
        self.tools
            .request(
                request_id,
                AGENT_TOOL_TIMEOUT,
                || {
                    self.outbound
                        .send(ServiceMessage::AgentToolCall {
                            request_id,
                            sid,
                            name,
                            args,
                        })
                        .is_ok()
                },
                NO_AGENT_SUBSCRIBER,
                "agent tool call timed out",
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
