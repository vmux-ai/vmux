use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use vmux_core::event::*;

use super::panel::StackExplorerVisibility;
use super::{
    ExplorerPanelDefaults, ExplorerState, ExplorerTree, ExplorerTreeChanged, ExplorerTreeDirty,
    ExplorerTreeUsers, IDLE_TREE_CAPACITY, RevealCurrent, UsesExplorerTree,
};
use crate::dir::{list_dir, project_root};
use crate::host::editor::FileView;

#[derive(Component)]
pub(crate) struct ExplorerDirLoadRequest {
    tree: Entity,
    path: PathBuf,
}

impl ExplorerDirLoadRequest {
    pub(crate) fn new(tree: Entity, path: PathBuf) -> Self {
        Self { tree, path }
    }
}

#[derive(Component)]
struct ExplorerDirLoadTask {
    tree: Entity,
    task: Task<(PathBuf, Vec<FileDirEntry>)>,
}

type TreeDirtyReady = (With<ExplorerTreeDirty>, With<vmux_core::page::PageReady>);

pub(super) struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                init_explorer_state,
                start_explorer_dir_loads,
                drain_explorer_dir_loads,
                reveal_on_file_change,
                emit_explorer_tree,
                prune_idle_explorer_trees,
            )
                .chain(),
        )
        .add_observer(mark_explorer_tree_dirty)
        .add_observer(on_explorer_reveal_current_input)
        .add_observer(reveal_current)
        .add_observer(on_explorer_collapse_all)
        .add_observer(on_explorer_tree_toggle)
        .add_observer(on_explorer_tree_prefetch)
        .add_observer(on_explorer_tree_refresh);
    }
}

fn reveal_current_in_tree(
    entity: Entity,
    current: &Path,
    state: &mut ExplorerState,
    tree_entity: Entity,
    tree: &mut ExplorerTree,
    commands: &mut Commands,
) {
    let mut tree_changed = false;
    let current_dir = if current.is_dir() {
        current
    } else {
        current.parent().unwrap_or(current)
    };
    let Ok(relative) = current_dir.strip_prefix(&tree.root) else {
        return;
    };
    let mut dir = tree.root.clone();
    tree_changed |= tree.expanded.insert(dir.clone());
    if tree.begin_dir_load(&dir, false) {
        commands.spawn(ExplorerDirLoadRequest::new(tree_entity, dir.clone()));
        tree_changed = true;
    }
    for component in relative.components() {
        dir.push(component);
        tree_changed |= tree.expanded.insert(dir.clone());
        if tree.begin_dir_load(&dir, false) {
            commands.spawn(ExplorerDirLoadRequest::new(tree_entity, dir.clone()));
            tree_changed = true;
        }
    }
    if tree_changed {
        tree.use_now();
        commands.trigger(ExplorerTreeChanged(tree_entity));
    }
    if tree_changed {
        state.focus_path = Some(current.to_path_buf());
        commands.entity(entity).insert(ExplorerTreeDirty);
    }
}

fn emit_explorer_focus(
    entity: Entity,
    current: &Path,
    reveal: ExplorerReveal,
    state: &mut ExplorerState,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if browsers.can_emit_to(&entity) {
        let effect = state.focus_effect(current, reveal);
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity, &effect,
        ));
    }
}

fn init_explorer_state(
    mut query: Query<(
        Entity,
        &FileView,
        &mut ExplorerState,
        Option<&UsesExplorerTree>,
    )>,
    mut trees: Query<(Entity, &mut ExplorerTree)>,
    mut commands: Commands,
) {
    for (entity, view, mut state, tree_of) in &mut query {
        let root = project_root(&view.path);
        if let Some(tree_of) = tree_of
            && let Ok((_, tree)) = trees.get_mut(tree_of.0)
            && tree.answers_for(&root)
        {
            continue;
        }
        state.focus_path = None;
        let mut selected = None;
        for (tree_entity, mut tree) in &mut trees {
            if !tree.answers_for(&root) {
                continue;
            }
            tree.use_now();
            tree.expanded.insert(root.clone());
            if tree.begin_dir_load(&root, false) {
                commands.spawn(ExplorerDirLoadRequest::new(tree_entity, root.clone()));
            }
            selected = Some(tree_entity);
            break;
        }
        let tree_entity = match selected {
            Some(tree_entity) => tree_entity,
            None => {
                let mut tree = ExplorerTree::new(root.clone());
                tree.expanded.insert(root.clone());
                tree.begin_dir_load(&root, false);
                let tree_entity = commands.spawn(tree).id();
                commands.spawn(ExplorerDirLoadRequest::new(tree_entity, root));
                tree_entity
            }
        };
        commands
            .entity(entity)
            .insert((UsesExplorerTree(tree_entity), ExplorerTreeDirty));
    }
}

