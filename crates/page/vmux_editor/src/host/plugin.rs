use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_command::ScopedKeys;
use vmux_core::PageMetadata;
use vmux_core::event::*;
use vmux_core::input::KeyStroke;
use vmux_core::page_open::{PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};
use vmux_layout::Browser;

use crate::dir::{list_dir, parent_listing, project_root};
use crate::edit::highlight_cache::HighlightCache;
use crate::edit::{EditCommand, EditCore, Motion, Selection};
use crate::explorer_model::flatten_tree;
use crate::keymap::{KeyInput, Keymap, KeymapKindExt, Mods};
use crate::lsp::workspace_edit::WorkspaceEditPlan;
use crate::preview;
use crate::wrap::WrapView;
use vmux_core::scroll::{clamp_top_line, rows_from_viewport, window_range};
use vmux_flex::prelude::*;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        let (tx, rx) = mpsc::channel();
        let proxy = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let wake = res
                .as_ref()
                .is_ok_and(|event| !matches!(event.kind, notify::EventKind::Access(_)));
            let _ = tx.send(res);
            if wake && let Some(proxy) = proxy.as_ref() {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
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
                default_visible: false,
                width: vmux_setting::EXPLORER_DEFAULT_WIDTH,
            })
            .register_type::<StackExplorerVisibility>()
            .init_resource::<ExplorerChromeSynced>()
            .init_resource::<PendingGlobalSearch>()
            .init_resource::<SharedFileViewMode>()
            .add_message::<vmux_core::event::RecordVisitRequest>()
            .add_message::<vmux_setting::SettingsWriteRequest>()
            .add_plugins(crate::contract::EditorContractPlugin)
            .add_plugins(crate::lsp::LspPlugin)
            .add_plugins(crate::app_key::FileKeyPlugin)
            .add_plugins(BinEventEmitterPlugin::<(
                FileResizeEvent,
                FileScrollEvent,
                FilePreviewRequest,
                FileOpenEvent,
                FileTextInput,
                FilePointerEvent,
                FileHoverRequest,
                FileDefinitionRequest,
                FileReferencesRequest,
                FileFoldToggle,
                FileRenameRequest,
                FileEditorAction,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                FileCodeActionPick,
                FileCompletionRequest,
                FileGotoRequest,
                FileCompletionCommit,
                FileOpenExternalRequest,
                FileVideoRect,
                FileViewModeSet,
                FileKeymapSet,
                KnowledgeLinkOpen,
                FilePropertyEdit,
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
                ExplorerSearchOpen,
            )>::default())
            .add_systems(
                Update,
                handle_file_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                (
                    (
                        reconcile_file_watches,
                        drain_file_changes,
                        reload_changed_files,
                        load_file_buffers,
                    )
                        .chain(),
                    send_initial_meta.after(load_file_buffers),
                    send_initial_text_meta.after(load_file_buffers),
                    send_initial_dir.after(load_file_buffers),
                    sync_media_allowlist.after(load_file_buffers),
                    send_initial_media
                        .after(load_file_buffers)
                        .after(sync_media_allowlist),
                    (detach_video_overlays, attach_video_overlays).chain(),
                    send_file_theme,
                    apply_file_view_mode_requests.before(send_file_view_mode),
                    send_file_view_mode,
                    send_file_keymap,
                    sync_editor_wrap_settings.after(load_file_buffers),
                    rehighlight_on_color_scheme,
                    drain_thumb_tasks,
                    flush_lsp_changes,
                    apply_goto,
                    apply_pending_goto,
                    reapply_keymap_on_change,
                    apply_lsp_folds,
                    persist_folds,
                ),
            )
            .add_systems(
                Update,
                apply_lsp_workspace_edit
                    .in_set(crate::lsp::server_request::ServerRequestSet::Answer),
            )
            .add_systems(
                Update,
                (
                    mark_notes_on_knowledge_change,
                    send_note.after(mark_notes_on_knowledge_change),
                ),
            )
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
                    emit_outline_markdown,
                    clear_outline_on_file_change,
                    apply_global_search_requests,
                    emit_global_search.after(apply_global_search_requests),
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
            .add_observer(on_file_rename_request)
            .add_observer(on_file_editor_action)
            .add_observer(on_file_code_action_pick)
            .add_observer(on_file_completion_request)
            .add_observer(on_file_goto_request)
            .add_observer(on_file_completion_commit)
            .add_observer(on_knowledge_link_open)
            .add_observer(on_file_property_edit)
            .add_observer(on_file_fold_toggle)
            .add_observer(on_file_view_mode_set)
            .add_observer(on_file_keymap_set)
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
            .add_observer(on_explorer_goto)
            .add_observer(on_explorer_search_open);
    }
}

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
    pub wrap_columns: u16,
    pub word_wrap: vmux_core::editor::WordWrap,
    pub word_wrap_column: u16,
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
    pub folds: crate::fold::FoldState,
    parsed_note: Option<crate::markdown::ParsedNote>,
    wrap_generation: u64,
    wrap_cache: Option<CachedWrapView>,
}

impl EditState {
    pub(crate) fn new(core: EditCore, hl: HighlightCache, folds: crate::fold::FoldState) -> Self {
        let parsed_note = crate::markdown::is_markdown_path(&core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&core.buffer.text()));
        Self {
            core,
            hl,
            folds,
            parsed_note,
            wrap_generation: 0,
            wrap_cache: None,
        }
    }

    fn refresh_parsed_note(&mut self) {
        self.parsed_note = crate::markdown::is_markdown_path(&self.core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&self.core.buffer.text()));
    }
}

/// Editor state for files this view has already shown.
///
/// Switching files replaces a view's contents in place, and discarding the state meant the undo
/// tree, cursor, marks, jump list and search all started over on the way back. Holding them here
/// is what makes the open-editors list behave like a tab strip rather than a history of reloads.
#[derive(Component, Default)]
struct ParkedEdits {
    by_path: HashMap<PathBuf, ParkedEdit>,
    recent: Vec<PathBuf>,
}

struct ParkedEdit {
    edit: EditState,
    diff: vmux_git::GitDiffSource,
    modified: Option<std::time::SystemTime>,
}

impl ParkedEdits {
    /// `EditCore` keeps a whole-rope snapshot per undo group, so this is bounded by count rather
    /// than left to grow with every file a session visits.
    const CAPACITY: usize = 8;

    /// Move the view's current editor state off `entity` and hold it under `path`.
    fn park(entity: &mut EntityWorldMut, path: PathBuf) {
        if !entity.contains::<EditState>() || !entity.contains::<vmux_git::GitDiffSource>() {
            return;
        }
        let Some(edit) = entity.take::<EditState>() else {
            return;
        };
        let Some(diff) = entity.take::<vmux_git::GitDiffSource>() else {
            return;
        };
        let parked = ParkedEdit {
            edit,
            diff,
            modified: Self::modified_at(&path),
        };
        let mut edits = entity.take::<ParkedEdits>().unwrap_or_default();
        edits.insert(path, parked);
        entity.insert(edits);
    }

    fn insert(&mut self, path: PathBuf, edit: ParkedEdit) {
        self.recent.retain(|p| p != &path);
        self.recent.push(path.clone());
        self.by_path.insert(path, edit);
        while self.recent.len() > Self::CAPACITY {
            let evicted = self.recent.remove(0);
            self.by_path.remove(&evicted);
        }
    }

    /// Take back the state for `path`, unless the file moved on underneath it.
    ///
    /// Unsaved edits win over a changed file: dropping them would lose work, and the external
    /// change path already refuses to overwrite a dirty buffer and warns instead.
    fn resume(&mut self, path: &Path) -> Option<ParkedEdit> {
        let parked = self.by_path.remove(path)?;
        self.recent.retain(|p| p != path);
        if parked.edit.core.dirty || parked.modified == Self::modified_at(path) {
            return Some(parked);
        }
        None
    }

    /// Whether a file this view is no longer showing has unsaved changes.
    ///
    /// Before parking, only the visible file's dirtiness was knowable at all — every other entry
    /// in the open-editors list had nowhere to read it from.
    fn is_dirty(&self, path: &Path) -> bool {
        self.by_path
            .get(path)
            .is_some_and(|parked| parked.edit.core.dirty)
    }

    fn modified_at(path: &Path) -> Option<std::time::SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }
}

