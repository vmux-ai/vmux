use base64::Engine;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use super::{
    AgentChatView, ChatAttachmentProjection, ChatMediaProjection, ChatSnapshotProjection,
    ChatTranscriptProjection,
};
use vmux_chat::event::{
    ChatAttachPaths, ChatAttachment, ChatAttachments, ChatItem, ChatMediaEntries, ChatMediaEntry,
    ChatMediaListRequest, ChatMediaQueryRequest, ChatPasteMedia, ChatPickFiles,
    ChatRemoveAttachment, ChatSnapshot, ChatTranscriptState,
};

pub(super) struct ChatMediaPlugin;

impl Plugin for ChatMediaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ChatPickFiles,
            ChatPasteMedia,
            ChatMediaQueryRequest,
            ChatMediaListRequest,
            ChatAttachPaths,
            ChatRemoveAttachment,
        )>::default())
            .add_observer(on_chat_pick_files)
            .add_observer(on_chat_paste_media)
            .add_observer(on_chat_media_query_request)
            .add_observer(on_chat_media_query)
            .add_observer(on_chat_media_list_request)
            .add_observer(on_chat_attach_paths)
            .add_observer(on_chat_remove_attachment)
            .add_observer(on_chat_attachment_hydration_request)
            .add_systems(
                Update,
                (
                    drain_chat_attachment_tasks,
                    drain_chat_media_list_tasks,
                    drain_chat_media_preview_tasks,
                ),
            );
    }
}

#[derive(Component)]
struct ChatAttachmentTask {
    webview: Entity,
    delivery: ChatAttachmentDelivery,
    paths: Vec<String>,
    task: Task<Vec<ChatAttachment>>,
}

#[derive(Clone, Copy)]
enum ChatAttachmentDelivery {
    Selected,
    Hydrated,
}

#[derive(Event)]
pub(super) struct ChatAttachmentHydrationRequest {
    pub(super) webview: Entity,
    pub(super) paths: Vec<String>,
}

#[derive(Component)]
struct ChatMediaListTask {
    webview: Entity,
    task: Task<ChatMediaEntries>,
}

#[derive(Component)]
struct ChatMediaPreviewTask {
    webview: Entity,
    task: Task<ChatMediaEntries>,
}

#[derive(EntityEvent)]
pub(super) struct ChatMediaQuery {
    #[event_target]
    webview: Entity,
    query: String,
}

impl ChatMediaQuery {
    pub(super) fn new(webview: Entity, query: String) -> Self {
        Self { webview, query }
    }
}

impl ChatMediaProjection {
    fn start(&mut self, query: String) -> Option<u64> {
        if self.0.query == query {
            return None;
        }
        self.0.request_id = self.0.request_id.wrapping_add(1).max(1);
        self.0.query = query;
        self.0.entries.clear();
        self.0.loading = !self.0.query.is_empty();
        Some(self.0.request_id)
    }

    fn finish(&mut self, entries: &ChatMediaEntries) -> bool {
        if self.0.request_id != entries.request_id || self.0.query != entries.query {
            return false;
        }
        self.0.entries.clone_from(&entries.entries);
        self.0.loading = false;
        true
    }
}

impl ChatAttachmentProjection {
    pub(super) fn merge_selected(&mut self, incoming: &ChatAttachments) -> bool {
        for attachment in &incoming.attachments {
            if attachment.preview_data_url.is_empty() {
                continue;
            }
            self.resolved.insert(attachment.path.clone());
            self.previews
                .insert(attachment.path.clone(), attachment.clone());
        }
        let merged = incoming.merge_into(&mut self.selected);
        self.hydrate_selected() || merged
    }

    pub(super) fn remove_selected(&mut self, path: &str) -> bool {
        let previous = self.selected.len();
        self.selected.retain(|attachment| attachment.path != path);
        self.selected.len() != previous
    }

