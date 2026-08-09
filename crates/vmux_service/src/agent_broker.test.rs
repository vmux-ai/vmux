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

#[test]
fn record_stop_gets_longer_timeout() {
    let stop = AgentQuery::RecordStop {
        dir: None,
        name: None,
    };
    assert_eq!(query_timeout(&stop), crate::protocol::RECORD_STOP_TIMEOUT);
    assert_eq!(query_timeout(&AgentQuery::GetSettings), AGENT_QUERY_TIMEOUT);
}

#[test]
fn browser_navigate_gets_longer_timeout() {
    let navigate = AgentCommand::BrowserNavigate {
        url: "https://example.com".into(),
        pane: None,
    };
    assert_eq!(command_timeout(&navigate), BROWSER_NAVIGATE_TIMEOUT);
    assert_eq!(
        command_timeout(&AgentCommand::OpenInNewStack {
            url: "https://example.com".into(),
        }),
        AGENT_COMMAND_TIMEOUT
    );
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
