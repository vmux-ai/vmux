use vmux_api::protocol::{
    AgentListAgents, AgentListModels, AgentListTeam, AgentNewChat, AgentRequest, AgentSelectModel,
    AgentSetEffort, SharedFailure, SharedMessage, SharedResponse,
};
use vmux_api::room::{ClientOpId, RemoteSession};

use super::super::server::{MAX_PROMPT_BYTES, RemoteState};
use crate::acp::AcpInput;
use vmux_agent::service::SessionInput;

pub(crate) async fn dispatch(state: &RemoteState, request: SharedMessage) -> SharedResponse {
    match request {
        SharedMessage::ListSessions => SharedResponse::Sessions(sessions(state).await),

        SharedMessage::AgentAttach { sid } => attach(state, &sid).await,

        SharedMessage::AgentInput {
            sid,
            text,
            context,
            attachments,
            preferred_mode,
        } => prompt(state, &sid, text, context, attachments, preferred_mode).await,

        SharedMessage::AgentCancel { sid } => {
            push_input(state, &sid, AcpInput::Cancel, SessionInput::Cancel).await
        }

        SharedMessage::AgentApprove {
            sid,
            call_id,
            decision,
        } => {
            push_input(
                state,
                &sid,
                AcpInput::Approve {
                    call_id: call_id.clone(),
                    decision,
                },
                SessionInput::Approve { call_id, decision },
            )
            .await
        }

        SharedMessage::AgentListMedia { sid, query } => media(state, &sid, query).await,

        SharedMessage::AgentNewChat {
            client_op_id,
            prompt,
            agent_url,
        } => {
            if !super::super::server::valid_client_op_id(&client_op_id) {
                return SharedResponse::Failed(SharedFailure::Invalid);
            }
            if !claim_once(state, &client_op_id).await {
                return SharedResponse::AlreadyApplied;
            }
            let response = broker(
                state,
                AgentNewChat {
                    client_op_id: client_op_id.clone(),
                    prompt,
                    agent_url,
                },
            )
            .await;
            if matches!(response, SharedResponse::Failed(_)) {
                release(state, &client_op_id).await;
            }
            response
        }

        SharedMessage::AgentListAgents => broker(state, AgentListAgents).await,

        SharedMessage::AgentListTeam => broker(state, AgentListTeam).await,

        SharedMessage::AgentListModels { sid } => broker(state, AgentListModels { sid }).await,

        SharedMessage::AgentSelectModel { sid, model_id } => {
            broker(state, AgentSelectModel { sid, model_id }).await
        }

        SharedMessage::AgentSetEffort { sid, level } => {
            broker(state, AgentSetEffort { sid, level }).await
        }
    }
}

async fn attach(state: &RemoteState, sid: &str) -> SharedResponse {
    if session_exists(state, sid).await {
        SharedResponse::Ok
    } else {
        SharedResponse::Failed(SharedFailure::NotFound)
    }
}

async fn media(state: &RemoteState, sid: &str, query: String) -> SharedResponse {
    if !session_exists(state, sid).await {
        return SharedResponse::Failed(SharedFailure::NotFound);
    }
    if query.len() > super::super::server::MAX_MEDIA_QUERY_BYTES {
        return SharedResponse::Failed(SharedFailure::Invalid);
    }
    match tokio::task::spawn_blocking(move || super::super::server::remote_media_entries(&query))
        .await
    {
        Ok(entries) => SharedResponse::Media(entries),
        Err(_) => SharedResponse::Failed(SharedFailure::Internal),
    }
}