    pub(super) fn clear_selected(&mut self) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        self.selected.clear();
        true
    }

    pub(super) fn state(&self) -> ChatAttachments {
        ChatAttachments {
            attachments: self.selected.clone(),
        }
    }

    pub(super) fn start_hydration(&mut self, paths: &[String]) -> Vec<std::path::PathBuf> {
        let mut started = Vec::new();
        for path in paths {
            if path.is_empty() || self.resolved.contains(path) || !self.pending.insert(path.clone())
            {
                continue;
            }
            started.push(std::path::PathBuf::from(path));
        }
        started
    }

    pub(super) fn finish_hydration(
        &mut self,
        requested: &[String],
        attachments: &[ChatAttachment],
    ) -> bool {
        for path in requested {
            self.pending.remove(path);
            self.resolved.insert(path.clone());
        }
        for attachment in attachments {
            self.previews
                .insert(attachment.path.clone(), attachment.clone());
        }
        self.hydrate_selected()
    }

    pub(super) fn hydrate_transcript(&self, state: &mut ChatTranscriptState) -> bool {
        let mut changed = false;
        for item in &mut state.items {
            let ChatItem::User { attachments, .. } = item else {
                continue;
            };
            changed |= self.hydrate(attachments);
        }
        changed
    }

    pub(super) fn hydrate_snapshot(&self, snapshot: &mut ChatSnapshot) -> bool {
        let mut changed = false;
        for prompt in &mut snapshot.queued {
            changed |= self.hydrate(&mut prompt.attachments);
        }
        changed
    }

    pub(super) fn hydration_paths(
        &self,
        transcript: &ChatTranscriptState,
        snapshot: &ChatSnapshot,
    ) -> Vec<String> {
        let mut paths = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.append_hydration_paths(&self.selected, &mut paths, &mut seen);
        for item in &transcript.items {
            let ChatItem::User { attachments, .. } = item else {
                continue;
            };
            self.append_hydration_paths(attachments, &mut paths, &mut seen);
        }
        for prompt in &snapshot.queued {
            self.append_hydration_paths(&prompt.attachments, &mut paths, &mut seen);
        }
        paths
    }

    fn hydrate(&self, attachments: &mut [ChatAttachment]) -> bool {
        let mut changed = false;
        for attachment in attachments {
            if !attachment.preview_data_url.is_empty() {
                continue;
            }
            let Some(preview) = self.previews.get(&attachment.path) else {
                continue;
            };
            if preview.preview_data_url.is_empty() {
                continue;
            }
            attachment
                .preview_data_url
                .clone_from(&preview.preview_data_url);
            changed = true;
        }
        changed
    }

    fn hydrate_selected(&mut self) -> bool {
        let previews = &self.previews;
        let mut changed = false;
        for attachment in &mut self.selected {
            if !attachment.preview_data_url.is_empty() {
                continue;
            }
            let Some(preview) = previews.get(&attachment.path) else {
                continue;
            };
            if preview.preview_data_url.is_empty() {
                continue;
            }
            attachment
                .preview_data_url
                .clone_from(&preview.preview_data_url);
            changed = true;
        }
        changed
    }

    fn append_hydration_paths(
        &self,
        attachments: &[ChatAttachment],
        paths: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        for attachment in attachments {
            if !attachment.mime_type.starts_with("image/")
                || !attachment.preview_data_url.is_empty()
                || self.resolved.contains(&attachment.path)
                || self.pending.contains(&attachment.path)
                || !seen.insert(attachment.path.clone())
            {
                continue;
            }
            paths.push(attachment.path.clone());
        }
    }
}

const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;

const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;

const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 512;

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