struct CachedWrapView {
    generation: u64,
    mode: vmux_core::editor::WordWrap,
    viewport_columns: u16,
    word_wrap_column: u16,
    view: WrapView,
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

#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_editor::plugin"]
pub struct StackExplorerVisibility {
    pub visible: bool,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StackExplorerRevision {
    client_id: u64,
    request_id: u64,
}

#[derive(Resource, Clone, Copy)]
struct ExplorerChrome {
    default_visible: bool,
    width: u32,
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

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileViewModeRequest(pub FileViewMode);

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchRequest {
    pub target_path: PathBuf,
    pub root: String,
    pub query: String,
    pub matches: Vec<ExplorerSearchMatch>,
}

#[derive(Component, Clone)]
struct GlobalSearchState(ExplorerSearchEvent);

#[derive(Component)]
struct GlobalSearchDirty;

#[derive(Resource, Default)]
struct PendingGlobalSearch(Vec<PendingGlobalSearchRequest>);

struct PendingGlobalSearchRequest {
    request: GlobalSearchRequest,
    retries_left: u8,
}

const GLOBAL_SEARCH_RETRY_LIMIT: u8 = 120;

#[derive(Component)]
struct FileViewModeSent;

#[derive(Component)]
struct FileKeymapSent;

#[derive(Component)]
struct NoteSent;

#[derive(Component, Clone, Copy)]
struct NoteRevealLine(u32);

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
type ReadyUnsentKeymap = (
    With<FileView>,
    Without<FileKeymapSent>,
    With<vmux_core::page::PageReady>,
);
type ReadySentKeymap = (
    With<FileView>,
    With<FileKeymapSent>,
    With<vmux_core::page::PageReady>,
);
type ReadyUnsentNote = (
    Without<NoteSent>,
    With<vmux_core::page::PageReady>,
    With<FileInitialMetaSent>,
);
type TreeDirtyReady = (With<ExplorerTreeDirty>, With<vmux_core::page::PageReady>);
type OpenEditorsDirtyReady = (With<OpenEditorsDirty>, With<vmux_core::page::PageReady>);
type OutlineDirtyReady = (With<OutlineDirty>, With<vmux_core::page::PageReady>);
type GlobalSearchDirtyReady = (
    With<GlobalSearchState>,
    With<GlobalSearchDirty>,
    With<vmux_core::page::PageReady>,
);
type ChromeUnsentReady = (
    With<FileView>,
    Without<ExplorerChromeSent>,
    With<vmux_core::page::PageReady>,
);

/// The path a `file://` url names.
///
/// Read off the raw string rather than through `Url`, because everything after the scheme is the
/// path here and the parser does not treat it that way. `file://Users/me/a.rs` — two slashes
/// instead of three, the usual typo — parses `Users` as a *host*, so reading `.path()` silently
/// opens `/me/a.rs`: a different file, or more often a missing one blamed on a path nobody
/// typed. A host is also case-folded, so `Users` cannot be put back afterwards. `localhost` is
/// the one host that really does mean this machine, and it is the one that gets dropped.
fn path_from_files_url(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("file://")
        .or_else(|| url.strip_prefix("FILE://"))?;
    // Only where the whole host is `localhost`. Without the boundary check the two-slash form
    // turns `file://localhost-notes/a.rs` into `/-notes/a.rs`, which is the very failure this
    // function exists to stop.
    let rest = match rest.strip_prefix("localhost") {
        Some(after) if after.is_empty() || after.starts_with('/') => after,
        _ => rest,
    };
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    if rest.is_empty() {
        return None;
    }
    let raw = match rest.starts_with('/') {
        true => rest.to_string(),
        false => format!("/{rest}"),
    };
    let decoded = percent_encoding::percent_decode_str(&raw)
        .decode_utf8()
        .ok()?;
    let path = PathBuf::from(decoded.as_ref());
    path.is_absolute().then_some(path)
}

fn new_file_view_bundle(url: &str, path: PathBuf) -> impl Bundle {
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
                wrap_columns: 0,
                word_wrap: vmux_core::editor::WordWrap::default(),
                word_wrap_column: 80,
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
            vmux_core::host::page::HostsPage,
        ),
        (
            WebviewSize(Vec2::new(1280.0, 720.0)),
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
            Visibility::Visible,
        ),
    )
}

pub fn restore_file_view_bundle(url: &str) -> Option<impl Bundle> {
    let path = path_from_files_url(url)?;
    Some(new_file_view_bundle(url, path))
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
            .spawn((new_file_view_bundle(&clean_url, path), ChildOf(task.stack)))
            .id();
        if let Some(pg) = pending {
            commands.entity(view).insert(pg);
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn settings_mappings(
    settings: &Option<Res<vmux_setting::AppSettings>>,
) -> (Vec<vmux_core::editor::KeyMapping>, String) {
    settings
        .as_ref()
        .map(|s| (s.editor.mappings.clone(), s.editor.leader.clone()))
        .unwrap_or_else(|| (Vec::new(), " ".to_string()))
}

fn settings_keymap(settings: &Option<Res<vmux_setting::AppSettings>>) -> vmux_core::KeymapKind {
    settings
        .as_ref()
        .map(|s| s.editor.keymap)
        .unwrap_or_default()
}

fn load_file_buffers(
    mut q: Query<(Entity, &FileView, Option<&mut ParkedEdits>), UnloadedFileView>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    store: Option<NonSend<crate::fold_store::FoldStore>>,
    mut commands: Commands,
) {
    for (entity, fv, mut parked) in &mut q {
        if fv.path.is_dir() {
            let entries = list_dir(&fv.path);
            commands
                .entity(entity)
                .remove::<MissingFileView>()
                .insert(FileDir { entries });
            continue;
        }
        let path_str = fv.path.to_string_lossy();
        if let Some(kind) = vmux_core::media::media_kind(&path_str) {
            let mime = vmux_core::media::media_mime(&path_str)
                .unwrap_or("application/octet-stream")
                .to_string();
            commands
                .entity(entity)
                .remove::<MissingFileView>()
                .insert(FileMedia { kind, mime });
            continue;
        }
        let size = std::fs::metadata(&fv.path).map(|m| m.len());
        // Past this the file still opens, it just opens plainly. Highlighting keeps a parser
        // state per line, so that is the cost that has to come off, not the text.
        let heavy = size
            .as_ref()
            .is_ok_and(|len| *len > crate::highlight::HIGHLIGHT_MAX_BYTES);
        match size {
            Ok(len) if len > crate::highlight::FILE_VIEW_MAX_BYTES => {
                commands
                    .entity(entity)
                    .remove::<MissingFileView>()
                    .insert(FileBuffer::error(format!(
                        "__error__:file too large ({len} bytes, max {})",
                        crate::highlight::FILE_VIEW_MAX_BYTES
                    )));
                continue;
            }
            Err(e) => {
                let mut entity_commands = commands.entity(entity);
                entity_commands.insert(FileBuffer::error(format!(
                    "__error__:cannot open {}: {e}",
                    fv.path.display()
                )));
                if e.kind() == std::io::ErrorKind::NotFound {
                    entity_commands.insert(MissingFileView);
                } else {
                    entity_commands.remove::<MissingFileView>();
                }
                continue;
            }
            _ => {}
        }
        let kind = settings_keymap(&settings);
        let (maps, leader) = settings_mappings(&settings);
        let markdown = crate::markdown::is_markdown_path(&fv.path);
        if let Some(parked) = parked.as_mut()
            && let Some(resumed) = parked.resume(&fv.path)
        {
            let mut entity_commands = commands.entity(entity);
            entity_commands
                .insert((
                    resumed.edit,
                    EditorKeymap(kind.make(&maps, &leader)),
                    resumed.diff,
                ))
                .remove::<MissingFileView>();
            if markdown {
                entity_commands.remove::<NoteSent>().insert(OutlineDirty);
            }
            continue;
        }
        let text = match std::fs::read(&fv.path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(t) => t,
                Err(_) => {
                    commands
                        .entity(entity)
                        .remove::<MissingFileView>()
                        .insert(FileBuffer::error(format!(
                            "__error__:not a UTF-8 text file: {}",
                            fv.path.display()
                        )));
                    continue;
                }
            },
            Err(e) => {
                let mut entity_commands = commands.entity(entity);
                entity_commands.insert(FileBuffer::error(format!(
                    "__error__:cannot read {}: {e}",
                    fv.path.display()
                )));
                if e.kind() == std::io::ErrorKind::NotFound {
                    entity_commands.insert(MissingFileView);
                } else {
                    entity_commands.remove::<MissingFileView>();
                }
                continue;
            }
        };
        let hl = match heavy {
            true => HighlightCache::plain(&fv.path),
            false => HighlightCache::new(&fv.path),
        };
        let mut core = EditCore::new(
            fv.path.clone(),
            hl.language.clone(),
            &text,
            kind.initial_mode(),
        );
        let mut folds = crate::fold::FoldState::default();
        if !heavy {
            folds.set_regions(crate::fold::indent_regions(&core.buffer.rope));
            if let Some(store) = &store {
                folds.collapsed.extend(store.get(&fv.path));
                folds.reconcile();
            }
        }
        core.fold_view = folds.view(core.buffer.len_lines() as u32);
        let mut entity_commands = commands.entity(entity);
        entity_commands
            .insert((
                EditState::new(core, hl, folds),
                EditorKeymap(kind.make(&maps, &leader)),
                vmux_git::GitDiffSource {
                    content: text,
                    dirty: false,
                },
            ))
            .remove::<MissingFileView>();
        if markdown {
            entity_commands.remove::<NoteSent>().insert(OutlineDirty);
        }
    }
}

#[derive(PartialEq, Eq)]
struct KeymapConfig {
    kind: vmux_core::KeymapKind,
    maps: Vec<vmux_core::editor::KeyMapping>,
    leader: String,
}

fn reapply_keymap_on_change(
    settings: Option<Res<vmux_setting::AppSettings>>,
    mut last: Local<Option<KeymapConfig>>,
    mut q: Query<(
        Entity,
        &mut EditState,
        &mut EditorKeymap,
        Option<&FileViewport>,
    )>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let (maps, leader) = settings_mappings(&settings);
    let next = KeymapConfig {
        kind: settings_keymap(&settings),
        maps,
        leader,
    };
    if last.as_ref() == Some(&next) {
        return;
    }
    let first = last.is_none();
    let kind_changed = last.as_ref().is_none_or(|prev| prev.kind != next.kind);
    let kind = next.kind;
    *last = Some(next);
    if first {
        return;
    }
    let Some(config) = last.as_ref() else {
        return;
    };
    for (entity, mut edit, mut keymap, viewport) in &mut q {
        keymap.0 = kind.make(&config.maps, &config.leader);
        if kind_changed {
            edit.core.mode = kind.initial_mode();
        }
        if let (Some(viewport), Some(browsers)) = (viewport, browsers.as_deref()) {
            emit_cursor(
                entity,
                &mut edit,
                keymap.0.as_ref(),
                viewport,
                browsers,
                &mut commands,
            );
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
    q: Query<(Entity, &FileBuffer), ReadyUnsentMeta>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, buf) in &q {
        if !browsers.can_emit_to(&entity) {
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
        if !browsers.can_emit_to(&entity) {
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
            &mut edit,
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
        if !browsers.can_emit_to(&entity) {
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
        if !browsers.can_emit_to(&entity) {
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
            if !browsers.can_emit_to(&entity) {
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

fn send_file_keymap(
    settings: Option<Res<vmux_setting::AppSettings>>,
    pending: Query<Entity, ReadyUnsentKeymap>,
    sent: Query<Entity, ReadySentKeymap>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let event = FileKeymapEvent {
        keymap: settings_keymap(&settings),
    };
    for entity in &pending {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_KEYMAP_EVENT,
            &event,
        ));
        commands.entity(entity).insert(FileKeymapSent);
    }
    if settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed())
    {
        for entity in &sent {
            if !browsers.can_emit_to(&entity) {
                continue;
            }
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_KEYMAP_EVENT,
                &event,
            ));
        }
    }
}

fn apply_file_view_mode_requests(
    mut reader: MessageReader<FileViewModeRequest>,
    mut mode: ResMut<SharedFileViewMode>,
) {
    if let Some(request) = reader.read().last() {
        mode.0 = request.0;
    }
}

fn active_note_block(blocks: &[NoteBlock], line: u32) -> Option<u32> {
    blocks
        .iter()
        .position(|block| block.start_line <= line && line < block.end_line)
        .or_else(|| blocks.iter().rposition(|block| block.start_line <= line))
        .or_else(|| (!blocks.is_empty()).then_some(0))
        .map(|index| index as u32)
}

fn send_note(
    mode: Res<SharedFileViewMode>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    q: Query<(Entity, &FileView, &EditState, Option<&NoteRevealLine>), ReadyUnsentNote>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    if mode.0 != FileViewMode::Note {
        return;
    }
    for (entity, file, edit, reveal) in &q {
        if !crate::markdown::is_markdown_path(&file.path) {
            commands.entity(entity).insert(NoteSent);
            continue;
        }
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let Some(mut note) = edit.parsed_note.clone() else {
            commands.entity(entity).insert(NoteSent);
            continue;
        };
        let references = index
            .as_deref()
            .filter(|index| index.loaded() && file.path.starts_with(index.root()))
            .map(|index| {
                index.resolve_blocks(&file.path, &mut note.blocks);
                let mut references = index.backlinks(&file.path);
                references.extend(index.unlinked_mentions(&file.path, 32));
                references
                    .into_iter()
                    .map(|reference| vmux_core::knowledge::KnowledgeReference {
                        title: reference.title,
                        path: reference.path.to_string_lossy().into_owned(),
                        line: reference.line,
                        preview: reference.preview,
                        unlinked: reference.unlinked,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = active_note_block(&note.blocks, edit.core.cursor_pos().line);
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            FILE_NOTE_EVENT,
            &FileNoteEvent {
                title: note.title,
                properties: note.properties,
                blocks: note.blocks,
                active,
                references,
                reveal_line: reveal.map(|line| line.0),
            },
        ));
        commands
            .entity(entity)
            .insert(NoteSent)
            .remove::<NoteRevealLine>();
    }
}

fn mark_notes_on_knowledge_change(
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    q: Query<Entity, With<FileView>>,
    mut commands: Commands,
) {
    if index.is_none_or(|index| !index.is_changed()) {
        return;
    }
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
        if !browsers.can_emit_to(&entity) {
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

fn wrapped_view<'a>(edit: &'a mut EditState, vp: &FileViewport) -> &'a WrapView {
    let stale = edit.wrap_cache.as_ref().is_none_or(|cache| {
        cache.generation != edit.wrap_generation
            || cache.mode != vp.word_wrap
            || cache.viewport_columns != vp.wrap_columns
            || cache.word_wrap_column != vp.word_wrap_column
    });
    if stale {
        let total = edit.core.buffer.len_lines() as u32;
        let folds = edit.folds.view(total);
        edit.wrap_cache = Some(CachedWrapView {
            generation: edit.wrap_generation,
            mode: vp.word_wrap,
            viewport_columns: vp.wrap_columns,
            word_wrap_column: vp.word_wrap_column,
            view: WrapView::new(
                &edit.core.buffer.rope,
                &folds,
                vp.word_wrap,
                vp.wrap_columns,
                vp.word_wrap_column,
            ),
        });
    }
    &edit.wrap_cache.as_ref().expect("wrap cache").view
}

/// Redraw the visible window for a caller outside this module that changed how the text should
/// look rather than what it says.
pub(crate) fn repaint_window(
    entity: Entity,
    edit: &mut EditState,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    emit_window(entity, edit, vp, browsers, commands);
}

fn emit_window(
    entity: Entity,
    edit: &mut EditState,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.can_emit_to(&entity) {
        return;
    }
    let total = edit.core.buffer.len_lines() as u32;
    let (visible, wrap_columns) = {
        let wrap = wrapped_view(edit, vp);
        (wrap.total_rows(), wrap.columns())
    };
    let (vis_first, vis_end) = window_range(visible, vp.top_row, vp.rows);
    let overscan = vmux_core::scroll::overscan_for(
        vp.rows,
        vmux_core::scroll::EDITOR_OVERSCAN_K,
        vmux_core::scroll::OVERSCAN_FLOOR,
        vmux_core::scroll::OVERSCAN_CAP,
    );
    let first_row = vis_first.saturating_sub(overscan);
    let end_row = (vis_end + overscan).min(visible);
    let layouts = wrapped_view(edit, vp).window(first_row, end_row);
    let first_row = layouts.first().map_or(first_row, |line| line.row);
    let mut lines = Vec::with_capacity(layouts.len());
    for layout in &layouts {
        let ln = layout.line_no;
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
            wrap_columns,
            layouts,
            lines,
        },
    ));
}

fn emit_cursor(
    entity: Entity,
    edit: &mut EditState,
    keymap: &dyn Keymap,
    vp: &FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if !browsers.can_emit_to(&entity) {
        return;
    }
    let total = edit.core.buffer.len_lines() as u32;
    let view = edit.folds.view(total);
    let source_primary = edit.core.cursor_pos();
    let mut primary = source_primary;
    let raw_selections = edit
        .core
        .sel_spans(0, total as u16)
        .into_iter()
        .filter(|selection| !view.is_hidden(selection.line))
        .collect::<Vec<_>>();
    let raw_search = edit
        .core
        .search_spans(0, total as u16)
        .into_iter()
        .filter(|span| !view.is_hidden(span.line))
        .collect::<Vec<_>>();
    let raw_carets: Vec<_> = edit
        .core
        .cursor_positions()
        .into_iter()
        .filter(|caret| !view.is_hidden(caret.line))
        .collect();
    let wrap = wrapped_view(edit, vp);
    (primary.row, primary.col) = wrap.position(primary.line, primary.col);
    let mut carets = raw_carets;
    for caret in &mut carets {
        (caret.row, caret.col) = wrap.position(caret.line, caret.col);
    }
    let selections = wrap.selections(raw_selections.iter().copied());
    let search = wrap.selections(raw_search.iter().copied());
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_CURSOR_EVENT,
        &FileCursorEvent {
            mode: keymap.mode(),
            mode_label: keymap.mode_label(),
            primary,
            carets,
            selections,
            source_primary,
            source_selections: raw_selections,
            command_line: keymap.command_line().unwrap_or_default(),
            search,
        },
    ));
}

fn wrapped_autoscroll(edit: &mut EditState, vp: &FileViewport) -> Option<u32> {
    if vp.rows == 0 {
        return None;
    }
    let cursor = edit.core.cursor_pos();
    let row = wrapped_view(edit, vp).position(cursor.line, cursor.col).0;
    if row < vp.top_row {
        Some(row)
    } else if row >= vp.top_row + vp.rows as u32 {
        Some(row + 1 - vp.rows as u32)
    } else {
        None
    }
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

fn sync_editor_wrap_settings(
    settings: Res<vmux_setting::AppSettings>,
    mut views: Query<(
        Entity,
        &mut FileViewport,
        Option<&mut EditState>,
        Option<&EditorKeymap>,
    )>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut viewport, edit, keymap) in &mut views {
        if viewport.word_wrap == settings.editor.word_wrap
            && viewport.word_wrap_column == settings.editor.word_wrap_column
        {
            continue;
        }
        viewport.word_wrap = settings.editor.word_wrap;
        viewport.word_wrap_column = settings.editor.word_wrap_column.max(1);
        viewport.top_row = 0;
        if let Some(mut edit) = edit {
            if let Some(top) = wrapped_autoscroll(&mut edit, &viewport) {
                viewport.top_row = top;
            }
            emit_window(entity, &mut edit, &viewport, &browsers, &mut commands);
            if let Some(keymap) = keymap {
                emit_cursor(
                    entity,
                    &mut edit,
                    keymap.0.as_ref(),
                    &viewport,
                    &browsers,
                    &mut commands,
                );
            }
        }
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
        .remove::<FileKeymapSent>()
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
    files: Query<(&FileView, Option<&EditState>)>,
    mut mode: ResMut<SharedFileViewMode>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    if views.contains(entity) {
        mode.0 = trigger.event().payload.mode;
        if mode.0 == FileViewMode::Note
            && files
                .get(entity)
                .is_ok_and(|(file, _)| crate::markdown::is_markdown_path(&file.path))
        {
            let reveal_line = files
                .get(entity)
                .ok()
                .and_then(|(_, edit)| edit.map(|edit| edit.core.cursor_pos().line));
            let mut entity_commands = commands.entity(entity);
            entity_commands.remove::<NoteSent>();
            if let Some(line) = reveal_line {
                entity_commands.insert(NoteRevealLine(line));
            }
        }
    }
}

fn on_file_keymap_set(
    trigger: On<BinReceive<FileKeymapSet>>,
    views: Query<(), With<FileView>>,
    mut settings: ResMut<vmux_setting::AppSettings>,
    mut writes: MessageWriter<vmux_setting::SettingsWriteRequest>,
) {
    if !views.contains(trigger.event().webview) {
        return;
    }
    let keymap = trigger.event().payload.keymap;
    if settings.editor.keymap == keymap {
        return;
    }
    match vmux_setting::apply_settings_update(
        settings.as_mut(),
        "editor.keymap",
        serde_json::to_value(keymap).unwrap_or_default(),
    ) {
        Ok(ron_bytes) => {
            writes.write(vmux_setting::SettingsWriteRequest { ron_bytes });
        }
        Err(error) => bevy::log::warn!("editor: keymap update rejected: {error}"),
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
    let rows = rows_from_viewport(evt.char_height, evt.viewport_height);
    if vp.rows == rows && vp.wrap_columns == evt.wrap_columns {
        return;
    }
    vp.rows = rows;
    vp.wrap_columns = evt.wrap_columns;
    if let Some(mut edit) = edit {
        edit.core.rows = vp.rows;
        edit.core.top_row = vp.top_row;
        let vpc = *vp;
        emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
        if let Some(keymap) = keymap {
            emit_cursor(
                entity,
                &mut edit,
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
    let visible = wrapped_view(&mut edit, &vp).total_rows();
    vp.top_row = clamp_top_line(evt.top_row, visible, vp.rows);
    let vpc = *vp;
    emit_window(entity, &mut edit, &vpc, &browsers, &mut commands);
    emit_cursor(
        entity,
        &mut edit,
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
        &mut edit,
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
            &mut edit,
            keymap.0.as_ref(),
            &vpc,
            &browsers,
            &mut commands,
        );
    }
}

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
        if !browsers.can_emit_to(&entity) {
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

fn needs_native_video(path: &Path) -> bool {
    vmux_core::media::is_proprietary_video(&path.to_string_lossy())
}

fn attach_video_overlays(q: Query<(Entity, &FileView, &FileMedia)>, browsers: NonSend<Browsers>) {
    for (entity, fv, media) in &q {
        if media.kind != vmux_core::media::MediaKind::Video || !needs_native_video(&fv.path) {
            continue;
        }
        // `has_browser`, as in `on_file_video_rect`: the overlay is CEF's own machinery rather
        // than a host event, and a natively hosted page answers `can_emit_to` without having any.
        if !browsers.has_browser(entity) {
            continue;
        }
        browsers.attach_media_overlay(&entity, &fv.path.to_string_lossy());
    }
}

fn on_file_video_rect(
    trigger: On<BinReceive<FileVideoRect>>,
    file_views: Query<(), With<FileView>>,
    browsers: NonSend<Browsers>,
) {
    let entity = trigger.event().webview;
    // `has_browser` rather than `can_emit_to`, unlike every other guard here: the overlay is
    // CEF's own machinery, not a host event, and a natively hosted page has none of it.
    if file_views.get(entity).is_err() || !browsers.has_browser(entity) {
        return;
    }
    let r = &trigger.event().payload;
    if !vmux_core::media::is_proprietary_video(&r.path) || r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    browsers.set_media_overlay(&entity, &r.path, (r.x, r.y, r.w, r.h));
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
    if !browsers.can_emit_to(&entity) {
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
                && browsers.can_emit_to(&webview)
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
    let previous = std::mem::replace(&mut fv.path, path);
    manager.close(&previous);
    let url = url::Url::from_file_path(&fv.path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| format!("file://{}", fv.path.to_string_lossy()));
    meta.title = fv
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| fv.path.to_string_lossy().to_string());
    meta.url = url;
    vp.top_row = top_line;
    commands.queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
        ParkedEdits::park(&mut entity, previous);
    });
    commands
        .entity(entity)
        .remove::<FileDir>()
        .remove::<FileBuffer>()
        .remove::<FileMedia>()
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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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

fn on_knowledge_link_open(
    trigger: On<BinReceive<KnowledgeLinkOpen>>,
    mut goto: MessageWriter<crate::lsp::manager::LspGoto>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let root = vmux_core::knowledge::KnowledgeVault::user().into_root();
    let requested = PathBuf::from(&request.path);
    let path = if request.create {
        let Ok(relative) = requested.strip_prefix(&root) else {
            return;
        };
        if requested.exists() {
            let Ok(canonical_root) = root.canonicalize() else {
                return;
            };
            let Ok(metadata) = std::fs::symlink_metadata(&requested) else {
                return;
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return;
            }
            let Ok(path) = requested.canonicalize() else {
                return;
            };
            if !path.starts_with(canonical_root) {
                return;
            }
            path
        } else {
            let relative = relative.to_string_lossy();
            match vmux_core::knowledge::KnowledgeVault::user().write_note(
                Some(&relative),
                &request.title,
                &format!("# {}", request.title),
            ) {
                Ok(path) => path,
                Err(error) => {
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(BinHostEmitEvent::from_rkyv(
                            entity,
                            FILE_ERROR_EVENT,
                            &FileErrorEvent { message: error },
                        ));
                    }
                    return;
                }
            }
        }
    } else {
        let Ok(root) = root.canonicalize() else {
            return;
        };
        if std::fs::symlink_metadata(&requested)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return;
        }
        let Ok(path) = requested.canonicalize() else {
            return;
        };
        if !path.starts_with(root) {
            return;
        }
        path
    };
    if let Some(line) = request.line {
        commands.entity(entity).insert(NoteRevealLine(line));
    }
    goto.write(crate::lsp::manager::LspGoto {
        entity,
        path,
        line: request.line.unwrap_or(0),
        utf16_col: 0,
    });
}

#[derive(Component)]
struct FileReloadRequested;

#[derive(Component)]
struct MissingFileView;

struct FileWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    dirs: HashSet<PathBuf>,
}

/// The path to compare two references to a file by, so a symlink and its target are one file.
///
/// Falls back to the path as written for anything that cannot be resolved, which is what makes it
/// usable on a file that has just been deleted or renamed out from under a view.
pub(crate) fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn watch_dir_for(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_dir() { path } else { path.parent()? };
    loop {
        if dir.is_dir() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn ensure_file_watch(watch: &mut FileWatch, dir: PathBuf) {
    if !watch.dirs.contains(&dir)
        && watch
            .watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .is_ok()
    {
        watch.dirs.insert(dir);
    }
}

fn reconcile_file_watches(
    views: Query<&FileView>,
    explorers: Query<&ExplorerState>,
    watch: Option<NonSendMut<FileWatch>>,
) {
    let Some(mut watch) = watch else {
        return;
    };
    for fv in &views {
        if let Some(dir) = watch_dir_for(&fv.path) {
            ensure_file_watch(&mut watch, dir);
        }
    }
    for st in &explorers {
        for dir in st.expanded.iter() {
            ensure_file_watch(&mut watch, dir.clone());
        }
    }
}

fn drain_file_changes(
    watch: Option<NonSend<FileWatch>>,
    self_writes: Option<NonSendMut<SelfWrites>>,
    views: Query<(Entity, &FileView, Has<MissingFileView>)>,
    mut explorers: Query<(Entity, &mut ExplorerState)>,
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
    for (entity, fv, missing) in &views {
        let cp = canon(&fv.path);
        let self_written = sw
            .as_ref()
            .map(|sw| sw.0.contains_key(&cp))
            .unwrap_or(false);
        let ancestor_changed = missing && changed.iter().any(|path| cp.starts_with(path));
        if (changed.contains(&cp) || ancestor_changed) && !self_written {
            commands.entity(entity).insert(FileReloadRequested);
        }
    }
    for (entity, mut st) in &mut explorers {
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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    for (entity, fv, edit) in &q {
        commands.entity(entity).remove::<FileReloadRequested>();
        let ready = browsers.can_emit_to(&entity);

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

/// The identifier the caret sits in, for pre-filling the rename box.
///
/// Empty when the caret is not in one, which is how the caller tells there is nothing to rename
/// without asking the server first.
fn word_at_col(line_text: &str, char_col: usize) -> String {
    let chars: Vec<char> = line_text.chars().collect();
    let start = word_start_col(line_text, char_col) as usize;
    let mut end = char_col.min(chars.len());
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }
    chars[start..end].iter().collect()
}

fn wiki_completion_context(edit: &EditState) -> Option<(u32, u32, String)> {
    if !crate::markdown::is_markdown_path(&edit.core.buffer.path) {
        return None;
    }
    let (line, _, col, text) = caret_lsp(edit);
    let chars = text.chars().collect::<Vec<_>>();
    let col = col.min(chars.len());
    let open = (0..col.saturating_sub(1))
        .rev()
        .find(|index| chars[*index] == '[' && chars[*index + 1] == '[')?;
    let fragment = chars[open + 2..col].iter().collect::<String>();
    if fragment.contains("]]") || fragment.contains('|') || fragment.contains('#') {
        return None;
    }
    Some((line, open as u32 + 2, fragment))
}

fn emit_wiki_completions(
    entity: Entity,
    edit: &EditState,
    index: &vmux_core::knowledge::KnowledgeIndex,
    browsers: &Browsers,
    commands: &mut Commands,
) -> bool {
    if !index.loaded() || !edit.core.buffer.path.starts_with(index.root()) {
        return false;
    }
    let Some((line, replace_from_col, prefix)) = wiki_completion_context(edit) else {
        return false;
    };
    if !browsers.can_emit_to(&entity) {
        return true;
    }
    let items = index
        .completions(&prefix, 32)
        .into_iter()
        .map(|(title, relative)| CompletionItem {
            label: title.clone(),
            insert_text: format!("{title}]]"),
            detail: relative,
            kind: "knowledge".to_string(),
        })
        .collect();
    commands.trigger(BinHostEmitEvent::from_rkyv(
        entity,
        FILE_COMPLETION_EVENT,
        &FileCompletionEvent {
            items,
            replace_from_col,
            line,
        },
    ));
    true
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
    edit.wrap_generation = edit.wrap_generation.wrapping_add(1);
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
    let mut viewport_changed = false;
    for cmd in cmds {
        if let EditCommand::ScrollViewport(lines) = &cmd {
            if browsers.can_emit_to(&entity) {
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    entity,
                    FILE_SCROLL_BY_EVENT,
                    &FileScrollByEvent { lines: *lines },
                ));
            }
            continue;
        }
        if let EditCommand::ScrollCursorTo(placement) = &cmd {
            let row = edit
                .folds
                .view(edit.core.buffer.len_lines() as u32)
                .buffer_to_row(edit.core.cursor_pos().line);
            let rows = vp.rows.max(1) as u32;
            vp.top_row = match placement {
                crate::edit::command::ScrollPlacement::Top => row,
                crate::edit::command::ScrollPlacement::Center => row.saturating_sub(rows / 2),
                crate::edit::command::ScrollPlacement::Bottom => row.saturating_sub(rows - 1),
            };
            edit.core.top_row = vp.top_row;
            viewport_changed = true;
            continue;
        }
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
            EditCommand::BeginRename => {
                let (line, _, ccol, lt) = caret_lsp(edit);
                let current = word_at_col(&lt, ccol);
                if current.is_empty() || !browsers.can_emit_to(&entity) {
                    continue;
                }
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    entity,
                    vmux_core::event::FILE_RENAME_BEGIN_EVENT,
                    &vmux_core::event::FileRenameBeginEvent {
                        line,
                        col: ccol as u32,
                        current,
                    },
                ));
                continue;
            }
            EditCommand::TriggerCompletion => {
                let (line, utf16, ccol, lt) = caret_lsp(edit);
                let replace_from = word_start_col(&lt, ccol);
                let path = edit.core.buffer.path.clone();
                manager.completion(entity, &path, line, utf16, replace_from);
                continue;
            }
            EditCommand::ScrollViewport(_) => unreachable!(),
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
                    if browsers.can_emit_to(&entity) {
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
        if matches!(cmd, EditCommand::Put { .. })
            && let Some(cb) = clipboard.0.as_mut()
            && let Ok(s) = cb.get_text()
            && s != edit.core.registers.clipboard_shadow
        {
            edit.core.registers.clipboard_shadow = s.clone();
            edit.core
                .registers
                .set_unnamed(crate::edit::RegisterValue::charwise(s));
        }
        let out = edit.core.apply(cmd);
        if out.text_changed {
            text_changed = true;
            let (l, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
            edit.hl.invalidate_from(l.saturating_sub(1));
        }
        sel_or_mode |= out.sel_changed || out.mode_changed;
        dirty_changed |= out.dirty_changed;
        if let Some(value) = out.yank
            && let Some(cb) = clipboard.0.as_mut()
        {
            edit.core.registers.clipboard_shadow = value.text.clone();
            let _ = cb.set_text(value.text);
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
    if let Some(top) = wrapped_autoscroll(edit, vp) {
        vp.top_row = top;
        viewport_changed = true;
    }
    let vpc = *vp;
    if text_changed || fold_changed || viewport_changed {
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
        edit.refresh_parsed_note();
        let markdown = edit.parsed_note.is_some();
        let mut entity_commands = commands.entity(entity);
        entity_commands
            .insert(LspEditDirty)
            .remove::<crate::lsp::manager::LintRan>();
        if markdown {
            entity_commands.remove::<NoteSent>().insert(OutlineDirty);
        }
    }
    text_changed
}

#[allow(clippy::too_many_arguments)]
fn on_file_key(
    trigger: On<BinReceive<KeyStroke>>,
    mut q: Query<(
        &mut EditState,
        &mut EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    app_keys: ScopedKeys,
    view_mode: Res<SharedFileViewMode>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let evt = &trigger.event().payload;
    if app_keys.answered(entity, evt) {
        return;
    }
    let Ok((mut edit, mut keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
        return;
    };
    let input = KeyInput {
        key: evt.key.clone(),
        mods: Mods {
            ctrl: evt.mods.ctrl,
            alt: evt.mods.alt,
            shift: evt.mods.shift,
            meta: evt.mods.super_key,
        },
        repeat: evt.repeat,
    };
    let mut cmds = accelerate_repeated_navigation(keymap.0.handle(&input), evt.repeat);
    if cmds.is_empty() {
        return;
    }
    if view_mode.0 == FileViewMode::Note
        && let Some(note) = edit.parsed_note.as_ref()
    {
        let line = edit.core.cursor_pos().line;
        cmds = remap_note_vertical_commands(cmds, &note.blocks, line);
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

fn accelerate_repeated_navigation(cmds: Vec<EditCommand>, repeat: bool) -> Vec<EditCommand> {
    if !repeat {
        return cmds;
    }
    cmds.into_iter()
        .flat_map(|cmd| {
            let accelerate = matches!(
                &cmd,
                EditCommand::Move(
                    Motion::Left
                        | Motion::Right
                        | Motion::LeftBounded
                        | Motion::RightBounded
                        | Motion::Up
                        | Motion::Down,
                ) | EditCommand::Select(
                    Motion::Left
                        | Motion::Right
                        | Motion::LeftBounded
                        | Motion::RightBounded
                        | Motion::Up
                        | Motion::Down,
                )
            );
            [Some(cmd.clone()), accelerate.then_some(cmd)]
                .into_iter()
                .flatten()
        })
        .collect()
}

fn remap_note_vertical_commands(
    cmds: Vec<EditCommand>,
    blocks: &[NoteBlock],
    start_line: u32,
) -> Vec<EditCommand> {
    let mut line = start_line;
    cmds.into_iter()
        .flat_map(|cmd| {
            let (direction, select) = match &cmd {
                EditCommand::Move(Motion::Down) => (1, false),
                EditCommand::Move(Motion::Up) => (-1, false),
                EditCommand::Select(Motion::Down) => (1, true),
                EditCommand::Select(Motion::Up) => (-1, true),
                _ => return vec![cmd],
            };
            match crate::markdown::note_vertical_target(blocks, line, direction) {
                Some(target) if target == line => Vec::new(),
                Some(target) => {
                    let steps = target.abs_diff(line) as usize;
                    line = target;
                    let motion = if direction > 0 {
                        Motion::Down
                    } else {
                        Motion::Up
                    };
                    let command = if select {
                        EditCommand::Select(motion)
                    } else {
                        EditCommand::Move(motion)
                    };
                    std::iter::repeat_n(command, steps).collect()
                }
                None => {
                    line = if direction > 0 {
                        line.saturating_add(1)
                    } else {
                        line.saturating_sub(1)
                    };
                    vec![cmd]
                }
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn on_file_text_input(
    trigger: On<BinReceive<FileTextInput>>,
    mut q: Query<(
        &mut EditState,
        &mut EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    if text.is_empty() {
        return;
    }
    let Ok((mut edit, mut keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
        return;
    };
    if !keymap.0.mode().accepts_text() {
        return;
    }
    keymap.0.record_text(&text);
    let command = if keymap.0.mode() == vmux_core::EditMode::Replace {
        EditCommand::OvertypeText(text)
    } else {
        EditCommand::InsertText(text)
    };
    run_commands(
        entity,
        vec![command],
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
    if let Some(index) = index.as_deref() {
        emit_wiki_completions(entity, &edit, index, &browsers, &mut commands);
    }
}

#[allow(clippy::too_many_arguments)]
fn on_file_property_edit(
    trigger: On<BinReceive<FilePropertyEdit>>,
    mut q: Query<(
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((mut edit, keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
        return;
    };
    if !crate::markdown::is_markdown_path(&edit.core.buffer.path) {
        return;
    }
    let text = edit.core.buffer.text();
    let updated = match vmux_core::knowledge::Frontmatter::of(&text).apply(&trigger.event().payload)
    {
        Ok(updated) => updated,
        Err(message) => {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                entity,
                FILE_ERROR_EVENT,
                &FileErrorEvent { message },
            ));
            return;
        }
    };
    if updated == text {
        return;
    }
    run_commands(
        entity,
        vec![EditCommand::ReplaceText(updated)],
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

/// Apply a `workspace/applyEdit` the server asked for, and answer it.
///
/// A rename touches every pane showing the file, so this collects them all rather than the
/// first match. Each pane keeps its own undo history, so each gets its own entry — but one
/// entry, not one per range: the whole document is swapped with a single `ReplaceText`.
#[allow(clippy::too_many_arguments)]
fn apply_lsp_workspace_edit(
    requests: Query<(Entity, &crate::lsp::server_request::AwaitingApplyEdit)>,
    mut views: Query<(
        Entity,
        &FileView,
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut replies: MessageWriter<crate::lsp::server_request::ServerReply>,
    mut renames: MessageReader<crate::lsp::manager::LspRequestedEdit>,
    mut commands: Commands,
) {
    for (request, awaiting) in &requests {
        let refusal = match WorkspaceEditPlan::of(&awaiting.0.edit) {
            Ok(plan) => apply_planned_documents(
                plan,
                &mut views,
                &mut clipboard,
                &mut self_writes,
                &mut manager,
                &browsers,
                &mut commands,
            ),
            Err(refusal) => Some(refusal.to_string()),
        };
        replies.write(crate::lsp::server_request::ServerReply {
            request,
            result: match &refusal {
                None => serde_json::json!({ "applied": true }),
                Some(reason) => {
                    serde_json::json!({ "applied": false, "failureReason": reason })
                }
            },
        });
    }

    // A rename reaches the same planner, but there is no request to answer — the user asked, so a
    // refusal goes back to the pane they asked from.
    for rename in renames.read() {
        let refusal = match &rename.result {
            Err(reason) => Some(reason.clone()),
            Ok(edit) => match WorkspaceEditPlan::of(edit) {
                Ok(plan) => apply_planned_documents(
                    plan,
                    &mut views,
                    &mut clipboard,
                    &mut self_writes,
                    &mut manager,
                    &browsers,
                    &mut commands,
                ),
                Err(refusal) => Some(refusal.to_string()),
            },
        };
        let Some(reason) = refusal else {
            continue;
        };
        if browsers.can_emit_to(&rename.entity) {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                rename.entity,
                vmux_core::event::FILE_EDIT_FAILED_EVENT,
                &vmux_core::event::FileEditFailedEvent { reason },
            ));
        }
    }
}

/// Returns the reason the edit could not be applied, or `None` when it was.
///
/// Documents are independent, so a failure part-way leaves earlier ones applied — honest
/// rollback across N ropes and M files buys less than it costs for a rename.
#[allow(clippy::too_many_arguments)]
fn apply_planned_documents(
    plan: WorkspaceEditPlan,
    views: &mut Query<(
        Entity,
        &FileView,
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    clipboard: &mut ClipboardHandle,
    self_writes: &mut SelfWrites,
    manager: &mut crate::lsp::manager::LspManager,
    browsers: &Browsers,
    commands: &mut Commands,
) -> Option<String> {
    for document in plan.documents {
        let wanted = canon(&document.path);
        if let (Some(expected), Some(actual)) =
            (document.version, manager.document_version(&document.path))
            && expected != actual
        {
            return Some(format!(
                "{} changed since the edit was computed",
                document.path.display()
            ));
        }

        let open: Vec<Entity> = views
            .iter()
            .filter(|(_, view, ..)| canon(&view.path) == wanted)
            .map(|(entity, ..)| entity)
            .collect();

        if open.is_empty() {
            if let Err(reason) = edit_closed_file(&document, self_writes) {
                return Some(reason);
            }
            continue;
        }

        // The server computed these ranges against one text. If two panes on this file have
        // drifted apart, at most one of them is that text and there is no way to tell which, so
        // applying would corrupt the other rather than merely overwrite it.
        let mut texts = open
            .iter()
            .filter_map(|entity| views.get(*entity).ok())
            .map(|(_, _, edit, ..)| edit.core.buffer.text());
        let first = texts.next().unwrap_or_default();
        if texts.any(|text| text != first) {
            return Some(format!(
                "{} is open more than once with different contents",
                document.path.display()
            ));
        }

        for entity in open {
            let Ok((_, _, mut edit, keymap, mut vp, mut diff_source)) = views.get_mut(entity)
            else {
                continue;
            };
            // Each pane owns its `EditCore`, so two panes on one file can hold different text.
            // Computing once and broadcasting would overwrite whichever pane did not win with
            // the other's unsaved work.
            let updated = match edit.core.buffer.with_lsp_edits(&document.edits) {
                Ok(updated) => updated,
                Err(e) => return Some(format!("{}: {e}", document.path.display())),
            };
            run_commands(
                entity,
                vec![EditCommand::ReplaceText(updated)],
                &mut edit,
                &mut diff_source,
                keymap.0.as_ref(),
                &mut vp,
                clipboard,
                self_writes,
                manager,
                browsers,
                commands,
            );
        }
    }
    None
}

/// Apply to disk, for a document no pane is showing.
///
/// Registered as a self-write first so the watcher does not read the rename back as an external
/// change and schedule a reload.
fn edit_closed_file(
    document: &crate::lsp::workspace_edit::PlannedDocument,
    self_writes: &mut SelfWrites,
) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(&document.path) else {
        return Err(format!("{} could not be read", document.path.display()));
    };
    let buffer =
        crate::edit::buffer::TextBuffer::from_text(document.path.clone(), String::new(), &text);
    let updated = match buffer.with_lsp_edits(&document.edits) {
        Ok(updated) => updated,
        Err(e) => return Err(format!("{}: {e}", document.path.display())),
    };
    self_writes
        .0
        .insert(canon(&document.path), std::time::Instant::now());
    write_atomic(&document.path, updated.as_bytes())
        .map_err(|e| format!("{}: {e}", document.path.display()))
}

fn on_file_hover_request(
    trigger: On<BinReceive<FileHoverRequest>>,
    q: Query<&EditState>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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
    select_end_col: Option<u32>,
}

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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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

/// Every context-menu row lands here.
///
/// The rows split three ways: some are a language-server request, some are an `EditCommand` that
/// has to go through `run_commands` so the clipboard and the viewport stay in step, and one is a
/// message to the shell. Keeping them in one observer is what lets the menu stay a table of
/// (label, shortcut, action) instead of a dozen bespoke wires.
#[allow(clippy::too_many_arguments)]
fn on_file_editor_action(
    trigger: On<BinReceive<FileEditorAction>>,
    mut q: Query<(
        &mut EditState,
        &EditorKeymap,
        &mut FileViewport,
        &mut vmux_git::GitDiffSource,
    )>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut code_actions: MessageWriter<crate::lsp::manager::LspCodeActionRequest>,
    mut app_commands: MessageWriter<vmux_command::host::command::AppCommand>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let action = trigger.event().payload;
    let Ok((mut edit, keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
        return;
    };
    let (line, utf16, ccol, lt) = caret_lsp(&edit);
    let path = edit.core.buffer.path.clone();

    let cmds = match action.action {
        EditorAction::CommandPalette => {
            // The bar belongs to the layout page and only the shell can address it. Asking by
            // command is how every other opener asks, so this does not become a second route in.
            app_commands.write(vmux_command::host::command::AppCommand::Browser(
                vmux_command::host::command::BrowserCommand::Bar(
                    vmux_command::host::command::BrowserBarCommand::OpenCommandBar,
                ),
            ));
            return;
        }
        EditorAction::CodeAction => {
            let (from_line, to_line) = edit.core.selected_lines();
            code_actions.write(crate::lsp::manager::LspCodeActionRequest {
                entity,
                path,
                from_line,
                to_line,
            });
            return;
        }
        EditorAction::GotoDeclaration => {
            manager.declaration(entity, &path, line, utf16);
            return;
        }
        EditorAction::GotoTypeDefinition => {
            manager.type_definition(entity, &path, line, utf16);
            return;
        }
        EditorAction::GotoImplementation => {
            manager.implementation(entity, &path, line, utf16);
            return;
        }
        EditorAction::FormatDocument => {
            manager.format_document(entity, &path);
            return;
        }
        EditorAction::FormatSelection => {
            let (from, to) = edit.core.selected_lines();
            manager.format_range(entity, &path, from, to);
            return;
        }
        EditorAction::Rename => {
            let current = word_at_col(&lt, ccol);
            if !current.is_empty() && browsers.can_emit_to(&entity) {
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    entity,
                    vmux_core::event::FILE_RENAME_BEGIN_EVENT,
                    &vmux_core::event::FileRenameBeginEvent {
                        line,
                        col: ccol as u32,
                        current,
                    },
                ));
            }
            return;
        }
        EditorAction::Copy => vec![EditCommand::Op {
            operator: crate::edit::command::Operator::Yank,
            target: crate::edit::command::Target::Selection,
            register: None,
        }],
        EditorAction::Cut => vec![EditCommand::Op {
            operator: crate::edit::command::Operator::Delete,
            target: crate::edit::command::Target::Selection,
            register: None,
        }],
        EditorAction::Paste => vec![EditCommand::Put {
            before: false,
            count: 1,
            register: None,
        }],
        EditorAction::ChangeAllOccurrences => vec![EditCommand::SelectAllOccurrences],
    };
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

/// Run the code action the user picked.
///
/// An action can carry an edit, a command, or both. The edit joins the same queue a rename's does;
/// the command goes to the server, which typically answers by asking this client to apply an edit
/// — the `workspace/applyEdit` path, already handled.
fn on_file_code_action_pick(
    trigger: On<BinReceive<FileCodeActionPick>>,
    q: Query<&EditState>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut edits: MessageWriter<crate::lsp::manager::LspRequestedEdit>,
) {
    let entity = trigger.event().webview;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    let path = edit.core.buffer.path.clone();
    let Some(workspace_edit) =
        manager.run_code_action(entity, trigger.event().payload.index as usize, &path)
    else {
        return;
    };
    edits.write(crate::lsp::manager::LspRequestedEdit {
        entity,
        result: Ok(workspace_edit),
    });
}

fn on_file_rename_request(
    trigger: On<BinReceive<FileRenameRequest>>,
    q: Query<&EditState>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let req = &trigger.event().payload;
    if req.new_name.trim().is_empty() {
        return;
    }
    let Ok(edit) = q.get(entity) else {
        return;
    };
    let (line, utf16, _) = req_pos(edit, req.line, req.col);
    let path = edit.core.buffer.path.clone();
    manager.rename(entity, &path, line, utf16, &req.new_name);
}

fn on_file_references_request(
    trigger: On<BinReceive<FileReferencesRequest>>,
    q: Query<&EditState>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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
    index: Option<Res<vmux_core::knowledge::KnowledgeIndex>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
) {
    let entity = trigger.event().webview;
    let req = trigger.event().payload;
    let Ok(edit) = q.get(entity) else {
        return;
    };
    if index
        .as_deref()
        .is_some_and(|index| emit_wiki_completions(entity, edit, index, &browsers, &mut commands))
    {
        return;
    }
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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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
        vec![EditCommand::InsertText(req.text)],
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
    if let Some(top) = wrapped_autoscroll(edit, vp) {
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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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
                &mut edit,
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
                .remove::<NoteSent>()
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
            &mut edit,
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
    mut q: Query<(&mut EditState, &mut EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let p = trigger.event().payload;
    let Ok((mut edit, mut keymap, vp)) = q.get_mut(entity) else {
        return;
    };
    let at = edit
        .core
        .buffer
        .coords_to_char(p.line as usize, p.col as usize);
    if p.add {
        edit.core.toggle_caret(at);
    } else if p.extend {
        let anchor = edit.core.primary().anchor;
        edit.core.selections = vec![Selection { anchor, head: at }];
    } else {
        // A plain click means "one caret, here", so it puts back any the user had added.
        edit.core.collapse_carets();
        edit.core.set_caret(at);
    }
    if let Some(command) = keymap.0.pointer_selection_mode(p.extend) {
        edit.core.apply(command);
    }
    emit_cursor(
        entity,
        &mut edit,
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
    mut manager: ResMut<crate::lsp::manager::LspManager>,
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
        manager.semantic_tokens(entity, &fv.path);
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
    if browsers.can_emit_to(&entity) {
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
        if !browsers.can_emit_to(&entity) {
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
            let next_path = path.with_file_name(&name);
            let knowledge_root = vmux_core::knowledge::KnowledgeVault::user().into_root();
            let rename_plan = (root
                .canonicalize()
                .ok()
                .zip(knowledge_root.canonicalize().ok())
                .is_some_and(|(root, knowledge_root)| root == knowledge_root))
            .then(|| {
                vmux_core::knowledge::KnowledgeIndex::build(&root)
                    .map(|index| {
                        vmux_core::knowledge::KnowledgeRenamePlan::build(&index, &path, &next_path)
                    })
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
            let (changed_path, was_dir) = crate::explorer_fs::rename_entry(&root, &path, &name)?;
            if let Some(plan) = rename_plan {
                plan.apply().map_err(|error| error.to_string())?;
            }
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
    if browsers.can_emit_to(&webview) {
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
    chrome.default_visible = settings.editor.explorer.visible();
    chrome.width = settings.editor.explorer.width();
    synced.0 = true;
    for e in &views {
        commands.entity(e).remove::<ExplorerChromeSent>();
    }
}

fn explorer_scope(entity: Entity, child_of: &Query<&ChildOf>) -> Entity {
    child_of.get(entity).map(ChildOf::parent).unwrap_or(entity)
}

fn emit_explorer_chrome(
    q: Query<(Entity, Option<&ChildOf>), ChromeUnsentReady>,
    visibility: Query<&StackExplorerVisibility>,
    revisions: Query<&StackExplorerRevision>,
    chrome: Res<ExplorerChrome>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, child_of) in &q {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let scope = child_of.map(ChildOf::parent).unwrap_or(entity);
        let visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(chrome.default_visible);
        let revision = revisions.get(scope).copied().unwrap_or_default();
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_CHROME_EVENT,
            &ExplorerChromeEvent {
                visible,
                width: chrome.width,
                client_id: revision.client_id,
                request_id: revision.request_id,
            },
        ));
        commands.entity(entity).insert(ExplorerChromeSent);
    }
}

fn persist_chrome_width(
    width: u32,
    settings: Option<ResMut<vmux_setting::AppSettings>>,
    saves: Option<ResMut<bevy::ecs::message::Messages<vmux_setting::SettingsSaveRequest>>>,
) {
    let Some(mut settings) = settings else {
        return;
    };
    settings.editor.explorer.width = Some(width);
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
    child_of: Query<&ChildOf>,
    mut visibility: Query<&mut StackExplorerVisibility>,
    mut revisions: Query<&mut StackExplorerRevision>,
    mut editors: Query<(Entity, &FileView, &mut ExplorerState, Option<&ChildOf>)>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let scope = explorer_scope(entity, &child_of);
    let next_visibility = StackExplorerVisibility {
        visible: trigger.event().payload.visible,
    };
    if let Ok(mut state) = visibility.get_mut(scope) {
        *state = next_visibility;
    } else {
        commands.entity(scope).insert(next_visibility);
    }
    let next_revision = StackExplorerRevision {
        client_id: trigger.event().payload.client_id,
        request_id: trigger.event().payload.request_id,
    };
    if let Ok(mut revision) = revisions.get_mut(scope) {
        *revision = next_revision;
    } else {
        commands.entity(scope).insert(next_revision);
    }
    for (view, _, _, parent) in &mut editors {
        let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
        if view_scope != scope {
            continue;
        }
        if view == entity {
            commands.entity(view).insert(ExplorerChromeSent);
        } else {
            commands.entity(view).remove::<ExplorerChromeSent>();
        }
    }
    if next_visibility.visible
        && let Ok((_, fv, mut st, _)) = editors.get_mut(entity)
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
    persist_chrome_width(chrome.width, settings, saves);
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

type OpenEditorsView = (
    Entity,
    &'static FileView,
    &'static ExplorerState,
    Option<&'static EditState>,
    Option<&'static ParkedEdits>,
);

fn emit_open_editors(
    q: Query<OpenEditorsView, OpenEditorsDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, fv, st, edit, parked) in &q {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let active_dirty = edit.map(|e| e.core.dirty).unwrap_or(false);
        let mut items = Vec::with_capacity(st.open_editors.len());
        for path in &st.open_editors {
            let active = *path == fv.path;
            let dirty = match active {
                true => active_dirty,
                false => parked.is_some_and(|p| p.is_dirty(path)),
            };
            items.push(OpenEditorItem {
                name: open_editor_name(path),
                path: path.to_string_lossy().into_owned(),
                active,
                dirty,
            });
        }
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

fn emit_outline_markdown(
    q: Query<(Entity, &EditState), OutlineDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, edit) in &q {
        if !browsers.can_emit_to(&entity) {
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
        if browsers.can_emit_to(&entity) {
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

fn apply_global_search_requests(
    mut reader: MessageReader<GlobalSearchRequest>,
    views: Query<(Entity, &FileView, Option<&ChildOf>)>,
    visibility: Query<&StackExplorerVisibility>,
    mut pending: ResMut<PendingGlobalSearch>,
    chrome: Res<ExplorerChrome>,
    mut commands: Commands,
) {
    pending.0.extend(
        reader
            .read()
            .cloned()
            .map(|request| PendingGlobalSearchRequest {
                request,
                retries_left: GLOBAL_SEARCH_RETRY_LIMIT,
            }),
    );
    let mut remaining = Vec::new();
    for mut pending_request in pending.0.drain(..) {
        let request = &pending_request.request;
        let Some((entity, _, parent)) = views
            .iter()
            .find(|(_, view, _)| view.path == request.target_path)
        else {
            pending_request.retries_left = pending_request.retries_left.saturating_sub(1);
            if pending_request.retries_left > 0 {
                remaining.push(pending_request);
            }
            continue;
        };
        let scope = parent.map(ChildOf::parent).unwrap_or(entity);
        let explorer_visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(chrome.default_visible);
        if !explorer_visible {
            commands
                .entity(scope)
                .insert(StackExplorerVisibility { visible: true });
            for (view, _, parent) in &views {
                let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
                if view_scope == scope {
                    commands.entity(view).remove::<ExplorerChromeSent>();
                }
            }
        }
        let request = pending_request.request;
        commands.entity(entity).insert((
            GlobalSearchState(ExplorerSearchEvent {
                root: request.root,
                query: request.query,
                matches: request.matches,
            }),
            GlobalSearchDirty,
        ));
    }
    pending.0 = remaining;
}

fn emit_global_search(
    q: Query<(Entity, &GlobalSearchState), GlobalSearchDirtyReady>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, search) in &q {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            EXPLORER_SEARCH_EVENT,
            &search.0,
        ));
        commands.entity(entity).remove::<GlobalSearchDirty>();
    }
}

fn on_explorer_search_open(
    trigger: On<BinReceive<ExplorerSearchOpen>>,
    mut views: Query<(&mut FileView, &mut FileViewport, &mut PageMetadata)>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut view, mut viewport, mut metadata)) = views.get_mut(entity) else {
        return;
    };
    navigate_file_view(
        entity,
        PathBuf::from(&request.path),
        request.line.saturating_sub(1),
        &mut view,
        &mut viewport,
        &mut metadata,
        &mut manager,
        &mut commands,
    );
    commands.entity(entity).insert(PendingGoto {
        line: request.line.saturating_sub(1),
        utf16_col: request.col,
        select_end_col: Some(request.end_col),
    });
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "files",
    title: "Files",
    title_message_id: None,
    replaces_command: None,
    keywords: &["file", "open"],
    icon: Some(vmux_core::BuiltinIcon::Files),
    command_bar: true,
};

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
    fn switching_to_note_reveals_the_current_cursor_line() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SharedFileViewMode>()
            .add_observer(on_file_view_mode_set);
        app.world_mut().resource_mut::<SharedFileViewMode>().0 = FileViewMode::Editor;

        let path = PathBuf::from("/note.md");
        let mut core = EditCore::new(
            path.clone(),
            "Markdown".into(),
            "one\ntwo\nthree\n",
            crate::edit::EditMode::Normal,
        );
        core.apply(EditCommand::Move(Motion::GotoLine(2)));
        let entity = app
            .world_mut()
            .spawn((
                FileView { path: path.clone() },
                EditState::new(
                    core,
                    HighlightCache::new(&path),
                    crate::fold::FoldState::default(),
                ),
            ))
            .id();

        app.world_mut().trigger(BinReceive {
            webview: entity,
            payload: FileViewModeSet {
                mode: FileViewMode::Note,
            },
        });
        app.update();

        assert_eq!(
            app.world().get::<NoteRevealLine>(entity).map(|line| line.0),
            Some(2)
        );
    }

    #[test]
    fn missing_file_view_loads_when_file_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("created-after-open");
        let path = parent.join("file.txt");
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(|_| {}).unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(
            Update,
            (
                reconcile_file_watches,
                drain_file_changes,
                reload_changed_files,
                load_file_buffers,
            )
                .chain(),
        );
        app.world_mut().insert_non_send(FileWatch {
            watcher,
            rx,
            dirs: HashSet::new(),
        });
        app.world_mut().insert_non_send(SelfWrites::default());
        app.world_mut().insert_non_send(Browsers::default());
        app.world_mut()
            .insert_resource(crate::lsp::manager::LspManager::new(
                crate::lsp::LspOutbox::default(),
                crate::lsp::server_request::ServerEvents::default().sender(),
            ));
        let entity = app
            .world_mut()
            .spawn((
                FileView { path: path.clone() },
                FileViewport {
                    top_row: 0,
                    rows: 0,
                    wrap_columns: 0,
                    word_wrap: vmux_core::editor::WordWrap::default(),
                    word_wrap_column: 80,
                },
            ))
            .id();

        app.update();
        assert!(
            app.world()
                .get::<FileBuffer>(entity)
                .unwrap()
                .language
                .starts_with("__error__:cannot open")
        );

        std::fs::create_dir(&parent).unwrap();
        std::fs::write(&path, "created\n").unwrap();
        tx.send(Ok(
            notify::Event::new(notify::EventKind::Any).add_path(parent)
        ))
        .unwrap();
        app.update();

        assert_eq!(
            app.world()
                .get::<EditState>(entity)
                .unwrap()
                .core
                .buffer
                .text(),
            "created\n"
        );
    }

    #[test]
    fn file_view_mode_request_updates_shared_mode() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SharedFileViewMode>()
            .add_message::<FileViewModeRequest>()
            .add_systems(Update, apply_file_view_mode_requests);

        app.world_mut()
            .resource_mut::<Messages<FileViewModeRequest>>()
            .write(FileViewModeRequest(FileViewMode::Diff));
        app.update();

        assert_eq!(
            app.world().resource::<SharedFileViewMode>().0,
            FileViewMode::Diff
        );
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
        let mut km = vmux_core::KeymapKind::Vim.make(&[], " ");
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

    #[test]
    fn repeated_navigation_advances_two_steps_without_accelerating_edits() {
        assert_eq!(
            accelerate_repeated_navigation(vec![EditCommand::Move(Motion::Down)], true),
            [
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down)
            ]
        );
        assert_eq!(
            accelerate_repeated_navigation(vec![EditCommand::DeleteBack], true),
            [EditCommand::DeleteBack]
        );
    }

    #[test]
    fn repeated_note_navigation_skips_a_separator_after_the_first_step() {
        let blocks = crate::markdown::parse_note("- one\n- two\n\nnext\n");
        let commands = remap_note_vertical_commands(
            accelerate_repeated_navigation(vec![EditCommand::Move(Motion::Down)], true),
            &blocks,
            0,
        );
        assert_eq!(
            commands,
            [
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
            ]
        );
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
    fn panel_visibility_is_shared_only_within_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(on_explorer_panel_set_visible);
        let first_stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: true })
            .id();
        let second_stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: true })
            .id();
        let first = app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::from("/a.rs"),
                },
                ExplorerState::default(),
                ExplorerChromeSent,
                ChildOf(first_stack),
            ))
            .id();
        let peer = app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::from("/b.rs"),
                },
                ExplorerState::default(),
                ExplorerChromeSent,
                ChildOf(first_stack),
            ))
            .id();
        let other = app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::from("/c.rs"),
                },
                ExplorerState::default(),
                ExplorerChromeSent,
                ChildOf(second_stack),
            ))
            .id();
        app.world_mut().trigger(BinReceive {
            webview: first,
            payload: ExplorerPanelSetVisible {
                visible: false,
                client_id: 7,
                request_id: 1,
            },
        });
        app.update();
        assert!(
            !app.world()
                .get::<StackExplorerVisibility>(first_stack)
                .unwrap()
                .visible
        );
        assert!(
            app.world()
                .get::<StackExplorerVisibility>(second_stack)
                .unwrap()
                .visible
        );
        assert!(app.world().get::<ExplorerChromeSent>(first).is_some());
        assert!(app.world().get::<ExplorerChromeSent>(peer).is_none());
        assert!(app.world().get::<ExplorerChromeSent>(other).is_some());

        app.world_mut().trigger(BinReceive {
            webview: first,
            payload: ExplorerPanelSetVisible {
                visible: false,
                client_id: 7,
                request_id: 2,
            },
        });
        app.update();
        let revision = app
            .world()
            .get::<StackExplorerRevision>(first_stack)
            .unwrap();
        assert_eq!(revision.client_id, 7);
        assert_eq!(revision.request_id, 2);
    }

    #[test]
    fn global_search_opens_only_the_target_stack_explorer() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ExplorerChrome {
                default_visible: false,
                width: 240,
            })
            .init_resource::<PendingGlobalSearch>()
            .add_message::<GlobalSearchRequest>()
            .add_systems(Update, apply_global_search_requests);
        let first_stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let second_stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let target = PathBuf::from("/project/a.rs");
        let first = app
            .world_mut()
            .spawn((
                FileView {
                    path: target.clone(),
                },
                ChildOf(first_stack),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::from("/project/b.rs"),
                },
                ChildOf(second_stack),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<GlobalSearchRequest>>()
            .write(GlobalSearchRequest {
                target_path: target,
                root: "/project".to_string(),
                query: "needle".to_string(),
                matches: Vec::new(),
            });
        app.update();

        assert!(
            app.world()
                .get::<StackExplorerVisibility>(first_stack)
                .unwrap()
                .visible
        );
        assert!(
            !app.world()
                .get::<StackExplorerVisibility>(second_stack)
                .unwrap()
                .visible
        );
        assert!(app.world().get::<GlobalSearchState>(first).is_some());
        assert!(app.world().get::<GlobalSearchState>(second).is_none());
    }

    #[test]
    fn panel_open_reveals_current_file() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (init_explorer_state, drain_explorer_dir_loads))
            .add_observer(on_explorer_panel_set_visible);
        let stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let e = app
            .world_mut()
            .spawn((
                FileView { path: file.clone() },
                ExplorerState::default(),
                ChildOf(stack),
            ))
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
        assert!(
            app.world()
                .get::<StackExplorerVisibility>(stack)
                .unwrap()
                .visible
        );
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(st.focus_path.as_deref(), Some(file.as_path()));
    }

    #[test]
    fn panel_width_clamps() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ExplorerChrome {
                default_visible: true,
                width: 240,
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
mod page_open_tests {
    use super::*;
    use vmux_core::PageOpenId;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_core::event::RecordVisitRequest>()
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
                    wrap_columns: 0,
                    word_wrap: vmux_core::editor::WordWrap::default(),
                    word_wrap_column: 80,
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

    /// Two slashes instead of three is the common typo. Reading only the path opens `/me/a.rs`
    /// and then blames a path the user never typed.
    #[test]
    fn a_host_is_folded_back_onto_the_path() {
        assert_eq!(
            path_from_files_url("file://Users/me/a.rs"),
            Some(PathBuf::from("/Users/me/a.rs"))
        );
    }

    #[test]
    fn localhost_really_does_mean_this_machine() {
        assert_eq!(
            path_from_files_url("file://localhost/Users/me/a.rs"),
            Some(PathBuf::from("/Users/me/a.rs"))
        );
    }

    /// Only the whole host. A directory that merely starts with those nine letters is a directory,
    /// and dropping the prefix off it names a path that does not exist.
    #[test]
    fn a_directory_named_after_localhost_keeps_its_name() {
        assert_eq!(
            path_from_files_url("file://localhost-notes/a.rs"),
            Some(PathBuf::from("/localhost-notes/a.rs"))
        );
    }
}

#[cfg(test)]
mod parked_edit_tests {
    use super::*;

    /// One view navigating between two files, driven through the observer production uses.
    struct Session {
        app: App,
        entity: Entity,
        dir: tempfile::TempDir,
    }

    impl Session {
        fn open(first: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_systems(Update, load_file_buffers)
                .add_observer(on_file_open);
            app.world_mut().insert_non_send(SelfWrites::default());
            app.world_mut().insert_non_send(Browsers::default());
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));
            let entity = app
                .world_mut()
                .spawn((
                    FileView {
                        path: dir.path().join(first),
                    },
                    FileViewport {
                        top_row: 0,
                        rows: 0,
                        wrap_columns: 0,
                        word_wrap: vmux_core::editor::WordWrap::default(),
                        word_wrap_column: 80,
                    },
                    PageMetadata::default(),
                ))
                .id();
            Self { app, entity, dir }
        }

        fn write(&self, name: &str, text: &str) {
            std::fs::write(self.dir.path().join(name), text).unwrap();
        }

        fn navigate_to(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(BinReceive {
                webview: self.entity,
                payload: FileOpenEvent { path },
            });
            self.app.update();
        }

        fn type_into_buffer(&mut self, text: &str) {
            let mut edit = self
                .app
                .world_mut()
                .get_mut::<EditState>(self.entity)
                .expect("a loaded buffer");
            edit.core.apply(EditCommand::InsertText(text.to_string()));
        }

        fn text(&self) -> String {
            self.app
                .world()
                .get::<EditState>(self.entity)
                .unwrap()
                .core
                .buffer
                .text()
        }

        fn undo(&mut self) {
            self.app
                .world_mut()
                .get_mut::<EditState>(self.entity)
                .unwrap()
                .core
                .apply(EditCommand::Undo);
        }
    }

    /// The bug this exists to stop: edit a file, look at another, come back, and the undo tree,
    /// cursor and marks were all gone because `navigate_file_view` dropped `EditState`.
    #[test]
    fn returning_to_a_file_keeps_its_undo_history() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "one\n");
        s.write("lib.rs", "two\n");
        s.app.update();
        assert_eq!(s.text(), "one\n");

        s.type_into_buffer("EDIT");
        assert_eq!(s.text(), "EDITone\n");

        s.navigate_to("lib.rs");
        assert_eq!(s.text(), "two\n");

        s.navigate_to("main.rs");
        assert_eq!(
            s.text(),
            "EDITone\n",
            "unsaved edit survives the round trip"
        );
        s.undo();
        assert_eq!(s.text(), "one\n", "and so does the undo tree behind it");
    }

    /// A clean buffer whose file moved on must not be restored over the newer text.
    #[test]
    fn a_file_changed_while_parked_is_reloaded() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "before\n");
        s.write("lib.rs", "other\n");
        s.app.update();
        assert_eq!(s.text(), "before\n");

        s.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.write("main.rs", "changed on disk\n");
        s.navigate_to("main.rs");

        assert_eq!(s.text(), "changed on disk\n");
    }

    /// Losing unsaved work to an external write would be worse than showing stale text; the
    /// external-change path warns about the conflict instead.
    #[test]
    fn unsaved_edits_survive_a_file_changing_while_parked() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "before\n");
        s.write("lib.rs", "other\n");
        s.app.update();