fn start_explorer_dir_loads(
    requests: Query<(Entity, &ExplorerDirLoadRequest), Added<ExplorerDirLoadRequest>>,
    mut commands: Commands,
) {
    for (entity, request) in &requests {
        let path = request.path.clone();
        let task = IoTaskPool::get().spawn(async move {
            let entries = list_dir(&path);
            (path, entries)
        });
        commands
            .entity(entity)
            .remove::<ExplorerDirLoadRequest>()
            .insert(ExplorerDirLoadTask {
                tree: request.tree,
                task,
            });
    }
}

fn drain_explorer_dir_loads(
    mut tasks: Query<(Entity, &mut ExplorerDirLoadTask)>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some((path, entries)) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        commands.entity(task_entity).despawn();
        let Ok(mut tree) = trees.get_mut(pending.tree) else {
            continue;
        };
        if !tree.loading.remove(&path) {
            continue;
        }
        let warm_ahead = tree.expanded.contains(&path);
        tree.children.insert(path.clone(), entries);
        tree.use_now();
        commands.trigger(ExplorerTreeChanged(pending.tree));
        if !warm_ahead {
            continue;
        }
        let mut ahead = Vec::new();
        for entry in tree.children.get(&path).into_iter().flatten() {
            if !entry.is_dir {
                continue;
            }
            if ahead.len() == EXPLORER_WARM_AHEAD {
                break;
            }
            ahead.push(PathBuf::from(&entry.path));
        }
        for dir in ahead {
            if tree.begin_dir_load(&dir, false) {
                commands.spawn(ExplorerDirLoadRequest::new(pending.tree, dir));
            }
        }
    }
}

fn mark_explorer_tree_dirty(
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

fn prune_idle_explorer_trees(
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
    if idle.len() <= IDLE_TREE_CAPACITY {
        return;
    }
    idle.sort_by_key(|(used, _)| *used);
    let drop_count = idle.len() - IDLE_TREE_CAPACITY;
    for (_, entity) in idle.into_iter().take(drop_count) {
        commands.entity(entity).despawn();
    }
}

fn reveal_on_file_change(
    mut views: Query<(Entity, &FileView, &mut ExplorerState, &UsesExplorerTree), Changed<FileView>>,
    child_of: Query<&ChildOf>,
    visibility: Query<&StackExplorerVisibility>,
    panel: Res<ExplorerPanelDefaults>,
    mut trees: Query<&mut ExplorerTree>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let browsers = browsers.as_deref();
    for (entity, view, mut state, tree_of) in &mut views {
        let scope = child_of.get(entity).map(ChildOf::parent).unwrap_or(entity);
        let visible = visibility
            .get(scope)
            .map(|state| state.visible)
            .unwrap_or(panel.default_visible);
        if !visible {
            continue;
        }
        let Ok(mut tree) = trees.get_mut(tree_of.0) else {
            continue;
        };
        reveal_current_in_tree(
            entity,
            &view.path,
            &mut state,
            tree_of.0,
            &mut tree,
            &mut commands,
        );
        let Some(browsers) = browsers else {
            continue;
        };
        emit_explorer_focus(
            entity,
            &view.path,
            ExplorerReveal::Followed,
            &mut state,
            browsers,
            &mut commands,
        );
    }
}

fn emit_explorer_tree(
    mut query: Query<(Entity, &FileView, &mut ExplorerState, &UsesExplorerTree), TreeDirtyReady>,
    trees: Query<&ExplorerTree>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (entity, view, mut state, tree_of) in &mut query {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let Ok(tree) = trees.get(tree_of.0) else {
            continue;
        };
        let rows = tree.rows(&tree.root);
        let focus_ready = state.focus_path.as_ref().is_some_and(|path| {
            path == &tree.root || rows.iter().any(|row| Path::new(&row.path) == path)
        });
        let focus_path = if focus_ready {
            state.focus_path.take()
        } else {
            None
        };
        commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
            entity,
            &ExplorerTreeEvent {
                root_name: ExplorerRoot::name(&tree.root),
                root_path: tree.root.to_string_lossy().into_owned(),
                current_path: view.path.to_string_lossy().into_owned(),
                loading: tree.is_loading(&tree.root),
                rows,
            },
        ));
        if let Some(focus_path) = focus_path {
            emit_explorer_focus(
                entity,
                &focus_path,
                ExplorerReveal::Followed,
                &mut state,
                &browsers,
                &mut commands,
            );
        }
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
    trigger: On<UiInput<ExplorerTreeToggle>>,
    query: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let Ok(mut tree) = trees.get_mut(tree_of.0) else {
        return;
    };
    if !tree.expanded.remove(&path) {
        if !tree.allows(&path) {
            return;
        }
        tree.expanded.insert(path.clone());
        if tree.begin_dir_load(&path, false) {
            commands.spawn(ExplorerDirLoadRequest::new(tree_of.0, path));
        }
    }
    tree.use_now();
    commands.trigger(ExplorerTreeChanged(tree_of.0));
}

