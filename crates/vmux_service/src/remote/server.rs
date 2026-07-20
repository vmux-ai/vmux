use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, StreamExt};
use tokio::sync::{Mutex, broadcast};

use crate::acp::{AcpInput, AcpSessionManager};
use crate::agent::{AgentSessionManager, SessionInput};
use crate::message::Message;
use crate::protocol::{ApprovalDecision, ServiceMessage};
use crate::remote::{
    ApprovalRequest, PromptRequest, RemoteApproval, RemoteEvent, RemoteSession, RemoteStatus,
};

const MAX_PROMPT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
struct RemoteState {
    token: Arc<str>,
    agents: Arc<Mutex<AgentSessionManager>>,
    acp: Arc<Mutex<AcpSessionManager>>,
}

pub fn spawn(
    agents: Arc<Mutex<AgentSessionManager>>,
    acp: Arc<Mutex<AcpSessionManager>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let token = match ensure_token() {
            Ok(token) => token,
            Err(error) => {
                tracing::error!(%error, "remote: token setup failed");
                return;
            }
        };
        let state = RemoteState {
            token: Arc::from(token),
            agents,
            acp,
        };
        let address = (std::net::Ipv4Addr::LOCALHOST, crate::remote_port());
        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(%error, port = crate::remote_port(), "remote: bind failed");
                return;
            }
        };
        tracing::info!(port = crate::remote_port(), "remote: mobile API ready");
        if let Err(error) = axum::serve(listener, router(state)).await {
            tracing::error!(%error, "remote: server failed");
        }
    })
}

fn router(state: RemoteState) -> Router {
    let api = Router::new()
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{sid}/events", get(session_events))
        .route("/api/sessions/{sid}/messages", post(send_prompt))
        .route("/api/sessions/{sid}/cancel", post(cancel))
        .route("/api/sessions/{sid}/approval", post(approve))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize));
    Router::new().merge(api).with_state(state)
}

async fn authorize(State(state): State<RemoteState>, request: Request, next: Next) -> Response {
    if request_token(request.headers()).is_some_and(|token| secure_eq(token, &state.token)) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn request_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
}

async fn list_sessions(State(state): State<RemoteState>) -> Json<Vec<RemoteSession>> {
    let mut sessions = state.agents.lock().await.remote_sessions();
    sessions.extend(state.acp.lock().await.remote_sessions());
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
    Json(sessions)
}

