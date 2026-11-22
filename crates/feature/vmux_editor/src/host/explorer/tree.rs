use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::prelude::{App, Changed, Commands, Plugin};
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::UiInput;
use vmux_ecs::event::*;

use super::panel::StackExplorerVisibility;
use super::{
    ExplorerDirectory, ExplorerDirectoryEntries, ExplorerDirectoryExpanded,
    ExplorerDirectoryLoading, ExplorerDirsChanged, ExplorerPanelDefaults, ExplorerState,
    ExplorerTree, ExplorerTreeChanged, ExplorerTreeDirty, ExplorerTreeUsers, RevealCurrent,
    UsesExplorerTree,
};
use crate::host::editor::FileView;
use crate::host::file_lifecycle::FileDir;

pub(super) struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExplorerDirsChanged>()
            .add_systems(
                Update,
                (
                    init_state,
                    refresh_changed_dirs,
                    start_dir_loads,
                    drain_dir_loads,
                    reveal_on_file_change,
                    emit,
                    prune_idle_trees,
                )
                    .chain(),
            )
            .add_observer(mark_dirty)
            .add_observer(request_reveal)
            .add_observer(reveal_current)
            .add_observer(collapse_all)
            .add_observer(toggle)
            .add_observer(prefetch)
            .add_observer(refresh)
            .add_observer(evict_subtree)
            .add_observer(refresh_directory);
    }
}

#[derive(EntityEvent)]
pub(super) struct ExplorerTreeSubtreeEvict {
    #[event_target]
    pub(super) tree: Entity,
    pub(super) path: PathBuf,
}

#[derive(EntityEvent)]
pub(super) struct ExplorerTreeDirRefresh {
    #[event_target]
    pub(super) tree: Entity,
    pub(super) path: PathBuf,
}

#[derive(Component)]
struct ExplorerDirLoadRequest {
    tree: Entity,
    directory: Entity,
    path: PathBuf,
    generation: u64,
}

#[derive(Component)]
struct ExplorerDirLoadTask {
    tree: Entity,
    directory: Entity,
    path: PathBuf,
    generation: u64,
    task: Task<Vec<FileDirEntry>>,
}

type TreeDirtyReady = (With<ExplorerTreeDirty>, With<vmux_ecs::page::PageReady>);

type DirectoryQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static ExplorerDirectory,
        Has<ExplorerDirectoryExpanded>,
        Option<&'static ExplorerDirectoryEntries>,
        Option<&'static ExplorerDirectoryLoading>,
    ),
>;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ExplorerTreeLookup<'w, 's> {
    trees: Query<'w, 's, &'static ExplorerTree>,
    directories: Query<
        'w,
        's,
        (
            &'static ChildOf,
            &'static ExplorerDirectory,
            Option<&'static ExplorerDirectoryEntries>,
        ),
    >,
}

