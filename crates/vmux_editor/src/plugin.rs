use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::PageMetadata;
use vmux_core::event::*;
use vmux_core::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_layout::Browser;

use crate::dir::{list_dir, parent_listing};
use crate::edit::highlight_cache::HighlightCache;
use crate::edit::{EditCommand, EditCore, Selection};
use crate::keymap::{KeyInput, Keymap, KeymapKindExt, Mods};
use crate::preview;
use crate::viewport::{clamp_top_line, rows_from_viewport, window_range};

const SCROLL_OVERSCAN: u32 = 48;

#[derive(Component, Clone, Debug)]
pub struct FileView {
    pub path: PathBuf,
}

#[derive(Component, Clone, Debug)]
pub struct FileBuffer {
    pub language: String,
}

impl FileBuffer {
    fn error(message: String) -> Self {
        Self { language: message }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct FileViewport {
    pub top_line: u32,
    pub rows: u16,
}

#[derive(Component, Clone, Debug)]
pub struct FileDir {
    pub entries: Vec<FileDirEntry>,
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

#[derive(Component)]
pub struct EditState {
    pub core: EditCore,
    pub hl: HighlightCache,
}

#[derive(Component)]
pub struct EditorKeymap(pub Box<dyn Keymap>);

#[derive(Component)]
struct LspEditDirty;

struct ClipboardHandle(Option<arboard::Clipboard>);

#[derive(Default)]
struct SelfWrites(std::collections::HashMap<PathBuf, std::time::Instant>);

#[derive(Component)]
pub struct FileInitialMetaSent;

#[derive(Component)]
pub struct FileThemeSent;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type UnloadedFileView = (
    Without<FileBuffer>,
    Without<FileDir>,
    Without<FileMedia>,
    Without<EditState>,
);
type ReadyUnsentMeta = (
    Without<FileInitialMetaSent>,
    With<vmux_core::page::PageReady>,
);
type ReadyUnsentTheme = (
    With<FileView>,
    Without<FileThemeSent>,
    With<vmux_core::page::PageReady>,
);

fn path_from_files_url(url: &str) -> Option<PathBuf> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "file" {
        return None;
    }
    let raw = parsed.path();
    if raw.is_empty() {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .ok()?;
    let path = PathBuf::from(decoded.as_ref());
    path.is_absolute().then_some(path)
}

fn new_file_view_bundle(
    url: &str,
    path: PathBuf,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> impl Bundle {
    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    (
        (
            FileView { path },
            FileViewport {
                top_line: 0,
                rows: 0,
            },
            Browser,
            WebviewWindowed,
            WebviewWindowedNativeFocus,
            WebviewOpaqueWindowedBackground,
            PageMetadata {
                title,
                url: url.to_string(),
                icon: vmux_core::PageIcon::None,
                bg_color: None,
            },
            WebviewSource::new(url),
            ResolvedWebviewUri(url.to_string()),
            Mesh3d(meshes.add(bevy::math::primitives::Plane3d::new(
                Vec3::Z,
                Vec2::splat(0.5),
            ))),
        ),
        (
            MeshMaterial3d(webview_mt.add(WebviewExtendStandardMaterial::default())),
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Inherited,
            Pickable::default(),
        ),
    )
}

pub fn restore_file_view_bundle(
    url: &str,
    meshes: &mut ResMut<Assets<Mesh>>,
    webview_mt: &mut ResMut<Assets<WebviewExtendStandardMaterial>>,
) -> Option<impl Bundle> {
    let path = path_from_files_url(url)?;
    Some(new_file_view_bundle(url, path, meshes, webview_mt))
}

fn clear_stack_children(stack: Entity, children_q: &Query<&Children>, commands: &mut Commands) {
    if let Ok(children) = children_q.get(stack) {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }
}

pub fn handle_file_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
) {
    for (entity, task) in &tasks {
        if !task.url.starts_with("file:") {
            continue;
        }
        let Some(path) = path_from_files_url(&task.url) else {
            commands.entity(entity).insert(PageOpenError {
                message: format!("malformed file URL '{}'", task.url),
            });
            continue;
        };
        clear_stack_children(task.stack, &children_q, &mut commands);
        commands.spawn((
            new_file_view_bundle(&task.url, path, &mut meshes, &mut webview_mt),
            ChildOf(task.stack),
        ));
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn settings_keymap(settings: &Option<Res<vmux_setting::AppSettings>>) -> vmux_core::KeymapKind {
    settings
        .as_ref()
        .map(|s| s.editor.keymap)
        .unwrap_or_default()
}

fn load_file_buffers(
    q: Query<(Entity, &FileView), UnloadedFileView>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut commands: Commands,
) {
    for (entity, fv) in &q {
        if fv.path.is_dir() {
            let entries = list_dir(&fv.path);
            commands.entity(entity).insert(FileDir { entries });
            continue;
        }
        let path_str = fv.path.to_string_lossy();
        if let Some(kind) = vmux_core::media::media_kind(&path_str) {
            let mime = vmux_core::media::media_mime(&path_str)
                .unwrap_or("application/octet-stream")
                .to_string();
            commands.entity(entity).insert(FileMedia { kind, mime });
            continue;
        }
        match std::fs::metadata(&fv.path).map(|m| m.len()) {
            Ok(len) if len > crate::highlight::FILE_VIEW_MAX_BYTES => {
                commands.entity(entity).insert(FileBuffer::error(format!(
                    "__error__:file too large ({len} bytes, max {})",
                    crate::highlight::FILE_VIEW_MAX_BYTES
                )));
                continue;
            }
            Err(e) => {
                commands.entity(entity).insert(FileBuffer::error(format!(
                    "__error__:cannot open {}: {e}",
                    fv.path.display()
                )));
                continue;
            }
            _ => {}
        }
        let text = match std::fs::read(&fv.path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(t) => t,
                Err(_) => {
                    commands.entity(entity).insert(FileBuffer::error(format!(
                        "__error__:not a UTF-8 text file: {}",
                        fv.path.display()
                    )));
                    continue;
                }
            },
            Err(e) => {
                commands.entity(entity).insert(FileBuffer::error(format!(
                    "__error__:cannot read {}: {e}",
                    fv.path.display()
                )));
                continue;
            }
        };
        let hl = HighlightCache::new(&fv.path);
        let kind = settings_keymap(&settings);
        let core = EditCore::new(
            fv.path.clone(),
            hl.language.clone(),
            &text,
            kind.initial_mode(),
        );
        commands
            .entity(entity)
            .insert((EditState { core, hl }, EditorKeymap(kind.make())));
    }
}

fn display_path(path: &std::path::Path) -> String {
    if let Ok(cwd) = std::env::current_dir()
        && let Ok(rel) = path.strip_prefix(&cwd)
    {
        return rel.to_string_lossy().to_string();
    }
    if let Some(home) = std::env::home_dir()
        && let Ok(rel) = path.strip_prefix(&home)
    {
        return format!("~/{}", rel.to_string_lossy());
    }
    path.to_string_lossy().to_string()
}

fn send_initial_meta(
    q: Query<(Entity, &FileBuffer), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, buf) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        if let Some(message) = buf.language.strip_prefix("__error__:") {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_ERROR_EVENT,
                &FileErrorEvent {
                    message: message.to_string(),
                },
            ));
        }
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn send_initial_text_meta(
    mut q: Query<
        (
            Entity,
            &FileView,
            &mut EditState,
            &EditorKeymap,
            &FileViewport,
        ),
        ReadyUnsentMeta,
    >,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, mut edit, keymap, vp) in &mut q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_META_EVENT,
            &FileMetaEvent {
                path: display_path(&fv.path),
                abs_path: fv.path.to_string_lossy().into_owned(),
                language: edit.core.buffer.language.clone(),
                total_lines: edit.core.buffer.len_lines() as u32,
            },
        ));
        if vp.rows > 0 {
            emit_window(entity, &mut edit, vp, &browsers, &mut commands);
        }
        emit_cursor(
            entity,
            &edit.core,
            keymap.0.as_ref(),
            vp,
            &browsers,
            &mut commands,
        );
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn send_file_theme(
    q: Query<Entity, ReadyUnsentTheme>,
    settings: Res<vmux_setting::AppSettings>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for entity in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        let (font_family, font_size, line_height) = settings
            .terminal
            .as_ref()
            .map(|t| {
                let th = t.resolve_theme(&t.default_theme);
                (th.font_family.clone(), th.font_size, th.line_height)
            })
            .unwrap_or_else(|| (String::new(), 0.0, 0.0));
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_THEME_EVENT,
            &FileThemeEvent {
                font_family,
                font_size,
                line_height,
            },
        ));
        commands.entity(entity).insert(FileThemeSent);
    }
}

