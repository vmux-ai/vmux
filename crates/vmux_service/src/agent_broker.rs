use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast, oneshot};

use crate::protocol::{
    AGENT_COMMAND_TIMEOUT, AGENT_QUERY_TIMEOUT, AGENT_TOOL_TIMEOUT, AgentCommand,
    AgentCommandResult, AgentQuery, AgentQueryResult, AgentRequestId, ProcessId, ServiceMessage,
};

pub type PendingCommands = Arc<Mutex<HashMap<AgentRequestId, oneshot::Sender<AgentCommandResult>>>>;
pub type PendingQueries = Arc<Mutex<HashMap<AgentRequestId, oneshot::Sender<AgentQueryResult>>>>;
pub type PendingToolCalls = Arc<Mutex<HashMap<AgentRequestId, oneshot::Sender<(String, bool)>>>>;

const NO_SUBSCRIBER: &str = "no desktop subscribed to agent commands";

#[derive(Clone)]
pub struct AgentBroker {
    agent_tx: broadcast::Sender<ServiceMessage>,
    pending_commands: PendingCommands,
    pending_queries: PendingQueries,
    pending_tool_calls: PendingToolCalls,
}

impl AgentBroker {
    pub fn new(
        agent_tx: broadcast::Sender<ServiceMessage>,
        pending_commands: PendingCommands,
        pending_queries: PendingQueries,
        pending_tool_calls: PendingToolCalls,
    ) -> Self {
        Self {
            agent_tx,
            pending_commands,
            pending_queries,
            pending_tool_calls,
        }
    }

    pub async fn command(
        &self,
        request_id: AgentRequestId,
        anchor: Option<ProcessId>,
        command: AgentCommand,
    ) -> Result<AgentCommandResult, String> {
        if self.agent_tx.receiver_count() == 0 {
            return Err(NO_SUBSCRIBER.to_string());
        }
        let (tx, rx) = oneshot::channel::<AgentCommandResult>();
        self.pending_commands.lock().await.insert(request_id, tx);

        if self
            .agent_tx
            .send(ServiceMessage::AgentCommand {
                request_id,
                anchor,
                command,
            })
            .is_err()
        {
            self.pending_commands.lock().await.remove(&request_id);
            return Err(NO_SUBSCRIBER.to_string());
        }

        match tokio::time::timeout(AGENT_COMMAND_TIMEOUT, rx).await {
            Ok(Ok(result)) => Ok(result),
            _ => {
                self.pending_commands.lock().await.remove(&request_id);
                Err("agent command timed out".to_string())
            }
        }
    }

    pub async fn query(
        &self,
        request_id: AgentRequestId,
        query: AgentQuery,
    ) -> Result<AgentQueryResult, String> {
        if self.agent_tx.receiver_count() == 0 {
            return Err(NO_SUBSCRIBER.to_string());
        }
        let (tx, rx) = oneshot::channel::<AgentQueryResult>();
        self.pending_queries.lock().await.insert(request_id, tx);

        if self
            .agent_tx
            .send(ServiceMessage::AgentQuery { request_id, query })
            .is_err()
        {
            self.pending_queries.lock().await.remove(&request_id);
            return Err(NO_SUBSCRIBER.to_string());
        }

        match tokio::time::timeout(AGENT_QUERY_TIMEOUT, rx).await {
            Ok(Ok(result)) => Ok(result),
            _ => {
                self.pending_queries.lock().await.remove(&request_id);
                Err("agent query timed out".to_string())
            }
        }
    }

    pub async fn tool_call(
        &self,
        request_id: AgentRequestId,
        sid: String,
        name: String,
        args_json: String,
    ) -> Result<(String, bool), String> {
        if self.agent_tx.receiver_count() == 0 {
            return Err(NO_SUBSCRIBER.to_string());
        }
        let (tx, rx) = oneshot::channel::<(String, bool)>();
        self.pending_tool_calls.lock().await.insert(request_id, tx);

        if self
            .agent_tx
            .send(ServiceMessage::AgentToolCall {
                request_id,
                sid,
                name,
                args_json,
            })
            .is_err()
        {
            self.pending_tool_calls.lock().await.remove(&request_id);
            return Err(NO_SUBSCRIBER.to_string());
        }

        match tokio::time::timeout(AGENT_TOOL_TIMEOUT, rx).await {
            Ok(Ok(result)) => Ok(result),
            _ => {
                self.pending_tool_calls.lock().await.remove(&request_id);
                Err("agent tool call timed out".to_string())
            }
        }
    }

    pub async fn resolve_tool(&self, request_id: AgentRequestId, content: String, is_error: bool) {
        if let Some(tx) = self.pending_tool_calls.lock().await.remove(&request_id) {
            let _ = tx.send((content, is_error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broker() -> (AgentBroker, broadcast::Sender<ServiceMessage>) {
        let (agent_tx, _) = broadcast::channel::<ServiceMessage>(16);
        let pending_commands: PendingCommands = Arc::new(Mutex::new(HashMap::new()));
        let pending_queries: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
        let pending_tool_calls: PendingToolCalls = Arc::new(Mutex::new(HashMap::new()));
        let b = AgentBroker::new(
            agent_tx.clone(),
            pending_commands,
            pending_queries,
            pending_tool_calls,
        );
        (b, agent_tx)
    }

    #[tokio::test]
    async fn command_errors_when_no_subscriber() {
        let (b, _agent_tx) = broker();
        let err = b
            .command(
                AgentRequestId::new(),
                None,
                AgentCommand::OpenInNewStack {
                    url: "https://x".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err, NO_SUBSCRIBER);
    }

    #[tokio::test]
    async fn command_resolves_when_desktop_responds() {
        let (b, agent_tx) = broker();
        let mut rx = agent_tx.subscribe();
        let pending = b.pending_commands.clone();

        let desktop = tokio::spawn(async move {
            if let Ok(ServiceMessage::AgentCommand { request_id, .. }) = rx.recv().await
                && let Some(tx) = pending.lock().await.remove(&request_id)
            {
                let _ = tx.send(AgentCommandResult::Ok);
            }
        });

        let result = b
            .command(
                AgentRequestId::new(),
                None,
                AgentCommand::OpenInNewStack {
                    url: "https://x".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(result, AgentCommandResult::Ok);
        desktop.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn command_times_out_when_desktop_silent() {
        let (b, agent_tx) = broker();
        let _rx = agent_tx.subscribe();
        let err = b
            .command(
                AgentRequestId::new(),
                None,
                AgentCommand::OpenInNewStack {
                    url: "https://x".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err, "agent command timed out");
    }

    #[tokio::test]
    async fn query_errors_when_no_subscriber() {
        let (b, _agent_tx) = broker();
        let err = b
            .query(AgentRequestId::new(), AgentQuery::GetSettings)
            .await
            .unwrap_err();
        assert_eq!(err, NO_SUBSCRIBER);
    }
}