impl ExplorerTreeLookup<'_, '_> {
    pub(super) fn create_parent(&self, tree: Entity, selected: &Path) -> Option<PathBuf> {
        let tree_state = self.trees.get(tree).ok()?;
        let root = tree_state.root();
        if selected.as_os_str().is_empty() {
            return Some(root.to_path_buf());
        }
        if selected == root {
            return Some(selected.to_path_buf());
        }
        for (parent, dir, entries) in &self.directories {
            if parent.parent() != tree {
                continue;
            }
            if dir.path == selected {
                return Some(selected.to_path_buf());
            }
            if let Some(entries) = entries
                && entries
                    .0
                    .iter()
                    .any(|entry| entry.is_dir && Path::new(&entry.path) == selected)
            {
                return Some(selected.to_path_buf());
            }
        }
        let parent = selected
            .parent()
            .filter(|parent| parent.starts_with(root))
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());
        Some(parent)
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ExplorerTreeActions<'w, 's> {
    trees: Query<'w, 's, (Entity, &'static mut ExplorerTree)>,
    directories: DirectoryQuery<'w, 's>,
    browsers: crate::host::delivery::Surface<'w, 's>,
    commands: Commands<'w, 's>,
}

impl ExplorerTreeActions<'_, '_> {
    fn find_directory(
        &self,
        tree_entity: Entity,
        path: &Path,
    ) -> Option<(Entity, bool, bool, bool)> {
        for (entity, parent, dir, expanded, entries, loading) in &self.directories {
            if parent.parent() == tree_entity && dir.path == path {
                return Some((entity, expanded, entries.is_some(), loading.is_some()));
            }
        }
        None
    }

    fn ensure_and_load(
        &mut self,
        tree_entity: Entity,
        path: &Path,
        expand: bool,
        force: bool,
    ) -> bool {
        let existing = self.find_directory(tree_entity, path);
        let Ok((_, mut tree)) = self.trees.get_mut(tree_entity) else {
            return false;
        };
        tree.use_now();
        let mut changed = false;
        match existing {
            Some((dir_entity, expanded, has_entries, is_loading)) => {
                if expand && !expanded {
                    self.commands
                        .entity(dir_entity)
                        .insert(ExplorerDirectoryExpanded);
                    changed = true;
                }
                let should_load = force || (!is_loading && !has_entries);
                if should_load && (!is_loading || force) {
                    let generation = tree.next_generation();
                    self.commands
                        .entity(dir_entity)
                        .insert(ExplorerDirectoryLoading { generation });
                    self.commands.spawn(ExplorerDirLoadRequest {
                        tree: tree_entity,
                        directory: dir_entity,
                        path: path.to_path_buf(),
                        generation,
                    });
                    changed = true;
                }
            }
            None => {
                let generation = tree.next_generation();
                let mut spawned = self.commands.spawn((
                    ChildOf(tree_entity),
                    ExplorerDirectory {
                        path: path.to_path_buf(),
                    },
                    ExplorerDirectoryLoading { generation },
                ));
                if expand {
                    spawned.insert(ExplorerDirectoryExpanded);
                }
                let dir_entity = spawned.id();
                self.commands.spawn(ExplorerDirLoadRequest {
                    tree: tree_entity,
                    directory: dir_entity,
                    path: path.to_path_buf(),
                    generation,
                });
                changed = true;
            }
        }
        changed
    }

    fn rows(&self, tree_entity: Entity, root: &Path) -> Vec<TreeRow> {
        let mut index: HashMap<&Path, (bool, bool, Option<&[FileDirEntry]>)> = HashMap::new();
        for (_, parent, dir, expanded, entries, loading) in &self.directories {
            if parent.parent() != tree_entity {
                continue;
            }
            index.insert(
                dir.path.as_path(),
                (
                    expanded,
                    loading.is_some(),
                    entries.map(|entries| entries.0.as_slice()),
                ),
            );
        }
        let mut rows = Vec::new();
        Self::append_rows(&index, root, 0, &mut rows);
        rows
    }

    fn append_rows(
        index: &HashMap<&Path, (bool, bool, Option<&[FileDirEntry]>)>,
        directory: &Path,
        depth: u16,
        rows: &mut Vec<TreeRow>,
    ) {
        let Some((_, _, Some(entries))) = index.get(directory) else {
            return;
        };
        for entry in *entries {
            let path = Path::new(&entry.path);
            let (expanded, loading) = match index.get(path) {
                Some((expanded, loading, _)) => (entry.is_dir && *expanded, *loading),
                None => (false, false),
            };
            rows.push(TreeRow {
                name: entry.name.clone(),
                path: entry.path.clone(),
                depth,
                is_dir: entry.is_dir,
                expanded,
                loading,
            });
            if expanded {
                Self::append_rows(index, path, depth + 1, rows);
            }
        }
    }

    fn reveal(
        &mut self,
        entity: Entity,
        current: &Path,
        state: &mut ExplorerState,
        tree_entity: Entity,
    ) {
        let root = match self.trees.get(tree_entity) {
            Ok((_, tree)) => tree.root().to_path_buf(),
            Err(_) => return,
        };
        let current_dir = if current.is_dir() {
            current
        } else {
            current.parent().unwrap_or(current)
        };
        let Ok(relative) = current_dir.strip_prefix(&root) else {
            return;
        };
        let mut dir = root;
        let mut tree_changed = self.ensure_and_load(tree_entity, &dir, true, false);
        for component in relative.components() {
            dir.push(component);
            tree_changed |= self.ensure_and_load(tree_entity, &dir, true, false);
        }
        if tree_changed {
            self.commands.trigger(ExplorerTreeChanged(tree_entity));
            state.focus_path = Some(current.to_path_buf());
            self.commands.entity(entity).insert(ExplorerTreeDirty);
        }
    }

    fn focus(
        &mut self,
        entity: Entity,
        current: &Path,
        reveal: ExplorerReveal,
        state: &mut ExplorerState,
    ) {
        if !self.browsers.can_emit_to(&entity) {
            return;
        }
        let effect = state.focus_effect(current, reveal);
        self.commands
            .trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &effect));
    }
}