async fn sessions(state: &RemoteState) -> Vec<RemoteSession> {
    let mut sessions = state.agents.remote_sessions().await;
    sessions.extend(state.acp.remote_sessions().await);
    for session in &mut sessions {
        if let Some(messages) = super::super::server::session_messages(state, &session.sid).await {
            session.title = vmux_api::room::Message::conversation_title(&messages, &session.name);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
    sessions
}

async fn session_exists(state: &RemoteState, sid: &str) -> bool {
    state.acp.remote_session(sid.to_string()).await.is_some()
        || state.agents.remote_session(sid.to_string()).await.is_some()
}

async fn push_input(
    state: &RemoteState,
    sid: &str,
    acp: AcpInput,
    page: SessionInput,
) -> SharedResponse {
    if state.acp.input(sid.to_string(), acp).await {
        return SharedResponse::Ok;
    }
    if state.agents.input(sid.to_string(), page).await {
        SharedResponse::Ok
    } else {
        SharedResponse::Failed(SharedFailure::NotFound)
    }
}

async fn prompt(
    state: &RemoteState,
    sid: &str,
    text: String,
    context: Option<String>,
    attachments: Vec<vmux_api::protocol::AgentAttachment>,
    preferred_mode: Option<String>,
) -> SharedResponse {
    if text.trim().is_empty() || text.len() > MAX_PROMPT_BYTES {
        return SharedResponse::Failed(SharedFailure::Invalid);
    }
    let Some(attachments) = super::super::server::validate_remote_attachments(attachments) else {
        return SharedResponse::Failed(SharedFailure::Invalid);
    };
    push_input(
        state,
        sid,
        AcpInput::User {
            text: text.clone(),
            context: context.clone(),
            attachments: attachments.clone(),
            preferred_mode,
        },
        SessionInput::User { text, attachments },
    )
    .await
}

async fn broker<T>(state: &RemoteState, payload: T) -> SharedResponse
where
    T: vmux_api::AgentRequestContract + serde::Serialize,
{
    use vmux_api::protocol::AgentCommandResult;
    let Ok(request) = AgentRequest::encode(&payload) else {
        return SharedResponse::Failed(SharedFailure::Invalid);
    };
    match super::super::server::broker_result(state, request).await {
        Some(AgentCommandResult::Text(json)) => SharedResponse::BrokerJson(json),
        Some(AgentCommandResult::Ok) | Some(AgentCommandResult::Layout(_)) => SharedResponse::Ok,
        Some(AgentCommandResult::Error(message)) => {
            tracing::warn!(%message, "remote quic: the GUI refused a brokered command");
            SharedResponse::Failed(SharedFailure::Invalid)
        }
        None => SharedResponse::Failed(SharedFailure::NoDesktop),
    }
}

async fn claim_once(state: &RemoteState, client_op_id: &ClientOpId) -> bool {
    state.client_ops.claim(client_op_id.clone()).await
}

async fn release(state: &RemoteState, client_op_id: &ClientOpId) {
    state.client_ops.release(client_op_id.clone()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn empty_state() -> RemoteState {
        let (agent_tx, _) = broadcast::channel(8);
        let (wake, wake_inbox) = tokio::sync::mpsc::unbounded_channel();
        drop(wake_inbox);
        let (agents, runtime) =
            vmux_agent::service::AgentSessions::new(tokio::runtime::Handle::current(), wake);
        drop(runtime);
        RemoteState {
            relay_token: Arc::from("token"),
            authorizations: crate::remote::authorization::RemoteAuthorizations::closed(),
            agents,
            acp: crate::acp::AcpSessions::closed(),
            broker: vmux_agent::service::AgentBroker::new(
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

        let over = dispatch(&state, prompt_of(MAX_PROMPT_BYTES + 1)).await;
        let under = dispatch(&state, prompt_of(16)).await;

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

        let response = dispatch(
            &state,
            SharedMessage::AgentInput {
                sid: "s".into(),
                text: "   ".into(),
                context: None,
                attachments: Vec::new(),
                preferred_mode: None,
            },
        )
        .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
    }

    #[tokio::test]
    async fn a_broker_request_with_no_desktop_attached_says_so() {
        let state = empty_state();

        let response = dispatch(&state, SharedMessage::AgentListAgents).await;

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
                    dispatch(&state, request).await,
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

        let response = dispatch(
            &state,
            SharedMessage::AgentNewChat {
                client_op_id: oversized.clone(),
                prompt: "hello".into(),
                agent_url: None,
            },
        )
        .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
    }
}
