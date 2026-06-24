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
use crate::highlight::Highlighter;
use crate::preview;
use crate::viewport::{clamp_top_line, rows_from_viewport, visible_slice};

#[derive(Component, Clone, Debug)]
pub struct FileView {
    pub path: PathBuf,
}

#[derive(Component, Clone, Debug)]
pub struct FileBuffer {
    pub language: String,
    pub lines: Vec<FileLine>,
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
pub struct FileImage {
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Component)]
struct ThumbTask {
    webview: Entity,
    task: Task<(String, Result<Vec<u8>, String>)>,
}

#[derive(Component)]
pub struct FileInitialMetaSent;

#[derive(Component)]
pub struct FileThemeSent;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type UnloadedFileView = (Without<FileBuffer>, Without<FileDir>, Without<FileImage>);
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
                favicon_url: String::new(),
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

fn load_file_buffers(q: Query<(Entity, &FileView), UnloadedFileView>, mut commands: Commands) {
    for (entity, fv) in &q {
        if fv.path.is_dir() {
            let entries = list_dir(&fv.path);
            commands.entity(entity).insert(FileDir { entries });
            continue;
        }
        if preview::is_image_path(&fv.path) {
            match std::fs::metadata(&fv.path).map(|m| m.len()) {
                Ok(len) if len <= preview::IMAGE_BYTES_CAP => match std::fs::read(&fv.path) {
                    Ok(bytes) => {
                        let mime = preview::image_mime(&fv.path).unwrap_or("image/png");
                        commands.entity(entity).insert(FileImage {
                            mime: mime.to_string(),
                            bytes,
                        });
                    }
                    Err(e) => {
                        commands.entity(entity).insert(FileBuffer {
                            language: format!("__error__:{e}"),
                            lines: Vec::new(),
                        });
                    }
                },
                _ => {
                    commands.entity(entity).insert(FileBuffer {
                        language: "__error__:image too large to preview".into(),
                        lines: Vec::new(),
                    });
                }
            }
            continue;
        }
        let hl = Highlighter::new();
        match hl.load_file(&fv.path) {
            Ok(out) => {
                commands.entity(entity).insert(FileBuffer {
                    language: out.language,
                    lines: out.lines,
                });
            }
            Err(message) => {
                commands.entity(entity).insert(FileBuffer {
                    language: format!("__error__:{message}"),
                    lines: Vec::new(),
                });
            }
        }
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
    q: Query<(Entity, &FileView, &FileBuffer, &FileViewport), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, buf, vp) in &q {
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
        } else {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_META_EVENT,
                &FileMetaEvent {
                    path: display_path(&fv.path),
                    abs_path: fv.path.to_string_lossy().into_owned(),
                    language: buf.language.clone(),
                    total_lines: buf.lines.len() as u32,
                },
            ));
            if vp.rows > 0 {
                emit_window(entity, buf, vp, &browsers, &mut commands);
            }
        }
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

fn emit_window(
    entity: Entity,
    buf: &FileBuffer,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = buf.lines.len() as u32;
    let slice = visible_slice(total, vp.top_line, vp.rows);
    let first = slice.start as u32;
    let lines = buf.lines[slice].to_vec();
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
        .remove::<FileThemeSent>();
}

fn on_file_resize(
    trigger: On<BinReceive<FileResizeEvent>>,
    mut q: Query<(&mut FileViewport, Option<&FileBuffer>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((mut vp, buf)) = q.get_mut(entity) else {
        return;
    };
    vp.rows = rows_from_viewport(evt.char_height, evt.viewport_height);
    if let Some(buf) = buf {
        emit_window(entity, buf, &vp, &browsers, &mut commands);
    }
}

fn on_file_scroll(
    trigger: On<BinReceive<FileScrollEvent>>,
    mut q: Query<(&FileBuffer, &mut FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((buf, mut vp)) = q.get_mut(entity) else {
        return;
    };
    vp.top_line = clamp_top_line(evt.top_line, buf.lines.len() as u32, vp.rows);
    emit_window(entity, buf, &vp, &browsers, &mut commands);
}

fn send_initial_image(
    q: Query<(Entity, &FileImage), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, img) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_IMAGE_EVENT,
            &FileImageEvent {
                mime: img.mime.clone(),
                bytes: img.bytes.clone(),
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
        .remove::<FileImage>()
        .remove::<FileInitialMetaSent>()
        .remove::<crate::lsp::manager::LspOpened>();
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
    for (entity, fv) in &q {
        if changed.contains(&canon(&fv.path)) {
            commands.entity(entity).insert(FileReloadRequested);
        }
    }
}

fn reload_changed_files(
    mut q: Query<(Entity, &FileView, &mut FileViewport), With<FileReloadRequested>>,
    browsers: NonSend<Browsers>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    for (entity, fv, mut vp) in &mut q {
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

        if preview::is_image_path(&fv.path) {
            let within = std::fs::metadata(&fv.path)
                .map(|m| m.len() <= preview::IMAGE_BYTES_CAP)
                .unwrap_or(false);
            if within && let Ok(bytes) = std::fs::read(&fv.path) {
                let mime = preview::image_mime(&fv.path).unwrap_or("image/png");
                commands.entity(entity).insert(FileImage {
                    mime: mime.to_string(),
                    bytes: bytes.clone(),
                });
                if ready {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        entity,
                        FILE_IMAGE_EVENT,
                        &FileImageEvent {
                            mime: mime.to_string(),
                            bytes,
                        },
                    ));
                }
            }
            continue;
        }

        let hl = Highlighter::new();
        let buf = match hl.load_file(&fv.path) {
            Ok(out) => FileBuffer {
                language: out.language,
                lines: out.lines,
            },
            Err(message) => {
                if ready {
                    commands.trigger(BinHostEmitEvent::from_rkyv(
                        entity,
                        FILE_ERROR_EVENT,
                        &FileErrorEvent { message },
                    ));
                }
                continue;
            }
        };
        let total = buf.lines.len() as u32;
        vp.top_line = clamp_top_line(vp.top_line, total, vp.rows);
        if ready {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_META_EVENT,
                &FileMetaEvent {
                    path: display_path(&fv.path),
                    abs_path: fv.path.to_string_lossy().into_owned(),
                    language: buf.language.clone(),
                    total_lines: total,
                },
            ));
            let vpc = *vp;
            emit_window(entity, &buf, &vpc, &browsers, &mut commands);
        }
        commands.entity(entity).insert(buf);
        manager.change(&fv.path);
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "files",
    title: "Files",
    keywords: &["file", "open"],
    icon: "file",
    command_bar: false,
};

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
        app.add_plugins(crate::lsp::LspPlugin)
            .add_plugins(BinEventEmitterPlugin::<(
                FileResizeEvent,
                FileScrollEvent,
                FilePreviewRequest,
                FileOpenEvent,
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
                    send_initial_dir,
                    send_initial_image,
                    send_file_theme,
                    drain_thumb_tasks,
                    reconcile_file_watches,
                    (drain_file_changes, reload_changed_files).chain(),
                ),
            )
            .add_observer(reset_file_sent_markers_on_page_ready)
            .add_observer(on_file_resize)
            .add_observer(on_file_scroll)
            .add_observer(on_file_preview_request)
            .add_observer(on_file_open);
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
