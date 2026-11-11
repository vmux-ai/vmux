use std::collections::HashSet;

use bevy_app::{App, Plugin as BevyPlugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::{
    Operation, OperationCompleted, OperationState, OperationTarget, OperationTargetRevision,
    Revision,
};

use crate::{
    Collapsed, FileId, FoldSet, LoadCompleted, LoadRequest, Loaded, Phase, Record, RetryRequest,
    Root, SaveCompleted, SaveRequest, Scope, Snapshot, State, Ticket, UpdateRequest,
};

pub(super) struct Plugin;

impl BevyPlugin for Plugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clock>()
            .add_message::<LoadRequest>()
            .add_message::<LoadCompleted>()
            .add_message::<UpdateRequest>()
            .add_message::<RetryRequest>()
            .add_message::<SaveRequest>()
            .add_message::<SaveCompleted>()
            .configure_sets(
                Update,
                (
                    FoldSet::Restore,
                    FoldSet::Mutate,
                    FoldSet::Save,
                    FoldSet::Project,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (retry, reset, cancel, restore)
                    .chain()
                    .in_set(FoldSet::Restore),
            )
            .add_systems(Update, mutate.in_set(FoldSet::Mutate))
            .add_systems(Update, (finish, save).chain().in_set(FoldSet::Save));
    }
}

#[derive(Resource, Default)]
struct Clock(u64);

impl Clock {
    fn next(&mut self, owner: Entity, path: &std::path::Path) -> Ticket {
        self.0 = self
            .0
            .checked_add(1)
            .expect("fold store generation exhausted");
        Ticket {
            owner,
            path: path.to_path_buf(),
            generation: self.0,
        }
    }
}

#[derive(Component)]
struct Loading(Ticket);

#[derive(Component)]
struct Saving {
    operation: Entity,
    ticket: Ticket,
    revision: u64,
}

#[derive(Component)]
struct Accepted(Ticket);

#[derive(Component)]
struct Retry;

#[derive(Component)]
struct Reload;

fn retry(
    mut requests: MessageReader<RetryRequest>,
    roots: Query<(&Root, &Scope, &State)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let ticket = &request.0;
        let Ok((root, scope, state)) = roots.get(ticket.owner) else {
            continue;
        };
        if root.0 != ticket.path || scope.0 != *ticket {
            continue;
        }
        if state.phase() == Phase::Failed {
            commands.entity(ticket.owner).insert(Reload);
        } else {
            commands.entity(ticket.owner).insert(Retry);
        }
    }
}

type ResetRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, Root>,
        Option<&'static Scope>,
        Has<Reload>,
    ),
>;

fn reset(
    roots: ResetRoots,
    records: Query<(Entity, &ChildOf), With<Record>>,
    mut clock: ResMut<Clock>,
    mut loads: MessageWriter<LoadRequest>,
    mut commands: Commands,
) {
    for (owner, root, scope, reload) in &roots {
        if !reload && !root.is_changed() && scope.is_some_and(|scope| scope.0.path == root.0) {
            continue;
        }
        for (entity, parent) in &records {
            if parent.parent() == owner {
                commands.entity(entity).despawn();
            }
        }
        let ticket = clock.next(owner, &root.0);
        let mut state = State::default();
        state.materialize();
        commands
            .entity(owner)
            .remove::<(Loaded, Saving, Retry, Reload)>()
            .insert((state, Scope(ticket.clone()), Loading(ticket.clone())));
        loads.write(LoadRequest(ticket));
    }
}

fn cancel(
    roots: Query<(&Root, &Scope)>,
    mut operations: Query<(Entity, &Accepted, &mut OperationState)>,
    mut commands: Commands,
) {
    for (entity, accepted, mut state) in &mut operations {
        let ticket = &accepted.0;
        if roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            continue;
        }
        if matches!(*state, OperationState::Pending | OperationState::Running) {
            *state = OperationState::Canceled;
        }
        commands.entity(entity).remove::<Accepted>();
    }
}

fn restore(
    mut completed: MessageReader<LoadCompleted>,
    mut roots: Query<(&Root, &Scope, &Loading, &mut State)>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        let ticket = &completion.ticket;
        let Ok((root, scope, loading, mut state)) = roots.get_mut(ticket.owner) else {
            continue;
        };
        if root.0 != ticket.path || scope.0 != *ticket || loading.0 != *ticket {
            continue;
        }
        commands.entity(ticket.owner).remove::<Loading>();
        match &completion.result {
            Ok(snapshot) => {
                for (path, collapsed) in &snapshot.files {
                    commands.spawn((
                        Record,
                        FileId(path.into()),
                        Collapsed(collapsed.iter().copied().collect()),
                        ChildOf(ticket.owner),
                    ));
                }
                state.ready();
                commands.entity(ticket.owner).insert(Loaded);
            }
            Err(error) => state.fail(error.clone()),
        }
    }
}

