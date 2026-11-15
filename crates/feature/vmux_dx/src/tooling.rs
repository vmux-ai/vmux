use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::cli::CliResult;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Operation, OperationState};

use crate::debug_io::{Flight, Task};
use crate::execution::Failure;
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::storage::Storage;
use crate::storage_io::{Capture, Done, Inspect, PathMetadata, StorageJob, ToolOutput};
use crate::target_lock::TargetLock;
use crate::tooling_io::{Acquire, CreateDirectory, Fingerprint, Handoff, Lock, Scoped, SourceKey};
use crate::{Completion, Settings};

pub(crate) struct ToolingPlugin;

impl bevy_app::Plugin for ToolingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<Inspect>::default(),
            IoPlugin::<SourceKey>::default(),
            IoPlugin::<CreateDirectory>::default(),
            IoPlugin::<Acquire>::default(),
            IoPlugin::<Capture>::default(),
            IoPlugin::<Handoff>::default(),
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
                cancel,
                guard,
                (
                    observe_input,
                    observe_compiler,
                    prepare_input,
                    check,
                    compile,
                    handoff,
                )
                    .chain(),
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
pub(crate) struct Waiting;

#[derive(Component)]
pub(crate) struct Checked;

#[derive(EntityEvent)]
pub(crate) struct Requested {
    pub entity: Entity,
    pub checkout: Checkout,
    pub environment: crate::environment::Environment,
}

#[derive(Component)]
#[require(Phase)]
struct Bootstrap;

#[derive(Component)]
struct Caller(Entity);

#[derive(Component)]
struct Arguments(Vec<OsString>);

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Manifest,
    Source,
    Cache,
    Create,
    Guard,
    Acquire,
    Recheck,
    Compile,
    Binary,
    Release,
    Handoff,
}

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
struct Guard;

struct IoPlugin<J: StorageJob>(std::marker::PhantomData<fn() -> J>);

impl<J: StorageJob> Default for IoPlugin<J> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<J: StorageJob> bevy_app::Plugin for IoPlugin<J> {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<Scoped<J>>>::default())
            .add_systems(Update, receive::<J>.in_set(Stage::Collect))
            .add_systems(Update, submit::<J>.in_set(Stage::Submit));
    }
}

type Pending<'w, 's> = Query<'w, 's, &'static Owner, (With<Io>, Without<Received>)>;

type Guards<'w, 's> = Query<'w, 's, &'static Owner, With<Guard>>;

type Loans<'w, 's> = Query<'w, 's, &'static Lock, With<Bootstrap>>;

type Flights<'w, 's> = Query<'w, 's, (&'static Owner, &'static Flight)>;

#[derive(SystemParam)]
struct Scope<'w, 's> {
    pending: Pending<'w, 's>,
    guards: Guards<'w, 's>,
    loans: Loans<'w, 's>,
    flights: Flights<'w, 's>,
}

type ReceiveResults<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobResult<Task<Scoped<J>>>,
        &'static JobOwner<Task<Scoped<J>>>,
        &'static Owner,
        &'static ChildOf,
    ),
    Without<Received>,
>;

type ReceiveRequests<'w, 's> =
    Query<'w, 's, &'static OperationState, (With<Bootstrap>, Without<Failed>)>;

type SubmitPending<'w, 's, J> =
    Query<'w, 's, (Entity, &'static J, &'static Owner), (With<Io>, Without<Submitted>)>;

type SubmitRequests<'w, 's> = Query<
    'w,
    's,
    (Option<&'static Lock>, &'static OperationState),
    (With<Bootstrap>, Without<Failed>),
>;

type AdvanceRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Checkout,
        &'static Settings,
        &'static Arguments,
        &'static mut Phase,
    ),
    (With<Bootstrap>, Without<Failed>, Without<Complete>),
>;

type AdvanceResults<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Owner,
        Option<&'static PathMetadata>,
        Option<&'static Fingerprint>,
        Option<&'static Done>,
        Option<&'static Lock>,
        Option<&'static ToolOutput>,
    ),
    With<Received>,
>;

type FinishRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Caller,
        Option<&'static Failed>,
        Option<&'static Complete>,
    ),
    With<Bootstrap>,