        s.type_into_buffer("MINE");
        s.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.write("main.rs", "theirs\n");
        s.navigate_to("main.rs");

        assert_eq!(s.text(), "MINEbefore\n");
    }

    #[test]
    fn only_the_most_recent_files_are_held() {
        let mut edits = ParkedEdits::default();
        for i in 0..ParkedEdits::CAPACITY + 3 {
            let path = PathBuf::from(format!("/tmp/{i}.rs"));
            let core = EditCore::new(
                path.clone(),
                "Rust".into(),
                "x\n",
                crate::edit::EditMode::Normal,
            );
            edits.insert(
                path,
                ParkedEdit {
                    edit: EditState::new(
                        core,
                        HighlightCache::new(Path::new("/tmp/a.rs")),
                        crate::fold::FoldState::default(),
                    ),
                    diff: vmux_git::GitDiffSource {
                        content: String::new(),
                        dirty: false,
                    },
                    modified: None,
                },
            );
        }
        assert_eq!(edits.by_path.len(), ParkedEdits::CAPACITY);
        assert!(edits.by_path.contains_key(Path::new("/tmp/10.rs")));
        assert!(!edits.by_path.contains_key(Path::new("/tmp/0.rs")));
    }
}

#[cfg(test)]
mod workspace_edit_tests {
    use super::*;

