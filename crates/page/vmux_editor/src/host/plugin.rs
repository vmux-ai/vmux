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

use crate::dir::{list_dir, parent_listing};
use crate::edit::highlight_cache::HighlightCache;
use crate::edit::{EditCommand, EditCore, Motion, Selection};
use crate::history::EditorHistoryPlugin;
use crate::host::explorer_mutation::ExplorerMutationPlugin;
#[cfg(test)]
use crate::host::explorer_panel::StackExplorerRevision;
use crate::host::explorer_panel::{
    ExplorerPanelDefaults, ExplorerPanelPlugin, ExplorerPanelSent, StackExplorerVisibility,
};
use crate::host::explorer_tabs::{ExplorerTabsPlugin, OpenEditorsDirty};
#[cfg(test)]
use crate::host::explorer_tree::{ExplorerTree, IDLE_TREE_CAPACITY};
use crate::host::explorer_tree::{ExplorerTreeDirty, ExplorerTreePlugin, ExplorerTrees};
use crate::host::note::{EditorNotePlugin, NoteSent};
use crate::host::status::{
    EditorStatusPlugin, FileInitialMetaSent, FileKeymapSent, FileThemeSent, FileViewModeSent,
    SharedFileViewMode,
};
use crate::host::viewport::{
    EditorCursor, EditorViewportPlugin, EditorWindow, FileViewport, FoldsDirty,
};
use crate::keymap::{KeyInput, Keymap, KeymapKindExt, Mods};
use crate::lsp::workspace_edit::WorkspaceEditPlan;
use crate::media::{EditorMediaPlugin, FileMedia};
use crate::navigation::EditorNavigationPlugin;
use crate::page_model::DisplayCells;
use crate::wrap::WrapView;
use vmux_core::scroll::clamp_top_line;
use vmux_flex::prelude::*;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(FILES_PAGE_MANIFEST);
        app.world_mut().spawn(PROJECTS_PAGE_MANIFEST);
        app.add_plugins((
            crate::contract::EditorContractPlugin,
            crate::lsp::LspPlugin,
            crate::app_key::FileKeyPlugin,
            crate::search::ProjectSearchPlugin,
            EditorFileLifecyclePlugin,
            EditorStatusPlugin,
            EditorViewportPlugin,
            EditorMediaPlugin,
            EditorNotePlugin,
            EditorEditingPlugin,
            EditorNavigationPlugin,
            EditorHistoryPlugin,
            EditorExplorerPlugin,
        ));
    }
}

struct EditorFileLifecyclePlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EditorFileLoadedSet;

impl Plugin for EditorFileLifecyclePlugin {
    fn build(&self, app: &mut App) {
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
        app.insert_non_send(SelfWrites::default())
            .insert_non_send(crate::fold_store::FoldStore::load())
            .add_message::<vmux_core::event::RecordVisitRequest>()
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
                        apply_loaded_file_buffers.in_set(EditorFileLoadedSet),
                    )
                        .chain(),
                    flush_lsp_changes,
                    apply_goto,
                    apply_pending_goto,
                    reapply_keymap_on_change,
                ),
            )
            .add_systems(
                Update,
                apply_lsp_workspace_edit
                    .in_set(crate::lsp::server_request::ServerRequestSet::Answer),
            )
            .add_observer(reset_file_sent_markers_on_page_ready);
    }
}

struct EditorEditingPlugin;

impl Plugin for EditorEditingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(ClipboardHandle(arboard::Clipboard::new().ok()))
            .add_plugins(BinEventEmitterPlugin::<(
                FileOpenEvent,
                FileTextInput,
                FilePointerEvent,
                FileHoverRequest,
                FileDefinitionRequest,
                FileReferencesRequest,
                FileRenameRequest,
                FileEditorAction,
                FileCodeActionPick,
                FileCompletionRequest,
                FileGotoRequest,
                FileCompletionCommit,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                KnowledgeLinkOpen,
                FilePropertyEdit,
                FileFindRequest,
                FileShapeSet,
                FileEncodingSet,
            )>::default())
            .add_observer(on_file_key)
            .add_observer(on_file_text_input)
            .add_observer(on_file_pointer)
            .add_observer(on_file_hover_request)
            .add_systems(Update, run_submitted_ex_lines)
            .add_observer(on_file_find_request)
            .add_observer(on_file_definition_request)
            .add_observer(on_file_references_request)
            .add_observer(on_file_rename_request)
            .add_observer(on_file_editor_action)
            .add_observer(on_file_code_action_pick)
            .add_observer(on_file_completion_request)
            .add_observer(on_file_goto_request)
            .add_observer(on_file_completion_commit)
            .add_observer(on_file_property_edit)
            .add_observer(on_file_shape_set)
            .add_observer(on_file_encoding_set);
    }
}

struct EditorExplorerPlugin;

impl Plugin for EditorExplorerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingGlobalSearch>()
            .add_plugins((
                ExplorerTreePlugin,
                ExplorerPanelPlugin,
                ExplorerMutationPlugin,
                ExplorerTabsPlugin,
            ))
            .add_plugins(BinEventEmitterPlugin::<(
                ExplorerTreeToggle,
                ExplorerTreePrefetch,
                ExplorerTreeRefresh,
                ExplorerRevealCurrent,
                ExplorerCloseEditor,
                ExplorerPanelSetVisible,
                ExplorerPanelWidth,
                ExplorerGoto,
                ExplorerSearchOpen,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(ExplorerCollapseAll,)>::default())
            .add_systems(
                Update,
                (
                    emit_outline_markdown,
                    clear_outline_on_file_change,
                    apply_global_search_requests,
                    emit_global_search.after(apply_global_search_requests),
                ),
            )
            .add_observer(on_explorer_goto)
            .add_observer(on_explorer_search_open);
    }
}

#[derive(Component, Clone, Debug)]
pub struct FileView {
    pub path: PathBuf,
}

impl FileView {
    fn in_stack(
        stack: Entity,
        children_q: &Query<&Children>,
        views: &Query<NavigableFileView>,
    ) -> Option<Entity> {
        let Ok(children) = children_q.get(stack) else {
            return None;
        };
        children.iter().find(|&child| views.contains(child))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn navigate(
        &mut self,
        entity: Entity,
        path: PathBuf,
        top_line: u32,
        viewport: &mut FileViewport,
        metadata: &mut PageMetadata,
        manager: &mut crate::lsp::manager::LspManager,
        commands: &mut Commands,
    ) {
        let previous = std::mem::replace(&mut self.path, path);
        manager.close(&previous);
        metadata.title = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.to_string_lossy().to_string());
        metadata.url = self.url();
        viewport.top_row = top_line;
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
            .remove::<FileLoadTask>()
            .remove::<EditorKeymap>()
            .remove::<NoteSent>()
            .remove::<LspEditDirty>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LspOpened>()
            .remove::<crate::lsp::manager::LintRan>();
    }

    fn url(&self) -> String {
        url::Url::from_file_path(&self.path)
            .map(|url| url.to_string())
            .unwrap_or_else(|_| format!("file://{}", self.path.to_string_lossy()))
    }

    pub(crate) fn display_path(&self) -> String {
        if let Ok(cwd) = std::env::current_dir()
            && let Ok(relative) = self.path.strip_prefix(&cwd)
        {
            return relative.to_string_lossy().to_string();
        }
        if let Some(home) = std::env::home_dir()
            && let Ok(relative) = self.path.strip_prefix(&home)
        {
            return format!("~/{}", relative.to_string_lossy());
        }
        self.path.to_string_lossy().to_string()
    }

    pub(crate) fn raw_media_url(&self) -> String {
        let mut url = self.url();
        url.push_str("?vmux-raw=1");
        url
    }
}

#[derive(Component, Clone, Debug)]
pub struct FileBuffer {
    pub language: String,
}

impl FileBuffer {
    fn failed(reason: LoadFailure, message: String) -> Self {
        Self {
            language: format!("{}{message}", reason.marker()),
        }
    }

    pub(crate) fn load_error(&self) -> Option<(bool, &str)> {
        LoadFailure::parse(&self.language)
            .map(|(reason, message)| (reason == LoadFailure::Undecodable, message))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoadFailure {
    Fatal,
    Undecodable,
}

impl LoadFailure {
    const FATAL: &'static str = "__error__:";
    const UNDECODABLE: &'static str = "__undecodable__:";

    fn marker(self) -> &'static str {
        match self {
            Self::Fatal => Self::FATAL,
            Self::Undecodable => Self::UNDECODABLE,
        }
    }

    fn parse(language: &str) -> Option<(Self, &str)> {
        if let Some(message) = language.strip_prefix(Self::UNDECODABLE) {
            return Some((Self::Undecodable, message));
        }
        let message = language.strip_prefix(Self::FATAL)?;
        Some((Self::Fatal, message))
    }
}

#[derive(Component, Clone, Debug)]
pub struct FileDir {
    pub entries: Vec<FileDirEntry>,
}

#[derive(Component)]
struct FileLoadTask {
    path: PathBuf,
    task: Task<FileLoad>,
}

#[cfg(test)]
impl FileLoadTask {
    fn settle(app: &mut App, entity: Entity) {
        for _ in 0..10_000 {
            app.update();
            let world = app.world();
            let loaded = world.get::<FileLoadTask>(entity).is_none()
                && (world.get::<FileBuffer>(entity).is_some()
                    || world.get::<FileDir>(entity).is_some()
                    || world.get::<FileMedia>(entity).is_some()
                    || world.get::<EditState>(entity).is_some());
            if loaded {
                return;
            }
            std::thread::yield_now();
        }
        panic!("file load did not settle");
    }
}

enum FileLoad {
    Directory(Vec<FileDirEntry>),
    Media {
        kind: vmux_core::media::MediaKind,
        mime: String,
    },
    Text {
        decoded: crate::encoding::DecodedText,
        heavy: bool,
    },
    Failed {
        reason: LoadFailure,
        message: String,
        missing: bool,
    },
}

impl FileLoad {
    fn read(path: &Path, forced: Option<FileEncoding>) -> Self {
        let metadata = std::fs::metadata(path);
        if metadata.as_ref().is_ok_and(|metadata| metadata.is_dir()) {
            return Self::Directory(list_dir(path));
        }

        let path_text = path.to_string_lossy();
        if let Some(kind) = vmux_core::media::media_kind(&path_text) {
            let mime = vmux_core::media::media_mime(&path_text)
                .unwrap_or("application/octet-stream")
                .to_string();
            return Self::Media { kind, mime };
        }

        let size = match metadata {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                return Self::Failed {
                    reason: LoadFailure::Fatal,
                    message: format!("cannot open {}: {error}", path.display()),
                    missing: error.kind() == std::io::ErrorKind::NotFound,
                };
            }
        };
        if size > crate::highlight::FILE_VIEW_MAX_BYTES {
            return Self::Failed {
                reason: LoadFailure::Fatal,
                message: format!(
                    "file too large ({size} bytes, max {})",
                    crate::highlight::FILE_VIEW_MAX_BYTES
                ),
                missing: false,
            };
        }

        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Self::Failed {
                    reason: LoadFailure::Fatal,
                    message: format!("cannot read {}: {error}", path.display()),
                    missing: error.kind() == std::io::ErrorKind::NotFound,
                };
            }
        };
        let decoded = match forced {
            Some(encoding) => crate::encoding::DecodedText::forced(&bytes, encoding),
            None => match crate::encoding::DecodedText::decode(&bytes) {
                Some(decoded) => decoded,
                None => {
                    return Self::Failed {
                        reason: LoadFailure::Undecodable,
                        message: format!("not a text file: {}", path.display()),
                        missing: false,
                    };
                }
            },
        };
        Self::Text {
            decoded,
            heavy: size > crate::highlight::HIGHLIGHT_MAX_BYTES,
        }
    }
}

