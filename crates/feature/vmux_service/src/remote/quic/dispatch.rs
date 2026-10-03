use vmux_agent::acp::AcpInput;
use vmux_api::conversation::RemoteSession;
use vmux_api::protocol::{
    AgentCommandResult, AgentListAgents, AgentListModels, AgentListTeam, AgentNewChat,
    AgentRequest, AgentSelectModel, AgentSetEffort, SharedFailure, SharedMessage, SharedResponse,
};

use super::super::server::{
    MAX_PROMPT_BYTES, RemoteAttachments, RemoteClientOpId, RemoteMediaQuery, RemoteState,
};

impl RemoteState {
    pub(crate) async fn dispatch(&self, request: SharedMessage) -> SharedResponse {
        match request {
            SharedMessage::ListSessions => SharedResponse::Sessions(self.sessions().await),

            SharedMessage::AgentAttach { sid } => self.attach(&sid).await,

            SharedMessage::AgentInput {
                sid,
                text,
                context,
                attachments,
                preferred_mode,
            } => {
                self.prompt(&sid, text, context, attachments, preferred_mode)
                    .await
            }

            SharedMessage::AgentCancel { sid } => self.push_input(&sid, AcpInput::Cancel).await,

            SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            } => {
                self.push_input(&sid, AcpInput::Approve { call_id, decision })
                    .await
            }

            SharedMessage::AgentListMedia { sid, query } => self.media(&sid, query).await,

            SharedMessage::AgentNewChat {
                client_op_id,
                prompt,
                agent_url,
            } => {
                if !RemoteClientOpId(&client_op_id).valid() {
                    return SharedResponse::Failed(SharedFailure::Invalid);
                }
                if !self.client_ops.claim(client_op_id.clone()).await {
                    return SharedResponse::AlreadyApplied;
                }
                let response = self
                    .broker(AgentNewChat {
                        client_op_id: client_op_id.clone(),
                        prompt,
                        agent_url,
                    })
                    .await;
                if matches!(response, SharedResponse::Failed(_)) {
                    self.client_ops.release(client_op_id).await;
                }
                response
            }

            SharedMessage::AgentListAgents => self.broker(AgentListAgents).await,

            SharedMessage::AgentListTeam => self.broker(AgentListTeam).await,

            SharedMessage::AgentListModels { sid } => self.broker(AgentListModels { sid }).await,

            SharedMessage::AgentSelectModel { sid, model_id } => {
                self.broker(AgentSelectModel { sid, model_id }).await
            }

            SharedMessage::AgentSetEffort { sid, level } => {
                self.broker(AgentSetEffort { sid, level }).await
            }
        }
    }

    async fn attach(&self, sid: &str) -> SharedResponse {
        if self.acp.remote_session(sid.to_string()).await.is_some() {
            return SharedResponse::Ok;
        }
        SharedResponse::Failed(SharedFailure::NotFound)
    }

    async fn media(&self, sid: &str, query: String) -> SharedResponse {
        if self.acp.remote_session(sid.to_string()).await.is_none() {
            return SharedResponse::Failed(SharedFailure::NotFound);
        }
        if query.len() > super::super::server::MAX_MEDIA_QUERY_BYTES {
            return SharedResponse::Failed(SharedFailure::Invalid);
        }
        match tokio::task::spawn_blocking(move || RemoteMediaQuery(&query).entries()).await {
            Ok(entries) => SharedResponse::Media(entries),
            Err(_) => SharedResponse::Failed(SharedFailure::Internal),
        }
    }

    async fn sessions(&self) -> Vec<RemoteSession> {
        let mut sessions = self.acp.remote_sessions().await;
        for session in &mut sessions {
            if let Some(messages) = self.session_messages(&session.id.0).await {
                session.title =
                    vmux_session::ConversationTitle::from_messages(&messages, &session.name);
            }
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
        sessions
    }

    async fn push_input(&self, sid: &str, input: AcpInput) -> SharedResponse {
        if self.acp.input(sid.to_string(), input).await {
            return SharedResponse::Ok;
        }
        SharedResponse::Failed(SharedFailure::NotFound)
    }

    async fn prompt(
        &self,
        sid: &str,
        text: String,
        context: Option<String>,
        attachments: Vec<vmux_api::protocol::AgentAttachment>,
        preferred_mode: Option<String>,
    ) -> SharedResponse {
        if text.trim().is_empty() || text.len() > MAX_PROMPT_BYTES {
            return SharedResponse::Failed(SharedFailure::Invalid);
        }
        let Some(attachments) = RemoteAttachments::validated(attachments) else {
            return SharedResponse::Failed(SharedFailure::Invalid);
        };
        self.push_input(
            sid,
            AcpInput::User {
                text,
                context,
                attachments,
                preferred_mode,
            },
        )
        .await
    }

    async fn broker<T>(&self, payload: T) -> SharedResponse
    where
        T: vmux_api::AgentRequestContract + serde::Serialize,
    {
        let Ok(request) = AgentRequest::encode(&payload) else {
            return SharedResponse::Failed(SharedFailure::Invalid);
        };
        match self.broker_result(request).await {
            Some(AgentCommandResult::Text(json)) => SharedResponse::BrokerJson(json),
            Some(AgentCommandResult::Ok) => SharedResponse::Ok,
            Some(AgentCommandResult::Error(message)) => {
                tracing::warn!(%message, "remote quic: the GUI refused a brokered command");
                SharedResponse::Failed(SharedFailure::Invalid)
            }
            None => SharedResponse::Failed(SharedFailure::NoDesktop),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use vmux_api::conversation::ClientOpId;

    fn empty_state() -> RemoteState {
        let (agent_tx, _) = broadcast::channel(8);
        let (acp_wake, acp_wake_inbox) = tokio::sync::mpsc::unbounded_channel();
        drop(acp_wake_inbox);
        let (acp, acp_runtime) =
            vmux_agent::acp::AcpSessions::new(tokio::runtime::Handle::current(), acp_wake);
        drop(acp_runtime);
        RemoteState {
            relay_token: Arc::from("token"),
            authorizations: crate::remote::authorization::RemoteAuthorizations::closed(),
            acp,
            broker: vmux_agent::broker::AgentBroker::new(
                agent_tx,
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            client_ops: crate::remote::client_operation::ClientOperations::closed(),
        }
    }

    fn prompt_of(length: usize) -> SharedMessage {
        SharedMessage::AgentInput {
            sid: "s".into(),
            text: "x".repeat(length),
            context: None,
            attachments: Vec::new(),
            preferred_mode: None,
        }
    }

    #[tokio::test]
    async fn an_oversized_prompt_is_refused_before_any_session_lookup() {
        let state = empty_state();

        let over = state.dispatch(prompt_of(MAX_PROMPT_BYTES + 1)).await;
        let under = state.dispatch(prompt_of(16)).await;

        assert!(matches!(
            over,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        assert!(matches!(
            under,
            SharedResponse::Failed(SharedFailure::NotFound)
        ));
    }

    #[tokio::test]
    async fn an_empty_prompt_is_refused() {
        let state = empty_state();

        let response = state
            .dispatch(SharedMessage::AgentInput {
                sid: "s".into(),
                text: "   ".into(),
                context: None,
                attachments: Vec::new(),
                preferred_mode: None,
            })
            .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
    }

    #[tokio::test]
    async fn a_broker_request_with_no_desktop_attached_says_so() {
        let state = empty_state();

        let response = state.dispatch(SharedMessage::AgentListAgents).await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::NoDesktop)
        ));
    }

    #[tokio::test]
    async fn operations_on_an_unknown_session_report_not_found() {
        let state = empty_state();

        for request in [
            SharedMessage::AgentCancel {
                sid: "ghost".into(),
            },
            SharedMessage::AgentAttach {
                sid: "ghost".into(),
            },
            SharedMessage::AgentListMedia {
                sid: "ghost".into(),
                query: String::new(),
            },
        ] {
            assert!(
                matches!(
                    state.dispatch(request).await,
                    SharedResponse::Failed(SharedFailure::NotFound)
                ),
                "unknown session should be NotFound"
            );
        }
    }

    #[tokio::test]
    async fn an_unbounded_client_op_id_is_refused() {
        let state = empty_state();
        let oversized = ClientOpId::new("x".repeat(4096));

        let response = state
            .dispatch(SharedMessage::AgentNewChat {
                client_op_id: oversized.clone(),
                prompt: "hello".into(),
                agent_url: None,
            })
            .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
    }
}
