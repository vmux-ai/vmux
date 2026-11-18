use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{Browsers, UiEventPlugin, UiInput};
use vmux_ecs::event::{
    ExplorerCreateDirectoryPromptRequest, ExplorerCreateFilePromptRequest,
    ExplorerDeletePromptRequest, ExplorerPromptDismissRequest, ExplorerPromptDraftRequest,
    ExplorerPromptState, ExplorerPromptSubmitRequest, ExplorerRenamePromptRequest,
};
use vmux_ecs::page::PageReady;
use vmux_ecs::{
    FileUiStateWrite, Operation, OperationCompleted, OperationPlugin, OperationRetained,
    OperationState, OperationTarget, Order,
};
use vmux_knowledge::{
    ApplyKnowledgeRename, KnowledgeIndex, KnowledgeReady, KnowledgeRenameComplete,
    KnowledgeRenamePlugin, KnowledgeRenamePrepared, MaterializeKnowledge, PrepareKnowledgeRename,
    RawSnapshot, RetireKnowledge,
};

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
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        if !app.is_plugin_added::<KnowledgeRenamePlugin>() {
            app.add_plugins(KnowledgeRenamePlugin);
        }
        app.init_resource::<RenameSequence>();
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
                    drain_roots,
                    select_roots,
                    start_snapshots,
                    drain_rename_sources,
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
            .add_observer(prepare_rename)
            .add_observer(begin_prepared)
            .add_observer(begin_rename)
            .add_observer(complete_rewrites)
            .add_observer(finish_rename)
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

#[derive(Component, Clone)]
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
struct ExplorerCreateTask(Task<std::result::Result<ExplorerCreateOutcome, String>>);

#[derive(Component)]
struct ExplorerRenameTask(Task<std::result::Result<ExplorerRenameOutcome, String>>);

#[derive(Component)]
struct ExplorerDeleteTask(Task<std::result::Result<ExplorerDeleteOutcome, String>>);

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
    commands.spawn((
        Operation::bundle("file.create").unwrap().running(),
        OperationTarget::new(entity),
        ChildOf(entity),
        ExplorerCreateTask(task),
    ));
}

#[derive(Component)]
struct RenamePending {
    root: PathBuf,
    old: PathBuf,
    parent: PathBuf,
    name: String,
}

#[derive(Component)]
struct KnowledgeSourceTask(Task<std::result::Result<RawSnapshot, String>>);

#[derive(EntityEvent)]
struct BeginRename {
    entity: Entity,
}

#[derive(EntityEvent)]
struct RenameFinished {
    entity: Entity,
    moved: Option<ExplorerRenameOutcome>,
    result: std::result::Result<(), String>,
}

#[derive(Resource, Default)]
struct RenameSequence(u32);

#[derive(Component)]
struct RenameRootTask(Task<RenameRootFacts>);

#[derive(Component)]
struct RenameRootFacts {
    project: Option<PathBuf>,
    vault: Option<PathBuf>,
}

#[derive(Component)]
struct VaultRenameRoot(PathBuf);

#[derive(Component)]
struct VaultRenameActive;

#[derive(Component)]
struct CapturePending;

fn rename(
    trigger: On<RenameEntry>,
    views: Query<&UsesExplorerTree>,
    trees: Query<&ExplorerTree>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut sequence: ResMut<RenameSequence>,
    mut commands: Commands,
) {
    let view = trigger.event_target();
    let Ok(tree_of) = views.get(view) else { return };
    let Ok(tree) = trees.get(tree_of.0) else {
        return;
    };
    let root = tree.root.clone();
    let old = trigger.event().path.clone();
    sequence.0 = sequence.0.wrapping_add(1);
    let entity = commands
        .spawn((
            Operation::bundle("file.rename").unwrap().running(),
            OperationTarget::new(view),
            OperationRetained,
            Order(sequence.0),
        ))
        .id();
    let Some(parent) = old.parent().map(Path::to_path_buf) else {
        commands.trigger(RenameFinished {
            entity,
            moved: None,
            result: Err("Explorer root cannot be changed".to_string()),
        });
        return;
    };
    let vault = vmux_knowledge::KnowledgeVault::user().into_root();
    commands.entity(entity).insert(RenamePending {
        root: root.clone(),
        old,
        parent,
        name: trigger.event().name.clone(),
    });
    let wake = wake.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let facts = RenameRootFacts {
            project: root.canonicalize().ok(),
            vault: vault.canonicalize().ok(),
        };
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        facts
    });
    commands.entity(entity).insert(RenameRootTask(task));
}

