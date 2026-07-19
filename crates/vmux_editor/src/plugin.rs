use std::collections::{HashMap, HashSet};
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

use crate::dir::{list_dir, parent_listing, project_root};
use crate::edit::highlight_cache::HighlightCache;
use crate::edit::{EditCommand, EditCore, Selection};
use crate::explorer_model::flatten_tree;
use crate::keymap::{KeyInput, Keymap, KeymapKindExt, Mods};
use crate::preview;
use crate::viewport::{clamp_top_line, rows_from_viewport, window_range};

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
    pub top_row: u32,
    pub rows: u16,
}

#[derive(Component, Clone, Debug)]
pub struct FileDir {
    pub entries: Vec<FileDirEntry>,
}

/// A media file (image/video/audio/pdf) opened in a `file://` view. Holds only
/// the kind and MIME; the bytes are served on demand over the CEF resource pipe.
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
    pub folds: crate::fold::FoldState,
}

#[derive(Component)]
struct FoldsDirty;

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

#[derive(Component, Default)]
pub(crate) struct ExplorerState {
    pub root: PathBuf,
    pub expanded: HashSet<PathBuf>,
    pub loading: HashSet<PathBuf>,
    pub children: HashMap<PathBuf, Vec<FileDirEntry>>,
    pub open_editors: Vec<PathBuf>,
    pub focus_path: Option<PathBuf>,
}

#[derive(Component)]
struct ExplorerDirLoadTask {
    webview: Entity,
    task: Task<(PathBuf, Vec<FileDirEntry>)>,
}

#[derive(Clone)]
enum ExplorerMutation {
    Create {
        parent: PathBuf,
        name: String,
        is_dir: bool,
    },
    Rename {
        path: PathBuf,
        name: String,
    },
    Delete {
        path: PathBuf,
    },
}

struct ExplorerMutationOutcome {
    changed_path: PathBuf,
    refresh_dir: PathBuf,
    old_path: Option<PathBuf>,
    was_dir: bool,
    open_created: bool,
}

#[derive(Component)]
struct ExplorerMutationTask {
    webview: Entity,
    operation: ExplorerMutation,
    task: Task<Result<ExplorerMutationOutcome, String>>,
}

#[derive(Component)]
struct ExplorerTreeDirty;

#[derive(Component)]
struct OpenEditorsDirty;

#[derive(Component)]
struct OutlineDirty;

#[derive(Component)]
struct ExplorerChromeSent;

#[derive(Resource, Clone, Copy)]
struct ExplorerChrome {
    visible: bool,
    width: u32,
    client_id: u64,
    request_id: u64,
}

#[derive(Resource, Default)]
struct ExplorerChromeSynced(bool);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
struct SharedFileViewMode(FileViewMode);

impl Default for SharedFileViewMode {
    fn default() -> Self {
        Self(FileViewMode::Note)
    }
}

#[derive(Component)]
struct FileViewModeSent;

#[derive(Component)]
struct NoteSent;

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
type ReadyUnsentViewMode = (
    With<FileView>,
    Without<FileViewModeSent>,
    With<vmux_core::page::PageReady>,
);
type ReadySentViewMode = (
    With<FileView>,
    With<FileViewModeSent>,
    With<vmux_core::page::PageReady>,
);
type ReadyUnsentNote = (Without<NoteSent>, With<vmux_core::page::PageReady>);
type ChangedNoteEditor = (With<FileView>, With<EditState>, Changed<EditState>);
type TreeDirtyReady = (With<ExplorerTreeDirty>, With<vmux_core::page::PageReady>);
type OpenEditorsDirtyReady = (With<OpenEditorsDirty>, With<vmux_core::page::PageReady>);
type OutlineDirtyReady = (With<OutlineDirty>, With<vmux_core::page::PageReady>);
type ChromeUnsentReady = (
    With<FileView>,
    Without<ExplorerChromeSent>,
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
                top_row: 0,
                rows: 0,
            },
            ExplorerState::default(),
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
            WebviewMaterialHandle(webview_mt.add(WebviewExtendStandardMaterial::default())),
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
    mut record_writer: MessageWriter<vmux_core::event::RecordVisitRequest>,
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
        let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();
        // Record only actual files as history/recent-file entries — browsing a
        // directory (the work-dir dir view) is not a "recent file".
        if !path.is_dir() {
            let title = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            record_writer.write(vmux_core::event::RecordVisitRequest {
                url: clean_url.clone(),
                title,
            });
        }
        let pending = parse_goto_fragment(&task.url);
        clear_stack_children(task.stack, &children_q, &mut commands);
        let view = commands
            .spawn((
                new_file_view_bundle(&clean_url, path, &mut meshes, &mut webview_mt),
                ChildOf(task.stack),
            ))
            .id();
        if let Some(pg) = pending {
            commands.entity(view).insert(pg);
        }
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
    store: Option<NonSend<crate::fold_store::FoldStore>>,
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
        let mut core = EditCore::new(
            fv.path.clone(),
            hl.language.clone(),
            &text,
            kind.initial_mode(),
        );
        let mut folds = crate::fold::FoldState::default();
        folds.set_regions(crate::fold::indent_regions(&core.buffer.rope));
        if let Some(store) = &store {
            folds.collapsed.extend(store.get(&fv.path));
            folds.reconcile();
        }
        core.fold_view = folds.view(core.buffer.len_lines() as u32);
        commands.entity(entity).insert((
            EditState { core, hl, folds },
            EditorKeymap(kind.make()),
            vmux_git::GitDiffSource {
                content: text,
                dirty: false,
            },
        ));
    }
}

/// Re-apply the editor keymap to already-open files when `editor.keymap`
/// changes at runtime (the keymap is otherwise only set at file open). Swaps
/// the keymap and resets each editor to the new keymap's initial mode (Vim ->
/// Normal, VSCode -> Insert) so switching to Vim engages without reopening.
fn reapply_keymap_on_change(
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut last: Local<Option<vmux_core::KeymapKind>>,
    mut q: Query<(&mut EditState, &mut EditorKeymap)>,
) {
    let kind = settings_keymap(&settings);
    if *last == Some(kind) {
        return;
    }
    let first = last.is_none();
    *last = Some(kind);
    if first {
        return;
    }
    for (mut edit, mut keymap) in &mut q {
        keymap.0 = kind.make();
        edit.core.mode = kind.initial_mode();
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
            &edit,
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

fn send_file_view_mode(
    mode: Res<SharedFileViewMode>,
    pending: Query<Entity, ReadyUnsentViewMode>,
    sent: Query<Entity, ReadySentViewMode>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let event = FileViewModeEvent { mode: mode.0 };
    for entity in &pending {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_VIEW_MODE_EVENT,
            &event,
        ));
        commands.entity(entity).insert(FileViewModeSent);
    }
    if mode.is_changed() {
        for entity in &sent {
            if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_VIEW_MODE_EVENT,
                &event,
            ));
        }
    }
}

fn active_note_block(blocks: &[NoteBlock], line: u32) -> Option<u32> {
    blocks
        .iter()
        .position(|block| block.start_line <= line && line < block.end_line)
        .map(|index| index as u32)
}

