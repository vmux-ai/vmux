use std::ffi::OsString;
use std::path::{Path, PathBuf};

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Cwd, Operation, OperationState, Ready};

use crate::debug_io::{Flight, Task};
use crate::environment::Environment;
use crate::execution::Failure;
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::storage_io::{Capture, ToolOutput};
use crate::{Attempt, Lease, Settings};

pub(crate) struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<Read>>::default())
            .configure_sets(
                Update,
                Stage.in_set(JobSet::Apply).after(crate::Stage::Execute),
            )
            .add_systems(PreUpdate, cancel)
            .add_systems(
                Update,
                (
                    initialize,
                    collect,
                    cancel,
                    (parse, parse_opened, inspect).chain(),
                    submit,
                    drain,
                    wake,
                )
                    .chain()
                    .in_set(Stage),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Stage;

#[derive(Component)]
#[require(Phase)]
pub(crate) struct Request {
    pub context: Entity,
    pub attempt: u64,
}

#[derive(SystemParam)]
struct Scope<'w, 's> {
    owners: Owners<'w, 's>,
    states: Query<'w, 's, &'static OperationState>,
}

impl Scope<'_, '_> {
    fn valid(&self, request: &Request, parent: &ChildOf) -> bool {
        let Ok((attempt, context)) = self.owners.get(parent.parent()) else {
            return false;
        };
        attempt.0 == request.attempt
            && context.parent() == request.context
            && self
                .states
                .get(request.context)
                .is_ok_and(|state| *state == OperationState::Running)
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Processes,
    User,
    Opened,
}

#[derive(Component)]
pub(crate) struct Drained;

#[derive(Component)]
struct Failed(Failure);

#[derive(Component)]
struct Owner(Entity);

#[derive(Component)]
struct Pid(u32);

#[derive(Component)]
struct Opened;

#[derive(Component)]
struct User(String);

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Received;

#[derive(Component, Clone)]
#[component(immutable)]
struct Read {
    environment: Environment,
    directory: PathBuf,
    program: OsString,
    arguments: Vec<OsString>,
}

impl vmux_ecs::jobs::Job for Read {
    type Output = Result<ToolOutput, Failure>;

    fn lane(&self) -> vmux_ecs::blocking::Lane {
        vmux_ecs::blocking::Lane::Process
    }

    fn run(self) -> Self::Output {
        vmux_ecs::jobs::Job::run(Capture {
            environment: self.environment,
            directory: self.directory,
            program: self.program,
            arguments: self.arguments,
        })
    }
}

type Owners<'w, 's> = Query<'w, 's, (&'static Attempt, &'static ChildOf)>;
type Incoming<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, &'static ChildOf),
    (Without<OperationState>, Without<Failed>),
>;
type Requests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static ChildOf,
        &'static mut Phase,
    ),
    (Without<Failed>, Without<Ready>),
>;
type Results<'w, 's> = Query<'w, 's, (Entity, &'static Owner, &'static ToolOutput), With<Received>>;
type Returned<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Owner,
        &'static ChildOf,
        &'static JobOwner<Task<Read>>,
        &'static JobResult<Task<Read>>,
    ),
    Without<Received>,
>;
type Pending<'w, 's> =
    Query<'w, 's, (Entity, &'static Owner, &'static Read, &'static ChildOf), Without<Submitted>>;
type Active<'w, 's> = Query<'w, 's, (Entity, &'static Request, &'static ChildOf), Without<Drained>>;

type ReadyRequests<'w, 's> =
    Query<'w, 's, (), (With<Request>, With<Ready>, With<Drained>, Without<Failed>)>;

#[derive(SystemParam)]
pub(crate) struct Facts<'w, 's> {
    ready: ReadyRequests<'w, 's>,
    errors: Query<'w, 's, &'static Failed>,
    drained: Query<'w, 's, (), With<Drained>>,
    commands: Query<'w, 's, (&'static Owner, &'static Name), With<Pid>>,
    paths: Query<'w, 's, (&'static Owner, &'static Cwd), With<Opened>>,
}

impl Facts<'_, '_> {
    pub(crate) fn ready(&self, entity: Entity) -> bool {
        self.ready.contains(entity)
    }

    pub(crate) fn error(&self, entity: Entity) -> Option<&Failure> {
        self.errors.get(entity).ok().map(|error| &error.0)
    }

    pub(crate) fn drained(&self, entity: Entity) -> bool {
        self.drained.contains(entity)
    }

    pub(crate) fn uses(&self, entity: Entity, path: &Path) -> bool {
        let text = path.to_string_lossy();
        for (owner, command) in &self.commands {
            if owner.0 == entity && command.as_str().contains(text.as_ref()) {
                return true;
            }
        }
        for (owner, opened) in &self.paths {
            if owner.0 == entity && opened.0.starts_with(path) {
                return true;
            }
        }
        false
    }
}

type DrainRequests<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<Request>,
        Without<Drained>,
        Or<(With<Failed>, With<Ready>)>,
    ),