    /// A server-driven edit, from the channel a reader thread pushes to through to the reply.
    ///
    /// Carries `ServerRequestPlugin` rather than re-registering its systems, so the ordering
    /// between answering and replying is the one production has. `EditorPlugin` itself cannot
    /// be added here: it opens a clipboard and a file watcher.
    struct ApplyEdit {
        app: App,
        views: Vec<Entity>,
        sent: std::sync::mpsc::Receiver<serde_json::Value>,
    }

    impl ApplyEdit {
        const BEFORE: &'static str = "one two three\n";

        /// The same panes and the same edit, but arriving as a `textDocument/rename` reply rather
        /// than as a request from the server. Nothing answers a reply, so `sent` stays empty.
        fn renamed(path: &Path, panes: usize) -> Self {
            let (mut app, views) = Self::bare(path, panes);
            app.world_mut()
                .write_message(crate::lsp::manager::LspRequestedEdit {
                    entity: views[0],
                    result: Ok(Self::renaming(path)),
                });
            let (_outgoing, sent) = std::sync::mpsc::channel();
            Self { app, views, sent }
        }

        fn of(path: &Path, panes: usize) -> Self {
            let (app, views) = Self::bare(path, panes);
            let (outgoing, sent) = std::sync::mpsc::channel();
            let events = app
                .world()
                .resource::<crate::lsp::server_request::ServerEvents>()
                .sender();
            events
                .send(crate::lsp::server_request::ServerEvent::ApplyEdit {
                    reply: crate::lsp::server_request::ReplyHandle::new(
                        crate::lsp::wire::RequestId::Number(1000),
                        outgoing,
                    ),
                    params: lsp_types::ApplyWorkspaceEditParams {
                        label: None,
                        edit: Self::renaming(path),
                    },
                })
                .unwrap();
            Self { app, views, sent }
        }