#[derive(Component)]
pub struct EditState {
    pub core: EditCore,
    pub hl: HighlightCache,
    pub folds: crate::fold::FoldState,
    indent_width: u16,
    parsed_note: Option<crate::markdown::ParsedNote>,
    wrap_generation: u64,
    wrap_cache: Option<CachedWrapView>,
}

impl EditState {
    pub(crate) fn new(core: EditCore, hl: HighlightCache, folds: crate::fold::FoldState) -> Self {
        let parsed_note = crate::markdown::is_markdown_path(&core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&core.buffer.text()));
        let indent_width = crate::shape::BufferShape::detect(&core.buffer.rope)
            .indent
            .width;
        Self {
            core,
            hl,
            folds,
            indent_width,
            parsed_note,
            wrap_generation: 0,
            wrap_cache: None,
        }
    }

    pub(crate) fn parsed_note(&self) -> Option<crate::markdown::ParsedNote> {
        self.parsed_note.clone()
    }

    pub(crate) fn cursor_line(&self) -> u32 {
        self.core.cursor_pos().line
    }

    pub(crate) fn indent_width(&self) -> u16 {
        self.indent_width
    }

    pub(crate) fn wrapped_view<'a>(&'a mut self, viewport: &FileViewport) -> &'a WrapView {
        let stale = self.wrap_cache.as_ref().is_none_or(|cache| {
            cache.generation != self.wrap_generation
                || cache.mode != viewport.word_wrap
                || cache.viewport_columns != viewport.wrap_columns
                || cache.word_wrap_column != viewport.word_wrap_column
        });
        if stale {
            let total = self.core.buffer.len_lines() as u32;
            let folds = self.folds.view(total);
            self.wrap_cache = Some(CachedWrapView {
                generation: self.wrap_generation,
                mode: viewport.word_wrap,
                viewport_columns: viewport.wrap_columns,
                word_wrap_column: viewport.word_wrap_column,
                view: WrapView::new(
                    &self.core.buffer.rope,
                    &folds,
                    viewport.word_wrap,
                    viewport.wrap_columns,
                    viewport.word_wrap_column,
                ),
            });
        }
        &self.wrap_cache.as_ref().expect("wrap cache").view
    }

    pub(crate) fn sync_fold_view(&mut self) {
        let total = self.core.buffer.len_lines() as u32;
        self.core.fold_view = self.folds.view(total);
        self.wrap_generation = self.wrap_generation.wrapping_add(1);
    }

    fn refresh_parsed_note(&mut self) {
        self.parsed_note = crate::markdown::is_markdown_path(&self.core.buffer.path)
            .then(|| crate::markdown::parse_note_document(&self.core.buffer.text()));
    }
}

#[derive(Component, Default)]
pub(super) struct ParkedEdits {
    by_path: HashMap<PathBuf, ParkedEdit>,
    recent: Vec<PathBuf>,
}

struct ParkedEdit {
    edit: EditState,
    diff: vmux_git::GitDiffSource,
    modified: Option<std::time::SystemTime>,
}

impl ParkedEdits {
    const CAPACITY: usize = 8;

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

    fn resume(&mut self, path: &Path) -> Option<ParkedEdit> {
        let parked = self.by_path.remove(path)?;
        self.recent.retain(|p| p != path);
        if parked.edit.core.dirty || parked.modified == Self::modified_at(path) {
            return Some(parked);
        }
        None
    }