>;
fn accept(event: On<Requested>, existing: Query<(), With<Waiting>>, mut commands: Commands) {
    if existing.contains(event.entity) {
        return;
    }
    let policy = match crate::policy::Policy::configured(&event.environment) {
        Ok(policy) => policy,
        Err(error) => {
            commands
                .entity(event.entity)
                .insert(CliResult(Err(error.to_string())));
            return;
        }
    };
    let mut checkout = event.checkout.clone();
    checkout.target = checkout.root.join(".build-tools/cargo");
    checkout.build_target = None;
    let root = commands
        .spawn((
            Bootstrap,
            Caller(event.entity),
            Phase::Manifest,
            Arguments(std::env::args_os().skip(1).collect()),
            Operation::bundle("dx.bootstrap").unwrap().running(),
            Settings(event.environment.clone()),
            Storage::new(policy),
            checkout.clone(),
            ChildOf(event.entity),
        ))
        .id();
    commands.spawn((
        Io,
        Owner(root),
        Inspect(checkout.root.join("crates/feature/vmux_dx/Cargo.toml")),
        Operation::bundle("dx.bootstrap.io").unwrap(),
        ChildOf(root),
    ));
    commands.entity(event.entity).insert(Waiting);
}

fn receive<J: StorageJob>(
    results: ReceiveResults<J>,
    requests: ReceiveRequests,
    callers: Query<(&Caller, Option<&ChildOf>)>,
    mut commands: Commands,
) {
    for (entity, result, job_owner, owner, parent) in &results {
        let valid = callers
            .get(owner.0)
            .is_ok_and(|(caller, parent)| parent.is_some_and(|parent| parent.parent() == caller.0))
            && parent.parent() == owner.0
            && job_owner.accepts(Some(parent))
            && requests
                .get(owner.0)
                .is_ok_and(|state| *state == OperationState::Running);
        if !valid {
            commands
                .entity(entity)
                .remove::<JobResult<Task<Scoped<J>>>>();
            continue;
        }
        let result = match &result.0 {
            Ok(report) => report.result.clone(),
            Err(error) => Err(Failure::message(error.clone())),
        };
        match result {
            Ok(value) => {
                commands
                    .entity(entity)
                    .insert((value, Received, OperationState::Succeeded));
            }
            Err(error) => {
                commands.entity(owner.0).insert(Failed(error));
            }
        }
        commands
            .entity(entity)
            .remove::<JobResult<Task<Scoped<J>>>>();
    }
}

fn submit<J: StorageJob>(
    pending: SubmitPending<J>,
    requests: SubmitRequests,
    callers: Query<(&Caller, Option<&ChildOf>)>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, job, owner) in &pending {
        let Ok((lock, state)) = requests.get(owner.0) else {
            continue;
        };
        if *state != OperationState::Running
            || !callers.get(owner.0).is_ok_and(|(caller, parent)| {
                parent.is_some_and(|parent| parent.parent() == caller.0)
            })
        {
            continue;
        }
        let loan: Option<Arc<TargetLock>> = lock.map(|lock| lock.0.clone());
        let (task, flight) = Task::new(Scoped(job.clone()), loan, Vec::new(), wake.0.clone());
        commands.entity(entity).insert((
            Submitted,
            flight,
            JobRequest(task),
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

fn guard(
    mut completions: MessageReader<Completion>,
    guards: Query<&Owner, With<Guard>>,
    mut requests: Query<&mut Phase, With<Bootstrap>>,
    mut commands: Commands,
) {
    for completion in completions.read() {
        let Ok(owner) = guards.get(completion.entity) else {
            continue;
        };
        if completion.attempt != 1 || completion.context != owner.0 {
            continue;
        }
        let Ok(mut phase) = requests.get_mut(owner.0) else {
            continue;
        };
        match &completion.result {
            Ok(_) => {
                *phase = if *phase == Phase::Guard {
                    Phase::Acquire
                } else {
                    Phase::Compile
                }
            }
            Err(error) => {
                commands.entity(owner.0).insert(Failed(error.clone()));
            }
        }
        commands.entity(completion.entity).despawn();
    }
}

fn observe_input(
    mut requests: AdvanceRequests,
    scope: Scope,
    results: AdvanceResults,
    mut commands: Commands,
) {
    for (entity, _checkout, settings, _arguments, mut phase) in &mut requests {
        if scope.pending.iter().any(|owner| owner.0 == entity)
            || scope.guards.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        let mut _settled = false;
        for (receipt, owner, metadata, fingerprint, done, _lock, _output) in &results {
            if owner.0 != entity {
                continue;
            }
            match *phase {
                Phase::Manifest => {
                    if metadata.is_some_and(|value| {
                        value.0.as_ref().is_some_and(|metadata| metadata.is_file())
                    }) {
                        *phase = Phase::Source;
                    } else {
                        commands.entity(entity).insert(Complete);
                        _settled = true;
                    }
                }
                Phase::Source => {
                    let Some(fingerprint) = fingerprint else {
                        continue;
                    };
                    if fingerprint.0 == env!("VMUX_DX_SOURCE_KEY") {
                        commands.entity(entity).insert(Complete);
                        _settled = true;
                    } else if settings.0.present("VMUX_DX_HANDOFF") {
                        commands.entity(entity).insert(Failed(Failure::message("The refreshed CLI still differs from its checkout; compiler inputs changed during bootstrap")));
                        _settled = true;
                    } else {
                        *phase = Phase::Cache;
                    }
                }
                Phase::Cache => {
                    let Some(metadata) = metadata else {
                        continue;
                    };
                    if metadata
                        .0
                        .as_ref()
                        .is_some_and(|metadata| metadata.is_symlink())
                    {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "Development tool cache must not be a symlink",
                        )));
                        _settled = true;
                    } else {
                        *phase = Phase::Create;
                    }
                }
                Phase::Create if done.is_some() => *phase = Phase::Guard,
                _ => continue,
            }

            commands.entity(receipt).despawn();
        }
    }
}