fn chat_attachment(path: std::path::PathBuf) -> Option<ChatAttachment> {
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    let mime_type = attachment_mime(&path);
    Some(ChatAttachment {
        path: path.to_string_lossy().into_owned(),
        name,
        mime_type,
        size: metadata.len(),
        preview_data_url: String::new(),
    })
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

fn chat_attachment_preview(path: std::path::PathBuf) -> Option<ChatAttachment> {
    let mut attachment = chat_attachment(path)?;
    if !attachment.mime_type.starts_with("image/") {
        return None;
    }
    attachment.preview_data_url =
        media_thumbnail_data_url(std::path::Path::new(&attachment.path), attachment.size);
    (!attachment.preview_data_url.is_empty()).then_some(attachment)
}

fn spawn_chat_attachment_task(
    webview: Entity,
    delivery: ChatAttachmentDelivery,
    paths: Vec<std::path::PathBuf>,
    wake: vmux_core::host::wake::Wake,
    commands: &mut Commands,
) {
    if paths.is_empty() {
        return;
    }
    let requested = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let task = IoTaskPool::get().spawn(async move {
        let _wake = wake;
        paths
            .into_iter()
            .filter_map(match delivery {
                ChatAttachmentDelivery::Hydrated => chat_attachment_preview,
                ChatAttachmentDelivery::Selected => chat_attachment,
            })
            .collect()
    });
    commands.spawn(ChatAttachmentTask {
        webview,
        delivery,
        paths: requested,
        task,
    });
}

fn spawn_selected_attachment_tasks(
    webview: Entity,
    paths: Vec<std::path::PathBuf>,
    wake: vmux_core::host::wake::Wake,
    commands: &mut Commands,
) {
    spawn_chat_attachment_task(
        webview,
        ChatAttachmentDelivery::Selected,
        paths.clone(),
        wake,
        commands,
    );
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
    ChatMediaEntries {
        request_id,
        query,
        entries,
    }
}

fn chat_media_previews(mut response: ChatMediaEntries) -> ChatMediaEntries {
    let mut remaining_thumbnail_bytes = MEDIA_THUMBNAIL_TOTAL_LIMIT;
    for entry in &mut response.entries {
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
    response
}

fn on_chat_media_list_request(trigger: On<UiInput<ChatMediaListRequest>>, mut commands: Commands) {
    let request = trigger.event().payload.clone();
    let task = IoTaskPool::get()
        .spawn(async move { chat_media_entries(request.request_id, request.query) });
    commands.spawn(ChatMediaListTask {
        webview: trigger.event().webview,
        task,
    });
}

fn on_chat_media_query_request(
    trigger: On<UiInput<ChatMediaQueryRequest>>,
    mut commands: Commands,
) {
    commands.trigger(ChatMediaQuery::new(
        trigger.event().webview,
        trigger.event().payload.query.clone(),
    ));
}

fn on_chat_media_query(
    trigger: On<ChatMediaQuery>,
    mut projections: Query<&mut ChatMediaProjection, With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event_target();
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    let query = trigger.event().query.clone();
    let Some(request_id) = projection.start(query.clone()) else {
        return;
    };
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview,
            &projection.0,
        ),
    );
    if query.is_empty() {
        return;
    }
    let task = IoTaskPool::get().spawn(async move { chat_media_entries(request_id, query) });
    commands.spawn(ChatMediaListTask { webview, task });
}

fn on_chat_attach_paths(
    trigger: On<UiInput<ChatAttachPaths>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
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
    spawn_selected_attachment_tasks(
        trigger.event().webview,
        paths,
        vmux_core::host::wake::Wake::from_resource(proxy),
        &mut commands,
    );
}

fn on_chat_attachment_hydration_request(
    trigger: On<ChatAttachmentHydrationRequest>,
    mut projections: Query<&mut ChatAttachmentProjection, With<AgentChatView>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    let Ok(mut projection) = projections.get_mut(request.webview) else {
        return;
    };
    let paths = projection.start_hydration(&request.paths);
    spawn_chat_attachment_task(
        request.webview,
        ChatAttachmentDelivery::Hydrated,
        paths,
        vmux_core::host::wake::Wake::from_resource(proxy),
        &mut commands,
    );
}

fn on_chat_remove_attachment(
    trigger: On<UiInput<ChatRemoveAttachment>>,
    mut projections: Query<&mut ChatAttachmentProjection, With<AgentChatView>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    if !projection.remove_selected(&trigger.event().payload.path) {
        return;
    }
    commands.trigger(
        vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
            webview,
            &projection.state(),
        ),
    );
}

fn on_chat_pick_files(
    trigger: On<UiInput<ChatPickFiles>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut dialog = rfd::FileDialog::new();
    if let Some(home) = std::env::var_os("HOME") {
        dialog = dialog.set_directory(std::path::PathBuf::from(home));
    }
    let Some(paths) = dialog.pick_files() else {
        return;
    };
    spawn_selected_attachment_tasks(
        trigger.event().webview,
        paths,
        vmux_core::host::wake::Wake::from_resource(proxy),
        &mut commands,
    );
}

fn tiff_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Tiff).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).ok()?;
    Some(output.into_inner())
}

