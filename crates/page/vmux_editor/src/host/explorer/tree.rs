use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use vmux_core::event::*;

use crate::dir::{list_dir, project_root};
use crate::explorer_model::flatten_tree;

use super::panel::StackExplorerVisibility;
use super::{
    ExplorerPanelDefaults, ExplorerState, ExplorerTree, ExplorerTreeDirty, ExplorerTrees,
    IDLE_TREE_CAPACITY,
};
use crate::host::editor::FileView;

impl ExplorerTree {
    fn rows(&self, root: &Path) -> Vec<TreeRow> {
        flatten_tree(root, &self.expanded, &self.loading, &self.children)
    }

    pub(super) fn evict_subtree(&mut self, path: &Path) {
        self.expanded.retain(|entry| !entry.starts_with(path));
        self.loading.retain(|entry| !entry.starts_with(path));
        self.children.retain(|entry, _| !entry.starts_with(path));
    }
}

impl ExplorerTrees {
    pub(super) fn at(&mut self, root: &Path) -> &mut ExplorerTree {
        self.clock += 1;
        let used = self.clock;
        let tree = self.by_root.entry(root.to_path_buf()).or_default();
        tree.used = used;
        tree
    }

    pub(super) fn prune(&mut self, live: &HashSet<PathBuf>) {
        let mut idle = Vec::new();
        for (root, tree) in &self.by_root {
            if live.contains(root) {
                continue;
            }
            idle.push((tree.used, root.clone()));
        }
        if idle.len() <= IDLE_TREE_CAPACITY {
            return;
        }
        idle.sort_by_key(|(used, _)| *used);
        let drop_count = idle.len() - IDLE_TREE_CAPACITY;
        for (_, root) in idle.into_iter().take(drop_count) {
            self.by_root.remove(&root);
            self.dirty.remove(&root);
        }
    }

    fn rows(&self, root: &Path) -> Vec<TreeRow> {
        match self.by_root.get(root) {
            Some(tree) => tree.rows(root),
            None => Vec::new(),
        }
    }

    fn is_loading(&self, root: &Path, path: &Path) -> bool {
        self.by_root
            .get(root)
            .is_some_and(|tree| tree.loading.contains(path))
    }

    pub(super) fn touch(&mut self, root: &Path) {
        self.dirty.insert(root.to_path_buf());
    }

    fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    fn take_dirty(&mut self) -> HashSet<PathBuf> {
        std::mem::take(&mut self.dirty)
    }

    pub(super) fn start_dir_load(
        &mut self,
        root: &Path,
        path: PathBuf,
        commands: &mut Commands,
        force: bool,
    ) -> bool {
        let tree = self.at(root);
        if tree.loading.contains(&path) || !force && tree.children.contains_key(&path) {
            return false;
        }
        tree.loading.insert(path.clone());
        let task = IoTaskPool::get().spawn(async move {
            let entries = list_dir(&path);
            (path, entries)
        });
        commands.spawn(ExplorerDirLoadTask {
            root: root.to_path_buf(),
            task,
        });
        self.touch(root);
        true
    }
}

#[derive(Component)]
struct ExplorerDirLoadTask {
    root: PathBuf,
    task: Task<(PathBuf, Vec<FileDirEntry>)>,
}

type TreeDirtyReady = (With<ExplorerTreeDirty>, With<vmux_core::page::PageReady>);

pub(super) struct ExplorerTreePlugin;

impl Plugin for ExplorerTreePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ExplorerTrees>()
            .add_systems(
                Update,
                (
                    init_explorer_state,
                    drain_explorer_dir_loads,
                    reveal_on_file_change,
                    mark_explorer_tree_dirty,
                    emit_explorer_tree,
                    prune_idle_explorer_trees,
                )
                    .chain(),
            )
            .add_observer(on_explorer_reveal_current)
            .add_observer(on_explorer_collapse_all)
            .add_observer(on_explorer_tree_toggle)
            .add_observer(on_explorer_tree_prefetch)
            .add_observer(on_explorer_tree_refresh);
    }
}

