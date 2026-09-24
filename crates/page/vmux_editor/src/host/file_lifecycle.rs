use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::event::*;

use super::editor::{Editor, FileView, ParkedEdits};
use super::explorer::OutlineDirty;
use super::explorer::{ExplorerPanelSent, ExplorerTreeDirty, ExplorerTrees, OpenEditorsDirty};
use super::keymap::KeymapConfig;
use super::note::NoteSent;
use super::status::{FileInitialMetaSent, FileKeymapSent, FileThemeSent, FileViewModeSent};
use crate::dir::{list_dir, parent_listing};
use crate::edit::{EditCore, highlight_cache::HighlightCache};
use crate::media::FileMedia;

pub(super) struct FileLifecyclePlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EditorFileLoadedSet;

impl Plugin for FileLifecyclePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = mpsc::channel();
        let proxy = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let wake = result
                .as_ref()
                .is_ok_and(|event| !matches!(event.kind, notify::EventKind::Access(_)));
            let _ = tx.send(result);
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
            Err(error) => tracing::warn!("file watcher init failed: {error}"),
        }
        app.insert_non_send(SelfWrites::default())
            .insert_non_send(crate::fold_store::FoldStore::load())
            .add_systems(
                Update,
                (
                    sync_file_git,
                    reconcile_file_watches,
                    drain_file_changes,
                    reload_changed_files,
                    load_file_buffers,
                    apply_loaded_file_buffers.in_set(EditorFileLoadedSet),
                )
                    .chain(),
            )
            .add_observer(reset_file_sent_markers_on_page_ready);
    }
}

