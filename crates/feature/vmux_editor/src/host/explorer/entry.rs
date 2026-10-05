use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::*;
use vmux_ecs::FileUiStateWrite;
use vmux_ecs::event::{
    ExplorerCreateDirectoryPromptRequest, ExplorerCreateFilePromptRequest,
    ExplorerDeletePromptRequest, ExplorerPromptDismissRequest, ExplorerPromptDraftRequest,
    ExplorerPromptState, ExplorerPromptSubmitRequest, ExplorerRenamePromptRequest,
};
use vmux_ecs::page::PageReady;

use super::fs::ExplorerFs;
use super::{
    ExplorerState, ExplorerTree, ExplorerTreeChanged, ExplorerTreeDirty, OpenEditorsDirty,
    UsesExplorerTree,
};
use crate::host::editor::{FileNavigateRequest, FileView};
use crate::host::feedback::ExplorerFeedback;

pub(super) struct ExplorerEntryPlugin;

impl Plugin for ExplorerEntryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(
            ExplorerCreateFilePromptRequest,
            ExplorerCreateDirectoryPromptRequest,
            ExplorerRenamePromptRequest,
            ExplorerDeletePromptRequest,
            ExplorerPromptDraftRequest,
            ExplorerPromptSubmitRequest,
            ExplorerPromptDismissRequest,
        )>::default())
            .add_systems(
                Update,
                (
                    clear_prompt,
                    project.after(clear_prompt),
                    drain_creates,
                    drain_renames,
                    drain_deletes,
                ),
            )
            .add_observer(open_file_prompt)
            .add_observer(open_directory_prompt)
            .add_observer(open_rename_prompt)
            .add_observer(open_delete_prompt)
            .add_observer(update_draft)
            .add_observer(submit_prompt)
            .add_observer(dismiss_prompt)
            .add_observer(create)
            .add_observer(rename)
            .add_observer(delete);
    }
}

#[derive(Component)]
struct CreateFilePrompt;

#[derive(Component)]
struct CreateDirectoryPrompt;

#[derive(Component)]
struct RenamePrompt;

#[derive(Component)]
struct DeletePrompt;

#[derive(Component)]
struct PromptTarget {
    path: PathBuf,
    name: String,
}

#[derive(Component, Default)]
struct PromptDraft(String);

#[derive(Component)]
struct PromptDirty;

#[derive(EntityEvent)]
struct CreateEntry {
    #[event_target]
    entity: Entity,
    parent: PathBuf,
    name: String,
    is_dir: bool,
}

#[derive(EntityEvent)]
struct RenameEntry {
    #[event_target]
    entity: Entity,
    path: PathBuf,
    name: String,
}

#[derive(EntityEvent)]
struct DeleteEntry {
    #[event_target]
    entity: Entity,
    path: PathBuf,
}

type PromptComponents = (
    CreateFilePrompt,
    CreateDirectoryPrompt,
    RenamePrompt,
    DeletePrompt,
    PromptTarget,
    PromptDraft,
);

fn open_file_prompt(
    trigger: On<UiInput<ExplorerCreateFilePromptRequest>>,
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
    let parent = tree.create_parent(Path::new(&trigger.event().payload.path));
    commands
        .entity(entity)
        .remove::<PromptComponents>()
        .insert((
            CreateFilePrompt,
            PromptTarget {
                path: parent,
                name: String::new(),
            },
            PromptDraft::default(),
            PromptDirty,
        ));
}

fn open_directory_prompt(
    trigger: On<UiInput<ExplorerCreateDirectoryPromptRequest>>,
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
    let parent = tree.create_parent(Path::new(&trigger.event().payload.path));
    commands
        .entity(entity)
        .remove::<PromptComponents>()
        .insert((
            CreateDirectoryPrompt,
            PromptTarget {
                path: parent,
                name: String::new(),
            },
            PromptDraft::default(),
            PromptDirty,
        ));
}

fn open_rename_prompt(trigger: On<UiInput<ExplorerRenamePromptRequest>>, mut commands: Commands) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    commands
        .entity(entity)
        .remove::<PromptComponents>()
        .insert((
            RenamePrompt,
            PromptTarget {
                path: PathBuf::from(&request.path),
                name: request.name.clone(),
            },
            PromptDraft(request.name.clone()),
            PromptDirty,
        ));
}