fn send_initial_dir(
    q: Query<(Entity, &FileView, &FileDir), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, dir) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        let (parent_path, parent_entries) = parent_listing(&fv.path);
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_DIR_EVENT,
            &FileDirEvent {
                path: display_path(&fv.path),
                abs_path: fv.path.to_string_lossy().into_owned(),
                entries: dir.entries.clone(),
                parent_path,
                parent_entries,
            },
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
    }
}

fn window_bounds(total: u32, vp: &FileViewport) -> (u32, u32) {
    let (vis_first, vis_end) = window_range(total, vp.top_line, vp.rows);
    (
        vis_first.saturating_sub(SCROLL_OVERSCAN),
        (vis_end + SCROLL_OVERSCAN).min(total),
    )
}

fn emit_window(
    entity: Entity,
    edit: &mut EditState,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = edit.core.buffer.len_lines() as u32;
    let (first, end) = window_bounds(total, vp);
    let lines = edit
        .hl
        .line_window(&edit.core.buffer.rope, first as usize, end as usize);
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_VIEWPORT_EVENT,
        &FileViewportPatch {
            first_line: first,
            total_lines: total,
            lines,
        },
    ));
}

fn emit_cursor(
    entity: Entity,
    core: &EditCore,
    keymap: &dyn Keymap,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = core.buffer.len_lines() as u32;
    let (first, end) = window_bounds(total, vp);
    let rows = (end - first).min(u16::MAX as u32) as u16;
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_CURSOR_EVENT,
        &FileCursorEvent {
            mode: keymap.mode(),
            mode_label: keymap.mode_label(),
            primary: core.cursor_pos(),
            selections: core.sel_spans(first, rows),
        },
    ));
}