fn send_note(
    mode: Res<SharedFileViewMode>,
    q: Query<(Entity, &FileView, &EditState), ReadyUnsentNote>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if mode.0 != FileViewMode::Note {
        return;
    }
    for (entity, file, edit) in &q {
        if !crate::markdown::is_markdown_path(&file.path) {
            commands.entity(entity).insert(NoteSent);
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        let note = crate::markdown::parse_note_document(&edit.core.buffer.text());
        let active = active_note_block(&note.blocks, edit.core.cursor_pos().line);
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_NOTE_EVENT,
            &FileNoteEvent {
                title: note.title,
                blocks: note.blocks,
                active,
            },
        ));
        commands.entity(entity).insert(NoteSent);
    }
}

fn mark_note_dirty(q: Query<Entity, ChangedNoteEditor>, mut commands: Commands) {
    for entity in &q {
        commands.entity(entity).remove::<NoteSent>();
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
    edit: &mut EditState,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let total = edit.core.buffer.len_lines() as u32;
    let view = edit.folds.view(total);
    let visible = view.visible_count();
    let (vis_first, vis_end) = window_range(visible, vp.top_row, vp.rows);
    let overscan = vmux_core::scroll::overscan_for(
        vp.rows,
        vmux_core::scroll::EDITOR_OVERSCAN_K,
        vmux_core::scroll::OVERSCAN_FLOOR,
        vmux_core::scroll::OVERSCAN_CAP,
    );
    let first_row = vis_first.saturating_sub(overscan);
    let end_row = (vis_end + overscan).min(visible);
    let line_nos = view.lines_for_window(first_row, end_row.saturating_sub(first_row));
    let mut lines = Vec::with_capacity(line_nos.len());
    for ln in line_nos {
        let mut fl = edit
            .hl
            .line_window(&edit.core.buffer.rope, ln as usize, ln as usize + 1);
        if let Some(mut l) = fl.pop() {
            l.fold = edit.folds.gutter(ln);
            lines.push(l);
        }
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_VIEWPORT_EVENT,
        &FileViewportPatch {
            first_row,
            total_rows: visible,
            total_lines: total,
            lines,
        },
    ));
}

fn emit_cursor(
    entity: Entity,
    edit: &EditState,
    keymap: &dyn Keymap,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
        return;
    }
    let _ = vp;
    let total = edit.core.buffer.len_lines() as u32;
    let view = edit.folds.view(total);
    let mut primary = edit.core.cursor_pos();
    primary.row = view.buffer_to_row(primary.line);
    let selections = edit
        .core
        .sel_spans(0, total as u16)
        .into_iter()
        .filter(|s| !view.is_hidden(s.line))
        .map(|mut s| {
            s.row = view.buffer_to_row(s.line);
            s
        })
        .collect();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_CURSOR_EVENT,
        &FileCursorEvent {
            mode: keymap.mode(),
            mode_label: keymap.mode_label(),
            primary,
            selections,
        },
    ));
}