fn open_delete_prompt(trigger: On<UiInput<ExplorerDeletePromptRequest>>, mut commands: Commands) {
    let entity = trigger.event().webview;
    let request = &trigger.event().payload;
    commands
        .entity(entity)
        .remove::<PromptComponents>()
        .insert((
            DeletePrompt,
            PromptTarget {
                path: PathBuf::from(&request.path),
                name: request.name.clone(),
            },
            PromptDraft::default(),
            PromptDirty,
        ));
}

fn update_draft(
    trigger: On<UiInput<ExplorerPromptDraftRequest>>,
    mut drafts: Query<&mut PromptDraft>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok(mut draft) = drafts.get_mut(entity) else {
        return;
    };
    if draft.0 == trigger.event().payload.draft {
        return;
    }
    draft.0.clone_from(&trigger.event().payload.draft);
    commands.entity(entity).insert(PromptDirty);
}

type Prompts<'w, 's> = Query<
    'w,
    's,
    (
        &'static PromptTarget,
        &'static PromptDraft,
        Option<&'static CreateFilePrompt>,
        Option<&'static CreateDirectoryPrompt>,
        Option<&'static RenamePrompt>,
        Option<&'static DeletePrompt>,
    ),
>;

fn submit_prompt(
    trigger: On<UiInput<ExplorerPromptSubmitRequest>>,
    prompts: Prompts,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let Ok((target, draft, create_file, create_directory, rename, delete)) = prompts.get(entity)
    else {
        return;
    };
    let name = draft.0.trim().to_string();
    if create_file.is_some() || create_directory.is_some() {
        if name.is_empty() {
            return;
        }
        commands.trigger(CreateEntry {
            entity,
            parent: target.path.clone(),
            name,
            is_dir: create_directory.is_some(),
        });
    } else if rename.is_some() {
        if name.is_empty() {
            return;
        }
        commands.trigger(RenameEntry {
            entity,
            path: target.path.clone(),
            name,
        });
    } else if delete.is_some() {
        commands.trigger(DeleteEntry {
            entity,
            path: target.path.clone(),
        });
    } else {
        return;
    }
    commands
        .entity(entity)
        .remove::<PromptComponents>()
        .insert(PromptDirty);
}

fn dismiss_prompt(trigger: On<UiInput<ExplorerPromptDismissRequest>>, mut commands: Commands) {
    commands
        .entity(trigger.event().webview)
        .remove::<PromptComponents>()
        .insert(PromptDirty);
}

fn clear_prompt(
    files: Query<Entity, (Changed<FileView>, With<PromptTarget>)>,
    mut commands: Commands,
) {
    for entity in &files {
        commands
            .entity(entity)
            .remove::<PromptComponents>()
            .insert(PromptDirty);
    }
}

