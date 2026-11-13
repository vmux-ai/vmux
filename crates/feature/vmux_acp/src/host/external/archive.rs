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

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ProviderArchiveWriteRequest>()
            .add_message::<ProviderArchiveCompleted>()
            .add_observer(cancel)
            .add_systems(
                Update,
                (finish_write, write)
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
struct WriteTask(Task<Result<ArchiveState, String>>);

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

fn write(
    paths: Option<Single<&SourcePaths>>,
    sources: Query<(Entity, &SourceKind), With<Source>>,
    mut requests: MessageReader<ProviderArchiveWriteRequest>,
    tasks: Query<(), With<WriteTask>>,
    queued: Query<
        (
            Entity,
            &ChildOf,
            &ArchiveWrite,
            &ProviderKind,
            &ProviderSessionId,
            &vmux_ecs::OperationState,
        ),
        Without<WriteTask>,
    >,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    let paths = paths
        .map(|paths| (**paths).clone())
        .or_else(SourcePaths::current);
    let mut source = sources
        .iter()
        .find(|(_, kind)| **kind == SourceKind::Codex)
        .map(|(entity, _)| entity);
    let wake = proxy.as_deref().map(|proxy| (**proxy).clone());
    let mut running = !tasks.is_empty();
    if !running && let Some(paths) = &paths {
        let mut next = None;
        for (entity, parent, request, _, _, state) in &queued {
            if *state != vmux_ecs::OperationState::Pending || sources.get(parent.parent()).is_err()
            {
                continue;
            }
            let priority = (request.revision, entity);
            if next.is_none_or(|previous| priority < previous) {
                next = Some(priority);
            }
        }
        if let Some((_, entity)) = next {
            let (_, _, request, kind, id, _) =
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
                    paths.codex.clone(),
                    wake.clone(),
                ),
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
                ChildOf(source_entity),
                vmux_ecs::Operation::bundle("provider archive").expect("archive operation kind"),
            ))
            .id();
        if !running {
            commands.entity(operation).insert((
                vmux_ecs::OperationState::Running,
                WriteTask::spawn(request.clone(), paths.codex.clone(), wake.clone()),
            ));
            running = true;
        }
    }
}

fn cancel(
    event: On<Remove, Source>,
    children: Query<&Children>,
    requests: Query<(
        Entity,
        &ArchiveWrite,
        &ProviderKind,
        &ProviderSessionId,
        &vmux_ecs::OperationState,
    )>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    let Ok(children) = children.get(event.entity) else {
        return;
    };
    let mut changed = false;
    for child in children {
        let Ok((entity, request, kind, id, state)) = requests.get(*child) else {
            continue;
        };
        if !matches!(
            state,
            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
        ) {
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
            .try_remove::<WriteTask>();
        changed = true;
    }
    if changed && let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn finish_write(
    mut tasks: Query<(
        Entity,
        &ChildOf,
        &ArchiveWrite,
        &ProviderKind,
        &ProviderSessionId,
        &mut WriteTask,
    )>,
    mut observations: MessageWriter<ArchiveWritten>,
    mut completed: MessageWriter<ProviderArchiveCompleted>,
    mut commands: Commands,
) {
    for (operation, parent, request, kind, id, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        let success = result.is_ok();
        match result {
            Ok(state) => {
                observations.write(ArchiveWritten { operation, state });
                commands.entity(parent.parent()).insert(RefreshRequested);
            }
            Err(error) => {
                bevy::log::warn!("failed to archive Codex session: {error}");
                commands
                    .entity(operation)
                    .insert(vmux_ecs::OperationState::Failed);
                if let Ok(error) = vmux_ecs::OperationError::new(error) {
                    commands.entity(operation).insert(error);
                }
            }
        }
        completed.write(ProviderArchiveCompleted {
            kind: kind.clone(),
            id: id.clone(),
            archived: request.archived,
            revision: request.revision,
            success,
        });
        commands.entity(operation).remove::<WriteTask>();
    }
}

fn refresh(
    paths: Option<Single<&SourcePaths>>,
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
    if let Some(paths) = paths
        .map(|paths| (**paths).clone())
        .or_else(SourcePaths::current)
    {
        let operation = commands
            .spawn((
                ArchiveRead {
                    generation: generation.0,
                },
                Started(Instant::now()),
                ChildOf(source),
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
    fn removing_the_storage_owner_acknowledges_active_archive_writes() {
        for (state, expected) in [
            (vmux_ecs::OperationState::Pending, 1),
            (vmux_ecs::OperationState::Running, 1),
            (vmux_ecs::OperationState::Succeeded, 0),
        ] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, vmux_session::provider::Plugin, Plugin));
            let source = app.world_mut().spawn(SourceKind::Codex.bundle()).id();
            app.world_mut().spawn((
                ArchiveWrite {
                    archived: true,
                    revision: 41,
                },
                ProviderKind("acp:codex".into()),
                ProviderSessionId("owned-write".into()),
                ChildOf(source),
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
