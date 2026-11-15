use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Cwd, Operation, OperationState, Order, Ready};

use crate::Attempt;
use crate::debug_io::{Flight, Task};
use crate::execution::Failure;
use crate::publication_io::Scoped;
use crate::reset_io::Rename;
use crate::signals::Signals;
use crate::storage_io::{Done, Inspect, PathMetadata, Remove, StorageJob};
use crate::temporary::Temporary;

pub(crate) struct PublicationPlugin;

impl bevy_app::Plugin for PublicationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<Inspect>::default(),
            IoPlugin::<Rename>::default(),
            IoPlugin::<Remove>::default(),
        ))
        .configure_sets(
            Update,
            (Stage::Collect, Stage::Decide, Stage::Submit, Stage::Finish)
                .chain()
                .in_set(JobSet::Apply)
                .after(crate::Stage::Execute),
        )
        .add_systems(PreUpdate, cancel)
        .add_systems(
            Update,
            (
                initialize,
                cancel,
                (adopt, inspect).chain(),
                (rollback, advance).chain(),
                retire,
            )
                .chain()
                .in_set(Stage::Decide),
        )
        .add_systems(Update, finish.in_set(Stage::Finish));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Stage {
    Collect,
    Decide,
    Submit,
    Finish,
}

#[derive(Component)]
pub(crate) struct Request {
    pub owner: Entity,
    pub lease: Option<Arc<crate::target_lock::TargetLock>>,
    pub context: Entity,
    pub attempt: u64,
    pub staging: Arc<Temporary>,
}

#[derive(Component)]
#[require(State)]
pub(crate) struct Move;

#[derive(Component)]
pub(crate) struct Destination(pub PathBuf);

#[derive(Component, Clone, Copy, Default, PartialEq, Eq, Debug)]
enum State {
    #[default]
    Pending,
    Inspecting,
    Prepared,
    BackingUp,
    BackupFailed,
    BackedUp,
    Publishing,
    Published,
    RollbackInspect,
    Removing,
    Restoring,
    RestoreFailed,
    Restored,
}

#[derive(Component)]
struct Backup(PathBuf);

#[derive(Component)]
struct Aborting;

#[derive(Component)]
pub(crate) struct Failed(pub Failure);

#[derive(Component)]
pub(crate) struct Drained;

#[derive(Component)]
struct Owner(Entity);

#[derive(Component)]
struct Io;

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Received;

#[derive(Component)]
struct Reported;

#[derive(Component, Clone, Copy)]
struct Purpose {
    kind: PurposeKind,
    entity: Option<Entity>,
}

impl Purpose {
    fn targeted(kind: PurposeKind, entity: Entity) -> Self {
        Self {
            kind,
            entity: Some(entity),
        }
    }

    fn target(&self, kind: PurposeKind) -> Option<&Entity> {
        if self.kind != kind {
            return None;
        }
        self.entity.as_ref()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PurposeKind {
    Inspect,
    Backup,
    BackupProbe,
    Publish,
    Remove,
    Restore,
    Rollback,
}

type Incoming<'w, 's> = Query<'w, 's, (Entity, &'static Request), Without<OperationState>>;
type Moves<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ChildOf,
        &'static Cwd,
        &'static Destination,
        &'static Order,
        &'static State,
        Option<&'static Backup>,
    ),
    With<Move>,
>;
type Replies<'w, 's, T> =
    Query<'w, 's, (Entity, &'static Owner, &'static Purpose, &'static T), With<Received>>;
type Returned<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static Owner,
        &'static ChildOf,
        &'static JobOwner<Task<Scoped<J>>>,
        &'static JobResult<Task<Scoped<J>>>,
    ),
    Without<Received>,
>;
type Pending<'w, 's, J> =
    Query<'w, 's, (Entity, &'static Owner, &'static J), (With<Io>, Without<Submitted>)>;
type Active<'w, 's> =
    Query<'w, 's, (Entity, &'static Request, Option<&'static Failed>), Without<Drained>>;

#[derive(bevy_ecs::system::SystemParam)]
struct Writer<'w, 's> {
    commands: Commands<'w, 's>,
}

impl Writer<'_, '_> {
    fn request<J: StorageJob>(&mut self, root: Entity, purpose: Purpose, job: J) {
        self.commands.spawn((
            Io,
            Owner(root),
            purpose,
            job,
            Operation::bundle("dx.publication.io").unwrap(),
            ChildOf(root),
        ));
    }
}

struct IoPlugin<J: StorageJob>(std::marker::PhantomData<fn() -> J>);

impl<J: StorageJob> Default for IoPlugin<J> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<J: StorageJob> bevy_app::Plugin for IoPlugin<J> {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<Scoped<J>>>::default())
            .add_systems(Update, collect::<J>.in_set(Stage::Collect))
            .add_systems(Update, submit::<J>.in_set(Stage::Submit));
    }
}

