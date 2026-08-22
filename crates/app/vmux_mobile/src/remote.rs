//! The phone half of the remote API.
//!
//! Every call the app makes of the desktop goes through [`Api`], which speaks the shared
//! subset of the protocol over QUIC. Nothing here is iOS-specific; the transport is.

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
    /// No such session on the Mac. Asking again will not conjure one.
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
    /// Fails when the pairing carries no certificate fingerprint.
    ///
    /// There is nothing to fall back to: the Mac is reached by pinning that certificate, so a
    /// pairing without one names a desktop this build cannot dial. Re-pairing is the only fix.
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

    /// Drop any live QUIC connection so the next call redials.
    pub(crate) async fn reset_transport(&self) {
        self.quic.reset().await;
    }

    /// Close the connection, for a client being replaced or cleared.
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

    /// The models this session can run, and its current effort level.
    pub(crate) async fn models(&self, sid: &str) -> Result<RemoteModelState, ApiError> {
        broker_json(
            &self.quic,
            SharedAgentCommand::ListModels {
                sid: sid.to_string(),
            },
        )
        .await
    }

    /// Switch the session to another of its models.
    pub(crate) async fn select_model(&self, sid: &str, model_id: &str) -> Result<(), ApiError> {
        self.command(SharedAgentCommand::SelectModel {
            sid: sid.to_string(),
            model_id: model_id.to_string(),
        })
        .await
    }

    /// Set how hard the session's agent is asked to think. An empty level restores its default.
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

    /// Subscribe to a session's events.
    pub(crate) async fn subscribe(&self, sid: &str) -> Result<crate::quic::Subscription, ApiError> {
        self.quic.subscribe(sid).await.map_err(Into::into)
    }

    /// Submit a prompt to a running session.
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

    /// Open a new chat on the desktop.
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

    /// Interrupt the session's in-flight turn.
    pub(crate) async fn cancel(&self, sid: &str) -> Result<(), ApiError> {
        let message = SharedMessage::agent(sid, AgentAction::Cancel);
        self.applied(self.quic.request(message).await)
    }

    /// Answer a pending tool approval.
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

    /// A replay is success, not failure: the desktop recognised the op and declined to run it
    /// twice, which is exactly what the idempotency key is for.
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

/// Project a shared event onto the shape the pages already render.
///
/// The desktop used to do this before serialising to SSE. Doing it here instead keeps the wire
/// typed — `RemoteEvent` is now a rendering concern of this app, not a thing any peer sends.
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
        // The daemon resolves these into Session before they reach a client; reaching here means
        // an older desktop that predates that, and there is nothing renderable to derive.
        Shared::AcpAgentInfo { .. }
        | Shared::AcpWorkspaceChanged { .. }
        | Shared::AcpModelInfo { .. } => None,
    }
}

/// GUI-held state comes back as JSON the desktop forwarded verbatim, so it is parsed here rather
/// than re-typed on the wire — the shape belongs to the page that renders it.
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