fn reset_file_sent_markers_on_page_ready(
    trigger: On<BinReceive<vmux_core::page::PageReady>>,
    file_views: Query<(), With<FileView>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if file_views.get(entity).is_err() {
        return;
    }
    commands
        .entity(entity)
        .remove::<FileInitialMetaSent>()
        .remove::<FileThemeSent>()
        .remove::<crate::lsp::manager::LspStatusSent>()
        .remove::<crate::lsp::manager::DiagSent>();
}

fn on_file_resize(
    trigger: On<BinReceive<FileResizeEvent>>,
    mut q: Query<(
        &mut FileViewport,
        Option<&mut EditState>,
        Option<&EditorKeymap>,
    )>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((mut vp, edit, keymap)) = q.get_mut(entity) else {
        return;
    };
    vp.rows = rows_from_viewport(evt.char_height, evt.viewport_height);
    if let Some(mut edit) = edit {
        edit.core.rows = vp.rows;
        let vpc = *vp;
        emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
        if let Some(keymap) = keymap {
            emit_cursor(
                entity,
                &edit.core,
                keymap.0.as_ref(),
                &vpc,
                &browsers,
                &mut commands,
            );
        }
    }
}

fn on_file_scroll(
    trigger: On<BinReceive<FileScrollEvent>>,
    mut q: Query<(&mut EditState, &mut FileViewport, &EditorKeymap)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((mut edit, mut vp, keymap)) = q.get_mut(entity) else {
        return;
    };
    let total = edit.core.buffer.len_lines() as u32;
    vp.top_line = clamp_top_line(evt.top_line, total, vp.rows);
    let vpc = *vp;
    emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
    emit_cursor(
        entity,
        &edit.core,
        keymap.0.as_ref(),
        &vpc,
        &browsers,
        &mut commands,
    );
}

fn sync_media_allowlist(media: Query<&FileView, With<FileMedia>>) {
    let paths: std::collections::HashSet<std::path::PathBuf> =
        media.iter().map(|fv| fv.path.clone()).collect();
    set_media_allowlist(paths);
}

fn raw_media_url(path: &std::path::Path) -> String {
    let mut url = url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.to_string_lossy()));
    url.push_str("?vmux-raw=1");
    url
}

fn send_initial_media(
    q: Query<(Entity, &FileView, &FileMedia), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, media) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_MEDIA_EVENT,
            &FileMediaEvent {
                kind: media.kind,
                mime: media.mime.clone(),
                url: raw_media_url(&fv.path),
                abs_path: fv.path.to_string_lossy().into_owned(),
            },
        ));
        commands.entity(entity).insert(FileInitialMetaSent);
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
    let req = trigger.event().payload.clone();
    let path = PathBuf::from(&req.path);
    if req.thumb && preview::is_image_path(&path) {
        let within_cap = std::fs::metadata(&path)
            .map(|m| m.len() <= preview::IMAGE_BYTES_CAP)
            .unwrap_or(false);
        if !within_cap {
            return;
        }
        let pool = IoTaskPool::get();
        let p = req.path.clone();
        let task = pool.spawn(async move {
            let r = std::fs::read(&p)
                .map_err(|e| e.to_string())
                .and_then(|b| preview::downscale_to_png(&b, preview::THUMB_MAX_EDGE));
            (p, r)
        });
        commands.spawn(ThumbTask {
            webview: entity,
            task,
        });
        return;
    }
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let kind = preview::build_preview_sync(&path);
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_PREVIEW_EVENT,
        &FilePreviewEvent {
            path: req.path,
            thumb: false,
            kind,
        },
    ));
}

