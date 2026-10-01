use std::sync::Arc;

use crate::RelayToken;
use base64::Engine;
use bevy::prelude::*;
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::RemotePaths;
use crate::remote::authorization::RemoteAuthorizations;
use crate::remote::client_operation::ClientOperations;
use crate::remote::{ClientOpId, RemoteMediaEntry, RemoteSession};
use vmux_agent::acp::AcpSessions;
use vmux_agent::broker::AgentBroker;
use vmux_api::protocol::AgentAttachment;
use vmux_api::room::Message;

pub(crate) struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            super::authorization::RemoteAuthorizationPlugin,
            super::client_operation::ClientOperationPlugin,
        ))
        .add_systems(Startup, start_runtime)
        .add_systems(Update, (refresh_exposure, reconcile_dialer).chain());
    }
}

#[derive(Component)]
pub(crate) struct RemoteRuntimeStartup(Option<RemoteRuntimeStart>);

struct RemoteRuntimeStart {
    runtime: Handle,
    authorizations: RemoteAuthorizations,
    acp: AcpSessions,
    broker: AgentBroker,
    client_ops: ClientOperations,
}

impl RemoteRuntimeStartup {
    pub(crate) fn new(
        runtime: Handle,
        authorizations: RemoteAuthorizations,
        acp: AcpSessions,
        broker: AgentBroker,
        client_ops: ClientOperations,
    ) -> Self {
        Self(Some(RemoteRuntimeStart {
            runtime,
            authorizations,
            acp,
            broker,
            client_ops,
        }))
    }
}

#[derive(Component)]
struct RemoteRuntime {
    runtime: Handle,
    liveness: watch::Sender<bool>,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct RemoteExposure(bool);

impl RemoteExposure {
    fn current() -> Self {
        Self::read(&RemotePaths::current().state())
    }

    fn read(path: &std::path::Path) -> Self {
        Self(std::fs::read_to_string(path).is_ok_and(|state| state.trim() == "enabled"))
    }
}

#[derive(Component)]
struct RemoteDialerTask(tokio::task::JoinHandle<()>);

impl Drop for RemoteDialerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn start_runtime(mut startups: Query<(Entity, &mut RemoteRuntimeStartup)>, mut commands: Commands) {
    for (entity, mut startup) in &mut startups {
        let Some(start) = startup.0.take() else {
            continue;
        };
        let relay_token = match RelayToken::ensure() {
            Ok(token) => token,
            Err(error) => {
                tracing::error!(%error, "remote: token setup failed");
                commands.entity(entity).remove::<RemoteRuntimeStartup>();
                continue;
            }
        };
        let exposure = RemoteExposure::current();
        let (liveness, _) = watch::channel(exposure.0);
        commands
            .entity(entity)
            .remove::<RemoteRuntimeStartup>()
            .insert((
                RemoteState {
                    relay_token: Arc::from(relay_token.as_str()),
                    authorizations: start.authorizations,
                    acp: start.acp,
                    broker: start.broker,
                    client_ops: start.client_ops,
                },
                RemoteRuntime {
                    runtime: start.runtime,
                    liveness,
                },
                exposure,
            ));
    }
}

fn refresh_exposure(mut runtimes: Query<(&RemoteRuntime, &mut RemoteExposure)>) {
    for (runtime, mut exposure) in &mut runtimes {
        let current = RemoteExposure::current();
        if *exposure == current {
            continue;
        }
        *exposure = current;
        runtime.liveness.send_replace(current.0);
        tracing::info!(enabled = current.0, "remote quic: exposure changed");
    }
}

fn reconcile_dialer(
    runtimes: Query<(
        Entity,
        &RemoteState,
        &RemoteRuntime,
        &RemoteExposure,
        Option<&RemoteDialerTask>,
    )>,
    mut commands: Commands,
) {
    for (entity, state, runtime, exposure, running) in &runtimes {
        if !exposure.0 {
            if running.is_some() {
                commands.entity(entity).remove::<RemoteDialerTask>();
                tracing::info!("remote quic: the relay dialer stopped");
            }
            continue;
        }
        if running.is_some_and(|task| !task.0.is_finished()) {
            continue;
        }
        tracing::info!("remote quic: dialing the relay");
        let task = runtime.runtime.spawn(super::quic::dialer::run(
            state.clone(),
            runtime.liveness.subscribe(),
        ));
        commands.entity(entity).insert(RemoteDialerTask(task));
    }
}

pub(crate) const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_ATTACHMENTS: usize = 16;
const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ATTACHMENT_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MEDIA_QUERY_BYTES: usize = 4 * 1024;
const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;
const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 512;
const MAX_CLIENT_OP_ID_BYTES: usize = 256;

#[derive(Clone, Component)]
pub(crate) struct RemoteState {
    pub(crate) relay_token: Arc<str>,
    pub(crate) authorizations: RemoteAuthorizations,
    pub(crate) acp: AcpSessions,
    pub(crate) broker: AgentBroker,
    pub(crate) client_ops: ClientOperations,
}

impl RemoteState {
    pub(crate) async fn broker_result(
        &self,
        request: vmux_api::protocol::AgentRequest,
    ) -> Option<vmux_api::protocol::AgentCommandResult> {
        self.broker
            .command(vmux_api::protocol::AgentRequestId::new(), None, request)
            .await
            .ok()
    }

