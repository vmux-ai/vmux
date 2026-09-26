use std::io::{self, Read};

use bevy_ecs::prelude::*;
use clap::Args;
use vmux_api::protocol::{
    AGENT_COMMAND_TIMEOUT, AgentCommand, AgentRequestId, AgentTurnEnded, ClientMessage, ProcessId,
    ServiceMessage,
};
use vmux_service::client::ServiceConnection;

#[derive(Args, Clone, Component, Debug)]
pub struct NotifyTurnEndRequest {
    #[arg(long)]
    anchor: Option<String>,
}

impl NotifyTurnEndRequest {
    pub(crate) async fn send(self) -> io::Result<()> {
        let anchor = match self.anchor {
            Some(raw) => raw.parse::<ProcessId>().ok(),
            None => std::env::var("VMUX_ANCHOR")
                .ok()
                .and_then(|value| value.parse::<ProcessId>().ok()),
        };
        let Some(anchor) = anchor else {
            return Ok(());
        };

        let mut input = String::new();
        let _ = io::stdin().read_to_string(&mut input);

        let Ok(connection) = ServiceConnection::connect().await else {
            return Ok(());
        };
        let request_id = AgentRequestId::new();
        if connection
            .send(&ClientMessage::AgentCommand {
                request_id,
                anchor: Some(anchor),
                command: AgentCommand::TurnEnded(AgentTurnEnded { anchor }),
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
}