fn drain_roots(
    mut tasks: Query<(Entity, &mut RenameRootTask, &OperationState)>,
    mut commands: Commands,
) {
    for (entity, mut task, state) in &mut tasks {
        let Some(facts) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<RenameRootTask>();
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            commands
                .entity(entity)
                .remove::<(RenamePending, OperationRetained)>();
            continue;
        }
        commands.entity(entity).insert(facts);
    }
}

fn select_roots(facts: Query<(Entity, &RenameRootFacts, &OperationState)>, mut commands: Commands) {
    for (entity, facts, state) in &facts {
        commands.entity(entity).remove::<RenameRootFacts>();
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            commands
                .entity(entity)
                .remove::<(RenamePending, OperationRetained)>();
            continue;
        }
        if let Some(root) = facts.project.as_ref()
            && facts.vault.as_ref() == Some(root)
        {
            commands
                .entity(entity)
                .insert((VaultRenameRoot(root.clone()), CapturePending));
        } else {
            commands.trigger(BeginRename { entity });
        }
    }
}

type WaitingCaptures<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static VaultRenameRoot,
        &'static Order,
        &'static OperationState,
    ),
    With<CapturePending>,
>;

fn start_snapshots(
    pending: WaitingCaptures,
    active: Query<&VaultRenameRoot, With<VaultRenameActive>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let mut held = std::collections::HashSet::new();
    for root in &active {
        held.insert(root.0.clone());
    }
    let mut waiting = pending.iter().collect::<Vec<_>>();
    waiting.sort_by_key(|(_, _, order, _)| order.0);
    for (entity, root, _, state) in waiting {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(entity).remove::<(
                RenamePending,
                CapturePending,
                VaultRenameRoot,
                OperationRetained,
            )>();
            continue;
        }
        if !held.insert(root.0.clone()) {
            continue;
        }
        let root = root.0.clone();
        let wake = wake.as_deref().map(|proxy| (**proxy).clone());
        let task = IoTaskPool::get().spawn(async move {
            let result = RawSnapshot::capture(&root).map_err(|error| error.to_string());
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        });
        commands
            .entity(entity)
            .remove::<CapturePending>()
            .insert((VaultRenameActive, KnowledgeSourceTask(task)));
    }
}

fn drain_rename_sources(
    mut sources: Query<(Entity, &mut KnowledgeSourceTask, &OperationState)>,
    mut commands: Commands,
) {
    for (entity, mut source, state) in &mut sources {
        let Some(result) = future::block_on(future::poll_once(&mut source.0)) else {
            continue;
        };
        commands.entity(entity).remove::<KnowledgeSourceTask>();
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(entity).remove::<(
                RenamePending,
                VaultRenameRoot,
                VaultRenameActive,
                OperationRetained,
            )>();
            continue;
        }
        match result {
            Ok(snapshot) => {
                commands.entity(entity).insert(KnowledgeIndex::default());
                commands.trigger(MaterializeKnowledge { entity, snapshot });
            }
            Err(error) => {
                commands.trigger(RenameFinished {
                    entity,
                    moved: None,
                    result: Err(error),
                });
            }
        }
    }
}

type ReadyRenames<'w, 's> = Query<
    'w,
    's,
    (&'static RenamePending, &'static OperationState),
    (Without<ExplorerRenameTask>, Without<ExplorerRenameOutcome>),