fn rehighlight_on_color_scheme(
    mut reader: bevy::ecs::message::MessageReader<vmux_setting::ColorSchemeChanged>,
    mut views: Query<(Entity, &mut EditState, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(ev) = reader.read().last().copied() else {
        return;
    };
    crate::highlight::set_dark_theme(matches!(ev.0, vmux_setting::ResolvedScheme::Dark));
    for (entity, mut edit, vp) in &mut views {
        let vpc = *vp;
        emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
    }
}

fn reset_file_sent_markers_on_page_ready(
    trigger: On<BinReceive<vmux_core::page::PageReady>>,
    file_views: Query<&FileView>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(fv) = file_views.get(entity) else {
        return;
    };
    commands
        .entity(entity)
        .remove::<FileInitialMetaSent>()
        .remove::<FileThemeSent>()
        .remove::<FileViewModeSent>()
        .remove::<NoteSent>()
        .remove::<crate::lsp::manager::LspStatusSent>()
        .remove::<crate::lsp::manager::DiagSent>()
        .remove::<ExplorerChromeSent>()
        .insert(ExplorerTreeDirty)
        .insert(OpenEditorsDirty);
    if crate::explorer_model::is_markdown(&fv.path) {
        commands.entity(entity).insert(OutlineDirty);
    }
}

fn on_file_view_mode_set(
    trigger: On<BinReceive<FileViewModeSet>>,
    views: Query<(), With<FileView>>,
    files: Query<&FileView>,
    mut mode: ResMut<SharedFileViewMode>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if views.contains(entity) {
        mode.0 = trigger.event().payload.mode;
        if mode.0 == FileViewMode::Note
            && files
                .get(entity)
                .is_ok_and(|file| crate::markdown::is_markdown_path(&file.path))
        {
            commands.entity(entity).remove::<NoteSent>();
        }
    }
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
                &edit,
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
    let visible = edit.folds.view(total).visible_count();
    vp.top_row = clamp_top_line(evt.top_row, visible, vp.rows);
    let vpc = *vp;
    emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
    emit_cursor(
        entity,
        &edit,
        keymap.0.as_ref(),
        &vpc,
        &browsers,
        &mut commands,
    );
}

fn on_file_fold_toggle(
    trigger: On<BinReceive<FileFoldToggle>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let line = trigger.event().payload.line;
    let Ok((mut edit, keymap, vp)) = q.get_mut(entity) else {
        return;
    };
    edit.folds.toggle(line);
    sync_fold_view(&mut edit);
    let vpc = *vp;
    emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
    emit_cursor(
        entity,
        &edit,
        keymap.0.as_ref(),
        &vpc,
        &browsers,
        &mut commands,
    );
    commands.entity(entity).insert(FoldsDirty);
}

fn persist_folds(
    q: Query<(Entity, &FileView, &EditState), With<FoldsDirty>>,
    mut store: NonSendMut<crate::fold_store::FoldStore>,
    mut commands: Commands,
) {
    let mut changed = false;
    for (entity, fv, edit) in q.iter() {
        let mut collapsed: Vec<u32> = edit.folds.collapsed.iter().copied().collect();
        collapsed.sort_unstable();
        store.set(&fv.path, &collapsed);
        commands.entity(entity).remove::<FoldsDirty>();
        changed = true;
    }
    if changed {
        store.save();
    }
}

fn apply_lsp_folds(
    mut msgs: MessageReader<crate::lsp::manager::LspFolds>,
    mut q: Query<(&mut EditState, &FileView, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for f in msgs.read() {
        let Ok((mut edit, fv, keymap, vp)) = q.get_mut(f.entity) else {
            continue;
        };
        if canon(&fv.path) != canon(&f.path) {
            continue;
        }
        let regions = if f.regions.is_empty() {
            crate::fold::indent_regions(&edit.core.buffer.rope)
        } else {
            f.regions.clone()
        };
        edit.folds.set_regions(regions);
        sync_fold_view(&mut edit);
        let vpc = *vp;
        emit_window(f.entity, &mut edit, &vpc, &browsers, &mut commands);
        emit_cursor(
            f.entity,
            &edit,
            keymap.0.as_ref(),
            &vpc,
            &browsers,
            &mut commands,
        );
    }
}

/// Mirror the set of paths the raw-media handler may serve into the CEF allowlist
/// each frame: open media views plus every file inside an open directory (so the
/// dir browser can preview/play any of its files without a per-selection race).
fn sync_media_allowlist(media: Query<&FileView, With<FileMedia>>, dirs: Query<&FileDir>) {
    let mut paths: std::collections::HashSet<std::path::PathBuf> =
        media.iter().map(|fv| fv.path.clone()).collect();
    for dir in &dirs {
        for entry in &dir.entries {
            paths.insert(std::path::PathBuf::from(&entry.path));
        }
    }
    set_media_allowlist(paths);
}

/// Build the raw-media URL (`file://<abs>?vmux-raw=1`) that the page points media
/// elements at; the CEF resource handler range-serves the file behind it.
fn raw_media_url(path: &std::path::Path) -> String {
    let mut url = url::Url::from_file_path(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", path.to_string_lossy()));
    url.push_str("?vmux-raw=1");
    url
}

/// Emit [`FileMediaEvent`] once the page is ready, so it can render the media
/// element pointed at the raw-media URL.
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

/// Containers AVFoundation decodes but this codec-less CEF build cannot. Open-codec
/// containers (webm/ogv) play in the page `<video>`, so we must not cover them with
/// a native overlay that AVFoundation can't render.
fn needs_native_video(path: &Path) -> bool {
    vmux_core::media::is_proprietary_video(&path.to_string_lossy())
}

/// Attach a native macOS `AVPlayer` overlay filling a full video view. This CEF
/// build lacks proprietary codecs (H.264/HEVC), so `.mov`/`.mp4` won't play in
/// `<video>`; the overlay decodes them through AVFoundation. Idempotent per path.
fn attach_video_overlays(q: Query<(Entity, &FileView, &FileMedia)>, browsers: NonSend<Browsers>) {
    for (entity, fv, media) in &q {
        if media.kind != vmux_core::media::MediaKind::Video || !needs_native_video(&fv.path) {
            continue;
        }
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        browsers.attach_media_overlay(&entity, &fv.path.to_string_lossy());
    }
}

/// Position/replace the native overlay over the dir-browser preview pane, using the
/// rect the page measured for its video host element.
fn on_file_video_rect(
    trigger: On<BinReceive<FileVideoRect>>,
    file_views: Query<(), With<FileView>>,
    browsers: NonSend<Browsers>,
) {
    let entity = trigger.event().webview;
    if file_views.get(entity).is_err() || !browsers.has_browser(entity) {
        return;
    }
    let r = &trigger.event().payload;
    if !vmux_core::media::is_proprietary_video(&r.path) || r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    browsers.set_media_overlay(&entity, &r.path, (r.x, r.y, r.w, r.h));
}

/// Tear down the native video overlay when a view stops being a video media view
/// or a dir browser (navigated away, reloaded as text, or despawned).
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
    let req = trigger.event().payload.clone();
    let path = PathBuf::from(&req.path);
    if !needs_native_video(&path) {
        browsers.detach_media_overlay(&entity);
    }
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

/// Open a media file in the system default app (the PDF view's "Open externally"
/// action). Restricted to the requesting view's own path.
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

#[allow(clippy::too_many_arguments)]
fn navigate_file_view(
    entity: Entity,
    path: PathBuf,
    top_line: u32,
    fv: &mut FileView,
    vp: &mut FileViewport,
    meta: &mut PageMetadata,
    manager: &mut crate::lsp::manager::LspManager,
    commands: &mut Commands,
) {
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
    vp.top_row = top_line;
    commands
        .entity(entity)
        .remove::<FileDir>()
        .remove::<FileBuffer>()
        .remove::<FileMedia>()
        .remove::<EditState>()
        .remove::<vmux_git::GitDiffSource>()
        .remove::<EditorKeymap>()
        .remove::<NoteSent>()
        .remove::<LspEditDirty>()
        .remove::<FileInitialMetaSent>()
        .remove::<crate::lsp::manager::LspOpened>()
        .remove::<crate::lsp::manager::LintRan>();
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
    navigate_file_view(
        entity,
        path,
        0,
        &mut fv,
        &mut vp,
        &mut meta,
        &mut manager,
        &mut commands,
    );
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

fn reconcile_file_watches(
    q: Query<(&FileView, &ExplorerState)>,
    watch: Option<NonSendMut<FileWatch>>,
) {
    let Some(mut watch) = watch else {
        return;
    };
    for (fv, st) in &q {
        if let Some(dir) = watch_dir_for(&fv.path)
            && watch.dirs.insert(dir.clone())
        {
            let _ = watch.watcher.watch(&dir, RecursiveMode::NonRecursive);
        }
        for dir in st.expanded.iter() {
            if watch.dirs.insert(dir.clone()) {
                let _ = watch.watcher.watch(dir, RecursiveMode::NonRecursive);
            }
        }
    }
}

fn drain_file_changes(
    watch: Option<NonSend<FileWatch>>,
    self_writes: Option<NonSendMut<SelfWrites>>,
    mut q: Query<(Entity, &FileView, &mut ExplorerState)>,
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
    for (entity, fv, mut st) in &mut q {
        let cp = canon(&fv.path);
        let self_written = sw
            .as_ref()
            .map(|sw| sw.0.contains_key(&cp))
            .unwrap_or(false);
        if changed.contains(&cp) && !self_written {
            commands.entity(entity).insert(FileReloadRequested);
        }
        let cached: Vec<PathBuf> = st.children.keys().cloned().collect();
        for d in cached {
            let dc = canon(&d);
            if changed
                .iter()
                .any(|c| c.parent().map(|p| canon(p) == dc).unwrap_or(false))
            {
                let _ = start_explorer_dir_load(entity, d, &mut st, &mut commands, true);
            }
        }
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
            .remove::<vmux_git::GitDiffSource>()
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

fn sync_fold_view(edit: &mut EditState) {
    let total = edit.core.buffer.len_lines() as u32;
    edit.core.fold_view = edit.folds.view(total);
}

#[allow(clippy::too_many_arguments)]
fn run_commands(
    entity: Entity,
    cmds: Vec<EditCommand>,
    edit: &mut EditState,
    diff_source: &mut vmux_git::GitDiffSource,
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
    let mut fold_changed = false;
    for cmd in cmds {
        if matches!(
            cmd,
            EditCommand::FoldToggle
                | EditCommand::FoldOpen
                | EditCommand::FoldClose
                | EditCommand::FoldToggleRecursive
                | EditCommand::FoldAll
                | EditCommand::UnfoldAll
        ) {
            let line = edit.core.cursor_pos().line;
            match cmd {
                EditCommand::FoldToggle => edit.folds.toggle(line),
                EditCommand::FoldOpen => edit.folds.open(line),
                EditCommand::FoldClose => edit.folds.close(line),
                EditCommand::FoldToggleRecursive => edit.folds.toggle_recursive(line),
                EditCommand::FoldAll => edit.folds.fold_all(),
                EditCommand::UnfoldAll => edit.folds.unfold_all(),
                _ => {}
            }
            sync_fold_view(edit);
            if let Some(header) = edit.folds.hiding_header(line) {
                let at = edit.core.buffer.line_to_char(header as usize);
                edit.core.set_caret(at);
            }
            fold_changed = true;
            continue;
        }
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
    if text_changed {
        let regions = crate::fold::indent_regions(&edit.core.buffer.rope);
        edit.folds.set_regions(regions);
        sync_fold_view(edit);
    }
    {
        let total = edit.core.buffer.len_lines() as u32;
        let caret_line = edit.core.cursor_pos().line;
        if edit.folds.view(total).is_hidden(caret_line) {
            edit.folds.reveal(caret_line);
            sync_fold_view(edit);
            fold_changed = true;
        }
    }
    if let Some(top) = edit.core.autoscroll_rows(vp.top_row, vp.rows, &edit.folds) {
        vp.top_row = top;
        text_changed = true;
    }
    let vpc = *vp;
    if text_changed || fold_changed {
        emit_window(entity, edit, &vpc, browsers, commands);
    }
    if text_changed || sel_or_mode || fold_changed {
        emit_cursor(entity, edit, keymap, &vpc, browsers, commands);
    }
    if fold_changed {
        commands.entity(entity).insert(FoldsDirty);
    }
    if text_changed || dirty_changed {
        diff_source.content = edit.core.buffer.text();
        diff_source.dirty = edit.core.dirty;
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
    mut q: Query<(
        &mut EditState,
        &mut EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    let Ok((mut edit, mut keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
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
        &mut diff_source,
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
    mut q: Query<(
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
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
    let Ok((mut edit, keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
        return;
    };
    if !keymap.0.mode().accepts_text() {
        return;
    }
    run_commands(
        entity,
        vec![EditCommand::InsertText(text)],
        &mut edit,
        &mut diff_source,
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
    /// When set, select from `utf16_col` to this column on `line` (highlights a
    /// match); otherwise just place the caret.
    select_end_col: Option<u32>,
}

/// Parse an editor goto fragment from a `file://` URL: `#L<line>` (1-based) or
/// `#L<line>:<col>-<end>` (0-based cols, to highlight a match).
fn parse_goto_fragment(url: &str) -> Option<PendingGoto> {
    let body = url.split_once('#')?.1.strip_prefix('L')?;
    let (line_s, sel) = match body.split_once(':') {
        Some((l, r)) => (l, Some(r)),
        None => (body, None),
    };
    let line = line_s.parse::<u32>().ok()?.saturating_sub(1);
    let (utf16_col, select_end_col) = match sel.and_then(|r| r.split_once('-')) {
        Some((s, e)) => (s.parse().unwrap_or(0), e.parse::<u32>().ok()),
        None => (0, None),
    };
    Some(PendingGoto {
        line,
        utf16_col,
        select_end_col,
    })
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
    mut q: Query<(
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: NonSendMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload.clone();
    let Ok((mut edit, keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
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
        &mut diff_source,
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
    if let Some(top) = edit.core.autoscroll_rows(vp.top_row, vp.rows, &edit.folds) {
        vp.top_row = top;
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
                &edit,
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
            vp.top_row = 0;
            commands
                .entity(g.entity)
                .remove::<EditState>()
                .remove::<vmux_git::GitDiffSource>()
                .remove::<FileBuffer>()
                .remove::<FileMedia>()
                .remove::<FileDir>()
                .remove::<FileInitialMetaSent>()
                .remove::<crate::lsp::manager::LspOpened>()
                .remove::<crate::lsp::manager::LintRan>()
                .insert(PendingGoto {
                    line: g.line,
                    utf16_col: g.utf16_col,
                    select_end_col: None,
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
        if let Some(end) = pg.select_end_col {
            let line = (pg.line as usize).min(edit.core.buffer.len_lines().saturating_sub(1));
            let lt: String = edit
                .core
                .buffer
                .rope
                .line(line)
                .chars()
                .filter(|c| *c != '\n' && *c != '\r')
                .collect();
            let s = crate::lsp::manager::utf16_to_char_col(&lt, pg.utf16_col) as usize;
            let e = crate::lsp::manager::utf16_to_char_col(&lt, end) as usize;
            let a = edit.core.buffer.coords_to_char(line, s);
            let b = edit.core.buffer.coords_to_char(line, e);
            edit.core.selections = vec![Selection { anchor: a, head: b }];
        }
        let vpc = *vp;
        emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
        emit_cursor(
            entity,
            &edit,
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
        &edit,
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
        manager.folding_range(entity, &fv.path);
        if !crate::explorer_model::is_markdown(&fv.path) {
            manager.document_symbol(entity, &fv.path);
        }
        commands.entity(entity).remove::<LspEditDirty>();
    }
}

fn explorer_root_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_uppercase())
        .unwrap_or_else(|| root.to_string_lossy().to_uppercase())
}

fn start_explorer_dir_load(
    entity: Entity,
    path: PathBuf,
    st: &mut ExplorerState,
    commands: &mut Commands,
    force: bool,
) -> bool {
    if st.loading.contains(&path) || !force && st.children.contains_key(&path) {
        return false;
    }
    st.loading.insert(path.clone());
    let task_path = path.clone();
    let task = IoTaskPool::get().spawn(async move {
        let entries = list_dir(&task_path);
        (task_path, entries)
    });
    commands.spawn(ExplorerDirLoadTask {
        webview: entity,
        task,
    });
    commands.entity(entity).insert(ExplorerTreeDirty);
    true
}

fn explorer_path_allowed(st: &ExplorerState, path: &Path) -> bool {
    path == st.root || path.starts_with(&st.root)
}

fn reveal_current_in_tree(
    entity: Entity,
    current: &Path,
    st: &mut ExplorerState,
    commands: &mut Commands,
) {
    let mut tree_changed = false;
    let root = project_root(current);
    if st.root != root {
        st.root = root;
        st.expanded.clear();
        st.loading.clear();
        st.children.clear();
        st.focus_path = None;
        tree_changed = true;
    }
    let current_dir = if current.is_dir() {
        current
    } else {
        current.parent().unwrap_or(current)
    };
    let Ok(relative) = current_dir.strip_prefix(&st.root) else {
        return;
    };
    let mut dir = st.root.clone();
    tree_changed |= st.expanded.insert(dir.clone());
    tree_changed |= start_explorer_dir_load(entity, dir.clone(), st, commands, false);
    for component in relative.components() {
        dir.push(component);
        tree_changed |= st.expanded.insert(dir.clone());
        tree_changed |= start_explorer_dir_load(entity, dir.clone(), st, commands, false);
    }
    if tree_changed {
        st.focus_path = Some(current.to_path_buf());
        commands.entity(entity).insert(ExplorerTreeDirty);
    }
}

fn emit_explorer_focus(
    entity: Entity,
    current: &Path,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_FOCUS_EVENT,
            &ExplorerFocusEvent {
                path: current.to_string_lossy().into_owned(),
            },
        ));
    }
}

fn init_explorer_state(
    mut q: Query<(Entity, &FileView, &mut ExplorerState)>,
    mut commands: Commands,
) {
    for (entity, fv, mut st) in &mut q {
        if !st.root.as_os_str().is_empty() {
            continue;
        }
        let root = project_root(&fv.path);
        st.expanded.insert(root.clone());
        st.root = root.clone();
        let _ = start_explorer_dir_load(entity, root, &mut st, &mut commands, false);
    }
}

fn drain_explorer_dir_loads(
    mut tasks: Query<(Entity, &mut ExplorerDirLoadTask)>,
    mut states: Query<&mut ExplorerState>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some((path, entries)) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let webview = pending.webview;
        commands.entity(task_entity).despawn();
        let Ok(mut st) = states.get_mut(webview) else {
            continue;
        };
        if !st.loading.remove(&path) {
            continue;
        }
        st.children.insert(path, entries);
        commands.entity(webview).insert(ExplorerTreeDirty);
    }
}

fn emit_explorer_tree(
    mut q: Query<(Entity, &FileView, &mut ExplorerState), TreeDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, mut st) in &mut q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        let rows = flatten_tree(&st.root, &st.expanded, &st.loading, &st.children);
        let focus_ready = st.focus_path.as_ref().is_some_and(|path| {
            path == &st.root || rows.iter().any(|row| Path::new(&row.path) == path)
        });
        let focus_path = if focus_ready {
            st.focus_path
                .take()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        };
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_TREE_EVENT,
            &ExplorerTreeEvent {
                root_name: explorer_root_name(&st.root),
                root_path: st.root.to_string_lossy().into_owned(),
                current_path: fv.path.to_string_lossy().into_owned(),
                focus_path,
                loading: st.loading.contains(&st.root),
                rows,
            },
        ));
        commands.entity(entity).remove::<ExplorerTreeDirty>();
    }
}

fn on_explorer_tree_toggle(
    trigger: On<BinReceive<ExplorerTreeToggle>>,
    mut q: Query<&mut ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(mut st) = q.get_mut(entity) else {
        return;
    };
    if st.expanded.contains(&path) {
        st.expanded.remove(&path);
    } else {
        if !explorer_path_allowed(&st, &path) {
            return;
        }
        st.expanded.insert(path.clone());
        let _ = start_explorer_dir_load(entity, path, &mut st, &mut commands, false);
    }
    commands.entity(entity).insert(ExplorerTreeDirty);
}

fn on_explorer_tree_prefetch(
    trigger: On<BinReceive<ExplorerTreePrefetch>>,
    mut q: Query<&mut ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(mut st) = q.get_mut(entity) else {
        return;
    };
    if explorer_path_allowed(&st, &path) {
        let _ = start_explorer_dir_load(entity, path, &mut st, &mut commands, false);
    }
}

fn on_explorer_tree_refresh(
    trigger: On<BinReceive<ExplorerTreeRefresh>>,
    mut q: Query<&mut ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(mut st) = q.get_mut(entity) else {
        return;
    };
    if explorer_path_allowed(&st, &path) {
        let _ = start_explorer_dir_load(entity, path, &mut st, &mut commands, true);
    }
}

fn on_explorer_reveal_current(
    trigger: On<BinReceive<ExplorerRevealCurrent>>,
    mut q: Query<(&FileView, &mut ExplorerState)>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((fv, mut st)) = q.get_mut(entity) else {
        return;
    };
    reveal_current_in_tree(entity, &fv.path, &mut st, &mut commands);
    if let Some(browsers) = browsers {
        emit_explorer_focus(entity, &fv.path, &browsers, &mut commands);
    }
}

fn run_explorer_mutation(
    root: PathBuf,
    operation: ExplorerMutation,
) -> Result<ExplorerMutationOutcome, String> {
    match operation {
        ExplorerMutation::Create {
            parent,
            name,
            is_dir,
        } => {
            let changed_path = crate::explorer_fs::create_entry(&root, &parent, &name, is_dir)?;
            Ok(ExplorerMutationOutcome {
                changed_path,
                refresh_dir: parent,
                old_path: None,
                was_dir: is_dir,
                open_created: !is_dir,
            })
        }
        ExplorerMutation::Rename { path, name } => {
            let refresh_dir = path
                .parent()
                .ok_or_else(|| "Explorer root cannot be changed".to_string())?
                .to_path_buf();
            let (changed_path, was_dir) = crate::explorer_fs::rename_entry(&root, &path, &name)?;
            Ok(ExplorerMutationOutcome {
                changed_path,
                refresh_dir,
                old_path: Some(path),
                was_dir,
                open_created: false,
            })
        }
        ExplorerMutation::Delete { path } => {
            let (refresh_dir, was_dir) = crate::explorer_fs::delete_entry(&root, &path)?;
            Ok(ExplorerMutationOutcome {
                changed_path: path.clone(),
                refresh_dir,
                old_path: Some(path),
                was_dir,
                open_created: false,
            })
        }
    }
}

fn start_explorer_mutation(
    entity: Entity,
    root: PathBuf,
    operation: ExplorerMutation,
    commands: &mut Commands,
) {
    let task_operation = operation.clone();
    let task = IoTaskPool::get().spawn(async move { run_explorer_mutation(root, task_operation) });
    commands.spawn(ExplorerMutationTask {
        webview: entity,
        operation,
        task,
    });
}

fn on_explorer_create(
    trigger: On<BinReceive<ExplorerCreate>>,
    q: Query<&ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(st) = q.get(entity) else {
        return;
    };
    let payload = &trigger.event().payload;
    start_explorer_mutation(
        entity,
        st.root.clone(),
        ExplorerMutation::Create {
            parent: PathBuf::from(&payload.parent),
            name: payload.name.clone(),
            is_dir: payload.is_dir,
        },
        &mut commands,
    );
}

fn on_explorer_rename(
    trigger: On<BinReceive<ExplorerRename>>,
    q: Query<&ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(st) = q.get(entity) else {
        return;
    };
    let payload = &trigger.event().payload;
    start_explorer_mutation(
        entity,
        st.root.clone(),
        ExplorerMutation::Rename {
            path: PathBuf::from(&payload.path),
            name: payload.name.clone(),
        },
        &mut commands,
    );
}

fn on_explorer_delete(
    trigger: On<BinReceive<ExplorerDelete>>,
    q: Query<&ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(st) = q.get(entity) else {
        return;
    };
    start_explorer_mutation(
        entity,
        st.root.clone(),
        ExplorerMutation::Delete {
            path: PathBuf::from(&trigger.event().payload.path),
        },
        &mut commands,
    );
}

fn remap_path(path: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    path.strip_prefix(old).ok().map(|suffix| new.join(suffix))
}

fn evict_explorer_subtree(st: &mut ExplorerState, path: &Path) {
    st.expanded.retain(|entry| !entry.starts_with(path));
    st.loading.retain(|entry| !entry.starts_with(path));
    st.children.retain(|entry, _| !entry.starts_with(path));
}

fn explorer_mutation_message(
    operation: &ExplorerMutation,
    outcome: &ExplorerMutationOutcome,
) -> String {
    match operation {
        ExplorerMutation::Create { is_dir: true, .. } => format!(
            "Created folder {}",
            outcome
                .changed_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        ExplorerMutation::Create { is_dir: false, .. } => format!(
            "Created file {}",
            outcome
                .changed_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        ExplorerMutation::Rename { .. } => format!(
            "Renamed to {}",
            outcome
                .changed_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        ExplorerMutation::Delete { path } => format!(
            "Deleted {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
    }
}

fn emit_explorer_fs_result(
    webview: Entity,
    ok: bool,
    message: String,
    open_path: String,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if browsers.has_browser(webview) && browsers.host_emit_ready(&webview) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            EXPLORER_FS_RESULT_EVENT,
            &ExplorerFsResult {
                ok,
                message,
                open_path,
            },
        ));
    }
}

fn drain_explorer_mutations(
    mut tasks: Query<(Entity, &mut ExplorerMutationTask)>,
    mut views: Query<(&FileView, &mut ExplorerState)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let webview = pending.webview;
        let operation = pending.operation.clone();
        commands.entity(task_entity).despawn();
        let Ok((fv, mut st)) = views.get_mut(webview) else {
            continue;
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                emit_explorer_fs_result(
                    webview,
                    false,
                    error,
                    String::new(),
                    &browsers,
                    &mut commands,
                );
                continue;
            }
        };
        let mut open_path = if outcome.open_created {
            Some(outcome.changed_path.clone())
        } else {
            None
        };
        if let Some(old_path) = outcome.old_path.as_ref() {
            match &operation {
                ExplorerMutation::Rename { .. } => {
                    for open in &mut st.open_editors {
                        if let Some(remapped) = remap_path(open, old_path, &outcome.changed_path) {
                            *open = remapped;
                        }
                    }
                    if let Some(remapped) = remap_path(&fv.path, old_path, &outcome.changed_path) {
                        open_path = Some(remapped);
                    }
                }
                ExplorerMutation::Delete { .. } => {
                    st.open_editors.retain(|open| !open.starts_with(old_path));
                    if fv.path.starts_with(old_path) {
                        open_path = Some(outcome.refresh_dir.clone());
                    }
                }
                ExplorerMutation::Create { .. } => {}
            }
            if outcome.was_dir {
                evict_explorer_subtree(&mut st, old_path);
            }
        }
        let _ = start_explorer_dir_load(
            webview,
            outcome.refresh_dir.clone(),
            &mut st,
            &mut commands,
            true,
        );
        commands
            .entity(webview)
            .insert((ExplorerTreeDirty, OpenEditorsDirty));
        emit_explorer_fs_result(
            webview,
            true,
            explorer_mutation_message(&operation, &outcome),
            open_path
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            &browsers,
            &mut commands,
        );
    }
}

fn sync_explorer_chrome(
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut chrome: ResMut<ExplorerChrome>,
    mut synced: ResMut<ExplorerChromeSynced>,
    views: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    if synced.0 {
        return;
    }
    let Some(settings) = settings else {
        return;
    };
    chrome.visible = settings.editor.explorer.visible();
    chrome.width = settings.editor.explorer.width();
    synced.0 = true;
    for e in &views {
        commands.entity(e).remove::<ExplorerChromeSent>();
    }
}

fn emit_explorer_chrome(
    q: Query<Entity, ChromeUnsentReady>,
    chrome: Res<ExplorerChrome>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for entity in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_CHROME_EVENT,
            &ExplorerChromeEvent {
                visible: chrome.visible,
                width: chrome.width,
                client_id: chrome.client_id,
                request_id: chrome.request_id,
            },
        ));
        commands.entity(entity).insert(ExplorerChromeSent);
    }
}

fn persist_chrome(
    chrome: ExplorerChrome,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>>,
) {
    let Some(mut settings) = settings else {
        return;
    };
    settings.editor.explorer.visible = Some(chrome.visible);
    settings.editor.explorer.width = Some(chrome.width);
    if let Some(mut saves) = saves {
        saves.write(vmux_setting::SettingsSaveRequest);
    }
}

fn mark_chrome_unsent(views: &Query<Entity, With<FileView>>, commands: &mut Commands) {
    for e in views {
        commands.entity(e).remove::<ExplorerChromeSent>();
    }
}

fn on_explorer_panel_set_visible(
    trigger: On<BinReceive<ExplorerPanelSetVisible>>,
    mut chrome: ResMut<ExplorerChrome>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>>,
    mut editors: Query<(Entity, &FileView, &mut ExplorerState)>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    chrome.visible = trigger.event().payload.visible;
    chrome.client_id = trigger.event().payload.client_id;
    chrome.request_id = trigger.event().payload.request_id;
    persist_chrome(*chrome, settings, saves);
    let entity = trigger.event().webview;
    for (view, _, _) in &mut editors {
        if view == entity {
            commands.entity(view).insert(ExplorerChromeSent);
        } else {
            commands.entity(view).remove::<ExplorerChromeSent>();
        }
    }
    if chrome.visible
        && let Ok((_, fv, mut st)) = editors.get_mut(entity)
    {
        reveal_current_in_tree(entity, &fv.path, &mut st, &mut commands);
        if let Some(browsers) = browsers {
            emit_explorer_focus(entity, &fv.path, &browsers, &mut commands);
        }
    }
}

fn on_explorer_panel_width(
    trigger: On<BinReceive<ExplorerPanelWidth>>,
    mut chrome: ResMut<ExplorerChrome>,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>>,
    views: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    chrome.width = trigger.event().payload.px.clamp(
        vmux_setting::EXPLORER_MIN_WIDTH,
        vmux_setting::EXPLORER_MAX_WIDTH,
    );
    persist_chrome(*chrome, settings, saves);
    mark_chrome_unsent(&views, &mut commands);
}

fn sync_open_editors(
    mut q: Query<(Entity, &FileView, &mut ExplorerState), Changed<FileView>>,
    mut commands: Commands,
) {
    for (entity, fv, mut st) in &mut q {
        if !fv.path.is_dir() {
            crate::explorer_model::note_open(&mut st.open_editors, &fv.path);
        }
        commands.entity(entity).insert(OpenEditorsDirty);
    }
}

fn open_editor_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn emit_open_editors(
    q: Query<(Entity, &FileView, &ExplorerState, Option<&EditState>), OpenEditorsDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, st, edit) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        let active_dirty = edit.map(|e| e.core.dirty).unwrap_or(false);
        let items = st
            .open_editors
            .iter()
            .map(|p| {
                let active = *p == fv.path;
                OpenEditorItem {
                    name: open_editor_name(p),
                    path: p.to_string_lossy().into_owned(),
                    active,
                    dirty: active && active_dirty,
                }
            })
            .collect();
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_OPEN_EDITORS_EVENT,
            &OpenEditorsEvent { items },
        ));
        commands.entity(entity).remove::<OpenEditorsDirty>();
    }
}

fn on_explorer_close_editor(
    trigger: On<BinReceive<ExplorerCloseEditor>>,
    mut q: Query<&mut ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(mut st) = q.get_mut(entity) else {
        return;
    };
    crate::explorer_model::close(&mut st.open_editors, &path);
    commands.entity(entity).insert(OpenEditorsDirty);
}

fn mark_outline_dirty(q: Query<(Entity, &FileView), Changed<EditState>>, mut commands: Commands) {
    for (entity, fv) in &q {
        if crate::explorer_model::is_markdown(&fv.path) {
            commands.entity(entity).insert(OutlineDirty);
        }
    }
}

fn emit_outline_markdown(
    q: Query<(Entity, &EditState), OutlineDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, edit) in &q {
        if !browsers.has_browser(entity) || !browsers.host_emit_ready(&entity) {
            continue;
        }
        let items = crate::explorer_model::markdown_outline(&edit.core.buffer.text());
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_OUTLINE_EVENT,
            &OutlineEvent { items },
        ));
        commands.entity(entity).remove::<OutlineDirty>();
    }
}

