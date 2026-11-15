use std::ffi::OsStr;
use std::path::PathBuf;

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Cwd, Operation, OperationState, Order};

use crate::debug_io::{Flight, Task};
use crate::execution::Failure;
use crate::process_observation;
use crate::repository::Checkout;
use crate::reset_io::{Rename, Scoped};
use crate::signals::Signals;
use crate::storage_io::{
    CanonicalPath, Canonicalize, Capture, DirectoryEntries, Done, Inspect, PathMetadata,
    ReadDirectory, Remove, StorageJob, ToolOutput,
};
use crate::work::Outcome;
use crate::{Attempt, Completion, Lease, Settings};

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<Inspect>::default(),
            IoPlugin::<Canonicalize>::default(),
            IoPlugin::<ReadDirectory>::default(),
            IoPlugin::<Capture>::default(),
            IoPlugin::<Remove>::default(),
            IoPlugin::<Rename>::default(),
        ))
        .configure_sets(
            Update,
            (Stage::Collect, Stage::Decide, Stage::Submit, Stage::Finish)
                .chain()
                .in_set(JobSet::Apply)
                .after(crate::Stage::Execute),
        )
        .add_observer(accept)
        .add_systems(PreUpdate, cancel)
        .add_systems(
            Update,
            (
                cancel, initialize, resolve, observe, timestamp, stores, sessions, logs, wake,
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

#[derive(EntityEvent)]
pub(crate) struct Requested {
    pub entity: Entity,
    pub context: Entity,
    pub attempt: u64,
}

#[derive(Component)]
#[require(Phase)]
struct Request {
    context: Entity,
    attempt: u64,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Select,
    Inspect,
    Canonical,
    Observe,
    Timestamp,
    Stores,
    Sessions,
    Logs,
    Done,
}

#[derive(Component)]
struct Development;

#[derive(Component)]
struct Suffix(String);

#[derive(Component)]
struct Observation(Entity);

#[derive(Component)]
struct Owner(Entity);

#[derive(Component)]
struct Io;

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Received;

#[derive(Component)]
struct Failed(Failure);

#[derive(Component)]
struct Complete;

#[derive(Component)]
struct Enumerated;

#[derive(Component)]
struct Store;

#[derive(Component)]
struct Session;

#[derive(Component)]
struct Candidate;

#[derive(Component, Clone, Copy)]
struct Purpose {
    kind: PurposeKind,
    entity: Option<Entity>,
}

impl Purpose {
    fn new(kind: PurposeKind) -> Self {
        Self { kind, entity: None }
    }

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

    fn is(&self, kind: PurposeKind) -> bool {
        self.kind == kind && self.entity.is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PurposeKind {
    Root,
    Store,
    Profiles,
    Session,
    Logs,
}

type Requests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, &'static mut Phase),
    (Without<Failed>, Without<Complete>),
>;
type Owners<'w, 's> = Query<'w, 's, (&'static Attempt, &'static ChildOf)>;
type Incoming<'w, 's> =
    Query<'w, 's, (Entity, &'static Request, &'static Phase), (Without<Cwd>, Without<Failed>)>;
type Replies<'w, 's, T> = Query<'w, 's, (Entity, &'static Owner, &'static Purpose, &'static T)>;
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
type Queued<'w, 's, J> =
    Query<'w, 's, (Entity, &'static Owner, &'static J), (With<Io>, Without<Submitted>)>;
type Pending<'w, 's> = Query<'w, 's, &'static Owner, (With<Io>, Without<Received>)>;
type PendingFiles<'w, 's> =
    Query<'w, 's, (Entity, &'static Owner, &'static Cwd), (With<Candidate>, Without<Submitted>)>;
type Closing<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        Option<&'static ChildOf>,
        Option<&'static Failed>,
    ),
    Or<(With<Failed>, With<Complete>)>,
>;

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

#[derive(bevy_ecs::system::SystemParam)]
struct Writer<'w, 's> {
    commands: Commands<'w, 's>,
}

impl Writer<'_, '_> {
    fn fail(&mut self, entity: Entity, failure: Failure) {
        self.commands.entity(entity).insert(Failed(failure));
    }

    fn request<J: StorageJob>(&mut self, owner: Entity, purpose: Purpose, job: J) {
        self.commands.spawn((
            Io,
            Owner(owner),
            purpose,
            job,
            Operation::bundle("dx.reset.io").unwrap(),
            ChildOf(owner),
        ));
    }
}

#[derive(bevy_ecs::system::SystemParam)]
struct Receipts<'w, 's> {
    metadata: Replies<'w, 's, PathMetadata>,
    done: Replies<'w, 's, Done>,
    entries: Replies<'w, 's, DirectoryEntries>,
    pending: Pending<'w, 's>,
}

type LayoutPaths<'w, 's> = Query<'w, 's, &'static Cwd>;

type LayoutSessions<'w, 's> = Query<'w, 's, &'static Owner, With<Session>>;

type LayoutSuffixes<'w, 's> = Query<'w, 's, &'static Suffix>;

type LayoutDevelopment<'w, 's> = Query<'w, 's, (), With<Development>>;

type LayoutEnumerated<'w, 's> = Query<'w, 's, (), With<Enumerated>>;

#[derive(bevy_ecs::system::SystemParam)]
struct Layout<'w, 's> {
    paths: LayoutPaths<'w, 's>,
    files: PendingFiles<'w, 's>,
    sessions: LayoutSessions<'w, 's>,
    suffixes: LayoutSuffixes<'w, 's>,
    development: LayoutDevelopment<'w, 's>,
    enumerated: LayoutEnumerated<'w, 's>,
}

type LifetimesRows<'w, 's> = Query<'w, 's, (Entity, &'static Owner)>;

type LifetimesFlights<'w, 's> = Query<'w, 's, (&'static Owner, &'static Flight)>;

type LifetimesObservations<'w, 's> = Query<'w, 's, &'static Observation>;

#[derive(bevy_ecs::system::SystemParam)]
struct Lifetimes<'w, 's> {
    rows: LifetimesRows<'w, 's>,
    flights: LifetimesFlights<'w, 's>,
    observations: LifetimesObservations<'w, 's>,
}

fn accept(event: On<Requested>, existing: Query<(), With<Request>>, mut commands: Commands) {
    if existing.contains(event.entity) {
        return;
    }
    commands.entity(event.entity).insert((
        Request {
            context: event.context,
            attempt: event.attempt,
        },
        Phase::Select,
    ));
}

fn initialize(incoming: Incoming, contexts: Query<&Settings>, mut commands: Commands) {
    for (entity, request, phase) in &incoming {
        if *phase != Phase::Select {
            continue;
        }
        let Ok(settings) = contexts.get(request.context) else {
            continue;
        };
        let development = settings
            .0
            .get("VMUX_BUILD_PROFILE")
            .unwrap_or(OsStr::new("dev"))
            == "dev";
        let base = if cfg!(target_os = "macos") {
            let home = match settings.0.required("HOME") {
                Ok(home) => PathBuf::from(home),
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::from_error(error)));
                    continue;
                }
            };
            home.join("Library/Application Support/Vmux")
        } else {
            std::env::temp_dir().join("Vmux")
        };
        let path = match settings.0.get("VMUX_DATA_DIR") {
            Some(path) => PathBuf::from(path),
            None if development => base.join("dev"),
            None => base,
        };
        if development {
            commands.entity(entity).insert(Development);
        }
        commands.entity(entity).insert(Cwd(path));
    }
}

fn collect<J: StorageJob>(results: Returned<J>, mut commands: Commands) {
    for (entity, owner, parent, job_owner, result) in &results {
        if parent.parent() != owner.0 || !job_owner.accepts(Some(parent)) {
            commands
                .entity(entity)
                .insert(OperationState::Canceled)
                .remove::<JobResult<Task<Scoped<J>>>>();
            continue;
        }
        match &result.0 {
            Ok(report) => match &report.result {
                Ok(value) => {
                    commands.entity(entity).insert((
                        value.clone(),
                        Received,
                        OperationState::Succeeded,
                    ));
                }
                Err(error) => {
                    commands.entity(owner.0).insert(Failed(error.clone()));
                }
            },
            Err(error) => {
                commands
                    .entity(owner.0)
                    .insert(Failed(Failure::message(error.clone())));
            }
        }
        commands
            .entity(entity)
            .remove::<JobResult<Task<Scoped<J>>>>();
    }
}

fn submit<J: StorageJob>(
    pending: Queued<J>,
    requests: Query<&Request, Without<Failed>>,
    loans: Query<&Lease>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, owner, job) in &pending {
        let Ok(request) = requests.get(owner.0) else {
            continue;
        };
        let loan = loans.get(request.context).ok().map(|lease| lease.0.clone());
        let (task, flight) = Task::new(Scoped(job.clone()), loan, Vec::new(), wake.0.clone());
        commands.entity(entity).insert((
            Submitted,
            JobRequest(task),
            flight,
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

fn resolve(
    mut requests: Requests,
    paths: Query<&Cwd>,
    metadata: Replies<PathMetadata>,
    canonical: Replies<CanonicalPath>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, _, mut phase) in &mut requests {
        let Ok(path) = paths.get(entity) else {
            continue;
        };
        for (receipt, owner, purpose, metadata) in &metadata {
            if owner.0 != entity || !purpose.is(PurposeKind::Root) || *phase != Phase::Inspect {
                continue;
            }
            if metadata
                .0
                .as_ref()
                .is_some_and(|value| value.is_dir() || value.is_symlink())
            {
                *phase = Phase::Canonical;
                io.request(
                    entity,
                    Purpose::new(PurposeKind::Root),
                    Canonicalize(path.0.clone()),
                );
            } else {
                io.commands.entity(entity).insert(Complete);
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, canonical) in &canonical {
            if owner.0 != entity || !purpose.is(PurposeKind::Root) || *phase != Phase::Canonical {
                continue;
            }
            io.commands.entity(entity).insert(Cwd(canonical.0.clone()));
            *phase = Phase::Observe;
            io.commands.entity(receipt).despawn();
        }
        if *phase == Phase::Select && !pending.iter().any(|owner| owner.0 == entity) {
            *phase = Phase::Inspect;
            io.request(
                entity,
                Purpose::new(PurposeKind::Root),
                Inspect(path.0.clone()),
            );
        }
    }
}

fn observe(
    mut requests: Requests,
    paths: Query<&Cwd>,
    observations: Query<&Observation>,
    facts: process_observation::Facts,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, request, mut phase) in &mut requests {
        if *phase != Phase::Observe {
            continue;
        }
        if let Ok(observation) = observations.get(entity) {
            if let Some(error) = facts.error(observation.0) {
                commands.entity(entity).insert(Failed(error.clone()));
            } else if facts.ready(observation.0) {
                let Ok(path) = paths.get(entity) else {
                    continue;
                };
                if facts.uses(observation.0, &path.0) {
                    commands.entity(entity).insert(Failed(Failure::message(
                        "Layout files are in use; quit that build before resetting",
                    )));
                } else {
                    *phase = Phase::Timestamp;
                }
            }
            continue;
        }
        let observation = commands
            .spawn((
                process_observation::Request {
                    context: request.context,
                    attempt: request.attempt,
                },
                Owner(entity),
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert(Observation(observation));
        wake.0.wake_by_ref();
    }
}

fn timestamp(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    results: Replies<ToolOutput>,
    suffixes: Query<&Suffix>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, request, mut phase) in &mut requests {
        if *phase != Phase::Timestamp {
            continue;
        }
        for (receipt, owner, _, output) in &results {
            if owner.0 != entity {
                continue;
            }
            if !output.0.status.success() {
                io.fail(entity, Failure::status(output.0.status));
            } else {
                match std::str::from_utf8(&output.0.stdout) {
                    Ok(value) if !value.trim().is_empty() && !value.contains('/') => {
                        io.commands
                            .entity(entity)
                            .insert(Suffix(value.trim().to_owned()));
                        *phase = Phase::Stores;
                    }
                    _ => {
                        io.fail(entity, Failure::message("Invalid layout backup timestamp"));
                    }
                }
            }
            io.commands.entity(receipt).despawn();
        }
        if suffixes.contains(entity)
            || pending.iter().any(|owner| owner.0 == entity)
            || *phase != Phase::Timestamp
        {
            continue;
        }
        let Ok((checkout, settings)) = contexts.get(request.context) else {
            continue;
        };
        io.request(
            entity,
            Purpose::new(PurposeKind::Root),
            Capture {
                environment: settings.0.clone(),
                directory: checkout.root.clone(),
                program: "date".into(),
                arguments: vec!["+%Y%m%d-%H%M%S".into()],
            },
        );
    }
}

fn stores(mut requests: Requests, layout: Layout, receipts: Receipts, mut io: Writer) {
    for (entity, _, mut phase) in &mut requests {
        if *phase != Phase::Stores {
            continue;
        }
        let Ok(path) = layout.paths.get(entity) else {
            continue;
        };
        if !layout.enumerated.contains(entity) {
            io.commands.entity(entity).insert(Enumerated);
            for (order, name) in ["store.ron", "store.version"].into_iter().enumerate() {
                let file = io
                    .commands
                    .spawn((
                        Store,
                        Candidate,
                        Owner(entity),
                        Cwd(path.0.join(name)),
                        Name::new(name),
                        Order(order as u32),
                        ChildOf(entity),
                    ))
                    .id();
                io.request(
                    entity,
                    Purpose::targeted(PurposeKind::Store, file),
                    Inspect(path.0.join(name)),
                );
            }
            continue;
        }
        for (receipt, owner, purpose, metadata) in &receipts.metadata {
            if owner.0 != entity {
                continue;
            }
            let Some(file) = purpose.target(PurposeKind::Store) else {
                continue;
            };
            let Ok(path) = layout.paths.get(*file) else {
                continue;
            };
            if metadata
                .0
                .as_ref()
                .is_some_and(|value| value.is_file() || value.is_symlink())
            {
                io.commands.entity(*file).insert(Submitted);
                if layout.development.contains(entity) {
                    io.request(
                        entity,
                        Purpose::targeted(PurposeKind::Store, *file),
                        Remove {
                            path: path.0.clone(),
                            directory: false,
                            lock: None,
                        },
                    );
                } else if let Ok(suffix) = layout.suffixes.get(entity) {
                    let name = path.0.file_name().unwrap().to_string_lossy();
                    io.request(
                        entity,
                        Purpose::targeted(PurposeKind::Store, *file),
                        Rename {
                            source: path.0.clone(),
                            destination: path
                                .0
                                .with_file_name(format!("{name}.cleanup-{}", suffix.0)),
                        },
                    );
                }
            } else {
                io.commands.entity(*file).despawn();
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, _) in &receipts.done {
            if owner.0 != entity {
                continue;
            }
            if let Some(file) = purpose.target(PurposeKind::Store) {
                io.commands.entity(*file).despawn();
                io.commands.entity(receipt).despawn();
            }
        }
        if !receipts.pending.iter().any(|owner| owner.0 == entity)
            && !layout.files.iter().any(|(_, owner, _)| owner.0 == entity)
        {
            *phase = Phase::Sessions;
            io.commands.entity(entity).remove::<Enumerated>();
        }
    }
}

fn sessions(mut requests: Requests, layout: Layout, receipts: Receipts, mut io: Writer) {
    for (entity, _, mut phase) in &mut requests {
        if *phase != Phase::Sessions {
            continue;
        }
        let Ok(path) = layout.paths.get(entity) else {
            continue;
        };
        if !layout.enumerated.contains(entity) {
            io.commands.entity(entity).insert(Enumerated);
            io.request(
                entity,
                Purpose::new(PurposeKind::Profiles),
                Inspect(path.0.join("profiles")),
            );
            continue;
        }
        let mut scheduled = false;
        for (receipt, owner, purpose, metadata) in &receipts.metadata {
            if owner.0 != entity {
                continue;
            }
            if purpose.is(PurposeKind::Profiles)
                && metadata.0.as_ref().is_some_and(|value| value.is_dir())
            {
                io.request(
                    entity,
                    Purpose::new(PurposeKind::Profiles),
                    ReadDirectory(path.0.join("profiles")),
                );
                scheduled = true;
            } else if let Some(file) = purpose.target(PurposeKind::Session) {
                let Ok(path) = layout.paths.get(*file) else {
                    continue;
                };
                if metadata
                    .0
                    .as_ref()
                    .is_some_and(|value| value.is_file() || value.is_symlink())
                {
                    io.request(
                        entity,
                        Purpose::targeted(PurposeKind::Session, *file),
                        Remove {
                            path: path.0.clone(),
                            directory: false,
                            lock: None,
                        },
                    );
                    scheduled = true;
                } else {
                    io.commands.entity(*file).despawn();
                }
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, entries) in &receipts.entries {
            if owner.0 != entity || !purpose.is(PurposeKind::Profiles) {
                continue;
            }
            for (path, metadata) in &entries.0 {
                if !metadata.is_dir() {
                    continue;
                }
                let path = path.join("session.ron");
                let file = io
                    .commands
                    .spawn((Session, Owner(entity), Cwd(path.clone()), ChildOf(entity)))
                    .id();
                io.request(
                    entity,
                    Purpose::targeted(PurposeKind::Session, file),
                    Inspect(path),
                );
                scheduled = true;
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, _) in &receipts.done {
            if owner.0 != entity {
                continue;
            }
            if let Some(file) = purpose.target(PurposeKind::Session) {
                io.commands.entity(*file).despawn();
                io.commands.entity(receipt).despawn();
            }
        }
        if !scheduled
            && !receipts.pending.iter().any(|owner| owner.0 == entity)
            && !layout.sessions.iter().any(|owner| owner.0 == entity)
        {
            *phase = Phase::Logs;
        }
    }
}

fn logs(
    mut requests: Requests,
    paths: Query<&Cwd>,
    metadata: Replies<PathMetadata>,
    done: Replies<Done>,
    development: Query<(), With<Development>>,
    pending: Pending,
    mut io: Writer,
) {
    for (entity, _, mut phase) in &mut requests {
        if *phase != Phase::Logs {
            continue;
        }
        if !development.contains(entity) {
            *phase = Phase::Done;
            continue;
        }
        let Ok(path) = paths.get(entity) else {
            continue;
        };
        let mut received = false;
        for (receipt, owner, purpose, metadata) in &metadata {
            if owner.0 != entity || !purpose.is(PurposeKind::Logs) {
                continue;
            }
            received = true;
            match &metadata.0 {
                Some(value) if value.is_dir() || value.is_symlink() => {
                    io.request(
                        entity,
                        Purpose::new(PurposeKind::Logs),
                        Remove {
                            path: path.0.join("logs"),
                            directory: value.is_dir(),
                            lock: None,
                        },
                    );
                }
                _ => *phase = Phase::Done,
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, _) in &done {
            if owner.0 == entity && purpose.is(PurposeKind::Logs) {
                received = true;
                *phase = Phase::Done;
                io.commands.entity(receipt).despawn();
            }
        }
        if !received && !pending.iter().any(|owner| owner.0 == entity) {
            io.request(
                entity,
                Purpose::new(PurposeKind::Logs),
                Inspect(path.0.join("logs")),
            );
        }
    }
}

fn cancel(
    requests: Query<(Entity, &Request, Option<&ChildOf>)>,
    owners: Owners,
    states: Query<&OperationState>,
    io: Query<(Entity, &Owner), With<Io>>,
    mut commands: Commands,
) {
    for (entity, request, parent) in &requests {
        let valid = owners.get(entity).is_ok_and(|(attempt, actual_parent)| {
            attempt.0 == request.attempt
                && actual_parent.parent() == request.context
                && parent.is_some_and(|parent| parent.parent() == request.context)
        }) && states
            .get(request.context)
            .is_ok_and(|state| *state == OperationState::Running);
        if valid && Signals::pending().is_none() {
            continue;
        }
        commands.entity(entity).insert(Failed(Failure::interrupted(
            Signals::pending().unwrap_or(15),
        )));
        for (child, owner) in &io {
            if owner.0 == entity {
                commands.entity(child).insert(OperationState::Canceled);
            }
        }
    }
}

fn wake(
    mut requests: Requests,
    pending: Query<(&Owner, &OperationState), With<Io>>,
    observations: Query<&Observation>,
    facts: process_observation::Facts,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, _, phase) in &mut requests {
        if *phase == Phase::Done {
            commands.entity(entity).insert(Complete);
            continue;
        }
        if observations
            .get(entity)
            .is_ok_and(|observation| !facts.drained(observation.0))
        {
            continue;
        }
        if !pending
            .iter()
            .any(|(owner, state)| owner.0 == entity && *state == OperationState::Running)
        {
            wake.0.wake_by_ref();
        }
    }
}

fn finish(
    requests: Closing,
    lifetimes: Lifetimes,
    facts: process_observation::Facts,
    paths: Query<&Cwd>,
    wake: Res<JobWake>,
    mut completions: MessageWriter<Completion>,
    mut commands: Commands,
) {
    for (entity, request, _parent, failed) in &requests {
        if lifetimes
            .flights
            .iter()
            .any(|(owner, flight)| owner.0 == entity && flight.pending())
        {
            continue;
        }
        if lifetimes
            .observations
            .get(entity)
            .is_ok_and(|observation| !facts.drained(observation.0))
        {
            continue;
        }
        let mut remaining = false;
        for (child, owner) in &lifetimes.rows {
            if owner.0 == entity {
                commands.entity(child).despawn();
                remaining = true;
            }
        }
        if remaining {
            commands.entity(entity).remove::<Observation>();
            wake.0.wake_by_ref();
            continue;
        }
        let result = if let Some(error) = failed {
            Err(error.0.clone())
        } else {
            if let Ok(path) = paths.get(entity) {
                println!("Layout reset: {}", path.0.display());
            }
            Ok(Outcome::Done)
        };
        completions.write(Completion {
            entity,
            context: request.context,
            attempt: request.attempt,
            result,
        });
        commands.entity(entity).remove::<Request>();
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    use vmux_ecs::cli::CliResult;

    struct Fixture {
        _directory: tempfile::TempDir,
        app: App,
        root: PathBuf,
        data: PathBuf,
        operation: Entity,
        step: Entity,
        _permit: crate::testing::Permit,
    }

    impl Fixture {
        fn new(profile: &str) -> Self {
            let permit = crate::testing::Permit::acquire();
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let data = root.join("data with spaces");
            fs::create_dir_all(root.join("bin")).unwrap();
            fs::create_dir(&data).unwrap();
            for (name, body) in [
                ("ps", "printf '12 vmux_desktop\\n'"),
                ("id", "printf '501\\n'"),
                ("lsof", "exec /bin/cat opened"),
                ("date", "printf '20261115-123456\\n'"),
            ] {
                let path = root.join("bin").join(name);
                fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            fs::write(root.join("opened"), "p12\nn/unrelated\n").unwrap();
            let mut environment = crate::environment::Environment::current();
            environment.set(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("bin").display()),
            );
            environment.set("VMUX_DATA_DIR", data.as_os_str());
            environment.set("VMUX_BUILD_PROFILE", profile);
            let policy = crate::policy::Policy::configured(&environment).unwrap();
            let mut app = App::new();
            app.add_plugins(crate::Plugin);
            let operation = app
                .world_mut()
                .spawn((
                    crate::Accepted,
                    Operation::bundle("dx.reset.fixture").unwrap().running(),
                    Checkout {
                        root: root.clone(),
                        target: root.join("target"),
                        build_target: None,
                    },
                    Settings(environment),
                    crate::storage::Storage::new(policy),
                ))
                .id();
            let step = app
                .world_mut()
                .spawn((
                    ChildOf(operation),
                    Attempt(1),
                    OperationState::Pending,
                    crate::work::Work::Reset,
                ))
                .id();
            Self {
                _permit: permit,
                _directory: directory,
                app,
                root,
                data,
                operation,
                step,
            }
        }

        fn write(&self, path: &str, value: &str) -> PathBuf {
            let path = self.data.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, value).unwrap();
            path
        }

        fn run(&mut self) -> u8 {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.app.update();
                if let Some(CliResult(Ok(code))) = self.app.world().get::<CliResult>(self.operation)
                {
                    return *code;
                }
                assert!(
                    Instant::now() < deadline,
                    "layout reset did not finish: phase {:?}, request {}, failed {}, step state {:?}",
                    self.app.world().get::<Phase>(self.step),
                    self.app.world().get::<Request>(self.step).is_some(),
                    self.app.world().get::<Failed>(self.step).is_some(),
                    self.app.world().get::<OperationState>(self.step)
                );
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn development_reset_removes_layout_and_logs_but_keeps_other_data_and_external_profiles() {
        let mut fixture = Fixture::new("dev");
        fixture.write("store.ron", "layout");
        fixture.write("store.version", "version");
        fixture.write("logs/application.log", "diagnostics");
        fixture.write("profiles/one/session.ron", "session");
        fixture.write("profiles/one/credentials", "secret");
        let outside = fixture.root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("session.ron"), "external session").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.data.join("profiles/external")).unwrap();
        assert_eq!(fixture.run(), 0);
        assert!(!fixture.data.join("store.ron").exists());
        assert!(!fixture.data.join("store.version").exists());
        assert!(!fixture.data.join("logs").exists());
        assert!(!fixture.data.join("profiles/one/session.ron").exists());
        assert_eq!(
            fs::read_to_string(fixture.data.join("profiles/one/credentials")).unwrap(),
            "secret"
        );
        assert_eq!(
            fs::read_to_string(outside.join("session.ron")).unwrap(),
            "external session"
        );
    }

    #[test]
    fn nondevelopment_reset_preserves_layout_backups_and_logs() {
        let mut fixture = Fixture::new("local");
        fixture.write("store.ron", "previous layout");
        fixture.write("store.version", "previous version");
        fixture.write("logs/application.log", "diagnostics");
        fixture.write("profiles/one/session.ron", "session");
        assert_eq!(fixture.run(), 0);
        assert_eq!(
            fs::read_to_string(fixture.data.join("store.ron.cleanup-20261115-123456")).unwrap(),
            "previous layout"
        );
        assert_eq!(
            fs::read_to_string(fixture.data.join("store.version.cleanup-20261115-123456")).unwrap(),
            "previous version"
        );
        assert_eq!(
            fs::read_to_string(fixture.data.join("logs/application.log")).unwrap(),
            "diagnostics"
        );
        assert!(!fixture.data.join("store.ron").exists());
        assert!(!fixture.data.join("profiles/one/session.ron").exists());
    }

    #[test]
    fn running_profile_refuses_reset_before_any_file_mutation() {
        let mut fixture = Fixture::new("dev");
        fixture.write("store.ron", "layout");
        fixture.write("logs/application.log", "diagnostics");
        fs::write(
            fixture.root.join("opened"),
            format!("p12\nn{}\n", fixture.data.display()),
        )
        .unwrap();
        assert_ne!(fixture.run(), 0);
        assert_eq!(
            fs::read_to_string(fixture.data.join("store.ron")).unwrap(),
            "layout"
        );
        assert_eq!(
            fs::read_to_string(fixture.data.join("logs/application.log")).unwrap(),
            "diagnostics"
        );
    }
}