fn init_state(
    mut query: Query<(
        Entity,
        &FileView,
        &mut ExplorerState,
        Option<&UsesExplorerTree>,
    )>,
    mut actions: ExplorerTreeActions,
) {
    for (entity, view, mut state, tree_of) in &mut query {
        let root = FileDir::project_root(&view.path);
        if let Some(tree_of) = tree_of
            && let Ok((_, tree)) = actions.trees.get(tree_of.0)
            && tree.answers_for(&root)
        {
            continue;
        }
        state.focus_path = None;
        let mut selected = None;
        for (tree_entity, tree) in &actions.trees {
            if tree.answers_for(&root) {
                selected = Some(tree_entity);
                break;
            }
        }
        let tree_entity = match selected {
            Some(tree_entity) => {
                actions.ensure_and_load(tree_entity, &root, true, false);
                tree_entity
            }
            None => {
                let mut tree = ExplorerTree::new(root.clone());
                let generation = tree.next_generation();
                let tree_entity = actions.commands.spawn(tree).id();
                let dir_entity = actions
                    .commands
                    .spawn((
                        ChildOf(tree_entity),
                        ExplorerDirectory { path: root.clone() },
                        ExplorerDirectoryExpanded,
                        ExplorerDirectoryLoading { generation },
                    ))
                    .id();
                actions.commands.spawn(ExplorerDirLoadRequest {
                    tree: tree_entity,
                    directory: dir_entity,
                    path: root,
                    generation,
                });
                tree_entity
            }
        };
        actions
            .commands
            .entity(entity)
            .insert((UsesExplorerTree(tree_entity), ExplorerTreeDirty));
    }
}

fn refresh_changed_dirs(
    mut changes: MessageReader<ExplorerDirsChanged>,
    mut actions: ExplorerTreeActions,
) {
    let mut changed_dirs = std::collections::HashSet::new();
    for event in changes.read() {
        for dir in &event.0 {
            changed_dirs.insert(dir.clone());
        }
    }
    if changed_dirs.is_empty() {
        return;
    }
    let mut targets = Vec::new();
    for (_, parent, dir, _, entries, _) in &actions.directories {
        if entries.is_none() {
            continue;
        }
        let canonical = vmux_path::PathIdentity::resolve(&dir.path).into_path_buf();
        if changed_dirs.contains(&canonical) {
            targets.push((parent.parent(), dir.path.clone()));
        }
    }
    let mut notified = std::collections::HashSet::new();
    for (tree_entity, path) in targets {
        if actions.ensure_and_load(tree_entity, &path, false, true) && notified.insert(tree_entity)
        {
            actions.commands.trigger(ExplorerTreeChanged(tree_entity));
        }
    }
}

fn start_dir_loads(
    requests: Query<(Entity, &ExplorerDirLoadRequest), Added<ExplorerDirLoadRequest>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    for (entity, request) in &requests {
        let path = request.path.clone();
        let wake = wake.clone();
        let task = IoTaskPool::get().spawn(async move {
            let entries = FileDir::read(&path);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            entries
        });
        commands
            .entity(entity)
            .remove::<ExplorerDirLoadRequest>()
            .insert(ExplorerDirLoadTask {
                tree: request.tree,
                directory: request.directory,
                path: request.path.clone(),
                generation: request.generation,
                task,
            });
    }
}

fn drain_dir_loads(
    mut tasks: Query<(Entity, &mut ExplorerDirLoadTask)>,
    mut actions: ExplorerTreeActions,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some(entries) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        actions.commands.entity(task_entity).despawn();
        let Ok((_, parent, _, expanded, _, loading)) = actions.directories.get(pending.directory)
        else {
            continue;
        };
        if parent.parent() != pending.tree
            || loading.is_none_or(|state| state.generation != pending.generation)
        {
            continue;
        }
        let Ok((_, mut tree)) = actions.trees.get_mut(pending.tree) else {
            continue;
        };
        tree.use_now();
        let mut ahead = Vec::new();
        if expanded {
            for entry in &entries {
                if !entry.is_dir {
                    continue;
                }
                if ahead.len() == ExplorerTree::WARM_AHEAD {
                    break;
                }
                ahead.push(PathBuf::from(&entry.path));
            }
        }
        let _ = &pending.path;
        actions
            .commands
            .entity(pending.directory)
            .remove::<ExplorerDirectoryLoading>()
            .insert(ExplorerDirectoryEntries(entries));
        actions.commands.trigger(ExplorerTreeChanged(pending.tree));
        for dir in ahead {
            actions.ensure_and_load(pending.tree, &dir, false, false);
        }
    }
}

