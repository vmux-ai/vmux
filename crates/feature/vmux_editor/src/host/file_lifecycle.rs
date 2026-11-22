use crate::host::editor::EditReadOnly;
#[cfg(test)]
use crate::host::editor::Editor;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::ecs::system::Commands;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::event::*;

use super::editor::FileView;
use super::explorer::OutlineDirty;
use super::explorer::{
    ExplorerPanelSent, ExplorerTree, ExplorerTreeChanged, ExplorerTreeDirty, OpenEditorsDirty,
};
pub(super) use super::file_loading::FileLoadTask;
use super::file_watch::{FileWatchPlugin, Observed, Reconciled};
pub(super) use super::file_write_io::SelfWrites;
use super::file_writes::{FileWritesPlugin, WriteReceipts};
use super::markdown::ParsedNote;
use super::note::NoteSent;
use super::status::{FileInitialMetaSent, FileKeymapSent, FileThemeSent, FileViewModeSent};

pub(super) struct FileLifecyclePlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EditorFileLoadedSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct FileChangesSet;

impl Plugin for FileLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FileWatchPlugin,
            FileWritesPlugin,
            super::file_loading::FileLoadingPlugin,
        ))
        .add_systems(
            Update,
            (sync_file_git, drain_file_changes, reload_changed_files)
                .chain()
                .in_set(FileChangesSet),
        )
        .add_observer(reset_sent);
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
    pub(super) fn failed(reason: LoadFailure, message: String) -> Self {
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
struct FileReloadRequested;

#[derive(Component, Clone)]
pub(super) struct ForcedEncoding {
    pub(super) path: PathBuf,
    pub(super) encoding: FileEncoding,
}

impl ForcedEncoding {
    pub(super) fn for_path(&self, path: &Path) -> Option<FileEncoding> {
        match self.path == path {
            true => Some(self.encoding),
            false => None,
        }
    }
}

#[derive(Component)]
pub(super) struct MissingFileView;

fn reset_sent(
    trigger: On<UiInput<vmux_ecs::page::PageReady>>,
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
    if ParsedNote::supports(&file.path) {
        commands.entity(entity).insert(OutlineDirty);
    }
}

fn drain_file_changes(
    mut observations: MessageReader<Observed>,
    mut reconciled: MessageReader<Reconciled>,
    self_writes: WriteReceipts,
    views: Query<(Entity, &FileView, Has<MissingFileView>)>,
    mut trees: Query<(Entity, &mut ExplorerTree)>,
    mut commands: Commands,
) {
    let mut changed = HashSet::new();
    for observation in observations.read() {
        match &observation.0 {
            Ok(event) if matches!(event.kind, notify::EventKind::Access(_)) => {}
            Ok(event) => {
                for path in &event.paths {
                    changed.insert(vmux_path::PathIdentity::resolve(path).into_path_buf());
                }
            }
            Err(error) => tracing::warn!("file watcher observation failed: {error}"),
        }
    }
    let mut reconciled_dirs = HashSet::new();
    for directory in reconciled.read() {
        reconciled_dirs.insert(directory.0.clone());
    }
    if changed.is_empty() && reconciled_dirs.is_empty() {
        return;
    }
    for (entity, file, missing) in &views {
        let path = vmux_path::PathIdentity::resolve(&file.path).into_path_buf();
        let self_written = self_writes.recent(&path);
        let ancestor_changed = missing
            && (changed.iter().any(|changed| path.starts_with(changed))
                || reconciled_dirs
                    .iter()
                    .any(|directory| path.starts_with(directory)));
        if (changed.contains(&path) || ancestor_changed) && !self_written {
            commands.entity(entity).insert(FileReloadRequested);
        }
    }
    let mut changed_dirs = reconciled_dirs;
    for path in &changed {
        if let Some(parent) = path.parent() {
            changed_dirs.insert(vmux_path::PathIdentity::resolve(parent).into_path_buf());
        }
    }
    for (tree_entity, mut tree) in &mut trees {
        let requests = tree.refresh_changed(tree_entity, &changed_dirs);
        if requests.is_empty() {
            continue;
        }
        for request in requests {
            commands.spawn(request);
        }
        commands.trigger(ExplorerTreeChanged(tree_entity));
    }
}

fn reload_changed_files(
    files: Query<(Entity, &FileView, Option<EditReadOnly>), With<FileReloadRequested>>,
    browsers: crate::host::delivery::Surface,
    mut lsp_changes: Option<MessageWriter<vmux_lsp::CloseDocument>>,
    mut commands: Commands,
) {
    for (entity, file, edit) in &files {
        commands.entity(entity).remove::<FileReloadRequested>();
        let ready = browsers.can_emit_to(&entity);

        if file.path.is_dir() {
            let entries = FileDir::read(&file.path);
            commands
                .entity(entity)
                .insert(FileDir { entries })
                .remove::<FileInitialMetaSent>();
            continue;
        }

        if let Some(kind) = vmux_api::media::MediaKind::from_path(&file.path.to_string_lossy()) {
            if ready {
                let mime = vmux_api::media::MediaKind::mime(&file.path.to_string_lossy())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or(0);
                let url = format!("{}&v={nonce}", file.raw_media_url());
                commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
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
            && edit.core.revision.dirty
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<crate::host::editor::EditorBundle>()
            .remove::<vmux_git::GitDiffSource>()
            .remove::<FileBuffer>()
            .remove::<FileLoadTask>()
            .remove::<FileInitialMetaSent>()
            .remove::<crate::lsp::manager::LintRan>();
        if let Some(lsp_changes) = lsp_changes.as_mut() {
            lsp_changes.write(vmux_lsp::CloseDocument {
                owner: entity,
                path: file.path.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::viewport::FileViewport;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(FileLifecyclePlugin);
        app.world_mut().insert_non_send(Browsers::default());

        app
    }

    fn file_view(path: PathBuf) -> impl Bundle {
        (
            FileView { path },
            FileViewport {
                top_row: 0,
                rows: 0,
                wrap_columns: 0,
                word_wrap: vmux_api::editor::WordWrap::default(),
                word_wrap_column: 80,
                scroll_revision: 0,
            },
        )
    }

    #[test]
    fn missing_file_view_loads_when_file_is_created() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("created-after-open");
        let path = parent.join("file.txt");
        let mut app = app();
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
        app.world_mut().write_message(Observed(Ok(
            notify::Event::new(notify::EventKind::Any).add_path(parent)
        )));
        FileLoadTask::settle(&mut app, entity);

        assert_eq!(
            Editor::read(app.world(), entity)
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