fn drain_thumb_tasks(
    mut q: Query<(Entity, &mut ThumbTask)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut t) in &mut q {
        if let Some((path, result)) = future::block_on(future::poll_once(&mut t.task)) {
            let webview = t.webview;
            commands.entity(task_entity).despawn();
            if let Ok(bytes) = result
                && browsers.has_browser(webview)
                && browsers.host_emit_ready(&webview)
            {
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    webview,
                    FILE_PREVIEW_EVENT,
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
}

fn on_file_open_external(
    trigger: On<BinReceive<FileOpenExternalRequest>>,
    views: Query<&FileView, With<FileMedia>>,
) {
    let entity = trigger.event().webview;
    let Ok(fv) = views.get(entity) else {
        return;
    };
    let req_path = PathBuf::from(&trigger.event().payload.path);
    if fv.path != req_path {
        return;
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    let _ = std::process::Command::new(program).arg(&req_path).spawn();
}

fn on_file_open(
    trigger: On<BinReceive<FileOpenEvent>>,
    mut views: Query<(&mut FileView, &mut FileViewport, &mut PageMetadata)>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok((mut fv, mut vp, mut meta)) = views.get_mut(entity) else {
        return;
    };
    manager.close(&fv.path);
    let url = url::Url::from_file_path(&path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.to_string_lossy()));
    meta.title = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    meta.url = url;
    fv.path = path;
    vp.top_line = 0;
    commands
        .entity(entity)
        .remove::<FileDir>()
        .remove::<FileBuffer>()
        .remove::<FileMedia>()
        .remove::<EditState>()
        .remove::<EditorKeymap>()
        .remove::<LspEditDirty>()
        .remove::<FileInitialMetaSent>()
        .remove::<crate::lsp::manager::LspOpened>()
        .remove::<crate::lsp::manager::LintRan>();
}

#[derive(Component)]
struct FileReloadRequested;

struct FileWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    dirs: HashSet<PathBuf>,
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn watch_dir_for(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        Some(path.to_path_buf())
    } else {
        path.parent().map(Path::to_path_buf)
    }
}

fn reconcile_file_watches(q: Query<&FileView>, watch: Option<NonSendMut<FileWatch>>) {
    let Some(mut watch) = watch else {
        return;
    };
    for fv in &q {
        let Some(dir) = watch_dir_for(&fv.path) else {
            continue;
        };
        if watch.dirs.insert(dir.clone()) {
            let _ = watch.watcher.watch(&dir, RecursiveMode::NonRecursive);
        }
    }
}

fn drain_file_changes(
    watch: Option<NonSend<FileWatch>>,
    self_writes: Option<NonSendMut<SelfWrites>>,
    q: Query<(Entity, &FileView)>,
    mut commands: Commands,
) {
    let Some(watch) = watch else {
        return;
    };
    let mut changed: HashSet<PathBuf> = HashSet::new();
    while let Ok(res) = watch.rx.try_recv() {
        if let Ok(event) = res {
            for p in event.paths {
                changed.insert(canon(&p));
            }
        }
    }
    if changed.is_empty() {
        return;
    }
    let mut sw = self_writes;
    if let Some(sw) = sw.as_mut() {
        sw.0.retain(|_, t| t.elapsed() < std::time::Duration::from_secs(2));
    }
    for (entity, fv) in &q {
        let cp = canon(&fv.path);
        if !changed.contains(&cp) {
            continue;
        }
        if let Some(sw) = sw.as_ref()
            && sw.0.contains_key(&cp)
        {
            continue;
        }
        commands.entity(entity).insert(FileReloadRequested);
    }
}

fn reload_changed_files(
    q: Query<(Entity, &FileView, Option<&EditState>), With<FileReloadRequested>>,
    browsers: NonSend<Browsers>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    for (entity, fv, edit) in &q {
        commands.entity(entity).remove::<FileReloadRequested>();
        let ready = browsers.has_browser(entity) && browsers.host_emit_ready(&entity);

        if fv.path.is_dir() {
            let entries = list_dir(&fv.path);
            commands.entity(entity).insert(FileDir {
                entries: entries.clone(),
            });
            if ready {
                let (parent_path, parent_entries) = parent_listing(&fv.path);
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    entity,
                    FILE_DIR_EVENT,
                    &FileDirEvent {
                        path: display_path(&fv.path),
                        abs_path: fv.path.to_string_lossy().into_owned(),
                        entries,
                        parent_path,
                        parent_entries,
                    },
                ));
            }
            continue;
        }

        if let Some(kind) = vmux_core::media::media_kind(&fv.path.to_string_lossy()) {
            if ready {
                let mime = vmux_core::media::media_mime(&fv.path.to_string_lossy())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let url = format!("{}&v={nonce}", raw_media_url(&fv.path));
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    entity,
                    FILE_MEDIA_EVENT,
                    &FileMediaEvent {
                        kind,
                        mime,
                        url,
                        abs_path: fv.path.to_string_lossy().into_owned(),
                    },
                ));
            }
            continue;
        }

        if let Some(edit) = edit
            && edit.core.dirty
        {
            if ready {
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    entity,
                    FILE_EXTERNAL_CHANGE_EVENT,
                    &FileExternalChange {
                        path: display_path(&fv.path),
                    },
                ));
            }
            continue;
        }
        commands
            .entity(entity)
            .remove::<EditState>()
            .remove::<FileBuffer>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LintRan>();
        manager.change(&fv.path);
    }
}