        fn bare(path: &Path, panes: usize) -> (App, Vec<Entity>) {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                crate::lsp::server_request::ServerRequestPlugin,
            ))
            .add_message::<crate::lsp::manager::LspRequestedEdit>()
            .add_systems(
                Update,
                apply_lsp_workspace_edit
                    .in_set(crate::lsp::server_request::ServerRequestSet::Answer),
            );
            app.world_mut().insert_non_send(ClipboardHandle(None));
            app.world_mut().insert_non_send(SelfWrites::default());
            app.world_mut().insert_non_send(Browsers::default());
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));

            let mut views = Vec::new();
            for _ in 0..panes {
                let core = EditCore::new(
                    path.to_path_buf(),
                    "Rust".into(),
                    Self::BEFORE,
                    crate::edit::EditMode::Normal,
                );
                views.push(
                    app.world_mut()
                        .spawn((
                            FileView {
                                path: path.to_path_buf(),
                            },
                            EditState::new(
                                core,
                                HighlightCache::new(path),
                                crate::fold::FoldState::default(),
                            ),
                            EditorKeymap(vmux_core::editor::KeymapKind::Vscode.make(&[], "\\")),
                            FileViewport {
                                top_row: 0,
                                rows: 0,
                                wrap_columns: 0,
                                word_wrap: vmux_core::editor::WordWrap::default(),
                                word_wrap_column: 80,
                            },
                            vmux_git::GitDiffSource {
                                content: Self::BEFORE.to_string(),
                                dirty: false,
                            },
                        ))
                        .id(),
                );
            }

            (app, views)
        }

        /// Two ranges in one document, given out of order, as a rename would produce.
        ///
        /// `clippy::mutable_key_type` fires on `Uri`'s internal cache, but `changes` is keyed
        /// that way by `lsp-types` and nothing here mutates a key.
        #[allow(clippy::mutable_key_type)]
        fn renaming(path: &Path) -> lsp_types::WorkspaceEdit {
            let edit = |start: u32, end: u32, text: &str| lsp_types::TextEdit {
                range: lsp_types::Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: start,
                    },
                    end: lsp_types::Position {
                        line: 0,
                        character: end,
                    },
                },
                new_text: text.to_string(),
            };
            let uri: lsp_types::Uri = format!("file://{}", path.display()).parse().unwrap();
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri, vec![edit(8, 13, "3"), edit(0, 3, "1")]);
            lsp_types::WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }
        }

        fn text(&self, entity: Entity) -> String {
            self.app
                .world()
                .get::<EditState>(entity)
                .unwrap()
                .core
                .buffer
                .text()
        }

        fn undo(&mut self, entity: Entity) {
            self.app
                .world_mut()
                .get_mut::<EditState>(entity)
                .unwrap()
                .core
                .apply(EditCommand::Undo);
        }
    }

    /// The pre-filled name, and the gate that decides there is nothing to rename.
    ///
    /// A caret sits *between* characters, so the word has to grow both ways from it; taking only
    /// the prefix gives the server half an identifier to rename.
    #[test]
    fn the_rename_prefill_is_the_whole_identifier_around_the_caret() {
        assert_eq!(word_at_col("let some_name = 1;", 8), "some_name");
        assert_eq!(word_at_col("let some_name = 1;", 4), "some_name");
        assert_eq!(word_at_col("let some_name = 1;", 13), "some_name");
        assert_eq!(
            word_at_col("let some_name = 1;", 14),
            "",
            "a caret on whitespace has nothing to rename"
        );
    }

    /// A rename's reply carries the `WorkspaceEdit` in the response rather than in a request, so
    /// it reaches the applier by message. Wiring the reader but never draining it, or ordering it
    /// outside the set the applier runs in, leaves the rename silently doing nothing.
    #[test]
    fn a_rename_reply_edits_the_panes_the_way_an_apply_edit_request_does() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::renamed(&path, 2);
        h.app.update();

        for view in h.views.clone() {
            assert_eq!(h.text(view), "1 two 3\n");
        }
    }

    #[test]
    fn apply_edit_reaches_every_pane_showing_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::of(&path, 2);
        h.app.update();

        for view in h.views.clone() {
            assert_eq!(h.text(view), "1 two 3\n");
            assert!(
                h.app.world().get::<EditState>(view).unwrap().core.dirty,
                "an applied edit leaves the buffer dirty for the user to save"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            ApplyEdit::BEFORE,
            "an open document is edited in the buffer, not written behind the user"
        );
    }

    /// N ranges must collapse to one undo entry, which a naive edit-per-range does not.
    #[test]
    fn the_whole_edit_undoes_in_one_step() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::of(&path, 1);
        h.app.update();
        let view = h.views[0];
        assert_eq!(h.text(view), "1 two 3\n");

        h.undo(view);
        assert_eq!(h.text(view), ApplyEdit::BEFORE);
    }

    /// Two panes on one file hold independent buffers. The server's ranges were computed
    /// against one text, so once the panes drift there is no text to apply them to: applying
    /// the first pane's result to the second overwrites its unsaved work, and re-applying the
    /// stale ranges to the second's own text corrupts it. Refuse instead.
    #[test]
    fn panes_that_have_drifted_apart_are_refused_rather_than_corrupted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::of(&path, 2);
        let second = h.views[1];
        h.app
            .world_mut()
            .get_mut::<EditState>(second)
            .unwrap()
            .core
            .apply(EditCommand::InsertText("MINE ".to_string()));
        h.app.update();

        assert_eq!(h.text(h.views[0]), ApplyEdit::BEFORE, "left untouched");
        assert_eq!(h.text(second), "MINE one two three\n", "left untouched");

        let reply = h.sent.try_recv().expect("the server must be answered");
        assert_eq!(reply["result"]["applied"], false);
        assert!(
            reply["result"]["failureReason"]
                .as_str()
                .is_some_and(|r| r.contains("different contents")),
            "the server is told why: {reply}"
        );
    }

    #[test]
    fn the_server_is_told_the_edit_applied() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::of(&path, 1);
        h.app.update();

        let reply = h.sent.try_recv().expect("the server must be answered");
        assert_eq!(reply["id"], 1000);
        assert_eq!(reply["result"]["applied"], true);
    }

    #[test]
    fn a_document_no_pane_shows_is_edited_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("closed.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::of(&path, 0);
        h.app.update();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1 two 3\n");
        assert_eq!(h.sent.try_recv().unwrap()["result"]["applied"], true);
    }
}
