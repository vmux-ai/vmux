use std::convert::Infallible;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path as AxumPath, Request, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY,
    SET_COOKIE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
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
    ApprovalRequest, PairRequest, PromptRequest, RemoteApproval, RemoteEvent, RemoteSession,
    RemoteStatus,
};

const COOKIE_NAME: &str = "vmux_remote";
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const APP_ICON: &[u8] = include_bytes!("../../../../icon.png");

#[derive(Clone)]
struct RemoteState {
    token: Arc<str>,
    agents: Arc<Mutex<AgentSessionManager>>,
    acp: Arc<Mutex<AcpSessionManager>>,
    web_root: Arc<PathBuf>,
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
            web_root: Arc::new(remote_web_root()),
        };
        let address = (std::net::Ipv4Addr::LOCALHOST, crate::remote_port());
        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(%error, port = crate::remote_port(), "remote: bind failed");
                return;
            }
        };
        tracing::info!(
            port = crate::remote_port(),
            root = %state.web_root.display(),
            "remote: mobile app ready"
        );
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
    Router::new()
        .route("/api/pair", post(pair))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
        .route("/icon.png", get(icon))
        .route("/", get(index))
        .route("/{*path}", get(asset))
        .merge(api)
        .with_state(state)
}

async fn authorize(State(state): State<RemoteState>, request: Request, next: Next) -> Response {
    if request_token(request.headers()).is_some_and(|token| secure_eq(token, &state.token)) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn request_token(headers: &HeaderMap) -> Option<&str> {
    if let Some(value) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        && let Some(token) = value.strip_prefix("Bearer ")
    {
        return Some(token.trim());
    }
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == COOKIE_NAME).then_some(value)
            })
        })
}

async fn pair(State(state): State<RemoteState>, Json(request): Json<PairRequest>) -> Response {
    if !secure_eq(request.token.trim(), &state.token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{COOKIE_NAME}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000",
            state.token
        ))
        .unwrap(),
    );
    response
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

async fn index(State(state): State<RemoteState>) -> Response {
    static_response(&state, "index.html").await
}

async fn asset(State(state): State<RemoteState>, AxumPath(path): AxumPath<String>) -> Response {
    if !safe_relative_path(&path) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let file = state.web_root.join(&path);
    if file.is_file() {
        return file_response(&file).await;
    }
    if Path::new(&path).extension().is_none() {
        return static_response(&state, "index.html").await;
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn static_response(state: &RemoteState, path: &str) -> Response {
    let file = state.web_root.join(path);
    if file.is_file() {
        file_response(&file).await
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Mobile web bundle missing. Build vmux_server first.",
        )
            .into_response()
    }
}

async fn file_response(path: &Path) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let content_type = mime_guess::from_path(path).first_or_octet_stream();
            secured_response(
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, content_type.as_ref())
                    .header(
                        CACHE_CONTROL,
                        if path.file_name().is_some_and(|name| name == "index.html") {
                            "no-cache"
                        } else {
                            "public, max-age=31536000, immutable"
                        },
                    )
                    .body(Body::from(bytes))
                    .unwrap(),
            )
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn manifest() -> Response {
    let body = serde_json::json!({
        "name": "Vmux Remote",
        "short_name": "Vmux",
        "description": "Continue Vmux agent chats from your phone.",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": "#0b0b0d",
        "theme_color": "#0b0b0d",
        "icons": [{
            "src": "/icon.png",
            "sizes": "512x512",
            "type": "image/png",
            "purpose": "any maskable"
        }]
    });
    secured_response(
        Response::builder()
            .header(CONTENT_TYPE, "application/manifest+json")
            .header(CACHE_CONTROL, "no-cache")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
}

async fn service_worker() -> Response {
    const SOURCE: &str = r#"self.addEventListener('install', event => event.waitUntil(self.skipWaiting()));
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('fetch', event => event.respondWith(fetch(event.request)));
"#;
    secured_response(
        Response::builder()
            .header(CONTENT_TYPE, "text/javascript; charset=utf-8")
            .header(CACHE_CONTROL, "no-cache")
            .body(Body::from(SOURCE))
            .unwrap(),
    )
}

async fn icon() -> Response {
    secured_response(
        Response::builder()
            .header(CONTENT_TYPE, "image/png")
            .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(Body::from(APP_ICON))
            .unwrap(),
    )
}

fn secured_response(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; font-src 'self'; base-uri 'self'; frame-ancestors 'none'",
        ),
    );
    response
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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

fn remote_web_root() -> PathBuf {
    if let Ok(executable) = std::env::current_exe() {
        for ancestor in executable.ancestors() {
            if ancestor.extension().and_then(|value| value.to_str()) == Some("app") {
                let root = ancestor.join("Contents/Resources/webview-apps/_shared");
                if root.is_dir() {
                    return root;
                }
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vmux_server/dist")
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
    fn request_token_accepts_bearer_and_cookie() {
        let mut bearer = HeaderMap::new();
        bearer.insert(AUTHORIZATION, HeaderValue::from_static("Bearer secret"));
        assert_eq!(request_token(&bearer), Some("secret"));

        let mut cookie = HeaderMap::new();
        cookie.insert(
            "cookie",
            HeaderValue::from_static("a=1; vmux_remote=secret"),
        );
        assert_eq!(request_token(&cookie), Some("secret"));
    }

    #[test]
    fn static_path_rejects_traversal() {
        assert!(safe_relative_path("assets/app.js"));
        assert!(!safe_relative_path("../secret"));
        assert!(!safe_relative_path("/absolute"));
    }
}
