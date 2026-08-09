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
    SharedMessage::agent(
        "s",
        AgentAction::Input {
            text: "x".repeat(length),
            context: None,
            attachments: Vec::new(),
        },
    )
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

/// A claimed id is retained until 4096 newer ones evict it, so its length is memory the
/// sender chooses. The HTTP handlers used to bound it; deleting them moved the check here,
/// and the second assertion is the point — refusing after the claim would still retain it.
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