fn clipboard_image_path() -> Option<std::path::PathBuf> {
    if let Some(path) = vmux_clipboard::image_file_path() {
        return Some(std::path::PathBuf::from(path));
    }
    let png = vmux_clipboard::read_image_png()
        .or_else(|| vmux_clipboard::read_image_tiff().and_then(|bytes| tiff_to_png(&bytes)))?;
    let directory = std::env::temp_dir().join("vmux-prompt-attachments");
    std::fs::create_dir_all(&directory).ok()?;
    let path = directory.join(format!("clipboard-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

fn on_chat_paste_media(
    trigger: On<UiInput<ChatPasteMedia>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(path) = clipboard_image_path() else {
        return;
    };
    spawn_selected_attachment_tasks(
        trigger.event().webview,
        vec![path],
        vmux_core::host::wake::Wake::from_resource(proxy),
        &mut commands,
    );
}

fn drain_chat_attachment_tasks(
    mut tasks: Query<(Entity, &mut ChatAttachmentTask)>,
    mut projections: Query<
        (
            &mut ChatAttachmentProjection,
            &mut ChatTranscriptProjection,
            &mut ChatSnapshotProjection,
        ),
        With<AgentChatView>,
    >,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(attachments) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if let Ok((mut projection, mut transcript, mut snapshot)) =
            projections.get_mut(pending.webview)
        {
            match pending.delivery {
                ChatAttachmentDelivery::Selected => {
                    let incoming = ChatAttachments { attachments };
                    if projection.merge_selected(&incoming) {
                        commands.trigger(vmux_core::host::UiStateWrite::<
                            vmux_chat::state::ChatUiState,
                        >::from_event(pending.webview, &projection.state()));
                    }
                }
                ChatAttachmentDelivery::Hydrated => {
                    let selected_changed =
                        projection.finish_hydration(&pending.paths, &attachments);
                    let transcript_changed = projection.hydrate_transcript(&mut transcript.state);
                    let snapshot_changed = projection.hydrate_snapshot(&mut snapshot.0);
                    if selected_changed {
                        commands.trigger(vmux_core::host::UiStateWrite::<
                            vmux_chat::state::ChatUiState,
                        >::from_event(pending.webview, &projection.state()));
                    }
                    if transcript_changed {
                        commands.trigger(vmux_core::host::UiStateWrite::<
                            vmux_chat::state::ChatUiState,
                        >::from_event(pending.webview, &transcript.state));
                    }
                    if snapshot_changed {
                        commands.trigger(vmux_core::host::UiStateWrite::<
                            vmux_chat::state::ChatUiState,
                        >::from_event(pending.webview, &snapshot.0));
                    }
                }
            }
            let paths = projection.hydration_paths(&transcript.state, &snapshot.0);
            if !paths.is_empty() {
                commands.trigger(ChatAttachmentHydrationRequest {
                    webview: pending.webview,
                    paths,
                });
            }
        } else {
            let response = ChatAttachments {
                attachments: attachments.clone(),
            };
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_api::command_bar::CommandBarUiState,
            >::from_event(pending.webview, &response));
            if matches!(pending.delivery, ChatAttachmentDelivery::Selected) {
                let paths = attachments
                    .iter()
                    .filter(|attachment| attachment.mime_type.starts_with("image/"))
                    .map(|attachment| std::path::PathBuf::from(&attachment.path))
                    .collect();
                spawn_chat_attachment_task(
                    pending.webview,
                    ChatAttachmentDelivery::Hydrated,
                    paths,
                    vmux_core::host::wake::Wake::beside(proxy.as_deref()),
                    &mut commands,
                );
            }
        }
        commands.entity(entity).despawn();
    }
}

fn drain_chat_media_list_tasks(
    mut tasks: Query<(Entity, &mut ChatMediaListTask)>,
    mut projections: Query<&mut ChatMediaProjection, With<AgentChatView>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(entries) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if let Ok(mut projection) = projections.get_mut(pending.webview) {
            if !projection.finish(&entries) {
                commands.entity(entity).despawn();
                continue;
            }
            commands.trigger(
                vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                    pending.webview,
                    &projection.0,
                ),
            );
        } else {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_api::command_bar::CommandBarUiState,
            >::from_event(pending.webview, &entries));
        }
        if entries
            .entries
            .iter()
            .any(|entry| !entry.is_dir && entry.mime_type.starts_with("image/"))
        {
            let wake = vmux_core::host::wake::Wake::beside(proxy.as_deref());
            let task = IoTaskPool::get().spawn(async move {
                let _wake = wake;
                chat_media_previews(entries)
            });
            commands.spawn(ChatMediaPreviewTask {
                webview: pending.webview,
                task,
            });
        }
        commands.entity(entity).despawn();
    }
}