fn caret_lsp(edit: &EditState) -> (u32, u32, usize, String) {
    let head = edit.core.primary().head;
    let (line, ccol) = edit.core.buffer.char_to_coords(head);
    let lt: String = edit
        .core
        .buffer
        .rope
        .line(line)
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let utf16 = crate::lsp::manager::char_to_utf16_col(&lt, ccol as u32);
    (line as u32, utf16, ccol, lt)
}

fn word_start_col(line_text: &str, char_col: usize) -> u32 {
    let chars: Vec<char> = line_text.chars().collect();
    let mut i = char_col.min(chars.len());
    while i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        i -= 1;
    }
    i as u32
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = match dir {
        Some(d) => d.join(format!(".{fname}.vmux-tmp")),
        None => PathBuf::from(format!(".{fname}.vmux-tmp")),
    };
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[allow(clippy::too_many_arguments)]
fn run_commands(
    entity: Entity,
    cmds: Vec<EditCommand>,
    edit: &mut EditState,
    keymap: &dyn Keymap,
    vp: &mut FileViewport,
    clipboard: &mut ClipboardHandle,
    self_writes: &mut SelfWrites,
    manager: &mut crate::lsp::manager::LspManager,
    browsers: &Browsers,
    commands: &mut Commands,
) -> bool {
    let mut text_changed = false;
    let mut sel_or_mode = false;
    let mut dirty_changed = false;
    for cmd in cmds {
        match &cmd {
            EditCommand::Hover => {
                let head = edit.core.primary().head;
                let (line, ccol) = edit.core.buffer.char_to_coords(head);
                let lt: String = edit
                    .core
                    .buffer
                    .rope
                    .line(line)
                    .chars()
                    .filter(|c| *c != '\n' && *c != '\r')
                    .collect();
                let utf16 = crate::lsp::manager::char_to_utf16_col(&lt, ccol as u32);
                manager.hover(
                    entity,
                    &edit.core.buffer.path,
                    line as u32,
                    utf16,
                    ccol as u32,
                );
                continue;
            }
            EditCommand::GotoDefinition => {
                let (line, utf16, _, _) = caret_lsp(edit);
                let path = edit.core.buffer.path.clone();
                manager.definition(entity, &path, line, utf16);
                continue;
            }
            EditCommand::FindReferences => {
                let (line, utf16, _, _) = caret_lsp(edit);
                let path = edit.core.buffer.path.clone();
                manager.references(entity, &path, line, utf16);
                continue;
            }
            EditCommand::TriggerCompletion => {
                let (line, utf16, ccol, lt) = caret_lsp(edit);
                let replace_from = word_start_col(&lt, ccol);
                let path = edit.core.buffer.path.clone();
                manager.completion(entity, &path, line, utf16, replace_from);
                continue;
            }
            _ => {}
        }
        if matches!(cmd, EditCommand::Save) {
            let path = edit.core.buffer.path.clone();
            let body = edit.core.buffer.text();
            match write_atomic(&path, body.as_bytes()) {
                Ok(()) => {
                    self_writes
                        .0
                        .insert(canon(&path), std::time::Instant::now());
                    let was_dirty = edit.core.dirty;
                    edit.core.mark_saved();
                    if was_dirty {
                        dirty_changed = true;
                    }
                    commands
                        .entity(entity)
                        .insert(LspEditDirty)
                        .remove::<crate::lsp::manager::LintRan>();
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), "editor save failed: {e}");
                    if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            FILE_ERROR_EVENT,
                            &FileErrorEvent {
                                message: format!("save failed: {e}"),
                            },
                        ));
                    }
                }
            }
            continue;
        }
        if matches!(cmd, EditCommand::Paste | EditCommand::PasteBefore)
            && let Some(cb) = clipboard.0.as_mut()
            && let Ok(s) = cb.get_text()
        {
            edit.core.register = Some((s, false));
        }
        let out = edit.core.apply(cmd);
        if out.text_changed {
            text_changed = true;
            let (l, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
            edit.hl.invalidate_from(l.saturating_sub(1));
        }
        sel_or_mode |= out.sel_changed || out.mode_changed;
        dirty_changed |= out.dirty_changed;
        if let Some((s, _)) = out.yank
            && let Some(cb) = clipboard.0.as_mut()
        {
            let _ = cb.set_text(s);
        }
    }
    if let Some(top) = edit.core.autoscroll(vp.top_line, vp.rows) {
        vp.top_line = top;
        text_changed = true;
    }
    let vpc = *vp;
    if text_changed {
        emit_window(entity, edit, &vpc, browsers, commands);
    }
    if text_changed || sel_or_mode {
        emit_cursor(entity, &edit.core, keymap, &vpc, browsers, commands);
    }
    if dirty_changed {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_DIRTY_EVENT,
            &FileDirtyEvent {
                dirty: edit.core.dirty,
            },
        ));
    }
    if text_changed {
        commands
            .entity(entity)
            .insert(LspEditDirty)
            .remove::<crate::lsp::manager::LintRan>();
    }
    text_changed
}