fn mark_dirty(
    trigger: On<ExplorerTreeChanged>,
    trees: Query<&ExplorerTreeUsers>,
    mut commands: Commands,
) {
    let Ok(views) = trees.get(trigger.event().0) else {
        return;
    };
    for entity in &views.0 {
        commands.entity(*entity).insert(ExplorerTreeDirty);
    }
}

fn prune_idle_trees(
    mut closed: RemovedComponents<ExplorerState>,
    trees: Query<(Entity, &ExplorerTree, Option<&ExplorerTreeUsers>)>,
    mut commands: Commands,
) {
    if closed.read().count() == 0 {
        return;
    }
    let mut idle = Vec::new();
    for (entity, tree, views) in &trees {
        if views.is_none_or(|views| views.0.is_empty()) {
            idle.push((tree.used, entity));
        }
    }
    if idle.len() <= ExplorerTree::IDLE_CAPACITY {
        return;
    }
    idle.sort_by_key(|(used, _)| *used);
    let drop_count = idle.len() - ExplorerTree::IDLE_CAPACITY;
    for (_, entity) in idle.into_iter().take(drop_count) {
        commands.entity(entity).despawn();
    }
}

fn reveal_on_file_change(
    mut views: Query<(Entity, &FileView, &mut ExplorerState, &UsesExplorerTree), Changed<FileView>>,
    child_of: Query<&ChildOf>,
    visibility: Query<&StackExplorerVisibility>,
    panel: Single<&ExplorerPanelDefaults>,
    mut actions: ExplorerTreeActions,
) {
    for (entity, view, mut state, tree_of) in &mut views {
        let scope = child_of.get(entity).map(ChildOf::parent).unwrap_or(entity);
        let visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        if !visible {
            continue;
        }
        actions.reveal(entity, &view.path, &mut state, tree_of.0);
        actions.focus(entity, &view.path, ExplorerReveal::Followed, &mut state);
    }
}

fn emit(
    mut query: Query<(Entity, &FileView, &mut ExplorerState, &UsesExplorerTree), TreeDirtyReady>,
    mut actions: ExplorerTreeActions,
) {
    if !actions.browsers.available() {
        return;
    }
    for (entity, view, mut state, tree_of) in &mut query {
        if !actions.browsers.can_emit_to(&entity) {
            continue;
        }
        let Ok((_, tree)) = actions.trees.get(tree_of.0) else {
            continue;
        };
        let root = tree.root().to_path_buf();
        let root_name = tree.root_name();
        let rows = actions.rows(tree_of.0, &root);
        let root_loading = actions
            .find_directory(tree_of.0, &root)
            .is_some_and(|(_, _, _, loading)| loading);
        let focus_ready = state.focus_path.as_ref().is_some_and(|path| {
            path == &root || rows.iter().any(|row| Path::new(&row.path) == path)
        });
        let focus_path = if focus_ready {
            state.focus_path.take()
        } else {
            None
        };
        actions
            .commands
            .trigger(vmux_ecs::FileUiStateWrite::from_event(
                entity,
                &ExplorerTreeEvent {
                    root_name,
                    root_path: root.to_string_lossy().into_owned(),
                    current_path: view.path.to_string_lossy().into_owned(),
                    loading: root_loading,
                    rows,
                },
            ));
        if let Some(focus_path) = focus_path {
            actions.focus(entity, &focus_path, ExplorerReveal::Followed, &mut state);
        }
        actions
            .commands
            .entity(entity)
            .remove::<ExplorerTreeDirty>();
    }
}

fn toggle(
    trigger: On<UiInput<ExplorerTreeToggle>>,
    query: Query<&UsesExplorerTree>,
    mut actions: ExplorerTreeActions,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let existing = actions.find_directory(tree_of.0, &path);
    if let Some((dir_entity, true, _, _)) = existing {
        if let Ok((_, mut tree)) = actions.trees.get_mut(tree_of.0) {
            tree.use_now();
        }
        actions
            .commands
            .entity(dir_entity)
            .remove::<ExplorerDirectoryExpanded>();
        actions.commands.trigger(ExplorerTreeChanged(tree_of.0));
        return;
    }
    let allowed = actions
        .trees
        .get(tree_of.0)
        .is_ok_and(|(_, tree)| tree.allows(&path));
    if !allowed {
        return;
    }
    actions.ensure_and_load(tree_of.0, &path, true, false);
    actions.commands.trigger(ExplorerTreeChanged(tree_of.0));
}

