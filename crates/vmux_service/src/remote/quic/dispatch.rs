//! The single place a remote request turns into an action.
//!
//! The HTTP path spread this across nine handlers, each resolving `sid` and pushing into a
//! registry itself. That was six separate calls into `AcpInput`/`SessionInput`, so every limit —
//! prompt size, replay dedup, attachment confinement — had to be remembered at each one. Here
//! there is one entry point, so a control that is applied once is applied everywhere.

use vmux_wire::protocol::{SharedAgentCommand, SharedFailure, SharedMessage, SharedResponse};
use vmux_wire::room::{ClientOpId, RemoteSession};

use super::super::server::{MAX_PROMPT_BYTES, RemoteState};
use crate::acp::AcpInput;
use crate::agent::SessionInput;

/// Turn one request into one response.
///
/// Every branch that mutates a session funnels through [`push_input`], and every branch that
/// needs the GUI funnels through the broker, so there is no path that skips a check.
pub(crate) async fn dispatch(state: &RemoteState, request: SharedMessage) -> SharedResponse {
    match request {
        SharedMessage::ListSessions => SharedResponse::Sessions(sessions(state).await),

        SharedMessage::ListMedia { sid, query } => {
            if !session_exists(state, &sid).await {
                return SharedResponse::Failed(SharedFailure::NotFound);
            }
            if query.len() > super::super::server::MAX_MEDIA_QUERY_BYTES {
                return SharedResponse::Failed(SharedFailure::Invalid);
            }
            // The filesystem walk confines itself to $HOME; blocking, so off the reactor.
            match tokio::task::spawn_blocking(move || {
                super::super::server::remote_media_entries(&query)
            })
            .await
            {
                Ok(entries) => SharedResponse::Media(entries),
                Err(_) => SharedResponse::Failed(SharedFailure::Internal),
            }
        }

        SharedMessage::AgentCommand(command) => {
            // Only the chat-creating command is not idempotent, so only it needs a claim.
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
                // Released so a genuine failure stays retryable; without this a dropped GUI would
                // burn the id and the client could never try again.
                release(state, &client_op_id).await;
            }
            response
        }

        SharedMessage::AttachPageAgent { sid } => {
            if session_exists(state, &sid).await {
                SharedResponse::Ok
            } else {
                SharedResponse::Failed(SharedFailure::NotFound)
            }
        }

        SharedMessage::AgentInput { sid, text, context } => {
            prompt(state, sid, text, context, Vec::new()).await
        }

        SharedMessage::AgentInputWithAttachments {
            sid,
            text,
            context,
            attachments,
        } => prompt(state, sid, text, context, attachments).await,

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
    }
}

/// Sessions from both registries, titled from their transcripts, newest first.
async fn sessions(state: &RemoteState) -> Vec<RemoteSession> {
    let mut sessions = state.agents.lock().await.remote_sessions();
    sessions.extend(state.acp.lock().await.remote_sessions());
    for session in &mut sessions {
        if let Some(messages) = super::super::server::session_messages(state, &session.sid).await {
            session.title = vmux_wire::room::conversation_title(&messages, &session.name);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
    sessions
}

async fn session_exists(state: &RemoteState, sid: &str) -> bool {
    state.acp.lock().await.contains(sid) || state.agents.lock().await.remote_session(sid).is_some()
}

/// ACP first, then page agents — the same order the HTTP handlers resolved in, so a session id
/// that lives in both keeps resolving to the same one.
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
    sid: String,
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
        &sid,
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
        SharedAgentCommand::ListAgents | SharedAgentCommand::ListTeam => None,
    }
}

/// Ask the GUI. `NoDesktop` rather than a generic error, because that one resolves on its own as
/// soon as a window is open and a client should retry rather than surface it as broken.
async fn broker(state: &RemoteState, command: SharedAgentCommand) -> SharedResponse {
    use crate::protocol::AgentCommandResult;
    match super::super::server::broker_result(state, command.into()).await {
        Some(AgentCommandResult::Text(json)) => SharedResponse::BrokerJson(json),
        // A command the GUI carries out without answering, such as opening a new chat. It ran.
        Some(AgentCommandResult::Ok) | Some(AgentCommandResult::Layout(_)) => SharedResponse::Ok,
        Some(AgentCommandResult::Error(message)) => {
            tracing::warn!(%message, "remote quic: the GUI refused a brokered command");
            SharedResponse::Failed(SharedFailure::Invalid)
        }
        None => SharedResponse::Failed(SharedFailure::NoDesktop),
    }
}

/// Claim a `client_op_id`, so a retry after a dropped connection does not run the operation
/// twice. Released on every failure path, or a failed op could never be retried.
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

    /// No sessions and no GUI attached — enough to exercise every check that runs before a
    /// registry lookup, which is where the limits live.
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
        SharedMessage::AgentInput {
            sid: "s".into(),
            text: "x".repeat(length),
            context: None,
        }
    }

    /// The cap has to be enforced here rather than at the transport, because `receive_window`
    /// bounds buffering but says nothing about a single legitimate-looking frame.
    #[tokio::test]
    async fn an_oversized_prompt_is_refused_before_any_session_lookup() {
        let state = empty_state();

        let over = dispatch(&state, prompt_of(MAX_PROMPT_BYTES + 1)).await;
        let under = dispatch(&state, prompt_of(16)).await;

        assert!(matches!(
            over,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        // No such session, so this gets as far as the lookup and fails differently — which is the
        // point: the size check is not what rejected it.
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
            },
        )
        .await;

        assert!(matches!(
            response,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
    }

    /// A client needs to tell "this will never work" from "try again once a window is open".
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
            SharedMessage::AgentCancel {
                sid: "ghost".into(),
            },
            SharedMessage::AttachPageAgent {
                sid: "ghost".into(),
            },
            SharedMessage::ListMedia {
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

    /// Replay protection is what makes a retry after a dropped connection safe.
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
