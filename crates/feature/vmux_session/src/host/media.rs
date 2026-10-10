use base64::Engine;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use ignore::WalkBuilder;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use super::prompt::ChatPromptFocusRevision;
use super::session::{
    ChatAttachmentProjection, ChatMediaProjection, ChatSnapshotProjection,
    ChatTranscriptProjection, ChatView, SessionViews,
};
use crate as vmux_session;
use crate::event::{
    ChatAttachPaths, ChatAttachment, ChatAttachments, ChatComposerMedia, ChatItem,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatMediaQueryRequest, ChatPasteMedia,
    ChatPickFiles, ChatRemoveAttachment, ChatSnapshot, ChatTranscriptState,
};
use vmux_api::prompt_media::{PromptComposerAttachment, PromptMediaOption};
use vmux_ecs::Cwd;
use vmux_ui::file_icon::FilePath;

const MAX_COMPOSER_FILE_RESULTS: usize = 100;
const MAX_COMPOSER_FILE_SCAN: usize = 100_000;

pub struct ChatMediaPlugin;

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
            .add_observer(pick_files)
            .add_observer(paste)
            .add_observer(query_request)
            .add_observer(query)
            .add_observer(list_request)
            .add_observer(attach_paths)
            .add_observer(remove_attachment)
            .add_observer(hydrate_attachments)
            .add_systems(
                Update,
                (
                    drain_attachment_tasks,
                    drain_list_tasks,
                    drain_preview_tasks,
                ),
            )
            .add_systems(PostUpdate, project);
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

#[derive(bevy::ecs::system::SystemParam)]
struct AttachmentTasks<'w, 's> {
    proxy: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
    commands: Commands<'w, 's>,
}

impl AttachmentTasks<'_, '_> {
    fn spawn(&mut self, webview: Entity, delivery: ChatAttachmentDelivery, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let requested = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let wake = vmux_ecs::wake::Wake::beside(self.proxy.as_deref());
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
        self.commands.spawn(ChatAttachmentTask {
            webview,
            delivery,
            paths: requested,
            task,
        });
    }

    fn selected(&mut self, webview: Entity, paths: Vec<PathBuf>) {
        self.spawn(webview, ChatAttachmentDelivery::Selected, paths);
    }
}

#[derive(Event)]
pub struct ChatAttachmentHydrationRequest {
    pub webview: Entity,
    pub paths: Vec<String>,
}

#[derive(Component, Default, PartialEq, Eq)]
struct ChatComposerMediaProjection(ChatComposerMedia);

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
pub struct ChatMediaQuery {
    #[event_target]
    webview: Entity,
    query: Option<String>,
}

impl ChatMediaQuery {
    pub fn new(webview: Entity, query: Option<String>) -> Self {
        Self { webview, query }
    }
}

