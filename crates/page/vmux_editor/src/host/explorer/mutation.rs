use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use vmux_core::event::{ExplorerCreate, ExplorerDelete, ExplorerFsResult, ExplorerRename};

use super::{ExplorerState, ExplorerTreeDirty, ExplorerTrees, OpenEditorsDirty};
use crate::host::editor::FileView;

pub(super) struct ExplorerMutationPlugin;

impl Plugin for ExplorerMutationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ExplorerCreate,
            ExplorerRename,
            ExplorerDelete,
        )>::default())
            .add_systems(Update, drain_explorer_mutations)
            .add_observer(on_explorer_create)
            .add_observer(on_explorer_rename)
            .add_observer(on_explorer_delete);
    }
}

#[derive(Clone)]
enum ExplorerMutation {
    Create {
        parent: PathBuf,
        name: String,
        is_dir: bool,
    },
    Rename {
        path: PathBuf,
        name: String,
    },
    Delete {
        path: PathBuf,
    },
}

struct ExplorerMutationOutcome {
    changed_path: PathBuf,
    refresh_dir: PathBuf,
    old_path: Option<PathBuf>,
    was_dir: bool,
    open_created: bool,
}

#[derive(Component)]
struct ExplorerMutationTask {
    webview: Entity,
    operation: ExplorerMutation,
    task: Task<Result<ExplorerMutationOutcome, String>>,
}

