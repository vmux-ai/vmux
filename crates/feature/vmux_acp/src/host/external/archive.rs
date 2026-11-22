use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use vmux_ecs::EntityTarget;
use vmux_session::provider::{
    ArchiveRead, ArchiveWrite, ArchiveWritten, ArchivesReceived, Generation, Source, Started,
};
use vmux_session::{
    ArchiveState, ProviderArchiveCompleted, ProviderArchiveWriteRequest, ProviderKind,
    ProviderSessionId,
};

use super::{SourceKind, SourcePaths, codex};

pub(super) struct ArchivePlugin;

impl bevy::prelude::Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProviderArchiveWriteRequest>()
            .add_message::<ProviderArchiveCompleted>()
            .add_observer(cancel)
            .add_systems(
                Update,
                (drain_retry_wakes, retire_contexts, finish_write, write)
                    .chain()
                    .after(super::activate)
                    .before(super::watch),
            )
            .add_systems(
                Update,
                (finish_refresh, refresh)
                    .chain()
                    .after(super::watch)
                    .before(super::scan),
            );
    }
}

#[derive(Component)]
pub(super) struct RefreshRequested;

#[derive(Component)]
struct RefreshTask(Task<Result<Option<HashMap<String, ArchiveState>>, String>>);

impl RefreshTask {
    fn spawn(root: PathBuf, wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>) -> Self {
        Self(IoTaskPool::get().spawn(async move {
            let result = codex::Reader::at(&root).archives();
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        }))
    }
}

#[derive(Component)]
struct WriteTask(Task<Result<ArchiveState, codex::ArchiveFailure>>);

#[derive(Component)]
struct Retired;

#[derive(Component)]
struct NativeDrained;

#[derive(Component, Default)]
struct WriteAttempt {
    count: u8,
    retry_at: Option<Instant>,
}

#[derive(Component)]
struct RetryWake(Task<()>);

impl RetryWake {
    fn spawn(
        delay: std::time::Duration,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        Self(IoTaskPool::get().spawn(async move {
            std::thread::sleep(delay);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        }))
    }
}

