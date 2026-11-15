use std::ffi::{OsStr, OsString};

use base64::{Engine, engine::general_purpose::STANDARD};
use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Cwd, Operation, OperationState, Order, Ready};

use crate::cli_package_io::{Bytes, CreateTemporary, Directory, ReadArchive, Scoped, WriteArchive};
use crate::debug_io::{Flight, Task};
use crate::execution::Failure;
use crate::publication;
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::storage_io::{Capture, Done, FileBytes, ReadFile, StorageJob, ToolOutput};
use crate::tooling_io::CreateDirectory;
use crate::work::Outcome;
use crate::{Attempt, Completion, Lease, Settings};

pub(crate) struct CliPackagePlugin;

impl bevy_app::Plugin for CliPackagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<Capture>::default(),
            IoPlugin::<ReadFile>::default(),
            IoPlugin::<CreateDirectory>::default(),
            IoPlugin::<CreateTemporary>::default(),
            IoPlugin::<WriteArchive>::default(),
            IoPlugin::<ReadArchive>::default(),
        ))
        .configure_sets(
            Update,
            (Stage::Collect, Stage::Decide, Stage::Submit, Stage::Finish)
                .chain()
                .in_set(JobSet::Apply)
                .after(crate::Stage::Execute),
        )
        .add_observer(accept::<Compile>)
        .add_observer(accept::<Package>)
        .add_systems(PreUpdate, cancel)
        .add_systems(
            Update,
            (
                cancel,
                compile,
                (prepare, create_staging).chain(),
                (archive, sign_archive, sign_binary).chain(),
                verify,
                publish,
                forget,
                wake,
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
pub(crate) struct Requested<T: Component> {
    pub entity: Entity,
    pub context: Entity,
    pub attempt: u64,
    pub input: T,
}

#[derive(Component, Clone)]
pub(crate) struct Compile;

#[derive(Component, Clone)]
pub(crate) struct Package {
    pub preview: bool,
}

#[derive(Component)]
#[require(Phase)]
struct Scope {
    context: Entity,
    attempt: u64,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Start,
    Compiling,
    Version,
    Probe,
    Directory,
    Temporary,
    SignBinary,
    Archive,
    SignArchive,
    Verify,
    Publication,
    Done,
}

#[derive(Component)]
struct Version(String);

#[derive(Component)]
struct Published(Entity);

#[derive(Component)]
struct Failed(Failure);

#[derive(Component)]
struct Complete;

#[derive(Component)]
struct Io {
    purpose: Purpose,
}

#[derive(Component)]
struct Owner(Entity);

#[derive(Component)]
struct AcceptedIo {
    context: Entity,
    attempt: u64,
}

#[derive(Component)]
struct Witness {
    owner: Entity,
    context: Entity,
}

#[derive(bevy_ecs::system::SystemParam)]
pub(crate) struct Drains<'w, 's> {
    witnesses: Query<'w, 's, &'static Witness>,
}

impl Drains<'_, '_> {
    pub(crate) fn pending(&self, context: Entity) -> bool {
        self.witnesses
            .iter()
            .any(|witness| witness.context == context)
    }
}

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Received;

#[derive(Clone, Copy, Default)]
enum Purpose {
    #[default]
    Compile,
    Version,
    Probe,
    Directory,
    Temporary,
    Binary,
    Archive,
    Signature,
    ArchiveBytes,
    SignatureBytes,
}

type Requests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scope, &'static mut Phase),
    (Without<Failed>, Without<Complete>),
>;
type Packages<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Scope, &'static Package, &'static mut Phase),
    (Without<Failed>, Without<Complete>),
>;
type Replies<'w, 's, T> =
    Query<'w, 's, (Entity, &'static Owner, &'static Io, &'static T), With<Received>>;
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
type Closing<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Scope,
        Option<&'static Failed>,
        Option<&'static Published>,
    ),
    Or<(With<Failed>, With<Complete>)>,
>;

#[derive(bevy_ecs::system::SystemParam)]
struct Writer<'w, 's> {
    commands: Commands<'w, 's>,
}

impl Writer<'_, '_> {
    fn fail(&mut self, entity: Entity, failure: Failure) {
        self.commands.entity(entity).insert(Failed(failure));
    }

    fn request<J: StorageJob>(&mut self, root: Entity, purpose: Purpose, job: J) {
        self.commands.spawn((
            Io { purpose },
            Owner(root),
            job,
            Operation::bundle("dx.cli-package.io").unwrap(),
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

#[derive(bevy_ecs::system::SystemParam)]
struct Receipts<'w, 's> {
    bytes: Replies<'w, 's, FileBytes>,
    outputs: Replies<'w, 's, ToolOutput>,
    done: Replies<'w, 's, Done>,
    temporary: Replies<'w, 's, Directory>,
    pending: Pending<'w, 's>,
}

type ArtifactsStaging<'w, 's> = Query<'w, 's, &'static Directory>;

type ArtifactsNames<'w, 's> = Query<'w, 's, &'static Name>;

type ArtifactsPublished<'w, 's> = Query<'w, 's, &'static Published>;

type ArtifactsResults<'w, 's> =
    Query<'w, 's, (Has<Ready>, Option<&'static publication::Failed>), With<publication::Drained>>;

#[derive(bevy_ecs::system::SystemParam)]
struct Artifacts<'w, 's> {
    staging: ArtifactsStaging<'w, 's>,
    names: ArtifactsNames<'w, 's>,
    published: ArtifactsPublished<'w, 's>,
    results: ArtifactsResults<'w, 's>,
}

type LifetimesFlights<'w, 's> = Query<'w, 's, (&'static Owner, &'static Flight)>;

type LifetimesRows<'w, 's> = Query<'w, 's, (Entity, &'static Owner)>;

type LifetimesPublications<'w, 's> = Query<'w, 's, (), With<publication::Drained>>;

type LifetimesDestinations<'w, 's> =
    Query<'w, 's, (&'static ChildOf, &'static publication::Destination), With<Ready>>;

#[derive(bevy_ecs::system::SystemParam)]
struct Lifetimes<'w, 's> {
    flights: LifetimesFlights<'w, 's>,
    rows: LifetimesRows<'w, 's>,
    publications: LifetimesPublications<'w, 's>,
    destinations: LifetimesDestinations<'w, 's>,
}

type WakeRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Scope>, Without<Failed>, Without<Complete>)>;
fn accept<T: Component + Clone>(
    event: On<Requested<T>>,
    existing: Query<(), With<Scope>>,
    mut commands: Commands,
) {
    if existing.contains(event.entity) {
        return;
    }
    commands.entity(event.entity).insert((
        Scope {
            context: event.context,
            attempt: event.attempt,
        },
        Phase::Start,
        event.input.clone(),
    ));
}

fn collect<J: StorageJob>(
    results: Returned<J>,
    accepted: Query<&AcceptedIo>,
    scopes: Query<&Scope>,
    mut commands: Commands,
) {
    for (entity, owner, parent, job_owner, result) in &results {
        let current = accepted.get(entity).is_ok_and(|input| {
            scopes
                .get(owner.0)
                .is_ok_and(|scope| scope.context == input.context && scope.attempt == input.attempt)
        });
        if !current || parent.parent() != owner.0 || !job_owner.accepts(Some(parent)) {
            commands
                .entity(entity)
                .insert(Received)
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
    scopes: Query<&Scope, Without<Failed>>,
    staging: Query<&Directory>,
    loans: Query<&Lease>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, owner, job) in &pending {
        let Ok(scope) = scopes.get(owner.0) else {
            continue;
        };
        let loan = loans.get(scope.context).ok().map(|lease| lease.0.clone());
        let temporary = staging
            .get(owner.0)
            .ok()
            .map(|directory| directory.0.clone());
        let (task, flight) = Task::new(
            Scoped(job.clone()),
            loan,
            temporary.into_iter().collect(),
            wake.0.clone(),
        );
        commands.spawn((
            Witness {
                owner: owner.0,
                context: scope.context,
            },
            flight.clone(),
            Name::new("Standalone CLI native IO"),
            vmux_ecs::CreatedAt::now(),
        ));
        commands.entity(entity).insert((
            Submitted,
            AcceptedIo {
                context: scope.context,
                attempt: scope.attempt,
            },
            JobRequest(task),
            flight,
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

fn compile(
    mut requests: Requests,
    compilations: Query<(), With<Compile>>,
    contexts: Query<(&Checkout, &Settings)>,
    outputs: Replies<ToolOutput>,
    mut io: Writer,
) {
    for (entity, scope, mut phase) in &mut requests {
        if !compilations.contains(entity) {
            continue;
        }
        if *phase == Phase::Start {
            if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                io.fail(
                    entity,
                    Failure::message(
                        "Standalone CLI release packaging currently targets macOS aarch64",
                    ),
                );
                continue;
            }
            let Ok((checkout, settings)) = contexts.get(scope.context) else {
                continue;
            };
            if let Err(error) = settings.0.required("VMUX_UPDATE_PUBLIC_KEY") {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
            let mut environment = settings.0.clone();
            environment.remove("CARGO_BUILD_TARGET");
            environment.remove("CARGO_ENCODED_RUSTFLAGS");
            environment.remove("RUSTFLAGS");
            let program = environment
                .get("CARGO_BIN")
                .unwrap_or(OsStr::new("cargo"))
                .to_owned();
            let mut arguments = [
                "build",
                "--locked",
                "--release",
                "-p",
                "vmux_cli",
                "--no-default-features",
                "--features",
                "dx",
                "--target",
                "aarch64-apple-darwin",
                "--target-dir",
            ]
            .map(OsString::from)
            .to_vec();
            arguments.push(checkout.target.clone().into_os_string());
            *phase = Phase::Compiling;
            io.request(
                entity,
                Purpose::Compile,
                Capture {
                    environment,
                    directory: checkout.root.clone(),
                    program,
                    arguments,
                },
            );
        }
        for (receipt, owner, purpose, output) in &outputs {
            if owner.0 != entity || !matches!(purpose.purpose, Purpose::Compile) {
                continue;
            }
            if output.0.status.success() {
                io.commands.entity(entity).insert(Complete);
            } else {
                io.fail(entity, Failure::status(output.0.status));
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn prepare(
    mut requests: Packages,
    contexts: Query<(&Checkout, &Settings)>,
    receipts: Receipts,
    mut io: Writer,
) {
    for (entity, scope, package, mut phase) in &mut requests {
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        if *phase == Phase::Start {
            if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                io.fail(
                    entity,
                    Failure::message(
                        "Standalone CLI release packaging currently targets macOS aarch64",
                    ),
                );
                continue;
            }
            *phase = Phase::Version;
            io.request(
                entity,
                Purpose::Version,
                ReadFile(checkout.root.join("Cargo.toml")),
            );
        }
        for (receipt, owner, purpose, bytes) in &receipts.bytes {
            if owner.0 != entity
                || !matches!(purpose.purpose, Purpose::Version)
                || *phase != Phase::Version
            {
                continue;
            }
            let version = std::str::from_utf8(&bytes.0)
                .ok()
                .and_then(|text| text.parse::<toml::Table>().ok())
                .and_then(|value| {
                    value
                        .get("workspace")?
                        .get("package")?
                        .get("version")?
                        .as_str()
                        .map(str::to_owned)
                });
            let Some(version) = version else {
                io.fail(entity, Failure::message("Missing workspace version"));
                io.commands.entity(receipt).despawn();
                continue;
            };
            let name = if package.preview {
                "vmux-cli-nightly-aarch64-apple-darwin.tar.gz".into()
            } else {
                format!("vmux-cli-v{version}-aarch64-apple-darwin.tar.gz")
            };
            io.commands
                .entity(entity)
                .insert((Version(version), Name::new(name)));
            let mut environment = settings.0.clone();
            environment.set("VMUX_CLI_UPDATE_HANDOFF", "1");
            *phase = Phase::Probe;
            io.request(
                entity,
                Purpose::Probe,
                Capture {
                    environment,
                    directory: checkout.root.clone(),
                    program: checkout
                        .target
                        .join("aarch64-apple-darwin/release/vmux")
                        .into_os_string(),
                    arguments: vec!["--version".into()],
                },
            );
            io.commands.entity(receipt).despawn();
        }
    }
}

fn create_staging(
    mut requests: Packages,
    contexts: Query<(&Checkout, &Settings)>,
    receipts: Receipts,
    versions: Query<&Version>,
    mut io: Writer,
) {
    for (entity, scope, _package, mut phase) in &mut requests {
        let Ok((checkout, _settings)) = contexts.get(scope.context) else {
            continue;
        };
        for (receipt, owner, purpose, output) in &receipts.outputs {
            if owner.0 != entity
                || !matches!(purpose.purpose, Purpose::Probe)
                || *phase != Phase::Probe
            {
                continue;
            }
            let Ok(version) = versions.get(entity) else {
                continue;
            };
            if !output.0.status.success()
                || std::str::from_utf8(&output.0.stdout).ok().map(str::trim)
                    != Some(format!("vmux {}", version.0).as_str())
            {
                io.fail(
                    entity,
                    Failure::message("Standalone release CLI does not match the workspace version"),
                );
            } else {
                *phase = Phase::Directory;
                io.request(
                    entity,
                    Purpose::Directory,
                    CreateDirectory(checkout.directory("release")),
                );
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, purpose, _) in &receipts.done {
            if owner.0 != entity
                || !matches!(purpose.purpose, Purpose::Directory)
                || *phase != Phase::Directory
            {
                continue;
            }
            *phase = Phase::Temporary;
            io.request(
                entity,
                Purpose::Temporary,
                CreateTemporary {
                    parent: checkout.directory("release"),
                    prefix: ".cli-release".into(),
                },
            );
            io.commands.entity(receipt).despawn();
        }
        for (receipt, owner, _, directory) in &receipts.temporary {
            if owner.0 == entity && *phase == Phase::Temporary {
                io.commands.entity(entity).insert(directory.clone());
                *phase = Phase::SignBinary;
                io.commands.entity(receipt).despawn();
            }
        }
    }
}

fn archive(
    mut requests: Packages,
    contexts: Query<(&Checkout, &Settings)>,
    staging: Query<&Directory>,
    names: Query<&Name>,
    receipts: Receipts,
    mut io: Writer,
) {
    for (entity, scope, _, mut phase) in &mut requests {
        let Ok((checkout, _settings)) = contexts.get(scope.context) else {
            continue;
        };
        let Ok(directory) = staging.get(entity) else {
            continue;
        };
        let Ok(name) = names.get(entity) else {
            continue;
        };
        let archive = directory.0.path().join(name.as_str());
        for (receipt, owner, purpose, output) in &receipts.outputs {
            if owner.0 != entity {
                continue;
            }
            if !output.0.status.success() {
                io.fail(entity, Failure::status(output.0.status));
                io.commands.entity(receipt).despawn();
                continue;
            }
            match purpose.purpose {
                Purpose::Binary => {
                    *phase = Phase::Archive;
                    io.request(
                        entity,
                        Purpose::Archive,
                        WriteArchive {
                            binary: checkout.target.join("aarch64-apple-darwin/release/vmux"),
                            destination: archive.clone(),
                        },
                    );
                }
                Purpose::Signature => {
                    *phase = Phase::Verify;
                    io.request(entity, Purpose::ArchiveBytes, ReadArchive(archive.clone()));
                    io.request(
                        entity,
                        Purpose::SignatureBytes,
                        ReadFile(directory.0.path().join(format!("{}.sig", name.as_str()))),
                    );
                }
                _ => continue,
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn sign_archive(
    mut requests: Packages,
    contexts: Query<(&Checkout, &Settings)>,
    staging: Query<&Directory>,
    names: Query<&Name>,
    receipts: Receipts,
    mut io: Writer,
) {
    for (entity, scope, _, mut phase) in &mut requests {
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        let Ok(directory) = staging.get(entity) else {
            continue;
        };
        let Ok(name) = names.get(entity) else {
            continue;
        };
        let archive = directory.0.path().join(name.as_str());
        for (receipt, owner, purpose, _) in &receipts.done {
            if owner.0 != entity || !matches!(purpose.purpose, Purpose::Archive) {
                continue;
            }
            let key = match settings.0.required("VMUX_UPDATE_PRIVATE_KEY") {
                Ok(key) => key.to_owned(),
                Err(error) => {
                    io.fail(entity, Failure::from_error(error));
                    continue;
                }
            };
            *phase = Phase::SignArchive;
            io.request(
                entity,
                Purpose::Signature,
                Capture {
                    environment: settings.0.clone(),
                    directory: checkout.root.clone(),
                    program: settings
                        .0
                        .get("CARGO_BIN")
                        .unwrap_or(OsStr::new("cargo"))
                        .to_owned(),
                    arguments: vec![
                        "packager".into(),
                        "signer".into(),
                        "sign".into(),
                        archive.clone().into_os_string(),
                        "--private-key".into(),
                        key,
                        "--password".into(),
                        settings
                            .0
                            .get("VMUX_UPDATE_PRIVATE_KEY_PASSWORD")
                            .unwrap_or(OsStr::new(""))
                            .to_owned(),
                    ],
                },
            );
            io.commands.entity(receipt).despawn();
        }
    }
}

fn sign_binary(
    mut requests: Packages,
    contexts: Query<(&Checkout, &Settings)>,
    staging: Query<&Directory>,
    names: Query<&Name>,
    receipts: Receipts,
    mut io: Writer,
) {
    for (entity, scope, _, phase) in &mut requests {
        let Ok((checkout, settings)) = contexts.get(scope.context) else {
            continue;
        };
        let Ok(directory) = staging.get(entity) else {
            continue;
        };
        let Ok(name) = names.get(entity) else {
            continue;
        };
        let _archive = directory.0.path().join(name.as_str());
        if *phase != Phase::SignBinary || receipts.pending.iter().any(|owner| owner.0 == entity) {
            continue;
        }
        let identity = match settings.0.required("APPLE_SIGNING_IDENTITY") {
            Ok(value) => value.to_owned(),
            Err(error) => {
                io.fail(entity, Failure::from_error(error));
                continue;
            }
        };
        let entitlements = settings
            .0
            .get("VMUX_HOST_ENTITLEMENTS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| checkout.root.join("packaging/macos/Vmux.entitlements"));
        let namespace = settings
            .0
            .get("VMUX_HOST_NAMESPACE")
            .unwrap_or(OsStr::new("ai.vmux"))
            .to_string_lossy();
        let identifier = settings
            .0
            .get("VMUX_HOST_CLI_ID")
            .map(OsStr::to_owned)
            .unwrap_or_else(|| format!("{namespace}.cli").into());
        let mut arguments = vec![
            "--force".into(),
            "--verify".into(),
            "--sign".into(),
            identity,
            "--options".into(),
            "runtime".into(),
            "--entitlements".into(),
            entitlements.into_os_string(),
        ];
        if let Some(keychain) = settings.0.get("CODESIGN_KEYCHAIN") {
            arguments.extend(["--keychain".into(), keychain.to_owned()]);
        }
        arguments.extend([
            "--identifier".into(),
            identifier,
            checkout
                .target
                .join("aarch64-apple-darwin/release/vmux")
                .into_os_string(),
        ]);
        io.request(
            entity,
            Purpose::Binary,
            Capture {
                environment: settings.0.clone(),
                directory: checkout.root.clone(),
                program: "codesign".into(),
                arguments,
            },
        );
    }
}

fn verify(
    mut requests: Packages,
    contexts: Query<&Settings>,
    archives: Replies<Bytes>,
    signatures: Replies<FileBytes>,
    mut commands: Commands,
) {
    for (entity, scope, _, mut phase) in &mut requests {
        if *phase != Phase::Verify {
            continue;
        }
        let Some((archive_receipt, _, _, archive)) =
            archives.iter().find(|(_, owner, _, _)| owner.0 == entity)
        else {
            continue;
        };
        let Some((signature_receipt, _, _, signature)) =
            signatures.iter().find(|(_, owner, purpose, _)| {
                owner.0 == entity && matches!(purpose.purpose, Purpose::SignatureBytes)
            })
        else {
            continue;
        };
        let Ok(settings) = contexts.get(scope.context) else {
            continue;
        };
        let result = (|| -> crate::Result<()> {
            let key = String::from_utf8(
                STANDARD.decode(
                    settings
                        .0
                        .required("VMUX_UPDATE_PUBLIC_KEY")?
                        .as_encoded_bytes(),
                )?,
            )?;
            let signature =
                String::from_utf8(STANDARD.decode(std::str::from_utf8(&signature.0)?.trim())?)?;
            minisign_verify::PublicKey::decode(&key)?.verify(
                &archive.0,
                &minisign_verify::Signature::decode(&signature)?,
                false,
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => *phase = Phase::Publication,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Failed(Failure::from_error(error)));
            }
        }
        commands.entity(archive_receipt).despawn();
        commands.entity(signature_receipt).despawn();
    }
}

fn publish(
    mut requests: Packages,
    loans: Query<&Lease>,
    contexts: Query<&Checkout>,
    artifacts: Artifacts,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, scope, _, mut phase) in &mut requests {
        if *phase != Phase::Publication {
            continue;
        }
        if let Ok(publication) = artifacts.published.get(entity) {
            if let Ok((ready, error)) = artifacts.results.get(publication.0) {
                if let Some(error) = error {
                    commands.entity(entity).insert(Failed(error.0.clone()));
                } else if ready {
                    *phase = Phase::Done;
                    commands.entity(entity).insert(Complete);
                }
            }
            continue;
        }
        let (Ok(checkout), Ok(directory), Ok(name)) = (
            contexts.get(scope.context),
            artifacts.staging.get(entity),
            artifacts.names.get(entity),
        ) else {
            continue;
        };
        let publication = commands
            .spawn((
                publication::Request {
                    owner: entity,
                    lease: loans.get(scope.context).ok().map(|lease| lease.0.clone()),
                    context: scope.context,
                    attempt: scope.attempt,
                    staging: directory.0.clone(),
                },
                Owner(entity),
            ))
            .id();
        for (order, name) in [name.as_str().to_owned(), format!("{}.sig", name.as_str())]
            .into_iter()
            .enumerate()
        {
            commands.spawn((
                publication::Move,
                Cwd(directory.0.path().join(&name)),
                publication::Destination(checkout.directory("release").join(name)),
                Order(order as u32),
                ChildOf(publication),
            ));
        }
        commands.entity(entity).insert(Published(publication));
        wake.0.wake_by_ref();
    }
}

fn cancel(
    requests: Query<(Entity, &Scope, Option<&ChildOf>)>,
    owners: Query<&Attempt>,
    states: Query<&OperationState>,
    io: Query<(Entity, &Owner), With<Io>>,
    mut commands: Commands,
) {
    for (entity, scope, parent) in &requests {
        if Signals::pending().is_none()
            && parent.is_some_and(|parent| parent.parent() == scope.context)
            && owners
                .get(entity)
                .is_ok_and(|attempt| attempt.0 == scope.attempt)
            && states
                .get(scope.context)
                .is_ok_and(|state| *state == OperationState::Running)
        {
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
    for (entity, owner) in &io {
        if !requests.contains(owner.0) {
            commands.entity(entity).insert(OperationState::Canceled);
        }
    }
}

fn forget(
    witnesses: Query<(Entity, &Witness, &Flight)>,
    scopes: Query<(), With<Scope>>,
    completed: Query<(), With<crate::Completed>>,
    states: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, witness, flight) in &witnesses {
        if flight.pending() {
            continue;
        };
        if !scopes.contains(witness.owner) && !completed.contains(witness.owner) {
            if let Ok(state) = states.get(witness.context) {
                if *state != OperationState::Canceled {
                    commands
                        .entity(witness.context)
                        .insert(OperationState::Failed);
                }
                commands
                    .entity(witness.context)
                    .insert(crate::Retirement(Failure::message(
                        "Standalone CLI operation owner disappeared",
                    )));
            }
            if let Ok(mut owner) = commands.get_entity(witness.owner) {
                owner
                    .remove::<crate::Running>()
                    .insert((crate::Completed, OperationState::Failed));
            }
        }
        commands.entity(entity).despawn();
    }
}

fn wake(
    requests: WakeRequests,
    published: Query<&Published>,
    io: Query<(&Owner, &OperationState), With<Io>>,
    wake: Res<JobWake>,
) {
    for entity in &requests {
        if published.contains(entity) {
            continue;
        }
        if !io
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
    wake: Res<JobWake>,
    mut completions: MessageWriter<Completion>,
    mut commands: Commands,
) {
    for (entity, scope, error, published) in &requests {
        if lifetimes
            .flights
            .iter()
            .any(|(owner, flight)| owner.0 == entity && flight.pending())
        {
            continue;
        }
        if published.is_some_and(|published| !lifetimes.publications.contains(published.0)) {
            continue;
        }
        let mut artifacts = Vec::new();
        if let Some(published) = published {
            for (parent, destination) in &lifetimes.destinations {
                if parent.parent() == published.0 {
                    artifacts.push(destination.0.clone());
                }
            }
        }
        let mut remaining = false;
        for (child, owner) in &lifetimes.rows {
            if owner.0 == entity {
                commands.entity(child).despawn();
                remaining = true;
            }
        }
        if remaining {
            if error.is_none() {
                for path in artifacts {
                    commands.spawn((
                        Name::new(path.display().to_string()),
                        crate::Artifact(path),
                        Ready,
                        crate::Completed,
                        vmux_ecs::CreatedAt::now(),
                        ChildOf(scope.context),
                    ));
                }
            }
            commands.entity(entity).remove::<Published>();
            wake.0.wake_by_ref();
            continue;
        }
        let result = match error {
            Some(error) => Err(error.0.clone()),
            None => Ok(Outcome::Done),
        };
        completions.write(Completion {
            entity,
            context: scope.context,
            attempt: scope.attempt,
            result,
        });
        commands.entity(entity).remove::<Scope>();
        wake.0.wake_by_ref();
    }
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    use vmux_ecs::cli::CliResult;

    #[test]
    fn deleting_a_cli_compile_owner_waits_for_its_native_worker_to_exit() {
        let _permit = crate::testing::Permit::acquire();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let worker = root.join("worker");
        fs::write(
            &worker,
            "#!/bin/sh\nprintf '%s' \"$$\" > pid\nexec sleep 60\n",
        )
        .unwrap();
        fs::set_permissions(&worker, fs::Permissions::from_mode(0o755)).unwrap();
        let environment = crate::environment::Environment::current();
        let mut app = App::new();
        app.add_plugins(crate::DxPlugin);
        let context = app
            .world_mut()
            .spawn((
                crate::Accepted,
                Operation::bundle("dx.cli.fixture").unwrap().running(),
                Settings(environment.clone()),
                Checkout {
                    root: root.clone(),
                    target: root.join("target"),
                    build_target: None,
                },
            ))
            .id();
        let step = app
            .world_mut()
            .spawn((
                Scope {
                    context,
                    attempt: 1,
                },
                Compile,
                Phase::Compiling,
                crate::Attempt(1),
                crate::Running,
                crate::Started {
                    context,
                    attempt: 1,
                },
                ChildOf(context),
            ))
            .id();
        app.world_mut().spawn((
            Io {
                purpose: Purpose::Compile,
            },
            Owner(step),
            Capture {
                environment,
                directory: root.clone(),
                program: worker.into_os_string(),
                arguments: Vec::new(),
            },
            Operation::bundle("dx.cli-package.io").unwrap(),
            ChildOf(step),
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("pid").exists() {
            app.update();
            assert!(Instant::now() < deadline, "compile worker did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        let pid: i32 = fs::read_to_string(root.join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        app.world_mut().entity_mut(step).despawn();
        loop {
            app.update();
            if app.world().get::<CliResult>(context).is_some() {
                break;
            };
            assert!(
                Instant::now() < deadline,
                "compile cancellation did not drain"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            app.world().get::<CliResult>(context),
            Some(CliResult(Err(_)))
        ));
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    }

    #[test]
    fn wrong_release_binary_version_preserves_published_artifacts() {
        let _permit = crate::testing::Permit::acquire();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let target = root.join("target");
        fs::create_dir_all(target.join("aarch64-apple-darwin/release")).unwrap();
        fs::create_dir_all(target.join("release")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = '9.0.0'\n",
        )
        .unwrap();
        let binary = target.join("aarch64-apple-darwin/release/vmux");
        fs::write(&binary, "#!/bin/sh\nprintf 'vmux 8.0.0\\n'\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let archive = target.join("release/vmux-cli-v9.0.0-aarch64-apple-darwin.tar.gz");
        fs::write(&archive, "previous release").unwrap();
        let mut app = App::new();
        app.add_plugins(crate::DxPlugin);
        let operation = app
            .world_mut()
            .spawn((
                crate::Accepted,
                Operation::bundle("dx.cli-release.fixture")
                    .unwrap()
                    .running(),
                Checkout {
                    root,
                    target,
                    build_target: None,
                },
                Settings(crate::environment::Environment::current()),
                crate::storage::Storage::new(crate::policy::Policy::environment().unwrap()),
            ))
            .id();
        app.world_mut().spawn((
            ChildOf(operation),
            Attempt(1),
            OperationState::Pending,
            crate::work::Work::CliPackage(false),
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.update();
            if let Some(CliResult(Ok(code))) = app.world().get::<CliResult>(operation) {
                assert_ne!(*code, 0);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "CLI package refusal did not finish"
            );
            std::thread::park_timeout(Duration::from_millis(1));
        }
        assert_eq!(fs::read_to_string(archive).unwrap(), "previous release");
    }
}
