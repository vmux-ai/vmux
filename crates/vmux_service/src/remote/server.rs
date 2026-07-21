use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;
use tokio::sync::{Mutex, broadcast};

use crate::acp::{AcpInput, AcpSessionManager};
use crate::agent::{AgentSessionManager, SessionInput};
use crate::agent_broker::AgentBroker;
use crate::message::Message;
use crate::protocol::{AgentAttachment, ApprovalDecision, ServiceMessage};
use crate::remote::{
    ApprovalRequest, NewChatRequest, PromptRequest, RemoteApproval, RemoteEvent, RemoteMediaEntry,
    RemoteSession, RemoteStatus,
};

const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 16;
const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ATTACHMENT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MEDIA_QUERY_BYTES: usize = 4 * 1024;
const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;
const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 96;

#[derive(Deserialize)]
struct MediaQuery {
    #[serde(default)]
    query: String,
}

#[derive(Clone)]
struct RemoteState {
    token: Arc<str>,
    paired: Arc<AtomicBool>,
    agents: Arc<Mutex<AgentSessionManager>>,
    acp: Arc<Mutex<AcpSessionManager>>,
    broker: AgentBroker,
}

pub fn spawn(
    agents: Arc<Mutex<AgentSessionManager>>,
    acp: Arc<Mutex<AcpSessionManager>>,
    broker: AgentBroker,
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
            paired: Arc::new(AtomicBool::new(crate::remote_paired_path().exists())),
            agents,
            acp,
            broker,
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
        .route("/api/sessions/{sid}/media", get(list_media))
        .route("/api/sessions/{sid}/messages", post(send_prompt))
        .route("/api/chats", post(create_chat))
        .route("/api/sessions/{sid}/cancel", post(cancel))
        .route("/api/sessions/{sid}/approval", post(approve))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize));
    Router::new().merge(api).with_state(state)
}