    pub(crate) async fn session_messages(&self, sid: &str) -> Option<Vec<Message>> {
        self.acp.remote_messages(sid.to_string()).await
    }

    pub(crate) async fn current_session(&self, sid: &str) -> Option<RemoteSession> {
        let mut session = self.acp.remote_session(sid.to_string()).await?;
        if let Some(messages) = self.session_messages(sid).await {
            session.title =
                vmux_session::ConversationTitle::from_messages(&messages, &session.name);
        }
        Some(session)
    }
}

pub(crate) struct RemoteClientOpId<'a>(pub(crate) &'a ClientOpId);

impl RemoteClientOpId<'_> {
    pub(crate) fn valid(&self) -> bool {
        let value = self.0.as_str();
        !value.trim().is_empty() && value.len() <= MAX_CLIENT_OP_ID_BYTES
    }
}

pub(crate) struct RemoteAttachments(Vec<AgentAttachment>);

impl RemoteAttachments {
    pub(crate) fn validated(attachments: Vec<AgentAttachment>) -> Option<Vec<AgentAttachment>> {
        if attachments.len() > MAX_ATTACHMENTS {
            return None;
        }
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)?
            .canonicalize()
            .ok()?;
        let mut total = 0_u64;
        let attachments = Self(attachments);
        attachments
            .0
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
                    mime_type: RemoteMediaPath(&path).mime(),
                    path: path.to_string_lossy().into_owned(),
                    size: metadata.len(),
                })
            })
            .collect()
    }
}

struct RemoteMediaPath<'a>(&'a std::path::Path);

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

pub(crate) struct RemoteMediaQuery<'a>(pub(crate) &'a str);

impl RemoteMediaQuery<'_> {
    fn decode(value: &str) -> std::path::PathBuf {
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

    pub(crate) fn entries(&self) -> Vec<RemoteMediaEntry> {
        let query = self.0;
        let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
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
            entry.preview_data_url =
                RemoteMediaPath(std::path::Path::new(&entry.path)).thumbnail(entry.size);
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

    #[test]
    fn client_operation_ids_are_bounded() {
        assert!(RemoteClientOpId(&ClientOpId::new("mobile:1:1")).valid());
        assert!(!RemoteClientOpId(&ClientOpId::new("  ")).valid());
        assert!(
            !RemoteClientOpId(&ClientOpId::new("x".repeat(MAX_CLIENT_OP_ID_BYTES + 1))).valid()
        );
    }

    #[test]
    fn remote_state_requires_enabled_marker() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-state");
        assert!(!RemoteExposure::read(&path).0);
        std::fs::write(&path, b"disabled\n").unwrap();
        assert!(!RemoteExposure::read(&path).0);
        std::fs::write(&path, b"enabled\n").unwrap();
        assert!(RemoteExposure::read(&path).0);
    }

    #[test]
    fn media_query_paths_decode_percent_escapes() {
        assert_eq!(
            RemoteMediaQuery::decode("Pictures/My%20Photo.png"),
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
        assert!(RemoteAttachments::validated(attachments).is_none());
    }
}