fn prefetch(
    trigger: On<UiInput<ExplorerTreePrefetch>>,
    query: Query<&UsesExplorerTree>,
    mut actions: ExplorerTreeActions,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let allowed = actions
        .trees
        .get(tree_of.0)
        .is_ok_and(|(_, tree)| tree.allows(&path));
    if allowed && actions.ensure_and_load(tree_of.0, &path, false, false) {
        actions.commands.trigger(ExplorerTreeChanged(tree_of.0));
    }
}

fn refresh(
    trigger: On<UiInput<ExplorerTreeRefresh>>,
    query: Query<&UsesExplorerTree>,
    mut actions: ExplorerTreeActions,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let allowed = actions
        .trees
        .get(tree_of.0)
        .is_ok_and(|(_, tree)| tree.allows(&path));
    if !allowed {
        return;
    }
    let mut targets = vec![path.clone()];
    for (_, parent, dir, expanded, _, _) in &actions.directories {
        if parent.parent() == tree_of.0
            && expanded
            && dir.path != path
            && dir.path.starts_with(&path)
        {
            targets.push(dir.path.clone());
        }
    }
    let mut changed = false;
    for target in targets {
        changed |= actions.ensure_and_load(tree_of.0, &target, false, true);
    }
    if changed {
        actions.commands.trigger(ExplorerTreeChanged(tree_of.0));
    }
}

fn evict_subtree(trigger: On<ExplorerTreeSubtreeEvict>, mut actions: ExplorerTreeActions) {
    let tree_entity = trigger.tree;
    let path = &trigger.path;
    let Ok((_, mut tree)) = actions.trees.get_mut(tree_entity) else {
        return;
    };
    tree.use_now();
    let mut to_remove = Vec::new();
    for (dir_entity, parent, dir, _, _, _) in &actions.directories {
        if parent.parent() == tree_entity && dir.path.starts_with(path) {
            to_remove.push(dir_entity);
        }
    }
    for dir_entity in to_remove {
        actions.commands.entity(dir_entity).despawn();
    }
    actions.commands.trigger(ExplorerTreeChanged(tree_entity));
}

fn refresh_directory(trigger: On<ExplorerTreeDirRefresh>, mut actions: ExplorerTreeActions) {
    let tree_entity = trigger.tree;
    if actions.ensure_and_load(tree_entity, &trigger.path, false, true) {
        actions.commands.trigger(ExplorerTreeChanged(tree_entity));
    }
}

fn request_reveal(trigger: On<UiInput<ExplorerRevealCurrent>>, mut commands: Commands) {
    commands.trigger(RevealCurrent {
        entity: trigger.event().webview,
        reveal: ExplorerReveal::Requested,
    });
}

fn reveal_current(
    trigger: On<RevealCurrent>,
    mut query: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut actions: ExplorerTreeActions,
) {
    let entity = trigger.event().entity;
    let Ok((view, mut state, tree_of)) = query.get_mut(entity) else {
        return;
    };
    actions.reveal(entity, &view.path, &mut state, tree_of.0);
    actions.focus(entity, &view.path, trigger.event().reveal, &mut state);
}

fn collapse_all(
    trigger: On<UiInput<ExplorerCollapseAll>>,
    query: Query<&UsesExplorerTree>,
    mut actions: ExplorerTreeActions,
) {
    let entity = trigger.event().webview;
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let root = match actions.trees.get_mut(tree_of.0) {
        Ok((_, mut tree)) => {
            tree.use_now();
            tree.root().to_path_buf()
        }
        Err(_) => return,
    };
    let mut collapsed = Vec::new();
    for (dir_entity, parent, dir, expanded, _, _) in &actions.directories {
        if parent.parent() == tree_of.0 && expanded && dir.path != root {
            collapsed.push(dir_entity);
        }
    }
    for dir_entity in collapsed {
        actions
            .commands
            .entity(dir_entity)
            .remove::<ExplorerDirectoryExpanded>();
    }
    actions.commands.trigger(ExplorerTreeChanged(tree_of.0));
}