fn on_explorer_tree_prefetch(
    trigger: On<UiInput<ExplorerTreePrefetch>>,
    query: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let Ok(mut tree) = trees.get_mut(tree_of.0) else {
        return;
    };
    if tree.allows(&path) && tree.begin_dir_load(&path, false) {
        commands.spawn(ExplorerDirLoadRequest::new(tree_of.0, path));
        commands.trigger(ExplorerTreeChanged(tree_of.0));
    }
}

fn on_explorer_tree_refresh(
    trigger: On<UiInput<ExplorerTreeRefresh>>,
    query: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let path = PathBuf::from(&trigger.event().payload.path);
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let Ok(mut tree) = trees.get_mut(tree_of.0) else {
        return;
    };
    if tree.allows(&path) && tree.begin_dir_load(&path, true) {
        commands.spawn(ExplorerDirLoadRequest::new(tree_of.0, path));
        commands.trigger(ExplorerTreeChanged(tree_of.0));
    }
}

fn on_explorer_reveal_current_input(
    trigger: On<UiInput<ExplorerRevealCurrent>>,
    mut commands: Commands,
) {
    commands.trigger(RevealCurrent {
        entity: trigger.event().webview,
        reveal: ExplorerReveal::Requested,
    });
}

fn reveal_current(
    trigger: On<RevealCurrent>,
    mut query: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    let Ok((view, mut state, tree_of)) = query.get_mut(entity) else {
        return;
    };
    let Ok(mut tree) = trees.get_mut(tree_of.0) else {
        return;
    };
    reveal_current_in_tree(
        entity,
        &view.path,
        &mut state,
        tree_of.0,
        &mut tree,
        &mut commands,
    );
    if let Some(browsers) = browsers {
        emit_explorer_focus(
            entity,
            &view.path,
            trigger.event().reveal,
            &mut state,
            &browsers,
            &mut commands,
        );
    }
}

fn on_explorer_collapse_all(
    trigger: On<UiInput<ExplorerCollapseAll>>,
    query: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(tree_of) = query.get(entity) else {
        return;
    };
    let Ok(mut tree) = trees.get_mut(tree_of.0) else {
        return;
    };
    let root = tree.root.clone();
    tree.expanded.retain(|path| *path == root);
    tree.use_now();
    commands.trigger(ExplorerTreeChanged(tree_of.0));
}

const EXPLORER_WARM_AHEAD: usize = 64;