pub(super) fn reveal_current_in_tree(
    entity: Entity,
    current: &Path,
    state: &mut ExplorerState,
    trees: &mut ExplorerTrees,
    commands: &mut Commands,
) {
    let mut shared_changed = false;
    let mut page_changed = false;
    let root = project_root(current);
    if state.root != root {
        state.root = root;
        state.focus_path = None;
        page_changed = true;
    }
    let current_dir = if current.is_dir() {
        current
    } else {
        current.parent().unwrap_or(current)
    };
    let Ok(relative) = current_dir.strip_prefix(&state.root) else {
        return;
    };
    let mut dir = state.root.clone();
    shared_changed |= trees.at(&state.root).expanded.insert(dir.clone());
    shared_changed |= trees.start_dir_load(&state.root, dir.clone(), commands, false);
    for component in relative.components() {
        dir.push(component);
        shared_changed |= trees.at(&state.root).expanded.insert(dir.clone());
        shared_changed |= trees.start_dir_load(&state.root, dir.clone(), commands, false);
    }
    if shared_changed {
        trees.touch(&state.root);
    }
    if shared_changed || page_changed {
        state.focus_path = Some(current.to_path_buf());
        commands.entity(entity).insert(ExplorerTreeDirty);
    }
}

pub(super) fn emit_explorer_focus(
    entity: Entity,
    current: &Path,
    reveal: ExplorerReveal,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if browsers.can_emit_to(&entity) {
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &ExplorerFocusEvent {
                path: current.to_string_lossy().into_owned(),
                reveal,
            },
        ));
    }
}

fn init_explorer_state(
    mut query: Query<(Entity, &FileView, &mut ExplorerState)>,
    mut trees: ResMut<ExplorerTrees>,
    mut commands: Commands,
) {
    for (entity, view, mut state) in &mut query {
        if !state.root.as_os_str().is_empty() {
            continue;
        }
        let root = project_root(&view.path);
        state.root = root.clone();
        trees.at(&root).expanded.insert(root.clone());
        trees.start_dir_load(&root, root.clone(), &mut commands, false);
        commands.entity(entity).insert(ExplorerTreeDirty);
    }
}

fn drain_explorer_dir_loads(
    mut tasks: Query<(Entity, &mut ExplorerDirLoadTask)>,
    mut trees: ResMut<ExplorerTrees>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some((path, entries)) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let root = pending.root.clone();
        commands.entity(task_entity).despawn();
        let tree = trees.at(&root);
        if !tree.loading.remove(&path) {
            continue;
        }
        let warm_ahead = tree.expanded.contains(&path);
        tree.children.insert(path.clone(), entries);
        trees.touch(&root);
        if !warm_ahead {
            continue;
        }
        let mut ahead = Vec::new();
        for entry in trees.at(&root).children.get(&path).into_iter().flatten() {
            if !entry.is_dir {
                continue;
            }
            if ahead.len() == EXPLORER_WARM_AHEAD {
                break;
            }
            ahead.push(PathBuf::from(&entry.path));
        }
        for dir in ahead {
            trees.start_dir_load(&root, dir, &mut commands, false);
        }
    }
}

fn mark_explorer_tree_dirty(
    mut trees: ResMut<ExplorerTrees>,
    views: Query<(Entity, &ExplorerState)>,
    mut commands: Commands,
) {
    if !trees.has_dirty() {
        return;
    }
    let dirty = trees.take_dirty();
    for (entity, state) in &views {
        if dirty.contains(&state.root) {
            commands.entity(entity).insert(ExplorerTreeDirty);
        }
    }
}

fn prune_idle_explorer_trees(
    mut closed: RemovedComponents<ExplorerState>,
    views: Query<&ExplorerState>,
    mut trees: ResMut<ExplorerTrees>,
) {
    if closed.read().count() == 0 {
        return;
    }
    let mut live = HashSet::new();
    for state in &views {
        live.insert(state.root.clone());
    }
    trees.prune(&live);
}