fn on_file_key(
    trigger: On<BinReceive<FileKeyEvent>>,
    mut q: Query<(&mut EditState, &mut EditorKeymap, &mut FileViewport)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((mut edit, mut keymap, mut vp)) = q.get_mut(entity) else {
        return;
    };
    let input = KeyInput {
        key: evt.key.clone(),
        mods: Mods {
            ctrl: evt.mods.ctrl,
            alt: evt.mods.alt,
            shift: evt.mods.shift,
            meta: evt.mods.meta,
        },
        repeat: evt.repeat,
    };
    let cmds = keymap.0.handle(&input);
    if cmds.is_empty() {
        return;
    }
    run_commands(
        entity,
        cmds,
        &mut edit,
        keymap.0.as_ref(),
        &mut vp,
        &mut clipboard,
        &mut self_writes,
        &mut manager,
        &browsers,
        &mut commands,
    );
}

fn on_file_text_input(
    trigger: On<BinReceive<FileTextInput>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &mut FileViewport)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    if text.is_empty() {
        return;
    }
    let Ok((mut edit, keymap, mut vp)) = q.get_mut(entity) else {
        return;
    };
    if !keymap.0.mode().accepts_text() {
        return;
    }
    run_commands(
        entity,
        vec![EditCommand::InsertText(text)],
        &mut edit,
        keymap.0.as_ref(),
        &mut vp,
        &mut clipboard,
        &mut self_writes,
        &mut manager,
        &browsers,
        &mut commands,
    );
}

fn on_file_hover_request(
    trigger: On<BinReceive<FileHoverRequest>>,
    q: Query<&EditState>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    let line = req
        .line
        .min(edit.core.buffer.len_lines().saturating_sub(1) as u32);
    let lt: String = edit
        .core
        .buffer
        .rope
        .line(line as usize)
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let utf16 = crate::lsp::manager::char_to_utf16_col(&lt, req.col);
    manager.hover(entity, &edit.core.buffer.path, line, utf16, req.col);
}

#[derive(Component)]
struct PendingGoto {
    line: u32,
    utf16_col: u32,
}

fn req_pos(edit: &EditState, line: u32, col: u32) -> (u32, u32, String) {
    let line = line.min(edit.core.buffer.len_lines().saturating_sub(1) as u32);
    let lt: String = edit
        .core
        .buffer
        .rope
        .line(line as usize)
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let utf16 = crate::lsp::manager::char_to_utf16_col(&lt, col);
    (line, utf16, lt)
}

fn on_file_definition_request(
    trigger: On<BinReceive<FileDefinitionRequest>>,
    q: Query<&EditState>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    let (line, utf16, _) = req_pos(edit, req.line, req.col);
    let path = edit.core.buffer.path.clone();
    manager.definition(entity, &path, line, utf16);
}

fn on_file_references_request(
    trigger: On<BinReceive<FileReferencesRequest>>,
    q: Query<&EditState>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    let (line, utf16, _) = req_pos(edit, req.line, req.col);
    let path = edit.core.buffer.path.clone();
    manager.references(entity, &path, line, utf16);
}

fn on_file_completion_request(
    trigger: On<BinReceive<FileCompletionRequest>>,
    q: Query<&EditState>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    let (line, utf16, lt) = req_pos(edit, req.line, req.col);
    let replace_from = word_start_col(&lt, req.col as usize);
    let path = edit.core.buffer.path.clone();
    manager.completion(entity, &path, line, utf16, replace_from);
}

fn on_file_goto_request(
    trigger: On<BinReceive<FileGotoRequest>>,
    mut goto_w: MessageWriter<crate::lsp::manager::LspGoto>,
) {
    let entity = trigger.event().webview;
    let req = &trigger.event().payload;
    let path = PathBuf::from(&req.path);
    let lt = crate::lsp::manager::disk_line(&path, req.line);
    let utf16 = crate::lsp::manager::char_to_utf16_col(&lt, req.col);
    goto_w.write(crate::lsp::manager::LspGoto {
        entity,
        path,
        line: req.line,
        utf16_col: utf16,
    });
}

fn on_file_completion_commit(
    trigger: On<BinReceive<FileCompletionCommit>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &mut FileViewport)>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload.clone();
    let Ok((mut edit, keymap, mut vp)) = q.get_mut(entity) else {
        return;
    };
    let start = edit
        .core
        .buffer
        .coords_to_char(req.line as usize, req.replace_from_col as usize);
    let head = edit.core.primary().head;
    let (a, b) = (start.min(head), start.max(head));
    edit.core.selections = vec![Selection { anchor: a, head: b }];
    run_commands(
        entity,
        vec![
            EditCommand::DeleteSelection,
            EditCommand::InsertText(req.text),
        ],
        &mut edit,
        keymap.0.as_ref(),
        &mut vp,
        &mut clipboard,
        &mut self_writes,
        &mut manager,
        &browsers,
        &mut commands,
    );
}