fn run_explorer_mutation(
    root: PathBuf,
    operation: ExplorerMutation,
) -> Result<ExplorerMutationOutcome, String> {
    match operation {
        ExplorerMutation::Create {
            parent,
            name,
            is_dir,
        } => {
            let changed_path = super::fs::create_entry(&root, &parent, &name, is_dir)?;
            Ok(ExplorerMutationOutcome {
                changed_path,
                refresh_dir: parent,
                old_path: None,
                was_dir: is_dir,
                open_created: !is_dir,
            })
        }
        ExplorerMutation::Rename { path, name } => {
            let refresh_dir = path
                .parent()
                .ok_or_else(|| "Explorer root cannot be changed".to_string())?
                .to_path_buf();
            let next_path = path.with_file_name(&name);
            let knowledge_root = vmux_core::knowledge::KnowledgeVault::user().into_root();
            let rename_plan = (root
                .canonicalize()
                .ok()
                .zip(knowledge_root.canonicalize().ok())
                .is_some_and(|(root, knowledge_root)| root == knowledge_root))
            .then(|| {
                vmux_core::knowledge::KnowledgeIndex::build(&root)
                    .map(|index| {
                        vmux_core::knowledge::KnowledgeRenamePlan::build(&index, &path, &next_path)
                    })
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
            let (changed_path, was_dir) = super::fs::rename_entry(&root, &path, &name)?;
            if let Some(plan) = rename_plan {
                plan.apply().map_err(|error| error.to_string())?;
            }
            Ok(ExplorerMutationOutcome {
                changed_path,
                refresh_dir,
                old_path: Some(path),
                was_dir,
                open_created: false,
            })
        }
        ExplorerMutation::Delete { path } => {
            let (refresh_dir, was_dir) = super::fs::delete_entry(&root, &path)?;
            Ok(ExplorerMutationOutcome {
                changed_path: path.clone(),
                refresh_dir,
                old_path: Some(path),
                was_dir,
                open_created: false,
            })
        }
    }
}

fn start_explorer_mutation(
    entity: Entity,
    root: PathBuf,
    operation: ExplorerMutation,
    commands: &mut Commands,
) {
    let task_operation = operation.clone();
    let task = IoTaskPool::get().spawn(async move { run_explorer_mutation(root, task_operation) });
    commands.spawn(ExplorerMutationTask {
        webview: entity,
        operation,
        task,
    });
}

fn on_explorer_create(
    trigger: On<BinReceive<ExplorerCreate>>,
    query: Query<&ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(state) = query.get(entity) else {
        return;
    };
    let payload = &trigger.event().payload;
    start_explorer_mutation(
        entity,
        state.root.clone(),
        ExplorerMutation::Create {
            parent: PathBuf::from(&payload.parent),
            name: payload.name.clone(),
            is_dir: payload.is_dir,
        },
        &mut commands,
    );
}

fn on_explorer_rename(
    trigger: On<BinReceive<ExplorerRename>>,
    query: Query<&ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(state) = query.get(entity) else {
        return;
    };
    let payload = &trigger.event().payload;
    start_explorer_mutation(
        entity,
        state.root.clone(),
        ExplorerMutation::Rename {
            path: PathBuf::from(&payload.path),
            name: payload.name.clone(),
        },
        &mut commands,
    );
}

fn on_explorer_delete(
    trigger: On<BinReceive<ExplorerDelete>>,
    query: Query<&ExplorerState>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(state) = query.get(entity) else {
        return;
    };
    start_explorer_mutation(
        entity,
        state.root.clone(),
        ExplorerMutation::Delete {
            path: PathBuf::from(&trigger.event().payload.path),
        },
        &mut commands,
    );
}

fn remap_path(path: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    path.strip_prefix(old).ok().map(|suffix| new.join(suffix))
}

fn explorer_mutation_message(
    operation: &ExplorerMutation,
    outcome: &ExplorerMutationOutcome,
) -> String {
    match operation {
        ExplorerMutation::Create { is_dir: true, .. } => format!(
            "Created folder {}",
            outcome
                .changed_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        ExplorerMutation::Create { is_dir: false, .. } => format!(
            "Created file {}",
            outcome
                .changed_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        ExplorerMutation::Rename { .. } => format!(
            "Renamed to {}",
            outcome
                .changed_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        ExplorerMutation::Delete { path } => format!(
            "Deleted {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
    }
}

fn emit_explorer_fs_result(
    webview: Entity,
    ok: bool,
    message: String,
    open_path: String,
    browsers: &Browsers,
    commands: &mut Commands,
) {
    if browsers.can_emit_to(&webview) {
        commands.trigger(BinHostEmitEvent::from_event(
            webview,
            &ExplorerFsResult {
                ok,
                message,
                open_path,
            },
        ));
    }
}

fn drain_explorer_mutations(
    mut tasks: Query<(Entity, &mut ExplorerMutationTask)>,
    mut views: Query<(&FileView, &mut ExplorerState)>,
    mut trees: ResMut<ExplorerTrees>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (task_entity, mut pending) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut pending.task)) else {
            continue;
        };
        let webview = pending.webview;
        let operation = pending.operation.clone();
        commands.entity(task_entity).despawn();
        let Ok((file_view, mut state)) = views.get_mut(webview) else {
            continue;
        };
        let root = state.root.clone();
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                emit_explorer_fs_result(
                    webview,
                    false,
                    error,
                    String::new(),
                    &browsers,
                    &mut commands,
                );
                continue;
            }
        };
        let mut open_path = if outcome.open_created {
            Some(outcome.changed_path.clone())
        } else {
            None
        };
        if let Some(old_path) = outcome.old_path.as_ref() {
            match &operation {
                ExplorerMutation::Rename { .. } => {
                    for open in &mut state.open_editors {
                        if let Some(remapped) = remap_path(open, old_path, &outcome.changed_path) {
                            *open = remapped;
                        }
                    }
                    if let Some(remapped) =
                        remap_path(&file_view.path, old_path, &outcome.changed_path)
                    {
                        open_path = Some(remapped);
                    }
                }
                ExplorerMutation::Delete { .. } => {
                    state
                        .open_editors
                        .retain(|open| !open.starts_with(old_path));
                    if file_view.path.starts_with(old_path) {
                        open_path = Some(outcome.refresh_dir.clone());
                    }
                }
                ExplorerMutation::Create { .. } => {}
            }
            if outcome.was_dir {
                trees.at(&root).evict_subtree(old_path);
                trees.touch(&root);
            }
        }
        trees.start_dir_load(&root, outcome.refresh_dir.clone(), &mut commands, true);
        commands
            .entity(webview)
            .insert((ExplorerTreeDirty, OpenEditorsDirty));
        emit_explorer_fs_result(
            webview,
            true,
            explorer_mutation_message(&operation, &outcome),
            open_path
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            &browsers,
            &mut commands,
        );
    }
}
