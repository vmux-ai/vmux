use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::prelude::{App, Plugin};
use bevy_cef::prelude::UiEventPlugin;
use vmux_ecs::event::{
    ExplorerCloseEditor, ExplorerCollapseAll, ExplorerFilesToggle, ExplorerOpenEditorsToggle,
    ExplorerOutlineToggle, ExplorerPanelSetVisible, ExplorerPanelViewSet,
    ExplorerPanelViewportWidth, ExplorerPanelWidth, ExplorerRevealCurrent, ExplorerTreePrefetch,
    ExplorerTreeRefresh, ExplorerTreeToggle, FileDirEntry,
};

use entry::ExplorerEntryPlugin;
use outline::OutlinePlugin;
pub(crate) use outline::OutlineRows;
use panel::PanelPlugin;
pub use panel::StackExplorerVisibility;
pub(crate) use panel::{ExplorerFindInFilesRequest, ExplorerRevealRequest, ExplorerToggleRequest};
pub use search::GlobalSearchRequest;
use search::SearchPlugin;
pub(crate) use search::{
    ExplorerSearchCancelRequest, ExplorerSearchCompleted, ExplorerSearchQuery,
    ExplorerSearchRunRequest,
};
use tree::{ExplorerTreeDirRefresh, ExplorerTreeLookup, ExplorerTreeSubtreeEvict, TreePlugin};

pub(crate) use tabs::{ArchivedEditorState, RestoreEditors};
mod entry;
mod fs;
mod outline;
mod panel;
mod search;
mod tabs;
mod tree;

#[cfg(test)]
mod tests;

pub(super) struct ExplorerPlugin;

impl Plugin for ExplorerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            TreePlugin,
            PanelPlugin,
            ExplorerEntryPlugin,
            OutlinePlugin,
            SearchPlugin,
            TabsPlugin,
        ))
        .add_plugins(UiEventPlugin::<(
            ExplorerTreeToggle,
            ExplorerTreePrefetch,
            ExplorerTreeRefresh,
            ExplorerRevealCurrent,
            ExplorerCloseEditor,
            ExplorerPanelSetVisible,
            ExplorerPanelViewSet,
            ExplorerPanelViewportWidth,
            ExplorerPanelWidth,
            ExplorerOpenEditorsToggle,
            ExplorerFilesToggle,
            ExplorerOutlineToggle,
        )>::default())
        .add_plugins(UiEventPlugin::<(ExplorerCollapseAll,)>::default());
    }
}

pub(super) struct TabsPlugin;

#[derive(EntityEvent)]
struct RevealCurrent {
    #[event_target]
    entity: Entity,
    reveal: vmux_ecs::event::ExplorerReveal,
}

#[derive(Message, Clone)]
pub(super) struct ExplorerDirsChanged(pub(super) HashSet<PathBuf>);

#[derive(Component)]
pub(super) struct OutlineDirty;

#[derive(Component)]
pub(super) struct ExplorerPanelSent;

#[derive(Component)]
pub(super) struct OpenEditorsDirty;

#[derive(Component)]
pub(super) struct ExplorerTreeDirty;

#[derive(Component, Clone, Copy)]
struct ExplorerPanelDefaults {
    default_visible: bool,
    width: u32,
    loaded: bool,
}

#[derive(Component)]
pub(super) struct ExplorerTree {
    root: PathBuf,
    used: std::time::Instant,
    generation: u64,
}

impl ExplorerTree {
    const IDLE_CAPACITY: usize = 4;
    const WARM_AHEAD: usize = 64;

    fn new(root: PathBuf) -> Self {
        Self {
            root,
            used: std::time::Instant::now(),
            generation: 0,
        }
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    fn root_name(&self) -> String {
        self.root
            .file_name()
            .map(|name| name.to_string_lossy().to_uppercase())
            .unwrap_or_else(|| self.root.to_string_lossy().to_uppercase())
    }

    fn answers_for(&self, root: &Path) -> bool {
        self.root == root
    }

    fn allows(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    fn use_now(&mut self) {
        self.used = std::time::Instant::now();
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }
}

#[derive(Component)]
pub(crate) struct ExplorerDirectory {
    pub(crate) path: PathBuf,
}

#[derive(Component)]
pub(crate) struct ExplorerDirectoryExpanded;

#[derive(Component)]
pub(super) struct ExplorerDirectoryEntries(pub(super) Vec<FileDirEntry>);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct ExplorerDirectoryLoading {
    pub(super) generation: u64,
}

#[derive(Event, Clone, Copy)]
pub(super) struct ExplorerTreeChanged(pub(super) Entity);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = ExplorerTreeUsers)]
struct UsesExplorerTree(Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = UsesExplorerTree)]
struct ExplorerTreeUsers(Vec<Entity>);

#[derive(Component, Default)]
pub(super) struct ExplorerState {
    open_editors: Vec<PathBuf>,
    focus_path: Option<PathBuf>,
    focus_revision: u64,
    active_editor: Option<PathBuf>,
    active_editor_is_dir: bool,
}

impl ExplorerState {
    pub(crate) fn restore_editors(&mut self, paths: Vec<PathBuf>, active: &Path) {
        self.open_editors = paths;
        self.note_open(active);
        self.active_editor = Some(active.to_path_buf());
        self.active_editor_is_dir = active.is_dir();
    }

    fn note_open(&mut self, path: &Path) {
        if !self.open_editors.iter().any(|open| open == path) {
            self.open_editors.push(path.to_path_buf());
        }
    }

    pub(super) fn focus_effect(
        &mut self,
        path: &Path,
        reveal: vmux_ecs::event::ExplorerReveal,
    ) -> vmux_ecs::event::ExplorerFocusEvent {
        self.focus_revision = self.focus_revision.wrapping_add(1).max(1);
        vmux_ecs::event::ExplorerFocusEvent {
            revision: self.focus_revision,
            path: path.to_string_lossy().into_owned(),
            reveal,
        }
    }

    #[cfg(test)]
    pub(super) fn open_editors(&self) -> &[PathBuf] {
        &self.open_editors
    }

    pub(super) fn close_editor(&mut self, path: &Path) -> Option<PathBuf> {
        let at = self.open_editors.iter().position(|open| open == path)?;
        self.open_editors.remove(at);
        let neighbour = at.min(self.open_editors.len().saturating_sub(1));
        self.open_editors.get(neighbour).cloned()
    }

    pub(super) fn editor_name(path: &Path) -> String {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string())
    }

    pub(super) fn editor_parent(path: &Path) -> String {
        let Some(parent) = path.parent() else {
            return String::new();
        };
        match parent.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            None if parent.has_root() => "/".to_string(),
            None => String::new(),
        }
    }
}
