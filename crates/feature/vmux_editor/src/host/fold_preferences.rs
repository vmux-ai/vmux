use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use bevy::ecs::system::SystemParam;
use serde::{Deserialize, Serialize};
pub use vmux_ecs::store::{Phase, State};
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationState, OperationTarget,
    OperationTargetRevision, Revision,
};

pub struct FoldPlugin;

impl Plugin for FoldPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
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

#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[require(State)]
pub struct Root(pub PathBuf);

#[derive(Component)]
pub struct Loaded;

#[derive(Component)]
#[require(Revision)]
pub struct Record;

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileId(pub PathBuf);

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Collapsed(pub BTreeSet<u32>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub owner: Entity,
    pub path: PathBuf,
    pub generation: u64,
}

#[derive(Component)]
pub(crate) struct Scope(pub Ticket);

#[derive(Message, Clone)]
pub struct UpdateRequest {
    pub ticket: Ticket,
    pub file: FileId,
    pub mutation: Mutation,
}

#[derive(Clone)]
pub struct Mutation {
    pub collapse: BTreeSet<u32>,
    pub expand: BTreeSet<u32>,
}

impl Collapsed {
    fn apply(&mut self, mutation: &Mutation) {
        for line in &mutation.expand {
            self.0.remove(line);
        }
        self.0.extend(&mutation.collapse);
    }
}

#[derive(Message, Clone)]
pub struct RetryRequest(pub Ticket);

#[derive(Message, Clone)]
pub struct LoadRequest(pub Ticket);

#[derive(Message)]
pub struct LoadCompleted {
    pub ticket: Ticket,
    pub result: Result<Snapshot, String>,
}

#[derive(Message, Clone)]
pub struct SaveRequest {
    pub ticket: Ticket,
    pub operation: Entity,
    pub revision: u64,
    pub snapshot: Snapshot,
}

#[derive(Message)]
pub struct SaveCompleted {
    pub ticket: Ticket,
    pub operation: Entity,
    pub revision: u64,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub files: BTreeMap<String, Vec<u32>>,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoldSet {
    Restore,
    Mutate,
    Save,
    Project,
}

#[derive(SystemParam)]
pub struct Preferences<'w, 's> {
    roots: Query<'w, 's, (&'static Root, &'static Scope, Option<&'static Loaded>)>,
    records: Query<
        'w,
        's,
        (
            Entity,
            &'static FileId,
            &'static Collapsed,
            &'static Revision,
            &'static ChildOf,
        ),
        With<Record>,
    >,
}

impl Preferences<'_, '_> {
    pub fn ticket(&self, root: Entity) -> Option<&Ticket> {
        let (path, scope, _) = self.roots.get(root).ok()?;
        if path.0 != scope.0.path {
            return None;
        }
        Some(&scope.0)
    }

    pub fn ready(&self, ticket: &Ticket) -> bool {
        self.roots
            .get(ticket.owner)
            .is_ok_and(|(path, scope, loaded)| {
                path.0 == ticket.path && scope.0 == *ticket && loaded.is_some()
            })
    }

    pub fn get(&self, ticket: &Ticket, file: &Path) -> Option<(Entity, &Collapsed, &Revision)> {
        if !self.ready(ticket) {
            return None;
        }
        let mut found = None;
        for (entity, id, collapsed, revision, parent) in &self.records {
            if parent.parent() == ticket.owner && id.0 == file {
                if found.is_some() {
                    return None;
                }
                found = Some((entity, collapsed, revision));
            }
        }
        found
    }
}

#[derive(Resource, Default)]
struct Clock(u64);

impl Clock {
    fn next(&mut self, owner: Entity, path: &Path) -> Ticket {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::SystemState;
    use vmux_ecs::OperationState;

    struct Fixture {
        app: App,
        root: Entity,
        ticket: Ticket,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(FoldPlugin);
            let root = app.world_mut().spawn(Root("/store/folds.ron".into())).id();
            app.update();
            let ticket = app
                .world_mut()
                .resource_mut::<Messages<LoadRequest>>()
                .drain()
                .next()
                .unwrap()
                .0;
            app.world_mut().write_message(LoadCompleted {
                ticket: ticket.clone(),
                result: Ok(Snapshot::default()),
            });
            app.update();
            Self { app, root, ticket }
        }

        fn change(&mut self, lines: &[u32]) -> SaveRequest {
            let mut state = SystemState::<Preferences>::new(self.app.world_mut());
            let expand = state
                .get(self.app.world())
                .unwrap()
                .get(&self.ticket, Path::new("/files/shared.rs"))
                .map(|(_, collapsed, _)| collapsed.0.clone())
                .unwrap_or_default();
            self.app.world_mut().write_message(UpdateRequest {
                ticket: self.ticket.clone(),
                file: FileId("/files/shared.rs".into()),
                mutation: Mutation {
                    collapse: lines.iter().copied().collect(),
                    expand,
                },
            });
            self.app.update();
            self.take_save()
        }

        fn take_save(&mut self) -> SaveRequest {
            self.app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .unwrap()
        }

        fn finish(&mut self, request: &SaveRequest, result: Result<(), String>) {
            self.app.world_mut().write_message(SaveCompleted {
                ticket: request.ticket.clone(),
                operation: request.operation,
                revision: request.revision,
                result,
            });
            self.app.update();
        }
    }