fn clear_outline_on_file_change(
    q: Query<Entity, (With<FileView>, Changed<FileView>)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for entity in &q {
        if browsers.has_browser(entity) && browsers.host_emit_ready(&entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                EXPLORER_OUTLINE_EVENT,
                &OutlineEvent { items: Vec::new() },
            ));
        }
    }
}

fn on_explorer_goto(
    trigger: On<BinReceive<ExplorerGoto>>,
    views: Query<&FileView>,
    mut goto_w: MessageWriter<crate::lsp::manager::LspGoto>,
) {
    let entity = trigger.event().webview;
    let Ok(fv) = views.get(entity) else {
        return;
    };
    goto_w.write(crate::lsp::manager::LspGoto {
        entity,
        path: fv.path.clone(),
        line: trigger.event().payload.line,
        utf16_col: 0,
    });
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
            .insert_non_send(crate::fold_store::FoldStore::load())
            .insert_resource(ExplorerChrome {
                visible: false,
                width: vmux_setting::EXPLORER_DEFAULT_WIDTH,
                client_id: 0,
                request_id: 0,
            })
            .init_resource::<ExplorerChromeSynced>()
            .init_resource::<SharedFileViewMode>()
            .add_message::<vmux_core::event::RecordVisitRequest>()
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
                FileFoldToggle,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                FileCompletionRequest,
                FileGotoRequest,
                FileCompletionCommit,
                FileOpenExternalRequest,
                FileVideoRect,
                FileViewModeSet,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                ExplorerTreeToggle,
                ExplorerTreePrefetch,
                ExplorerTreeRefresh,
                ExplorerRevealCurrent,
                ExplorerCreate,
                ExplorerRename,
                ExplorerDelete,
                ExplorerCloseEditor,
                ExplorerPanelSetVisible,
                ExplorerPanelWidth,
                ExplorerGoto,
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
                    sync_media_allowlist,
                    send_initial_media.after(sync_media_allowlist),
                    (detach_video_overlays, attach_video_overlays).chain(),
                    send_file_theme,
                    send_file_view_mode,
                    rehighlight_on_color_scheme,
                    drain_thumb_tasks,
                    reconcile_file_watches,
                    flush_lsp_changes,
                    apply_goto,
                    apply_pending_goto,
                    reapply_keymap_on_change,
                    apply_lsp_folds,
                    persist_folds,
                    (drain_file_changes, reload_changed_files).chain(),
                ),
            )
            .add_systems(Update, (mark_note_dirty, send_note.after(mark_note_dirty)))
            .add_systems(Update, (drain_explorer_dir_loads, drain_explorer_mutations))
            .add_systems(
                Update,
                (
                    init_explorer_state,
                    emit_explorer_tree,
                    sync_explorer_chrome,
                    emit_explorer_chrome,
                    sync_open_editors,
                    emit_open_editors,
                    mark_outline_dirty,
                    emit_outline_markdown,
                    clear_outline_on_file_change,
                ),
            )
            .add_observer(reset_file_sent_markers_on_page_ready)
            .add_observer(on_file_resize)
            .add_observer(on_file_scroll)
            .add_observer(on_file_preview_request)
            .add_observer(on_file_open)
            .add_observer(on_file_open_external)
            .add_observer(on_file_video_rect)
            .add_observer(on_file_key)
            .add_observer(on_file_text_input)
            .add_observer(on_file_pointer)
            .add_observer(on_file_hover_request)
            .add_observer(on_file_definition_request)
            .add_observer(on_file_references_request)
            .add_observer(on_file_completion_request)
            .add_observer(on_file_goto_request)
            .add_observer(on_file_completion_commit)
            .add_observer(on_file_fold_toggle)
            .add_observer(on_file_view_mode_set)
            .add_observer(on_explorer_tree_toggle)
            .add_observer(on_explorer_tree_prefetch)
            .add_observer(on_explorer_tree_refresh)
            .add_observer(on_explorer_reveal_current)
            .add_observer(on_explorer_create)
            .add_observer(on_explorer_rename)
            .add_observer(on_explorer_delete)
            .add_observer(on_explorer_panel_set_visible)
            .add_observer(on_explorer_panel_width)
            .add_observer(on_explorer_close_editor)
            .add_observer(on_explorer_goto);
    }
}

