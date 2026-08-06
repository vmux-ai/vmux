use std::{net::IpAddr, time::Duration};

use crate::protocol::AgentCommand;
use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use vmux_remote::{DesktopCommand, DesktopCommandKind, DesktopResponse};

use super::*;

const SSE_BUFFER_LIMIT: usize = 2 * 1024 * 1024;
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct DesktopRelayClient {
    http: reqwest::Client,
    relay_url: String,
    device_id: String,
    state: RemoteState,
}

pub(super) fn spawn(state: RemoteState) -> tokio::task::JoinHandle<()> {
    DesktopRelayClient::run(state)
}

impl DesktopRelayClient {
    fn run(state: RemoteState) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match Self::from_config(state.clone()) {
                    Ok(Some(client)) => {
                        tracing::info!(
                            relay_url = %client.relay_url,
                            device_id = %client.device_id,
                            "remote relay: connecting"
                        );
                        if let Err(error) = client.command_loop().await {
                            tracing::warn!(%error, "remote relay: disconnected");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(%error, "remote relay: failed to configure"),
                }
                tokio::time::sleep(RECONNECT_DELAY).await;
            }
        })
    }

    fn from_config(state: RemoteState) -> Result<Option<Self>, String> {
        if !remote_enabled() {
            return Ok(None);
        }
        let Some(relay_url) = configured_relay_url() else {
            return Ok(None);
        };
        let device_id =
            ensure_device_id().map_err(|error| format!("failed to create device id: {error}"))?;
        let http = relay_client(&relay_url)
            .map_err(|error| format!("failed to create HTTP client: {error}"))?;
        Ok(Some(Self {
            http,
            relay_url,
            device_id,
            state,
        }))
    }

    async fn command_loop(&self) -> Result<(), String> {
        let endpoint = format!("{}/desktop/{}/commands", self.relay_url, self.device_id);
        let response = self
            .http
            .get(&endpoint)
            .bearer_auth(self.state.token.as_ref())
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
                // Skip rather than fail: a relay newer than this desktop will send commands it has
                // never heard of, and tearing down the whole stream for one of them would leave
                // every other route reconnecting in a loop.
                let command: DesktopCommand = match serde_json::from_str(&payload) {
                    Ok(command) => command,
                    Err(error) => {
                        tracing::warn!(%error, "remote relay: ignoring unrecognised command");
                        continue;
                    }
                };
                let client = self.clone();
                tokio::spawn(async move {
                    client.handle_command(command).await;
                });
            }
        }
        Ok(())
    }

    async fn handle_command(&self, command: DesktopCommand) {
        let DesktopCommand { id, kind } = command;
        let response = match kind {
            DesktopCommandKind::ListSessions => list_sessions_response(self.state.clone()).await,
            DesktopCommandKind::ListAgents => {
                broker_list_response(self.state.clone(), AgentCommand::ListAgents).await
            }
            DesktopCommandKind::ListTeam => {
                broker_list_response(self.state.clone(), AgentCommand::ListTeam).await
            }
            DesktopCommandKind::CreateChat { body } => {
                create_chat_response(self.state.clone(), body).await
            }
            DesktopCommandKind::SendPrompt { sid, body } => {
                send_prompt_response(self.state.clone(), sid, body).await
            }
            DesktopCommandKind::Cancel { sid } => cancel_response(self.state.clone(), sid).await,
            DesktopCommandKind::Approve { sid, body } => {
                approve_response(self.state.clone(), sid, body).await
            }
            DesktopCommandKind::ListMedia { sid, query } => {
                list_media_response(self.state.clone(), sid, query).await
            }
            DesktopCommandKind::SubscribeSession { sid, stream_id } => {
                self.subscribe_session(sid, stream_id).await;
                return;
            }
        };
        if let Err(error) = self.post_response(&id, &response).await {
            tracing::warn!(%error, "remote relay: failed to post command response");
        }
    }

    async fn post_response(
        &self,
        command_id: &str,
        response: &DesktopResponse,
    ) -> Result<(), reqwest::Error> {
        let endpoint = format!(
            "{}/desktop/{}/responses/{command_id}",
            self.relay_url, self.device_id
        );
        self.http
            .post(endpoint)
            .bearer_auth(self.state.token.as_ref())
            .json(response)
            .send()
            .await?;
        Ok(())
    }

    async fn subscribe_session(&self, sid: String, stream_id: String) {
        let Some((session, events, mut receiver)) = session_stream(&self.state, &sid).await else {
            let _ = self
                .post_stream_event(
                    &stream_id,
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
            if self.post_stream_event(&stream_id, &event).await.is_err() {
                return;
            }
        }
        loop {
            match receiver.recv().await {
                Ok(message) => {
                    if let Some(event) = service_event(&self.state, &sid, message).await
                        && self.post_stream_event(&stream_id, &event).await.is_err()
                    {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if let Some(event) = session_snapshot(&self.state, &sid).await {
                        let _ = self.post_stream_event(&stream_id, &event).await;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }

    async fn post_stream_event(
        &self,
        stream_id: &str,
        event: &RemoteEvent,
    ) -> Result<(), reqwest::Error> {
        let endpoint = format!(
            "{}/desktop/{}/streams/{stream_id}/events",
            self.relay_url, self.device_id
        );
        self.http
            .post(endpoint)
            .bearer_auth(self.state.token.as_ref())
            .json(event)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

fn relay_client(relay_url: &str) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder();
    if accepts_local_development_cert(relay_url) {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder.build().map_err(|error| error.to_string())
}

fn accepts_local_development_cert(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host == "localhost" {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
    })
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

/// A GUI-held list, relayed as-is.
///
/// The body is forwarded without being parsed into its concrete type: the relay is a pipe, and
/// re-deriving the shape here would be a second place to keep in step with the page.
async fn broker_list_response(state: RemoteState, command: AgentCommand) -> DesktopResponse {
    let Some(json) = broker_json(&state, command).await else {
        return status_response(StatusCode::BAD_GATEWAY);
    };
    match serde_json::from_str::<Value>(&json) {
        Ok(body) => DesktopResponse {
            status: StatusCode::OK.as_u16(),
            body,
        },
        Err(_) => status_response(StatusCode::BAD_GATEWAY),
    }
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
        agent_url: request.agent_url.clone(),
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

/// Where this daemon should reach the relay.
///
/// The file is the normal source: the desktop app resolves the URL and writes it there, because
/// launchd gives this process no inherited environment. An explicit variable still wins for a
/// daemon started by hand, and the hosted default covers one that has never seen the app.
fn configured_relay_url() -> Option<String> {
    if let Ok(from_env) = std::env::var("VMUX_REMOTE_RELAY_URL") {
        return crate::normalize_relay_url(&from_env);
    }
    match std::fs::read_to_string(crate::remote_relay_url_path()) {
        Ok(persisted) => crate::normalize_relay_url(&persisted),
        Err(_) => crate::normalize_relay_url(crate::DEFAULT_RELAY_URL),
    }
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
