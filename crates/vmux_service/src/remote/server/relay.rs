use std::time::Duration;

use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;

const SSE_BUFFER_LIMIT: usize = 2 * 1024 * 1024;
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct DesktopCommand {
    id: String,
    kind: DesktopCommandKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DesktopCommandKind {
    ListSessions,
    CreateChat { body: Value },
    SendPrompt { sid: String, body: Value },
    Cancel { sid: String },
    Approve { sid: String, body: Value },
    ListMedia { sid: String, query: String },
    SubscribeSession { sid: String, stream_id: String },
}

#[derive(Debug, Serialize)]
struct DesktopResponse {
    status: u16,
    body: Value,
}

pub(super) fn spawn(state: RemoteState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            if !remote_enabled() {
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            }
            let Some(relay_url) = configured_relay_url() else {
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            };
            let device_id = match ensure_device_id() {
                Ok(device_id) => device_id,
                Err(error) => {
                    tracing::warn!(%error, "remote relay: failed to create device id");
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    continue;
                }
            };
            tracing::info!(%relay_url, %device_id, "remote relay: connecting");
            if let Err(error) = command_loop(&client, &relay_url, &device_id, state.clone()).await {
                tracing::warn!(%error, "remote relay: disconnected");
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    })
}

async fn command_loop(
    client: &reqwest::Client,
    relay_url: &str,
    device_id: &str,
    state: RemoteState,
) -> Result<(), String> {
    let endpoint = format!("{relay_url}/desktop/{device_id}/commands");
    let response = client
        .get(&endpoint)
        .bearer_auth(state.token.as_ref())
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "command stream returned HTTP {}",
            response.status()
        ));
    }
    tracing::info!("remote relay: command stream open");
    let mut parser = SseParser::default();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        for payload in parser.push(&chunk)? {
            let command: DesktopCommand =
                serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            let client = client.clone();
            let relay_url = relay_url.to_string();
            let device_id = device_id.to_string();
            let state = state.clone();
            tokio::spawn(async move {
                handle_command(&client, &relay_url, &device_id, state, command).await;
            });
        }
    }
    Ok(())
}

async fn handle_command(
    client: &reqwest::Client,
    relay_url: &str,
    device_id: &str,
    state: RemoteState,
    command: DesktopCommand,
) {
    let token = state.token.clone();
    let response = match command.kind {
        DesktopCommandKind::ListSessions => list_sessions_response(state).await,
        DesktopCommandKind::CreateChat { body } => create_chat_response(state, body).await,
        DesktopCommandKind::SendPrompt { sid, body } => {
            send_prompt_response(state, sid, body).await
        }
        DesktopCommandKind::Cancel { sid } => cancel_response(state, sid).await,
        DesktopCommandKind::Approve { sid, body } => approve_response(state, sid, body).await,
        DesktopCommandKind::ListMedia { sid, query } => {
            list_media_response(state, sid, query).await
        }
        DesktopCommandKind::SubscribeSession { sid, stream_id } => {
            subscribe_session(
                client.clone(),
                relay_url.to_string(),
                device_id.to_string(),
                state,
                sid,
                stream_id,
            )
            .await;
            return;
        }
    };
    let endpoint = format!("{relay_url}/desktop/{device_id}/responses/{}", command.id);
    if let Err(error) = client
        .post(endpoint)
        .bearer_auth(token.as_ref())
        .json(&response)
        .send()
        .await
    {
        tracing::warn!(%error, "remote relay: failed to post command response");
    }
}

