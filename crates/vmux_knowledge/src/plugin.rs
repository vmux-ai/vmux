use std::path::PathBuf;
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::{CefPageAttachRequest, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

use crate::event::{
    NOTE_CREATED_EVENT, NOTE_ERROR_EVENT, NOTE_READ_RESPONSE_EVENT, NOTE_WRITTEN_EVENT,
    NOTES_PAGE_URL, NOTES_QUERY_RESPONSE_EVENT, NoteCreateRequest, NoteCreatedEvent,
    NoteErrorEvent, NoteOpenRequest, NoteOperation, NoteReadRequest, NoteReadResponse, NoteSummary,
    NoteWriteRequest, NoteWrittenEvent, NotesQueryRequest, NotesQueryResponse,
};
use crate::store::{
    NoteIndexEntry, build_index, create_note, query_index, read_note, read_response,
    resolve_note_path, vault_dir, write_note,
};

pub struct KnowledgePlugin;

#[derive(Resource)]
struct KnowledgeIndex {
    entries: Vec<NoteIndexEntry>,
    dirty: bool,
    generation: u64,
}

impl Default for KnowledgeIndex {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            dirty: true,
            generation: 0,
        }
    }
}

struct KnowledgeWatch {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
}

struct QueryTaskOutput {
    entries: Vec<NoteIndexEntry>,
    notes: Vec<NoteSummary>,
    total: u32,
    has_more: bool,
}

#[derive(Component)]
struct NotesQueryTask {
    webview: Entity,
    request_id: u64,
    offset: u32,
    limit: u32,
    query: String,
    vault_path: String,
    generation: u64,
    task: Task<Result<QueryTaskOutput, String>>,
}

#[derive(Component)]
struct NoteReadTask {
    webview: Entity,
    request_id: u64,
    path: String,
    task: Task<Result<NoteReadResponse, String>>,
}

#[derive(Component)]
struct NoteCreateTask {
    webview: Entity,
    request_id: u64,
    task: Task<Result<crate::store::NoteDocument, String>>,
}

#[derive(Component)]
struct NoteWriteTask {
    webview: Entity,
    request_id: u64,
    path: String,
    task: Task<Result<NoteReadResponse, String>>,
}

impl Plugin for KnowledgePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        let vault = vault_dir();
        if crate::store::ensure_vault(&vault).is_ok() {
            let (tx, rx) = mpsc::channel();
            match notify::recommended_watcher(move |result| {
                let _ = tx.send(result);
            }) {
                Ok(mut watcher) => {
                    if watcher.watch(&vault, RecursiveMode::Recursive).is_ok() {
                        app.insert_non_send(KnowledgeWatch {
                            _watcher: watcher,
                            rx,
                        });
                    }
                }
                Err(error) => bevy::log::warn!("knowledge watcher init failed: {error}"),
            }
        }
        app.init_resource::<KnowledgeIndex>()
            .add_systems(
                Update,
                handle_notes_page_open.in_set(PageOpenSet::HandleKnownPages),
            )
            .add_systems(
                Update,
                (
                    drain_knowledge_watch,
                    drain_notes_query_tasks,
                    drain_note_read_tasks,
                    drain_note_create_tasks,
                    drain_note_write_tasks,
                ),
            )
            .add_plugins(BinEventEmitterPlugin::<(
                NotesQueryRequest,
                NoteReadRequest,
                NoteCreateRequest,
                NoteOpenRequest,
                NoteWriteRequest,
            )>::for_hosts(&["notes"]))
            .add_observer(on_notes_query)
            .add_observer(on_note_read)
            .add_observer(on_note_create)
            .add_observer(on_note_write)
            .add_observer(on_note_open)
            .add_message::<CefPageAttachRequest>()
            .add_message::<vmux_layout::OpenInNewStackRequest>();
    }
}

fn on_note_write(
    trigger: On<BinReceive<NoteWriteRequest>>,
    pending: Query<(Entity, &NoteWriteTask)>,
    mut commands: Commands,
) {
    for (entity, task) in &pending {
        if task.webview == trigger.event().webview {
            commands.entity(entity).despawn();
        }
    }
    let request = trigger.event().payload.clone();
    let path = request.path.clone();
    let task = IoTaskPool::get().spawn(async move {
        let vault = vault_dir();
        let document = write_note(&vault, PathBuf::from(&path).as_path(), &request.source)?;
        Ok(read_response(&document, &vault, request.request_id))
    });
    commands.spawn(NoteWriteTask {
        webview: trigger.event().webview,
        request_id: trigger.event().payload.request_id,
        path: trigger.event().payload.path.clone(),
        task,
    });
}