    #[test]
    fn failed_saves_retain_dirty_state_and_wait_for_explicit_retry() {
        let mut fixture = Fixture::new();
        let request = fixture.change(&[7, 2]);
        fixture.finish(&request, Err("disk full".into()));
        let state = fixture.app.world().get::<State>(fixture.root).unwrap();
        assert_eq!(state.dirty(), Some(request.revision));
        assert_eq!(state.error(), Some("disk full"));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .is_none()
        );
        fixture
            .app
            .world_mut()
            .write_message(RetryRequest(fixture.ticket.clone()));
        fixture.app.update();
        let retry = fixture.take_save();
        assert_eq!(retry.snapshot.files["/files/shared.rs"], [2, 7]);
        fixture.finish(&retry, Ok(()));
        let state = fixture.app.world().get::<State>(fixture.root).unwrap();
        assert!(!state.is_dirty());
        assert!(state.error().is_none());
        assert_eq!(
            fixture.app.world().get::<OperationState>(retry.operation),
            Some(&OperationState::Succeeded)
        );
    }

    #[test]
    fn older_save_success_cannot_clear_newer_fold_changes() {
        let mut fixture = Fixture::new();
        let first = fixture.change(&[2]);
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: fixture.ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [9].into(),
                expand: [2].into(),
            },
        });
        fixture.app.update();
        fixture.finish(&first, Ok(()));
        let second = fixture.take_save();
        assert!(second.revision > first.revision);
        assert_eq!(second.snapshot.files["/files/shared.rs"], [9]);
        fixture.finish(&first, Ok(()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .saving(),
            Some(second.revision)
        );
        fixture.finish(&second, Ok(()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .loaded(),
            Some(second.revision)
        );
    }

    #[test]
    fn roots_isolate_file_identity_and_view_deltas_compose() {
        let mut fixture = Fixture::new();
        let other = fixture
            .app
            .world_mut()
            .spawn(Root("/other/folds.ron".into()))
            .id();
        fixture.app.update();
        let ticket = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: ticket.clone(),
            result: Ok(Snapshot::default()),
        });
        fixture.app.update();
        for line in [2, 7] {
            fixture.app.world_mut().write_message(UpdateRequest {
                ticket: fixture.ticket.clone(),
                file: FileId("/files/shared.rs".into()),
                mutation: Mutation {
                    collapse: [line].into(),
                    expand: Default::default(),
                },
            });
        }
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [99].into(),
                expand: BTreeSet::new(),
            },
        });
        fixture.app.update();
        let mut state = SystemState::<Preferences>::new(fixture.app.world_mut());
        let preferences = state.get(fixture.app.world()).unwrap();
        assert_eq!(
            preferences
                .get(&fixture.ticket, Path::new("/files/shared.rs"))
                .unwrap()
                .1
                .0,
            [2, 7].into()
        );
        assert_eq!(
            preferences
                .get(&ticket, Path::new("/files/shared.rs"))
                .unwrap()
                .1
                .0,
            [99].into()
        );
        assert_ne!(other, fixture.root);
    }

    #[test]
    fn owner_removal_cancels_the_operation_and_refuses_its_late_result() {
        let mut fixture = Fixture::new();
        let request = fixture.change(&[2]);
        fixture.app.world_mut().despawn(fixture.root);
        let replacement = fixture
            .app
            .world_mut()
            .spawn(Root(request.ticket.path.clone()))
            .id();
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(request.operation),
            Some(&OperationState::Canceled)
        );
        fixture.finish(&request, Ok(()));
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(replacement)
                .unwrap()
                .loaded(),
            None
        );
        assert!(fixture.app.world().get::<Loaded>(replacement).is_none());
    }

    #[test]
    fn replacing_a_root_invalidates_mutations_and_load_results_from_its_old_generation() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(Root(fixture.ticket.path.clone()));
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: fixture.ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [2].into(),
                expand: BTreeSet::new(),
            },
        });
        fixture.app.update();
        let next = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap()
            .0;
        assert_ne!(next.generation, fixture.ticket.generation);
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: fixture.ticket.clone(),
            result: Ok(Snapshot {
                files: [("/files/shared.rs".into(), vec![2])].into(),
            }),
        });
        fixture.app.update();
        assert!(fixture.app.world().get::<Loaded>(fixture.root).is_none());
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: next,
            result: Ok(Snapshot::default()),
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<Record>>()
                .iter(fixture.app.world())
                .count(),
            0
        );
    }

    #[test]
    fn unreadable_store_stays_failed_and_cannot_be_overwritten_by_an_update() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.root)
            .insert(Root("/bad/folds.ron".into()));
        fixture.app.update();
        let ticket = fixture
            .app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap()
            .0;
        fixture.app.world_mut().write_message(LoadCompleted {
            ticket: ticket.clone(),
            result: Err("invalid RON".into()),
        });
        fixture.app.world_mut().write_message(UpdateRequest {
            ticket: ticket.clone(),
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [2].into(),
                expand: BTreeSet::new(),
            },
        });
        fixture.app.update();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<State>(fixture.root)
                .unwrap()
                .phase(),
            Phase::Failed
        );
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<SaveRequest>>()
                .drain()
                .next()
                .is_none()
        );
        fixture.app.world_mut().write_message(RetryRequest(ticket));
        fixture.app.update();
        assert!(
            fixture
                .app
                .world_mut()
                .resource_mut::<Messages<LoadRequest>>()
                .drain()
                .next()
                .is_some()
        );
    }
}