type ChangedFileViews<'w, 's> =
    Query<'w, 's, (Entity, &'static FileView), Or<(Added<FileView>, Changed<FileView>)>>;

fn sync_file_git(views: ChangedFileViews, mut document: Local<u64>, mut commands: Commands) {
    for (entity, view) in &views {
        *document = document.wrapping_add(1).max(1);
        commands
            .entity(entity)
            .remove::<vmux_git::FileGit>()
            .insert(vmux_git::FileGit::new(&view.path, *document));
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
pub(super) enum LoadFailure {
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

    pub(super) fn parse(language: &str) -> Option<(Self, &str)> {
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
pub(super) struct FileLoadTask {
    path: PathBuf,
    task: Task<FileLoad>,
}

#[cfg(test)]
impl FileLoadTask {
    pub(super) fn settle(app: &mut App, entity: Entity) {
        for _ in 0..10_000 {
            app.update();
            let world = app.world();
            let loaded = world.get::<FileLoadTask>(entity).is_none()
                && (world.get::<FileBuffer>(entity).is_some()
                    || world.get::<FileDir>(entity).is_some()
                    || world.get::<FileMedia>(entity).is_some()
                    || world.get::<Editor>(entity).is_some());
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

#[derive(Default)]
pub(super) struct SelfWrites(pub(super) HashMap<PathBuf, std::time::Instant>);

#[derive(Component)]
struct FileReloadRequested;

#[derive(Component, Clone)]
pub(super) struct ForcedEncoding {
    pub(super) path: PathBuf,
    pub(super) encoding: FileEncoding,
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
pub(super) struct MissingFileView;

struct FileWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    dirs: HashSet<PathBuf>,
}

type UnloadedFileView = (
    Without<FileBuffer>,
    Without<FileDir>,
    Without<FileMedia>,
    Without<Editor>,
    Without<FileLoadTask>,
);

#[allow(clippy::type_complexity)]
fn load_file_buffers(
    mut files: Query<
        (
            Entity,
            &FileView,
            Option<&mut ParkedEdits>,
            Option<&ForcedEncoding>,
        ),
        UnloadedFileView,
    >,
    settings: Option<Res<vmux_setting::AppSettings>>,
    proxy: Option<Res<bevy::winit::EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let keymap = KeymapConfig::resolve(settings.as_deref());
    for (entity, file, mut parked, forced) in &mut files {
        let forced = forced.and_then(|encoding| encoding.for_path(&file.path));
        let markdown = crate::markdown::is_markdown_path(&file.path);
        if forced.is_none()
            && let Some(parked) = parked.as_mut()
            && let Some(resumed) = parked.resume(&file.path)
        {
            let mut entity_commands = commands.entity(entity);
            entity_commands
                .insert((resumed.edit, keymap.keymap(), resumed.diff))
                .remove::<MissingFileView>();
            if markdown {
                entity_commands.remove::<NoteSent>().insert(OutlineDirty);
            }
            continue;
        }
        let path = file.path.clone();
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
    mut files: Query<(Entity, &FileView, &mut FileLoadTask)>,
    settings: Option<Res<vmux_setting::AppSettings>>,
    store: Option<NonSend<crate::fold_store::FoldStore>>,
    mut commands: Commands,
) {
    let keymap = KeymapConfig::resolve(settings.as_deref());
    for (entity, view, mut pending) in &mut files {
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
                let markdown = crate::markdown::is_markdown_path(&view.path);
                let crate::encoding::DecodedText { text, encoding } = decoded;
                let highlight = match heavy {
                    true => HighlightCache::plain(&view.path),
                    false => HighlightCache::new(&view.path),
                };
                let mut core = EditCore::new(
                    view.path.clone(),
                    highlight.language.clone(),
                    &text,
                    keymap.initial_mode(),
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
                        Editor::new(core, highlight, folds),
                        keymap.keymap(),
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

fn reset_file_sent_markers_on_page_ready(
    trigger: On<UiInput<vmux_core::page::PageReady>>,
    file_views: Query<&FileView>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(file) = file_views.get(entity) else {
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
    if crate::explorer_model::is_markdown(&file.path) {
        commands.entity(entity).insert(OutlineDirty);
    }
}

pub(crate) fn canon(path: &Path) -> PathBuf {
    vmux_path::PathIdentity::resolve(path).into_path_buf()
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
    for file in &views {
        if let Some(dir) = watch_dir_for(&file.path) {
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
    let mut changed = HashSet::new();
    while let Ok(result) = watch.rx.try_recv() {
        if let Ok(event) = result {
            for path in event.paths {
                changed.insert(canon(&path));
            }
        }
    }
    if changed.is_empty() {
        return;
    }
    let mut self_writes = self_writes;
    if let Some(self_writes) = self_writes.as_mut() {
        self_writes
            .0
            .retain(|_, written| written.elapsed() < std::time::Duration::from_secs(2));
    }
    for (entity, file, missing) in &views {
        let path = canon(&file.path);
        let self_written = self_writes
            .as_ref()
            .is_some_and(|self_writes| self_writes.0.contains_key(&path));
        let ancestor_changed = missing && changed.iter().any(|changed| path.starts_with(changed));
        if (changed.contains(&path) || ancestor_changed) && !self_written {
            commands.entity(entity).insert(FileReloadRequested);
        }
    }
    let mut changed_dirs = HashSet::new();
    for path in &changed {
        if let Some(parent) = path.parent() {
            changed_dirs.insert(canon(parent));
        }
    }
    trees.refresh_changed(&changed_dirs, &mut commands);
}

fn reload_changed_files(
    files: Query<(Entity, &FileView, Option<&Editor>), With<FileReloadRequested>>,
    browsers: NonSend<Browsers>,
    mut manager: ResMut<crate::lsp::manager::LspManager>,
    mut commands: Commands,
) {
    for (entity, file, edit) in &files {
        commands.entity(entity).remove::<FileReloadRequested>();
        let ready = browsers.can_emit_to(&entity);

        if file.path.is_dir() {
            let entries = list_dir(&file.path);
            commands.entity(entity).insert(FileDir {
                entries: entries.clone(),
            });
            if ready {
                let (parent_path, parent_entries) = parent_listing(&file.path);
                commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                    entity,
                    &FileDirEvent {
                        path: file.display_path(),
                        abs_path: file.path.to_string_lossy().into_owned(),
                        entries,
                        parent_path,
                        parent_entries,
                    },
                ));
            }
            continue;
        }

        if let Some(kind) = vmux_core::media::media_kind(&file.path.to_string_lossy()) {
            if ready {
                let mime = vmux_core::media::media_mime(&file.path.to_string_lossy())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or(0);
                let url = format!("{}&v={nonce}", file.raw_media_url());
                commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                    entity,
                    &FileMediaEvent {
                        kind,
                        mime,
                        url,
                        abs_path: file.path.to_string_lossy().into_owned(),
                    },
                ));
            }
            continue;
        }

        if let Some(edit) = edit
            && edit.core.dirty
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<Editor>()
            .remove::<vmux_git::GitDiffSource>()
            .remove::<FileBuffer>()
            .remove::<FileLoadTask>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LintRan>();
        manager.change(&file.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::viewport::FileViewport;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ExplorerTrees>()
            .add_plugins(FileLifecyclePlugin);
        app.world_mut().insert_non_send(Browsers::default());
        app.world_mut()
            .insert_resource(crate::lsp::manager::LspManager::new(
                crate::lsp::LspOutbox::default(),
                crate::lsp::server_request::ServerEvents::default().sender(),
            ));
        app
    }

    fn file_view(path: PathBuf) -> impl Bundle {
        (
            FileView { path },
            FileViewport {
                top_row: 0,
                rows: 0,
                wrap_columns: 0,
                word_wrap: vmux_core::editor::WordWrap::default(),
                word_wrap_column: 80,
            },
        )
    }

    #[test]
    fn missing_file_view_loads_when_file_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("created-after-open");
        let path = parent.join("file.txt");
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(|_| {}).unwrap();
        let mut app = app();
        app.world_mut().insert_non_send(FileWatch {
            watcher,
            rx,
            dirs: HashSet::new(),
        });
        let entity = app.world_mut().spawn(file_view(path.clone())).id();

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
                .get::<Editor>(entity)
                .unwrap()
                .core
                .buffer
                .text(),
            "created\n"
        );
    }

    #[test]
    fn navigating_to_another_directory_reloads_its_entries() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        std::fs::create_dir(&first).unwrap();
        std::fs::write(first.join("first.txt"), "").unwrap();
        let second = temp.path().join("second");
        std::fs::create_dir(&second).unwrap();
        std::fs::write(second.join("second.txt"), "").unwrap();

        let mut app = app();
        let entity = app.world_mut().spawn(file_view(first)).id();
        FileLoadTask::settle(&mut app, entity);
        assert!(
            app.world()
                .get::<FileDir>(entity)
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.name == "first.txt")
        );

        app.world_mut().get_mut::<FileView>(entity).unwrap().path = second;
        app.world_mut().entity_mut(entity).remove::<FileDir>();
        FileLoadTask::settle(&mut app, entity);
        let dir = app.world().get::<FileDir>(entity).unwrap();
        assert!(dir.entries.iter().any(|entry| entry.name == "second.txt"));
        assert!(!dir.entries.iter().any(|entry| entry.name == "first.txt"));
    }
}