#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, KeymapKindExt, Mods};

    #[test]
    fn file_view_mode_is_shared_across_editors() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SharedFileViewMode>()
            .add_observer(on_file_view_mode_set);
        let first = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/a.rs"),
            })
            .id();
        let second = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/b.rs"),
            })
            .id();

        app.world_mut().trigger(BinReceive {
            webview: first,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        assert_eq!(
            app.world().resource::<SharedFileViewMode>().0,
            FileViewMode::Diff
        );
        assert!(app.world().get::<FileView>(second).is_some());
    }

    #[test]
    fn non_editor_cannot_change_file_view_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SharedFileViewMode>()
            .add_observer(on_file_view_mode_set);
        let other = app.world_mut().spawn_empty().id();

        app.world_mut().trigger(BinReceive {
            webview: other,
            payload: FileViewModeSet {
                mode: FileViewMode::Diff,
            },
        });

        assert_eq!(
            app.world().resource::<SharedFileViewMode>().0,
            FileViewMode::Note
        );
    }

    #[test]
    fn file_view_mode_defaults_to_note() {
        assert_eq!(SharedFileViewMode::default().0, FileViewMode::Note);
    }

    #[test]
    fn parse_goto_fragment_line_and_select() {
        let g = parse_goto_fragment("file:///a/b.rs#L10").unwrap();
        assert_eq!((g.line, g.utf16_col, g.select_end_col), (9, 0, None));
        let g = parse_goto_fragment("file:///a/b.rs#L10:5-12").unwrap();
        assert_eq!((g.line, g.utf16_col, g.select_end_col), (9, 5, Some(12)));
        assert!(parse_goto_fragment("file:///a/b.rs").is_none());
        assert!(parse_goto_fragment("file:///a/b.rs#x").is_none());
    }

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
            .add_message::<vmux_core::event::RecordVisitRequest>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_file_page_open);
        app
    }

    #[test]
    fn file_open_records_history_visit() {
        use bevy::ecs::message::Messages;
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "file:///etc/hostname#L3".to_string(),
            request_id: None,
        });
        app.update();
        let msgs = app
            .world()
            .resource::<Messages<vmux_core::event::RecordVisitRequest>>();
        let mut cursor = msgs.get_cursor();
        let recorded: Vec<_> = cursor.read(msgs).collect();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].url, "file:///etc/hostname");
        assert_eq!(recorded[0].title, "hostname");
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
                    top_row: 0,
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