type InitializePlans<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        &'static Cwd,
        &'static Destination,
        &'static Order,
    ),
    With<Move>,
>;

type CollectMoves<'w, 's> = Query<'w, 's, Option<&'static Backup>, With<Move>>;

type FinishRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static Failed>),
    (With<Request>, With<Drained>, Without<Reported>),
>;

fn initialize(incoming: Incoming, plans: InitializePlans, mut commands: Commands) {
    for (entity, request) in &incoming {
        let mut orders = std::collections::HashSet::new();
        let mut destinations = std::collections::HashSet::new();
        let mut invalid = false;
        for (parent, source, destination, order) in &plans {
            if parent.parent() != entity {
                continue;
            }
            if source.0.parent() != Some(request.staging.path())
                || destination.0.parent() != request.staging.path().parent()
                || source.0.file_name() != destination.0.file_name()
                || !orders.insert(order.0)
                || !destinations.insert(destination.0.clone())
            {
                invalid = true;
                break;
            }
        }
        if invalid {
            commands.entity(entity).insert((
                Aborting,
                Failed(Failure::message("Invalid or duplicate publication target")),
            ));
        }
        commands
            .entity(entity)
            .insert(Operation::bundle("dx.publication").unwrap().running());
    }
}

fn collect<J: StorageJob>(
    results: Returned<J>,
    failed: Query<(), With<Failed>>,
    purposes: Query<&Purpose>,
    moves: CollectMoves,
    mut commands: Commands,
) {
    for (entity, owner, parent, job_owner, result) in &results {
        if parent.parent() != owner.0 || !job_owner.accepts(Some(parent)) {
            continue;
        }
        let value = match &result.0 {
            Ok(report) => report.result.clone(),
            Err(error) => Err(Failure::message(error.clone())),
        };
        match value {
            Ok(value) => {
                commands
                    .entity(entity)
                    .insert((value, Received, OperationState::Succeeded));
            }
            Err(error) => {
                if !failed.contains(owner.0) {
                    commands.entity(owner.0).insert(Failed(error));
                }
                commands.entity(owner.0).insert(Aborting);
                commands.entity(entity).insert(OperationState::Failed);
                if let Ok(purpose) = purposes.get(entity) {
                    match purpose {
                        Purpose {
                            kind: PurposeKind::Backup,
                            entity: Some(target),
                        } => {
                            commands.entity(*target).insert(State::BackupFailed);
                        }
                        Purpose {
                            kind: PurposeKind::Publish,
                            entity: Some(target),
                        } => {
                            commands.entity(*target).insert(
                                if moves.get(*target).is_ok_and(|backup| backup.is_some()) {
                                    State::BackedUp
                                } else {
                                    State::Prepared
                                },
                            );
                        }
                        Purpose {
                            kind: PurposeKind::Restore,
                            entity: Some(target),
                        } => {
                            commands.entity(*target).insert(State::RestoreFailed);
                        }
                        Purpose {
                            kind: PurposeKind::Rollback,
                            entity: Some(target),
                        } => {
                            commands.entity(*target).insert(State::Removing);
                        }
                        Purpose {
                            kind: PurposeKind::BackupProbe,
                            entity: Some(target),
                        } => {
                            commands.entity(*target).insert(State::RestoreFailed);
                        }
                        _ => {}
                    }
                }
            }
        }
        commands
            .entity(entity)
            .remove::<JobResult<Task<Scoped<J>>>>();
    }
}