async fn list_sessions_response(state: RemoteState) -> DesktopResponse {
    let mut sessions = state.agents.lock().await.remote_sessions();
    sessions.extend(state.acp.lock().await.remote_sessions());
    for session in &mut sessions {
        if let Some(messages) = session_messages(&state, &session.sid).await {
            session.title = vmux_remote::conversation_title(&messages, &session.name);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
    json_response(StatusCode::OK, sessions)
}

async fn create_chat_response(state: RemoteState, body: Value) -> DesktopResponse {
    let Ok(request) = serde_json::from_value::<NewChatRequest>(body) else {
        return status_response(StatusCode::BAD_REQUEST);
    };
    let prompt = request.text.trim();
    if prompt.is_empty()
        || prompt.len() > MAX_PROMPT_BYTES
        || !valid_client_op_id(&request.client_op_id)
    {
        return status_response(StatusCode::BAD_REQUEST);
    }
    if !state
        .client_ops
        .lock()
        .await
        .claim(request.client_op_id.clone())
    {
        return status_response(StatusCode::ACCEPTED);
    }
    let command = crate::protocol::AgentCommand::NewAgentChat {
        prompt: prompt.to_string(),
    };
    match state
        .broker
        .command(crate::protocol::AgentRequestId::new(), None, command)
        .await
    {
        Ok(crate::protocol::AgentCommandResult::Ok) => status_response(StatusCode::ACCEPTED),
        Ok(crate::protocol::AgentCommandResult::Error(_)) | Err(_) => {
            state.client_ops.lock().await.release(&request.client_op_id);
            status_response(StatusCode::BAD_GATEWAY)
        }
        Ok(_) => {
            state.client_ops.lock().await.release(&request.client_op_id);
            status_response(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn send_prompt_response(state: RemoteState, sid: String, body: Value) -> DesktopResponse {
    let Ok(request) = serde_json::from_value::<PromptRequest>(body) else {
        return status_response(StatusCode::BAD_REQUEST);
    };
    let text = request.text.trim();
    let Some(attachments) = validate_remote_attachments(request.attachments) else {
        return status_response(StatusCode::BAD_REQUEST);
    };
    if (text.is_empty() && attachments.is_empty())
        || text.len() > MAX_PROMPT_BYTES
        || !valid_client_op_id(&request.client_op_id)
    {
        return status_response(StatusCode::BAD_REQUEST);
    }
    if !state
        .client_ops
        .lock()
        .await
        .claim(request.client_op_id.clone())
    {
        return status_response(StatusCode::ACCEPTED);
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
        return status_response(StatusCode::ACCEPTED);
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(&sid).is_none() {
        drop(agents);
        state.client_ops.lock().await.release(&request.client_op_id);
        return status_response(StatusCode::NOT_FOUND);
    }
    agents.input(
        &sid,
        SessionInput::User {
            text: text.to_string(),
            attachments,
        },
    );
    status_response(StatusCode::ACCEPTED)
}

async fn cancel_response(state: RemoteState, sid: String) -> DesktopResponse {
    if state.acp.lock().await.contains(&sid) {
        state.acp.lock().await.input(&sid, AcpInput::Cancel);
        return status_response(StatusCode::ACCEPTED);
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(&sid).is_none() {
        return status_response(StatusCode::NOT_FOUND);
    }
    agents.input(&sid, SessionInput::Cancel);
    status_response(StatusCode::ACCEPTED)
}

async fn approve_response(state: RemoteState, sid: String, body: Value) -> DesktopResponse {
    let Ok(request) = serde_json::from_value::<ApprovalRequest>(body) else {
        return status_response(StatusCode::BAD_REQUEST);
    };
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
        return status_response(StatusCode::ACCEPTED);
    }
    let agents = state.agents.lock().await;
    if agents.remote_session(&sid).is_none() {
        return status_response(StatusCode::NOT_FOUND);
    }
    agents.input(
        &sid,
        SessionInput::Approve {
            call_id: request.call_id,
            decision,
        },
    );
    status_response(StatusCode::ACCEPTED)
}

async fn list_media_response(state: RemoteState, sid: String, query: String) -> DesktopResponse {
    if query.len() > MAX_MEDIA_QUERY_BYTES {
        return status_response(StatusCode::BAD_REQUEST);
    }
    let exists = state.acp.lock().await.contains(&sid)
        || state.agents.lock().await.remote_session(&sid).is_some();
    if !exists {
        return status_response(StatusCode::NOT_FOUND);
    }
    let entries = match tokio::task::spawn_blocking(move || remote_media_entries(&query)).await {
        Ok(entries) => entries,
        Err(_) => return status_response(StatusCode::INTERNAL_SERVER_ERROR),
    };
    json_response(StatusCode::OK, entries)
}

async fn subscribe_session(
    client: reqwest::Client,
    relay_url: String,
    device_id: String,
    state: RemoteState,
    sid: String,
    stream_id: String,
) {
    let Some((session, events, mut receiver)) = session_stream(&state, &sid).await else {
        let _ = post_stream_event(
            &client,
            &relay_url,
            &device_id,
            &stream_id,
            state.token.as_ref(),
            &RemoteEvent::Status {
                status: RemoteStatus::Errored("Session not found.".to_string()),
            },
        )
        .await;
        return;
    };
    let room_id = session.room_id.clone();
    let through_seq = events
        .last()
        .map(|event| event.server_seq)
        .unwrap_or_default();
    let initial = [
        RemoteEvent::Session { session },
        RemoteEvent::Snapshot {
            room_id,
            through_seq,
            events,
        },
    ];
    for event in initial {
        if post_stream_event(
            &client,
            &relay_url,
            &device_id,
            &stream_id,
            state.token.as_ref(),
            &event,
        )
        .await
        .is_err()
        {
            return;
        }
    }
    loop {
        match receiver.recv().await {
            Ok(message) => {
                if let Some(event) = service_event(&state, &sid, message).await
                    && post_stream_event(
                        &client,
                        &relay_url,
                        &device_id,
                        &stream_id,
                        state.token.as_ref(),
                        &event,
                    )
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                if let Some(event) = session_snapshot(&state, &sid).await {
                    let _ = post_stream_event(
                        &client,
                        &relay_url,
                        &device_id,
                        &stream_id,
                        state.token.as_ref(),
                        &event,
                    )
                    .await;
                }
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn post_stream_event(
    client: &reqwest::Client,
    relay_url: &str,
    device_id: &str,
    stream_id: &str,
    token: &str,
    event: &RemoteEvent,
) -> Result<(), reqwest::Error> {
    let endpoint = format!("{relay_url}/desktop/{device_id}/streams/{stream_id}/events");
    client
        .post(endpoint)
        .bearer_auth(token)
        .json(event)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn status_response(status: StatusCode) -> DesktopResponse {
    DesktopResponse {
        status: status.as_u16(),
        body: Value::Null,
    }
}

fn json_response<T: Serialize>(status: StatusCode, body: T) -> DesktopResponse {
    DesktopResponse {
        status: status.as_u16(),
        body: serde_json::to_value(body).unwrap_or(Value::Null),
    }
}

fn configured_relay_url() -> Option<String> {
    std::env::var("VMUX_REMOTE_RELAY_URL")
        .ok()
        .or_else(|| std::fs::read_to_string(crate::remote_relay_url_path()).ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn ensure_device_id() -> std::io::Result<String> {
    let path = crate::remote_relay_device_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(existing.to_string());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let device_id = uuid::Uuid::new_v4().simple().to_string();
    std::fs::write(&path, &device_id)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(device_id)
}

#[derive(Default)]
struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > SSE_BUFFER_LIMIT {
            self.buffer.clear();
            return Err("SSE command buffer exceeded limit".to_string());
        }
        let mut events = Vec::new();
        while let Some(index) = self.buffer.windows(2).position(|window| window == b"\n\n") {
            let raw = String::from_utf8(self.buffer[..index].to_vec())
                .map_err(|error| error.to_string())?;
            self.buffer.drain(..index + 2);
            let data = raw
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                events.push(data);
            }
        }
        Ok(events)
    }
}