fn drain_note_write_tasks(
    mut tasks: Query<(Entity, &mut NoteWriteTask)>,
    mut index: ResMut<KnowledgeIndex>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(note) => {
                index.dirty = true;
                index.generation = index.generation.wrapping_add(1);
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    task.webview,
                    NOTE_WRITTEN_EVENT,
                    &NoteWrittenEvent {
                        request_id: task.request_id,
                        note,
                    },
                ));
            }
            Err(error) => emit_error(
                task.webview,
                NoteOperation::Write,
                task.request_id,
                task.path.clone(),
                error,
                &mut commands,
            ),
        }
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_notes_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    mut attach_writer: MessageWriter<CefPageAttachRequest>,
    mut commands: Commands,
) {
    for (entity, task) in &tasks {
        if task.url.trim_end_matches('/') != NOTES_PAGE_URL.trim_end_matches('/') {
            continue;
        }
        attach_writer.write(CefPageAttachRequest {
            stack: task.stack,
            url: NOTES_PAGE_URL.to_string(),
            title: "Knowledge".to_string(),
            bg_color: None,
        });
        commands.entity(entity).insert(PageOpenHandled);
    }
}

fn drain_knowledge_watch(
    watch: Option<NonSendMut<KnowledgeWatch>>,
    mut index: ResMut<KnowledgeIndex>,
) {
    let Some(watch) = watch else {
        return;
    };
    let changed = watch.rx.try_iter().any(|result| result.is_ok());
    if changed {
        index.dirty = true;
        index.generation = index.generation.wrapping_add(1);
    }
}

fn on_notes_query(
    trigger: On<BinReceive<NotesQueryRequest>>,
    pending: Query<(Entity, &NotesQueryTask)>,
    index: Res<KnowledgeIndex>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    for (entity, task) in &pending {
        if task.webview == trigger.event().webview {
            commands.entity(entity).despawn();
        }
    }
    let limit = request.limit.clamp(1, 200);
    if !index.dirty {
        let (notes, total, has_more) =
            query_index(&index.entries, &request.query, request.offset, limit);
        emit_query_response(
            trigger.event().webview,
            request.request_id,
            request.offset,
            vault_dir().to_string_lossy().into_owned(),
            notes,
            total,
            has_more,
            &mut commands,
        );
        return;
    }
    spawn_query_task(
        trigger.event().webview,
        request.request_id,
        request.offset,
        limit,
        request.query.clone(),
        index.entries.clone(),
        index.generation,
        &mut commands,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_query_task(
    webview: Entity,
    request_id: u64,
    offset: u32,
    limit: u32,
    query: String,
    previous: Vec<NoteIndexEntry>,
    generation: u64,
    commands: &mut Commands,
) {
    let vault = vault_dir();
    let task_vault = vault.clone();
    let task_query = query.clone();
    let task = IoTaskPool::get().spawn(async move {
        let entries = build_index(&task_vault, &previous).map_err(|error| error.to_string())?;
        let (notes, total, has_more) = query_index(&entries, &task_query, offset, limit);
        Ok(QueryTaskOutput {
            entries,
            notes,
            total,
            has_more,
        })
    });
    commands.spawn(NotesQueryTask {
        webview,
        request_id,
        offset,
        limit,
        query,
        vault_path: vault.to_string_lossy().into_owned(),
        generation,
        task,
    });
}

fn drain_notes_query_tasks(
    mut tasks: Query<(Entity, &mut NotesQueryTask)>,
    mut index: ResMut<KnowledgeIndex>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(output) => {
                index.entries = output.entries;
                if index.generation != task.generation {
                    spawn_query_task(
                        task.webview,
                        task.request_id,
                        task.offset,
                        task.limit,
                        task.query.clone(),
                        index.entries.clone(),
                        index.generation,
                        &mut commands,
                    );
                    continue;
                }
                index.dirty = false;
                emit_query_response(
                    task.webview,
                    task.request_id,
                    task.offset,
                    task.vault_path.clone(),
                    output.notes,
                    output.total,
                    output.has_more,
                    &mut commands,
                );
            }
            Err(error) => emit_error(
                task.webview,
                NoteOperation::Query,
                task.request_id,
                String::new(),
                error,
                &mut commands,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_query_response(
    webview: Entity,
    request_id: u64,
    offset: u32,
    vault_path: String,
    notes: Vec<NoteSummary>,
    total: u32,
    has_more: bool,
    commands: &mut Commands,
) {
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        NOTES_QUERY_RESPONSE_EVENT,
        &NotesQueryResponse {
            request_id,
            offset,
            vault_path,
            notes,
            total,
            has_more,
        },
    ));
}

fn on_note_read(
    trigger: On<BinReceive<NoteReadRequest>>,
    pending: Query<(Entity, &NoteReadTask)>,
    mut commands: Commands,
) {
    for (entity, task) in &pending {
        if task.webview == trigger.event().webview {
            commands.entity(entity).despawn();
        }
    }
    let request = trigger.event().payload.clone();
    let task_path = request.path.clone();
    let task = IoTaskPool::get().spawn(async move {
        let vault = vault_dir();
        let document = read_note(&vault, PathBuf::from(task_path).as_path())?;
        Ok(read_response(&document, &vault, request.request_id))
    });
    commands.spawn(NoteReadTask {
        webview: trigger.event().webview,
        request_id: trigger.event().payload.request_id,
        path: trigger.event().payload.path.clone(),
        task,
    });
}

fn drain_note_read_tasks(mut tasks: Query<(Entity, &mut NoteReadTask)>, mut commands: Commands) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(response) => commands.trigger(BinHostEmitEvent::from_rkyv(
                task.webview,
                NOTE_READ_RESPONSE_EVENT,
                &response,
            )),
            Err(error) => emit_error(
                task.webview,
                NoteOperation::Read,
                task.request_id,
                task.path.clone(),
                error,
                &mut commands,
            ),
        }
    }
}

