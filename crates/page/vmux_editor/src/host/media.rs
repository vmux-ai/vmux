use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use vmux_core::event::{
    FileMediaEvent, FileOpenExternalRequest, FilePreviewEvent, FilePreviewRequest, FileVideoRect,
    PreviewKind,
};

use crate::host::edit_state::FileView;
use crate::host::file_lifecycle::{EditorFileLoadedSet, FileDir};
use crate::host::preview;
use crate::host::status::FileInitialMetaSent;

pub(crate) struct EditorMediaPlugin;

impl Plugin for EditorMediaPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            FilePreviewRequest,
            FileOpenExternalRequest,
            FileVideoRect,
        )>::default())
            .add_systems(
                Update,
                (
                    sync_media_allowlist.after(EditorFileLoadedSet),
                    send_initial_media
                        .after(EditorFileLoadedSet)
                        .after(sync_media_allowlist),
                    (detach_video_overlays, attach_video_overlays).chain(),
                    drain_thumb_tasks,
                ),
            )
            .add_observer(on_file_preview_request)
            .add_observer(on_file_open_external)
            .add_observer(on_file_video_rect);
    }
}

#[derive(Component, Clone, Debug)]
pub struct FileMedia {
    pub kind: vmux_core::media::MediaKind,
    pub mime: String,
}

#[derive(Component)]
struct ThumbTask {
    webview: Entity,
    task: Task<(String, Result<Vec<u8>, String>)>,
}

type ReadyMedia = (
    Without<FileInitialMetaSent>,
    With<vmux_core::page::PageReady>,
);

fn sync_media_allowlist(media: Query<&FileView, With<FileMedia>>, dirs: Query<&FileDir>) {
    let mut paths: std::collections::HashSet<std::path::PathBuf> =
        media.iter().map(|file| file.path.clone()).collect();
    for dir in &dirs {
        for entry in &dir.entries {
            paths.insert(std::path::PathBuf::from(&entry.path));
        }
    }
    set_media_allowlist(paths);
}

fn send_initial_media(
    media: Query<(Entity, &FileView, &FileMedia), ReadyMedia>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, file, media) in &media {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &FileMediaEvent {
                kind: media.kind,
                mime: media.mime.clone(),
                url: file.raw_media_url(),
                abs_path: file.path.to_string_lossy().into_owned(),
            },
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn needs_native_video(path: &Path) -> bool {
    vmux_core::media::is_proprietary_video(&path.to_string_lossy())
}

fn attach_video_overlays(
    media: Query<(Entity, &FileView, &FileMedia)>,
    browsers: NonSend<Browsers>,
) {
    for (entity, file, media) in &media {
        if media.kind != vmux_core::media::MediaKind::Video || !needs_native_video(&file.path) {
            continue;
        }
        if !browsers.has_browser(entity) {
            continue;
        }
        browsers.attach_media_overlay(&entity, &file.path.to_string_lossy());
    }
}

fn on_file_video_rect(
    trigger: On<BinReceive<FileVideoRect>>,
    file_views: Query<(), With<FileView>>,
    browsers: NonSend<Browsers>,
) {
    let entity = trigger.event().webview;
    if file_views.get(entity).is_err() || !browsers.has_browser(entity) {
        return;
    }
    let rect = &trigger.event().payload;
    if !vmux_core::media::is_proprietary_video(&rect.path) || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    browsers.set_media_overlay(&entity, &rect.path, (rect.x, rect.y, rect.w, rect.h));
}

fn detach_video_overlays(
    mut removed_media: RemovedComponents<FileMedia>,
    mut removed_dir: RemovedComponents<FileDir>,
    browsers: NonSend<Browsers>,
) {
    for entity in removed_media.read().chain(removed_dir.read()) {
        browsers.detach_media_overlay(&entity);
    }
}

fn on_file_preview_request(
    trigger: On<BinReceive<FilePreviewRequest>>,
    file_views: Query<(), With<FileView>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if file_views.get(entity).is_err() {
        return;
    }
    let request = trigger.event().payload.clone();
    let path = PathBuf::from(&request.path);
    if !needs_native_video(&path) {
        browsers.detach_media_overlay(&entity);
    }
    if request.thumb && preview::is_image_path(&path) {
        let within_cap = std::fs::metadata(&path)
            .map(|metadata| metadata.len() <= preview::IMAGE_BYTES_CAP)
            .unwrap_or(false);
        if !within_cap {
            return;
        }
        let path = request.path.clone();
        let task = IoTaskPool::get().spawn(async move {
            let result = std::fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| preview::downscale_to_png(&bytes, preview::THUMB_MAX_EDGE));
            (path, result)
        });
        commands.spawn(ThumbTask {
            webview: entity,
            task,
        });
        return;
    }
    if !browsers.can_emit_to(&entity) {
        return;
    }
    let kind = preview::build_preview_sync(&path);
    commands.trigger(BinHostEmitEvent::from_event(
        entity,
        &FilePreviewEvent {
            path: request.path,
            thumb: false,
            kind,
        },
    ));
}

fn drain_thumb_tasks(
    mut tasks: Query<(Entity, &mut ThumbTask)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut task) in &mut tasks {
        let Some((path, result)) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        let webview = task.webview;
        commands.entity(task_entity).despawn();
        if let Ok(bytes) = result
            && browsers.can_emit_to(&webview)
        {
            commands.trigger(BinHostEmitEvent::from_event(
                webview,
                &FilePreviewEvent {
                    path,
                    thumb: true,
                    kind: PreviewKind::Image {
                        mime: "image/png".to_string(),
                        bytes,
                    },
                },
            ));
        }
    }
}

fn on_file_open_external(
    trigger: On<BinReceive<FileOpenExternalRequest>>,
    media: Query<&FileView, With<FileMedia>>,
) {
    let entity = trigger.event().webview;
    let Ok(file) = media.get(entity) else {
        return;
    };
    let path = PathBuf::from(&trigger.event().payload.path);
    if file.path != path {
        return;
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    let _ = std::process::Command::new(program).arg(&path).spawn();
}