fn goto_caret(edit: &mut EditState, line: u32, utf16_col: u32, vp: &mut FileViewport) {
    let line = (line as usize).min(edit.core.buffer.len_lines().saturating_sub(1));
    let lt: String = edit
        .core
        .buffer
        .rope
        .line(line)
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let ccol = crate::lsp::manager::utf16_to_char_col(&lt, utf16_col);
    let at = edit.core.buffer.coords_to_char(line, ccol as usize);
    edit.core.set_caret(at);
    if let Some(top) = edit.core.autoscroll(vp.top_line, vp.rows) {
        vp.top_line = top;
    }
}

#[allow(clippy::type_complexity)]
fn apply_goto(
    mut msgs: MessageReader<crate::lsp::manager::LspGoto>,
    mut q: Query<(
        &mut EditState,
        &mut FileViewport,
        &mut FileView,
        &mut PageMetadata,
        &EditorKeymap,
    )>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for g in msgs.read() {
        let Ok((mut edit, mut vp, mut fv, mut meta, keymap)) = q.get_mut(g.entity) else {
            continue;
        };
        if canon(&fv.path) == canon(&g.path) {
            goto_caret(&mut edit, g.line, g.utf16_col, &mut vp);
            let vpc = *vp;
            emit_window(g.entity, &mut edit, &vpc, &browsers, &mut commands);
            emit_cursor(
                g.entity,
                &edit.core,
                keymap.0.as_ref(),
                &vpc,
                &browsers,
                &mut commands,
            );
        } else {
            manager.close(&fv.path);
            let url = url::Url::from_file_path(&g.path)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| format!("file://{}", g.path.to_string_lossy()));
            meta.title = g
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            meta.url = url;
            fv.path = g.path.clone();
            vp.top_line = 0;
            commands
                .entity(g.entity)
                .remove::<EditState>()
                .remove::<FileBuffer>()
                .remove::<FileMedia>()
                .remove::<FileDir>()
                .remove::<FileInitialMetaSent>()
                .remove::<crate::lsp::manager::LspOpened>()
                .remove::<crate::lsp::manager::LintRan>()
                .insert(PendingGoto {
                    line: g.line,
                    utf16_col: g.utf16_col,
                });
        }
    }
}

fn apply_pending_goto(
    mut q: Query<(
        Entity,
        &mut EditState,
        &mut FileViewport,
        &EditorKeymap,
        &PendingGoto,
    )>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut edit, mut vp, keymap, pg) in &mut q {
        goto_caret(&mut edit, pg.line, pg.utf16_col, &mut vp);
        let vpc = *vp;
        emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
        emit_cursor(
            entity,
            &edit.core,
            keymap.0.as_ref(),
            &vpc,
            &browsers,
            &mut commands,
        );
        commands.entity(entity).remove::<PendingGoto>();
    }
}

fn on_file_pointer(
    trigger: On<BinReceive<FilePointerEvent>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let p = trigger.event().payload;
    let Ok((mut edit, keymap, vp)) = q.get_mut(entity) else {
        return;
    };
    let at = edit
        .core
        .buffer
        .coords_to_char(p.line as usize, p.col as usize);
    if p.extend {
        let anchor = edit.core.primary().anchor;
        edit.core.selections = vec![Selection { anchor, head: at }];
    } else {
        edit.core.set_caret(at);
    }
    emit_cursor(
        entity,
        &edit.core,
        keymap.0.as_ref(),
        vp,
        &browsers,
        &mut commands,
    );
}