>;

fn prepare_rename(trigger: On<KnowledgeReady>, pending: ReadyRenames, mut commands: Commands) {
    let Ok((pending, state)) = pending.get(trigger.entity) else {
        return;
    };
    if !matches!(state, OperationState::Pending | OperationState::Running) {
        commands.trigger(RetireKnowledge {
            entity: trigger.entity,
        });
        commands.entity(trigger.entity).remove::<(
            RenamePending,
            VaultRenameRoot,
            VaultRenameActive,
            OperationRetained,
        )>();
        return;
    }
    commands.trigger(PrepareKnowledgeRename {
        entity: trigger.entity,
        old: pending.old.clone(),
        new: pending.old.with_file_name(&pending.name),
    });
}

fn begin_prepared(trigger: On<KnowledgeRenamePrepared>, mut commands: Commands) {
    commands.trigger(BeginRename {
        entity: trigger.entity,
    });
}

fn begin_rename(
    trigger: On<BeginRename>,
    pending: ReadyRenames,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok((pending, state)) = pending.get(trigger.entity) else {
        return;
    };
    if !matches!(state, OperationState::Pending | OperationState::Running) {
        commands.trigger(RetireKnowledge {
            entity: trigger.entity,
        });
        commands.entity(trigger.entity).remove::<(
            RenamePending,
            VaultRenameRoot,
            VaultRenameActive,
            OperationRetained,
        )>();
        return;
    }
    let root = pending.root.clone();
    let old_path = pending.old.clone();
    let parent = pending.parent.clone();
    let name = pending.name.clone();
    let wake = wake.as_deref().map(|proxy| (**proxy).clone());
    let task = IoTaskPool::get().spawn(async move {
        let result =
            ExplorerFs::new(&root).and_then(|filesystem| filesystem.rename(&old_path, &name));
        if let Some(wake) = wake {
            let _ = wake.send_event(WinitUserEvent::WakeUp);
        }
        let (new_path, was_dir) = result?;
        Ok(ExplorerRenameOutcome {
            old_path,
            new_path,
            parent,
            was_dir,
        })
    });
    commands
        .entity(trigger.entity)
        .insert(ExplorerRenameTask(task));
}

fn complete_rewrites(
    trigger: On<KnowledgeRenameComplete>,
    outcomes: Query<&ExplorerRenameOutcome>,
    mut commands: Commands,
) {
    let Ok(outcome) = outcomes.get(trigger.entity) else {
        return;
    };
    commands.trigger(RenameFinished {
        entity: trigger.entity,
        moved: Some(outcome.clone()),
        result: trigger.result.clone(),
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
    commands.spawn((
        Operation::bundle("file.delete").unwrap().running(),
        OperationTarget::new(entity),
        ChildOf(entity),
        ExplorerDeleteTask(task),
    ));
}

fn drain_creates(
    mut tasks: Query<(
        Entity,
        &mut ExplorerCreateTask,
        &OperationTarget,
        &OperationState,
    )>,
    views: Query<&UsesExplorerTree>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    for (task_entity, mut pending, target, state) in &mut tasks {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(task_entity).remove::<ExplorerCreateTask>();
            continue;
        }
        let Some(result) = future::block_on(future::poll_once(&mut pending.0)) else {
            continue;
        };
        let webview = target.entity();
        commands.trigger(OperationCompleted::from_result(task_entity, &result));
        commands.entity(task_entity).remove::<ExplorerCreateTask>();
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
    mut tasks: Query<(
        Entity,
        &mut ExplorerRenameTask,
        &OperationState,
        Option<&KnowledgeIndex>,
    )>,
    mut commands: Commands,
) {
    for (entity, mut task, state, knowledge) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ExplorerRenameTask>();
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            let moved = result.ok();
            commands.trigger(RenameFinished {
                entity,
                moved,
                result: Err("Rename canceled".to_string()),
            });
            continue;
        }
        match result {
            Ok(outcome) if knowledge.is_some() => {
                let old = outcome.old_path.clone();
                let new = outcome.new_path.clone();
                commands.entity(entity).insert(outcome);
                commands.trigger(ApplyKnowledgeRename { entity, old, new });
            }
            Ok(outcome) => {
                commands.trigger(RenameFinished {
                    entity,
                    moved: Some(outcome),
                    result: Ok(()),
                });
            }
            Err(error) => {
                commands.trigger(RenameFinished {
                    entity,
                    moved: None,
                    result: Err(error),
                });
            }
        }
    }
}