fn on_note_create(trigger: On<BinReceive<NoteCreateRequest>>, mut commands: Commands) {
    let request = trigger.event().payload.clone();
    let task = IoTaskPool::get().spawn(async move {
        create_note(&vault_dir(), &request.title).map_err(|error| error.to_string())
    });
    commands.spawn(NoteCreateTask {
        webview: trigger.event().webview,
        request_id: trigger.event().payload.request_id,
        task,
    });
}

fn drain_note_create_tasks(
    mut tasks: Query<(Entity, &mut NoteCreateTask)>,
    mut index: ResMut<KnowledgeIndex>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        commands.entity(entity).despawn();
        match result {
            Ok(document) => {
                index.dirty = true;
                index.generation = index.generation.wrapping_add(1);
                commands.trigger(BinHostEmitEvent::from_rkyv(
                    task.webview,
                    NOTE_CREATED_EVENT,
                    &NoteCreatedEvent {
                        request_id: task.request_id,
                        note: document.summary,
                    },
                ));
            }
            Err(error) => emit_error(
                task.webview,
                NoteOperation::Create,
                task.request_id,
                String::new(),
                error,
                &mut commands,
            ),
        }
    }
}

fn on_note_open(
    trigger: On<BinReceive<NoteOpenRequest>>,
    mut open_writer: MessageWriter<vmux_layout::OpenInNewStackRequest>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    match resolve_note_path(&vault_dir(), PathBuf::from(&request.path).as_path()) {
        Ok(path) => match url::Url::from_file_path(path) {
            Ok(url) => {
                open_writer.write(vmux_layout::OpenInNewStackRequest {
                    url: url.to_string(),
                });
            }
            Err(()) => emit_error(
                trigger.event().webview,
                NoteOperation::Open,
                request.request_id,
                request.path.clone(),
                "failed to build note URL".to_string(),
                &mut commands,
            ),
        },
        Err(error) => emit_error(
            trigger.event().webview,
            NoteOperation::Open,
            request.request_id,
            request.path.clone(),
            error,
            &mut commands,
        ),
    }
}

fn emit_error(
    webview: Entity,
    operation: NoteOperation,
    request_id: u64,
    path: String,
    message: String,
    commands: &mut Commands,
) {
    commands.trigger(BinHostEmitEvent::from_rkyv(
        webview,
        NOTE_ERROR_EVENT,
        &NoteErrorEvent {
            operation,
            request_id,
            path,
            message,
        },
    ));
}