impl ChatMediaProjection {
    fn start(&mut self, query: Option<String>) -> Option<(u64, String)> {
        self.0.request_id = self.0.request_id.wrapping_add(1).max(1);
        let Some(query) = query else {
            self.0.query.clear();
            self.0.entries.clear();
            self.0.loading = false;
            return None;
        };
        self.0.query.clone_from(&query);
        self.0.loading = self.0.entries.is_empty();
        Some((self.0.request_id, query))
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

struct ComposerFileQuery {
    root: PathBuf,
    query: String,
}

impl ComposerFileQuery {
    fn new(root: PathBuf, query: String) -> Self {
        Self { root, query }
    }

    fn search(self, request_id: u64) -> ChatMediaEntries {
        let query = decode_media_query_path(&self.query)
            .to_string_lossy()
            .trim_start_matches("./")
            .to_ascii_lowercase();
        let mut hits = Vec::new();
        let walk = WalkBuilder::new(&self.root)
            .hidden(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .build();
        for entry in walk.take(MAX_COMPOSER_FILE_SCAN) {
            let Ok(entry) = entry else {
                continue;
            };
            if entry.depth() == 0 {
                continue;
            }
            let Some(kind) = entry.file_type() else {
                continue;
            };
            let Ok(relative) = entry.path().strip_prefix(&self.root) else {
                continue;
            };
            let relative = relative.to_string_lossy().into_owned();
            let Some(rank) = ComposerFileHit::rank(&relative, kind.is_dir(), &query) else {
                continue;
            };
            hits.push(ComposerFileHit {
                path: entry.path().to_path_buf(),
                relative,
                is_dir: kind.is_dir(),
                rank,
            });
        }
        hits.sort_by(ComposerFileHit::compare);
        hits.truncate(MAX_COMPOSER_FILE_RESULTS);
        let mut entries = Vec::with_capacity(hits.len());
        for hit in hits {
            entries.push(hit.entry());
        }
        ChatMediaEntries {
            request_id,
            query: self.query,
            entries,
        }
    }
}

struct ComposerFileHit {
    path: PathBuf,
    relative: String,
    is_dir: bool,
    rank: u8,
}

impl ComposerFileHit {
    fn rank(relative: &str, is_dir: bool, query: &str) -> Option<u8> {
        if query.is_empty() {
            return Some(0);
        }
        let relative = relative.to_ascii_lowercase();
        let name = Path::new(&relative)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(relative.as_str());
        if name == query {
            return Some(0);
        }
        if name.starts_with(query) {
            return Some(1);
        }
        if relative.starts_with(query) {
            return Some(2);
        }
        if name.contains(query) {
            return Some(3);
        }
        if relative.contains(query) {
            return Some(4);
        }
        Self::subsequence(&relative, query).then_some(if is_dir { 5 } else { 6 })
    }

    fn subsequence(value: &str, query: &str) -> bool {
        let mut query = query.chars();
        let Some(mut wanted) = query.next() else {
            return true;
        };
        for character in value.chars() {
            if character != wanted {
                continue;
            }
            let Some(next) = query.next() else {
                return true;
            };
            wanted = next;
        }
        false
    }

    fn compare(left: &Self, right: &Self) -> Ordering {
        left.rank
            .cmp(&right.rank)
            .then_with(|| {
                left.relative
                    .matches('/')
                    .count()
                    .cmp(&right.relative.matches('/').count())
            })
            .then_with(|| right.is_dir.cmp(&left.is_dir))
            .then_with(|| {
                left.relative
                    .to_lowercase()
                    .cmp(&right.relative.to_lowercase())
            })
    }

    fn entry(self) -> ChatMediaEntry {
        let name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.relative.clone());
        let parent = Path::new(&self.relative)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mime_type = if self.is_dir {
            String::new()
        } else {
            attachment_mime(&self.path)
        };
        ChatMediaEntry {
            path: self.path.to_string_lossy().into_owned(),
            name,
            parent,
            mime_type,
            is_dir: self.is_dir,
            preview_data_url: String::new(),
        }
    }
}

impl ChatAttachmentProjection {
    fn merge_selected(&mut self, incoming: &ChatAttachments) -> bool {
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

    fn remove_selected(&mut self, path: &str) -> bool {
        let previous = self.selected.len();
        self.selected.retain(|attachment| attachment.path != path);
        self.selected.len() != previous
    }

    pub fn clear_selected(&mut self) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        self.selected.clear();
        true
    }

    pub fn state(&self) -> ChatAttachments {
        ChatAttachments {
            attachments: self.selected.clone(),
        }
    }

    fn start_hydration(&mut self, paths: &[String]) -> Vec<PathBuf> {
        let mut started = Vec::new();
        for path in paths {
            if path.is_empty() || self.resolved.contains(path) || !self.pending.insert(path.clone())
            {
                continue;
            }
            started.push(PathBuf::from(path));
        }
        started
    }

    fn finish_hydration(&mut self, requested: &[String], attachments: &[ChatAttachment]) -> bool {
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

    pub fn hydrate_transcript(&self, state: &mut ChatTranscriptState) -> bool {
        let mut changed = false;
        for item in &mut state.items {
            let ChatItem::User { attachments, .. } = item else {
                continue;
            };
            changed |= self.hydrate(attachments);
        }
        changed
    }

    pub fn hydrate_snapshot(&self, snapshot: &mut ChatSnapshot) -> bool {
        let mut changed = false;
        for prompt in &mut snapshot.queued {
            changed |= self.hydrate(&mut prompt.attachments);
        }
        changed
    }

    pub fn hydration_paths(
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

fn attachment_mime(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    if let Some(mime) = vmux_api::media::MediaKind::mime(&path_str) {
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

fn chat_attachment(path: PathBuf) -> Option<ChatAttachment> {
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

fn media_thumbnail_data_url(path: &Path, source_size: u64) -> String {
    if source_size > MEDIA_THUMBNAIL_SOURCE_LIMIT {
        return String::new();
    }
    let Some(mime) = vmux_api::media::MediaKind::image_mime(&path.to_string_lossy()) else {
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

fn chat_attachment_preview(path: PathBuf) -> Option<ChatAttachment> {
    let mut attachment = chat_attachment(path)?;
    if !attachment.mime_type.starts_with("image/") {
        return None;
    }
    attachment.preview_data_url =
        media_thumbnail_data_url(Path::new(&attachment.path), attachment.size);
    (!attachment.preview_data_url.is_empty()).then_some(attachment)
}

type ChangedMediaViews<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChatMediaProjection,
        &'static ChatAttachmentProjection,
        Option<&'static ChatComposerMediaProjection>,
    ),
    Or<(
        Changed<ChatMediaProjection>,
        Changed<ChatAttachmentProjection>,
    )>,
>;

fn project(views: ChangedMediaViews, mut commands: Commands) {
    for (webview, media, attachments, current) in &views {
        let mut options = Vec::with_capacity(media.0.entries.len());
        for entry in &media.0.entries {
            options.push(PromptMediaOption {
                key: format!("media-{}", entry.path),
                name: entry.name.clone(),
                display_path: entry.display_path(),
                preview_data_url: entry.preview_data_url.clone(),
                label: FilePath(&entry.name).extension_label(),
                is_dir: entry.is_dir,
            });
        }
        let state = attachments.state();
        let mut rendered = Vec::with_capacity(state.attachments.len());
        for (index, attachment) in state.attachments.iter().enumerate() {
            rendered.push(PromptComposerAttachment {
                key: format!("attachment-{}", attachment.path),
                name: attachment.name.clone(),
                label: FilePath(&attachment.name).extension_label(),
                preview_data_url: attachment.preview_data_url.clone(),
                remove_index: Some(index as u32),
            });
        }
        let projection = ChatComposerMediaProjection(ChatComposerMedia {
            options,
            attachments: rendered,
        });
        if current.is_some_and(|current| current == &projection) {
            continue;
        }
        commands
            .entity(webview)
            .insert(ChatComposerMediaProjection(projection.0.clone()));
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                webview,
                &projection.0,
            ),
        );
    }
}

fn decode_media_query_path(value: &str) -> PathBuf {
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

fn chat_media_entries(request_id: u64, query: String) -> ChatMediaEntries {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
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
        entry.preview_data_url = media_thumbnail_data_url(Path::new(&entry.path), source_size);
        if !entry.preview_data_url.is_empty() {
            remaining_thumbnail_bytes = remaining_thumbnail_bytes.saturating_sub(source_size);
        }
    }
    response
}

fn list_request(trigger: On<UiInput<ChatMediaListRequest>>, mut commands: Commands) {
    let request = trigger.event().payload.clone();
    let task = IoTaskPool::get()
        .spawn(async move { chat_media_entries(request.request_id, request.query) });
    commands.spawn(ChatMediaListTask {
        webview: trigger.event().webview,
        task,
    });
}

fn query_request(trigger: On<UiInput<ChatMediaQueryRequest>>, mut commands: Commands) {
    commands.trigger(ChatMediaQuery::new(
        trigger.event().webview,
        Some(trigger.event().payload.query.clone()),
    ));
}

fn query(
    trigger: On<ChatMediaQuery>,
    mut projections: Query<&mut ChatMediaProjection, With<ChatView>>,
    views: SessionViews,
    sessions: Query<&Cwd>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event_target();
    let Ok(mut projection) = projections.get_mut(webview) else {
        return;
    };
    let query = trigger.event().query.clone();
    let request = projection.start(query);
    commands.trigger(
        vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
            webview,
            &projection.0,
        ),
    );
    let Some((request_id, query)) = request else {
        return;
    };
    let Some(root) = views
        .session(webview)
        .and_then(|session| sessions.get(session).ok())
        .map(|cwd| cwd.0.clone())
    else {
        projection.0.loading = false;
        commands.trigger(
            vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                webview,
                &projection.0,
            ),
        );
        return;
    };
    let wake = vmux_ecs::wake::Wake::beside(proxy.as_deref());
    let task = IoTaskPool::get().spawn(async move {
        let _wake = wake;
        ComposerFileQuery::new(root, query).search(request_id)
    });
    commands.spawn(ChatMediaListTask { webview, task });
}

fn attach_paths(trigger: On<UiInput<ChatAttachPaths>>, mut tasks: AttachmentTasks) {
    let paths = trigger
        .event()
        .payload
        .paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect();
    tasks.selected(trigger.event().webview, paths);
}

fn hydrate_attachments(
    trigger: On<ChatAttachmentHydrationRequest>,
    mut projections: Query<&mut ChatAttachmentProjection, With<ChatView>>,
    mut tasks: AttachmentTasks,
) {
    let request = trigger.event();
    let Ok(mut projection) = projections.get_mut(request.webview) else {
        return;
    };
    let paths = projection.start_hydration(&request.paths);
    tasks.spawn(request.webview, ChatAttachmentDelivery::Hydrated, paths);
}

fn remove_attachment(
    trigger: On<UiInput<ChatRemoveAttachment>>,
    mut projections: Query<
        (&mut ChatAttachmentProjection, &mut ChatPromptFocusRevision),
        With<ChatView>,
    >,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok((mut projection, mut focus)) = projections.get_mut(webview) else {
        return;
    };
    if !projection.remove_selected(&trigger.event().payload.path) {
        return;
    }
    commands.trigger(
        vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
            webview,
            &projection.state(),
        ),
    );
    commands.trigger(
        vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
            webview,
            &focus.next(),
        ),
    );
}

fn pick_files(trigger: On<UiInput<ChatPickFiles>>, mut tasks: AttachmentTasks) {
    let mut dialog = rfd::FileDialog::new();
    if let Some(home) = std::env::var_os("HOME") {
        dialog = dialog.set_directory(PathBuf::from(home));
    }
    let Some(paths) = dialog.pick_files() else {
        return;
    };
    tasks.selected(trigger.event().webview, paths);
}

fn tiff_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Tiff).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).ok()?;
    Some(output.into_inner())
}

