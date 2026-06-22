use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::PageMetadata;
use vmux_core::event::*;
use vmux_core::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_layout::Browser;
use vmux_layout::event::TERMINAL_CEF_BG_COLOR;

use crate::highlight::Highlighter;
use crate::viewport::{clamp_top_line, rows_from_viewport, visible_slice};

/// Webview entity marker carrying the file being viewed.
#[derive(Component, Clone, Debug)]
pub struct FileView {
    pub path: PathBuf,
}

/// Whole-file highlighted content, cached on the `FileView` entity.
#[derive(Component, Clone, Debug)]
pub struct FileBuffer {
    pub language: String,
    pub lines: Vec<FileLine>,
}

/// Current scroll offset + viewport height (in rows) for a `FileView`.
#[derive(Component, Clone, Copy, Debug)]
pub struct FileViewport {
    pub top_line: u32,
    pub rows: u16,
}

#[derive(Component)]
pub struct FileInitialMetaSent;

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

/// Parse the absolute filesystem path out of a `file://` URL.
/// `file:///Users/me/a%20b.rs` -> `/Users/me/a b.rs`.
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
    Some(PathBuf::from(decoded.as_ref()))
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
            PageMetadata {
                title,
                url: url.to_string(),
                favicon_url: String::new(),
                bg_color: Some(TERMINAL_CEF_BG_COLOR.to_string()),
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

/// Build a `FileView` webview bundle for session restore. The path is parsed from the
/// saved `file://` URL; returns `None` if it isn't a valid file URL.
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

fn load_file_buffers(q: Query<(Entity, &FileView), Without<FileBuffer>>, mut commands: Commands) {
    for (entity, fv) in &q {
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

fn send_initial_meta(
    q: Query<(Entity, &FileView, &FileBuffer), Without<FileInitialMetaSent>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, buf) in &q {
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
                    path: fv.path.to_string_lossy().to_string(),
                    language: buf.language.clone(),
                    total_lines: buf.lines.len() as u32,
                },
            ));
        }
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

fn on_file_resize(
    trigger: On<BinReceive<FileResizeEvent>>,
    mut q: Query<(&FileBuffer, &mut FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((buf, mut vp)) = q.get_mut(entity) else {
        return;
    };
    vp.rows = rows_from_viewport(evt.char_height, evt.viewport_height);
    emit_window(entity, buf, &vp, &browsers, &mut commands);
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
        app.add_plugins(BinEventEmitterPlugin::<(FileResizeEvent, FileScrollEvent)>::default())
            .add_systems(
                Update,
                handle_file_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(Update, (load_file_buffers, send_initial_meta))
            .add_observer(on_file_resize)
            .add_observer(on_file_scroll);
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
}
