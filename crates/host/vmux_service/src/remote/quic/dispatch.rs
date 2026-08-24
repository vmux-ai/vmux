use vmux_wire::protocol::{
    AgentAction, SharedAgentCommand, SharedFailure, SharedMessage, SharedResponse,
};
use vmux_wire::room::{ClientOpId, RemoteSession};

use super::super::server::{MAX_PROMPT_BYTES, RemoteState};
use crate::acp::AcpInput;
use crate::agent::SessionInput;

pub(crate) async fn dispatch(state: &RemoteState, request: SharedMessage) -> SharedResponse {
    match request {
        SharedMessage::ListSessions => SharedResponse::Sessions(sessions(state).await),

        SharedMessage::Agent { sid, action } => agent(state, &sid, action).await,

        SharedMessage::AgentCommand(command) => {
            let Some(client_op_id) = new_chat_op_id(&command) else {
                return broker(state, command).await;
            };
            if !super::super::server::valid_client_op_id(&client_op_id) {
                return SharedResponse::Failed(SharedFailure::Invalid);
            }
            if !claim_once(state, &client_op_id).await {
                return SharedResponse::AlreadyApplied;
            }
            let response = broker(state, command).await;
            if matches!(response, SharedResponse::Failed(_)) {
                release(state, &client_op_id).await;
            }
            response
        }
    }
}

async fn agent(state: &RemoteState, sid: &str, action: AgentAction) -> SharedResponse {
    match action {
        AgentAction::Attach => {
            if session_exists(state, sid).await {
                SharedResponse::Ok
            } else {
                SharedResponse::Failed(SharedFailure::NotFound)
            }
        }

        AgentAction::Input {
            text,
            context,
            attachments,
        } => prompt(state, sid, text, context, attachments).await,

        AgentAction::Cancel => push_input(state, sid, AcpInput::Cancel, SessionInput::Cancel).await,

        AgentAction::Approve { call_id, decision } => {
            push_input(
                state,
                sid,
                AcpInput::Approve {
                    call_id: call_id.clone(),
                    decision,
                },
                SessionInput::Approve { call_id, decision },
            )
            .await
        }

        AgentAction::ListMedia { query } => {
            if !session_exists(state, sid).await {
                return SharedResponse::Failed(SharedFailure::NotFound);
            }
            if query.len() > super::super::server::MAX_MEDIA_QUERY_BYTES {
                return SharedResponse::Failed(SharedFailure::Invalid);
            }
            match tokio::task::spawn_blocking(move || {
                super::super::server::remote_media_entries(&query)
            })
            .await
            {
                Ok(entries) => SharedResponse::Media(entries),
                Err(_) => SharedResponse::Failed(SharedFailure::Internal),
            }
        }
    }
}

async fn sessions(state: &RemoteState) -> Vec<RemoteSession> {
    let mut sessions = state.agents.lock().await.remote_sessions();
    sessions.extend(state.acp.lock().await.remote_sessions());
    for session in &mut sessions {
        if let Some(messages) = super::super::server::session_messages(state, &session.sid).await {
            session.title = vmux_wire::room::Message::conversation_title(&messages, &session.name);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
    sessions
}

async fn session_exists(state: &RemoteState, sid: &str) -> bool {
    state.acp.lock().await.contains(sid) || state.agents.lock().await.remote_session(sid).is_some()
}

async fn push_input(
    state: &RemoteState,
    sid: &str,
    acp: AcpInput,
    page: SessionInput,
) -> SharedResponse {
    if state.acp.lock().await.contains(sid) {
        state.acp.lock().await.input(sid, acp);
        return SharedResponse::Ok;
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(sid).is_none() {
        return SharedResponse::Failed(SharedFailure::NotFound);
    }
    agents.input(sid, page);
    SharedResponse::Ok
}

async fn prompt(
    state: &RemoteState,
    sid: &str,
    text: String,
    context: Option<String>,
    attachments: Vec<vmux_wire::protocol::AgentAttachment>,
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
        },
        SessionInput::User { text, attachments },
    )
    .await
}

fn new_chat_op_id(command: &SharedAgentCommand) -> Option<ClientOpId> {
    match command {
        SharedAgentCommand::NewAgentChat { client_op_id, .. } => Some(client_op_id.clone()),
        SharedAgentCommand::ListAgents
        | SharedAgentCommand::ListTeam
        | SharedAgentCommand::ListModels { .. }
        | SharedAgentCommand::SelectModel { .. }
        | SharedAgentCommand::SetEffort { .. } => None,
    }
}

async fn broker(state: &RemoteState, command: SharedAgentCommand) -> SharedResponse {
    use crate::protocol::AgentCommandResult;
    match super::super::server::broker_result(state, command.into()).await {
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
    state.client_ops.lock().await.claim(client_op_id.clone())
}

async fn release(state: &RemoteState, client_op_id: &ClientOpId) {
    state.client_ops.lock().await.release(client_op_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tokio::sync::{Mutex, broadcast};

    fn empty_state() -> RemoteState {
        let (agent_tx, _) = broadcast::channel(8);
        RemoteState {
            token: Arc::from("token"),
            paired: Arc::new(AtomicBool::new(false)),
            agents: Arc::new(Mutex::new(Default::default())),
            acp: Arc::new(Mutex::new(Default::default())),
            broker: crate::agent_broker::AgentBroker::new(
                agent_tx,
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            client_ops: Arc::new(Mutex::new(Default::default())),
        }
    }

    fn prompt_of(length: usize) -> SharedMessage {
        SharedMessage::agent(
            "s",
            AgentAction::Input {
                text: "x".repeat(length),
                context: None,
                attachments: Vec::new(),
            },
        )
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
            SharedMessage::agent(
                "s",
                AgentAction::Input {
                    text: "   ".into(),
                    context: None,
                    attachments: Vec::new(),
                },
            ),
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

        let response = dispatch(
            &state,
            SharedMessage::AgentCommand(SharedAgentCommand::ListAgents),
        )
        .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::NoDesktop)
        ));
    }

    #[tokio::test]
    async fn operations_on_an_unknown_session_report_not_found() {
        let state = empty_state();

        for request in [
            SharedMessage::agent("ghost", AgentAction::Cancel),
            SharedMessage::agent("ghost", AgentAction::Attach),
            SharedMessage::agent(
                "ghost",
                AgentAction::ListMedia {
                    query: String::new(),
                },
            ),
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
    async fn an_unbounded_client_op_id_is_refused_before_it_is_claimed() {
        let state = empty_state();
        let oversized = ClientOpId::new("x".repeat(4096));

        let response = dispatch(
            &state,
            SharedMessage::AgentCommand(SharedAgentCommand::NewAgentChat {
                client_op_id: oversized.clone(),
                prompt: "hello".into(),
                agent_url: None,
            }),
        )
        .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        assert!(
            claim_once(&state, &oversized).await,
            "a refused id must never have been claimed"
        );
    }

    #[tokio::test]
    async fn a_client_op_id_can_only_be_claimed_once_but_is_reusable_after_release() {
        let state = empty_state();
        let id = ClientOpId::new("op-1");

        assert!(claim_once(&state, &id).await);
        assert!(!claim_once(&state, &id).await);

        release(&state, &id).await;

        assert!(
            claim_once(&state, &id).await,
            "a released op must be retryable"
        );
    }
}