async fn create_chat(
    State(state): State<RemoteState>,
    Json(request): Json<NewChatRequest>,
) -> StatusCode {
    let prompt = request.text.trim();
    if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES {
        return StatusCode::BAD_REQUEST;
    }
    let command = crate::protocol::AgentCommand::NewAgentChat {
        prompt: prompt.to_string(),
    };
    match state
        .broker
        .command(crate::protocol::AgentRequestId::new(), None, command)
        .await
    {
        Ok(crate::protocol::AgentCommandResult::Ok) => StatusCode::ACCEPTED,
        Ok(crate::protocol::AgentCommandResult::Error(_)) | Err(_) => StatusCode::BAD_GATEWAY,
        Ok(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn authorize(State(state): State<RemoteState>, request: Request, next: Next) -> Response {
    if !remote_enabled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if request_token(request.headers()).is_some_and(|token| secure_eq(token, &state.token)) {
        mark_paired(&state.paired);
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn remote_enabled() -> bool {
    remote_enabled_at(&crate::remote_state_path())
}

fn remote_enabled_at(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|state| state.trim() == "enabled")
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
    for session in &mut sessions {
        if let Some(messages) = session_messages(&state, &session.sid).await {
            session.title = vmux_remote::conversation_title(&messages, &session.name);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
    Json(sessions)
}

async fn send_prompt(
    State(state): State<RemoteState>,
    AxumPath(sid): AxumPath<String>,
    Json(request): Json<PromptRequest>,
) -> StatusCode {
    let text = request.text.trim();
    let Some(attachments) = validate_remote_attachments(request.attachments) else {
        return StatusCode::BAD_REQUEST;
    };
    if (text.is_empty() && attachments.is_empty()) || text.len() > MAX_PROMPT_BYTES {
        return StatusCode::BAD_REQUEST;
    }
    if state.acp.lock().await.contains(&sid) {
        state.acp.lock().await.input(
            &sid,
            AcpInput::User {
                text: text.to_string(),
                context: None,
                attachments,
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
            attachments,
        },
    );
    StatusCode::ACCEPTED
}

async fn list_media(
    State(state): State<RemoteState>,
    AxumPath(sid): AxumPath<String>,
    Query(query): Query<MediaQuery>,
) -> Result<Json<Vec<RemoteMediaEntry>>, StatusCode> {
    if query.query.len() > MAX_MEDIA_QUERY_BYTES {
        return Err(StatusCode::BAD_REQUEST);
    }
    let exists = state.acp.lock().await.contains(&sid)
        || state.agents.lock().await.remote_session(&sid).is_some();
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    let entries = tokio::task::spawn_blocking(move || remote_media_entries(&query.query))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(entries))
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
    let disconnect_check = tokio::time::interval(Duration::from_secs(1));
    let live = stream::unfold((receiver, disconnect_check), move |stream_state| {
        let remote_state = live_state.clone();
        let sid = live_sid.clone();
        async move {
            let (mut receiver, mut disconnect_check) = stream_state;
            loop {
                tokio::select! {
                    _ = disconnect_check.tick() => {
                        if !remote_enabled() {
                            return None;
                        }
                    }
                    message = receiver.recv() => match message {
                        Ok(message) => {
                            if let Some(event) = service_event(&remote_state, &sid, message).await {
                                return Some((
                                    remote_sse(event),
                                    (receiver, disconnect_check),
                                ));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            if let Some(messages) = session_messages(&remote_state, &sid).await {
                                return Some((
                                    remote_sse(RemoteEvent::Snapshot { messages }),
                                    (receiver, disconnect_check),
                                ));
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
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
        if let Some(mut session) = acp.remote_session(sid) {
            let messages = acp.remote_messages(sid)?;
            session.title = vmux_remote::conversation_title(&messages, &session.name);
            return Some((session, messages, acp.subscribe(sid)?));
        }
    }
    let agents = state.agents.lock().await;
    let mut session = agents.remote_session(sid)?;
    let messages = agents.remote_messages(sid).await?;
    session.title = vmux_remote::conversation_title(&messages, &session.name);
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
    let acp_session = {
        let acp = state.acp.lock().await;
        acp.remote_session(sid)
    };
    let mut session = if let Some(session) = acp_session {
        session
    } else {
        state.agents.lock().await.remote_session(sid)?
    };
    if let Some(messages) = session_messages(state, sid).await {
        session.title = vmux_remote::conversation_title(&messages, &session.name);
    }
    Some(session)
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
        ServiceMessage::AgentApprovalResolved { .. } => {
            Some(RemoteEvent::Approval { approval: None })
        }
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

fn validate_remote_attachments(attachments: Vec<AgentAttachment>) -> Option<Vec<AgentAttachment>> {
    if attachments.len() > MAX_ATTACHMENTS {
        return None;
    }
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)?
        .canonicalize()
        .ok()?;
    let mut total = 0_u64;
    attachments
        .into_iter()
        .map(|attachment| {
            let path = std::path::PathBuf::from(&attachment.path)
                .canonicalize()
                .ok()?;
            if !path.starts_with(&home) {
                return None;
            }
            let metadata = path.metadata().ok()?;
            if !metadata.is_file() || metadata.len() > MAX_ATTACHMENT_BYTES {
                return None;
            }
            total = total.checked_add(metadata.len())?;
            if total > MAX_ATTACHMENT_TOTAL_BYTES {
                return None;
            }
            Some(AgentAttachment {
                name: path.file_name()?.to_string_lossy().into_owned(),
                mime_type: attachment_mime(&path),
                path: path.to_string_lossy().into_owned(),
                size: metadata.len(),
            })
        })
        .collect()
}

fn attachment_mime(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    if let Some(mime) = vmux_core::media::media_mime(&path_str) {
        return mime.to_string();
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "tif" | "tiff" => "image/tiff",
        "heic" | "heif" => "image/heic",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "md" | "markdown" => "text/markdown",
        "txt" | "rs" | "toml" | "ron" | "yaml" | "yml" | "js" | "ts" | "tsx" | "jsx" | "css"
        | "sh" | "zsh" | "bash" | "py" | "go" | "c" | "h" | "cc" | "cpp" | "hpp" | "java"
        | "kt" | "swift" => "text/plain",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn media_thumbnail_data_url(path: &std::path::Path, source_size: u64) -> String {
    if source_size > MEDIA_THUMBNAIL_SOURCE_LIMIT {
        return String::new();
    }
    let Some(mime) = vmux_core::media::image_mime(&path.to_string_lossy()) else {
        return String::new();
    };
    if mime == "image/svg+xml" || mime == "image/avif" {
        return String::new();
    }
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let Ok(image) = image::load_from_memory(&bytes) else {
        return String::new();
    };
    let thumbnail = image.thumbnail(MEDIA_THUMBNAIL_MAX_EDGE, MEDIA_THUMBNAIL_MAX_EDGE);
    let mut output = std::io::Cursor::new(Vec::new());
    if thumbnail
        .write_to(&mut output, image::ImageFormat::Png)
        .is_err()
    {
        return String::new();
    }
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(output.into_inner())
    )
}

fn decode_media_query_path(value: &str) -> std::path::PathBuf {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                char::from(bytes[index + 1]).to_digit(16),
                char::from(bytes[index + 2]).to_digit(16),
            )
        {
            decoded.push(((high << 4) | low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    std::path::PathBuf::from(String::from_utf8_lossy(&decoded).into_owned())
}

fn remote_media_entries(query: &str) -> Vec<RemoteMediaEntry> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Vec::new();
    };
    let candidate = if let Some(rest) = query.strip_prefix("file://") {
        decode_media_query_path(rest)
    } else if let Some(rest) = query.strip_prefix("~/") {
        home.join(decode_media_query_path(rest))
    } else if query == "~" {
        home.clone()
    } else {
        let path = decode_media_query_path(query);
        if path.is_absolute() {
            path
        } else {
            home.join(path)
        }
    };
    let query_is_dir = query.is_empty() || query.ends_with('/') || candidate.is_dir();
    let (directory, filter) = if query_is_dir {
        (candidate, String::new())
    } else {
        (
            candidate
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| home.clone()),
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase(),
        )
    };
    let Ok(home) = home.canonicalize() else {
        return Vec::new();
    };
    let Ok(directory) = directory.canonicalize() else {
        return Vec::new();
    };
    if !directory.starts_with(&home) {
        return Vec::new();
    }
    let mut entries = std::fs::read_dir(&directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.')
                || (!filter.is_empty() && !name.to_ascii_lowercase().contains(&filter))
            {
                return None;
            }
            let is_dir = entry.file_type().ok()?.is_dir();
            let metadata = (!is_dir).then(|| path.metadata().ok()).flatten();
            let mime_type = if is_dir {
                String::new()
            } else {
                attachment_mime(&path)
            };
            if !is_dir
                && !mime_type.starts_with("image/")
                && !mime_type.starts_with("audio/")
                && !mime_type.starts_with("video/")
                && mime_type != "application/pdf"
            {
                return None;
            }
            let parent = path
                .parent()
                .and_then(|parent| parent.strip_prefix(&home).ok())
                .map(|parent| {
                    if parent.as_os_str().is_empty() {
                        "~".to_string()
                    } else {
                        format!("~/{}", parent.to_string_lossy())
                    }
                })
                .unwrap_or_else(|| "~".to_string());
            Some(RemoteMediaEntry {
                path: path.to_string_lossy().into_owned(),
                name,
                parent,
                mime_type,
                size: metadata.map(|metadata| metadata.len()).unwrap_or_default(),
                is_dir,
                preview_data_url: String::new(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right.is_dir.cmp(&left.is_dir).then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        })
    });
    entries.truncate(100);
    let mut remaining_thumbnail_bytes = MEDIA_THUMBNAIL_TOTAL_LIMIT;
    for entry in &mut entries {
        if entry.is_dir || !entry.mime_type.starts_with("image/") {
            continue;
        }
        if entry.size > remaining_thumbnail_bytes {
            continue;
        }
        entry.preview_data_url =
            media_thumbnail_data_url(std::path::Path::new(&entry.path), entry.size);
        if !entry.preview_data_url.is_empty() {
            remaining_thumbnail_bytes = remaining_thumbnail_bytes.saturating_sub(entry.size);
        }
    }
    entries
}

fn mark_paired(paired: &AtomicBool) {
    if paired.swap(true, Ordering::AcqRel) {
        return;
    }
    let path = crate::remote_paired_path();
    let result = path
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| std::fs::write(&path, b"paired\n"));
    if let Err(error) = result {
        paired.store(false, Ordering::Release);
        tracing::warn!(%error, "remote: failed to record paired phone");
    }
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
    let _ = std::fs::remove_file(crate::remote_paired_path());
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

    #[test]
    fn remote_state_requires_enabled_marker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-state");
        assert!(!remote_enabled_at(&path));
        std::fs::write(&path, b"disabled\n").unwrap();
        assert!(!remote_enabled_at(&path));
        std::fs::write(&path, b"enabled\n").unwrap();
        assert!(remote_enabled_at(&path));
    }

    #[test]
    fn media_query_paths_decode_percent_escapes() {
        assert_eq!(
            decode_media_query_path("Pictures/My%20Photo.png"),
            std::path::PathBuf::from("Pictures/My Photo.png")
        );
    }

    #[test]
    fn remote_attachments_are_count_limited_before_file_access() {
        let attachments = (0..=MAX_ATTACHMENTS)
            .map(|index| AgentAttachment {
                path: format!("/missing/{index}"),
                name: format!("{index}.png"),
                mime_type: "image/png".into(),
                size: 1,
            })
            .collect();
        assert!(validate_remote_attachments(attachments).is_none());
    }
}
