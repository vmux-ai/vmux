use crate::pairing::Credentials;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use vmux_ui::i18n::translate;
use vmux_wire::protocol::{AgentAction, SharedAgentCommand, SharedMessage, SharedResponse};
use vmux_wire::room::{
    ApprovalRequest, ClientOpId, NewChatRequest, PromptRequest, RemoteAgent, RemoteApproval,
    RemoteEvent, RemoteMediaEntry, RemoteModelState, RemoteSession, RemoteStatus,
};

static NEXT_CLIENT_OP_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn next_client_op_id() -> ClientOpId {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_CLIENT_OP_ID.fetch_add(1, Ordering::Relaxed);
    ClientOpId::new(format!("mobile:{timestamp}:{sequence}"))
}

#[derive(Clone)]
pub(crate) struct Api {
    quic: crate::quic::QuicApi,
}

pub(crate) enum ApiError {
    Unauthorized,
    NotFound,
    Message(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str(&translate("mobile-error-pairing-expired")),
            Self::NotFound => f.write_str(&translate("mobile-error-not-offered")),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl Api {
    pub(crate) fn new(credentials: Credentials) -> Result<Self, ApiError> {
        let Some(endpoint) = credentials.endpoint() else {
            return Err(ApiError::Message(translate(
                "mobile-error-pairing-outdated",
            )));
        };
        Ok(Self {
            quic: crate::quic::QuicApi::new(endpoint),
        })
    }

    pub(crate) async fn reset_transport(&self) {
        self.quic.reset().await;
    }

    pub(crate) fn close(&self) {
        self.quic.close();
    }

    pub(crate) async fn agents(&self) -> Result<Vec<RemoteAgent>, ApiError> {
        broker_json(&self.quic, SharedAgentCommand::ListAgents).await
    }

    pub(crate) async fn sessions(&self) -> Result<Vec<RemoteSession>, ApiError> {
        match self.quic.request(SharedMessage::ListSessions).await {
            Ok(SharedResponse::Sessions(sessions)) => Ok(sessions),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) async fn models(&self, sid: &str) -> Result<RemoteModelState, ApiError> {
        broker_json(
            &self.quic,
            SharedAgentCommand::ListModels {
                sid: sid.to_string(),
            },
        )
        .await
    }

    pub(crate) async fn select_model(&self, sid: &str, model_id: &str) -> Result<(), ApiError> {
        self.command(SharedAgentCommand::SelectModel {
            sid: sid.to_string(),
            model_id: model_id.to_string(),
        })
        .await
    }

    pub(crate) async fn set_effort(&self, sid: &str, level: &str) -> Result<(), ApiError> {
        self.command(SharedAgentCommand::SetEffort {
            sid: sid.to_string(),
            level: level.to_string(),
        })
        .await
    }

    pub(crate) async fn command(&self, command: SharedAgentCommand) -> Result<(), ApiError> {
        self.applied(
            self.quic
                .request(SharedMessage::AgentCommand(command))
                .await,
        )
    }

    pub(crate) async fn team(&self) -> Result<Vec<vmux_wire::team::TeamMemberRow>, ApiError> {
        broker_json(&self.quic, SharedAgentCommand::ListTeam).await
    }

    pub(crate) async fn subscribe(&self, sid: &str) -> Result<crate::quic::Subscription, ApiError> {
        self.quic.subscribe(sid).await.map_err(Into::into)
    }

    pub(crate) async fn send_prompt(
        &self,
        sid: &str,
        request: &PromptRequest,
    ) -> Result<(), ApiError> {
        let message = SharedMessage::agent(
            sid,
            AgentAction::Input {
                text: request.text.clone(),
                context: None,
                attachments: request.attachments.clone(),
            },
        );
        self.applied(self.quic.request(message).await)
    }

    pub(crate) async fn create_chat(&self, request: &NewChatRequest) -> Result<(), ApiError> {
        let command = SharedAgentCommand::NewAgentChat {
            client_op_id: request.client_op_id.clone(),
            prompt: request.text.clone(),
            agent_url: request.agent_url.clone(),
        };
        self.applied(
            self.quic
                .request(SharedMessage::AgentCommand(command))
                .await,
        )
    }

    pub(crate) async fn cancel(&self, sid: &str) -> Result<(), ApiError> {
        let message = SharedMessage::agent(sid, AgentAction::Cancel);
        self.applied(self.quic.request(message).await)
    }

    pub(crate) async fn approve(
        &self,
        sid: &str,
        request: &ApprovalRequest,
    ) -> Result<(), ApiError> {
        let message = SharedMessage::agent(
            sid,
            AgentAction::Approve {
                call_id: request.call_id.clone(),
                decision: request.decision,
            },
        );
        self.applied(self.quic.request(message).await)
    }

    pub(crate) fn applied(
        &self,
        outcome: Result<SharedResponse, crate::quic::QuicError>,
    ) -> Result<(), ApiError> {
        match outcome {
            Ok(SharedResponse::Ok | SharedResponse::AlreadyApplied) => Ok(()),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) async fn media(
        &self,
        sid: &str,
        query: &str,
    ) -> Result<Vec<RemoteMediaEntry>, ApiError> {
        let request = SharedMessage::agent(
            sid,
            AgentAction::ListMedia {
                query: query.to_string(),
            },
        );
        match self.quic.request(request).await {
            Ok(SharedResponse::Media(entries)) => Ok(entries),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }
}

pub(crate) fn remote_event_from_shared(
    event: vmux_wire::protocol::SharedEvent,
) -> Option<RemoteEvent> {
    use vmux_wire::protocol::SharedEvent as Shared;
    match event {
        Shared::AgentDelta { sid, text } => Some(RemoteEvent::Delta {
            room_id: vmux_wire::room::RoomId::for_session(&sid),
            text,
        }),
        Shared::AgentRunStatusChanged { status, .. } => Some(RemoteEvent::Status {
            status: RemoteStatus::from(&status),
        }),
        Shared::AgentAwaitingApproval {
            call_id,
            name,
            args_json,
            ..
        } => Some(RemoteEvent::Approval {
            approval: Some(RemoteApproval {
                call_id,
                name,
                args_json,
            }),
        }),
        Shared::AgentApprovalResolved { .. } => Some(RemoteEvent::Approval { approval: None }),
        Shared::AgentMessagesSnapshot { sid, messages_json } => {
            let messages: Vec<vmux_wire::room::Message> =
                serde_json::from_str(&messages_json).ok()?;
            let room_id = vmux_wire::room::RoomId::for_session(&sid);
            let events = vmux_wire::room::RoomEvent::from_messages(&sid, 0, &messages);
            Some(RemoteEvent::Snapshot {
                room_id,
                through_seq: events.len() as u64,
                events,
            })
        }
        Shared::Session { session } => Some(RemoteEvent::Session { session }),
        Shared::AcpAgentInfo { .. }
        | Shared::AcpWorkspaceChanged { .. }
        | Shared::AcpModelInfo { .. } => None,
    }
}

async fn broker_json<T: serde::de::DeserializeOwned>(
    quic: &crate::quic::QuicApi,
    command: SharedAgentCommand,
) -> Result<T, ApiError> {
    match quic.request(SharedMessage::AgentCommand(command)).await {
        Ok(SharedResponse::BrokerJson(json)) => {
            serde_json::from_str(&json).map_err(|error| ApiError::Message(error.to_string()))
        }
        Ok(_) => Err(ApiError::Message(translate(
            "mobile-error-unexpected-answer",
        ))),
        Err(error) => Err(error.into()),
    }
}

impl From<crate::quic::QuicError> for ApiError {
    fn from(error: crate::quic::QuicError) -> Self {
        use crate::quic::QuicError;
        use vmux_wire::protocol::SharedFailure;
        match error {
            QuicError::Unauthorized => Self::Unauthorized,
            QuicError::Refused(SharedFailure::NotFound) => Self::NotFound,
            other => Self::Message(other.to_string()),
        }
    }
}
