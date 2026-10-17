use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use vmux_api::conversation::{ClientOpId, Message, RemoteMediaEntry, RemoteSession};
use vmux_api::protocol::{
    AgentAttachment, AgentCommandResult, AgentListAgents, AgentListModels, AgentListTeam,
    AgentNewChat, AgentRequest, AgentRequestId, AgentSelectModel, AgentSetEffort, ServiceMessage,
    SharedEvent, SharedFailure, SharedMessage, SharedResponse,
};
use vmux_transport::service::{RemoteDriver, RemoteFuture, RemoteOperationStore};

use crate::acp::{AcpInput, AcpSessions};
use crate::broker_driver::AgentBroker;

const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 16;
const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ATTACHMENT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MEDIA_QUERY_BYTES: usize = 4 * 1024;
const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;
const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 512;
const MAX_CLIENT_OP_ID_BYTES: usize = 256;

pub struct AgentRemoteDriver {
    sessions: AcpSessions,
    broker: AgentBroker,
    operations: Arc<dyn RemoteOperationStore>,
}

impl AgentRemoteDriver {
    pub fn new(
        sessions: AcpSessions,
        broker: AgentBroker,
        operations: Arc<dyn RemoteOperationStore>,
    ) -> Self {
        Self {
            sessions,
            broker,
            operations,
        }
    }