>;

type WakeRequests<'w, 's> = Query<'w, 's, Entity, (With<Request>, Without<Failed>, Without<Ready>)>;

fn initialize(incoming: Incoming, scope: Scope, mut commands: Commands) {
    for (entity, request, parent) in &incoming {
        if !scope.valid(request, parent) {
            commands.entity(entity).insert((
                Phase::Processes,
                Failed(Failure::message("Process observation owner changed")),
            ));
            continue;
        }
        commands.entity(entity).insert((
            Phase::Processes,
            Operation::bundle("dx.processes").unwrap().running(),
        ));
    }
}

fn collect(results: Returned, mut commands: Commands) {
    for (entity, owner, parent, job_owner, result) in &results {
        if parent.parent() != owner.0 || !job_owner.accepts(Some(parent)) {
            commands
                .entity(entity)
                .insert(OperationState::Canceled)
                .remove::<JobResult<Task<Read>>>();
            continue;
        }
        match &result.0 {
            Ok(report) => match &report.result {
                Ok(output) => {
                    commands.entity(entity).insert((
                        output.clone(),
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
        commands.entity(entity).remove::<JobResult<Task<Read>>>();
    }
}

fn parse(
    mut requests: Requests,
    results: Results,
    contexts: Query<(&Checkout, &Settings)>,
    mut commands: Commands,
) {
    for (entity, request, _, mut phase) in &mut requests {
        let Ok((_checkout, _settings)) = contexts.get(request.context) else {
            continue;
        };
        let mut _received = false;
        for (receipt, owner, output) in &results {
            if owner.0 != entity {
                continue;
            }
            _received = true;
            let valid = if *phase == Phase::Opened {
                matches!(output.0.status.code(), Some(0 | 1)) && output.0.stderr.is_empty()
            } else {
                output.0.status.success()
            };
            if !valid {
                let mut error = Failure::status(output.0.status);
                if !output.0.stderr.is_empty() {
                    error = error.context(String::from_utf8_lossy(&output.0.stderr).trim());
                }
                commands.entity(entity).insert(Failed(error));
                commands.entity(receipt).despawn();
                continue;
            }
            let text = match std::str::from_utf8(&output.0.stdout) {
                Ok(text) => text,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message(error.to_string())));
                    commands.entity(receipt).despawn();
                    continue;
                }
            };
            match *phase {
                Phase::Processes => {
                    for line in text.lines() {
                        let Some((pid, command)) = line.trim().split_once(char::is_whitespace)
                        else {
                            continue;
                        };
                        let pid = match pid.parse::<u32>() {
                            Ok(pid) => pid,
                            Err(error) => {
                                commands
                                    .entity(entity)
                                    .insert(Failed(Failure::message(error.to_string())));
                                break;
                            }
                        };
                        if pid != std::process::id() {
                            commands.spawn((
                                Pid(pid),
                                Name::new(command.trim().to_owned()),
                                Owner(entity),
                                ChildOf(entity),
                            ));
                        }
                    }
                    *phase = Phase::User;
                }
                Phase::User => {
                    let uid = text.trim();
                    if uid.is_empty() || !uid.bytes().all(|byte| byte.is_ascii_digit()) {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "Cannot determine the current user",
                        )));
                    } else {
                        commands.entity(entity).insert(User(uid.to_owned()));
                        *phase = Phase::Opened;
                    }
                }
                _ => continue,
            }

            commands.entity(receipt).despawn();
        }
    }
}

