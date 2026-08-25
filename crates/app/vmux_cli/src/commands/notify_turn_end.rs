use std::io::{self, Read};

use vmux_client::client::ServiceConnection;
use vmux_client::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentRequestId, ClientMessage, ProcessId, ServiceMessage,
};

pub async fn run(anchor: Option<String>) -> io::Result<()> {
    let anchor = match anchor {
        Some(raw) => raw.parse::<ProcessId>().ok(),
        None => std::env::var("VMUX_ANCHOR")
            .ok()
            .and_then(|s| s.parse::<ProcessId>().ok()),
    };
    let Some(anchor) = anchor else {
        return Ok(());
    };

    let mut buf = String::new();
    let _ = io::stdin().read_to_string(&mut buf);

    let Ok(connection) = ServiceConnection::connect().await else {
        return Ok(());
    };
    let request_id = AgentRequestId::new();
    if connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor: Some(anchor),
            command: AgentCommand::TurnEnded { anchor },
        })
        .await
        .is_err()
    {
        return Ok(());
    }

    let _ = tokio::time::timeout(AGENT_COMMAND_TIMEOUT, async {
        while let Ok(Some(message)) = connection.recv().await {
            if let ServiceMessage::AgentCommandResult {
                request_id: received,
                ..
            } = message
                && received == request_id
            {
                break;
            }
        }
    })
    .await;

    Ok(())
}
