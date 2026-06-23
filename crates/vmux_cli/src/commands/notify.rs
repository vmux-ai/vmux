use std::io;

use vmux_service::client::ServiceConnection;
use vmux_service::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentCommandResult, AgentRequestId, ClientMessage,
    ProcessId, ServiceMessage,
};

pub async fn run(
    title: Option<String>,
    body: Option<String>,
    anchor: Option<String>,
) -> io::Result<()> {
    let anchor = match anchor {
        Some(raw) => Some(raw.parse::<ProcessId>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid --anchor: {raw}"),
            )
        })?),
        None => std::env::var("VMUX_ANCHOR")
            .ok()
            .and_then(|s| s.parse::<ProcessId>().ok()),
    };

    let connection = match ServiceConnection::connect().await {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("vmux notify: cannot connect to vmux service: {error}");
            return Ok(());
        }
    };

    let request_id = AgentRequestId::new();
    if let Err(error) = connection
        .send(&ClientMessage::AgentCommand {
            request_id,
            anchor,
            command: AgentCommand::Notify { title, body },
        })
        .await
    {
        eprintln!("vmux notify: failed to send: {error}");
        return Ok(());
    }

    let _ = tokio::time::timeout(AGENT_COMMAND_TIMEOUT, async {
        while let Ok(Some(message)) = connection.recv().await {
            if let ServiceMessage::AgentCommandResult {
                request_id: received,
                result,
            } = message
                && received == request_id
            {
                if let AgentCommandResult::Error(message) = result {
                    eprintln!("vmux notify: {message}");
                }
                break;
            }
        }
    })
    .await;

    Ok(())
}