fn clipboard_image_path() -> Option<PathBuf> {
    if let Some(path) = vmux_clipboard::Clipboard::image_file_path() {
        return Some(PathBuf::from(path));
    }
    let png = vmux_clipboard::Clipboard::read_image_png().or_else(|| {
        vmux_clipboard::Clipboard::read_image_tiff().and_then(|bytes| tiff_to_png(&bytes))
    })?;
    let directory = std::env::temp_dir().join("vmux-prompt-attachments");
    std::fs::create_dir_all(&directory).ok()?;
    let path = directory.join(format!("clipboard-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&path, png).ok()?;
    Some(path)
}

fn paste(trigger: On<UiInput<ChatPasteMedia>>, mut tasks: AttachmentTasks) {
    let Some(path) = clipboard_image_path() else {
        return;
    };
    tasks.selected(trigger.event().webview, vec![path]);
}

fn drain_attachment_tasks(
    mut tasks: Query<(Entity, &mut ChatAttachmentTask)>,
    mut projections: Query<
        (
            &mut ChatAttachmentProjection,
            &mut ChatPromptFocusRevision,
            &mut ChatTranscriptProjection,
            &mut ChatSnapshotProjection,
        ),
        With<ChatView>,
    >,
    mut attachment_tasks: AttachmentTasks,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(attachments) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if let Ok((mut projection, mut focus, mut transcript, mut snapshot)) =
            projections.get_mut(pending.webview)
        {
            match pending.delivery {
                ChatAttachmentDelivery::Selected => {
                    let incoming = ChatAttachments { attachments };
                    if projection.merge_selected(&incoming) {
                        attachment_tasks.commands.trigger(vmux_ecs::UiStateWrite::<
                            vmux_session::state::ChatUiState,
                        >::from_event(
                            pending.webview, &projection.state()
                        ));
                        attachment_tasks.commands.trigger(vmux_ecs::UiStateWrite::<
                            vmux_session::state::ChatUiState,
                        >::from_event(
                            pending.webview, &focus.next()
                        ));
                    }
                }
                ChatAttachmentDelivery::Hydrated => {
                    let selected_changed =
                        projection.finish_hydration(&pending.paths, &attachments);
                    let transcript_changed = projection.hydrate_transcript(&mut transcript.state);
                    let snapshot_changed = projection.hydrate_snapshot(&mut snapshot.0);
                    if selected_changed {
                        attachment_tasks.commands.trigger(vmux_ecs::UiStateWrite::<
                            vmux_session::state::ChatUiState,
                        >::from_event(
                            pending.webview, &projection.state()
                        ));
                    }
                    if transcript_changed {
                        attachment_tasks.commands.trigger(vmux_ecs::UiStateWrite::<
                            vmux_session::state::ChatUiState,
                        >::from_event(
                            pending.webview, &transcript.state
                        ));
                    }
                    if snapshot_changed {
                        attachment_tasks.commands.trigger(vmux_ecs::UiStateWrite::<
                            vmux_session::state::ChatUiState,
                        >::from_event(
                            pending.webview, &snapshot.0
                        ));
                    }
                }
            }
            let paths = projection.hydration_paths(&transcript.state, &snapshot.0);
            if !paths.is_empty() {
                attachment_tasks
                    .commands
                    .trigger(ChatAttachmentHydrationRequest {
                        webview: pending.webview,
                        paths,
                    });
            }
        } else {
            let response = ChatAttachments {
                attachments: attachments.clone(),
            };
            attachment_tasks.commands.trigger(vmux_ecs::UiStateWrite::<
                vmux_api::command_bar::CommandBarUiState,
            >::from_event(pending.webview, &response));
            if matches!(pending.delivery, ChatAttachmentDelivery::Selected) {
                let paths = attachments
                    .iter()
                    .filter(|attachment| attachment.mime_type.starts_with("image/"))
                    .map(|attachment| PathBuf::from(&attachment.path))
                    .collect();
                attachment_tasks.spawn(pending.webview, ChatAttachmentDelivery::Hydrated, paths);
            }
        }
        attachment_tasks.commands.entity(entity).despawn();
    }
}

fn drain_list_tasks(
    mut tasks: Query<(Entity, &mut ChatMediaListTask)>,
    mut projections: Query<&mut ChatMediaProjection, With<ChatView>>,
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
                vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                    pending.webview,
                    &projection.0,
                ),
            );
        } else {
            commands.trigger(vmux_ecs::UiStateWrite::<
                vmux_api::command_bar::CommandBarUiState,
            >::from_event(pending.webview, &entries));
        }
        if entries
            .entries
            .iter()
            .any(|entry| !entry.is_dir && entry.mime_type.starts_with("image/"))
        {
            let wake = vmux_ecs::wake::Wake::beside(proxy.as_deref());
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

fn drain_preview_tasks(
    mut tasks: Query<(Entity, &mut ChatMediaPreviewTask)>,
    mut projections: Query<&mut ChatMediaProjection, With<ChatView>>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(entries) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if let Ok(mut projection) = projections.get_mut(pending.webview) {
            if projection.finish(&entries) {
                commands.trigger(
                    vmux_ecs::UiStateWrite::<vmux_session::state::ChatUiState>::from_event(
                        pending.webview,
                        &projection.0,
                    ),
                );
            }
        } else {
            commands.trigger(vmux_ecs::UiStateWrite::<
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
        let stale = projection.start(Some("old".into())).unwrap().0;
        projection.0.entries.push(ChatMediaEntry {
            path: "/repo/old.rs".into(),
            name: "old.rs".into(),
            ..Default::default()
        });
        let current = projection.start(Some("new".into())).unwrap().0;
        assert_eq!(projection.0.entries[0].name, "old.rs");
        assert!(!projection.0.loading);
        assert!(!projection.finish(&ChatMediaEntries {
            request_id: stale,
            query: "old".into(),
            entries: Vec::new(),
        }));
        assert!(!projection.0.loading);
        assert_eq!(projection.0.entries[0].name, "old.rs");
        assert!(projection.finish(&ChatMediaEntries {
            request_id: current,
            query: "new".into(),
            entries: Vec::new(),
        }));
        assert!(!projection.0.loading);
    }

    #[test]
    fn project_file_query_lists_source_files_for_bare_and_typed_mentions() {
        let root =
            std::env::temp_dir().join(format!("vmux-composer-file-query-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("docs/readme.md"), "docs").unwrap();

        let all = ComposerFileQuery::new(root.clone(), String::new()).search(1);
        assert!(all.entries.iter().any(|entry| entry.name == "main.rs"));
        assert!(all.entries.iter().any(|entry| entry.name == "readme.md"));

        let matched = ComposerFileQuery::new(root.clone(), "main".to_string()).search(2);
        assert_eq!(matched.entries[0].name, "main.rs");
        assert_eq!(matched.entries[0].parent, "src");
        assert_eq!(matched.entries[0].display_path(), "src/main.rs");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn closing_the_file_selector_clears_its_projection() {
        let mut projection = ChatMediaProjection::default();
        projection.start(Some(String::new()));
        assert!(projection.0.loading);

        assert_eq!(projection.start(None), None);
        assert!(!projection.0.loading);
        assert!(projection.0.query.is_empty());
        assert!(projection.0.entries.is_empty());
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
            queued: vec![vmux_session::event::QueuedPromptSnapshot {
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
            PathBuf::from("Pictures/My Image%.png")
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