impl WriteTask {
    fn spawn(
        request: ProviderArchiveWriteRequest,
        root: PathBuf,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        Self(IoTaskPool::get().spawn(async move {
            let result = codex::Reader::at(&root).set_archive(&request.id.0, request.archived);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            result
        }))
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct StorageReads<'w, 's> {
    paths: Option<Single<'w, 's, &'static SourcePaths>>,
    sources: Query<'w, 's, (Entity, &'static SourceKind), With<Source>>,
    profile: vmux_ecs::profile::CurrentProfile<'w, 's>,
}

impl StorageReads<'_, '_> {
    fn paths(&self) -> Option<SourcePaths> {
        self.paths
            .as_ref()
            .map(|paths| (***paths).clone())
            .or_else(SourcePaths::current)
    }

    fn source(&self, requested: SourceKind) -> Option<Entity> {
        self.sources
            .iter()
            .find_map(|(entity, kind)| (*kind == requested).then_some(entity))
    }

    fn context(&self) -> vmux_session::provider::NativeContext {
        vmux_session::provider::NativeContext::capture(
            self.profile.entity(),
            self.profile.profile(),
            self.profile.paths(),
        )
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ContextReads<'w, 's> {
    sources: Query<'w, 's, (), With<Source>>,
    contexts: Query<'w, 's, Option<&'static vmux_session::provider::NativeContext>>,
    profile: vmux_ecs::profile::CurrentProfile<'w, 's>,
}

impl ContextReads<'_, '_> {
    fn matches(&self, operation: Entity, source: Entity) -> bool {
        self.sources.contains(source)
            && self
                .contexts
                .get(operation)
                .ok()
                .flatten()
                .is_none_or(|context| {
                    context.matches(
                        self.profile.entity(),
                        self.profile.profile(),
                        self.profile.paths(),
                    )
                })
    }
}

fn write(
    storage: StorageReads,
    mut requests: MessageReader<ProviderArchiveWriteRequest>,
    tasks: Query<(), With<WriteTask>>,
    queued: Query<
        (
            Entity,
            &EntityTarget<Source>,
            &ArchiveWrite,
            &ProviderKind,
            &ProviderSessionId,
            &vmux_ecs::OperationState,
            &vmux_ecs::Cwd,
            &WriteAttempt,
        ),
        (Without<WriteTask>, Without<Retired>, Without<RetryWake>),
    >,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    let paths = storage.paths();
    let mut source = storage.source(SourceKind::Codex);
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let mut running = !tasks.is_empty();
    if !running {
        let mut next = None;
        for (entity, parent, request, _, _, state, _, attempt) in &queued {
            if *state != vmux_ecs::OperationState::Pending
                || storage.sources.get(parent.entity()).is_err()
                || attempt
                    .retry_at
                    .is_some_and(|deadline| Instant::now() < deadline)
            {
                continue;
            }
            let priority = (request.revision, entity);
            if next.is_none_or(|previous| priority < previous) {
                next = Some(priority);
            }
        }
        if let Some((_, entity)) = next {
            let (_, _, request, kind, id, _, root, attempt) =
                queued.get(entity).expect("queued archive operation");
            commands.entity(entity).insert((
                vmux_ecs::OperationState::Running,
                WriteTask::spawn(
                    ProviderArchiveWriteRequest {
                        kind: kind.clone(),
                        id: id.clone(),
                        archived: request.archived,
                        revision: request.revision,
                    },
                    root.0.clone(),
                    wake.clone(),
                ),
                WriteAttempt {
                    count: attempt.count + 1,
                    retry_at: None,
                },
            ));
            running = true;
        }
    }
    for request in requests.read() {
        let Some(paths) = paths.as_ref().filter(|_| request.kind.0 == "acp:codex") else {
            completed.write(ProviderArchiveCompleted {
                kind: request.kind.clone(),
                id: request.id.clone(),
                archived: request.archived,
                revision: request.revision,
                success: false,
            });
            continue;
        };
        let source_entity = match source {
            Some(source) => source,
            None => {
                let entity = commands.spawn(SourceKind::Codex.bundle()).id();
                source = Some(entity);
                entity
            }
        };
        let operation = commands
            .spawn((
                ArchiveWrite {
                    archived: request.archived,
                    revision: request.revision,
                },
                request.kind.clone(),
                request.id.clone(),
                EntityTarget::<Source>::new(source_entity),
                vmux_ecs::Cwd(paths.codex.clone()),
                WriteAttempt::default(),
                vmux_ecs::OperationRetained,
                storage.context(),
                vmux_ecs::Operation::bundle("provider archive").expect("archive operation kind"),
            ))
            .id();
        if !running {
            commands.entity(operation).insert((
                vmux_ecs::OperationState::Running,
                WriteTask::spawn(request.clone(), paths.codex.clone(), wake.clone()),
                WriteAttempt {
                    count: 1,
                    retry_at: None,
                },
            ));
            running = true;
        }
    }
}

fn drain_retry_wakes(mut wakes: Query<(Entity, &mut RetryWake)>, mut commands: Commands) {
    for (entity, mut wake) in &mut wakes {
        if future::block_on(future::poll_once(&mut wake.0)).is_some() {
            commands.entity(entity).remove::<RetryWake>();
        }
    }
}

fn cancel(
    event: On<Remove, Source>,
    requests: Query<(
        Entity,
        &EntityTarget<Source>,
        &ArchiveWrite,
        &ProviderKind,
        &ProviderSessionId,
        &vmux_ecs::OperationState,
        Has<WriteTask>,
        Has<RetryWake>,
        Has<NativeDrained>,
    )>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for (entity, source, request, kind, id, state, running, retry, drained) in &requests {
        if source.entity() != event.entity
            || !matches!(
                state,
                vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
            )
        {
            continue;
        }
        commands.entity(entity).try_insert(Retired);
        if running || retry {
            continue;
        }
        if drained {
            commands
                .entity(entity)
                .try_insert(vmux_ecs::OperationState::Canceled)
                .try_remove::<vmux_ecs::OperationRetained>();
            continue;
        }
        completed.write(ProviderArchiveCompleted {
            kind: kind.clone(),
            id: id.clone(),
            archived: request.archived,
            revision: request.revision,
            success: false,
        });
        commands
            .entity(entity)
            .try_insert(vmux_ecs::OperationState::Canceled)
            .try_remove::<vmux_ecs::OperationRetained>();
    }
}

fn retire_contexts(
    operations: Query<(
        Entity,
        &EntityTarget<Source>,
        &ArchiveWrite,
        &ProviderKind,
        &ProviderSessionId,
        &vmux_ecs::OperationState,
        Option<&vmux_session::provider::NativeContext>,
        Has<WriteTask>,
        Has<RetryWake>,
        Has<NativeDrained>,
    )>,
    sources: Query<(), With<Source>>,
    profile: vmux_ecs::profile::CurrentProfile,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for (entity, source, request, kind, id, state, context, running, retry, drained) in &operations
    {
        if !matches!(
            state,
            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
        ) {
            continue;
        }
        if sources.contains(source.entity())
            && context.is_none_or(|context| {
                context.matches(profile.entity(), profile.profile(), profile.paths())
            })
        {
            continue;
        }
        commands.entity(entity).insert(Retired);
        if running || retry {
            continue;
        }
        if !drained {
            completed.write(ProviderArchiveCompleted {
                kind: kind.clone(),
                id: id.clone(),
                archived: request.archived,
                revision: request.revision,
                success: false,
            });
        }
        commands
            .entity(entity)
            .insert(vmux_ecs::OperationState::Canceled)
            .remove::<vmux_ecs::OperationRetained>();
    }
}

fn finish_write(
    mut tasks: Query<(
        Entity,
        &EntityTarget<Source>,
        &ArchiveWrite,
        &ProviderKind,
        &ProviderSessionId,
        &mut WriteTask,
        &WriteAttempt,
        Has<Retired>,
    )>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    current: ContextReads,
    mut observations: MessageWriter<ArchiveWritten>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for (operation, parent, request, kind, id, mut task, attempt, retired) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        let current = !retired && current.matches(operation, parent.entity());
        if current
            && let Err(error) = &result
            && error.retryable
            && attempt.count < 3
        {
            let delay = std::time::Duration::from_millis(40 << (attempt.count.saturating_sub(1)));
            commands
                .entity(operation)
                .insert((
                    vmux_ecs::OperationState::Pending,
                    WriteAttempt {
                        count: attempt.count,
                        retry_at: Some(Instant::now() + delay),
                    },
                    RetryWake::spawn(delay, proxy.as_deref().map(|proxy| (**proxy).clone())),
                ))
                .remove::<WriteTask>();
            continue;
        }
        let success = current && result.is_ok();
        if !current {
            commands
                .entity(operation)
                .insert(vmux_ecs::OperationState::Canceled);
        } else {
            match result {
                Ok(state) => {
                    observations.write(ArchiveWritten { operation, state });
                    commands.entity(parent.entity()).insert(RefreshRequested);
                }
                Err(error) => {
                    bevy::log::warn!("failed to archive Codex session: {}", error.message);
                    commands
                        .entity(operation)
                        .insert(vmux_ecs::OperationState::Failed);
                    if let Ok(error) = vmux_ecs::OperationError::new(error.message) {
                        commands.entity(operation).insert(error);
                    }
                }
            }
        }
        commands.entity(operation).insert(NativeDrained);
        completed.write(ProviderArchiveCompleted {
            kind: kind.clone(),
            id: id.clone(),
            archived: request.archived,
            revision: request.revision,
            success,
        });
        commands.entity(operation).remove::<WriteTask>();
        if !success {
            commands
                .entity(operation)
                .remove::<vmux_ecs::OperationRetained>();
        }
    }
}

fn refresh(
    storage: StorageReads,
    sources: Query<
        (
            Entity,
            &SourceKind,
            &Generation,
            Option<&EntityTarget<ArchiveRead>>,
        ),
        (With<Source>, With<RefreshRequested>),
    >,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some((source, _, generation, target)) = sources
        .iter()
        .find(|(_, kind, _, _)| **kind == SourceKind::Codex)
    else {
        return;
    };
    if target.is_some() {
        return;
    }
    if let Some(paths) = storage.paths() {
        let operation = commands
            .spawn((
                ArchiveRead {
                    generation: generation.0,
                },
                Started(Instant::now()),
                EntityTarget::<Source>::new(source),
                vmux_ecs::OperationRetained,
                storage.context(),
                vmux_ecs::Operation::bundle("provider archive read")
                    .expect("archive read operation kind")
                    .running(),
                RefreshTask::spawn(
                    paths.codex.clone(),
                    proxy.as_deref().map(|proxy| (**proxy).clone()),
                ),
            ))
            .id();
        commands
            .entity(source)
            .insert(EntityTarget::<ArchiveRead>::new(operation));
        commands.entity(source).remove::<RefreshRequested>();
    }
}

fn finish_refresh(
    mut tasks: Query<(Entity, &mut RefreshTask)>,
    mut observations: MessageWriter<ArchivesReceived>,
    mut commands: Commands,
) {
    for (operation, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        observations.write(ArchivesReceived { operation, result });
        commands.entity(operation).remove::<RefreshTask>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_sqlite_retries_through_ecs_and_completes_once_after_unlock() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state_5.sqlite");
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, archived_at INTEGER, updated_at INTEGER); INSERT INTO threads VALUES ('busy', 0, NULL, 1); BEGIN EXCLUSIVE;").unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_session::provider::ProviderPlugin,
            ArchivePlugin,
        ));
        app.world_mut().spawn(SourcePaths {
            codex: directory.path().to_path_buf(),
            claude: directory.path().join("claude"),
            vibe: directory.path().join("vibe"),
        });
        app.world_mut().write_message(ProviderArchiveWriteRequest {
            kind: ProviderKind("acp:codex".into()),
            id: ProviderSessionId("busy".into()),
            archived: true,
            revision: 51,
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(4);
        let mut retried = false;
        let mut completions = Vec::new();
        while Instant::now() < deadline {
            app.update();
            if !retried
                && app
                    .world_mut()
                    .query_filtered::<Entity, With<RetryWake>>()
                    .iter(app.world())
                    .next()
                    .is_some()
            {
                assert!(completions.is_empty());
                connection.execute_batch("COMMIT;").unwrap();
                retried = true;
            }
            completions.extend(
                app.world_mut()
                    .resource_mut::<Messages<ProviderArchiveCompleted>>()
                    .drain(),
            );
            if !completions.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            retried,
            "a held SQLite lock must produce an ECS retry before completion"
        );
        assert_eq!(completions.len(), 1);
        assert!(completions[0].success);
        assert_eq!(completions[0].revision, 51);
        assert_eq!(
            connection
                .query_row(
                    "SELECT archived FROM threads WHERE id = 'busy'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ProviderArchiveCompleted>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn source_despawn_retains_running_native_write_until_drain_and_reports_once() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_session::provider::ProviderPlugin,
            ArchivePlugin,
        ));
        let source = app.world_mut().spawn(SourceKind::Codex.bundle()).id();
        let (started_send, started) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let task = IoTaskPool::get().spawn(async move {
            started_send.send(()).unwrap();
            released.recv().unwrap();
            Ok(ArchiveState::Archived(500))
        });
        let operation = app
            .world_mut()
            .spawn((
                vmux_ecs::Operation::bundle("provider archive")
                    .unwrap()
                    .running(),
                vmux_ecs::OperationRetained,
                EntityTarget::<Source>::new(source),
                ArchiveWrite {
                    archived: true,
                    revision: 77,
                },
                ProviderKind("acp:codex".into()),
                ProviderSessionId("held".into()),
                WriteAttempt {
                    count: 1,
                    retry_at: None,
                },
                WriteTask(task),
            ))
            .id();
        let began = started.recv_timeout(std::time::Duration::from_secs(2));
        app.world_mut().entity_mut(source).despawn();
        app.update();
        let held = app.world().get::<WriteTask>(operation).is_some();
        let early = app
            .world_mut()
            .resource_mut::<Messages<ProviderArchiveCompleted>>()
            .drain()
            .count();
        release.send(()).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        let mut completions = Vec::new();
        while app.world().get::<WriteTask>(operation).is_some() && Instant::now() < deadline {
            app.update();
            completions.extend(
                app.world_mut()
                    .resource_mut::<Messages<ProviderArchiveCompleted>>()
                    .drain(),
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(began.is_ok());
        assert!(held);
        assert_eq!(early, 0);
        assert_eq!(completions.len(), 1);
        assert!(!completions[0].success);
        assert_eq!(completions[0].revision, 77);
        assert!(app.world().get::<WriteTask>(operation).is_none());
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ProviderArchiveCompleted>>()
                .drain()
                .count(),
            0
        );
    }

    #[test]
    fn source_despawn_retains_native_archive_read_until_receipt_drain() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_session::provider::ProviderPlugin,
            ArchivePlugin,
        ));
        let source = app.world_mut().spawn(SourceKind::Codex.bundle()).id();
        let (release, released) = std::sync::mpsc::channel();
        let task = IoTaskPool::get().spawn(async move {
            released.recv().unwrap();
            Ok(Some(HashMap::from([(
                "held".to_string(),
                ArchiveState::Archived(10),
            )])))
        });
        let operation = app
            .world_mut()
            .spawn((
                vmux_ecs::Operation::bundle("provider archive read")
                    .unwrap()
                    .running(),
                vmux_ecs::OperationRetained,
                EntityTarget::<Source>::new(source),
                ArchiveRead { generation: 0 },
                RefreshTask(task),
            ))
            .id();
        app.world_mut()
            .entity_mut(source)
            .insert(EntityTarget::<ArchiveRead>::new(operation));
        app.world_mut().entity_mut(source).despawn();
        app.update();
        let held = app.world().get::<RefreshTask>(operation).is_some();
        release.send(()).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while app.world().get::<RefreshTask>(operation).is_some() && Instant::now() < deadline {
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        app.update();
        assert!(held);
        assert!(app.world().get::<RefreshTask>(operation).is_none());
        assert_eq!(
            app.world().get::<vmux_ecs::OperationState>(operation),
            Some(&vmux_ecs::OperationState::Canceled)
        );
        assert!(
            app.world()
                .get::<vmux_ecs::OperationRetained>(operation)
                .is_none()
        );
    }

    #[test]
    fn removing_the_storage_owner_acknowledges_active_archive_writes() {
        for (state, expected) in [
            (vmux_ecs::OperationState::Pending, 1),
            (vmux_ecs::OperationState::Running, 1),
            (vmux_ecs::OperationState::Succeeded, 0),
        ] {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_session::provider::ProviderPlugin,
                ArchivePlugin,
            ));
            let source = app.world_mut().spawn(SourceKind::Codex.bundle()).id();
            app.world_mut().spawn((
                ArchiveWrite {
                    archived: true,
                    revision: 41,
                },
                ProviderKind("acp:codex".into()),
                ProviderSessionId("owned-write".into()),
                EntityTarget::<Source>::new(source),
                state,
            ));
            app.world_mut().entity_mut(source).remove::<Source>();
            let completed = app
                .world_mut()
                .resource_mut::<Messages<ProviderArchiveCompleted>>()
                .drain()
                .collect::<Vec<_>>();
            assert_eq!(completed.len(), expected);
            if let Some(result) = completed.first() {
                assert_eq!(result.kind.0, "acp:codex");
                assert_eq!(result.id.0, "owned-write");
                assert_eq!(result.revision, 41);
                assert!(!result.success);
            }
        }
    }
}