fn reveal_on_file_change(
    mut views: Query<(Entity, &FileView, &mut ExplorerState), Changed<FileView>>,
    child_of: Query<&ChildOf>,
    visibility: Query<&StackExplorerVisibility>,
    panel: Res<ExplorerPanelDefaults>,
    mut trees: ResMut<ExplorerTrees>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let browsers = browsers.as_deref();
    for (entity, view, mut state) in &mut views {
        let scope = child_of.get(entity).map(ChildOf::parent).unwrap_or(entity);
        let visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        if !visible {
            continue;
        }
        reveal_current_in_tree(entity, &view.path, &mut state, &mut trees, &mut commands);
        let Some(browsers) = browsers else {
            continue;
        };
        emit_explorer_focus(
            entity,
            &view.path,
            ExplorerReveal::Followed,
            browsers,
            &mut commands,
        );
    }
}

fn emit_explorer_tree(
    mut query: Query<(Entity, &FileView, &mut ExplorerState), TreeDirtyReady>,
    trees: Res<ExplorerTrees>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (entity, view, mut state) in &mut query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let rows = trees.rows(&state.root);
        let focus_ready = state.focus_path.as_ref().is_some_and(|path| {
            path == &state.root || rows.iter().any(|row| Path::new(&row.path) == path)
        });
        let focus_path = if focus_ready {
            state
                .focus_path
                .take()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            String::new()
        };
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &ExplorerTreeEvent {
                root_name: ExplorerRoot::name(&state.root),
                root_path: state.root.to_string_lossy().into_owned(),
                current_path: view.path.to_string_lossy().into_owned(),
                focus_path,
                loading: trees.is_loading(&state.root, &state.root),
                rows,
            },
        ));
        commands.entity(entity).remove::<ExplorerTreeDirty>();
    }
}

struct ExplorerRoot;

impl ExplorerRoot {
    fn name(root: &Path) -> String {
        root.file_name()
            .map(|name| name.to_string_lossy().to_uppercase())
            .unwrap_or_else(|| root.to_string_lossy().to_uppercase())
    }
}

fn on_explorer_tree_toggle(
    trigger: On<BinReceive<ExplorerTreeToggle>>,
    query: Query<&ExplorerState>,
    mut trees: ResMut<ExplorerTrees>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(state) = query.get(entity) else {
        return;
    };
    let root = state.root.clone();
    if !trees.at(&root).expanded.remove(&path) {
        if !state.allows(&path) {
            return;
        }
        trees.at(&root).expanded.insert(path.clone());
        trees.start_dir_load(&root, path, &mut commands, false);
    }
    trees.touch(&root);
    commands.entity(entity).insert(ExplorerTreeDirty);
}

fn on_explorer_tree_prefetch(
    trigger: On<BinReceive<ExplorerTreePrefetch>>,
    query: Query<&ExplorerState>,
    mut trees: ResMut<ExplorerTrees>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(state) = query.get(entity) else {
        return;
    };
    if state.allows(&path) {
        trees.start_dir_load(&state.root, path, &mut commands, false);
    }
}

fn on_explorer_tree_refresh(
    trigger: On<BinReceive<ExplorerTreeRefresh>>,
    query: Query<&ExplorerState>,
    mut trees: ResMut<ExplorerTrees>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(state) = query.get(entity) else {
        return;
    };
    if state.allows(&path) {
        trees.start_dir_load(&state.root, path, &mut commands, true);
    }
}

fn on_explorer_reveal_current(
    trigger: On<BinReceive<ExplorerRevealCurrent>>,
    mut query: Query<(&FileView, &mut ExplorerState)>,
    mut trees: ResMut<ExplorerTrees>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((view, mut state)) = query.get_mut(entity) else {
        return;
    };
    reveal_current_in_tree(entity, &view.path, &mut state, &mut trees, &mut commands);
    if let Some(browsers) = browsers {
        emit_explorer_focus(
            entity,
            &view.path,
            ExplorerReveal::Requested,
            &browsers,
            &mut commands,
        );
    }
}

fn on_explorer_collapse_all(
    trigger: On<BinReceive<ExplorerCollapseAll>>,
    query: Query<&ExplorerState>,
    mut trees: ResMut<ExplorerTrees>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(state) = query.get(entity) else {
        return;
    };
    let root = state.root.clone();
    trees.at(&root).expanded.retain(|path| *path == root);
    trees.touch(&root);
    commands.entity(entity).insert(ExplorerTreeDirty);
}

const EXPLORER_WARM_AHEAD: usize = 64;