#[allow(clippy::type_complexity)]
fn project(
    prompts: Query<
        (
            Entity,
            Option<&PromptTarget>,
            Option<&PromptDraft>,
            Option<&CreateFilePrompt>,
            Option<&CreateDirectoryPrompt>,
            Option<&RenamePrompt>,
            Option<&DeletePrompt>,
        ),
        (With<PromptDirty>, With<PageReady>),
    >,
    browsers: Option<NonSend<Browsers>>,
    mut commands: Commands,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (entity, target, draft, create_file, create_directory, rename, delete) in &prompts {
        if !browsers.can_emit_to(&entity) {
            continue;
        }
        let title_message_id = if create_file.is_some() {
            "editor-new-file"
        } else if create_directory.is_some() {
            "editor-new-folder"
        } else if rename.is_some() {
            "common-rename"
        } else if delete.is_some() {
            "common-delete"
        } else {
            ""
        };
        commands.trigger(FileUiStateWrite::from_event(
            entity,
            &ExplorerPromptState {
                open: !title_message_id.is_empty(),
                title_message_id: title_message_id.to_string(),
                name: target.map(|target| target.name.clone()).unwrap_or_default(),
                draft: draft.map(|draft| draft.0.clone()).unwrap_or_default(),
                destructive: delete.is_some(),
            },
        ));
        commands.entity(entity).remove::<PromptDirty>();
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

fn create(
    trigger: On<CreateEntry>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(tree_of) = views.get(entity) else {
        return;
    };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let root = tree.root.clone();
    let parent = trigger.event().parent.clone();
    let name = trigger.event().name.clone();
    let is_dir = trigger.event().is_dir;
    let task = IoTaskPool::get().spawn(async move {
        let path = ExplorerFs::new(&root)?.create(&parent, &name, is_dir)?;
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

fn rename(
    trigger: On<RenameEntry>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(tree_of) = views.get(entity) else {
        return;
    };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let root = tree.root.clone();
    let old_path = trigger.event().path.clone();
    let name = trigger.event().name.clone();
    let task = IoTaskPool::get().spawn(async move {
        let parent = old_path
            .parent()
            .ok_or_else(|| "Explorer root cannot be changed".to_string())?
            .to_path_buf();
        let next_path = old_path.with_file_name(&name);
        let knowledge_root = vmux_knowledge::KnowledgeVault::user().into_root();
        let rename_plan = (root
            .canonicalize()
            .ok()
            .zip(knowledge_root.canonicalize().ok())
            .is_some_and(|(root, knowledge_root)| root == knowledge_root))
        .then(|| {
            vmux_knowledge::KnowledgeIndex::build(&root)
                .map(|index| {
                    vmux_knowledge::KnowledgeRenamePlan::build(&index, &old_path, &next_path)
                })
                .map_err(|error| error.to_string())
        })
        .transpose()?;
        let (new_path, was_dir) = ExplorerFs::new(&root)?.rename(&old_path, &name)?;
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

fn delete(
    trigger: On<DeleteEntry>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(tree_of) = views.get(entity) else {
        return;
    };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let root = tree.root.clone();
    let path = trigger.event().path.clone();
    let task = IoTaskPool::get().spawn(async move {
        let (parent, was_dir) = ExplorerFs::new(&root)?.delete(&path)?;
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

fn drain_creates(
    mut tasks: Query<(Entity, &mut ExplorerCreateTask)>,
    views: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
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
                commands.trigger(ExplorerFeedback::new(webview, false, error));
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
        let kind = if outcome.is_dir { "folder" } else { "file" };
        if !outcome.is_dir {
            commands.trigger(FileNavigateRequest::new(webview, outcome.path.clone(), 0));
        }
        commands.trigger(ExplorerFeedback::new(
            webview,
            true,
            format!(
                "Created {kind} {}",
                outcome
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        ));
    }
}

fn drain_renames(
    mut tasks: Query<(Entity, &mut ExplorerRenameTask)>,
    mut views: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
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
                commands.trigger(ExplorerFeedback::new(webview, false, error));
                continue;
            }
        };
        for open in &mut state.open_editors {
            if let Ok(suffix) = open.strip_prefix(&outcome.old_path) {
                *open = outcome.new_path.join(suffix);
            }
        }
        let open_path = match file_view.path.strip_prefix(&outcome.old_path) {
            Ok(suffix) => Some(outcome.new_path.join(suffix)),
            Err(_) => None,
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
        if let Some(path) = open_path {
            commands.trigger(FileNavigateRequest::new(webview, path, 0));
        }
        commands.trigger(ExplorerFeedback::new(
            webview,
            true,
            format!(
                "Renamed to {}",
                outcome
                    .new_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        ));
    }
}

fn drain_deletes(
    mut tasks: Query<(Entity, &mut ExplorerDeleteTask)>,
    mut views: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
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
                commands.trigger(ExplorerFeedback::new(webview, false, error));
                continue;
            }
        };
        state
            .open_editors
            .retain(|open| !open.starts_with(&outcome.path));
        let open_path = if file_view.path.starts_with(&outcome.path) {
            Some(outcome.parent.clone())
        } else {
            None
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
        if let Some(path) = open_path {
            commands.trigger(FileNavigateRequest::new(webview, path, 0));
        }
        commands.trigger(ExplorerFeedback::new(
            webview,
            true,
            format!(
                "Deleted {}",
                outcome
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
        ));
    }
}