fn observe_compiler(
    mut requests: AdvanceRequests,
    scope: Scope,
    results: AdvanceResults,
    mut commands: Commands,
) {
    for (entity, _checkout, _settings, _arguments, mut phase) in &mut requests {
        if scope.pending.iter().any(|owner| owner.0 == entity)
            || scope.guards.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        let mut _settled = false;
        for (receipt, owner, metadata, _fingerprint, _done, lock, output) in &results {
            if owner.0 != entity {
                continue;
            }
            match *phase {
                Phase::Acquire => {
                    let Some(lock) = lock else {
                        continue;
                    };
                    commands.entity(entity).insert(lock.clone());
                    *phase = Phase::Recheck;
                }
                Phase::Compile => {
                    let Some(output) = output else {
                        continue;
                    };
                    if output.0.status.success() {
                        *phase = Phase::Binary;
                    } else {
                        let mut error = Failure::status(output.0.status);
                        if !output.0.stderr.is_empty() {
                            error = error.context(String::from_utf8_lossy(&output.0.stderr).trim());
                        }
                        commands.entity(entity).insert(Failed(error));
                        _settled = true;
                    }
                }
                Phase::Binary => {
                    if metadata.is_some_and(|value| {
                        value.0.as_ref().is_some_and(|metadata| metadata.is_file())
                    }) {
                        *phase = Phase::Release;
                    } else {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "Development CLI build produced no executable",
                        )));
                        _settled = true;
                    }
                }
                _ => continue,
            }

            commands.entity(receipt).despawn();
        }
    }
}