fn mutate(world: &mut World) {
    let requests: Vec<_> = world
        .resource_mut::<Messages<UpdateRequest>>()
        .drain()
        .collect();
    for request in requests {
        let ticket = &request.ticket;
        let Some(root) = world.get::<Root>(ticket.owner) else {
            continue;
        };
        if root.0 != ticket.path
            || world
                .get::<Scope>(ticket.owner)
                .is_none_or(|scope| scope.0 != *ticket)
            || world.get::<Loaded>(ticket.owner).is_none()
        {
            continue;
        }
        let mut records =
            world.query_filtered::<(Entity, &FileId, &Collapsed, &ChildOf), With<Record>>();
        let mut found = None;
        let mut duplicate = false;
        for (entity, id, collapsed, parent) in records.iter(world) {
            if parent.parent() != ticket.owner || *id != request.file {
                continue;
            }
            if found.is_some() {
                duplicate = true;
                break;
            }
            found = Some((entity, collapsed.clone()));
        }
        if duplicate {
            continue;
        }
        let mut collapsed = found
            .as_ref()
            .map(|(_, collapsed)| collapsed.clone())
            .unwrap_or_default();
        let previous = collapsed.clone();
        collapsed.apply(&request.mutation);
        if collapsed == previous {
            continue;
        }
        match found {
            Some((entity, _)) => {
                let revision = world.get::<Revision>(entity).copied().unwrap_or_default();
                world
                    .entity_mut(entity)
                    .insert((collapsed, revision.saturating_next()));
            }
            None => {
                world.spawn((
                    Record,
                    request.file,
                    collapsed,
                    Revision(1),
                    ChildOf(ticket.owner),
                ));
            }
        }
        if let Some(mut state) = world.get_mut::<State>(ticket.owner) {
            state.advance();
        }
    }
}

fn finish(
    mut completed: MessageReader<SaveCompleted>,
    mut roots: Query<(&Root, &Scope, &Saving, &mut State)>,
    mut commands: Commands,
) {
    for completion in completed.read() {
        let ticket = &completion.ticket;
        let Ok((root, scope, saving, mut state)) = roots.get_mut(ticket.owner) else {
            continue;
        };
        if root.0 != ticket.path
            || scope.0 != *ticket
            || saving.ticket != *ticket
            || saving.operation != completion.operation
            || saving.revision != completion.revision
        {
            continue;
        }
        match &completion.result {
            Ok(()) => {
                state.succeed(completion.revision);
            }
            Err(error) => {
                state.retry(completion.revision, error.clone());
            }
        }
        commands.entity(ticket.owner).remove::<Saving>();
        commands.entity(completion.operation).remove::<Accepted>();
        commands.trigger(OperationCompleted::from_result(
            completion.operation,
            &completion.result,
        ));
    }
}

type SaveRoots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Scope,
        &'static mut State,
        Has<Retry>,
    ),
    With<Loaded>,
>;

fn save(
    mut roots: SaveRoots,
    records: Query<(&FileId, &Collapsed, &ChildOf), With<Record>>,
    mut requests: MessageWriter<SaveRequest>,
    mut commands: Commands,
) {
    for (owner, root, scope, mut state, retry) in &mut roots {
        if root.0 != scope.0.path || (state.error().is_some() && !retry) {
            continue;
        }
        let Some(revision) = state.begin() else {
            continue;
        };
        let mut snapshot = Snapshot::default();
        let mut seen = HashSet::new();
        let mut duplicate = false;
        for (file, collapsed, parent) in &records {
            if parent.parent() != owner {
                continue;
            }
            if !seen.insert(&file.0) {
                duplicate = true;
                break;
            }
            if !collapsed.0.is_empty() {
                snapshot.files.insert(
                    file.0.to_string_lossy().into_owned(),
                    collapsed.0.iter().copied().collect(),
                );
            }
        }
        if duplicate {
            state.retry(revision, "duplicate fold file identity".into());
            commands.entity(owner).remove::<Retry>();
            continue;
        }
        let ticket = scope.0.clone();
        let operation = commands
            .spawn((
                Operation::bundle("fold.save").unwrap().running(),
                OperationTarget::new(owner),
                OperationTargetRevision(revision),
                Accepted(ticket.clone()),
            ))
            .id();
        commands.entity(owner).remove::<Retry>().insert(Saving {
            operation,
            ticket: ticket.clone(),
            revision,
        });
        requests.write(SaveRequest {
            ticket,
            operation,
            revision,
            snapshot,
        });
    }
}
