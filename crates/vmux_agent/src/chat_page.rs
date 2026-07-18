//! The `vmux://agent` chat page: a native Dioxus UI that renders an agent session's
//! conversation + run-state (pushed from ECS) and sends prompt/approval intents back.
//! This is the single agent front-end; it replaced the legacy CLI-install setup page.

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod composer;
pub mod event;

#[cfg(not(target_arch = "wasm32"))]
mod turns;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use base64::Engine;
#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

#[cfg(not(target_arch = "wasm32"))]
use crate::chat_page::event::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatAttachPaths, ChatAttachment,
    ChatAttachmentPreviewRequest, ChatAttachments, ChatCancel, ChatCancelQueuedPrompt,
    ChatClearQueue, ChatEscape, ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest,
    ChatPasteMedia, ChatPickFiles, ChatResume, ChatSnapshot, ChatSubmit, MODEL_STATE_EVENT,
    ModelOptionEntry, ModelState, QueuedPromptSnapshot, RESUMABLE_SESSIONS_EVENT,
    ResumableSessionEntry, ResumableSessions, ResumeListRequest, ResumeSession,
    RuntimeSwitchRequest, SLASH_COMMANDS_EVENT, SelectModel, SelectWorkspace, SlashCommandEntry,
    SlashCommands,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::chat_page::turns::group_turns;
#[cfg(not(target_arch = "wasm32"))]
use crate::client::acp::{AcpModelState, AcpSession};
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{AgentMessages, AgentSession, PromptQueue};
#[cfg(not(target_arch = "wasm32"))]
use crate::events::{AgentApprovalReply, ApprovalDecision};
#[cfg(not(target_arch = "wasm32"))]
use crate::handoff::{
    DEFAULT_CONTEXT_LIMIT, ImportedConversation, build_context, visible_messages,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::run_state::{AgentRunState, AgentTurnMeta};
#[cfg(not(target_arch = "wasm32"))]
use crate::strategy::{AgentStrategies, acp_agent_kind, kind_supports_cross_runtime};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::agent::{AgentKind, StackSessionHandoff, SwapStackSession};
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::team::Profile;
#[cfg(not(target_arch = "wasm32"))]
use vmux_core::{PageMetadata, PageOpenRequest, PageOpenTarget};
#[cfg(not(target_arch = "wasm32"))]
use vmux_service::client::ServiceClient;
#[cfg(not(target_arch = "wasm32"))]
use vmux_service::protocol::{AgentAttachment, ClientMessage};

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

#[cfg(any(test, target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ApprovalDetail {
    label: String,
    value: String,
}

#[cfg(any(test, target_arch = "wasm32"))]
fn approval_details(args_json: &str) -> Vec<ApprovalDetail> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(args_json) else {
        return if args_json.trim().is_empty() {
            Vec::new()
        } else {
            vec![ApprovalDetail {
                label: "Details".to_string(),
                value: args_json.to_string(),
            }]
        };
    };
    let mut details = Vec::new();
    flatten_approval_details("", &value, &mut details);
    details
}

#[cfg(any(test, target_arch = "wasm32"))]
fn flatten_approval_details(
    path: &str,
    value: &serde_json::Value,
    details: &mut Vec<ApprovalDetail>,
) {
    if let serde_json::Value::Object(fields) = value {
        for (name, value) in fields {
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            flatten_approval_details(&child_path, value, details);
        }
        return;
    }
    let label = approval_detail_label(path);
    let value = match value {
        serde_json::Value::String(value) => value.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    details.push(ApprovalDetail { label, value });
}

#[cfg(any(test, target_arch = "wasm32"))]
fn approval_detail_label(path: &str) -> String {
    let path = path.strip_prefix("arguments.").unwrap_or(path);
    let label = if path.is_empty() { "details" } else { path };
    label
        .split('.')
        .map(|part| {
            let words = part.replace('_', " ");
            let mut chars = words.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(not(target_arch = "wasm32"))]
pub struct AgentChatPagePlugin;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message)]
struct AcpSetModelRequest {
    sid: String,
    request_id: u64,
    config_id: String,
    model_id: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
struct AcpModelRequestCounter(u64);

#[cfg(not(target_arch = "wasm32"))]
impl AcpModelRequestCounter {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for AgentChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.init_resource::<AcpModelRequestCounter>()
            .add_message::<AcpSetModelRequest>()
            .add_message::<WorkspaceSelected>()
            .add_plugins(BinEventEmitterPlugin::<(
                ChatSubmit,
                ChatApproval,
                ChatCancel,
                ChatResume,
                ChatClearQueue,
                ChatCancelQueuedPrompt,
                ChatEscape,
                ResumeListRequest,
                ResumeSession,
                RuntimeSwitchRequest,
                SelectModel,
                SelectWorkspace,
            )>::for_hosts(&["agent"]))
            .add_plugins(BinEventEmitterPlugin::<(
                ChatPickFiles,
                ChatPasteMedia,
                ChatMediaListRequest,
                ChatAttachPaths,
                ChatAttachmentPreviewRequest,
            )>::for_hosts(&["agent"]))
            .add_observer(on_chat_submit)
            .add_observer(on_chat_approval)
            .add_observer(on_chat_cancel)
            .add_observer(on_chat_resume)
            .add_observer(on_chat_clear_queue)
            .add_observer(on_chat_cancel_queued_prompt)
            .add_observer(on_chat_escape)
            .add_observer(on_chat_pick_files)
            .add_observer(on_chat_paste_media)
            .add_observer(on_chat_media_list_request)
            .add_observer(on_chat_attach_paths)
            .add_observer(on_chat_attachment_preview_request)
            .add_observer(on_resume_list_request)
            .add_observer(on_resume_session)
            .add_observer(on_runtime_switch_request)
            .add_observer(on_select_model)
            .add_observer(on_select_workspace)
            .add_observer(reset_chat_synced_on_page_ready)
            .add_systems(
                Update,
                (
                    (track_turn_duration, push_chat_to_page).chain(),
                    sync_chat_to_ready_views,
                    push_pending_workspace_to_page,
                    activate_selected_workspaces,
                    push_acp_model_state_to_page,
                    push_removed_acp_model_state_to_page,
                    send_acp_model_requests,
                    drain_chat_attachment_tasks,
                    drain_chat_media_list_tasks,
                    drain_resume_list_tasks,
                    drain_resume_handoff_tasks,
                ),
            );
    }
}

/// Record per-turn wall-clock from `AgentRunState` edges (covers page + ACP mutation sites
/// uniformly). Idempotent: the `turn_start` guard tolerates repeated same-state sets and does
/// not reset across a mid-turn `AwaitingApproval`.
#[cfg(not(target_arch = "wasm32"))]
fn track_turn_duration(
    time: Res<Time>,
    mut sessions: Query<(&AgentRunState, &mut AgentTurnMeta), Changed<AgentRunState>>,
) {
    for (state, mut meta) in &mut sessions {
        match state {
            AgentRunState::Streaming => {
                if meta.turn_start.is_none() {
                    meta.turn_start = Some(time.elapsed());
                }
            }
            AgentRunState::Idle | AgentRunState::Errored(_) => {
                if let Some(start) = meta.turn_start.take() {
                    meta.durations
                        .push(time.elapsed().saturating_sub(start).as_secs() as u32);
                }
            }
            AgentRunState::AwaitingApproval { .. } | AgentRunState::Installing { .. } => {}
        }
    }
}

/// Marks a chat-page webview (ACP or Page agent) so the ready→resync path can find it cheaply.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct AgentChatView;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component, Clone, Debug)]
pub(crate) struct PendingAgentWorkspace {
    pub target_url: String,
    pub error: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone, Debug)]
struct WorkspaceSelected {
    stack: Entity,
    path: std::path::PathBuf,
}

