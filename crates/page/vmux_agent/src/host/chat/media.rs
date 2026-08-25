use base64::Engine;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};

use vmux_chat::event::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    ChatAttachPaths, ChatAttachment, ChatAttachmentPreviewRequest, ChatAttachments,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatPasteMedia, ChatPickFiles,
};

pub(super) struct ChatMediaPlugin;

impl Plugin for ChatMediaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(
            ChatPickFiles,
            ChatPasteMedia,
            ChatMediaListRequest,
            ChatAttachPaths,
            ChatAttachmentPreviewRequest,
        )>::for_hosts(&["agent", "start"]))
            .add_observer(on_chat_pick_files)
            .add_observer(on_chat_paste_media)
            .add_observer(on_chat_media_list_request)
            .add_observer(on_chat_attach_paths)
            .add_observer(on_chat_attachment_preview_request)
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
    event: &'static str,
    task: Task<ChatAttachments>,
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

const MEDIA_THUMBNAIL_SOURCE_LIMIT: u64 = 25 * 1024 * 1024;

const MEDIA_THUMBNAIL_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;

const MEDIA_THUMBNAIL_MAX_EDGE: u32 = 96;

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
    event: &'static str,
    paths: Vec<std::path::PathBuf>,
    previews: bool,
    commands: &mut Commands,
) {
    if paths.is_empty() {
        return;
    }
    let task = IoTaskPool::get().spawn(async move {
        ChatAttachments {
            attachments: paths
                .into_iter()
                .filter_map(if previews {
                    chat_attachment_preview
                } else {
                    chat_attachment
                })
                .collect(),
        }
    });
    commands.spawn(ChatAttachmentTask {
        webview,
        event,
        task,
    });
}

fn spawn_selected_attachment_tasks(
    webview: Entity,
    paths: Vec<std::path::PathBuf>,
    commands: &mut Commands,
) {
    spawn_chat_attachment_task(
        webview,
        CHAT_ATTACHMENTS_EVENT,
        paths.clone(),
        false,
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

fn on_chat_attach_paths(trigger: On<BinReceive<ChatAttachPaths>>, mut commands: Commands) {
    let paths = trigger
        .event()
        .payload
        .paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .collect();
    spawn_selected_attachment_tasks(trigger.event().webview, paths, &mut commands);
}

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
        true,
        &mut commands,
    );
}

fn on_chat_pick_files(trigger: On<BinReceive<ChatPickFiles>>, mut commands: Commands) {
    let mut dialog = rfd::FileDialog::new();
    if let Some(home) = std::env::var_os("HOME") {
        dialog = dialog.set_directory(std::path::PathBuf::from(home));
    }
    let Some(paths) = dialog.pick_files() else {
        return;
    };
    spawn_selected_attachment_tasks(trigger.event().webview, paths, &mut commands);
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

fn on_chat_paste_media(trigger: On<BinReceive<ChatPasteMedia>>, mut commands: Commands) {
    let Some(path) = clipboard_image_path() else {
        return;
    };
    spawn_selected_attachment_tasks(trigger.event().webview, vec![path], &mut commands);
}

fn drain_chat_attachment_tasks(
    mut tasks: Query<(Entity, &mut ChatAttachmentTask)>,
    mut commands: Commands,
) {
    for (entity, mut pending) in &mut tasks {
        let Some(attachments) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let preview_paths = (pending.event == CHAT_ATTACHMENTS_EVENT).then(|| {
            attachments
                .attachments
                .iter()
                .map(|attachment| std::path::PathBuf::from(&attachment.path))
                .collect::<Vec<_>>()
        });
        commands.trigger(BinHostEmitEvent::from_rkyv(
            pending.webview,
            pending.event,
            &attachments,
        ));
        if let Some(paths) = preview_paths {
            spawn_chat_attachment_task(
                pending.webview,
                CHAT_ATTACHMENT_PREVIEWS_EVENT,
                paths,
                true,
                &mut commands,
            );
        }
        commands.entity(entity).despawn();
    }
}

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
        if entries
            .entries
            .iter()
            .any(|entry| !entry.is_dir && entry.mime_type.starts_with("image/"))
        {
            let task = IoTaskPool::get().spawn(async move { chat_media_previews(entries) });
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

#[cfg(test)]
mod tests {
    use super::*;

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