async fn send_prompt(
    State(state): State<RemoteState>,
    AxumPath(sid): AxumPath<String>,
    Json(request): Json<PromptRequest>,
) -> StatusCode {
    let text = request.text.trim();
    if text.is_empty() || text.len() > MAX_PROMPT_BYTES {
        return StatusCode::BAD_REQUEST;
    }
    if state.acp.lock().await.contains(&sid) {
        state.acp.lock().await.input(
            &sid,
            AcpInput::User {
                text: text.to_string(),
                context: None,
                attachments: Vec::new(),
            },
        );
        return StatusCode::ACCEPTED;
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(&sid).is_none() {
        return StatusCode::NOT_FOUND;
    }
    agents.input(
        &sid,
        SessionInput::User {
            text: text.to_string(),
            attachments: Vec::new(),
        },
    );
    StatusCode::ACCEPTED
}

async fn cancel(State(state): State<RemoteState>, AxumPath(sid): AxumPath<String>) -> StatusCode {
    if state.acp.lock().await.contains(&sid) {
        state.acp.lock().await.input(&sid, AcpInput::Cancel);
        return StatusCode::ACCEPTED;
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(&sid).is_none() {
        return StatusCode::NOT_FOUND;
    }
    agents.input(&sid, SessionInput::Cancel);
    StatusCode::ACCEPTED
}

async fn approve(
    State(state): State<RemoteState>,
    AxumPath(sid): AxumPath<String>,
    Json(request): Json<ApprovalRequest>,
) -> StatusCode {
    let decision = if request.allow {
        ApprovalDecision::Allow
    } else {
        ApprovalDecision::Deny
    };
    if state.acp.lock().await.contains(&sid) {
        state.acp.lock().await.input(
            &sid,
            AcpInput::Approve {
                call_id: request.call_id,
                decision,
            },
        );
        return StatusCode::ACCEPTED;
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(&sid).is_none() {
        return StatusCode::NOT_FOUND;
    }
    agents.input(
        &sid,
        SessionInput::Approve {
            call_id: request.call_id,
            decision,
        },
    );
    StatusCode::ACCEPTED
}

async fn session_events(
    State(state): State<RemoteState>,
    AxumPath(sid): AxumPath<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let Some((session, messages, receiver)) = session_stream(&state, &sid).await else {
        return Err(StatusCode::NOT_FOUND);
    };
    let initial = vec![
        remote_sse(RemoteEvent::Session { session }),
        remote_sse(RemoteEvent::Snapshot { messages }),
    ];
    let live_state = state.clone();
    let live_sid = sid.clone();
    let live = stream::unfold(receiver, move |mut receiver| {
        let state = live_state.clone();
        let sid = live_sid.clone();
        async move {
            loop {
                match receiver.recv().await {
                    Ok(message) => {
                        if let Some(event) = service_event(&state, &sid, message).await {
                            return Some((remote_sse(event), receiver));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Some(messages) = session_messages(&state, &sid).await {
                            return Some((
                                remote_sse(RemoteEvent::Snapshot { messages }),
                                receiver,
                            ));
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        }
    });
    Ok(Sse::new(stream::iter(initial).chain(live)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

async fn session_stream(
    state: &RemoteState,
    sid: &str,
) -> Option<(
    RemoteSession,
    Vec<Message>,
    broadcast::Receiver<ServiceMessage>,
)> {
    {
        let acp = state.acp.lock().await;
        if let Some(session) = acp.remote_session(sid) {
            return Some((session, acp.remote_messages(sid)?, acp.subscribe(sid)?));
        }
    }
    let agents = state.agents.lock().await;
    let session = agents.remote_session(sid)?;
    let messages = agents.remote_messages(sid).await?;
    Some((session, messages, agents.subscribe(sid)?))
}

async fn session_messages(state: &RemoteState, sid: &str) -> Option<Vec<Message>> {
    {
        let acp = state.acp.lock().await;
        if let Some(messages) = acp.remote_messages(sid) {
            return Some(messages);
        }
    }
    state.agents.lock().await.remote_messages(sid).await
}

async fn current_session(state: &RemoteState, sid: &str) -> Option<RemoteSession> {
    if let Some(session) = state.acp.lock().await.remote_session(sid) {
        return Some(session);
    }
    state.agents.lock().await.remote_session(sid)
}

async fn service_event(
    state: &RemoteState,
    sid: &str,
    message: ServiceMessage,
) -> Option<RemoteEvent> {
    match message {
        ServiceMessage::AgentDelta { text, .. } => Some(RemoteEvent::Delta { text }),
        ServiceMessage::AgentRunStatusChanged { status, .. } => Some(RemoteEvent::Status {
            status: RemoteStatus::from(&status),
        }),
        ServiceMessage::AgentAwaitingApproval {
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
        ServiceMessage::AgentMessagesSnapshot { messages_json, .. } => {
            serde_json::from_str(&messages_json)
                .ok()
                .map(|messages| RemoteEvent::Snapshot { messages })
        }
        ServiceMessage::AcpAgentInfo { .. }
        | ServiceMessage::AcpModelInfo { .. }
        | ServiceMessage::AcpWorkspaceChanged { .. } => current_session(state, sid)
            .await
            .map(|session| RemoteEvent::Session { session }),
        _ => None,
    }
}

fn remote_sse(event: RemoteEvent) -> Result<Event, Infallible> {
    Ok(Event::default().data(serde_json::to_string(&event).unwrap()))
}

fn secure_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn ensure_token() -> std::io::Result<String> {
    let path = crate::remote_token_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if existing.len() >= 32 {
            return Ok(existing.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    std::fs::write(&path, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_comparison_requires_exact_token() {
        assert!(secure_eq("abc", "abc"));
        assert!(!secure_eq("abc", "abd"));
        assert!(!secure_eq("abc", "ab"));
    }

    #[test]
    fn request_token_accepts_bearer() {
        let mut bearer = HeaderMap::new();
        bearer.insert(AUTHORIZATION, "Bearer secret".parse().unwrap());
        assert_eq!(request_token(&bearer), Some("secret"));
    }
}
