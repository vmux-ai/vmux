use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{
    ExplorerCloseEditor, ExplorerCollapseAll, ExplorerPanelSetVisible, ExplorerPanelWidth,
    ExplorerRevealCurrent, ExplorerTreePrefetch, ExplorerTreeRefresh, ExplorerTreeToggle,
    FileDirEntry,
};

mod fs;
mod mutation;
mod outline;
mod panel;
mod search;
mod tabs;
mod tree;

#[cfg(test)]
mod tests;

use mutation::MutationPlugin;
use outline::OutlinePlugin;
use panel::PanelPlugin;
pub use panel::StackExplorerVisibility;
pub use search::GlobalSearchRequest;
use search::SearchPlugin;
use tree::{ExplorerDirLoadRequest, TreePlugin};

#[derive(Component)]
pub(super) struct OutlineDirty;

#[derive(Component)]
pub(super) struct ExplorerPanelSent;

#[derive(Component)]
pub(super) struct OpenEditorsDirty;

#[derive(Component)]
pub(super) struct ExplorerTreeDirty;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StackExplorerRevision {
    client_id: u64,
    request_id: u64,
}

#[derive(Resource, Clone, Copy)]
struct ExplorerPanelDefaults {
    default_visible: bool,
    width: u32,
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

    pub(super) fn expanded_dirs(&self) -> impl Iterator<Item = &PathBuf> {
        self.expanded.iter()
    }

    fn use_now(&mut self) {
        self.used = std::time::Instant::now();
    }

    fn rows(&self, root: &Path) -> Vec<vmux_core::event::TreeRow> {
        crate::explorer_model::flatten_tree(root, &self.expanded, &self.loading, &self.children)
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
            MutationPlugin,
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
            ExplorerPanelWidth,
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
    pub(super) fn focus_effect(
        &mut self,
        path: &Path,
        reveal: vmux_core::event::ExplorerReveal,
    ) -> vmux_core::event::ExplorerFocusEvent {
        self.focus_revision = self.focus_revision.wrapping_add(1).max(1);
        vmux_core::event::ExplorerFocusEvent {
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