fn flush_lsp_changes(
    time: Res<Time>,
    mut acc: Local<f32>,
    q: Query<(Entity, &FileView, &EditState), With<LspEditDirty>>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    if q.is_empty() {
        return;
    }
    *acc += time.delta_secs();
    if *acc < 0.15 {
        return;
    }
    *acc = 0.0;
    for (entity, fv, edit) in &q {
        manager.change_with_text(&fv.path, &edit.core.buffer.text());
        commands.entity(entity).remove::<LspEditDirty>();
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "files",
    title: "Files",
    keywords: &["file", "open"],
    icon: Some(vmux_core::BuiltinIcon::Files),
    command_bar: false,
};

/// Wires the file editor: buffer loading, filesystem watching, image and theme sends, LSP
/// change flushing, and the file webview event bridge (adds [`LspPlugin`]).
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        let (tx, rx) = mpsc::channel();
        match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(watcher) => {
                app.insert_non_send(FileWatch {
                    watcher,
                    rx,
                    dirs: HashSet::new(),
                });
            }
            Err(e) => tracing::warn!("file watcher init failed: {e}"),
        }
        app.insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()))
            .insert_non_send(SelfWrites::default())
            .add_plugins(crate::lsp::LspPlugin)
            .add_plugins(BinEventEmitterPlugin::<(
                FileResizeEvent,
                FileScrollEvent,
                FilePreviewRequest,
                FileOpenEvent,
                FileTextInput,
                FileKeyEvent,
                FilePointerEvent,
                FileHoverRequest,
                FileDefinitionRequest,
                FileReferencesRequest,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                FileCompletionRequest,
                FileGotoRequest,
                FileCompletionCommit,
                FileOpenExternalRequest,
            )>::default())
            .add_systems(
                Update,
                handle_file_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                (
                    load_file_buffers,
                    send_initial_meta,
                    send_initial_text_meta,
                    send_initial_dir,
                    send_initial_media,
                    sync_media_allowlist,
                    send_file_theme,
                    drain_thumb_tasks,
                    reconcile_file_watches,
                    flush_lsp_changes,
                    apply_goto,
                    apply_pending_goto,
                    (drain_file_changes, reload_changed_files).chain(),
                ),
            )
            .add_observer(reset_file_sent_markers_on_page_ready)
            .add_observer(on_file_resize)
            .add_observer(on_file_scroll)
            .add_observer(on_file_preview_request)
            .add_observer(on_file_open)
            .add_observer(on_file_open_external)
            .add_observer(on_file_key)
            .add_observer(on_file_text_input)
            .add_observer(on_file_pointer)
            .add_observer(on_file_hover_request)
            .add_observer(on_file_definition_request)
            .add_observer(on_file_references_request)
            .add_observer(on_file_completion_request)
            .add_observer(on_file_goto_request)
            .add_observer(on_file_completion_commit);
    }
}

#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, KeymapKindExt, Mods};

    #[test]
    fn vim_dd_deletes_line_via_keymap_and_core() {
        let mut km = vmux_core::KeymapKind::Vim.make();
        let mut core = EditCore::new(
            std::path::PathBuf::from("a.txt"),
            "Plain Text".into(),
            "one\ntwo\nthree\n",
            crate::edit::EditMode::Normal,
        );
        for key in ["d", "d"] {
            for cmd in km.handle(&KeyInput {
                key: key.into(),
                mods: Mods::default(),
                repeat: false,
            }) {
                core.apply(cmd);
            }
        }
        assert_eq!(core.buffer.text(), "two\nthree\n");
    }

    #[test]
    fn vscode_typing_inserts_and_marks_dirty() {
        let mut core = EditCore::new(
            std::path::PathBuf::from("a.txt"),
            "Plain Text".into(),
            "",
            crate::edit::EditMode::Insert,
        );
        core.apply(EditCommand::InsertText("hello".into()));
        assert_eq!(core.buffer.text(), "hello");
        assert!(core.dirty);
    }
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn parses_simple_path() {
        assert_eq!(
            path_from_files_url("file:///Users/me/src/main.rs"),
            Some(PathBuf::from("/Users/me/src/main.rs"))
        );
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(
            path_from_files_url("file:///Users/me/a%20b.rs"),
            Some(PathBuf::from("/Users/me/a b.rs"))
        );
    }

    #[test]
    fn rejects_non_files_scheme() {
        assert_eq!(path_from_files_url("vmux://terminal/"), None);
    }

    #[test]
    fn empty_path_is_root() {
        assert_eq!(path_from_files_url("file:///"), Some(PathBuf::from("/")));
    }
}

#[cfg(test)]
mod page_open_tests {
    use super::*;
    use vmux_core::PageOpenId;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_file_page_open);
        app
    }

    #[test]
    fn claims_files_url_and_attaches_fileview() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "file:///etc/hostname".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let mut q = app.world_mut().query::<(&ChildOf, &FileView)>();
        let found: Vec<_> = q
            .iter(app.world())
            .filter(|(c, _)| c.0 == stack)
            .map(|(_, fv)| fv.path.clone())
            .collect();
        assert_eq!(found, vec![PathBuf::from("/etc/hostname")]);
    }

    #[test]
    fn ignores_non_files_url() {
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://terminal/".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_none());
    }

    #[test]
    fn navigate_relists_when_path_changes() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        fs::create_dir(&a).unwrap();
        fs::write(a.join("f1"), "").unwrap();
        let b = tmp.path().join("b");
        fs::create_dir(&b).unwrap();
        fs::write(b.join("f2"), "").unwrap();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, load_file_buffers);
        let e = app
            .world_mut()
            .spawn((
                FileView { path: a.clone() },
                FileViewport {
                    top_line: 0,
                    rows: 0,
                },
            ))
            .id();
        app.update();
        assert!(
            app.world()
                .get::<FileDir>(e)
                .unwrap()
                .entries
                .iter()
                .any(|x| x.name == "f1")
        );

        app.world_mut().get_mut::<FileView>(e).unwrap().path = b.clone();
        app.world_mut().entity_mut(e).remove::<FileDir>();
        app.update();
        let dir = app.world().get::<FileDir>(e).unwrap();
        assert!(dir.entries.iter().any(|x| x.name == "f2"));
        assert!(!dir.entries.iter().any(|x| x.name == "f1"));
    }
}