#[cfg(test)]
mod fold_window_tests {
    use crate::fold::{FoldState, indent_regions};
    use ropey::Rope;

    #[test]
    fn collapsed_region_hidden_from_window() {
        let r = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
        let mut folds = FoldState::default();
        folds.set_regions(indent_regions(&r));
        folds.close(0);
        let view = folds.view(r.len_lines() as u32);
        let visible = view.lines_for_window(0, view.visible_count());
        assert!(visible.contains(&0));
        assert!(!visible.contains(&1) && !visible.contains(&2));
        assert!(visible.contains(&3));
    }
}

#[cfg(test)]
mod explorer_tests {
    use super::*;
    use std::fs;

    fn git_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("README.md"), "# hi\n").unwrap();
        fs::write(tmp.path().join("src").join("lib.rs"), "fn main(){}\n").unwrap();
        tmp
    }

    fn toggle(app: &mut App, e: Entity, path: &Path) {
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerTreeToggle {
                path: path.to_string_lossy().to_string(),
            },
        });
    }

    fn wait_for_children(app: &mut App, e: Entity, path: &Path) {
        for _ in 0..1000 {
            app.update();
            if app
                .world()
                .get::<ExplorerState>(e)
                .is_some_and(|st| st.children.contains_key(path))
            {
                return;
            }
            std::thread::yield_now();
        }
        panic!("directory load did not finish: {}", path.display());
    }

    #[test]
    fn init_builds_root_listing_and_marks_dirty() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads));
        let e = app
            .world_mut()
            .spawn((FileView { path: file }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, e, tmp.path());
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(st.root.as_path(), tmp.path());
        assert!(st.expanded.contains(&tmp.path().to_path_buf()));
        assert!(
            st.children
                .get(tmp.path())
                .unwrap()
                .iter()
                .any(|x| x.name == "src")
        );
        assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
    }

    #[test]
    fn toggle_expands_then_collapses_subdir() {
        let tmp = git_repo();
        let file = tmp.path().join("README.md");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
            .add_observer(on_explorer_tree_toggle);
        let e = app
            .world_mut()
            .spawn((FileView { path: file }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, e, tmp.path());
        let src = tmp.path().join("src");
        toggle(&mut app, e, &src);
        wait_for_children(&mut app, e, &src);
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert!(st.expanded.contains(&src));
        assert!(
            st.children
                .get(&src)
                .unwrap()
                .iter()
                .any(|x| x.name == "lib.rs")
        );
        toggle(&mut app, e, &src);
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert!(!st.expanded.contains(&src));
    }

    #[test]
    fn reveal_current_expands_ancestors_and_focuses_file() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
            .add_observer(on_explorer_reveal_current);
        let e = app
            .world_mut()
            .spawn((FileView { path: file.clone() }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, e, tmp.path());
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerRevealCurrent,
        });
        let src = tmp.path().join("src");
        wait_for_children(&mut app, e, &src);
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert!(st.expanded.contains(tmp.path()));
        assert!(st.expanded.contains(&src));
        assert_eq!(st.focus_path.as_deref(), Some(file.as_path()));
    }

    #[test]
    fn repeated_reveal_skips_unchanged_tree_rebuild() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
            .add_observer(on_explorer_reveal_current);
        let e = app
            .world_mut()
            .spawn((FileView { path: file }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, e, tmp.path());
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerRevealCurrent,
        });
        wait_for_children(&mut app, e, &tmp.path().join("src"));
        app.world_mut().entity_mut(e).remove::<ExplorerTreeDirty>();
        app.world_mut()
            .get_mut::<ExplorerState>(e)
            .unwrap()
            .focus_path = None;
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerRevealCurrent,
        });
        assert!(app.world().get::<ExplorerTreeDirty>(e).is_none());
        assert!(
            app.world()
                .get::<ExplorerState>(e)
                .unwrap()
                .focus_path
                .is_none()
        );
    }

    #[test]
    fn panel_visibility_is_idempotent() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ExplorerChrome {
                visible: true,
                width: 240,
                client_id: 0,
                request_id: 0,
            })
            .add_observer(on_explorer_panel_set_visible);
        let e = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/x"),
            })
            .id();
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerPanelSetVisible {
                visible: false,
                client_id: 7,
                request_id: 1,
            },
        });
        assert!(!app.world().resource::<ExplorerChrome>().visible);
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerPanelSetVisible {
                visible: false,
                client_id: 7,
                request_id: 2,
            },
        });
        let chrome = app.world().resource::<ExplorerChrome>();
        assert!(!chrome.visible);
        assert_eq!(chrome.client_id, 7);
        assert_eq!(chrome.request_id, 2);
    }

    #[test]
    fn panel_open_reveals_current_file() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ExplorerChrome {
                visible: false,
                width: 240,
                client_id: 0,
                request_id: 0,
            })
            .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
            .add_observer(on_explorer_panel_set_visible);
        let e = app
            .world_mut()
            .spawn((FileView { path: file.clone() }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, e, tmp.path());
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerPanelSetVisible {
                visible: true,
                client_id: 9,
                request_id: 1,
            },
        });
        wait_for_children(&mut app, e, &tmp.path().join("src"));
        assert!(app.world().resource::<ExplorerChrome>().visible);
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(st.focus_path.as_deref(), Some(file.as_path()));
    }

    #[test]
    fn panel_width_clamps() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ExplorerChrome {
                visible: true,
                width: 240,
                client_id: 0,
                request_id: 0,
            })
            .add_observer(on_explorer_panel_width);
        let e = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/x"),
            })
            .id();
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerPanelWidth { px: 9000 },
        });
        assert_eq!(app.world().resource::<ExplorerChrome>().width, 600);
    }

    #[test]
    fn open_editors_track_on_navigate_and_close() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_open_editors)
            .add_observer(on_explorer_close_editor);
        let a = PathBuf::from("/proj/a.rs");
        let b = PathBuf::from("/proj/b.rs");
        let e = app
            .world_mut()
            .spawn((FileView { path: a.clone() }, ExplorerState::default()))
            .id();
        app.update();
        app.world_mut().get_mut::<FileView>(e).unwrap().path = b.clone();
        app.update();
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(st.open_editors, vec![a.clone(), b.clone()]);
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerCloseEditor {
                path: a.to_string_lossy().to_string(),
            },
        });
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(st.open_editors, vec![b]);
    }

    #[test]
    fn explorer_goto_writes_lsp_goto_message() {
        use crate::lsp::manager::LspGoto;
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LspGoto>()
            .add_observer(on_explorer_goto);
        let e = app
            .world_mut()
            .spawn(FileView {
                path: PathBuf::from("/x.rs"),
            })
            .id();
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerGoto {
                path: "/x.rs".to_string(),
                line: 12,
            },
        });
        let mut msgs = app.world_mut().resource_mut::<Messages<LspGoto>>();
        let got: Vec<_> = msgs.drain().collect();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].line, 12);
        assert_eq!(got[0].path, PathBuf::from("/x.rs"));
    }
}