    async fn handle(&self, request: SharedMessage) -> SharedResponse {
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
            SharedMessage::AgentCancel { sid } => self.cancel(sid).await,
            SharedMessage::AgentApprove {
                sid,
                call_id,
                decision,
            } => self.approve(sid, call_id, decision).await,
            SharedMessage::AgentListMedia { sid, query } => self.media(&sid, query).await,
            SharedMessage::AgentNewChat {
                client_op_id,
                prompt,
                agent_url,
            } => {
                if !RemoteClientOpId(&client_op_id).valid() {
                    return SharedResponse::Failed(SharedFailure::Invalid);
                }
                if !self.operations.claim(client_op_id.clone()).await {
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
                    self.operations.release(client_op_id).await;
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
        if self
            .sessions
            .remote_session(sid.to_string())
            .await
            .is_some()
        {
            return SharedResponse::Ok;
        }
        SharedResponse::Failed(SharedFailure::NotFound)
    }

    async fn media(&self, sid: &str, query: String) -> SharedResponse {
        if self
            .sessions
            .remote_session(sid.to_string())
            .await
            .is_none()
        {
            return SharedResponse::Failed(SharedFailure::NotFound);
        }
        if query.len() > MAX_MEDIA_QUERY_BYTES {
            return SharedResponse::Failed(SharedFailure::Invalid);
        }
        match tokio::task::spawn_blocking(move || RemoteMediaQuery(&query).entries()).await {
            Ok(entries) => SharedResponse::Media(entries),
            Err(_) => SharedResponse::Failed(SharedFailure::Internal),
        }
    }

    async fn sessions(&self) -> Vec<RemoteSession> {
        let mut sessions = self.sessions.remote_sessions().await;
        for session in &mut sessions {
            if let Some(messages) = self.messages(&session.id.0).await {
                session.title =
                    vmux_session::ConversationTitle::from_messages(&messages, &session.name);
            }
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at_ms));
        sessions
    }

    async fn messages(&self, sid: &str) -> Option<Vec<Message>> {
        self.sessions.remote_messages(sid.to_string()).await
    }

    async fn current_session(&self, sid: &str) -> Option<RemoteSession> {
        let mut session = self.sessions.remote_session(sid.to_string()).await?;
        if let Some(messages) = self.messages(sid).await {
            session.title =
                vmux_session::ConversationTitle::from_messages(&messages, &session.name);
        }
        Some(session)
    }

    async fn cancel(&self, sid: String) -> SharedResponse {
        if self.sessions.input(sid, AcpInput::Cancel).await {
            return SharedResponse::Ok;
        }
        SharedResponse::Failed(SharedFailure::NotFound)
    }

    async fn approve(
        &self,
        sid: String,
        call_id: String,
        decision: vmux_api::protocol::ApprovalDecision,
    ) -> SharedResponse {
        if self
            .sessions
            .input(sid, AcpInput::Approve { call_id, decision })
            .await
        {
            return SharedResponse::Ok;
        }
        SharedResponse::Failed(SharedFailure::NotFound)
    }

    async fn prompt(
        &self,
        sid: &str,
        text: String,
        context: Option<String>,
        attachments: Vec<AgentAttachment>,
        preferred_mode: Option<String>,
    ) -> SharedResponse {
        if text.trim().is_empty() || text.len() > MAX_PROMPT_BYTES {
            return SharedResponse::Failed(SharedFailure::Invalid);
        }
        let Some(attachments) = RemoteAttachments::validated(attachments) else {
            return SharedResponse::Failed(SharedFailure::Invalid);
        };
        if self
            .sessions
            .input(
                sid.to_string(),
                AcpInput::User {
                    text,
                    context,
                    attachments,
                    preferred_mode,
                },
            )
            .await
        {
            return SharedResponse::Ok;
        }
        SharedResponse::Failed(SharedFailure::NotFound)
    }

    async fn broker<T>(&self, payload: T) -> SharedResponse
    where
        T: vmux_api::AgentRequestContract + serde::Serialize,
    {
        let Ok(request) = AgentRequest::encode(&payload) else {
            return SharedResponse::Failed(SharedFailure::Invalid);
        };
        match self
            .broker
            .command(AgentRequestId::new(), None, request)
            .await
            .ok()
        {
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

impl RemoteDriver for AgentRemoteDriver {
    fn dispatch(&self, request: SharedMessage) -> RemoteFuture<'_, SharedResponse> {
        Box::pin(self.handle(request))
    }

    fn subscription(&self, request: &SharedMessage) -> Option<String> {
        let SharedMessage::AgentAttach { sid } = request else {
            return None;
        };
        Some(sid.clone())
    }

    fn subscribe(
        &self,
        sid: String,
    ) -> RemoteFuture<'_, Option<tokio::sync::broadcast::Receiver<ServiceMessage>>> {
        Box::pin(self.sessions.subscribe(sid))
    }

    fn resolve(&self, sid: String, event: SharedEvent) -> RemoteFuture<'_, Option<SharedEvent>> {
        Box::pin(async move {
            match event {
                SharedEvent::AcpAgentInfo { .. } | SharedEvent::AcpWorkspaceChanged { .. } => {
                    let session = self.current_session(&sid).await?;
                    Some(SharedEvent::Session { session })
                }
                other => Some(other),
            }
        })
    }

    fn snapshot(&self, sid: String) -> RemoteFuture<'_, Option<SharedEvent>> {
        Box::pin(async move {
            match self.sessions.snapshot(sid).await? {
                ServiceMessage::Shared(event) => Some(event),
                _ => None,
            }
        })
    }
}

struct RemoteClientOpId<'a>(&'a ClientOpId);

impl RemoteClientOpId<'_> {
    fn valid(&self) -> bool {
        let value = self.0.as_str();
        !value.trim().is_empty() && value.len() <= MAX_CLIENT_OP_ID_BYTES
    }
}

struct RemoteAttachments(Vec<AgentAttachment>);

impl RemoteAttachments {
    fn validated(attachments: Vec<AgentAttachment>) -> Option<Vec<AgentAttachment>> {
        if attachments.len() > MAX_ATTACHMENTS {
            return None;
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)?
            .canonicalize()
            .ok()?;
        let mut total = 0_u64;
        let attachments = Self(attachments);
        attachments
            .0
            .into_iter()
            .map(|attachment| {
                let path = PathBuf::from(&attachment.path).canonicalize().ok()?;
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
                    mime_type: RemoteMediaPath(&path).mime(),
                    path: path.to_string_lossy().into_owned(),
                    size: metadata.len(),
                })
            })
            .collect()
    }
}

struct RemoteMediaPath<'a>(&'a Path);