    pub(super) fn is_dirty(&self, path: &Path) -> bool {
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
pub struct EditorKeymap(pub Box<dyn Keymap>);

impl EditorKeymap {
    pub(crate) fn configured_kind(
        settings: &Option<Res<vmux_setting::AppSettings>>,
    ) -> vmux_core::KeymapKind {
        settings
            .as_ref()
            .map(|settings| settings.editor.keymap)
            .unwrap_or_default()
    }
}

#[derive(Component)]
struct LspEditDirty;

struct ClipboardHandle(Option<arboard::Clipboard>);

#[derive(Default)]
struct SelfWrites(std::collections::HashMap<PathBuf, std::time::Instant>);

#[derive(Component, Default)]
pub(crate) struct ExplorerState {
    pub root: PathBuf,
    pub open_editors: Vec<PathBuf>,
    pub focus_path: Option<PathBuf>,
    pub(super) active_editor: Option<PathBuf>,
    pub(super) active_editor_is_dir: bool,
}

impl ExplorerState {
    pub(super) fn allows(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    pub(super) fn close_editor(&mut self, path: &Path) -> Option<PathBuf> {
        let at = self.open_editors.iter().position(|open| open == path)?;
        self.open_editors.remove(at);
        let neighbour = at.min(self.open_editors.len().saturating_sub(1));
        self.open_editors.get(neighbour).cloned()
    }
}

#[derive(Component)]
struct OutlineDirty;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct GlobalSearchRequest {
    pub target_path: PathBuf,
    pub root: String,
    pub query: String,
    pub files: Vec<ExplorerSearchFile>,
    pub capped: bool,
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
type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);
type UnloadedFileView = (
    Without<FileBuffer>,
    Without<FileDir>,
    Without<FileMedia>,
    Without<EditState>,
    Without<FileLoadTask>,
);
type UnloadedFile = (
    Entity,
    &'static FileView,
    Option<&'static mut ParkedEdits>,
    Option<&'static ForcedEncoding>,
);
type EncodingTarget = (
    &'static FileView,
    Option<&'static mut EditState>,
    Option<&'static EditorKeymap>,
    Option<&'static mut FileViewport>,
    Option<&'static mut vmux_git::GitDiffSource>,
);
type OutlineDirtyReady = (With<OutlineDirty>, With<vmux_core::page::PageReady>);
type GlobalSearchDirtyReady = (
    With<GlobalSearchState>,
    With<GlobalSearchDirty>,
    With<vmux_core::page::PageReady>,
);
type NavigableFileView = (
    &'static mut FileView,
    &'static mut FileViewport,
    &'static mut PageMetadata,
);

fn new_file_view_bundle(url: &str, path: PathBuf) -> impl Bundle {
    let title = if url.starts_with("vmux://") {
        url.to_string()
    } else {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    };
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
            vmux_core::host::page::BindsEditingChords,
            vmux_core::host::page::HostHistory::default(),
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
    let path = vmux_core::file_url::FileUrl::parse(url)?.path()?;
    Some(new_file_view_bundle(url, path))
}

pub fn handle_file_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut views: Query<NavigableFileView>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    effective_startup_dir: Option<Res<vmux_layout::settings::EffectiveStartupDir>>,
    mut commands: Commands,
    mut record_writer: MessageWriter<vmux_core::event::RecordVisitRequest>,
) {
    for (entity, task) in &tasks {
        let project_dir = effective_startup_dir
            .as_deref()
            .and_then(|effective| effective.0.as_ref())
            .and_then(|(_, path)| path.as_deref());
        let knowledge_root = vmux_core::knowledge::KnowledgeVault::user().into_root();
        let Some(target) = FilePageTarget::resolve(&task.url, project_dir, &knowledge_root) else {
            continue;
        };
        let Some(path) = target.path else {
            commands.entity(entity).insert(PageOpenError {
                message: target.error,
            });
            continue;
        };
        let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();
        let page_url = if clean_url.trim_end_matches('/')
            == vmux_core::knowledge::KNOWLEDGE_PAGE_URL.trim_end_matches('/')
        {
            FileView { path: path.clone() }.url()
        } else {
            clean_url.clone()
        };
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
        let view = match FileView::in_stack(task.stack, &children_q, &views) {
            Some(view) => {
                if let Ok((mut fv, mut viewport, mut metadata)) = views.get_mut(view)
                    && fv.path != path
                {
                    fv.navigate(
                        view,
                        path,
                        0,
                        &mut viewport,
                        &mut metadata,
                        &mut manager,
                        &mut commands,
                    );
                }
                if let Ok((_, _, mut metadata)) = views.get_mut(view)
                    && page_url.starts_with("vmux://")
                {
                    metadata.title.clone_from(&page_url);
                    metadata.url = page_url.clone();
                    metadata.icon = vmux_core::PageIcon::None;
                }
                view
            }
            None => {
                vmux_layout::stack::Stack::clear_children(task.stack, &children_q, &mut commands);
                commands
                    .spawn((new_file_view_bundle(&page_url, path), ChildOf(task.stack)))
                    .id()
            }
        };
        if let Some(pg) = pending {
            commands.entity(view).insert(pg);
        }
        commands.entity(entity).insert(PageOpenHandled);
    }
}

struct FilePageTarget {
    path: Option<PathBuf>,
    error: String,
}

impl FilePageTarget {
    fn resolve(url: &str, project_dir: Option<&Path>, knowledge_root: &Path) -> Option<Self> {
        if url.trim_end_matches('/') == vmux_api::space::PROJECTS_PAGE_URL.trim_end_matches('/') {
            return Some(Self {
                path: Some(
                    project_dir
                        .map(Path::to_path_buf)
                        .unwrap_or_else(vmux_core::profile::projects_dir),
                ),
                error: String::new(),
            });
        }
        if url.trim_end_matches('/')
            == vmux_core::knowledge::KNOWLEDGE_PAGE_URL.trim_end_matches('/')
        {
            return Some(Self {
                path: Some(knowledge_root.to_path_buf()),
                error: String::new(),
            });
        }
        if !url.starts_with("file:") {
            return None;
        }
        Some(Self {
            path: vmux_core::file_url::FileUrl::parse(url).and_then(|file| file.path()),
            error: format!("malformed file URL '{url}'"),
        })
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

fn load_file_buffers(
    mut q: Query<UnloadedFile, UnloadedFileView>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    for (entity, fv, mut parked, forced) in &mut q {
        let forced = forced.and_then(|f| f.for_path(&fv.path));
        let kind = EditorKeymap::configured_kind(&settings);
        let (maps, leader) = settings_mappings(&settings);
        let markdown = crate::markdown::is_markdown_path(&fv.path);
        if forced.is_none()
            && let Some(parked) = parked.as_mut()
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
        let path = fv.path.clone();
        let task_path = path.clone();
        let wake = proxy.as_deref().map(|wrapper| (**wrapper).clone());
        let task = IoTaskPool::get().spawn(async move {
            let loaded = FileLoad::read(&path, forced);
            if let Some(proxy) = wake {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
            loaded
        });
        commands.entity(entity).insert(FileLoadTask {
            path: task_path,
            task,
        });
    }
}

fn apply_loaded_file_buffers(
    mut q: Query<(Entity, &FileView, &mut FileLoadTask)>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    store: Option<NonSend<crate::fold_store::FoldStore>>,
    mut commands: Commands,
) {
    for (entity, view, mut pending) in &mut q {
        let Some(loaded) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        if pending.path != view.path {
            commands.entity(entity).remove::<FileLoadTask>();
            continue;
        }
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<FileLoadTask>();
        match loaded {
            FileLoad::Directory(entries) => {
                entity_commands
                    .remove::<MissingFileView>()
                    .insert(FileDir { entries });
            }
            FileLoad::Media { kind, mime } => {
                entity_commands
                    .remove::<MissingFileView>()
                    .insert(FileMedia { kind, mime });
            }
            FileLoad::Failed {
                reason,
                message,
                missing,
            } => {
                entity_commands.insert(FileBuffer::failed(reason, message));
                if missing {
                    entity_commands.insert(MissingFileView);
                } else {
                    entity_commands.remove::<MissingFileView>();
                }
            }
            FileLoad::Text { decoded, heavy } => {
                let kind = EditorKeymap::configured_kind(&settings);
                let (maps, leader) = settings_mappings(&settings);
                let markdown = crate::markdown::is_markdown_path(&view.path);
                let crate::encoding::DecodedText { text, encoding } = decoded;
                let hl = match heavy {
                    true => HighlightCache::plain(&view.path),
                    false => HighlightCache::new(&view.path),
                };
                let mut core = EditCore::new(
                    view.path.clone(),
                    hl.language.clone(),
                    &text,
                    kind.initial_mode(),
                );
                core.buffer.encoding = encoding;
                let mut folds = crate::fold::FoldState::default();
                if !heavy {
                    folds.set_regions(crate::fold::indent_regions(&core.buffer.rope));
                    if let Some(store) = &store {
                        folds.collapsed.extend(store.get(&view.path));
                        folds.reconcile();
                    }
                }
                core.fold_view = folds.view(core.buffer.len_lines() as u32);
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
        kind: EditorKeymap::configured_kind(&settings),
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
            EditorCursor::emit(
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
        .remove::<ExplorerPanelSent>()
        .insert(ExplorerTreeDirty)
        .insert(OpenEditorsDirty);
    if crate::explorer_model::is_markdown(&fv.path) {
        commands.entity(entity).insert(OutlineDirty);
    }
}

#[allow(clippy::too_many_arguments)]
fn on_file_shape_set(
    trigger: On<BinReceive<FileShapeSet>>,
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
    let wanted = trigger.event().payload;
    let Ok((mut edit, keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
        return;
    };
    run_commands(
        entity,
        vec![EditCommand::Reshape(crate::shape::BufferShape {
            indent: wanted.indent,
            line_ending: wanted.line_ending,
        })],
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
    let shape = crate::shape::BufferShape::detect(&edit.core.buffer.rope);
    edit.indent_width = shape.indent.width;
    if !browsers.can_emit_to(&entity) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_event(
        entity,
        &FileShapeEvent {
            indent: shape.indent,
            line_ending: shape.line_ending,
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn on_file_encoding_set(
    trigger: On<BinReceive<FileEncodingSet>>,
    mut q: Query<EncodingTarget>,
    mut clipboard: NonSendMut<ClipboardHandle>,
    mut self_writes: NonSendMut<SelfWrites>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let wanted = trigger.event().payload;
    let Ok((fv, edit, keymap, vp, diff_source)) = q.get_mut(entity) else {
        return;
    };
    if wanted.action == FileEncodingAction::Reopen {
        commands
            .entity(entity)
            .insert(ForcedEncoding {
                path: fv.path.clone(),
                encoding: wanted.encoding,
            })
            .remove::<EditState>()
            .remove::<vmux_git::GitDiffSource>()
            .remove::<FileBuffer>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LintRan>();
        manager.change(&fv.path);
        return;
    }
    let (Some(mut edit), Some(keymap), Some(mut vp), Some(mut diff_source)) =
        (edit, keymap, vp, diff_source)
    else {
        return;
    };
    edit.core.buffer.encoding = wanted.encoding;
    run_commands(
        entity,
        vec![EditCommand::Save],
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
    if !browsers.can_emit_to(&entity) {
        return;
    }
    commands.trigger(BinHostEmitEvent::from_event(
        entity,
        &FileEncodingEvent {
            encoding: edit.core.buffer.encoding,
        },
    ));
}

#[derive(Component)]
struct FileReloadRequested;

#[derive(Component, Clone)]
struct ForcedEncoding {
    path: PathBuf,
    encoding: FileEncoding,
}

impl ForcedEncoding {
    fn for_path(&self, path: &Path) -> Option<FileEncoding> {
        match self.path == path {
            true => Some(self.encoding),
            false => None,
        }
    }
}

#[derive(Component)]
struct MissingFileView;

struct FileWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    dirs: HashSet<PathBuf>,
}

pub(crate) fn canon(p: &Path) -> PathBuf {
    vmux_path::PathIdentity::resolve(p).into_path_buf()
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
    trees: Res<ExplorerTrees>,
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
    for dir in trees.expanded_dirs() {
        ensure_file_watch(&mut watch, dir.clone());
    }
}

fn drain_file_changes(
    watch: Option<NonSend<FileWatch>>,
    self_writes: Option<NonSendMut<SelfWrites>>,
    views: Query<(Entity, &FileView, Has<MissingFileView>)>,
    mut trees: ResMut<ExplorerTrees>,
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
    let mut changed_dirs: HashSet<PathBuf> = HashSet::new();
    for path in &changed {
        if let Some(parent) = path.parent() {
            changed_dirs.insert(canon(parent));
        }
    }
    for root in trees.roots() {
        let cached: Vec<PathBuf> = trees.at(&root).children.keys().cloned().collect();
        for d in cached {
            if !changed_dirs.contains(&canon(&d)) {
                continue;
            }
            trees.start_dir_load(&root, d, &mut commands, true);
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
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &FileDirEvent {
                        path: fv.display_path(),
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
                let url = format!("{}&v={nonce}", fv.raw_media_url());
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
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
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &FileExternalChange {
                        path: fv.display_path(),
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
    commands.trigger(BinHostEmitEvent::from_event(
        entity,
        &FileCompletionEvent {
            items,
            replace_from_col,
            line,
        },
    ));
    true
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
    let top_before = vp.top_row;
    let mut text_changed = false;
    let mut cursor_stale = false;
    let mut dirty_changed = false;
    let mut fold_changed = false;
    for cmd in cmds {
        if let EditCommand::ScrollViewport(lines) = &cmd {
            let visible = vp.visible_rows(edit);
            let target = (vp.top_row as i64 + *lines as i64).clamp(0, u32::MAX as i64) as u32;
            let target = clamp_top_line(target, visible, vp.rows);
            vp.scroll_to(target, entity, browsers, commands);
            edit.core.top_row = vp.top_row;
            if vp.follow_scrolled_cursor(edit) {
                cursor_stale = true;
            }
            continue;
        }
        if let EditCommand::ScrollCursorTo(placement) = &cmd {
            let row = edit
                .folds
                .view(edit.core.buffer.len_lines() as u32)
                .buffer_to_row(edit.core.cursor_pos().line);
            let rows = vp.rows.max(1) as u32;
            let target = match placement {
                crate::edit::command::ScrollPlacement::Top => row,
                crate::edit::command::ScrollPlacement::Center => row.saturating_sub(rows / 2),
                crate::edit::command::ScrollPlacement::Bottom => row.saturating_sub(rows - 1),
            };
            vp.scroll_to(target, entity, browsers, commands);
            edit.core.top_row = vp.top_row;
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
            edit.sync_fold_view();
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
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &vmux_core::event::FileRenameBeginEvent {
                        line,
                        col: ccol as u32,
                        current,
                    },
                ));
                continue;
            }
            EditCommand::ClearSearchHighlight => {
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &vmux_core::event::FileKey::FindClose,
                ));
                cursor_stale = true;
            }
            EditCommand::OpenFind { forward } => {
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
                    &vmux_core::event::FileKey::Find { forward: *forward },
                ));
                continue;
            }
            EditCommand::OpenCommandLine => {
                commands.write_message(vmux_command::host::command::AppCommand::Browser(
                    vmux_command::host::command::BrowserCommand::Bar(
                        vmux_command::host::command::BrowserBarCommand::OpenExBar,
                    ),
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
            let encoding = edit.core.buffer.encoding;
            let bytes = match (crate::encoding::Reencode { encoding }).applied(&body) {
                Ok(bytes) => bytes,
                Err(unmappable) => {
                    tracing::warn!(path = %path.display(), "editor save refused: {unmappable}");
                    if browsers.can_emit_to(&entity) {
                        commands.trigger(BinHostEmitEvent::from_event(
                            entity,
                            &FileErrorEvent {
                                message: format!("save failed: {unmappable}"),
                                undecodable: false,
                            },
                        ));
                    }
                    continue;
                }
            };
            match vmux_path::AtomicFile::write(&path, &bytes) {
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
                        commands.trigger(BinHostEmitEvent::from_event(
                            entity,
                            &FileErrorEvent {
                                message: format!("save failed: {e}"),
                                undecodable: false,
                            },
                        ));
                    }
                }
            }
            continue;
        }
        if matches!(cmd, EditCommand::Paste) {
            let Some(cb) = clipboard.0.as_mut() else {
                continue;
            };
            let Ok(s) = cb.get_text() else {
                continue;
            };
            if edit.core.paste(&s) {
                text_changed = true;
                let (l, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
                edit.hl.invalidate_from(l.saturating_sub(1));
            }
            cursor_stale = true;
            dirty_changed = true;
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
        cursor_stale |= out.sel_changed || out.mode_changed;
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
        edit.sync_fold_view();
    }
    {
        let total = edit.core.buffer.len_lines() as u32;
        let caret_line = edit.core.cursor_pos().line;
        if edit.folds.view(total).is_hidden(caret_line) {
            edit.folds.reveal(caret_line);
            edit.sync_fold_view();
            fold_changed = true;
        }
    }
    if let Some(top) = vp.autoscroll(edit) {
        vp.scroll_to(top, entity, browsers, commands);
        edit.core.top_row = vp.top_row;
    }
    let vpc = *vp;
    if text_changed || fold_changed || vpc.left_render_band(top_before) {
        EditorWindow::emit(entity, edit, &vpc, browsers, commands);
    }
    if text_changed || cursor_stale || fold_changed {
        EditorCursor::emit(entity, edit, keymap, &vpc, browsers, commands);
    }
    if fold_changed {
        commands.entity(entity).insert(FoldsDirty);
    }
    if dirty_changed {
        commands.entity(entity).insert(OpenEditorsDirty);
    }
    if text_changed || dirty_changed {
        diff_source.content = edit.core.buffer.text();
        diff_source.dirty = edit.core.dirty;
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
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
    let mut out = Vec::with_capacity(cmds.len() * 2);
    for cmd in cmds {
        if let EditCommand::ScrollViewport(lines) = cmd {
            out.push(EditCommand::ScrollViewport(lines.saturating_mul(2)));
            continue;
        }
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
        if accelerate {
            out.push(cmd.clone());
        }
        out.push(cmd);
    }
    out
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
    let updated = match vmux_core::knowledge::Frontmatter::from(text.as_str())
        .apply(&trigger.event().payload)
    {
        Ok(updated) => updated,
        Err(message) => {
            commands.trigger(BinHostEmitEvent::from_event(
                entity,
                &FileErrorEvent {
                    message,
                    undecodable: false,
                },
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
        let refusal = match WorkspaceEditPlan::within(&awaiting.root, &awaiting.params.edit) {
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

    for rename in renames.read() {
        let refusal = match &rename.result {
            Err(reason) => Some(reason.clone()),
            Ok(edit) => match WorkspaceEditPlan::within(&rename.root, edit) {
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
            commands.trigger(BinHostEmitEvent::from_event(
                rename.entity,
                &vmux_core::event::FileEditFailedEvent { reason },
            ));
        }
    }
}

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
        let wanted = canon(document.path.as_path());
        if let (Some(expected), Some(actual)) = (
            document.version,
            manager.document_version(document.path.as_path()),
        ) && expected != actual
        {
            return Some(format!(
                "{} changed since the edit was computed",
                document.path.as_path().display()
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

        let mut texts = open
            .iter()
            .filter_map(|entity| views.get(*entity).ok())
            .map(|(_, _, edit, ..)| edit.core.buffer.text());
        let first = texts.next().unwrap_or_default();
        if texts.any(|text| text != first) {
            return Some(format!(
                "{} is open more than once with different contents",
                document.path.as_path().display()
            ));
        }

        for entity in open {
            let Ok((_, _, mut edit, keymap, mut vp, mut diff_source)) = views.get_mut(entity)
            else {
                continue;
            };
            let updated = match edit.core.buffer.with_lsp_edits(&document.edits) {
                Ok(updated) => updated,
                Err(e) => return Some(format!("{}: {e}", document.path.as_path().display())),
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

fn edit_closed_file(
    document: &crate::lsp::workspace_edit::PlannedDocument,
    self_writes: &mut SelfWrites,
) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(document.path.as_path()) else {
        return Err(format!(
            "{} could not be read",
            document.path.as_path().display()
        ));
    };
    let buffer = crate::edit::buffer::TextBuffer::from_text(
        document.path.as_path().to_path_buf(),
        String::new(),
        &text,
    );
    let updated = match buffer.with_lsp_edits(&document.edits) {
        Ok(updated) => updated,
        Err(e) => return Err(format!("{}: {e}", document.path.as_path().display())),
    };
    self_writes
        .0
        .insert(canon(document.path.as_path()), std::time::Instant::now());
    vmux_path::AtomicFile::write(document.path.as_path(), updated.as_bytes())
        .map_err(|e| format!("{}: {e}", document.path.as_path().display()))
}

#[allow(clippy::too_many_arguments)]
fn run_submitted_ex_lines(
    mut submitted: MessageReader<vmux_command::host::ExLineSubmitted>,
    children: Query<&Children>,
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
    for message in submitted.read() {
        let cmds = crate::edit::ex::ExLine::edits(&message.line);
        if cmds.is_empty() {
            continue;
        }
        let Some(stack) = message.stack else {
            continue;
        };
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(entity) = kids.iter().find(|child| q.contains(*child)) else {
            continue;
        };
        let Ok((mut edit, keymap, mut vp, mut diff_source)) = q.get_mut(entity) else {
            continue;
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
}

fn on_file_find_request(
    trigger: On<BinReceive<FileFindRequest>>,
    mut q: Query<(&mut EditState, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = trigger.event().payload.clone();
    let Ok((mut edit, keymap, vp)) = q.get_mut(entity) else {
        return;
    };
    if request.done || request.query.is_empty() {
        edit.core.apply(EditCommand::ClearSearchHighlight);
    } else if request.step {
        edit.core.apply(EditCommand::Move(Motion::SearchNext {
            reverse: request.reverse,
        }));
    } else {
        let pattern = match request.regex {
            true => crate::edit::search::translate(&request.query),
            false => regex::escape(&request.query),
        };
        edit.core.apply(EditCommand::SetSearch {
            pattern,
            forward: request.forward,
        });
    }
    EditorCursor::emit(
        entity,
        &mut edit,
        keymap.0.as_ref(),
        vp,
        &browsers,
        &mut commands,
    );
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
    let (line, utf16, _, col) = req_pos(edit, req.line, req.col);
    manager.hover(entity, &edit.core.buffer.path, line, utf16, col);
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

fn req_pos(edit: &EditState, line: u32, cell: u32) -> (u32, u32, String, u32) {
    let line = line.min(edit.core.buffer.len_lines().saturating_sub(1) as u32);
    let lt: String = edit
        .core
        .buffer
        .rope
        .line(line as usize)
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let col = DisplayCells::char_at(&lt, cell) as u32;
    let utf16 = crate::lsp::manager::char_to_utf16_col(&lt, col);
    (line, utf16, lt, col)
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
    let (line, utf16, _, _) = req_pos(edit, req.line, req.col);
    let path = edit.core.buffer.path.clone();
    manager.definition(entity, &path, line, utf16);
}

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
                commands.trigger(BinHostEmitEvent::from_event(
                    entity,
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
        EditorAction::Paste => vec![EditCommand::Paste],
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
    let Some((root, workspace_edit)) =
        manager.run_code_action(entity, trigger.event().payload.index as usize, &path)
    else {
        return;
    };
    edits.write(crate::lsp::manager::LspRequestedEdit {
        entity,
        root,
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
    let (line, utf16, _, _) = req_pos(edit, req.line, req.col);
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
    let (line, utf16, _, _) = req_pos(edit, req.line, req.col);
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
    let (line, utf16, lt, col) = req_pos(edit, req.line, req.col);
    let replace_from = word_start_col(&lt, col as usize);
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

fn goto_caret(
    entity: Entity,
    edit: &mut EditState,
    line: u32,
    utf16_col: u32,
    vp: &mut FileViewport,
    browsers: &Browsers,
    commands: &mut Commands,
) {
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
    if let Some(top) = vp.autoscroll(edit) {
        vp.scroll_to(top, entity, browsers, commands);
        edit.core.top_row = vp.top_row;
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
            goto_caret(
                g.entity,
                &mut edit,
                g.line,
                g.utf16_col,
                &mut vp,
                &browsers,
                &mut commands,
            );
            let vpc = *vp;
            EditorWindow::emit(g.entity, &mut edit, &vpc, &browsers, &mut commands);
            EditorCursor::emit(
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
        goto_caret(
            entity,
            &mut edit,
            pg.line,
            pg.utf16_col,
            &mut vp,
            &browsers,
            &mut commands,
        );
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
        EditorWindow::emit(entity, &mut edit, &vpc, &browsers, &mut commands);
        EditorCursor::emit(
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
    let col = edit.core.char_at_cell(p.line as usize, p.col);
    let at = edit.core.buffer.coords_to_char(p.line as usize, col);
    if p.add {
        edit.core.toggle_caret(at);
    } else if p.extend {
        let anchor = edit.core.primary().anchor;
        edit.core.selections = vec![Selection { anchor, head: at }];
    } else {
        edit.core.collapse_carets();
        edit.core.set_caret(at);
    }
    if let Some(command) = keymap.0.pointer_selection_mode(p.extend) {
        edit.core.apply(command);
    }
    EditorCursor::emit(
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
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
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
            commands.trigger(BinHostEmitEvent::from_event(
                entity,
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
    panel: Res<ExplorerPanelDefaults>,
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
            .unwrap_or(panel.default_visible);
        if !explorer_visible {
            commands
                .entity(scope)
                .insert(StackExplorerVisibility { visible: true });
            for (view, _, parent) in &views {
                let view_scope = parent.map(ChildOf::parent).unwrap_or(view);
                if view_scope == scope {
                    commands.entity(view).remove::<ExplorerPanelSent>();
                }
            }
        }
        let request = pending_request.request;
        commands.entity(entity).insert((
            GlobalSearchState(ExplorerSearchEvent {
                root: request.root,
                query: request.query,
                files: request.files,
                capped: request.capped,
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
        commands.trigger(BinHostEmitEvent::from_event(entity, &search.0));
        commands.entity(entity).remove::<GlobalSearchDirty>();
    }
}

fn on_explorer_search_open(
    trigger: On<BinReceive<ExplorerSearchOpen>>,
    mut views: Query<NavigableFileView>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    let Ok((mut view, mut viewport, mut metadata)) = views.get_mut(entity) else {
        return;
    };
    view.navigate(
        entity,
        PathBuf::from(&request.path),
        request.line.saturating_sub(1),
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

pub const FILES_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "files",
    title: "Files",
    title_message_id: None,
    replaces_command: None,
    keywords: &["file", "open"],
    icon: Some(vmux_core::BuiltinIcon::Files),
    command_bar: true,
};

pub const PROJECTS_PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "projects",
    title: "Projects",
    title_message_id: Some("layout-projects"),
    replaces_command: None,
    keywords: &["project", "files", "folder", "open"],
    icon: Some(vmux_core::BuiltinIcon::Project),
    command_bar: true,
};

#[cfg(test)]
mod edit_flow_tests {
    use super::*;
    use crate::keymap::{KeyInput, KeymapKindExt, Mods};

    #[test]
    fn missing_file_view_loads_when_file_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("created-after-open");
        let path = parent.join("file.txt");
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(|_| {}).unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ExplorerTrees>()
            .add_systems(
                Update,
                (
                    reconcile_file_watches,
                    drain_file_changes,
                    reload_changed_files,
                    load_file_buffers,
                    apply_loaded_file_buffers,
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

        FileLoadTask::settle(&mut app, entity);
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
        FileLoadTask::settle(&mut app, entity);

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
    fn a_held_scroll_key_covers_the_same_ground_as_a_held_motion_key() {
        let held = |cmd| accelerate_repeated_navigation(vec![cmd], true);
        let rows = |cmds: Vec<EditCommand>| {
            cmds.iter()
                .map(|cmd| match cmd {
                    EditCommand::ScrollViewport(lines) => *lines,
                    EditCommand::Move(Motion::Down) => 1,
                    EditCommand::Move(Motion::Up) => -1,
                    other => panic!("unexpected {other:?}"),
                })
                .sum::<i32>()
        };

        assert_eq!(
            rows(held(EditCommand::ScrollViewport(1))),
            rows(held(EditCommand::Move(Motion::Down)))
        );
        assert_eq!(
            rows(held(EditCommand::ScrollViewport(-1))),
            rows(held(EditCommand::Move(Motion::Up)))
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
    use vmux_api::BinEvent;

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

    impl ExplorerTree {
        fn in_app<'a>(app: &'a App, root: &Path) -> &'a Self {
            app.world()
                .resource::<ExplorerTrees>()
                .by_root
                .get(root)
                .unwrap_or_else(|| panic!("no explorer tree for {}", root.display()))
        }
    }

    struct ExplorerApp;

    impl ExplorerApp {
        fn hidden() -> App {
            Self::with(false)
        }

        fn visible() -> App {
            Self::with(true)
        }

        fn with(default_visible: bool) -> App {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ExplorerTreePlugin))
                .insert_resource(ExplorerPanelDefaults {
                    default_visible,
                    width: 240,
                });
            app
        }
    }

    fn wait_for_children(app: &mut App, root: &Path, path: &Path) {
        for _ in 0..1000 {
            app.update();
            let loaded = app
                .world()
                .resource::<ExplorerTrees>()
                .by_root
                .get(root)
                .is_some_and(|tree| tree.children.contains_key(path));
            if loaded {
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
        let mut app = ExplorerApp::hidden();
        let e = app
            .world_mut()
            .spawn((FileView { path: file }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        assert_eq!(
            app.world().get::<ExplorerState>(e).unwrap().root.as_path(),
            tmp.path()
        );
        let tree = ExplorerTree::in_app(&app, tmp.path());
        assert!(tree.expanded.contains(&tmp.path().to_path_buf()));
        assert!(
            tree.children
                .get(tmp.path())
                .unwrap()
                .iter()
                .any(|x| x.name == "src")
        );
        assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
    }

    #[test]
    fn a_second_page_on_a_warm_root_reuses_the_loaded_tree() {
        let tmp = git_repo();
        let src = tmp.path().join("src");
        let mut app = ExplorerApp::hidden();
        let first = app
            .world_mut()
            .spawn((
                FileView {
                    path: tmp.path().join("README.md"),
                },
                ExplorerState::default(),
            ))
            .id();
        wait_for_children(&mut app, tmp.path(), &src);
        app.world_mut()
            .entity_mut(first)
            .remove::<ExplorerTreeDirty>();
        let second = app
            .world_mut()
            .spawn((
                FileView {
                    path: src.join("lib.rs"),
                },
                ExplorerState::default(),
            ))
            .id();
        app.update();
        assert!(
            app.world().get::<ExplorerTreeDirty>(second).is_some(),
            "a page joining a warm root must still be asked to draw its tree"
        );
        assert!(
            app.world().get::<ExplorerTreeDirty>(first).is_none(),
            "a page joining a warm root must not re-walk the directories others already hold"
        );
    }

    #[test]
    fn expansion_outlives_the_page_that_made_it() {
        let tmp = git_repo();
        let src = tmp.path().join("src");
        let mut app = ExplorerApp::hidden();
        let first = app
            .world_mut()
            .spawn((
                FileView {
                    path: tmp.path().join("README.md"),
                },
                ExplorerState::default(),
            ))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        toggle(&mut app, first, &src);
        wait_for_children(&mut app, tmp.path(), &src);
        app.world_mut().entity_mut(first).despawn();
        app.update();
        assert!(
            ExplorerTree::in_app(&app, tmp.path())
                .expanded
                .contains(&src),
            "closing a page must not collapse the workspace tree the next one opens on"
        );
    }

    #[test]
    fn pruning_drops_the_stalest_idle_trees_and_never_a_live_one() {
        let mut trees = ExplorerTrees::default();
        let roots: Vec<PathBuf> = (0..IDLE_TREE_CAPACITY + 2)
            .map(|n| PathBuf::from(format!("/project{n}")))
            .collect();
        for root in &roots {
            trees.at(root);
        }
        let live: HashSet<PathBuf> = [roots[0].clone()].into_iter().collect();

        trees.prune(&live);

        assert!(
            trees.by_root.contains_key(&roots[0]),
            "a root a page still shows must survive however stale it is"
        );
        assert!(
            !trees.by_root.contains_key(&roots[1]),
            "the stalest idle root is the one that goes"
        );
        assert!(trees.by_root.contains_key(roots.last().unwrap()));
        assert_eq!(trees.by_root.len(), IDLE_TREE_CAPACITY + 1);
    }

    #[test]
    fn expanding_warms_the_next_level_and_stops_there() {
        let tmp = git_repo();
        let deep = tmp.path().join("src").join("deep");
        fs::create_dir_all(&deep).unwrap();
        let mut app = ExplorerApp::hidden();
        app.world_mut().spawn((
            FileView {
                path: tmp.path().join("README.md"),
            },
            ExplorerState::default(),
        ));

        wait_for_children(&mut app, tmp.path(), tmp.path());
        wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));

        for _ in 0..200 {
            app.update();
            std::thread::yield_now();
        }
        let tree = ExplorerTree::in_app(&app, tmp.path());
        assert!(
            !tree.expanded.contains(&tmp.path().join("src")),
            "warming must not expand anything on the user's behalf"
        );
        assert!(
            !tree.children.contains_key(&deep),
            "a warmed directory must not warm its own children, or a deep tree loads itself"
        );
    }

    #[test]
    fn toggle_expands_then_collapses_subdir() {
        let tmp = git_repo();
        let file = tmp.path().join("README.md");
        let mut app = ExplorerApp::hidden();
        let e = app
            .world_mut()
            .spawn((FileView { path: file }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        let src = tmp.path().join("src");
        toggle(&mut app, e, &src);
        wait_for_children(&mut app, tmp.path(), &src);
        let tree = ExplorerTree::in_app(&app, tmp.path());
        assert!(tree.expanded.contains(&src));
        assert!(
            tree.children
                .get(&src)
                .unwrap()
                .iter()
                .any(|x| x.name == "lib.rs")
        );
        toggle(&mut app, e, &src);
        assert!(
            !ExplorerTree::in_app(&app, tmp.path())
                .expanded
                .contains(&src)
        );
    }

    #[test]
    fn reveal_current_expands_ancestors_and_focuses_file() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = ExplorerApp::hidden();
        let e = app
            .world_mut()
            .spawn((FileView { path: file.clone() }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerRevealCurrent,
        });
        let src = tmp.path().join("src");
        wait_for_children(&mut app, tmp.path(), &src);
        let tree = ExplorerTree::in_app(&app, tmp.path());
        assert!(tree.expanded.contains(tmp.path()));
        assert!(tree.expanded.contains(&src));
        assert_eq!(
            app.world()
                .get::<ExplorerState>(e)
                .unwrap()
                .focus_path
                .as_deref(),
            Some(file.as_path())
        );
    }

    #[test]
    fn repeated_reveal_skips_unchanged_tree_rebuild() {
        let tmp = git_repo();
        let file = tmp.path().join("src").join("lib.rs");
        let mut app = ExplorerApp::hidden();
        let e = app
            .world_mut()
            .spawn((FileView { path: file }, ExplorerState::default()))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerRevealCurrent,
        });
        wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));
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
    fn opening_a_file_reveals_it_without_an_explicit_request() {
        let tmp = git_repo();
        let src = tmp.path().join("src");
        let file = src.join("lib.rs");
        let mut app = ExplorerApp::visible();
        let e = app
            .world_mut()
            .spawn((
                FileView {
                    path: tmp.path().join("README.md"),
                },
                ExplorerState::default(),
            ))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        assert!(
            !ExplorerTree::in_app(&app, tmp.path())
                .expanded
                .contains(&src)
        );
        app.world_mut().get_mut::<FileView>(e).unwrap().path = file.clone();
        wait_for_children(&mut app, tmp.path(), &src);
        assert!(
            ExplorerTree::in_app(&app, tmp.path())
                .expanded
                .contains(&src)
        );
        assert_eq!(
            app.world()
                .get::<ExplorerState>(e)
                .unwrap()
                .focus_path
                .as_deref(),
            Some(file.as_path())
        );
    }

    #[test]
    fn opening_a_file_leaves_a_hidden_explorer_collapsed() {
        let tmp = git_repo();
        let src = tmp.path().join("src");
        let mut app = ExplorerApp::hidden();
        let e = app
            .world_mut()
            .spawn((
                FileView {
                    path: tmp.path().join("README.md"),
                },
                ExplorerState::default(),
            ))
            .id();
        wait_for_children(&mut app, tmp.path(), tmp.path());
        app.world_mut().get_mut::<FileView>(e).unwrap().path = src.join("lib.rs");
        for _ in 0..200 {
            app.update();
            std::thread::yield_now();
        }
        assert!(
            !ExplorerTree::in_app(&app, tmp.path())
                .expanded
                .contains(&src)
        );
        assert!(
            app.world()
                .get::<ExplorerState>(e)
                .unwrap()
                .focus_path
                .is_none()
        );
    }

    #[derive(Resource, Default)]
    struct SentReveals(Vec<ExplorerReveal>);

    impl SentReveals {
        fn watch(app: &mut App, webview: Entity) {
            let mut browsers = Browsers::default();
            browsers.set_externally_hosted(webview);
            app.insert_non_send(browsers)
                .init_resource::<Self>()
                .add_observer(Self::record);
        }

        fn record(emit: On<BinHostEmitEvent>, mut sent: ResMut<Self>) {
            if emit.id() != ExplorerFocusEvent::id() {
                return;
            }
            let decoded =
                rkyv::from_bytes::<ExplorerFocusEvent, rkyv::rancor::Error>(emit.payload());
            let Ok(event) = decoded else {
                return;
            };
            sent.0.push(event.reveal);
        }

        fn drain(app: &mut App) -> Vec<ExplorerReveal> {
            std::mem::take(&mut app.world_mut().resource_mut::<Self>().0)
        }
    }

    #[test]
    fn only_an_asked_for_reveal_may_take_focus_from_the_editor() {
        let tmp = git_repo();
        let src = tmp.path().join("src");
        let mut app = ExplorerApp::visible();
        let e = app
            .world_mut()
            .spawn((
                FileView {
                    path: tmp.path().join("README.md"),
                },
                ExplorerState::default(),
            ))
            .id();
        SentReveals::watch(&mut app, e);
        wait_for_children(&mut app, tmp.path(), tmp.path());
        let _ = SentReveals::drain(&mut app);
        app.world_mut().get_mut::<FileView>(e).unwrap().path = src.join("lib.rs");
        wait_for_children(&mut app, tmp.path(), &src);
        assert_eq!(
            SentReveals::drain(&mut app),
            vec![ExplorerReveal::Followed],
            "opening a file must not pull the caret out of the editor"
        );
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerRevealCurrent,
        });
        app.update();
        assert_eq!(
            SentReveals::drain(&mut app),
            vec![ExplorerReveal::Requested]
        );
    }

    #[test]
    fn collapse_all_leaves_the_root_expanded_and_nothing_else() {
        let tmp = git_repo();
        let src = tmp.path().join("src");
        let mut app = ExplorerApp::visible();
        let e = app
            .world_mut()
            .spawn((
                FileView {
                    path: src.join("lib.rs"),
                },
                ExplorerState::default(),
            ))
            .id();
        wait_for_children(&mut app, tmp.path(), &src);
        assert!(ExplorerTree::in_app(&app, tmp.path()).expanded.len() > 1);
        app.world_mut().entity_mut(e).remove::<ExplorerTreeDirty>();
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerCollapseAll,
        });
        app.update();
        assert_eq!(
            ExplorerTree::in_app(&app, tmp.path()).expanded,
            HashSet::from([tmp.path().to_path_buf()]),
            "dropping the root makes the next reveal look like a tree change and re-scroll"
        );
        assert!(app.world().get::<ExplorerTreeDirty>(e).is_some());
    }

    #[test]
    fn showing_the_panel_reveals_without_taking_the_caret() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ExplorerPanelPlugin))
            .init_resource::<ExplorerTrees>()
            .insert_resource(ExplorerPanelDefaults {
                default_visible: false,
                width: 240,
            });
        let stack = app
            .world_mut()
            .spawn(StackExplorerVisibility { visible: false })
            .id();
        let view = app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::from("/a.rs"),
                },
                ExplorerState::default(),
                ChildOf(stack),
            ))
            .id();
        SentReveals::watch(&mut app, view);

        app.world_mut().trigger(BinReceive {
            webview: view,
            payload: ExplorerPanelSetVisible {
                visible: true,
                client_id: 1,
                request_id: 1,
            },
        });
        app.update();

        assert_eq!(
            SentReveals::drain(&mut app),
            vec![ExplorerReveal::Followed],
            "opening the panel shows where you are; it does not move the keyboard there"
        );
    }

    #[test]
    fn panel_visibility_is_shared_only_within_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ExplorerPanelPlugin))
            .init_resource::<ExplorerTrees>();
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
                ExplorerPanelSent,
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
                ExplorerPanelSent,
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
                ExplorerPanelSent,
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
        assert!(app.world().get::<ExplorerPanelSent>(first).is_some());
        assert!(app.world().get::<ExplorerPanelSent>(peer).is_none());
        assert!(app.world().get::<ExplorerPanelSent>(other).is_some());

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
            .insert_resource(ExplorerPanelDefaults {
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
                files: Vec::new(),
                capped: false,
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
        let mut app = ExplorerApp::hidden();
        app.add_plugins(ExplorerPanelPlugin);
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
        wait_for_children(&mut app, tmp.path(), tmp.path());
        app.world_mut().trigger(BinReceive {
            webview: e,
            payload: ExplorerPanelSetVisible {
                visible: true,
                client_id: 9,
                request_id: 1,
            },
        });
        wait_for_children(&mut app, tmp.path(), &tmp.path().join("src"));
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
        app.add_plugins((MinimalPlugins, ExplorerPanelPlugin))
            .insert_resource(ExplorerPanelDefaults {
                default_visible: true,
                width: 240,
            });
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
        assert_eq!(app.world().resource::<ExplorerPanelDefaults>().width, 600);
    }

    #[test]
    fn open_editors_track_on_navigate_and_close() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src");
        std::fs::create_dir(&dir).unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ExplorerTabsPlugin))
            .insert_resource(crate::lsp::manager::LspManager::new(
                crate::lsp::LspOutbox::default(),
                crate::lsp::server_request::ServerEvents::default().sender(),
            ));
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
        assert_eq!(st.open_editors, vec![b.clone()]);
        app.world_mut().get_mut::<FileView>(e).unwrap().path = dir.clone();
        app.update();
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(
            st.open_editors,
            vec![b.clone(), dir],
            "a directory the reader navigated to needs a tab of its own, or the only way out \
             of the navigator is to open another file"
        );
        let c = PathBuf::from("/proj/c.rs");
        app.world_mut().get_mut::<FileView>(e).unwrap().path = c.clone();
        app.update();
        let st = app.world().get::<ExplorerState>(e).unwrap();
        assert_eq!(
            st.open_editors,
            vec![b, c],
            "opening a file from the directory navigator replaces that navigator tab"
        );
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
        app.add_plugins((MinimalPlugins, EditorNavigationPlugin, ExplorerTabsPlugin))
            .add_message::<vmux_core::event::RecordVisitRequest>()
            .insert_resource(crate::lsp::manager::LspManager::new(
                crate::lsp::LspOutbox::default(),
                crate::lsp::server_request::ServerEvents::default().sender(),
            ))
            .add_systems(Update, handle_file_page_open);
        app
    }

    struct EditorStack {
        app: App,
        stack: Entity,
    }

    impl EditorStack {
        fn empty() -> Self {
            let mut app = app();
            let stack = app.world_mut().spawn_empty().id();
            Self { app, stack }
        }

        fn showing(url: &str) -> Self {
            let mut stack = Self::empty();
            stack.open(url);
            stack
        }

        fn open(&mut self, url: &str) {
            let stack = self.stack;
            self.app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: url.to_string(),
                request_id: None,
            });
            self.app.update();
            self.app.update();
        }

        fn pages(&mut self) -> Vec<Entity> {
            let stack = self.stack;
            let mut q = self
                .app
                .world_mut()
                .query::<(Entity, &ChildOf, &FileView)>();
            let mut found = Vec::new();
            for (entity, child_of, _) in q.iter(self.app.world()) {
                if child_of.0 == stack {
                    found.push(entity);
                }
            }
            found
        }

        fn page(&mut self) -> Entity {
            let pages = self.pages();
            assert_eq!(pages.len(), 1);
            pages[0]
        }

        fn path(&self, page: Entity) -> PathBuf {
            self.app.world().get::<FileView>(page).unwrap().path.clone()
        }

        fn goto_line(&self, page: Entity) -> Option<u32> {
            let goto = self.app.world().get::<PendingGoto>(page)?;
            Some(goto.line)
        }

        fn open_editors(&self, page: Entity) -> Vec<PathBuf> {
            self.app
                .world()
                .get::<ExplorerState>(page)
                .unwrap()
                .open_editors
                .clone()
        }

        fn url(&self, page: Entity) -> String {
            self.app
                .world()
                .get::<PageMetadata>(page)
                .unwrap()
                .url
                .clone()
        }

        fn select(&mut self, page: Entity, path: &Path) {
            self.app.world_mut().trigger(BinReceive {
                webview: page,
                payload: FileOpenEvent {
                    path: path.to_string_lossy().into_owned(),
                },
            });
            self.app.update();
        }
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
    fn opening_another_file_navigates_the_editor_already_in_the_stack() {
        let mut stack = EditorStack::showing("file:///etc/hostname");
        let page = stack.page();
        stack.open("file:///etc/hosts#L12");
        assert_eq!(stack.pages(), vec![page]);
        assert_eq!(stack.path(page), PathBuf::from("/etc/hosts"));
        assert_eq!(stack.goto_line(page), Some(11));
        assert_eq!(
            stack.open_editors(page),
            vec![PathBuf::from("/etc/hostname"), PathBuf::from("/etc/hosts")]
        );
    }

    #[test]
    fn knowledge_page_redirects_to_its_directory() {
        let mut stack = EditorStack::showing(vmux_core::knowledge::KNOWLEDGE_PAGE_URL);
        let page = stack.page();

        assert_eq!(
            vmux_core::file_url::FileUrl::parse(&stack.url(page)).and_then(|url| url.path()),
            Some(vmux_core::knowledge::KnowledgeVault::user().into_root())
        );
    }

    #[test]
    fn selecting_a_note_updates_the_file_url() {
        let mut stack = EditorStack::showing(vmux_core::knowledge::KNOWLEDGE_PAGE_URL);
        let page = stack.page();

        stack.select(page, Path::new("/tmp/note.md"));

        assert_eq!(stack.url(page), "file:///tmp/note.md");
    }

    #[test]
    fn reopening_the_file_already_shown_keeps_the_page_loaded_and_adds_no_tab() {
        let mut stack = EditorStack::showing("file:///etc/hostname");
        let page = stack.page();
        stack.app.world_mut().entity_mut(page).insert(FileDir {
            entries: Vec::new(),
        });
        stack.open("file:///etc/hostname#L7");
        assert_eq!(stack.pages(), vec![page]);
        assert!(stack.app.world().get::<FileDir>(page).is_some());
        assert_eq!(stack.goto_line(page), Some(6));
        assert_eq!(
            stack.open_editors(page),
            vec![PathBuf::from("/etc/hostname")]
        );
    }

    #[test]
    fn a_stack_holding_no_editor_page_gets_a_fresh_one() {
        let mut stack = EditorStack::empty();
        let target = stack.stack;
        let occupant = stack.app.world_mut().spawn(ChildOf(target)).id();
        stack.open("file:///etc/hostname");
        assert!(stack.app.world().get_entity(occupant).is_err());
        let page = stack.page();
        assert_eq!(stack.path(page), PathBuf::from("/etc/hostname"));
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
        app.add_plugins(MinimalPlugins).add_systems(
            Update,
            (load_file_buffers, apply_loaded_file_buffers).chain(),
        );
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
        FileLoadTask::settle(&mut app, e);
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
        FileLoadTask::settle(&mut app, e);
        let dir = app.world().get::<FileDir>(e).unwrap();
        assert!(dir.entries.iter().any(|x| x.name == "f2"));
        assert!(!dir.entries.iter().any(|x| x.name == "f1"));
    }
}

#[cfg(test)]
mod parked_edit_tests {
    use super::*;

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
                .add_plugins(EditorNavigationPlugin)
                .add_systems(
                    Update,
                    (load_file_buffers, apply_loaded_file_buffers).chain(),
                )
                .add_observer(on_file_encoding_set);
            app.world_mut().insert_non_send(SelfWrites::default());
            app.world_mut().insert_non_send(Browsers::default());
            app.world_mut().insert_non_send(ClipboardHandle(None));
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

        fn write_bytes(&self, name: &str, bytes: &[u8]) {
            std::fs::write(self.dir.path().join(name), bytes).unwrap();
        }

        fn bytes(&self, name: &str) -> Vec<u8> {
            std::fs::read(self.dir.path().join(name)).unwrap()
        }

        fn encoding(&self) -> FileEncoding {
            self.app
                .world()
                .get::<EditState>(self.entity)
                .expect("a loaded buffer")
                .core
                .buffer
                .encoding
        }

        fn failure(&self) -> Option<LoadFailure> {
            let buf = self.app.world().get::<FileBuffer>(self.entity)?;
            let (reason, _) = LoadFailure::parse(&buf.language)?;
            Some(reason)
        }

        fn encoding_action(&mut self, encoding: FileEncoding, action: FileEncodingAction) {
            self.app.world_mut().trigger(BinReceive {
                webview: self.entity,
                payload: FileEncodingSet { encoding, action },
            });
            self.settle();
        }

        fn navigate_to(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(BinReceive {
                webview: self.entity,
                payload: FileOpenEvent { path },
            });
            self.settle();
        }

        fn settle(&mut self) {
            FileLoadTask::settle(&mut self.app, self.entity);
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

    const SHIFT_JIS_SAMPLE: [u8; 17] = [
        0x93, 0xFA, 0x96, 0x7B, 0x8C, 0xEA, 0x82, 0xCC, 0x83, 0x65, 0x83, 0x4C, 0x83, 0x58, 0x83,
        0x67, 0x0A,
    ];

    #[test]
    fn a_shift_jis_file_opens_and_saves_back_as_shift_jis() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.settle();

        assert_eq!(s.text(), "日本語のテキスト\n", "decoded on load");
        assert_eq!(s.encoding(), FileEncoding::ShiftJis);

        s.type_into_buffer("EDIT");
        s.encoding_action(FileEncoding::ShiftJis, FileEncodingAction::Save);

        let mut expected = b"EDIT".to_vec();
        expected.extend_from_slice(&SHIFT_JIS_SAMPLE);
        assert_eq!(
            s.bytes("main.txt"),
            expected,
            "the file is still shift_jis, not transcoded to utf-8"
        );
    }

    #[test]
    fn saving_a_character_the_encoding_cannot_hold_leaves_the_file_untouched() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.settle();

        s.type_into_buffer("€");
        s.encoding_action(FileEncoding::ShiftJis, FileEncodingAction::Save);

        assert_eq!(
            s.bytes("main.txt"),
            SHIFT_JIS_SAMPLE,
            "a lossy save is refused rather than written with substitutions"
        );
    }

    #[test]
    fn reopening_with_an_encoding_redecodes_the_same_bytes() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.settle();
        assert_eq!(s.encoding(), FileEncoding::ShiftJis);

        s.encoding_action(FileEncoding::EucJp, FileEncodingAction::Reopen);

        assert_eq!(s.encoding(), FileEncoding::EucJp);
        assert_ne!(
            s.text(),
            "日本語のテキスト\n",
            "the override is honoured over what detection chose"
        );
    }

    #[test]
    fn a_file_that_would_not_decode_can_be_reopened_from_the_failure_itself() {
        let mut s = Session::open("main.log");
        s.write_bytes("main.log", b"caf\xe9\x00\x00 log\x00");
        s.settle();

        assert_eq!(s.failure(), Some(LoadFailure::Undecodable));
        assert!(
            s.app.world().get::<EditState>(s.entity).is_none(),
            "no buffer is loaded, so the footer chooser has nothing to hang off"
        );

        s.encoding_action(FileEncoding::Iso8859_1, FileEncodingAction::Reopen);

        assert_eq!(s.failure(), None, "the failure is cleared, not repeated");
        assert_eq!(s.encoding(), FileEncoding::Iso8859_1);
        assert_eq!(s.text(), "café\u{0}\u{0} log\u{0}");
    }

    #[test]
    fn a_failure_no_encoding_can_rescue_is_not_offered_one() {
        let mut s = Session::open("gone.log");
        s.settle();

        assert_eq!(s.failure(), Some(LoadFailure::Fatal));
    }

    #[test]
    fn an_encoding_chosen_for_one_file_does_not_follow_the_pane_to_the_next() {
        let mut s = Session::open("main.txt");
        s.write_bytes("main.txt", &SHIFT_JIS_SAMPLE);
        s.write("plain.txt", "ascii\n");
        s.settle();

        s.encoding_action(FileEncoding::Utf16Le, FileEncodingAction::Reopen);
        assert_eq!(s.encoding(), FileEncoding::Utf16Le);

        s.navigate_to("plain.txt");

        assert_eq!(s.encoding(), FileEncoding::Utf8);
        assert_eq!(s.text(), "ascii\n");
    }

    #[test]
    fn returning_to_a_file_keeps_its_undo_history() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "one\n");
        s.write("lib.rs", "two\n");
        s.settle();
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

    #[test]
    fn a_file_changed_while_parked_is_reloaded() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "before\n");
        s.write("lib.rs", "other\n");
        s.settle();
        assert_eq!(s.text(), "before\n");

        s.navigate_to("lib.rs");
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.write("main.rs", "changed on disk\n");
        s.navigate_to("main.rs");

        assert_eq!(s.text(), "changed on disk\n");
    }

    #[test]
    fn unsaved_edits_survive_a_file_changing_while_parked() {
        let mut s = Session::open("main.rs");
        s.write("main.rs", "before\n");
        s.write("lib.rs", "other\n");
        s.settle();

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

    struct ApplyEdit {
        app: App,
        views: Vec<Entity>,
        sent: std::sync::mpsc::Receiver<serde_json::Value>,
    }

    impl ApplyEdit {
        const BEFORE: &'static str = "one two three\n";

        fn renamed(path: &Path, panes: usize) -> Self {
            let (mut app, views) = Self::bare(path, panes);
            app.world_mut()
                .write_message(crate::lsp::manager::LspRequestedEdit {
                    entity: views[0],
                    root: path.parent().unwrap_or(path).to_path_buf(),
                    result: Ok(Self::renaming(path)),
                });
            let (_outgoing, sent) = std::sync::mpsc::channel();
            Self { app, views, sent }
        }

        fn with_edit(path: &Path, panes: usize) -> Self {
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
                    root: path.parent().unwrap_or(path).to_path_buf(),
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

        let mut h = ApplyEdit::with_edit(&path, 2);
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

    #[test]
    fn the_whole_edit_undoes_in_one_step() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::with_edit(&path, 1);
        h.app.update();
        let view = h.views[0];
        assert_eq!(h.text(view), "1 two 3\n");

        h.undo(view);
        assert_eq!(h.text(view), ApplyEdit::BEFORE);
    }

    #[test]
    fn panes_that_have_drifted_apart_are_refused_rather_than_corrupted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, ApplyEdit::BEFORE).unwrap();

        let mut h = ApplyEdit::with_edit(&path, 2);
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

        let mut h = ApplyEdit::with_edit(&path, 1);
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

        let mut h = ApplyEdit::with_edit(&path, 0);
        h.app.update();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1 two 3\n");
        assert_eq!(h.sent.try_recv().unwrap()["result"]["applied"], true);
    }
}

#[cfg(test)]
mod host_history_tests {
    use super::*;
    use vmux_core::PageOpenId;
    use vmux_core::host::page::{HostHistory, HostHistoryDelta, HostHistoryStep};

    struct Editor {
        app: App,
        view: Entity,
        dir: tempfile::TempDir,
    }

    impl Editor {
        fn opened(name: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(vmux_core::CorePlugin)
                .add_plugins(EditorNavigationPlugin)
                .add_plugins(EditorHistoryPlugin)
                .add_message::<vmux_core::event::RecordVisitRequest>()
                .add_systems(Update, handle_file_page_open);
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));
            let stack = app.world_mut().spawn_empty().id();
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: format!("file://{}", dir.path().join(name).display()),
                request_id: None,
            });
            app.update();
            app.update();
            let view = app
                .world_mut()
                .query_filtered::<Entity, With<FileView>>()
                .single(app.world())
                .expect("opening a file:// page spawns one file view");
            Self { app, view, dir }
        }

        fn open(&mut self, name: &str) {
            let path = self.dir.path().join(name).to_string_lossy().into_owned();
            self.app.world_mut().trigger(BinReceive {
                webview: self.view,
                payload: FileOpenEvent { path },
            });
            self.app.update();
        }

        fn scroll_to(&mut self, top_row: u32) {
            self.app
                .world_mut()
                .get_mut::<FileViewport>(self.view)
                .expect("a file view has a viewport")
                .top_row = top_row;
            self.app.update();
        }

        fn step(&mut self, delta: HostHistoryDelta) {
            let webview = self.view;
            self.app
                .world_mut()
                .write_message(HostHistoryStep { webview, delta });
            self.app.update();
        }

        fn showing(&self) -> String {
            self.app
                .world()
                .get::<FileView>(self.view)
                .expect("a file view")
                .path
                .file_name()
                .expect("a named file")
                .to_string_lossy()
                .into_owned()
        }

        fn top_row(&self) -> u32 {
            self.app
                .world()
                .get::<FileViewport>(self.view)
                .expect("a file view has a viewport")
                .top_row
        }

        fn history(&self) -> &HostHistory {
            self.app
                .world()
                .get::<HostHistory>(self.view)
                .expect("a file view owns its history")
        }
    }

    #[test]
    fn back_and_forward_walk_the_files_the_editor_opened() {
        let mut editor = Editor::opened("a.rs");
        editor.open("b.rs");
        editor.open("c.rs");
        assert!(editor.history().can_go_back());
        assert!(!editor.history().can_go_forward());

        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "b.rs");
        assert!(editor.history().can_go_forward());

        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "a.rs");
        assert!(!editor.history().can_go_back());

        editor.step(HostHistoryDelta::Forward);
        assert_eq!(editor.showing(), "b.rs");
    }

    #[test]
    fn opening_a_file_after_going_back_drops_the_forward_trail() {
        let mut editor = Editor::opened("a.rs");
        editor.open("b.rs");
        editor.open("c.rs");
        editor.step(HostHistoryDelta::Back);
        editor.step(HostHistoryDelta::Back);

        editor.open("d.rs");

        assert!(!editor.history().can_go_forward());
        editor.step(HostHistoryDelta::Back);
        assert_eq!(editor.showing(), "a.rs");
        editor.step(HostHistoryDelta::Forward);
        assert_eq!(editor.showing(), "d.rs");
    }

    #[test]
    fn going_back_lands_on_the_line_the_file_was_left_at() {
        let mut editor = Editor::opened("a.rs");
        editor.scroll_to(120);
        editor.open("b.rs");
        assert_eq!(editor.top_row(), 0);

        editor.step(HostHistoryDelta::Back);

        assert_eq!(editor.showing(), "a.rs");
        assert_eq!(editor.top_row(), 120);
    }
}

#[cfg(test)]
mod open_editor_tests {
    use super::*;

    impl ExplorerState {
        fn holding(paths: &[&str]) -> Self {
            Self {
                open_editors: paths.iter().map(PathBuf::from).collect(),
                ..Self::default()
            }
        }
    }

    #[test]
    fn closing_an_editor_hands_back_the_tab_that_takes_its_place() {
        let mut st = ExplorerState::holding(&["/a", "/b", "/c"]);
        assert_eq!(
            st.close_editor(Path::new("/b")),
            Some(PathBuf::from("/c")),
            "closing a middle tab moves right, as the tab strip reads"
        );
        assert_eq!(
            st.close_editor(Path::new("/c")),
            Some(PathBuf::from("/a")),
            "closing the last tab falls back to the one on its left"
        );
        assert_eq!(st.close_editor(Path::new("/a")), None);
        assert!(st.open_editors.is_empty());
    }

    #[test]
    fn closing_an_editor_that_was_never_open_changes_nothing() {
        let mut st = ExplorerState::holding(&["/a"]);
        assert_eq!(st.close_editor(Path::new("/zzz")), None);
        assert_eq!(st.open_editors, vec![PathBuf::from("/a")]);
    }
}

#[cfg(test)]
mod announced_scroll_tests {
    use super::*;
    use vmux_api::BinEvent;

    #[derive(Resource, Default)]
    struct Emitted(Vec<String>);

    struct GotoSession {
        app: App,
        view: Entity,
    }

    impl GotoSession {
        fn scrolled_to_the_top_of(path: &Path) -> Self {
            let text = (0..600)
                .map(|i| format!("let v{i} = {i};\n"))
                .collect::<String>();
            let core = EditCore::new(
                path.to_path_buf(),
                "Rust".into(),
                &text,
                crate::edit::EditMode::Normal,
            );
            let edit = EditState::new(
                core,
                HighlightCache::new(path),
                crate::fold::FoldState::default(),
            );
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<crate::lsp::manager::LspGoto>()
                .init_resource::<Emitted>()
                .add_systems(Update, apply_goto)
                .add_observer(
                    |trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Emitted>| {
                        emitted.0.push(trigger.event().id().to_string());
                    },
                );
            app.world_mut()
                .insert_resource(crate::lsp::manager::LspManager::new(
                    crate::lsp::LspOutbox::default(),
                    crate::lsp::server_request::ServerEvents::default().sender(),
                ));
            let view = app
                .world_mut()
                .spawn((
                    edit,
                    FileView {
                        path: path.to_path_buf(),
                    },
                    FileViewport {
                        top_row: 0,
                        rows: 40,
                        wrap_columns: 0,
                        word_wrap: vmux_core::editor::WordWrap::Off,
                        word_wrap_column: 80,
                    },
                    EditorKeymap(vmux_core::editor::KeymapKind::Vscode.make(&[], "\\")),
                    PageMetadata {
                        title: String::new(),
                        url: String::new(),
                        icon: vmux_core::PageIcon::None,
                        bg_color: None,
                    },
                ))
                .id();
            let mut browsers = Browsers::default();
            browsers.set_externally_hosted(view);
            app.world_mut().insert_non_send(browsers);
            Self { app, view }
        }

        fn goto(&mut self, line: u32) {
            let entity = self.view;
            let path = self
                .app
                .world()
                .get::<FileView>(entity)
                .expect("a file view")
                .path
                .clone();
            self.app
                .world_mut()
                .write_message(crate::lsp::manager::LspGoto {
                    entity,
                    path,
                    line,
                    utf16_col: 0,
                });
            self.app.update();
        }
    }

    #[test]
    fn a_jump_that_moves_the_viewport_tells_the_page_where_it_went() {
        let mut session = GotoSession::scrolled_to_the_top_of(Path::new("/tmp/goto.rs"));
        session.goto(500);

        let top = session
            .app
            .world()
            .get::<FileViewport>(session.view)
            .expect("a viewport")
            .top_row;
        assert!(top > 0, "jumping to line 500 has to move the window");
        assert!(
            session
                .app
                .world()
                .resource::<Emitted>()
                .0
                .iter()
                .any(|id| id == FileScrollByEvent::id()),
            "the window was repainted at row {top}, so a page still parked at row 0 \
             would render the band off screen unless the move is announced: {:?}",
            session.app.world().resource::<Emitted>().0
        );
    }
}
