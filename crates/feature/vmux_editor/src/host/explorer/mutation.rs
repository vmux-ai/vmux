use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use vmux_core::event::{ExplorerCreate, ExplorerDelete, ExplorerFsResult, ExplorerRename};

use super::{
    ExplorerState, ExplorerTree, ExplorerTreeChanged, ExplorerTreeDirty, OpenEditorsDirty,
    UsesExplorerTree,
};
use crate::host::editor::FileView;

pub(super) struct MutationPlugin;

impl Plugin for MutationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ExplorerCreate,
            ExplorerRename,
            ExplorerDelete,
        )>::default())
            .add_systems(
                Update,
                (
                    drain_explorer_creates,
                    drain_explorer_renames,
                    drain_explorer_deletes,
                ),
            )
            .add_observer(on_explorer_create)
            .add_observer(on_explorer_rename)
            .add_observer(on_explorer_delete);
    }
}

struct ExplorerCreateOutcome {
    path: PathBuf,
    parent: PathBuf,
    is_dir: bool,
}

struct ExplorerRenameOutcome {
    old_path: PathBuf,
    new_path: PathBuf,
    parent: PathBuf,
    was_dir: bool,
}

struct ExplorerDeleteOutcome {
    path: PathBuf,
    parent: PathBuf,
    was_dir: bool,
}

#[derive(Component)]
struct ExplorerCreateTask {
    webview: Entity,
    task: Task<Result<ExplorerCreateOutcome, String>>,
}

#[derive(Component)]
struct ExplorerRenameTask {
    webview: Entity,
    task: Task<Result<ExplorerRenameOutcome, String>>,
}

#[derive(Component)]
struct ExplorerDeleteTask {
    webview: Entity,
    task: Task<Result<ExplorerDeleteOutcome, String>>,
}

fn on_explorer_create(
    trigger: On<UiInput<ExplorerCreate>>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(tree_of) = views.get(entity) else {
        return;
    };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let payload = &trigger.event().payload;
    let root = tree.root.clone();
    let parent = PathBuf::from(&payload.parent);
    let name = payload.name.clone();
    let is_dir = payload.is_dir;
    let task = IoTaskPool::get().spawn(async move {
        let path = super::fs::create_entry(&root, &parent, &name, is_dir)?;
        Ok(ExplorerCreateOutcome {
            path,
            parent,
            is_dir,
        })
    });
    commands.spawn(ExplorerCreateTask {
        webview: entity,
        task,
    });
}

fn on_explorer_rename(
    trigger: On<UiInput<ExplorerRename>>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(tree_of) = views.get(entity) else {
        return;
    };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let payload = &trigger.event().payload;
    let root = tree.root.clone();
    let old_path = PathBuf::from(&payload.path);
    let name = payload.name.clone();
    let task = IoTaskPool::get().spawn(async move {
        let parent = old_path
            .parent()
            .ok_or_else(|| "Explorer root cannot be changed".to_string())?
            .to_path_buf();
        let next_path = old_path.with_file_name(&name);
        let knowledge_root = vmux_core::knowledge::KnowledgeVault::user().into_root();
        let rename_plan = (root
            .canonicalize()
            .ok()
            .zip(knowledge_root.canonicalize().ok())
            .is_some_and(|(root, knowledge_root)| root == knowledge_root))
        .then(|| {
            vmux_core::knowledge::KnowledgeIndex::build(&root)
                .map(|index| {
                    vmux_core::knowledge::KnowledgeRenamePlan::build(&index, &old_path, &next_path)
                })
                .map_err(|error| error.to_string())
        })
        .transpose()?;
        let (new_path, was_dir) = super::fs::rename_entry(&root, &old_path, &name)?;
        if let Some(plan) = rename_plan {
            plan.apply().map_err(|error| error.to_string())?;
        }
        Ok(ExplorerRenameOutcome {
            old_path,
            new_path,
            parent,
            was_dir,
        })
    });
    commands.spawn(ExplorerRenameTask {
        webview: entity,
        task,
    });
}

fn on_explorer_delete(
    trigger: On<UiInput<ExplorerDelete>>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(tree_of) = views.get(entity) else {
        return;
    };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let root = tree.root.clone();
    let path = PathBuf::from(&trigger.event().payload.path);
    let task = IoTaskPool::get().spawn(async move {
        let (parent, was_dir) = super::fs::delete_entry(&root, &path)?;
        Ok(ExplorerDeleteOutcome {
            path,
            parent,
            was_dir,
        })
    });
    commands.spawn(ExplorerDeleteTask {
        webview: entity,
        task,
    });
}