fn drain_chat_media_preview_tasks(
    mut tasks: Query<(Entity, &mut ChatMediaPreviewTask)>,
    mut projections: Query<&mut ChatMediaProjection, With<AgentChatView>>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(entries) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if let Ok(mut projection) = projections.get_mut(pending.webview) {
            if projection.finish(&entries) {
                commands.trigger(
                    vmux_core::host::UiStateWrite::<vmux_chat::state::ChatUiState>::from_event(
                        pending.webview,
                        &projection.0,
                    ),
                );
            }
        } else {
            commands.trigger(vmux_core::host::UiStateWrite::<
                vmux_api::command_bar::CommandBarUiState,
            >::from_event(pending.webview, &entries));
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_projection_rejects_stale_results() {
        let mut projection = ChatMediaProjection::default();
        let stale = projection.start("old".into()).unwrap();
        let current = projection.start("new".into()).unwrap();
        assert!(!projection.finish(&ChatMediaEntries {
            request_id: stale,
            query: "old".into(),
            entries: Vec::new(),
        }));
        assert!(projection.0.loading);
        assert!(projection.finish(&ChatMediaEntries {
            request_id: current,
            query: "new".into(),
            entries: Vec::new(),
        }));
        assert!(!projection.0.loading);
    }

    #[test]
    fn attachment_projection_deduplicates_requests_and_hydrates_every_surface() {
        let image = ChatAttachment {
            path: "/tmp/image.png".into(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            size: 4,
            preview_data_url: String::new(),
        };
        let mut projection = ChatAttachmentProjection::default();
        assert!(projection.merge_selected(&ChatAttachments {
            attachments: vec![image.clone(), image.clone()],
        }));
        let mut transcript = ChatTranscriptState {
            items: vec![ChatItem::User {
                text: "inspect".into(),
                context: None,
                attachments: vec![image.clone()],
                created_at_ms: 0,
            }],
            ..Default::default()
        };
        let mut snapshot = ChatSnapshot {
            queued: vec![vmux_chat::event::QueuedPromptSnapshot {
                id: 1,
                text: String::new(),
                attachments: vec![image.clone()],
            }],
            ..Default::default()
        };

        let paths = projection.hydration_paths(&transcript, &snapshot);
        assert_eq!(paths, ["/tmp/image.png"]);
        assert_eq!(projection.start_hydration(&paths).len(), 1);
        assert!(
            projection
                .start_hydration(&["/tmp/image.png".into()])
                .is_empty()
        );

        let preview = ChatAttachment {
            preview_data_url: "data:image/png;base64,cG5n".into(),
            ..image
        };
        assert!(projection.finish_hydration(&paths, std::slice::from_ref(&preview)));
        assert!(projection.hydrate_transcript(&mut transcript));
        assert!(projection.hydrate_snapshot(&mut snapshot));
        assert_eq!(
            projection.selected[0].preview_data_url,
            preview.preview_data_url
        );
        let ChatItem::User { attachments, .. } = &transcript.items[0] else {
            panic!("expected user item");
        };
        assert_eq!(attachments[0].preview_data_url, preview.preview_data_url);
        assert_eq!(
            snapshot.queued[0].attachments[0].preview_data_url,
            preview.preview_data_url
        );
        assert!(
            projection
                .hydration_paths(&transcript, &snapshot)
                .is_empty()
        );
    }

    #[test]
    fn attachment_projection_removes_selected_paths() {
        let mut projection = ChatAttachmentProjection::default();
        projection.merge_selected(&ChatAttachments {
            attachments: vec![ChatAttachment {
                path: "/tmp/image.png".into(),
                ..Default::default()
            }],
        });

        assert!(projection.remove_selected("/tmp/image.png"));
        assert!(projection.selected.is_empty());
        assert!(!projection.remove_selected("/tmp/image.png"));
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
        let image = image::RgbaImage::from_pixel(2048, 1024, image::Rgba([20, 40, 60, 255]));
        image.save(&path).unwrap();
        let source_size = std::fs::metadata(&path).unwrap().len();

        let data_url = media_thumbnail_data_url(&path, source_size);

        std::fs::remove_file(path).unwrap();
        let encoded = data_url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let thumbnail = image::load_from_memory(&bytes).unwrap();
        assert_eq!(
            thumbnail.width().max(thumbnail.height()),
            MEDIA_THUMBNAIL_MAX_EDGE,
            "a preview is bounded by the edge the composer and the transcript draw it at"
        );
    }

    #[test]
    fn clipboard_tiff_is_converted_to_png() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            4,
            3,
            image::Rgba([20, 40, 60, 255]),
        ));
        let mut tiff = std::io::Cursor::new(Vec::new());
        image.write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();

        let png = tiff_to_png(&tiff.into_inner()).unwrap();
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();

        assert_eq!((decoded.width(), decoded.height()), (4, 3));
    }
}