fn parse_opened(
    mut requests: Requests,
    results: Results,
    existing: Query<(Entity, &Owner, &Pid)>,
    contexts: Query<(&Checkout, &Settings)>,
    mut commands: Commands,
) {
    for (entity, request, _, phase) in &mut requests {
        let Ok((_checkout, _settings)) = contexts.get(request.context) else {
            continue;
        };
        let mut _received = false;
        for (receipt, owner, output) in &results {
            if owner.0 != entity {
                continue;
            }
            _received = true;
            let valid = if *phase == Phase::Opened {
                matches!(output.0.status.code(), Some(0 | 1)) && output.0.stderr.is_empty()
            } else {
                output.0.status.success()
            };
            if !valid {
                let mut error = Failure::status(output.0.status);
                if !output.0.stderr.is_empty() {
                    error = error.context(String::from_utf8_lossy(&output.0.stderr).trim());
                }
                commands.entity(entity).insert(Failed(error));
                commands.entity(receipt).despawn();
                continue;
            }
            let text = match std::str::from_utf8(&output.0.stdout) {
                Ok(text) => text,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message(error.to_string())));
                    commands.entity(receipt).despawn();
                    continue;
                }
            };
            match *phase {
                Phase::Opened => {
                    let mut by_pid = std::collections::HashMap::new();
                    for (candidate, owner, value) in &existing {
                        if owner.0 == entity {
                            by_pid.insert(value.0, candidate);
                        }
                    }
                    let mut process = None;
                    let mut count = 0;
                    for line in text.lines() {
                        if let Some(value) = line.strip_prefix('p') {
                            process = None;
                            let pid = match value.parse::<u32>() {
                                Ok(pid) => pid,
                                Err(error) => {
                                    commands
                                        .entity(entity)
                                        .insert(Failed(Failure::message(error.to_string())));
                                    break;
                                }
                            };
                            if pid == std::process::id() {
                                continue;
                            }
                            process = by_pid.get(&pid).copied();
                            if process.is_none() {
                                process = Some(
                                    commands
                                        .spawn((
                                            Pid(pid),
                                            Name::new("Opened artifact owner"),
                                            Owner(entity),
                                            ChildOf(entity),
                                        ))
                                        .id(),
                                );
                                by_pid.insert(pid, process.unwrap());
                            }
                        } else if let Some(path) = line.strip_prefix('n')
                            && let Some(process) = process
                        {
                            count += 1;
                            commands.spawn((
                                Opened,
                                Cwd(PathBuf::from(path)),
                                Owner(entity),
                                ChildOf(process),
                            ));
                        }
                    }
                    if count == 0 {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "Cannot inspect running artifacts: lsof returned no open paths",
                        )));
                    } else {
                        commands.entity(entity).insert(Ready);
                    }
                }
                _ => continue,
            }

            commands.entity(receipt).despawn();
        }
    }
}

fn inspect(
    mut requests: Requests,
    contexts: Query<(&Checkout, &Settings)>,
    users: Query<&User>,
    pending: Query<&Owner, With<Read>>,
    mut commands: Commands,
) {
    for (entity, request, _, phase) in &mut requests {
        let Ok((checkout, settings)) = contexts.get(request.context) else {
            continue;
        };

        if pending.iter().any(|owner| owner.0 == entity) {
            continue;
        }
        let (program, arguments) = match *phase {
            Phase::Processes => ("ps", vec!["-axo".into(), "pid=,args=".into()]),
            Phase::User => ("id", vec!["-u".into()]),
            Phase::Opened => {
                let Ok(user) = users.get(entity) else {
                    continue;
                };
                (
                    "lsof",
                    vec![
                        "-n".into(),
                        "-P".into(),
                        "-a".into(),
                        "-u".into(),
                        user.0.clone().into(),
                        "-d".into(),
                        "txt,cwd".into(),
                        "-Fpn".into(),
                    ],
                )
            }
        };
        commands.spawn((
            Owner(entity),
            ChildOf(entity),
            Operation::bundle("dx.processes.io").unwrap(),
            Read {
                environment: settings.0.clone(),
                directory: checkout.root.clone(),
                program: program.into(),
                arguments,
            },
        ));
    }
}

fn submit(
    pending: Pending,
    requests: Query<&Request, Without<Failed>>,
    loans: Query<&Lease>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, owner, read, parent) in &pending {
        if parent.parent() != owner.0 {
            continue;
        }
        let Ok(request) = requests.get(owner.0) else {
            continue;
        };
        let loan = loans.get(request.context).ok().map(|loan| loan.0.clone());
        let (task, flight) = Task::new(read.clone(), loan, Vec::new(), wake.0.clone());
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
    scope: Scope,
    children: Query<(Entity, &Owner), With<Read>>,
    mut commands: Commands,
) {
    for (entity, request, parent) in &requests {
        if Signals::pending().is_none() && scope.valid(request, parent) {
            continue;
        }
        commands.entity(entity).insert(Failed(Failure::interrupted(
            Signals::pending().unwrap_or(15),
        )));
        for (child, owner) in &children {
            if owner.0 == entity {
                commands.entity(child).insert(OperationState::Canceled);
            }
        }
    }
}

fn drain(
    requests: DrainRequests,
    flights: Query<(&Owner, &Flight)>,
    io: Query<(Entity, &Owner), With<Read>>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for entity in &requests {
        if flights
            .iter()
            .any(|(owner, flight)| owner.0 == entity && flight.pending())
        {
            continue;
        }
        for (child, owner) in &io {
            if owner.0 == entity {
                commands.entity(child).despawn();
            }
        }
        commands.entity(entity).insert(Drained);
        wake.0.wake_by_ref();
    }
}

fn wake(
    requests: WakeRequests,
    io: Query<(&Owner, &OperationState), With<Read>>,
    wake: Res<JobWake>,
) {
    for entity in &requests {
        if !io
            .iter()
            .any(|(owner, state)| owner.0 == entity && *state == OperationState::Running)
        {
            wake.0.wake_by_ref();
        }
    }
}