fn drain_explorer_creates(
    mut tasks: Query<(Entity, &mut ExplorerCreateTask)>,
    views: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let webview = pending.webview;
        commands.entity(task_entity).despawn();
        let Ok(tree_of) = views.get(webview) else {
            continue;
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                if browsers.can_emit_to(&webview) {
                    commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                        webview,
                        &ExplorerFsResult {
                            ok: false,
                            message: error,
                            open_path: String::new(),
                        },
                    ));
                }
                continue;
            }
        };
        if let Ok(mut tree) = trees.get_mut(tree_of.0)
            && tree.begin_dir_load(&outcome.parent, true)
        {
            commands.spawn(super::tree::ExplorerDirLoadRequest::new(
                tree_of.0,
                outcome.parent,
            ));
            commands.trigger(ExplorerTreeChanged(tree_of.0));
        }
        commands
            .entity(webview)
            .insert((ExplorerTreeDirty, OpenEditorsDirty));
        if browsers.can_emit_to(&webview) {
            let kind = if outcome.is_dir { "folder" } else { "file" };
            let open_path = if outcome.is_dir {
                String::new()
            } else {
                outcome.path.to_string_lossy().into_owned()
            };
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                webview,
                &ExplorerFsResult {
                    ok: true,
                    message: format!(
                        "Created {kind} {}",
                        outcome
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ),
                    open_path,
                },
            ));
        }
    }
}

fn drain_explorer_renames(
    mut tasks: Query<(Entity, &mut ExplorerRenameTask)>,
    mut views: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let webview = pending.webview;
        commands.entity(task_entity).despawn();
        let Ok((file_view, mut state, tree_of)) = views.get_mut(webview) else {
            continue;
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                if browsers.can_emit_to(&webview) {
                    commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                        webview,
                        &ExplorerFsResult {
                            ok: false,
                            message: error,
                            open_path: String::new(),
                        },
                    ));
                }
                continue;
            }
        };
        for open in &mut state.open_editors {
            if let Ok(suffix) = open.strip_prefix(&outcome.old_path) {
                *open = outcome.new_path.join(suffix);
            }
        }
        let open_path = if let Ok(suffix) = file_view.path.strip_prefix(&outcome.old_path) {
            outcome.new_path.join(suffix).to_string_lossy().into_owned()
        } else {
            String::new()
        };
        if let Ok(mut tree) = trees.get_mut(tree_of.0) {
            if outcome.was_dir {
                tree.evict_subtree(&outcome.old_path);
            }
            if tree.begin_dir_load(&outcome.parent, true) {
                commands.spawn(super::tree::ExplorerDirLoadRequest::new(
                    tree_of.0,
                    outcome.parent,
                ));
            }
            commands.trigger(ExplorerTreeChanged(tree_of.0));
        }
        commands
            .entity(webview)
            .insert((ExplorerTreeDirty, OpenEditorsDirty));
        if browsers.can_emit_to(&webview) {
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                webview,
                &ExplorerFsResult {
                    ok: true,
                    message: format!(
                        "Renamed to {}",
                        outcome
                            .new_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ),
                    open_path,
                },
            ));
        }
    }
}

fn drain_explorer_deletes(
    mut tasks: Query<(Entity, &mut ExplorerDeleteTask)>,
    mut views: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let webview = pending.webview;
        commands.entity(task_entity).despawn();
        let Ok((file_view, mut state, tree_of)) = views.get_mut(webview) else {
            continue;
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                if browsers.can_emit_to(&webview) {
                    commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                        webview,
                        &ExplorerFsResult {
                            ok: false,
                            message: error,
                            open_path: String::new(),
                        },
                    ));
                }
                continue;
            }
        };
        state
            .open_editors
            .retain(|open| !open.starts_with(&outcome.path));
        let open_path = if file_view.path.starts_with(&outcome.path) {
            outcome.parent.to_string_lossy().into_owned()
        } else {
            String::new()
        };
        if let Ok(mut tree) = trees.get_mut(tree_of.0) {
            if outcome.was_dir {
                tree.evict_subtree(&outcome.path);
            }
            if tree.begin_dir_load(&outcome.parent, true) {
                commands.spawn(super::tree::ExplorerDirLoadRequest::new(
                    tree_of.0,
                    outcome.parent,
                ));
            }
            commands.trigger(ExplorerTreeChanged(tree_of.0));
        }
        commands
            .entity(webview)
            .insert((ExplorerTreeDirty, OpenEditorsDirty));
        if browsers.can_emit_to(&webview) {
            commands.trigger(vmux_core::host::FileUiStateWrite::from_event(
                webview,
                &ExplorerFsResult {
                    ok: true,
                    message: format!(
                        "Deleted {}",
                        outcome
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ),
                    open_path,
                },
            ));
        }
    }
}