fn prepare_input(mut requests: AdvanceRequests, scope: Scope, mut commands: Commands) {
    for (entity, checkout, _settings, _arguments, phase) in &mut requests {
        if scope.pending.iter().any(|owner| owner.0 == entity)
            || scope.guards.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        match *phase {
            Phase::Source => {
                commands.spawn((
                    Io,
                    Owner(entity),
                    SourceKey(checkout.root.clone()),
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            Phase::Cache => {
                commands.spawn((
                    Io,
                    Owner(entity),
                    Inspect(checkout.root.join(".build-tools")),
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            Phase::Create => {
                commands.spawn((
                    Io,
                    Owner(entity),
                    CreateDirectory(checkout.root.join(".build-tools")),
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            Phase::Acquire => {
                commands.spawn((
                    Io,
                    Owner(entity),
                    Acquire {
                        root: checkout.root.clone(),
                        target: checkout.target.clone(),
                    },
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            _ => continue,
        }
    }
}

fn check(mut requests: AdvanceRequests, scope: Scope, wake: Res<JobWake>, mut commands: Commands) {
    for (entity, _checkout, _settings, _arguments, phase) in &mut requests {
        if scope.pending.iter().any(|owner| owner.0 == entity)
            || scope.guards.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        match *phase {
            Phase::Guard | Phase::Recheck => {
                let loan = scope.loans.get(entity).ok().map(|lock| lock.0.clone());
                commands.spawn((
                    Guard,
                    Owner(entity),
                    ChildOf(entity),
                    crate::Attempt(1),
                    crate::Running,
                    crate::Started {
                        context: entity,
                        attempt: 1,
                    },
                    OperationState::Running,
                    crate::storage_runtime::Request::new(
                        crate::storage_runtime::RequestKind::Guard,
                    ),
                    crate::storage_runtime::Captured {
                        root: entity,
                        attempt: 1,
                    },
                    crate::storage_runtime::Loan(loan),
                ));
                wake.0.wake_by_ref();
            }
            _ => continue,
        }
    }
}

fn compile(mut requests: AdvanceRequests, scope: Scope, mut commands: Commands) {
    for (entity, checkout, settings, _arguments, phase) in &mut requests {
        if scope.pending.iter().any(|owner| owner.0 == entity)
            || scope.guards.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        match *phase {
            Phase::Compile => {
                let mut environment = settings.0.clone();
                environment.remove("CARGO_BUILD_TARGET");
                environment.remove("RUSTFLAGS");
                environment.remove("CARGO_ENCODED_RUSTFLAGS");
                let program = environment
                    .get("CARGO_BIN")
                    .unwrap_or(OsStr::new("cargo"))
                    .to_owned();
                let mut argv = Vec::new();
                for value in [
                    "build",
                    "--locked",
                    "-p",
                    "vmux_cli",
                    "--no-default-features",
                    "--features",
                    "dx,ci-syntax",
                    "--target-dir",
                ] {
                    argv.push(OsString::from(value));
                }
                argv.push(checkout.target.clone().into_os_string());
                eprintln!("Updating the development CLI for the selected checkout");
                commands.spawn((
                    Io,
                    Owner(entity),
                    Capture {
                        environment,
                        directory: checkout.root.clone(),
                        program,
                        arguments: argv,
                    },
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            Phase::Binary => {
                commands.spawn((
                    Io,
                    Owner(entity),
                    Inspect(checkout.target.join("debug/vmux")),
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            _ => continue,
        }
    }
}

fn handoff(
    mut requests: AdvanceRequests,
    scope: Scope,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, checkout, _settings, arguments, mut phase) in &mut requests {
        if scope.pending.iter().any(|owner| owner.0 == entity)
            || scope.guards.iter().any(|owner| owner.0 == entity)
        {
            continue;
        }
        match *phase {
            Phase::Release => {
                if scope
                    .flights
                    .iter()
                    .any(|(owner, flight)| owner.0 == entity && flight.pending())
                {
                    continue;
                }
                commands.entity(entity).remove::<Lock>();
                *phase = Phase::Handoff;
                wake.0.wake_by_ref();
            }
            Phase::Handoff => {
                commands.spawn((
                    Io,
                    Owner(entity),
                    Handoff {
                        binary: checkout.target.join("debug/vmux"),
                        arguments: arguments.0.clone(),
                    },
                    Operation::bundle("dx.bootstrap.io").unwrap(),
                    ChildOf(entity),
                ));
            }
            _ => continue,
        }
    }
}

fn cancel(
    requests: Query<(Entity, &Caller, Option<&ChildOf>), With<Bootstrap>>,
    invocations: Query<(), With<Waiting>>,
    children: Query<(Entity, &Owner)>,
    mut commands: Commands,
) {
    for (entity, caller, parent) in &requests {
        if Signals::pending().is_none()
            && parent.is_some_and(|parent| parent.parent() == caller.0)
            && invocations.contains(caller.0)
        {
            continue;
        };
        let signal = Signals::pending().unwrap_or(15);
        commands.entity(entity).insert((
            Failed(Failure::interrupted(signal)),
            OperationState::Canceled,
        ));
        for (child, owner) in &children {
            if owner.0 == entity {
                commands.entity(child).insert(OperationState::Canceled);
            }
        }
    }
}

fn finish(
    requests: FinishRequests,
    flights: Query<(&Owner, &Flight)>,
    children: Query<(Entity, &Owner)>,
    guards: Query<(), With<Guard>>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, caller, failed, complete) in &requests {
        if failed.is_none() && complete.is_none() {
            continue;
        }
        if flights
            .iter()
            .any(|(owner, flight)| owner.0 == entity && flight.pending())
        {
            continue;
        }
        if children
            .iter()
            .any(|(child, owner)| owner.0 == entity && guards.contains(child))
        {
            continue;
        }
        let mut child_exists = false;
        for (child, owner) in &children {
            if owner.0 == entity {
                commands.entity(child).despawn();
                child_exists = true;
            }
        }
        if child_exists {
            wake.0.wake_by_ref();
            continue;
        }
        if commands.get_entity(caller.0).is_err() {
            commands.entity(entity).despawn();
            wake.0.wake_by_ref();
            continue;
        };
        match failed {
            Some(failed) => {
                commands
                    .entity(caller.0)
                    .insert(CliResult(Ok(failed.0.code() as u8)));
                eprintln!("vmux: {}", failed.0);
            }
            None => {
                commands.entity(caller.0).insert(Checked);
            }
        }
        commands.entity(caller.0).remove::<Waiting>();
        commands.entity(entity).despawn();
        wake.0.wake_by_ref();
    }
}
