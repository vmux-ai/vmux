//! What Remote is made of, minus the transport.
//!
//! There is no server here any more — the axum listener this file was named for is gone, and a
//! desktop behind NAT could never have been reached by one. [`spawn`] mints the token, builds the
//! state, and hands it to the QUIC dialer in `quic`, which is what a phone actually talks to.
//!
//! What remains is everything that is true regardless of transport: the shared [`RemoteState`],
//! replay dedup, the limits on prompts and attachments, and the `$HOME`-confined media walk.
//! `quic/dispatch.rs` is the only caller.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use tokio::sync::Mutex;

use crate::RemotePaths;
use crate::acp::AcpSessionManager;
use crate::agent::AgentSessionManager;
use crate::agent_broker::AgentBroker;
use crate::message::Message;
use crate::protocol::AgentAttachment;
use crate::remote::{ClientOpId, RemoteMediaEntry, RemoteSession};

pub(crate) const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 16;
const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ATTACHMENT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MEDIA_QUERY_BYTES: usize = 4 * 1024;
const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;
const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 96;
const MAX_CLIENT_OP_IDS: usize = 4096;
const MAX_CLIENT_OP_ID_BYTES: usize = 256;

#[derive(Clone)]
pub(crate) struct RemoteState {
    pub(crate) token: Arc<str>,
    pub(crate) paired: Arc<AtomicBool>,
    pub(crate) agents: Arc<Mutex<AgentSessionManager>>,
    pub(crate) acp: Arc<Mutex<AcpSessionManager>>,
    pub(crate) broker: AgentBroker,
    pub(crate) client_ops: Arc<Mutex<ClientOpDeduper>>,
}

#[derive(Default)]
pub(crate) struct ClientOpDeduper {
    seen: HashSet<ClientOpId>,
    order: VecDeque<ClientOpId>,
}

impl ClientOpDeduper {
    pub(crate) fn claim(&mut self, client_op_id: ClientOpId) -> bool {
        if !self.seen.insert(client_op_id.clone()) {
            return false;
        }
        self.order.push_back(client_op_id);
        while self.order.len() > MAX_CLIENT_OP_IDS {
            if let Some(expired) = self.order.pop_front() {
                self.seen.remove(&expired);
            }
        }
        true
    }

    pub(crate) fn release(&mut self, client_op_id: &ClientOpId) {
        self.seen.remove(client_op_id);
        self.order.retain(|queued| queued != client_op_id);
    }
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
            paired: Arc::new(AtomicBool::new(RemotePaths::current().paired().exists())),
            agents,
            acp,
            broker,
            client_ops: Arc::new(Mutex::new(ClientOpDeduper::default())),
        };
        // The desktop is unreachable without the relay, so a failure here is not something to
        // serve around — it is Remote being down, and it says so.
        match super::quic::spawn(state) {
            Ok(handle) => {
                if let Err(error) = handle.await {
                    tracing::error!(%error, "remote quic: relay task ended");
                }
            }
            Err(error) => tracing::error!(%error, "remote quic: cannot reach the relay"),
        }
    })
}

pub(crate) fn remote_enabled() -> bool {
    remote_enabled_at(&RemotePaths::current().state())
}

fn remote_enabled_at(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|state| state.trim() == "enabled")
}

/// Ask the GUI for something only it holds, keeping the shape of its answer.
///
/// The daemon and the ECS are separate processes, so anything derived from ECS state costs a
/// round-trip rather than a read.
///
/// `None` means no GUI answered. That is distinct from a GUI that answered `Ok` with no payload,
/// and collapsing the two once reported a created chat as a missing desktop.
pub(crate) async fn broker_result(
    state: &RemoteState,
    command: crate::protocol::AgentCommand,
) -> Option<crate::protocol::AgentCommandResult> {
    state
        .broker
        .command(crate::protocol::AgentRequestId::new(), None, command)
        .await
        .ok()
}

pub(crate) async fn session_messages(state: &RemoteState, sid: &str) -> Option<Vec<Message>> {
    {
        let acp = state.acp.lock().await;
        if let Some(messages) = acp.remote_messages(sid) {
            return Some(messages);
        }
    }
    state.agents.lock().await.remote_messages(sid).await
}

pub(crate) async fn current_session(state: &RemoteState, sid: &str) -> Option<RemoteSession> {
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
        session.title = vmux_wire::room::Message::conversation_title(&messages, &session.name);
    }
    Some(session)
}

pub(crate) fn secure_eq(left: &str, right: &str) -> bool {
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

/// A replay key the deduper is willing to hold.
///
/// It is remembered until 4096 newer ones push it out, so an unbounded one is retained memory the
/// sender chooses the size of. The prompt cap does not cover this — it measures the text.
pub(crate) fn valid_client_op_id(client_op_id: &ClientOpId) -> bool {
    let value = client_op_id.as_str();
    !value.trim().is_empty() && value.len() <= MAX_CLIENT_OP_ID_BYTES
}

pub(crate) fn validate_remote_attachments(
    attachments: Vec<AgentAttachment>,
) -> Option<Vec<AgentAttachment>> {
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

pub(crate) fn remote_media_entries(query: &str) -> Vec<RemoteMediaEntry> {
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

pub(crate) fn mark_paired(paired: &AtomicBool) {
    if paired.swap(true, Ordering::AcqRel) {
        return;
    }
    let path = RemotePaths::current().paired();
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
    let remote = RemotePaths::current();
    let path = remote.token();
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
    let _ = std::fs::remove_file(remote.paired());
    super::write_private(&path, &token)?;
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
    fn client_operation_ids_are_bounded() {
        assert!(valid_client_op_id(&ClientOpId::new("mobile:1:1")));
        assert!(!valid_client_op_id(&ClientOpId::new("  ")));
        assert!(!valid_client_op_id(&ClientOpId::new(
            "x".repeat(MAX_CLIENT_OP_ID_BYTES + 1)
        )));
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

    #[test]
    fn client_operation_deduplication_is_bounded_and_releasable() {
        let mut deduper = ClientOpDeduper::default();
        let first = ClientOpId::new("first");
        assert!(deduper.claim(first.clone()));
        assert!(!deduper.claim(first.clone()));
        deduper.release(&first);
        assert!(deduper.claim(first));

        for index in 0..=MAX_CLIENT_OP_IDS {
            assert!(deduper.claim(ClientOpId::new(format!("op-{index}"))));
        }
        assert_eq!(deduper.order.len(), MAX_CLIENT_OP_IDS);
        assert_eq!(deduper.seen.len(), MAX_CLIENT_OP_IDS);
    }
}