fn finish_rename(
    trigger: On<RenameFinished>,
    operations: Query<Option<&OperationTarget>, With<Operation>>,
    mut views: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    commands.trigger(RetireKnowledge { entity });
    commands.entity(entity).remove::<(
        RenamePending,
        ExplorerRenameOutcome,
        VaultRenameRoot,
        VaultRenameActive,
        OperationRetained,
    )>();
    let Ok(target) = operations.get(entity) else {
        return;
    };
    commands.trigger(OperationCompleted::from_result(entity, &trigger.result));
    let Some(target) = target else { return };
    let webview = target.entity();
    let Ok((file_view, mut state, tree_of)) = views.get_mut(webview) else {
        return;
    };
    let Some(outcome) = trigger.moved.clone() else {
        if let Err(error) = &trigger.result {
            commands.trigger(ExplorerFeedback::new(webview, false, error.clone()));
        }
        return;
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
    let message = match &trigger.result {
        Ok(()) => format!(
            "Renamed to {}",
            outcome
                .new_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ),
        Err(error) => error.clone(),
    };
    commands.trigger(ExplorerFeedback::new(
        webview,
        trigger.result.is_ok(),
        message,
    ));
}

fn drain_deletes(
    mut tasks: Query<(
        Entity,
        &mut ExplorerDeleteTask,
        &OperationTarget,
        &OperationState,
    )>,
    mut views: Query<(&FileView, &mut ExplorerState, &UsesExplorerTree)>,
    mut trees: Query<&mut ExplorerTree>,
    mut commands: Commands,
) {
    for (task_entity, mut pending, target, state) in &mut tasks {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            commands.entity(task_entity).remove::<ExplorerDeleteTask>();
            continue;
        }
        let Some(result) = future::block_on(future::poll_once(&mut pending.0)) else {
            continue;
        };
        let webview = target.entity();
        commands.trigger(OperationCompleted::from_result(task_entity, &result));
        commands.entity(task_entity).remove::<ExplorerDeleteTask>();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};

    struct RenameFixture {
        directory: tempfile::TempDir,
        app: App,
        view: Entity,
        operation: Entity,
        release: Option<Arc<AtomicBool>>,
    }

    impl RenameFixture {
        fn new(destination_exists: bool) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let old = root.join("alpha.md");
            std::fs::write(&old, "# Alpha\n").unwrap();
            std::fs::write(root.join("source.md"), "See [[Alpha|the note]].\n").unwrap();
            if destination_exists {
                std::fs::write(root.join("renamed.md"), "# Existing\n").unwrap();
            }
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_non_send(Browsers::default())
                .add_plugins(ExplorerEntryPlugin);
            let view = app.world_mut().spawn_empty().id();
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("file.rename").unwrap().running(),
                    OperationTarget::new(view),
                    OperationRetained,
                    RenamePending {
                        root: root.clone(),
                        old,
                        parent: root.clone(),
                        name: "renamed.md".to_string(),
                    },
                ))
                .id();
            app.world_mut().flush();
            let release = Arc::new(AtomicBool::new(false));
            let resume = release.clone();
            let (started, ready) = mpsc::channel();
            let task = IoTaskPool::get().spawn(async move {
                started.send(()).unwrap();
                while !resume.load(Ordering::Acquire) {
                    future::yield_now().await;
                }
                RawSnapshot::capture(&root).map_err(|error| error.to_string())
            });
            ready.recv_timeout(Duration::from_secs(2)).unwrap();
            app.world_mut()
                .entity_mut(operation)
                .insert(KnowledgeSourceTask(task));
            Self {
                directory,
                app,
                view,
                operation,
                release: Some(release),
            }
        }

        fn release(&mut self) {
            if let Some(release) = self.release.take() {
                release.store(true, Ordering::Release);
            }
        }

        fn finish(&mut self) -> OperationState {
            self.release();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                self.app.update();
                let state = self
                    .app
                    .world()
                    .get::<OperationState>(self.operation)
                    .unwrap()
                    .clone();
                if !matches!(state, OperationState::Pending | OperationState::Running) {
                    return state;
                }
                assert!(
                    Instant::now() < deadline,
                    "accepted rename must drain and finish"
                );
                std::thread::yield_now();
            }
        }
    }

    impl Drop for RenameFixture {
        fn drop(&mut self) {
            self.release();
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline
                && (self
                    .app
                    .world()
                    .get::<KnowledgeSourceTask>(self.operation)
                    .is_some()
                    || self
                        .app
                        .world()
                        .get::<ExplorerRenameTask>(self.operation)
                        .is_some()
                    || self
                        .app
                        .world()
                        .get::<KnowledgeIndex>(self.operation)
                        .is_some())
            {
                self.app.update();
                std::thread::yield_now();
            }
        }
    }

    #[test]
    fn accepted_knowledge_rename_keeps_its_io_after_the_view_is_removed() {
        let mut fixture = RenameFixture::new(false);
        fixture.app.world_mut().despawn(fixture.view);
        fixture.app.update();
        assert!(
            fixture
                .app
                .world()
                .get::<KnowledgeSourceTask>(fixture.operation)
                .is_some()
        );
        assert_eq!(fixture.finish(), OperationState::Succeeded);
        assert!(!fixture.directory.path().join("alpha.md").exists());
        assert!(fixture.directory.path().join("renamed.md").exists());
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("source.md")).unwrap(),
            "See [[renamed|the note]].\n"
        );
        assert!(
            fixture
                .app
                .world()
                .get::<KnowledgeIndex>(fixture.operation)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ExplorerRenameTask>(fixture.operation)
                .is_none()
        );
    }

    #[test]
    fn failed_filesystem_rename_does_not_start_planned_knowledge_rewrites() {
        let mut fixture = RenameFixture::new(true);
        assert_eq!(fixture.finish(), OperationState::Failed);
        assert!(fixture.directory.path().join("alpha.md").exists());
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("renamed.md")).unwrap(),
            "# Existing\n"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.directory.path().join("source.md")).unwrap(),
            "See [[Alpha|the note]].\n"
        );
        assert!(
            fixture
                .app
                .world()
                .get::<KnowledgeIndex>(fixture.operation)
                .is_none()
        );
    }
    struct QueuedRenames {
        directory: tempfile::TempDir,
        app: App,
    }

    impl QueuedRenames {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_non_send(Browsers::default())
                .add_plugins(ExplorerEntryPlugin);
            app.world_mut().flush();
            Self { directory, app }
        }

        fn vault(&self, name: &str) -> PathBuf {
            let root = self.directory.path().join(name);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("a.md"), "# A\n").unwrap();
            std::fs::write(root.join("b.md"), "# B\n").unwrap();
            std::fs::write(root.join("source.md"), "See [[a]] [[b]].\n").unwrap();
            root.canonicalize().unwrap()
        }

        fn queue(&mut self, root: &Path, old: &str, new: &str, order: u32) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("file.rename").unwrap().running(),
                    OperationRetained,
                    Order(order),
                    RenamePending {
                        root: root.to_path_buf(),
                        old: root.join(old),
                        parent: root.to_path_buf(),
                        name: new.to_string(),
                    },
                    VaultRenameRoot(root.to_path_buf()),
                    CapturePending,
                ))
                .id()
        }

        fn finish(&mut self, operation: Entity) {
            let deadline = Instant::now() + Duration::from_secs(3);
            while matches!(
                self.app.world().get::<OperationState>(operation),
                Some(OperationState::Pending | OperationState::Running)
            ) {
                assert!(Instant::now() < deadline, "queued rename must finish");
                self.app.update();
                std::thread::yield_now();
            }
            assert_eq!(
                self.app.world().get::<OperationState>(operation),
                Some(&OperationState::Succeeded)
            );
        }
    }

    #[test]
    fn same_vault_renames_capture_the_previous_operations_completed_rewrites() {
        let mut fixture = QueuedRenames::new();
        let root = fixture.vault("vault");
        let first = fixture.queue(&root, "a.md", "renamed-a.md", 1);
        let second = fixture.queue(&root, "b.md", "renamed-b.md", 2);
        fixture.finish(second);
        fixture.finish(first);
        assert_eq!(
            std::fs::read_to_string(root.join("source.md")).unwrap(),
            "See [[renamed-a]] [[renamed-b]].\n"
        );
        assert!(
            fixture
                .app
                .world()
                .get::<OperationRetained>(first)
                .is_none()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<OperationRetained>(second)
                .is_none()
        );
    }

    #[test]
    fn different_vaults_progress_while_another_capture_is_pending() {
        let mut fixture = QueuedRenames::new();
        let first_root = fixture.vault("first");
        let second_root = fixture.vault("second");
        let first = fixture.queue(&first_root, "a.md", "renamed-a.md", 1);
        fixture
            .app
            .world_mut()
            .entity_mut(first)
            .remove::<CapturePending>()
            .insert(VaultRenameActive);
        let release = Arc::new(AtomicBool::new(false));
        let resume = release.clone();
        let task = IoTaskPool::get().spawn(async move {
            while !resume.load(Ordering::Acquire) {
                future::yield_now().await;
            }
            RawSnapshot::capture(&first_root).map_err(|error| error.to_string())
        });
        fixture
            .app
            .world_mut()
            .entity_mut(first)
            .insert(KnowledgeSourceTask(task));
        let second = fixture.queue(&second_root, "b.md", "renamed-b.md", 2);
        fixture.finish(second);
        assert!(
            fixture
                .app
                .world()
                .get::<KnowledgeSourceTask>(first)
                .is_some()
        );
        release.store(true, Ordering::Release);
        fixture.finish(first);
    }

    #[test]
    fn native_move_paths_reconcile_even_when_link_rewriting_reports_failure() {
        let mut fixture = QueuedRenames::new();
        let root = fixture.vault("vault");
        let view = fixture
            .app
            .world_mut()
            .spawn((
                FileView {
                    path: PathBuf::new(),
                },
                ExplorerState::default(),
            ))
            .id();
        let tree = fixture
            .app
            .world_mut()
            .spawn(ExplorerTree::new(root.clone()))
            .id();
        let old = root.join("a.md");
        let new = root.join("renamed-a.md");
        fixture
            .app
            .world_mut()
            .entity_mut(view)
            .insert(UsesExplorerTree(tree));
        fixture
            .app
            .world_mut()
            .get_mut::<ExplorerState>(view)
            .unwrap()
            .open_editors
            .push(old.clone());
        let operation = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("file.rename").unwrap().running(),
                OperationTarget::new(view),
                OperationRetained,
            ))
            .id();
        fixture.app.world_mut().trigger(RenameFinished {
            entity: operation,
            moved: Some(ExplorerRenameOutcome {
                old_path: old,
                new_path: new.clone(),
                parent: root,
                was_dir: false,
            }),
            result: Err("Source changed".to_string()),
        });
        fixture.app.world_mut().flush();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ExplorerState>(view)
                .unwrap()
                .open_editors,
            [new]
        );
        assert_eq!(
            fixture.app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert!(fixture.app.world().get::<ExplorerTreeDirty>(view).is_some());
    }
}
