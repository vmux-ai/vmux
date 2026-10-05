use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_ecs::event::{
    ExplorerCloseEditor, ExplorerCollapseAll, ExplorerFilesToggle, ExplorerOpenEditorsToggle,
    ExplorerOutlineToggle, ExplorerPanelSetVisible, ExplorerPanelViewSet,
    ExplorerPanelViewportWidth, ExplorerPanelWidth, ExplorerRevealCurrent, ExplorerTreePrefetch,
    ExplorerTreeRefresh, ExplorerTreeToggle, FileDirEntry, TreeRow,
};

use entry::ExplorerEntryPlugin;
use outline::OutlinePlugin;
pub(crate) use outline::OutlineRows;
use panel::PanelPlugin;
pub use panel::StackExplorerVisibility;
pub(crate) use panel::{ExplorerFindInFilesRequest, ExplorerRevealRequest, ExplorerToggleRequest};
pub use search::GlobalSearchRequest;
use search::SearchPlugin;
use tree::{ExplorerDirLoadRequest, TreePlugin};

mod entry;
mod fs;
mod outline;
mod panel;
mod search;
mod tabs;
mod tree;

#[cfg(test)]
mod tests;

#[derive(EntityEvent)]
struct RevealCurrent {
    #[event_target]
    entity: Entity,
    reveal: vmux_ecs::event::ExplorerReveal,
}

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
    expanded: HashSet<PathBuf>,
    loading: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<FileDirEntry>>,
    used: std::time::Instant,
}

impl ExplorerTree {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            expanded: HashSet::new(),
            loading: HashSet::new(),
            children: HashMap::new(),
            used: std::time::Instant::now(),
        }
    }

    fn answers_for(&self, root: &Path) -> bool {
        self.root == root
    }

    fn allows(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    fn create_parent(&self, selected: &Path) -> PathBuf {
        if selected.as_os_str().is_empty() {
            return self.root.clone();
        }
        if selected == self.root
            || self
                .children
                .values()
                .flatten()
                .any(|entry| entry.is_dir && Path::new(&entry.path) == selected)
        {
            return selected.to_path_buf();
        }
        selected
            .parent()
            .filter(|parent| parent.starts_with(&self.root))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.root.clone())
    }

    pub(super) fn expanded_dirs(&self) -> impl Iterator<Item = &PathBuf> {
        self.expanded.iter()
    }

    fn use_now(&mut self) {
        self.used = std::time::Instant::now();
    }

    fn rows(&self, root: &Path) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        self.append_rows(root, 0, &mut rows);
        rows
    }

    fn append_rows(&self, directory: &Path, depth: u16, rows: &mut Vec<TreeRow>) {
        let Some(entries) = self.children.get(directory) else {
            return;
        };
        for entry in entries {
            let path = PathBuf::from(&entry.path);
            let expanded = entry.is_dir && self.expanded.contains(&path);
            rows.push(TreeRow {
                name: entry.name.clone(),
                path: entry.path.clone(),
                depth,
                is_dir: entry.is_dir,
                expanded,
                loading: self.loading.contains(&path),
            });
            if expanded {
                self.append_rows(&path, depth + 1, rows);
            }
        }
    }

    fn is_loading(&self, path: &Path) -> bool {
        self.loading.contains(path)
    }

    fn begin_dir_load(&mut self, path: &Path, force: bool) -> bool {
        self.use_now();
        if self.loading.contains(path) || !force && self.children.contains_key(path) {
            return false;
        }
        self.loading.insert(path.to_path_buf());
        true
    }

    pub(super) fn refresh_changed(
        &mut self,
        tree: Entity,
        changed_dirs: &HashSet<PathBuf>,
    ) -> Vec<ExplorerDirLoadRequest> {
        let mut requests = Vec::new();
        let cached: Vec<PathBuf> = self.children.keys().cloned().collect();
        for dir in cached {
            let canonical = vmux_path::PathIdentity::resolve(&dir).into_path_buf();
            if changed_dirs.contains(&canonical) && self.begin_dir_load(&dir, true) {
                requests.push(ExplorerDirLoadRequest::new(tree, dir));
            }
        }
        requests
    }

    fn evict_subtree(&mut self, path: &Path) {
        self.use_now();
        self.expanded.retain(|entry| !entry.starts_with(path));
        self.loading.retain(|entry| !entry.starts_with(path));
        self.children.retain(|entry, _| !entry.starts_with(path));
    }
}

#[derive(Event, Clone, Copy)]
pub(super) struct ExplorerTreeChanged(pub(super) Entity);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = ExplorerTreeUsers)]
struct UsesExplorerTree(Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = UsesExplorerTree)]
struct ExplorerTreeUsers(Vec<Entity>);

const IDLE_TREE_CAPACITY: usize = 4;

pub(super) struct TabsPlugin;

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

#[derive(Component, Default)]
pub(super) struct ExplorerState {
    open_editors: Vec<PathBuf>,
    focus_path: Option<PathBuf>,
    focus_revision: u64,
    active_editor: Option<PathBuf>,
    active_editor_is_dir: bool,
}

impl ExplorerState {
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
}