fn submit<J: StorageJob>(
    pending: Pending<J>,
    requests: Query<&Request, Without<Drained>>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, owner, job) in &pending {
        let Ok(request) = requests.get(owner.0) else {
            continue;
        };
        let loan = request.lease.clone();
        let (task, flight) = Task::new(
            Scoped(job.clone()),
            loan,
            vec![request.staging.clone()],
            wake.0.clone(),
        );
        commands.entity(entity).insert((
            Submitted,
            JobRequest(task),
            flight,
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

fn cancel(
    requests: Active,
    owners: Query<(&Attempt, &ChildOf)>,
    states: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, request, failed) in &requests {
        let valid = owners.get(request.owner).is_ok_and(|(attempt, context)| {
            attempt.0 == request.attempt && context.parent() == request.context
        }) && states
            .get(request.context)
            .is_ok_and(|state| *state == OperationState::Running);
        if valid && Signals::pending().is_none() {
            continue;
        }
        if failed.is_none() {
            commands.entity(entity).insert(Failed(Failure::interrupted(
                Signals::pending().unwrap_or(15),
            )));
        }
        commands.entity(entity).insert(Aborting);
    }
}

fn adopt(done: Replies<Done>, moves: Moves, mut io: Writer) {
    for (receipt, _, purpose, _) in &done {
        let (target, state) = match purpose {
            Purpose {
                kind: PurposeKind::Backup,
                entity: Some(entity),
            } => (*entity, State::BackedUp),
            Purpose {
                kind: PurposeKind::Publish,
                entity: Some(entity),
            } => (*entity, State::Published),
            Purpose {
                kind: PurposeKind::Remove,
                entity: Some(entity),
            } => {
                let Ok((_, _, _, _, _, _, backup)) = moves.get(*entity) else {
                    continue;
                };
                (
                    *entity,
                    if backup.is_some() {
                        State::Restoring
                    } else {
                        State::Restored
                    },
                )
            }
            Purpose {
                kind: PurposeKind::Restore,
                entity: Some(entity),
            } => (*entity, State::Restored),
            _ => continue,
        };
        io.commands.entity(target).insert(state);
        if state == State::Published {
            io.commands.entity(target).insert(Ready);
        }
        io.commands.entity(receipt).despawn();
    }
}

fn inspect(
    metadata: Replies<PathMetadata>,
    moves: Moves,
    requests: Query<&Request>,
    aborting: Query<(), With<Aborting>>,
    mut io: Writer,
) {
    for (receipt, owner, purpose, metadata) in &metadata {
        let target = match purpose {
            Purpose {
                kind: PurposeKind::Inspect,
                entity: Some(entity),
            }
            | Purpose {
                kind: PurposeKind::Rollback,
                entity: Some(entity),
            }
            | Purpose {
                kind: PurposeKind::BackupProbe,
                entity: Some(entity),
            } => *entity,
            _ => continue,
        };
        let Ok((_, _, _, destination, order, _, backup)) = moves.get(target) else {
            continue;
        };
        let Ok(request) = requests.get(owner.0) else {
            continue;
        };
        if purpose.target(PurposeKind::Inspect).is_some() {
            if aborting.contains(owner.0) {
                io.commands.entity(target).insert(State::Pending);
            } else if metadata
                .0
                .as_ref()
                .is_some_and(|metadata| metadata.is_symlink())
            {
                io.commands.entity(target).insert(State::Pending);
                io.commands.entity(owner.0).insert((
                    Aborting,
                    Failed(Failure::message("Refusing to replace a symlink artifact")),
                ));
            } else if metadata.0.is_some() {
                let backup = request
                    .staging
                    .path()
                    .join(format!(".previous-{}", order.0));
                io.commands
                    .entity(target)
                    .insert((Backup(backup.clone()), State::BackingUp));
                io.request(
                    owner.0,
                    Purpose::targeted(PurposeKind::Backup, target),
                    Rename {
                        source: destination.0.clone(),
                        destination: backup,
                    },
                );
            } else {
                io.commands.entity(target).insert(State::Prepared);
            }
        } else if purpose.target(PurposeKind::BackupProbe).is_some() {
            io.commands.entity(target).insert(if metadata.0.is_some() {
                State::BackedUp
            } else {
                State::Restored
            });
        } else if let Some(metadata) = &metadata.0 {
            io.commands.entity(target).insert(State::Removing);
            io.request(
                owner.0,
                Purpose::targeted(PurposeKind::Remove, target),
                Remove {
                    path: destination.0.clone(),
                    directory: metadata.is_dir(),
                    lock: None,
                },
            );
        } else {
            io.commands.entity(target).insert(if backup.is_some() {
                State::Restoring
            } else {
                State::Restored
            });
        }
        io.commands.entity(receipt).despawn();
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct Selection<'w, 's> {
    moves: Moves<'w, 's>,
}

impl Selection<'_, '_> {
    fn next(&self, root: Entity, rollback: bool) -> Option<Entity> {
        let mut candidate = None;
        let mut candidate_order = None;
        for item in &self.moves {
            if item.1.parent() != root {
                continue;
            }
            if rollback
                && matches!(
                    *item.5,
                    State::Pending | State::Inspecting | State::Prepared | State::Restored
                )
            {
                continue;
            }
            if !rollback && *item.5 == State::Published {
                continue;
            }
            if candidate_order.is_none_or(|previous| {
                if rollback {
                    item.4.0 > previous
                } else {
                    item.4.0 < previous
                }
            }) {
                candidate = Some(item);
                candidate_order = Some(item.4.0);
            }
        }
        candidate.map(|item| item.0)
    }
}

type Rollbacks<'w, 's> =
    Query<'w, 's, (Entity, &'static Request), (Without<Drained>, With<Aborting>)>;

type Advancing<'w, 's> =
    Query<'w, 's, (Entity, &'static Request), (Without<Drained>, Without<Aborting>)>;

fn rollback(
    requests: Rollbacks,
    selection: Selection,
    io_rows: Query<(Entity, &Owner, &OperationState), With<Io>>,
    flights: Query<(&Owner, &Flight)>,
    wake: Res<JobWake>,
    mut io: Writer,
) {
    for (root, request) in &requests {
        if io_rows.iter().any(|(_, owner, state)| {
            owner.0 == root && matches!(*state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        if flights
            .iter()
            .any(|(owner, flight)| owner.0 == root && flight.pending())
        {
            continue;
        }
        for (row, owner, _) in &io_rows {
            if owner.0 == root {
                io.commands.entity(row).despawn();
            }
        }
        let rollback = true;
        let Some((entity, _, _source, destination, _, state, backup)) = selection
            .next(root, rollback)
            .and_then(|entity| selection.moves.get(entity).ok())
        else {
            io.commands.entity(root).insert(Drained);
            if !rollback {
                io.commands.entity(root).insert(Ready);
            }
            wake.0.wake_by_ref();
            continue;
        };

        match *state {
            State::BackupFailed => {
                if let Some(backup) = backup {
                    io.request(
                        root,
                        Purpose::targeted(PurposeKind::BackupProbe, entity),
                        Inspect(backup.0.clone()),
                    );
                } else {
                    io.commands.entity(entity).insert(State::Restored);
                    wake.0.wake_by_ref();
                }
            }
            State::RestoreFailed => {
                request.staging.retention().store(true, Ordering::Release);
                eprintln!(
                    "Artifact restore failed at {}; retained {}",
                    backup
                        .map_or(destination.0.as_path(), |backup| backup.0.as_path())
                        .display(),
                    request.staging.path().display()
                );
                io.commands.entity(entity).insert(State::Restored);
                wake.0.wake_by_ref();
            }
            State::Restoring => {
                let Some(backup) = backup else {
                    io.commands.entity(entity).insert(State::Restored);
                    wake.0.wake_by_ref();
                    continue;
                };
                io.request(
                    root,
                    Purpose::targeted(PurposeKind::Restore, entity),
                    Rename {
                        source: backup.0.clone(),
                        destination: destination.0.clone(),
                    },
                );
            }
            State::Removing => {
                request.staging.retention().store(true, Ordering::Release);
                eprintln!(
                    "Artifact rollback failed at {}; retained {}",
                    destination.0.display(),
                    request.staging.path().display()
                );
                io.commands.entity(entity).insert(if backup.is_some() {
                    State::Restoring
                } else {
                    State::Restored
                });
                wake.0.wake_by_ref();
            }
            State::Restored => {}
            _ => {
                io.commands.entity(entity).insert(State::RollbackInspect);
                io.request(
                    root,
                    Purpose::targeted(PurposeKind::Rollback, entity),
                    Inspect(destination.0.clone()),
                );
            }
        }
    }
}

fn advance(
    requests: Advancing,
    selection: Selection,
    io_rows: Query<(Entity, &Owner, &OperationState), With<Io>>,
    flights: Query<(&Owner, &Flight)>,
    wake: Res<JobWake>,
    mut io: Writer,
) {
    for (root, request) in &requests {
        if io_rows.iter().any(|(_, owner, state)| {
            owner.0 == root && matches!(*state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        if flights
            .iter()
            .any(|(owner, flight)| owner.0 == root && flight.pending())
        {
            continue;
        }
        for (row, owner, _) in &io_rows {
            if owner.0 == root {
                io.commands.entity(row).despawn();
            }
        }
        let rollback = false;
        let Some((entity, _, source, destination, _, state, _backup)) = selection
            .next(root, rollback)
            .and_then(|entity| selection.moves.get(entity).ok())
        else {
            io.commands.entity(root).insert(Drained);
            if !rollback {
                io.commands.entity(root).insert(Ready);
            }
            wake.0.wake_by_ref();
            continue;
        };

        match *state {
            State::Pending => {
                if !source.0.starts_with(request.staging.path())
                    || source.0.parent() != Some(request.staging.path())
                {
                    io.commands.entity(root).insert((
                        Aborting,
                        Failed(Failure::message("Publication source is outside staging")),
                    ));
                    wake.0.wake_by_ref();
                    continue;
                }
                io.commands.entity(entity).insert(State::Inspecting);
                io.request(
                    root,
                    Purpose::targeted(PurposeKind::Inspect, entity),
                    Inspect(destination.0.clone()),
                );
            }
            State::Prepared | State::BackedUp => {
                io.commands.entity(entity).insert(State::Publishing);
                io.request(
                    root,
                    Purpose::targeted(PurposeKind::Publish, entity),
                    Rename {
                        source: source.0.clone(),
                        destination: destination.0.clone(),
                    },
                );
            }
            _ => {}
        }
    }
}

#[derive(bevy_ecs::system::SystemParam)]
pub(crate) struct Drains<'w, 's> {
    requests: Query<'w, 's, &'static Request>,
}

impl Drains<'_, '_> {
    pub(crate) fn pending(&self, context: Entity) -> bool {
        self.requests
            .iter()
            .any(|request| request.context == context)
    }
}

fn retire(
    requests: Query<(Entity, &Request, Option<&Failed>), With<Drained>>,
    owners: Query<(), With<crate::Completed>>,
    existing: Query<()>,
    mut commands: Commands,
) {
    for (entity, request, failure) in &requests {
        if owners.contains(request.owner) {
            commands.entity(entity).despawn();
            continue;
        };
        if existing.contains(request.owner) {
            continue;
        };
        let error = failure
            .map(|error| error.0.clone())
            .unwrap_or_else(|| Failure::message("Publication owner disappeared"));
        if let Ok(mut context) = commands.get_entity(request.context) {
            context.insert(crate::Retirement(error));
        }
        commands.entity(entity).despawn();
    }
}

fn finish(
    requests: FinishRequests,
    moves: Query<(Entity, &ChildOf, &State), With<Move>>,
    mut commands: Commands,
) {
    for (root, failed) in &requests {
        for (entity, parent, state) in &moves {
            if parent.parent() == root && *state != State::Published {
                commands.entity(entity).remove::<Ready>();
            }
        }
        if let Some(error) = failed {
            eprintln!("Artifact publication failed: {}", error.0);
            commands.entity(root).insert(OperationState::Failed);
        } else {
            commands.entity(root).insert(OperationState::Succeeded);
        }
        commands.entity(root).insert(Reported);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    struct Fixture {
        _directory: tempfile::TempDir,
        app: App,
        context: Entity,
        request: Entity,
        staging: Arc<Temporary>,
        destination: PathBuf,
        _permit: crate::testing::Permit,
    }

    impl Fixture {
        fn new() -> Self {
            let permit = crate::testing::Permit::acquire();
            let directory = tempfile::tempdir().unwrap();
            let destination = directory.path().canonicalize().unwrap().join("release");
            fs::create_dir(&destination).unwrap();
            let staging = Arc::new(Temporary::create(&destination, ".publication").unwrap());
            let mut app = App::new();
            app.add_plugins(crate::DxPlugin);
            let context = app
                .world_mut()
                .spawn(
                    Operation::bundle("dx.publication.fixture")
                        .unwrap()
                        .running(),
                )
                .id();
            let step = app.world_mut().spawn((ChildOf(context), Attempt(1))).id();
            let request = app
                .world_mut()
                .spawn((Request {
                    owner: step,
                    lease: None,
                    context,
                    attempt: 1,
                    staging: staging.clone(),
                },))
                .id();
            Self {
                _permit: permit,
                _directory: directory,
                app,
                context,
                request,
                staging,
                destination,
            }
        }

        fn artifact(
            &mut self,
            name: &str,
            previous: &str,
            next: Option<&str>,
            order: u32,
        ) -> Entity {
            fs::write(self.destination.join(name), previous).unwrap();
            if let Some(next) = next {
                fs::write(self.staging.path().join(name), next).unwrap();
            }
            self.app
                .world_mut()
                .spawn((
                    Move,
                    Cwd(self.staging.path().join(name)),
                    Destination(self.destination.join(name)),
                    Order(order),
                    ChildOf(self.request),
                ))
                .id()
        }

        fn run(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.app.world().get::<Drained>(self.request).is_none() {
                self.app.update();
                assert!(Instant::now() < deadline, "publication did not drain");
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }

        fn wait(&mut self, entity: Entity, state: State) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.app.world().get::<State>(entity) != Some(&state) {
                self.app.update();
                assert!(
                    self.app.world().get::<Drained>(self.request).is_none(),
                    "publication ended before mutation boundary"
                );
                assert!(
                    Instant::now() < deadline,
                    "publication never reached mutation boundary"
                );
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn successful_publication_replaces_all_outputs() {
        let mut fixture = Fixture::new();
        fixture.artifact("app", "old app", Some("new app"), 0);
        fixture.artifact("signature", "old signature", Some("new signature"), 1);
        fixture.run();
        assert!(fixture.app.world().get::<Failed>(fixture.request).is_none());
        assert_eq!(
            fs::read_to_string(fixture.destination.join("app")).unwrap(),
            "new app"
        );
        assert_eq!(
            fs::read_to_string(fixture.destination.join("signature")).unwrap(),
            "new signature"
        );
    }

    #[test]
    fn later_move_failure_restores_all_previous_outputs() {
        let mut fixture = Fixture::new();
        fixture.artifact("app", "old app", Some("new app"), 0);
        fixture.artifact("signature", "old signature", None, 1);
        fixture.run();
        assert!(fixture.app.world().get::<Failed>(fixture.request).is_some());
        assert_eq!(
            fs::read_to_string(fixture.destination.join("app")).unwrap(),
            "old app"
        );
        assert_eq!(
            fs::read_to_string(fixture.destination.join("signature")).unwrap(),
            "old signature"
        );
    }

    #[test]
    fn cancellation_adopts_an_inflight_move_and_rolls_it_back_before_draining() {
        let mut fixture = Fixture::new();
        let artifact = fixture.artifact("app", "old app", Some("new app"), 0);
        fixture.wait(artifact, State::Publishing);
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.context)
            .insert(OperationState::Canceled);
        fixture.run();
        assert!(fixture.app.world().get::<Failed>(fixture.request).is_some());
        assert_eq!(
            fs::read_to_string(fixture.destination.join("app")).unwrap(),
            "old app"
        );
    }

    #[test]
    fn removing_the_package_owner_preserves_and_rolls_back_its_inflight_publication() {
        let mut fixture = Fixture::new();
        let artifact = fixture.artifact("app", "old app", Some("new app"), 0);
        fixture.wait(artifact, State::Publishing);
        let owner = fixture
            .app
            .world()
            .get::<Request>(fixture.request)
            .unwrap()
            .owner;
        fixture.app.world_mut().entity_mut(owner).despawn();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world()
                .get::<vmux_ecs::cli::CliResult>(fixture.context)
                .is_some()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Publication did not roll back after owner removal"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            fs::read_to_string(fixture.destination.join("app")).unwrap(),
            "old app"
        );
        assert!(fixture.app.world().get_entity(fixture.request).is_err());
    }

    #[test]
    fn failed_restore_retains_the_staging_backup() {
        let mut fixture = Fixture::new();
        let artifact = fixture.artifact("app", "old app", Some("new app"), 0);
        fixture.artifact("signature", "old signature", None, 1);
        fixture.wait(artifact, State::Restoring);
        fs::create_dir(fixture.destination.join("app")).unwrap();
        fs::write(fixture.destination.join("app/blocker"), "keep").unwrap();
        fixture.run();
        assert!(fixture.app.world().get::<Failed>(fixture.request).is_some());
        assert!(fixture.staging.retention().load(Ordering::Acquire));
        assert_eq!(
            fs::read_to_string(fixture.staging.path().join(".previous-0")).unwrap(),
            "old app"
        );
    }
}