impl RemoteMediaPath<'_> {
    fn mime(&self) -> String {
        let path = self.0.to_string_lossy();
        if let Some(mime) = vmux_api::media::MediaKind::mime(&path) {
            return mime.to_string();
        }
        let extension = self
            .0
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
            "txt" | "rs" | "toml" | "ron" | "yaml" | "yml" | "js" | "ts" | "tsx" | "jsx"
            | "css" | "sh" | "zsh" | "bash" | "py" | "go" | "c" | "h" | "cc" | "cpp" | "hpp"
            | "java" | "kt" | "swift" => "text/plain",
            "zip" => "application/zip",
            "gz" => "application/gzip",
            "tar" => "application/x-tar",
            _ => "application/octet-stream",
        }
        .to_string()
    }

    fn thumbnail(&self, source_size: u64) -> String {
        if source_size > MEDIA_THUMBNAIL_SOURCE_LIMIT {
            return String::new();
        }
        let Some(mime) = vmux_api::media::MediaKind::image_mime(&self.0.to_string_lossy()) else {
            return String::new();
        };
        if mime == "image/svg+xml" || mime == "image/avif" {
            return String::new();
        }
        let Ok(bytes) = std::fs::read(self.0) else {
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
}

struct RemoteMediaQuery<'a>(&'a str);

impl RemoteMediaQuery<'_> {
    fn decode(value: &str) -> PathBuf {
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
        PathBuf::from(String::from_utf8_lossy(&decoded).into_owned())
    }

    fn entries(&self) -> Vec<RemoteMediaEntry> {
        let query = self.0;
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return Vec::new();
        };
        let candidate = if let Some(rest) = query.strip_prefix("file://") {
            Self::decode(rest)
        } else if let Some(rest) = query.strip_prefix("~/") {
            home.join(Self::decode(rest))
        } else if query == "~" {
            home.clone()
        } else {
            let path = Self::decode(query);
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
                    .map(Path::to_path_buf)
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
                    RemoteMediaPath(&path).mime()
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
            entry.preview_data_url = RemoteMediaPath(Path::new(&entry.path)).thumbnail(entry.size);
            if !entry.preview_data_url.is_empty() {
                remaining_thumbnail_bytes = remaining_thumbnail_bytes.saturating_sub(entry.size);
            }
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ClosedOperations;

    impl RemoteOperationStore for ClosedOperations {
        fn claim(&self, _id: ClientOpId) -> RemoteFuture<'_, bool> {
            Box::pin(async { false })
        }

        fn release(&self, _id: ClientOpId) -> RemoteFuture<'_, ()> {
            Box::pin(async {})
        }
    }

    fn driver() -> AgentRemoteDriver {
        let (sender, _) = tokio::sync::broadcast::channel(8);
        let (wake, inbox) = tokio::sync::mpsc::unbounded_channel();
        drop(inbox);
        let (sessions, runtime) = AcpSessions::new(tokio::runtime::Handle::current(), wake);
        drop(runtime);
        AgentRemoteDriver::new(
            sessions,
            AgentBroker::new(
                sender,
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            Arc::new(ClosedOperations),
        )
    }

    fn prompt(length: usize) -> SharedMessage {
        SharedMessage::AgentInput {
            sid: "s".into(),
            text: "x".repeat(length),
            context: None,
            attachments: Vec::new(),
            preferred_mode: None,
        }
    }

    #[tokio::test]
    async fn rejects_invalid_prompts_before_session_lookup() {
        let driver = driver();
        assert!(matches!(
            driver.dispatch(prompt(MAX_PROMPT_BYTES + 1)).await,
            SharedResponse::Failed(SharedFailure::Invalid)
        ));
        assert!(matches!(
            driver.dispatch(prompt(16)).await,
            SharedResponse::Failed(SharedFailure::NotFound)
        ));
    }

    #[test]
    fn decodes_media_paths() {
        assert_eq!(
            RemoteMediaQuery::decode("Pictures/My%20Photo.png"),
            PathBuf::from("Pictures/My Photo.png")
        );
    }
}