/// Set once the current snapshot has been pushed to a ready chat webview; cleared when the page
/// (re)signals ready (mount or Cmd+R reload) so the transcript is re-pushed instead of blanking.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ChatSynced;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ResumeListTask {
    webview: Entity,
    task: Task<ResumableSessions>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ResumeHandoffTask {
    stack: Entity,
    target_url: String,
    cwd: std::path::PathBuf,
    task: Task<Result<StackSessionHandoff, String>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ChatAttachmentTask {
    webview: Entity,
    event: &'static str,
    task: Task<ChatAttachments>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct ChatMediaListTask {
    webview: Entity,
    task: Task<ChatMediaEntries>,
}

#[cfg(not(target_arch = "wasm32"))]
const ATTACHMENT_PREVIEW_LIMIT: u64 = 8 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 96;

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn chat_attachment(path: std::path::PathBuf) -> Option<ChatAttachment> {
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    let mime_type = attachment_mime(&path);
    let preview_data_url =
        if mime_type.starts_with("image/") && metadata.len() <= ATTACHMENT_PREVIEW_LIMIT {
            std::fs::read(&path)
                .ok()
                .map(|bytes| {
                    format!(
                        "data:{mime_type};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    )
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
    Some(ChatAttachment {
        path: path.to_string_lossy().into_owned(),
        name,
        mime_type,
        size: metadata.len(),
        preview_data_url,
    })
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn spawn_chat_attachment_task(
    webview: Entity,
    event: &'static str,
    paths: Vec<std::path::PathBuf>,
    commands: &mut Commands,
) {
    if paths.is_empty() {
        return;
    }
    let task = IoTaskPool::get().spawn(async move {
        ChatAttachments {
            attachments: paths.into_iter().filter_map(chat_attachment).collect(),
        }
    });
    commands.spawn(ChatAttachmentTask {
        webview,
        event,
        task,
    });
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn chat_media_entries(request_id: u64, query: String) -> ChatMediaEntries {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return ChatMediaEntries {
            request_id,
            query,
            entries: Vec::new(),
        };
    };
    let candidate = if let Some(rest) = query.strip_prefix("file://") {
        decode_media_query_path(rest)
    } else if let Some(rest) = query.strip_prefix("~/") {
        home.join(decode_media_query_path(rest))
    } else if query == "~" {
        home.clone()
    } else {
        let path = decode_media_query_path(&query);
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
        return ChatMediaEntries {
            request_id,
            query,
            entries: Vec::new(),
        };
    };
    let Ok(directory) = directory.canonicalize() else {
        return ChatMediaEntries {
            request_id,
            query,
            entries: Vec::new(),
        };
    };
    if !directory.starts_with(&home) {
        return ChatMediaEntries {
            request_id,
            query,
            entries: Vec::new(),
        };
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
            Some(ChatMediaEntry {
                path: path.to_string_lossy().into_owned(),
                name,
                parent,
                mime_type,
                is_dir,
                preview_data_url: String::new(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    entries.truncate(100);
    let mut remaining_thumbnail_bytes = MEDIA_THUMBNAIL_TOTAL_LIMIT;
    for entry in &mut entries {
        if entry.is_dir || !entry.mime_type.starts_with("image/") {
            continue;
        }
        let source_size = std::fs::metadata(&entry.path)
            .map(|metadata| metadata.len())
            .unwrap_or(u64::MAX);
        if source_size > remaining_thumbnail_bytes {
            continue;
        }
        entry.preview_data_url =
            media_thumbnail_data_url(std::path::Path::new(&entry.path), source_size);
        if !entry.preview_data_url.is_empty() {
            remaining_thumbnail_bytes = remaining_thumbnail_bytes.saturating_sub(source_size);
        }
    }
    ChatMediaEntries {
        request_id,
        query,
        entries,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_media_list_request(
    trigger: On<BinReceive<ChatMediaListRequest>>,
    mut commands: Commands,
) {
    let request = trigger.event().payload.clone();
    let task = IoTaskPool::get()
        .spawn(async move { chat_media_entries(request.request_id, request.query) });
    commands.spawn(ChatMediaListTask {
        webview: trigger.event().webview,
        task,
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_attach_paths(trigger: On<BinReceive<ChatAttachPaths>>, mut commands: Commands) {
    let paths = trigger
        .event()
        .payload
        .paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .collect();
    spawn_chat_attachment_task(
        trigger.event().webview,
        CHAT_ATTACHMENTS_EVENT,
        paths,
        &mut commands,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_attachment_preview_request(
    trigger: On<BinReceive<ChatAttachmentPreviewRequest>>,
    mut commands: Commands,
) {
    let paths = trigger
        .event()
        .payload
        .paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .collect();
    spawn_chat_attachment_task(
        trigger.event().webview,
        CHAT_ATTACHMENT_PREVIEWS_EVENT,
        paths,
        &mut commands,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_pick_files(trigger: On<BinReceive<ChatPickFiles>>, mut commands: Commands) {
    let mut dialog = rfd::FileDialog::new();
    if let Some(home) = std::env::var_os("HOME") {
        dialog = dialog.set_directory(std::path::PathBuf::from(home));
    }
    let Some(paths) = dialog.pick_files() else {
        return;
    };
    spawn_chat_attachment_task(
        trigger.event().webview,
        CHAT_ATTACHMENTS_EVENT,
        paths,
        &mut commands,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn clipboard_image_path() -> Option<std::path::PathBuf> {
    if let Some(path) = vmux_terminal::clipboard::image_file_path() {
        return Some(std::path::PathBuf::from(path));
    }
    let png = vmux_terminal::clipboard::read_image_png()?;
    let directory = std::env::temp_dir().join("vmux-prompt-attachments");
    std::fs::create_dir_all(&directory).ok()?;
    let path = directory.join(format!("clipboard-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_paste_media(trigger: On<BinReceive<ChatPasteMedia>>, mut commands: Commands) {
    let Some(path) = clipboard_image_path() else {
        return;
    };
    spawn_chat_attachment_task(
        trigger.event().webview,
        CHAT_ATTACHMENTS_EVENT,
        vec![path],
        &mut commands,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_chat_attachment_tasks(
    mut tasks: Query<(Entity, &mut ChatAttachmentTask)>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(attachments) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            pending.webview,
            pending.event,
            &attachments,
        ));
        commands.entity(entity).despawn();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_chat_media_list_tasks(
    mut tasks: Query<(Entity, &mut ChatMediaListTask)>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(entries) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            pending.webview,
            CHAT_MEDIA_ENTRIES_EVENT,
            &entries,
        ));
        commands.entity(entity).despawn();
    }
}

/// Push the current transcript + slash commands to any chat webview that is ready but not yet
/// synced. Runs every frame and retries until the webview's emit channel is ready, so the very
/// first snapshot always lands. Re-runs after a reload because [`reset_chat_synced_on_page_ready`]
/// clears `ChatSynced` when the page re-signals ready — without this, Cmd+R blanked the chat
/// (the `Changed`/`Added` pushes never re-fire for an unchanged, already-added session).
#[cfg(not(target_arch = "wasm32"))]
fn sync_chat_to_ready_views(
    pending: Query<
        Entity,
        (
            With<AgentChatView>,
            With<vmux_core::page::PageReady>,
            Without<ChatSynced>,
        ),
    >,
    child_of: Query<&ChildOf>,
    sessions: Query<(
        &AgentMessages,
        &AgentRunState,
        Option<&AgentTurnMeta>,
        Option<&Profile>,
        Option<&PageMetadata>,
        &PromptQueue,
        Option<&ImportedConversation>,
    )>,
    pending_workspaces: Query<&PendingAgentWorkspace>,
    acp_sessions: Query<(&AcpSession, Option<&AcpModelState>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for webview in &pending {
        let Ok(parent) = child_of.get(webview) else {
            continue;
        };
        let stack = parent.parent();
        let session = sessions.get(stack).ok();
        let snapshot = match session {
            Some((messages, state, turn_meta, profile, meta, queue, imported)) => {
                snapshot_of(messages, state, turn_meta, profile, meta, queue, imported)
            }
            None => {
                let Ok(pending) = pending_workspaces.get(stack) else {
                    continue;
                };
                workspace_snapshot(pending)
            }
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot,
        ));
        if session.is_some() {
            let (cross, model_state) = acp_sessions
                .get(stack)
                .ok()
                .map(|(acp, model)| {
                    (
                        acp_agent_kind(&acp.agent_id)
                            .map(kind_supports_cross_runtime)
                            .unwrap_or(false),
                        model,
                    )
                })
                .unwrap_or((false, None));
            emit_model_state(webview, model_state, cross, &mut commands);
        }
        commands.entity(webview).insert(ChatSynced);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn workspace_snapshot(pending: &PendingAgentWorkspace) -> ChatSnapshot {
    ChatSnapshot {
        status: "workspace".to_string(),
        workspace_required: true,
        workspace_error: pending.error.clone(),
        ..Default::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_pending_workspace_to_page(
    pending: Query<(Entity, &PendingAgentWorkspace), Changed<PendingAgentWorkspace>>,
    children: Query<&Children>,
    chat_views: Query<(), With<AgentChatView>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, pending) in &pending {
        let Ok(children) = children.get(stack) else {
            continue;
        };
        let Some(webview) = children.iter().find(|entity| chat_views.contains(*entity)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &workspace_snapshot(pending),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn model_state_of(state: Option<&AcpModelState>) -> ModelState {
    let Some(state) = state else {
        return ModelState::default();
    };
    ModelState {
        current_model_id: state.display_model_id().to_string(),
        current_model_name: state.current_name().to_string(),
        models: state
            .models
            .iter()
            .map(|model| ModelOptionEntry {
                id: model.id.clone(),
                name: model.name.clone(),
                description: model.description.clone().unwrap_or_default(),
            })
            .collect(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn emit_model_state(
    webview: Entity,
    model_state: Option<&AcpModelState>,
    cross_runtime: bool,
    commands: &mut Commands,
) {
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        MODEL_STATE_EVENT,
        &model_state_of(model_state),
    ));
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        SLASH_COMMANDS_EVENT,
        &SlashCommands {
            commands: slash_commands_for(cross_runtime, model_state.is_some()),
        },
    ));
}

#[cfg(not(target_arch = "wasm32"))]
fn push_acp_model_state_to_page(
    sessions: Query<(Entity, &AcpSession, &AcpModelState), Changed<AcpModelState>>,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, session, model_state) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| is_browser.contains(entity)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        let cross = acp_agent_kind(&session.agent_id)
            .map(kind_supports_cross_runtime)
            .unwrap_or(false);
        emit_model_state(webview, Some(model_state), cross, &mut commands);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_removed_acp_model_state_to_page(
    mut removed: RemovedComponents<AcpModelState>,
    sessions: Query<&AcpSession>,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for stack in removed.read() {
        let Ok(session) = sessions.get(stack) else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&entity| is_browser.contains(entity)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        let cross = acp_agent_kind(&session.agent_id)
            .map(kind_supports_cross_runtime)
            .unwrap_or(false);
        emit_model_state(webview, None, cross, &mut commands);
    }
}

/// A chat webview re-signals `PageReady` on every (re)mount, including a Cmd+R reload. Clear its
/// `ChatSynced` marker so [`sync_chat_to_ready_views`] re-pushes the full transcript.
#[cfg(not(target_arch = "wasm32"))]
fn reset_chat_synced_on_page_ready(
    trigger: On<BinReceive<vmux_core::page::PageReady>>,
    chat_views: Query<(), With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if chat_views.get(webview).is_ok() {
        commands.entity(webview).remove::<ChatSynced>();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn snapshot_of(
    messages: &AgentMessages,
    state: &AgentRunState,
    turn_meta: Option<&AgentTurnMeta>,
    profile: Option<&Profile>,
    meta: Option<&PageMetadata>,
    queue: &PromptQueue,
    imported: Option<&ImportedConversation>,
) -> ChatSnapshot {
    let display_messages = visible_messages(imported, &messages.0);
    let durations: &[u32] = turn_meta.map(|m| m.durations.as_slice()).unwrap_or(&[]);
    let running = matches!(state, AgentRunState::Streaming);
    let items = group_turns(&display_messages, durations, running);
    let messages_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
    let (status, error) = match state {
        AgentRunState::Idle => ("idle", String::new()),
        AgentRunState::Installing { pct, message } => {
            let text = match pct {
                Some(p) => format!("{message} ({p}%)"),
                None => message.clone(),
            };
            ("installing", text)
        }
        AgentRunState::Streaming => ("streaming", String::new()),
        AgentRunState::AwaitingApproval { .. } => ("awaiting", String::new()),
        AgentRunState::Errored(message) => ("errored", message.clone()),
    };
    let (call_id, name, args_json) = match state {
        AgentRunState::AwaitingApproval {
            call_id,
            name,
            args,
        } => (call_id.clone(), name.clone(), args.to_string()),
        _ => (String::new(), String::new(), String::new()),
    };
    let (agent_name, accent_color) = profile
        .map(|p| (p.name.clone(), p.avatar.color.clone()))
        .unwrap_or_default();
    let agent_icon = meta
        .map(|m| m.icon.favicon_url().to_string())
        .unwrap_or_default();
    ChatSnapshot {
        messages_json,
        status: status.to_string(),
        error,
        approval_call_id: call_id,
        approval_name: name,
        approval_args_json: args_json,
        agent_name,
        agent_icon,
        accent_color,
        handoff_source: imported
            .map(|imported| imported.source_agent.clone())
            .unwrap_or_default(),
        handoff_truncated: imported.is_some_and(|imported| imported.truncated),
        handoff_message_count: imported
            .map(|imported| {
                u32::try_from(group_turns(&imported.messages, &[], false).len()).unwrap_or(u32::MAX)
            })
            .unwrap_or_default(),
        queued: queue
            .items
            .iter()
            .map(|item| QueuedPromptSnapshot {
                id: item.id,
                text: item.text.clone(),
                attachment_names: item
                    .attachments
                    .iter()
                    .map(|attachment| attachment.name.clone())
                    .collect(),
            })
            .collect(),
        paused: queue.paused,
        workspace_required: false,
        workspace_error: String::new(),
    }
}

/// Push each changed session's conversation + run-state to its pane webview (the child
/// `Browser` of the session entity).
#[cfg(not(target_arch = "wasm32"))]
fn push_chat_to_page(
    sessions: Query<
        (
            Entity,
            &AgentMessages,
            &AgentRunState,
            Option<&AgentTurnMeta>,
            Option<&Profile>,
            Option<&PageMetadata>,
            &PromptQueue,
            Option<&ImportedConversation>,
        ),
        Or<(
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<AgentTurnMeta>,
            Changed<PromptQueue>,
            Changed<Profile>,
            Changed<ImportedConversation>,
        )>,
    >,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, messages, state, turn_meta, profile, meta, queue, imported) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&e| is_browser.contains(e)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(messages, state, turn_meta, profile, meta, queue, imported),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, &mut AgentRunState)>,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let text = payload.text.clone();
    let attachments = payload
        .attachments
        .iter()
        .filter(|attachment| !attachment.path.is_empty())
        .map(|attachment| AgentAttachment {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        })
        .collect::<Vec<_>>();
    if text.trim().is_empty() && attachments.is_empty() {
        return;
    }
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    if let Ok((mut queue, mut state)) = sessions.get_mut(parent.parent()) {
        enqueue_prompt(&mut queue, &mut state, text, attachments);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn workspace_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("workspace")
        .to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn ancestor_workspace_tab(
    stack: Entity,
    child_of: &Query<&ChildOf>,
    tabs: &Query<&mut vmux_layout::tab::Tab>,
) -> Option<Entity> {
    let mut current = stack;
    loop {
        if tabs.contains(current) {
            return Some(current);
        }
        current = child_of.get(current).ok()?.parent();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_select_workspace(
    trigger: On<BinReceive<SelectWorkspace>>,
    child_of: Query<&ChildOf>,
    pending_workspaces: Query<(), With<PendingAgentWorkspace>>,
    mut selected_workspaces: MessageWriter<WorkspaceSelected>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    if !pending_workspaces.contains(stack) {
        return;
    }
    let Some(path) = rfd::FileDialog::new().pick_folder() else {
        return;
    };
    selected_workspaces.write(WorkspaceSelected { stack, path });
}

#[cfg(not(target_arch = "wasm32"))]
fn activate_selected_workspaces(
    mut selected_workspaces: MessageReader<WorkspaceSelected>,
    child_of: Query<&ChildOf>,
    mut tabs: Query<&mut vmux_layout::tab::Tab>,
    mut pending_workspaces: Query<&mut PendingAgentWorkspace>,
    managed_root: Option<Res<vmux_layout::worktree::ManagedWorktreeRoot>>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut commands: Commands,
) {
    let mut activated = std::collections::HashSet::new();
    for request in selected_workspaces.read() {
        if !activated.insert(request.stack) {
            continue;
        }
        let stack = request.stack;
        let Ok(pending) = pending_workspaces.get(stack) else {
            continue;
        };
        let target_url = pending.target_url.clone();
        let selected = match request.path.canonicalize() {
            Ok(selected) if selected.is_dir() => selected,
            Ok(selected) => {
                if let Ok(mut pending) = pending_workspaces.get_mut(stack) {
                    pending.error = format!("Workspace is not a directory: {}", selected.display());
                }
                continue;
            }
            Err(error) => {
                if let Ok(mut pending) = pending_workspaces.get_mut(stack) {
                    pending.error = format!("Invalid workspace directory: {error}");
                }
                continue;
            }
        };
        let Some(tab_entity) = ancestor_workspace_tab(stack, &child_of, &tabs) else {
            continue;
        };
        let Ok(mut tab) = tabs.get_mut(tab_entity) else {
            continue;
        };
        let selected_name = workspace_name(&selected);
        let generated_name = vmux_layout::worktree::is_generated_tab_name(&tab.name);
        let slug_hint = vmux_layout::worktree::tab_worktree_slug_hint(&tab.name, &selected);
        if generated_name {
            tab.name.clone_from(&selected_name);
        }
        let project_dir = selected.to_string_lossy().into_owned();
        let activation = if vmux_git::worktree::checkout_info(&selected).is_ok() {
            let managed_root = managed_root.as_deref().cloned().unwrap_or_default();
            match vmux_layout::worktree::create_worktree_blocking(
                &selected,
                &slug_hint,
                &managed_root.0,
            ) {
                Ok(activation) => Some(activation),
                Err(error) => {
                    if let Ok(mut pending) = pending_workspaces.get_mut(stack) {
                        pending.error = error;
                    }
                    continue;
                }
            }
        } else {
            None
        };
        let mut tab_commands = commands.entity(tab_entity);
        tab_commands
            .insert((
                vmux_layout::tab::TabWorkspace {
                    project_dir: project_dir.clone(),
                },
                vmux_layout::tab::TabDirDecided,
            ))
            .remove::<vmux_layout::tab::TabWorktreeUnavailable>();
        match activation {
            Some(activation) => {
                tab.startup_dir = Some(activation.execution_dir.to_string_lossy().into_owned());
                tab_commands.insert((activation.metadata, activation.ready));
            }
            None => {
                tab.startup_dir = Some(project_dir);
                tab_commands
                    .remove::<vmux_layout::tab::TabWorktree>()
                    .remove::<vmux_layout::worktree::TabWorktreeReady>();
            }
        }
        commands.entity(stack).remove::<PendingAgentWorkspace>();
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(stack),
            url: target_url,
            request_id: None,
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn enqueue_prompt(
    queue: &mut PromptQueue,
    state: &mut AgentRunState,
    text: String,
    attachments: Vec<AgentAttachment>,
) {
    queue.enqueue_with_attachments(text, attachments);
    if matches!(state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn cancel_session(
    service: Option<&ServiceClient>,
    acp: Option<&AcpSession>,
    page: Option<&AgentSession>,
) {
    let Some(service) = service else {
        return;
    };
    let Some(sid) = acp
        .map(|session| session.sid.clone())
        .or_else(|| page.map(|session| session.sid.clone()))
    else {
        return;
    };
    service.0.send(ClientMessage::AgentCancel { sid });
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_cancel(
    trigger: On<BinReceive<ChatCancel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&mut PromptQueue, Option<&AcpSession>, Option<&AgentSession>)>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    if queue.flush_pending() {
        queue.cancel_flush();
    }
    cancel_session(service.as_deref(), acp, page);
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_resume(
    trigger: On<BinReceive<ChatResume>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.resume();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_clear_queue(
    trigger: On<BinReceive<ChatClearQueue>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.clear();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_cancel_queued_prompt(
    trigger: On<BinReceive<ChatCancelQueuedPrompt>>,
    child_of: Query<&ChildOf>,
    mut queues: Query<&mut PromptQueue>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    if let Ok(mut queue) = queues.get_mut(parent.parent()) {
        queue.remove(trigger.event().payload.id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_escape(
    trigger: On<BinReceive<ChatEscape>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(
        &mut PromptQueue,
        &mut AgentRunState,
        Option<&AcpSession>,
        Option<&AgentSession>,
    )>,
    service: Option<Res<ServiceClient>>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((mut queue, mut state, acp, page)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    let running = matches!(
        *state,
        AgentRunState::Streaming | AgentRunState::AwaitingApproval { .. }
    );
    let flush = if queue.items.is_empty() {
        if queue.flush_pending() {
            queue.cancel_flush();
        }
        false
    } else {
        queue.request_flush()
    };
    if flush && matches!(*state, AgentRunState::Errored(_)) {
        *state = AgentRunState::Idle;
    }
    if running {
        cancel_session(service.as_deref(), acp, page);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_approval(
    trigger: On<BinReceive<ChatApproval>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let decision = match payload.decision {
        1 => ApprovalDecision::Allow,
        2 => ApprovalDecision::AllowAlways,
        _ => ApprovalDecision::Deny,
    };
    commands.trigger(AgentApprovalReply {
        session: parent.parent(),
        call_id: payload.call_id.clone(),
        decision,
    });
}

/// A short "2h ago"-style age for a session's last-modified time.
#[cfg(not(target_arch = "wasm32"))]
fn relative_time(mtime: std::time::SystemTime) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

/// The slash commands offered on an ACP pane.
#[cfg(not(target_arch = "wasm32"))]
fn slash_commands_for(cross_runtime: bool, has_models: bool) -> Vec<SlashCommandEntry> {
    let mut v = vec![
        SlashCommandEntry {
            name: "upload".into(),
            description: "Attach files".into(),
        },
        SlashCommandEntry {
            name: "resume".into(),
            description: "Resume a past session".into(),
        },
    ];
    if has_models {
        v.push(SlashCommandEntry {
            name: "model".into(),
            description: "Select model".into(),
        });
    }
    if cross_runtime {
        v.push(SlashCommandEntry {
            name: "cli".into(),
            description: "Continue this session in the CLI".into(),
        });
    }
    v
}

#[cfg(not(target_arch = "wasm32"))]
fn on_select_model(
    trigger: On<BinReceive<SelectModel>>,
    child_of: Query<&ChildOf>,
    mut sessions: Query<(&AcpSession, &mut AcpModelState)>,
    mut counter: ResMut<AcpModelRequestCounter>,
    mut requests: MessageWriter<AcpSetModelRequest>,
) {
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let Ok((session, mut model_state)) = sessions.get_mut(parent.parent()) else {
        return;
    };
    let model_id = trigger.event().payload.model_id.clone();
    if model_state.display_model_id() == model_id
        || !model_state.models.iter().any(|model| model.id == model_id)
    {
        return;
    }
    let request_id = counter.next();
    requests.write(AcpSetModelRequest {
        sid: session.sid.clone(),
        request_id,
        config_id: model_state.config_id.clone(),
        model_id: model_id.clone(),
    });
    model_state.pending = Some(crate::client::acp::PendingAcpModelSelection {
        request_id,
        model_id,
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn send_acp_model_requests(
    mut requests: MessageReader<AcpSetModelRequest>,
    service: Option<Res<ServiceClient>>,
) {
    let Some(service) = service else {
        return;
    };
    for request in requests.read() {
        service.0.send(ClientMessage::AcpSetModel {
            sid: request.sid.clone(),
            request_id: request.request_id,
            config_id: request.config_id.clone(),
            model_id: request.model_id.clone(),
        });
    }
}

/// The target url + cwd for an ACP↔CLI runtime handoff of the current session, or `None` when
/// the handoff is unavailable (unknown agent, no session id yet, bad `to`).
#[cfg(not(target_arch = "wasm32"))]
fn runtime_switch_target(
    agent_id: &str,
    resume: Option<&str>,
    cwd: &std::path::Path,
    to: &str,
    acp_ids: &[String],
) -> Option<(String, std::path::PathBuf)> {
    let kind = acp_agent_kind(agent_id)?;
    if !kind_supports_cross_runtime(kind) {
        return None;
    }
    let sid = resume?;
    let target = match to {
        "cli" => crate::AgentUrl::Cli {
            kind,
            sid: sid.to_string(),
        },
        "acp" => crate::AgentUrl::for_session(kind, sid, true, acp_ids),
        _ => return None,
    };
    Some((target.format(), cwd.to_path_buf()))
}

/// Page → native: `/resume` was opened — reply with the on-disk session list.
#[cfg(not(target_arch = "wasm32"))]
fn resume_entries(
    sessions: Vec<crate::client::cli::strategy::ResumableSession>,
    active_kind: Option<AgentKind>,
    active_name: &str,
) -> Vec<ResumableSessionEntry> {
    sessions
        .into_iter()
        .map(|session| {
            let dir = session
                .cwd
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| session.cwd.to_string_lossy().to_string());
            let agent_name = if Some(session.kind) == active_kind && !active_name.is_empty() {
                active_name.to_string()
            } else {
                session.kind.display_name().to_string()
            };
            ResumableSessionEntry {
                kind: session.kind.as_url_segment().to_string(),
                sid: session.sid,
                cwd: session.cwd.to_string_lossy().to_string(),
                title: session.title,
                subtitle: format!("{} · {}", relative_time(session.mtime), dir),
                agent_name,
                cross_runtime: session.cross_runtime,
            }
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn foreign_handoff_target(
    active_agent_id: &str,
    active_kind: Option<AgentKind>,
    source_kind: AgentKind,
) -> Option<String> {
    (active_kind != Some(source_kind)).then(|| {
        crate::AgentUrl::Acp {
            id: active_agent_id.to_string(),
            sid: None,
        }
        .format()
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn resume_agent_name(
    profile: Option<&Profile>,
    kind: Option<AgentKind>,
    acp_id: Option<&str>,
) -> String {
    profile
        .map(|profile| profile.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| kind.map(|kind| kind.display_name().to_string()))
        .or_else(|| acp_id.map(str::to_string))
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn on_resume_list_request(
    trigger: On<BinReceive<ResumeListRequest>>,
    strategies: Option<Res<AgentStrategies>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    agent_sessions: Query<&AgentSession>,
    profiles: Query<&Profile>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let strategies = strategies.map(|s| (*s).clone()).unwrap_or_default();
    let stack = child_of.get(webview).ok().map(ChildOf::parent);
    let acp = stack.and_then(|stack| acp_sessions.get(stack).ok());
    let kind = acp
        .and_then(|acp| acp_agent_kind(&acp.agent_id))
        .or_else(|| {
            stack.and_then(|stack| agent_sessions.get(stack).ok().map(|session| session.kind))
        });
    let agent_name = resume_agent_name(
        stack.and_then(|stack| profiles.get(stack).ok()),
        kind,
        acp.map(|acp| acp.agent_id.as_str()),
    );
    let task = IoTaskPool::get().spawn(async move {
        let sessions = resume_entries(strategies.list_all_sessions(), kind, &agent_name);
        ResumableSessions { sessions }
    });
    commands.spawn(ResumeListTask { webview, task });
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_resume_list_tasks(
    mut tasks: Query<(Entity, &mut ResumeListTask)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(sessions) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        commands.trigger(BinHostEmitEvent::from_rkyv(
            task.webview,
            RESUMABLE_SESSIONS_EVENT,
            &sessions,
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_resume_handoff_tasks(
    mut tasks: Query<(Entity, &mut ResumeHandoffTask)>,
    mut states: Query<&mut AgentRunState>,
    mut swap: MessageWriter<SwapStackSession>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(handoff) => {
                swap.write(SwapStackSession {
                    stack: pending.stack,
                    target_url: pending.target_url.clone(),
                    cwd: pending.cwd.clone(),
                    handoff: Some(handoff),
                });
            }
            Err(message) => {
                if let Ok(mut state) = states.get_mut(pending.stack) {
                    *state = AgentRunState::Errored(message);
                }
            }
        }
    }
}

/// Page → native: resume a picked session on this stack, in the current runtime.
#[cfg(not(target_arch = "wasm32"))]
fn on_resume_session(
    trigger: On<BinReceive<ResumeSession>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    settings: Res<vmux_setting::AppSettings>,
    strategies: Option<Res<AgentStrategies>>,
    mut commands: Commands,
    mut swap: MessageWriter<SwapStackSession>,
) {
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let Some(kind) = AgentKind::from_url_segment(&payload.kind) else {
        return;
    };
    if let Ok(acp) = acp_sessions.get(stack)
        && let Some(target_url) =
            foreign_handoff_target(&acp.agent_id, acp_agent_kind(&acp.agent_id), kind)
    {
        let strategies = strategies
            .map(|strategies| (*strategies).clone())
            .unwrap_or_default();
        let source_sid = payload.sid.clone();
        let source_agent = kind.display_name().to_string();
        let cwd = std::path::PathBuf::from(&payload.cwd);
        let task = IoTaskPool::get().spawn(async move {
            let messages = strategies.load_transcript(kind, &source_sid)?;
            let built = build_context(&messages, DEFAULT_CONTEXT_LIMIT);
            let messages_json = serde_json::to_string(&messages)
                .map_err(|err| format!("serialize imported conversation: {err}"))?;
            Ok(StackSessionHandoff {
                source_agent,
                source_kind: kind,
                source_sid,
                messages_json,
                context: built.text,
                truncated: built.truncated,
            })
        });
        commands.spawn(ResumeHandoffTask {
            stack,
            target_url,
            cwd,
            task,
        });
        return;
    }
    let prefer_acp = acp_sessions.get(stack).is_ok();
    let acp_ids: Vec<String> = settings.agent.acp.iter().map(|c| c.id.clone()).collect();
    let target = crate::AgentUrl::for_session(kind, &payload.sid, prefer_acp, &acp_ids);
    swap.write(SwapStackSession {
        stack,
        target_url: target.format(),
        cwd: std::path::PathBuf::from(&payload.cwd),
        handoff: None,
    });
}

/// Page → native: hand the current ACP session off to the other runtime (the `/cli` fallback).
#[cfg(not(target_arch = "wasm32"))]
fn on_runtime_switch_request(
    trigger: On<BinReceive<RuntimeSwitchRequest>>,
    child_of: Query<&ChildOf>,
    acp_sessions: Query<&AcpSession>,
    settings: Res<vmux_setting::AppSettings>,
    mut swap: MessageWriter<SwapStackSession>,
) {
    let to = trigger.event().payload.to.clone();
    let Ok(parent) = child_of.get(trigger.event().webview) else {
        return;
    };
    let stack = parent.parent();
    let Ok(acp) = acp_sessions.get(stack) else {
        bevy::log::warn!("runtime switch: current pane is not an ACP session");
        return;
    };
    let acp_ids: Vec<String> = settings.agent.acp.iter().map(|c| c.id.clone()).collect();
    let Some((target_url, cwd)) = runtime_switch_target(
        &acp.agent_id,
        acp.resume.as_deref(),
        &acp.cwd,
        &to,
        &acp_ids,
    ) else {
        bevy::log::warn!(
            "runtime switch to '{to}' unavailable for ACP agent '{}' (no shared session id yet)",
            acp.agent_id
        );
        return;
    };
    swap.write(SwapStackSession {
        stack,
        target_url,
        cwd,
        handoff: None,
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_workspace_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("dashboard");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        git(&repo, &["add", "seed.txt"]);
        git(&repo, &["commit", "-qm", "seed"]);
        (root, repo)
    }

    #[test]
    fn selected_git_workspace_creates_worktree_and_resumes_agent_open() {
        let (_root, repo) = init_workspace_repo();
        let managed_root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<WorkspaceSelected>()
            .add_message::<PageOpenRequest>()
            .insert_resource(vmux_layout::worktree::ManagedWorktreeRoot(
                managed_root.path().to_path_buf(),
            ))
            .add_systems(Update, activate_selected_workspaces);
        let tab = app
            .world_mut()
            .spawn(vmux_layout::tab::Tab {
                name: "Tab 1".into(),
                startup_dir: None,
            })
            .id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::stack_bundle(),
                PendingAgentWorkspace {
                    target_url: "vmux://agent/codex/cli".into(),
                    error: String::new(),
                },
                ChildOf(tab),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<WorkspaceSelected>>()
            .write(WorkspaceSelected {
                stack,
                path: repo.clone(),
            });

        app.update();

        let tab_data = app.world().get::<vmux_layout::tab::Tab>(tab).unwrap();
        assert_eq!(tab_data.name, "dashboard");
        assert!(
            Path::new(tab_data.startup_dir.as_deref().unwrap())
                .starts_with(managed_root.path().canonicalize().unwrap())
        );
        let metadata = app
            .world()
            .get::<vmux_layout::tab::TabWorktree>(tab)
            .unwrap();
        assert_eq!(metadata.branch, "vmux/dashboard");
        assert!(
            app.world()
                .get::<vmux_layout::worktree::TabWorktreeReady>(tab)
                .is_some()
        );
        assert!(app.world().get::<PendingAgentWorkspace>(stack).is_none());
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "vmux://agent/codex/cli");
        assert!(matches!(requests[0].target, PageOpenTarget::Stack(target) if target == stack));
    }

    #[test]
    fn workspace_snapshot_keeps_picker_error_visible() {
        let snapshot = workspace_snapshot(&PendingAgentWorkspace {
            target_url: "vmux://agent/codex/cli".into(),
            error: "branch exists".into(),
        });

        assert!(snapshot.workspace_required);
        assert_eq!(snapshot.workspace_error, "branch exists");
        assert_eq!(snapshot.status, "workspace");
    }

    #[test]
    fn media_query_paths_decode_percent_escapes() {
        assert_eq!(
            decode_media_query_path("Pictures/My%20Image%25.png"),
            std::path::PathBuf::from("Pictures/My Image%.png")
        );
    }

    #[test]
    fn media_thumbnail_is_small_png_data_url() {
        let path =
            std::env::temp_dir().join(format!("vmux-media-thumbnail-{}.png", uuid::Uuid::new_v4()));
        let image = image::RgbaImage::from_pixel(240, 120, image::Rgba([20, 40, 60, 255]));
        image.save(&path).unwrap();
        let source_size = std::fs::metadata(&path).unwrap().len();

        let data_url = media_thumbnail_data_url(&path, source_size);

        std::fs::remove_file(path).unwrap();
        let encoded = data_url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let thumbnail = image::load_from_memory(&bytes).unwrap();
        assert_eq!(thumbnail.width().max(thumbnail.height()), 96);
    }

    #[test]
    fn runtime_switch_builtin_acp_agents_to_cli() {
        let cases = [
            ("claude", "claude"),
            ("claude-acp", "claude"),
            ("codex", "codex"),
            ("codex-acp", "codex"),
            ("vibe", "vibe"),
            ("mistral-vibe", "vibe"),
        ];
        let ids = cases
            .iter()
            .map(|(id, _)| (*id).to_string())
            .collect::<Vec<_>>();
        for (agent_id, cli_segment) in cases {
            let got = runtime_switch_target(agent_id, Some("sid-9"), Path::new("/w"), "cli", &ids);
            assert_eq!(
                got,
                Some((
                    format!("vmux://agent/{cli_segment}/cli/sid-9"),
                    std::path::PathBuf::from("/w")
                ))
            );
        }
    }

    #[test]
    fn runtime_switch_requires_session_id() {
        let ids = vec!["claude".to_string()];
        assert_eq!(
            runtime_switch_target("claude", None, Path::new("/w"), "cli", &ids),
            None
        );
    }

    #[test]
    fn runtime_switch_gated_for_unknown_agent() {
        let ids = vec!["claude".to_string()];
        assert_eq!(
            runtime_switch_target("custom", Some("s"), Path::new("/w"), "cli", &ids),
            None
        );
    }

    #[test]
    fn slash_commands_include_cli_only_when_cross_runtime() {
        let base = slash_commands_for(false, false);
        assert_eq!(base.len(), 2);
        assert_eq!(base[0].name, "upload");
        let with_model = slash_commands_for(false, true);
        assert_eq!(with_model.len(), 3);
        assert_eq!(with_model[2].name, "model");
        let with_cli = slash_commands_for(true, false);
        assert_eq!(with_cli.len(), 3);
        assert_eq!(with_cli[2].name, "cli");
    }

    #[test]
    fn model_selection_updates_cached_state_before_response() {
        let mut app = App::new();
        app.init_resource::<AcpModelRequestCounter>()
            .add_message::<AcpSetModelRequest>()
            .add_observer(on_select_model);
        let stack = app
            .world_mut()
            .spawn((
                AcpSession {
                    agent_id: "claude".into(),
                    sid: "s1".into(),
                    cwd: "/tmp".into(),
                    anchor: vmux_core::ProcessId::new(),
                    resume: None,
                },
                AcpModelState {
                    config_id: "model".into(),
                    current_model_id: "default".into(),
                    pending: None,
                    models: vec![
                        vmux_service::protocol::AcpModelOption {
                            id: "default".into(),
                            name: "Default".into(),
                            description: None,
                        },
                        vmux_service::protocol::AcpModelOption {
                            id: "fable".into(),
                            name: "Fable".into(),
                            description: None,
                        },
                    ],
                },
            ))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectModel {
                model_id: "fable".into(),
            },
        });

        let state = app.world().get::<AcpModelState>(stack).unwrap();
        assert_eq!(state.current_model_id, "default");
        assert_eq!(
            state.pending.as_ref().map(|pending| pending.request_id),
            Some(1)
        );
        assert_eq!(
            state
                .pending
                .as_ref()
                .map(|pending| pending.model_id.as_str()),
            Some("fable")
        );
        assert_eq!(state.current_name(), "Fable");
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AcpSetModelRequest>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sid, "s1");
        assert_eq!(requests[0].request_id, 1);
        assert_eq!(requests[0].config_id, "model");
        assert_eq!(requests[0].model_id, "fable");

        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectModel {
                model_id: "fable".into(),
            },
        });
        app.world_mut().trigger(BinReceive {
            webview,
            payload: SelectModel {
                model_id: "missing".into(),
            },
        });
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<AcpSetModelRequest>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn resume_results_include_all_agent_kinds_with_source_labels() {
        use crate::client::cli::strategy::ResumableSession;
        use std::time::SystemTime;

        let session = |kind, sid: &str| ResumableSession {
            kind,
            sid: sid.into(),
            cwd: "/work".into(),
            mtime: SystemTime::UNIX_EPOCH,
            title: sid.into(),
            cross_runtime: kind_supports_cross_runtime(kind),
        };
        let entries = resume_entries(
            vec![
                session(AgentKind::Claude, "claude-1"),
                session(AgentKind::Codex, "codex-1"),
            ],
            Some(AgentKind::Claude),
            "Antigravity",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].agent_name, "Antigravity");
        assert_eq!(entries[1].agent_name, "Codex");
    }

    #[test]
    fn foreign_resume_keeps_active_acp_agent_fresh() {
        assert_eq!(
            foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Codex,),
            Some("vmux://agent/claude".to_string())
        );
        assert_eq!(
            foreign_handoff_target("claude", Some(AgentKind::Claude), AgentKind::Claude,),
            None
        );
        assert_eq!(
            foreign_handoff_target("custom-acp", None, AgentKind::Codex),
            Some("vmux://agent/custom-acp".to_string())
        );
    }

    #[test]
    fn snapshot_reports_grouped_imported_item_boundary() {
        let imported = ImportedConversation {
            source_agent: "Codex".into(),
            source_kind: AgentKind::Codex,
            source_sid: "codex-1".into(),
            messages: vec![
                crate::Message::user("one"),
                crate::Message::Assistant {
                    blocks: vec![crate::AssistantBlock::ToolUse {
                        call_id: "call-1".into(),
                        name: "run".into(),
                        args: "{}".into(),
                    }],
                },
                crate::Message::ToolResult {
                    call_id: "call-1".into(),
                    content: "two".into(),
                    is_error: false,
                },
            ],
            truncated: false,
            first_prompt: None,
        };
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::Idle,
            None,
            None,
            None,
            &PromptQueue::default(),
            Some(&imported),
        );

        assert_eq!(snapshot.handoff_message_count, 2);
    }

    #[test]
    fn snapshot_includes_approval_tool_and_input() {
        let snapshot = snapshot_of(
            &AgentMessages::default(),
            &AgentRunState::AwaitingApproval {
                call_id: "call-1".into(),
                name: "vmux.run".into(),
                args: serde_json::json!({"command": "echo hi", "focus": true}),
            },
            None,
            None,
            None,
            &PromptQueue::default(),
            None,
        );

        assert_eq!(snapshot.approval_name, "vmux.run");
        assert_eq!(
            snapshot.approval_args_json,
            r#"{"command":"echo hi","focus":true}"#
        );
    }

    #[test]
    fn approval_prompt_renders_structured_tool_input() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("snap.approval_args_json.clone()"));
        assert!(source.contains("approval_details(&args_json)"));
    }

    #[test]
    fn approval_details_parse_nested_json() {
        assert_eq!(
            approval_details(
                r#"{"arguments":{"path":"/tmp/SKILL.md"},"server":"vmux","tool":"read_file"}"#
            ),
            vec![
                ApprovalDetail {
                    label: "Path".into(),
                    value: "/tmp/SKILL.md".into(),
                },
                ApprovalDetail {
                    label: "Server".into(),
                    value: "vmux".into(),
                },
                ApprovalDetail {
                    label: "Tool".into(),
                    value: "read_file".into(),
                },
            ]
        );
        assert!(approval_details("{}").is_empty());
    }

    #[test]
    fn disclosure_icons_use_animated_plus_minus() {
        let page = include_str!("chat_page/page.rs");
        let css = include_str!("../../vmux_server/assets/index.css");
        assert!(!page.contains('▸'));
        assert!(page.contains("render_disclosure_icon"));
        assert!(css.contains(".disclosure[open] > summary .disclosure-icon::after"));
    }

    #[test]
    fn disclosure_panels_animate_open_and_closed() {
        let css = include_str!("../../vmux_server/assets/index.css");
        assert!(css.contains("interpolate-size: allow-keywords"));
        assert!(css.contains(".disclosure::details-content"));
        assert!(css.contains(".disclosure[open]::details-content"));
        assert!(css.contains("transition-behavior: allow-discrete"));
    }

    #[test]
    fn submitting_after_error_rearms_prompt_dispatch() {
        let mut queue = PromptQueue::default();
        let mut state = AgentRunState::Errored("failed".into());

        enqueue_prompt(&mut queue, &mut state, "retry".into(), Vec::new());

        assert!(matches!(state, AgentRunState::Idle));
        assert_eq!(
            queue.items.front().map(|item| item.text.as_str()),
            Some("retry")
        );
        assert!(!queue.paused);
    }

    #[test]
    fn normal_cancel_overrides_pending_flush() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_cancel);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatCancel> {
            webview,
            payload: ChatCancel,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn escape_flush_rearms_errored_queue() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("retry".into());
        queue.paused = true;
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Errored("failed".into())))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(matches!(
            app.world().get::<AgentRunState>(stack),
            Some(AgentRunState::Idle)
        ));
        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert!(queue.flush_pending());
        assert!(!queue.paused);
    }

    #[test]
    fn escape_without_queue_clears_stale_flush() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_escape);
        let mut queue = PromptQueue::default();
        queue.enqueue("queued".into());
        assert!(queue.request_flush());
        queue.items.clear();
        let stack = app
            .world_mut()
            .spawn((queue, AgentRunState::Streaming))
            .id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut().trigger(BinReceive::<ChatEscape> {
            webview,
            payload: ChatEscape,
        });
        app.world_mut().flush();

        assert!(
            !app.world()
                .get::<PromptQueue>(stack)
                .unwrap()
                .flush_pending()
        );
    }

    #[test]
    fn cancel_queued_prompt_removes_only_target() {
        use bevy_cef::prelude::BinReceive;

        let mut app = App::new();
        app.add_observer(on_chat_cancel_queued_prompt);
        let mut queue = PromptQueue::default();
        queue.enqueue("first".into());
        queue.enqueue("second".into());
        let second_id = queue.items[1].id;
        let stack = app.world_mut().spawn(queue).id();
        let webview = app.world_mut().spawn(ChildOf(stack)).id();

        app.world_mut()
            .trigger(BinReceive::<ChatCancelQueuedPrompt> {
                webview,
                payload: ChatCancelQueuedPrompt { id: second_id },
            });
        app.world_mut().flush();

        let queue = app.world().get::<PromptQueue>(stack).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].text, "first");
    }

    #[test]
    fn resume_agent_name_prefers_profile_then_kind_then_id() {
        let profile = Profile::registry("Antigravity", "antigravity");
        assert_eq!(
            resume_agent_name(Some(&profile), Some(AgentKind::Claude), Some("claude")),
            "Antigravity"
        );
        assert_eq!(
            resume_agent_name(None, Some(AgentKind::Claude), Some("claude")),
            "Claude"
        );
        assert_eq!(
            resume_agent_name(None, None, Some("custom-acp")),
            "custom-acp"
        );
    }

    #[test]
    fn resume_list_scan_runs_on_io_task_pool() {
        let source = include_str!("chat_page.rs");
        let handler = source
            .split("fn on_resume_list_request")
            .nth(1)
            .expect("resume-list handler")
            .split("fn on_resume_session")
            .next()
            .expect("resume-list handler body");
        assert!(handler.contains("IoTaskPool::get().spawn"));
        assert!(source.contains("fn drain_resume_list_tasks"));
    }

    #[test]
    fn composer_resume_menu_remains_escapeable_when_empty() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("let session_menu_open = resume_query.is_some();"));
        assert!(source.contains("e.key() == Key::Escape && !command_modifier"));
    }

    #[test]
    fn composer_resume_selector_supports_prompt_filter_and_keyboard_navigation() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("SelectorMode::Resume"));
        assert!(source.contains("filter_sessions"));
        assert!(source.contains("No matching sessions"));
        assert!(source.contains("menu_direction"));
        assert!(source.contains("ScrollLogicalPosition::Nearest"));
        assert!(source.contains("agent-selector-item-{i}"));
    }

    #[test]
    fn composer_resume_rows_render_agent_name() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("session.agent_name"));
        assert!(source.contains("max-w-[40%] shrink-0 truncate text-xs text-muted-foreground"));
    }

    #[test]
    fn composer_captures_global_prompt_input_without_stealing_shortcuts() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("install_global_prompt_input"));
        assert!(source.contains("meta_key() || event.ctrl_key() || event.alt_key()"));
        assert!(source.contains("PromptEdit::Backspace"));
        assert!(source.contains("PromptEdit::Delete"));
        assert!(source.contains("dispatch_keyboard_event"));
    }

    #[test]
    fn installing_chat_uses_matrix_loading_composer() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("let installing_splash = installing && items.read().is_empty();"));
        assert!(source.contains("MatrixRain {"));
        assert!(source.contains("PromptGhost {"));
        assert!(source.contains("terminal: false"));
        assert!(source.contains("id: \"chat-scroll\""));
        assert!(source.contains("bg-gradient-to-t from-background via-background/95"));
        assert!(!source.contains("absolute inset-0 z-20 flex items-center justify-center"));
    }

    #[test]
    fn composer_auto_grows_and_contains_action_button() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("fn resize_prompt_textarea()"));
        assert!(source.contains("textarea.scroll_height().clamp(40, 160)"));
        assert!(source.contains("max-h-40 min-h-10"));
        assert!(source.contains("rounded-2xl"));
        assert!(source.contains("backdrop-blur-3xl backdrop-saturate-150"));
        assert!(source.contains("h-8 w-8 shrink-0 self-center items-center justify-center"));
        assert!(source.contains("if draft.read().is_empty()"));
        assert!(source.contains("show_capability_examples"));
        assert!(source.contains("attachments.read().is_empty()"));
        assert!(source.contains("if show_capability_examples"));
        assert!(source.contains("Type / for commands or @ for media"));
        assert!(!source.contains("agent-chat-caret relative top-px ml-px h-4 w-1.5 shrink-0"));
        assert!(source.contains("prompt_prefix_at_utf16"));
        assert!(source.contains("agent-chat-caret"));
        assert!(source.contains("caret-color:transparent"));
        assert!(source.contains("onkeyup"));
        assert!(source.contains("onscroll"));
        assert!(source.contains("placeholder:text-transparent"));
    }

    #[test]
    fn composer_supports_history_uploads_and_clipboard_media() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("prompt_history_direction"));
        assert!(source.contains("move_prompt_history"));
        assert!(source.contains("ChatPickFiles"));
        assert!(source.contains("ChatPasteMedia"));
        assert!(source.contains("preview_data_url"));
        assert!(source.contains("Remove attachment"));
        assert!(source.contains("attachments_to_submit"));
        assert!(source.contains("ChatMediaListRequest"));
        assert!(source.contains("select_media_entry"));
        assert!(source.contains("entry.preview_data_url"));
        assert!(source.contains("h-12 w-16"));
        assert!(source.contains("attachment-pill-{attachment.path}"));
        assert!(source.contains("String::new()"));
        assert!(source.contains("CHAT_ATTACHMENT_PREVIEWS_EVENT"));
        assert!(source.contains("render_user_attachment"));
        assert!(source.contains("max-h-80 max-w-full object-contain"));
    }

    #[test]
    fn workspace_gate_uses_native_folder_picker_event() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("Choose a workspace"));
        assert!(source.contains("try_cef_bin_emit_rkyv(&SelectWorkspace)"));
        assert!(source.contains("if !workspace_required()"));
    }

    #[test]
    fn composer_slash_items_support_mouse_selection() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("onclick: move |_| run_slash_command"));
        assert!(source.contains("onclick: move |_| select_resume_session"));
    }

    #[test]
    fn composer_escape_defers_queue_state_to_native() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("try_cef_bin_emit_rkyv(&ChatEscape)"));
        assert!(!source.contains("ChatFlush"));
    }

    #[test]
    fn queued_flush_controls_repurpose_composer_button() {
        let source = include_str!("chat_page/page.rs");
        assert!(source.contains("kbd {"));
        assert!(source.contains("if queued.read().is_empty()"));
        assert!(source.contains("ChatCancelQueuedPrompt"));
        assert!(source.contains("title: \"Cancel queued prompt\""));
        assert!(source.contains("title: \"Stop\""));
        assert!(source.contains("rect { x: \"6\", y: \"6\""));
        assert!(source.contains("\"Send all queued prompts now (Esc)\""));
        assert!(source.contains("path { d: \"M12 19V5\" }"));
    }

    #[test]
    fn composer_ctrl_c_cancel_does_not_use_streaming_snapshot() {
        let source = include_str!("chat_page/page.rs");
        let handler = source
            .split("} else if e.modifiers().ctrl()")
            .nth(1)
            .expect("ctrl-c handler")
            .split("},")
            .next()
            .expect("ctrl-c handler body");
        assert!(handler.contains("try_cef_bin_emit_rkyv(&ChatCancel)"));
        assert!(!handler.contains("&& streaming"));
    }

    #[test]
    fn page_ready_clears_chat_synced_only_for_chat_views() {
        use bevy::prelude::*;
        use bevy_cef::prelude::BinReceive;
        use vmux_core::page::PageReady;

        let mut app = App::new();
        app.add_observer(reset_chat_synced_on_page_ready);

        let chat = app.world_mut().spawn((AgentChatView, ChatSynced)).id();
        let other = app.world_mut().spawn(ChatSynced).id();

        app.world_mut().trigger(BinReceive::<PageReady> {
            webview: chat,
            payload: PageReady {},
        });
        app.world_mut().trigger(BinReceive::<PageReady> {
            webview: other,
            payload: PageReady {},
        });
        app.world_mut().flush();

        assert!(
            app.world().get::<ChatSynced>(chat).is_none(),
            "a chat view must re-sync (ChatSynced cleared) when the page reloads"
        );
        assert!(
            app.world().get::<ChatSynced>(other).is_some(),
            "a non-chat view must be left untouched"
        );
    }

    fn duration_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, track_turn_duration);
        app
    }

    #[test]
    fn streaming_then_idle_records_one_duration() {
        let mut app = duration_app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        assert!(
            app.world()
                .get::<AgentTurnMeta>(e)
                .unwrap()
                .turn_start
                .is_some()
        );
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::Idle;
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert_eq!(meta.durations.len(), 1);
        assert!(meta.turn_start.is_none());
    }

    #[test]
    fn awaiting_approval_does_not_finalize() {
        let mut app = duration_app();
        let e = app.world_mut().spawn(AgentRunState::Streaming).id();
        app.update();
        *app.world_mut().get_mut::<AgentRunState>(e).unwrap() = AgentRunState::AwaitingApproval {
            call_id: "c".into(),
            name: "n".into(),
            args: serde_json::Value::Null,
        };
        app.update();
        let meta = app.world().get::<AgentTurnMeta>(e).unwrap();
        assert!(meta.durations.is_empty());
        assert!(meta.turn_start.is_some());
    }
}
