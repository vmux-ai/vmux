use serde::Deserialize;
use vmux_api::protocol::{
    AgentListModels, AgentNewChat, AgentSelectModel, AgentSetEffort, SharedEvent as Shared,
    SharedFailure, SharedMessage, SharedResponse,
};
use vmux_api::session::{
    ApprovalRequest, ClientOpId, ConversationEvent, NewChatRequest, PromptRequest, RemoteAgent,
    RemoteApproval, RemoteEvent, RemoteMediaEntry, RemoteModelState, RemoteSession, RemoteStatus,
    SessionId,
};
use vmux_ui::i18n::translate;

use crate::credentials::Credentials;
use crate::quic::QuicError;

#[derive(Clone)]
pub(crate) struct Api {
    quic: crate::quic::QuicApi,
    credentials: Credentials,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Deserialize)]
struct CreatedChatReceipt {
    client_op_id: ClientOpId,
    session_id: SessionId,
}

impl CreatedChatReceipt {
    fn decode(expected_op_id: &ClientOpId, json: &str) -> Result<SessionId, ApiError> {
        let receipt: Self = serde_json::from_str(json)
            .map_err(|_| ApiError::Message(translate("mobile-error-unexpected-answer")))?;
        if receipt.client_op_id != *expected_op_id {
            return Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            )));
        }
        if receipt.session_id.as_str().trim().is_empty() {
            return Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            )));
        }
        Ok(receipt.session_id)
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
            credentials,
        })
    }

    pub(crate) async fn paired_credentials(&self) -> Option<Credentials> {
        let credential = self.quic.credential().await;
        if !matches!(credential, vmux_transport::ClientCredential::Device(_)) {
            return None;
        }
        let mut credentials = self.credentials.clone();
        credentials.credential = Some(credential);
        Some(credentials)
    }

    pub(crate) async fn reset_transport(&self) {
        self.quic.reset().await;
    }

    pub(crate) fn seal(&self) {
        self.quic.seal();
    }

    pub(crate) async fn close_and_drain(&self) {
        self.quic.close_and_drain().await;
    }

    pub(crate) async fn agents(&self) -> Result<Vec<RemoteAgent>, ApiError> {
        self.broker_json(SharedMessage::AgentListAgents).await
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
        self.broker_json(SharedMessage::AgentListModels {
            request: AgentListModels {
                sid: sid.to_string(),
            },
        })
        .await
    }

    pub(crate) async fn select_model(&self, sid: &str, model_id: &str) -> Result<(), ApiError> {
        self.apply(SharedMessage::AgentSelectModel {
            request: AgentSelectModel {
                sid: sid.to_string(),
                model_id: model_id.to_string(),
            },
        })
        .await
    }

    pub(crate) async fn set_effort(&self, sid: &str, level: &str) -> Result<(), ApiError> {
        self.apply(SharedMessage::AgentSetEffort {
            request: AgentSetEffort {
                sid: sid.to_string(),
                level: level.to_string(),
            },
        })
        .await
    }

    pub(crate) async fn apply(&self, request: SharedMessage) -> Result<(), ApiError> {
        self.applied(self.quic.request(request).await)
    }

    pub(crate) async fn team(&self) -> Result<Vec<vmux_api::team::TeamMemberRow>, ApiError> {
        self.broker_json(SharedMessage::AgentListTeam).await
    }

    pub(crate) async fn subscribe(&self, sid: &str) -> Result<crate::quic::Subscription, ApiError> {
        self.quic.subscribe(sid).await.map_err(Into::into)
    }

    pub(crate) async fn send_prompt(
        &self,
        sid: &str,
        request: &PromptRequest,
    ) -> Result<(), ApiError> {
        let message = SharedMessage::AgentInput {
            sid: sid.to_string(),
            text: request.text.clone(),
            context: None,
            attachments: request.attachments.clone(),
            preferred_mode: None,
        };
        self.applied(self.quic.request(message).await)
    }

    pub(crate) async fn create_chat(
        &self,
        request: &NewChatRequest,
    ) -> Result<SessionId, ApiError> {
        let message = SharedMessage::AgentNewChat {
            request: AgentNewChat {
                client_op_id: request.client_op_id.clone(),
                prompt: request.text.clone(),
                agent_url: request.agent_url.clone(),
            },
        };
        Self::created_chat(&request.client_op_id, self.quic.request(message).await)
    }

    pub(crate) async fn cancel(&self, sid: &str) -> Result<(), ApiError> {
        let message = SharedMessage::AgentCancel {
            sid: sid.to_string(),
        };
        self.applied(self.quic.request(message).await)
    }

    pub(crate) async fn approve(
        &self,
        sid: &str,
        request: &ApprovalRequest,
    ) -> Result<(), ApiError> {
        let message = SharedMessage::AgentApprove {
            sid: sid.to_string(),
            call_id: request.call_id.clone(),
            decision: request.decision,
        };
        self.applied(self.quic.request(message).await)
    }

    pub(crate) fn applied(
        &self,
        outcome: Result<SharedResponse, QuicError>,
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
        let request = SharedMessage::AgentListMedia {
            sid: sid.to_string(),
            query: query.to_string(),
        };
        match self.quic.request(request).await {
            Ok(SharedResponse::Media(entries)) => Ok(entries),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn event(event: Shared) -> Option<RemoteEvent> {
        match event {
            Shared::AgentDelta { sid, text } => Some(RemoteEvent::Delta {
                session_id: SessionId::from(sid),
                text,
            }),
            Shared::AgentRunStatusChanged { status, .. } => Some(RemoteEvent::Status {
                status: RemoteStatus::from(&status),
            }),
            Shared::AgentAwaitingApproval {
                call_id,
                name,
                args,
                ..
            } => Some(RemoteEvent::Approval {
                approval: Some(RemoteApproval {
                    call_id,
                    name,
                    args,
                }),
            }),
            Shared::AgentApprovalResolved { .. } => Some(RemoteEvent::Approval { approval: None }),
            Shared::AgentMessagesSnapshot { sid, messages } => {
                let session_id = SessionId::from(sid);
                let events = ConversationEvent::from_messages(&session_id, 0, &messages);
                Some(RemoteEvent::Snapshot {
                    session_id,
                    through_seq: events.len() as u64,
                    events,
                })
            }
            Shared::Session { session } => Some(RemoteEvent::Session {
                session: Box::new(session),
            }),
            Shared::AcpAgentInfo { .. } | Shared::AcpWorkspaceChanged { .. } => None,
        }
    }

    fn created_chat(
        expected_op_id: &ClientOpId,
        outcome: Result<SharedResponse, QuicError>,
    ) -> Result<SessionId, ApiError> {
        match outcome {
            Ok(SharedResponse::BrokerJson(json)) => {
                CreatedChatReceipt::decode(expected_op_id, &json)
            }
            Ok(SharedResponse::Ok | SharedResponse::AlreadyApplied) => Err(ApiError::NotFound),
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }

    async fn broker_json<T: serde::de::DeserializeOwned>(
        &self,
        request: SharedMessage,
    ) -> Result<T, ApiError> {
        match self.quic.request(request).await {
            Ok(SharedResponse::BrokerJson(json)) => {
                serde_json::from_str(&json).map_err(|error| ApiError::Message(error.to_string()))
            }
            Ok(_) => Err(ApiError::Message(translate(
                "mobile-error-unexpected-answer",
            ))),
            Err(error) => Err(error.into()),
        }
    }
}

impl From<QuicError> for ApiError {
    fn from(error: QuicError) -> Self {
        match error {
            QuicError::Unauthorized => Self::Unauthorized,
            QuicError::Refused(SharedFailure::NotFound) => Self::NotFound,
            other => Self::Message(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_chat_receipt_requires_matching_operation_id_and_non_empty_session_id() {
        let op = ClientOpId::new("mobile:1:1");
        let valid = Api::created_chat(
            &op,
            Ok(SharedResponse::BrokerJson(
                r#"{"client_op_id":"mobile:1:1","session_id":"sid-42"}"#.to_string(),
            )),
        );
        assert_eq!(valid, Ok(SessionId::new("sid-42")));

        let mismatched = Api::created_chat(
            &op,
            Ok(SharedResponse::BrokerJson(
                r#"{"client_op_id":"mobile:1:2","session_id":"sid-42"}"#.to_string(),
            )),
        );
        assert!(mismatched.is_err());

        let empty_sid = Api::created_chat(
            &op,
            Ok(SharedResponse::BrokerJson(
                r#"{"client_op_id":"mobile:1:1","session_id":"   "}"#.to_string(),
            )),
        );
        assert!(empty_sid.is_err());

        let legacy_ok = Api::created_chat(&op, Ok(SharedResponse::Ok));
        assert_eq!(legacy_ok, Err(ApiError::NotFound));
    }
}
