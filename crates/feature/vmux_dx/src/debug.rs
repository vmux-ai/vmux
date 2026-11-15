use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use sha2::{Digest, Sha256};
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Operation, OperationState, Order};

use crate::debug_io::{
    CaptureOutput, CopyFile, CreateDirectory, CreateTemporary, DirectoryEntries, Done, FileBytes,
    FileDigest, Flight, HashFile, InspectPath, IoJob, IoValue, LinkPath, PathMetadata,
    PendingValue, ProcessValue, ReadDirectory, ReadFile, RemoveFile, RenamePath, RunProcess,
    SpawnProcess, Task, TemporaryValue, ToolOutput, WriteFile,
};
use crate::execution::Failure;
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::work::Outcome;
use crate::{Completion, Settings};

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<CaptureOutput>::default(),
            IoPlugin::<SpawnProcess>::default(),
            IoPlugin::<RunProcess>::default(),
            IoPlugin::<ReadFile>::default(),
            IoPlugin::<HashFile>::default(),
            IoPlugin::<InspectPath>::default(),
            IoPlugin::<ReadDirectory>::default(),
            IoPlugin::<CreateDirectory>::default(),
            IoPlugin::<CopyFile>::default(),
            IoPlugin::<RemoveFile>::default(),
            IoPlugin::<LinkPath>::default(),
            IoPlugin::<WriteFile>::default(),
            IoPlugin::<RenamePath>::default(),
            IoPlugin::<CreateTemporary>::default(),
        ))
        .configure_sets(
            Update,
            (
                Stage::Collect,
                Stage::Decide,
                Stage::Admit,
                Stage::Submit,
                Stage::Release,
            )
                .chain()
                .in_set(JobSet::Apply)
                .after(crate::Stage::Execute),
        )
        .add_systems(PreUpdate, cancel)
        .add_systems(
            Update,
            (
                initialize, cancel, identity, capacity, clear, sign, bundle, publish, run, stop,
                signature, provider, wake,
            )
                .chain()
                .in_set(Stage::Decide),
        )
        .add_systems(Update, (admit, ApplyDeferred).chain().in_set(Stage::Admit))
        .add_systems(Update, (release, finish).chain().in_set(Stage::Release));
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stage {
    Collect,
    Decide,
    Admit,
    Submit,
    Release,
}

struct IoPlugin<J: IoJob>(std::marker::PhantomData<fn() -> J>);

impl<J: IoJob> Default for IoPlugin<J> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<J: IoJob> bevy_app::Plugin for IoPlugin<J> {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<J>>::default())
            .add_systems(Update, collect::<J>.in_set(Stage::Collect))
            .add_systems(Update, submit::<J>.in_set(Stage::Submit));
    }
}

#[derive(Component, Default)]
pub(crate) struct Io;

#[derive(Component)]
struct Admitted;

#[derive(Component, Clone)]
pub enum Request {
    Capacity {
        minimum: u64,
    },
    Identity {
        print: bool,
    },
    Clear,
    Sign,
    Bundle,
    Publish,
    Run {
        profile: String,
        platform: String,
    },
    Stop,
    ProviderCompile {
        package: String,
        binary: String,
        manifest: PathBuf,
    },
    ProviderSnapshot {
        binary: String,
    },
    ProviderInvoke {
        arguments: Vec<OsString>,
    },
    Signature {
        path: PathBuf,
        binary: PathBuf,
        stamp: PathBuf,
        identifier: Option<String>,
        suffix: String,
    },
}

#[derive(Component)]
pub struct Captured {
    pub root: Entity,
    pub attempt: u64,
}

#[derive(Component)]
pub struct Lease(pub Option<std::sync::Arc<crate::target_lock::TargetLock>>);

#[derive(Component, Clone, Copy)]
struct Root(Entity);

#[derive(Component, Clone, Copy)]
struct Owner(Entity);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    CapacityInspect,
    CapacityRead,
    IdentityKeychain,
    IdentityLookup,
    IdentityProvision,
    IdentityConfirm,
    ClearInspect,
    ClearFinish,
    SignWaiting,
    BundleBranch,
    BundlePlan,
    BundleService,
    BundleServiceWait,
    BundleDesktop,
    BundleWait,
    PublishCheck,
    PublishDone,
    LaunchInspect,
    LaunchInputs,
    LaunchSelect,
    LaunchVerify,
    LaunchExecute,
    LaunchDone,
    StopExecute,
    StopDone,
    SignatureInputs,
    SignatureCache,
    SignaturePrepare,
    SignatureFormat,
    SignatureRefresh,
    SignatureRecord,
    SignatureDone,
    SignatureRpaths,
    SignatureBindings,
    ProviderCompiled,
    SnapshotInspect,
    SnapshotCopy,
    SnapshotCheck,
    ProviderInvoked,
}

#[derive(Component)]
struct Sequence(u32);

impl Sequence {
    fn request<J: IoJob>(
        &mut self,
        commands: &mut Commands,
        owner: Entity,
        previous: &mut Option<Entity>,
        job: J,
        tag: Tag,
        optional: bool,
    ) {
        let mut child = commands.spawn((
            Operation::bundle("dx.debug.io").unwrap(),
            Io,
            job,
            tag,
            Order(self.0),
            ChildOf(owner),
        ));
        self.0 += 1;
        if let Some(previous) = *previous {
            child.insert(After(previous));
        }
        if optional {
            child.insert(Optional);
        }
        *previous = Some(child.id());
    }
}

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Complete(Outcome);

#[derive(Component)]
struct Sent;

#[derive(Component, Clone)]
struct Failed(Failure);

#[derive(Component)]
struct After(Entity);

#[derive(Component)]
struct Optional;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    None,
    Capacity,
    Keychain,
    Identities,
    Temporary,
    Password,
    Config,
    Partition,
    Manifest,
    Branch,
    Commit,
    Stdlib,
    Rpaths,
    Libraries,
    Stamp,
    Binary,
    Path,
    Directory,
    Entries,
    Signature,
    Format,
    Entitlements,
    Input(u32),
    Library(u32),
}

#[derive(Component)]
struct Input(PathBuf);

#[derive(Component)]
struct Expected(String);

#[derive(Component)]
struct Identity(String);

#[derive(Component)]
struct Keychain(OsString);

#[derive(Component)]
struct Sources;

#[derive(Component)]
struct CopyFrom(PathBuf);

#[derive(Component)]
struct Copied;

#[derive(Component)]
struct Linking(PathBuf);

#[derive(Component)]
struct Rpath(PathBuf);

#[derive(Component)]
struct Binding {
    original: String,
    replacement: String,
}

#[derive(Component)]
struct Target(PathBuf);

#[derive(Component)]
struct File(PathBuf);

#[derive(Component)]
struct Layout {
    app: PathBuf,
    service: PathBuf,
    binary: PathBuf,
    plist: PathBuf,
}

type NativeResults<'w, 's> = Query<
    'w,
    's,
    (
        EntityRef<'static>,
        &'static Tag,
        &'static Order,
        &'static ChildOf,
        &'static OperationState,
    ),
    (
        With<Io>,
        Or<(
            With<ToolOutput>,
            With<FileBytes>,
            With<FileDigest>,
            With<PathMetadata>,
            With<DirectoryEntries>,
            With<TemporaryValue>,
            With<ProcessValue>,
            With<PendingValue>,
            With<Done>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct Facts<'w, 's> {
    values: NativeResults<'w, 's>,
    states: Query<'w, 's, (&'static OperationState, &'static ChildOf), With<Io>>,
}

#[derive(SystemParam)]
struct Scope<'w, 's> {
    requests: Query<
        'w,
        's,
        (
            &'static Root,
            &'static Owner,
            &'static ChildOf,
            Option<&'static Failed>,
        ),
        With<Request>,
    >,
    roots: Query<'w, 's, &'static OperationState, Without<Io>>,
}

impl Scope<'_, '_> {
    fn live(&self, mut entity: Entity) -> bool {
        if Signals::pending().is_some() {
            return false;
        }
        loop {
            let Ok((root, owner, parent, failure)) = self.requests.get(entity) else {
                return false;
            };
            if failure.is_some() || owner.0 != parent.parent() {
                return false;
            }
            if owner.0 == root.0 {
                return self
                    .roots
                    .get(root.0)
                    .is_ok_and(|state| *state == OperationState::Running);
            }
            entity = owner.0;
        }
    }
}

impl Facts<'_, '_> {
    fn get<T: Component>(&self, owner: Entity, tag: Tag) -> Option<&T> {
        let mut latest = None;
        for (entity, key, order, parent, state) in &self.values {
            if parent.parent() != owner || *key != tag || *state != OperationState::Succeeded {
                continue;
            }
            if latest.is_none_or(|(_, previous)| order.0 > previous) {
                latest = Some((entity, order.0));
            }
        }
        latest.and_then(|(entity, _)| entity.get::<T>())
    }

    fn text(&self, owner: Entity, tag: Tag) -> Option<String> {
        if let Some(FileBytes(bytes)) = self.get::<FileBytes>(owner, tag) {
            return std::str::from_utf8(bytes).ok().map(String::from);
        }
        if let Some(ToolOutput(output)) = self.get::<ToolOutput>(owner, tag) {
            return std::str::from_utf8(&output.stdout).ok().map(String::from);
        }
        self.get::<FileDigest>(owner, tag)
            .map(|digest| digest.0.clone())
    }

    fn pending(&self, owner: Entity) -> bool {
        self.states.iter().any(|(state, parent)| {
            parent.parent() == owner
                && matches!(state, OperationState::Pending | OperationState::Running)
        })
    }
}

type Requests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Checkout,
        &'static Settings,
        &'static mut Phase,
        &'static mut Sequence,
    ),
    (Without<Io>, Without<Complete>, Without<Failed>),
>;

type Initializing<'w, 's> =
    Query<'w, 's, (Entity, &'static Captured), (With<Request>, Without<Phase>)>;
type Canceling<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static ChildOf,
        Option<&'static Failed>,
    ),
    (With<Request>, Without<Complete>),
>;
type Signatures<'w, 's> = Query<
    'w,
    's,
    (
        &'static ChildOf,
        Option<&'static Complete>,
        Option<&'static Failed>,
    ),
    With<Sources>,
>;
type Finishing<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Captured,
        Option<&'static Complete>,
        Option<&'static Failed>,
    ),
    (With<Request>, Without<Sent>),
>;

fn initialize(requests: Initializing, mut commands: Commands) {
    for (entity, captured) in &requests {
        commands.entity(entity).insert((
            Root(captured.root),
            Owner(captured.root),
            Phase::Start,
            Sequence(0),
        ));
    }
}

type Receipts<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobResult<Task<J>>,
        &'static JobOwner<Task<J>>,
        &'static ChildOf,
        Has<Optional>,
    ),
    Without<<J as IoJob>::Data>,
>;

fn collect<J: IoJob>(
    receipts: Receipts<J>,
    roots: Query<(Entity, &Root, Option<&Failed>), With<Request>>,
    states: Query<&OperationState, Without<Io>>,
    scope: Scope,
    mut commands: Commands,
) {
    for (entity, result, owner, parent, optional) in &receipts {
        let Ok((request, root, failed)) = roots.get(parent.parent()) else {
            continue;
        };
        if !scope.live(request)
            || !owner.accepts(Some(parent))
            || failed.is_some()
            || !states
                .get(root.0)
                .is_ok_and(|state| *state == OperationState::Running)
        {
            commands.entity(entity).insert(OperationState::Canceled);
            continue;
        }
        let report = match &result.0 {
            Ok(report) => report,
            Err(error) => {
                commands
                    .entity(request)
                    .insert(Failed(Failure::message(error.clone())));
                commands.entity(entity).insert(OperationState::Failed);
                continue;
            }
        };
        let value = match &report.result {
            Ok(value) => value,
            Err(error) => {
                commands.entity(request).insert(Failed(error.clone()));
                commands.entity(entity).insert(OperationState::Failed);
                continue;
            }
        };
        if let Err(error) = value.validate(optional) {
            commands.entity(request).insert(Failed(error));
            commands.entity(entity).insert(OperationState::Failed);
            continue;
        }
        commands
            .entity(entity)
            .insert((value.clone(), OperationState::Succeeded))
            .remove::<JobResult<Task<J>>>();
    }
}

fn cancel(
    requests: Canceling,
    roots: Query<&OperationState, Without<Io>>,
    owners: Query<&Failed, With<Request>>,
    scope: Scope,
    mut effects: Query<(&ChildOf, &mut OperationState), With<Io>>,
    mut commands: Commands,
) {
    for (entity, root, owner, failure) in &requests {
        let parent_failure = owners.get(owner.parent()).ok();
        if scope.live(entity)
            && roots
                .get(root.0)
                .is_ok_and(|state| *state == OperationState::Running)
            && failure.is_none()
            && parent_failure.is_none()
        {
            continue;
        }
        for (parent, mut state) in &mut effects {
            if parent.parent() == entity {
                *state = OperationState::Canceled;
            }
        }
        if failure.is_none() {
            let failure = match parent_failure {
                Some(failure) => failure.clone(),
                None if Signals::pending().is_some() => {
                    Failed(Failure::interrupted(Signals::pending().unwrap()))
                }
                None => Failed(Failure::message("Debug request owner is no longer active")),
            };
            commands.entity(entity).insert(failure);
        }
    }
}

fn identity(
    mut requests: Requests,
    facts: Facts,
    identities: Query<&Identity>,
    keychains: Query<&Keychain>,
    mut commands: Commands,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        let Request::Identity { print } = request else {
            continue;
        };
        if facts.pending(entity) {
            continue;
        }
        let environment = &settings.0;
        let mut previous = None;
        match *phase {
            Phase::Start => {
                if environment.present("APPLE_SIGNING_IDENTITY") || !cfg!(target_os = "macos") {
                    let identity = match environment.get("APPLE_SIGNING_IDENTITY") {
                        Some(value) => match value.to_str() {
                            Some(value) => value.to_owned(),
                            None => {
                                commands
                                    .entity(entity)
                                    .insert(Failed(Failure::message("Invalid signing identity")));
                                continue;
                            }
                        },
                        None => String::new(),
                    };
                    if *print {
                        println!("{identity}");
                    }
                    commands
                        .entity(entity)
                        .insert(Complete(Outcome::Identity(identity)));
                    continue;
                }
                let identity = environment
                    .get("VMUX_LOCAL_SIGNING_IDENTITY")
                    .unwrap_or(OsStr::new("Vmux Dev"));
                let Some(identity) = identity.to_str() else {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message("Invalid signing identity")));
                    continue;
                };
                if identity.contains(['\r', '\n']) {
                    commands.entity(entity).insert(Failed(Failure::message(
                        "Signing identity must be a single line",
                    )));
                    continue;
                }
                commands.entity(entity).insert(Identity(identity.into()));
                if let Some(keychain) = environment.get("VMUX_LOCAL_CODESIGN_KEYCHAIN") {
                    commands
                        .entity(entity)
                        .insert(Keychain(keychain.to_owned()));
                } else {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            environment,
                            &checkout.root,
                            "security".into(),
                            ["default-keychain", "-d", "user"]
                                .map(OsString::from)
                                .into(),
                        ),
                        Tag::Keychain,
                        false,
                    );
                }
                *phase = Phase::IdentityKeychain;
            }
            Phase::IdentityKeychain => {
                let keychain = match keychains.get(entity) {
                    Ok(keychain) => keychain.0.clone(),
                    Err(_) => {
                        let Some(value) = facts.text(entity, Tag::Keychain) else {
                            continue;
                        };
                        let keychain = OsString::from(value.trim().trim_matches('"'));
                        commands.entity(entity).insert(Keychain(keychain.clone()));
                        keychain
                    }
                };
                if keychain.is_empty() {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message("Default user keychain not found")));
                    continue;
                }
                let mut arguments = ["find-identity", "-v", "-p", "codesigning"]
                    .map(OsString::from)
                    .to_vec();
                arguments.push(keychain);
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "security".into(), arguments),
                    Tag::Identities,
                    false,
                );
                *phase = Phase::IdentityLookup;
            }
            Phase::IdentityLookup => {
                let identity = identities.get(entity).unwrap();
                let Some(found) = facts.text(entity, Tag::Identities) else {
                    continue;
                };
                if found.contains(&format!("\"{}\"", identity.0)) {
                    if *print {
                        println!("{}", identity.0);
                    }
                    commands
                        .entity(entity)
                        .insert(Complete(Outcome::Identity(identity.0.clone())));
                    continue;
                }
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CreateTemporary,
                    Tag::Temporary,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "uuidgen".into(), Vec::new()),
                    Tag::Password,
                    false,
                );
                *phase = Phase::IdentityProvision;
            }
            Phase::IdentityProvision => {
                let identity = &identities.get(entity).unwrap().0;
                let keychain = &keychains.get(entity).unwrap().0;
                let Some(TemporaryValue(temporary)) = facts.get(entity, Tag::Temporary) else {
                    continue;
                };
                let Some(password) = facts.text(entity, Tag::Password) else {
                    continue;
                };
                let password = password.trim();
                let config = temporary.path().join("openssl.cnf");
                let key = temporary.path().join("key.pem");
                let cert = temporary.path().join("cert.pem");
                let p12 = temporary.path().join("identity.p12");
                let text = format!(
                    "[req]\ndistinguished_name=req_distinguished_name\nx509_extensions=v3_codesign\nprompt=no\n[req_distinguished_name]\nCN={identity}\n[v3_codesign]\nbasicConstraints=critical,CA:false\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n"
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    WriteFile(config.clone(), text.into_bytes()),
                    Tag::Config,
                    false,
                );
                let mut arguments = [
                    "req", "-new", "-newkey", "rsa:2048", "-nodes", "-x509", "-days", "3650",
                    "-sha256", "-config",
                ]
                .map(OsString::from)
                .to_vec();
                arguments.extend([
                    config.into_os_string(),
                    "-keyout".into(),
                    key.clone().into_os_string(),
                    "-out".into(),
                    cert.clone().into_os_string(),
                ]);
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "openssl".into(), arguments),
                    Tag::None,
                    false,
                );
                let arguments = vec![
                    "pkcs12".into(),
                    "-export".into(),
                    "-out".into(),
                    p12.clone().into_os_string(),
                    "-inkey".into(),
                    key.into_os_string(),
                    "-in".into(),
                    cert.clone().into_os_string(),
                    "-passout".into(),
                    format!("pass:{password}").into(),
                    "-keypbe".into(),
                    "PBE-SHA1-3DES".into(),
                    "-certpbe".into(),
                    "PBE-SHA1-3DES".into(),
                    "-macalg".into(),
                    "sha1".into(),
                ];
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "openssl".into(), arguments),
                    Tag::None,
                    false,
                );
                let arguments = vec![
                    "import".into(),
                    p12.into_os_string(),
                    "-f".into(),
                    "pkcs12".into(),
                    "-k".into(),
                    keychain.clone(),
                    "-P".into(),
                    password.into(),
                    "-T".into(),
                    "/usr/bin/codesign".into(),
                ];
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "security".into(), arguments),
                    Tag::None,
                    false,
                );
                let arguments = vec![
                    "add-trusted-cert".into(),
                    "-r".into(),
                    "trustRoot".into(),
                    "-p".into(),
                    "codeSign".into(),
                    "-k".into(),
                    keychain.clone(),
                    cert.into_os_string(),
                ];
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "security".into(), arguments),
                    Tag::None,
                    false,
                );
                let arguments = vec![
                    "set-key-partition-list".into(),
                    "-S".into(),
                    "apple-tool:,apple:,codesign:".into(),
                    "-s".into(),
                    "-l".into(),
                    identity.into(),
                    keychain.clone(),
                ];
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "security".into(), arguments),
                    Tag::Partition,
                    true,
                );
                let arguments = vec![
                    "find-identity".into(),
                    "-v".into(),
                    "-p".into(),
                    "codesigning".into(),
                    keychain.clone(),
                ];
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(environment, &checkout.root, "security".into(), arguments),
                    Tag::Identities,
                    false,
                );
                *phase = Phase::IdentityConfirm;
            }
            Phase::IdentityConfirm => {
                let identity = identities.get(entity).unwrap();
                let Some(found) = facts.text(entity, Tag::Identities) else {
                    continue;
                };
                if !found.contains(&format!("\"{}\"", identity.0)) {
                    commands.entity(entity).insert(Failed(Failure::message(
                        "Created identity is not valid for codesigning",
                    )));
                    continue;
                }
                if let Some(ToolOutput(output)) = facts.get(entity, Tag::Partition)
                    && !output.status.success()
                {
                    eprintln!("codesign may request keychain access");
                }
                if *print {
                    println!("{}", identity.0);
                }
                commands
                    .entity(entity)
                    .insert(Complete(Outcome::Identity(identity.0.clone())));
            }
            _ => {}
        }
    }
}

fn capacity(mut requests: Requests, facts: Facts, targets: Query<&Target>, mut commands: Commands) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        let Request::Capacity { minimum } = request else {
            continue;
        };
        if facts.pending(entity) {
            continue;
        }
        let mut previous = None;
        match *phase {
            Phase::Start => {
                commands
                    .entity(entity)
                    .insert(Target(checkout.target.clone()));
                *phase = Phase::CapacityInspect;
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    InspectPath(checkout.target.clone()),
                    Tag::Path,
                    false,
                );
            }
            Phase::CapacityInspect => {
                let Some(PathMetadata(metadata)) = facts.get(entity, Tag::Path) else {
                    continue;
                };
                let Ok(target) = targets.get(entity) else {
                    continue;
                };
                if metadata.is_some() {
                    *phase = Phase::CapacityRead;
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "df".into(),
                            vec!["-Pk".into(), target.0.clone().into_os_string()],
                        ),
                        Tag::Capacity,
                        false,
                    );
                } else if let Some(parent) = target.0.parent() {
                    let parent = parent.to_owned();
                    commands.entity(entity).insert(Target(parent.clone()));
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        InspectPath(parent),
                        Tag::Path,
                        false,
                    );
                } else {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message("Cannot find target filesystem")));
                }
            }
            Phase::CapacityRead => {
                let Some(text) = facts.text(entity, Tag::Capacity) else {
                    continue;
                };
                let free = match text.lines().nth(1) {
                    Some(line) => match line.split_whitespace().nth(3) {
                        Some(value) => match value.parse::<u64>() {
                            Ok(value) => value.checked_mul(1024),
                            Err(_) => None,
                        },
                        None => None,
                    },
                    None => None,
                };
                match free {
                    Some(free) if free >= *minimum => {
                        commands.entity(entity).insert(Complete(Outcome::Done));
                    }
                    Some(_) => {
                        commands.entity(entity).insert(Failed(Failure::message(format!(
                            "Insufficient build space: need {} GiB free; stop unused apps/builds and run vmux cache clean --apply",
                            *minimum as f64 / crate::policy::GIB as f64,
                        ))));
                    }
                    None => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message("Invalid filesystem capacity")));
                    }
                }
            }
            _ => {}
        }
    }
}

fn clear(mut requests: Requests, facts: Facts, mut commands: Commands) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if !matches!(request, Request::Clear) || facts.pending(entity) {
            continue;
        }
        let mut previous = None;
        match *phase {
            Phase::Start => {
                let product = match settings.0.product(&checkout.root) {
                    Ok(product) => product,
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(error.to_string())));
                        continue;
                    }
                };
                let debug = checkout.directory("debug");
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CreateDirectory(debug.clone()),
                    Tag::None,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    InspectPath(debug.join(".vmux-dev-ready")),
                    Tag::Stamp,
                    false,
                );
                let alias = checkout
                    .directory("release")
                    .join(format!("{} Dev.app", product.value("VMUX_HOST_NAME")));
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    InspectPath(alias),
                    Tag::Path,
                    false,
                );
                *phase = Phase::ClearInspect;
            }
            Phase::ClearInspect => {
                let product = match settings.0.product(&checkout.root) {
                    Ok(product) => product,
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(error.to_string())));
                        continue;
                    }
                };
                if let Some(PathMetadata(Some(_))) = facts.get(entity, Tag::Stamp) {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        RemoveFile(checkout.directory("debug").join(".vmux-dev-ready")),
                        Tag::None,
                        false,
                    );
                }
                if let Some(PathMetadata(Some(metadata))) = facts.get(entity, Tag::Path)
                    && metadata.file_type().is_symlink()
                {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        RemoveFile(
                            checkout
                                .directory("release")
                                .join(format!("{} Dev.app", product.value("VMUX_HOST_NAME"))),
                        ),
                        Tag::None,
                        false,
                    );
                }
                *phase = Phase::ClearFinish;
            }
            Phase::ClearFinish => {
                commands.entity(entity).insert(Complete(Outcome::Done));
            }
            _ => {}
        }
    }
}

fn sign(mut requests: Requests, roots: Query<&Root>, children: Signatures, mut commands: Commands) {
    for (entity, request, checkout, settings, mut phase, _) in &mut requests {
        if !matches!(request, Request::Sign) {
            continue;
        }
        if *phase == Phase::Start {
            let product = match settings.0.product(&checkout.root) {
                Ok(product) => product,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message(error.to_string())));
                    continue;
                }
            };
            let framework = match settings.0.get("CEF_FRAMEWORK_DIR") {
                Some(path) => PathBuf::from(path),
                None => match settings.0.get("HOME") {
                    Some(path) => PathBuf::from(path)
                        .join(".local/share/Chromium Embedded Framework.framework"),
                    None => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message("Missing HOME")));
                        continue;
                    }
                },
            };
            for (path, identifier) in [
                (
                    framework.join("Libraries/bevy_cef_debug_render_process"),
                    format!(
                        "{}.dev.helper.bevy-cef-debug-render-process",
                        product.value("VMUX_HOST_BUNDLE_ID")
                    ),
                ),
                (
                    checkout.directory("debug").join("vmux_desktop"),
                    format!("{}.dev", product.value("VMUX_HOST_BUNDLE_ID")),
                ),
            ] {
                let signature = commands
                    .spawn((
                        Request::Signature {
                            path: path.clone(),
                            binary: path.clone(),
                            stamp: path.with_extension("dev-signed"),
                            identifier: Some(identifier.clone()),
                            suffix: format!("\n{identifier}"),
                        },
                        checkout.clone(),
                        settings.clone(),
                        Root(roots.get(entity).unwrap().0),
                        Phase::Start,
                        Sequence(0),
                        Sources,
                        Owner(entity),
                        ChildOf(entity),
                    ))
                    .id();
                commands.spawn((Input(path), Order(0), ChildOf(signature)));
            }
            *phase = Phase::SignWaiting;
        } else {
            let mut pending = false;
            for (parent, complete, failed) in &children {
                if parent.parent() != entity {
                    continue;
                }
                if let Some(failed) = failed {
                    commands.entity(entity).insert(failed.clone());
                    pending = true;
                    break;
                }
                pending |= complete.is_none();
            }
            if !pending {
                commands.entity(entity).insert(Complete(Outcome::Done));
            }
        }
    }
}

fn bundle(
    mut requests: Requests,
    facts: Facts,
    roots: Query<&Root>,
    layouts: Query<&Layout>,
    signatures: Signatures,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if !matches!(request, Request::Bundle) || facts.pending(entity) {
            continue;
        }
        let previous_phase = *phase;
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Failed(Failure::message(error.to_string())));
                continue;
            }
        };
        let debug = checkout.directory("debug");
        let mut previous = None;
        match *phase {
            Phase::Start => {
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    ReadFile(checkout.root.join("Cargo.toml")),
                    Tag::Manifest,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(
                        &settings.0,
                        &checkout.root,
                        "git".into(),
                        ["symbolic-ref", "--quiet", "--short", "HEAD"]
                            .map(OsString::from)
                            .into(),
                    ),
                    Tag::Branch,
                    true,
                );
                *phase = Phase::BundleBranch;
            }
            Phase::BundleBranch => {
                let Some(ToolOutput(output)) = facts.get(entity, Tag::Branch) else {
                    continue;
                };
                let branch = facts.text(entity, Tag::Branch).unwrap_or_default();
                if !output.status.success() || branch.trim().is_empty() {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "git".into(),
                            ["rev-parse", "--short=7", "HEAD"]
                                .map(OsString::from)
                                .into(),
                        ),
                        Tag::Commit,
                        true,
                    );
                }
                *phase = Phase::BundlePlan;
            }
            Phase::BundlePlan => {
                let Some(manifest) = facts.text(entity, Tag::Manifest) else {
                    continue;
                };
                let manifest: toml::Table = match manifest.parse() {
                    Ok(manifest) => manifest,
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(error.to_string())));
                        continue;
                    }
                };
                let Some(version) = manifest
                    .get("workspace")
                    .and_then(|value| value.get("package"))
                    .and_then(|value| value.get("version"))
                    .and_then(toml::Value::as_str)
                else {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message("Missing workspace version")));
                    continue;
                };
                let branch = facts.text(entity, Tag::Branch).unwrap_or_default();
                let commit = facts.text(entity, Tag::Commit).unwrap_or_default();
                let source = if !branch.trim().is_empty() {
                    branch.trim()
                } else if !commit.trim().is_empty() {
                    commit.trim()
                } else {
                    "unknown"
                };
                let title = serde_json::to_string(&format!(
                    "{} Dev ({source})",
                    product.value("VMUX_HOST_NAME")
                ))
                .unwrap();
                let app = debug.join(format!("{} Dev.app", product.value("VMUX_HOST_NAME")));
                let contents = app.join("Contents");
                let plist = contents.join("Info.plist");
                let service = contents
                    .join("Library/LoginItems")
                    .join(product.value("VMUX_HOST_SERVICE_BUNDLE"));
                let service_contents = service.join("Contents");
                let service_plist = service_contents.join("Info.plist");
                for directory in [
                    contents.join("MacOS"),
                    contents.join("Resources"),
                    contents.join("Frameworks"),
                    service_contents.join("MacOS"),
                ] {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CreateDirectory(directory),
                        Tag::None,
                        false,
                    );
                }
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CopyFile(
                        checkout.root.join("packaging/macos/Info.plist"),
                        plist.clone(),
                    ),
                    Tag::None,
                    false,
                );
                let mut updates = vec![
                    format!(
                        "Set :CFBundleIdentifier {}.dev",
                        product.value("VMUX_HOST_BUNDLE_ID")
                    ),
                    format!("Set :CFBundleDisplayName {title}"),
                    format!("Set :CFBundleName {title}"),
                    format!(
                        "Merge {}",
                        serde_json::to_string(product.value("VMUX_HOST_USAGE_DESCRIPTIONS"))
                            .unwrap()
                    ),
                    format!("Set :CFBundleShortVersionString {version}"),
                    format!("Set :CFBundleVersion {version}"),
                ];
                for update in updates.drain(..) {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            settings
                                .0
                                .get("PLIST_BUDDY")
                                .unwrap_or(OsStr::new("/usr/libexec/PlistBuddy"))
                                .to_owned(),
                            vec!["-c".into(), update.into(), plist.as_os_str().into()],
                        ),
                        Tag::None,
                        false,
                    );
                }
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CopyFile(
                        product.value("VMUX_HOST_ICON").into(),
                        contents.join("Resources/Vmux.icns"),
                    ),
                    Tag::None,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CopyFile(
                        product.value("VMUX_HOST_TRAY_ICON").into(),
                        contents.join("Resources/tray.png"),
                    ),
                    Tag::None,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CopyFile(debug.join("vmux"), contents.join("MacOS/vmux")),
                    Tag::None,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CopyFile(plist.clone(), service_plist.clone()),
                    Tag::None,
                    false,
                );
                for key in ["CFBundleExecutable", "CFBundleName", "CFBundleDisplayName"] {
                    let update = format!(
                        "Set :{key} {}",
                        serde_json::to_string(product.value("VMUX_HOST_SERVICE_NAME")).unwrap()
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            settings
                                .0
                                .get("PLIST_BUDDY")
                                .unwrap_or(OsStr::new("/usr/libexec/PlistBuddy"))
                                .to_owned(),
                            vec!["-c".into(), update.into(), service_plist.as_os_str().into()],
                        ),
                        Tag::None,
                        false,
                    );
                }
                for update in [
                    format!(
                        "Set :CFBundleIdentifier {}.dev",
                        product.value("VMUX_HOST_SERVICE_ID")
                    ),
                    "Set :LSUIElement true".into(),
                ] {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            settings
                                .0
                                .get("PLIST_BUDDY")
                                .unwrap_or(OsStr::new("/usr/libexec/PlistBuddy"))
                                .to_owned(),
                            vec!["-c".into(), update.into(), service_plist.as_os_str().into()],
                        ),
                        Tag::None,
                        false,
                    );
                }
                let binary = service_contents
                    .join("MacOS")
                    .join(product.value("VMUX_HOST_SERVICE_NAME"));
                commands.entity(entity).insert(Layout {
                    app,
                    service,
                    binary,
                    plist,
                });
                *phase = Phase::BundleService;
            }
            Phase::BundleService => {
                let layout = layouts.get(entity).unwrap();
                let signature = commands
                    .spawn((
                        Request::Signature {
                            path: layout.service.clone(),
                            binary: layout.binary.clone(),
                            stamp: debug.join(".vmux-dev-service-signed"),
                            identifier: None,
                            suffix: String::new(),
                        },
                        CopyFrom(debug.join("vmux_service")),
                        checkout.clone(),
                        settings.clone(),
                        Root(roots.get(entity).unwrap().0),
                        Phase::Start,
                        Sequence(0),
                        Sources,
                        Owner(entity),
                        ChildOf(entity),
                    ))
                    .id();
                for (order, path) in [
                    debug.join("vmux_service"),
                    layout.service.join("Contents/Info.plist"),
                ]
                .into_iter()
                .enumerate()
                {
                    commands.spawn((Input(path), Order(order as u32), ChildOf(signature)));
                }
                *phase = Phase::BundleServiceWait;
            }
            Phase::BundleServiceWait | Phase::BundleWait => {
                let mut pending = false;
                for (parent, complete, failed) in &signatures {
                    if parent.parent() != entity {
                        continue;
                    }
                    if let Some(failed) = failed {
                        commands.entity(entity).insert(failed.clone());
                        pending = true;
                        break;
                    }
                    pending |= complete.is_none();
                }
                if pending {
                    continue;
                }
                if *phase == Phase::BundleWait {
                    commands.entity(entity).insert(Complete(Outcome::Done));
                    continue;
                }
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(
                        &settings.0,
                        &checkout.root,
                        settings
                            .0
                            .get("RUSTC")
                            .unwrap_or(OsStr::new("rustc"))
                            .to_owned(),
                        ["--print", "target-libdir"].map(OsString::from).into(),
                    ),
                    Tag::Stdlib,
                    false,
                );
                *phase = Phase::BundleDesktop;
            }
            Phase::BundleDesktop => {
                let Some(stdlib) = facts.text(entity, Tag::Stdlib) else {
                    continue;
                };
                let layout = layouts.get(entity).unwrap();
                let binary = layout.app.join("Contents/MacOS/vmux_desktop");
                let signature = commands
                    .spawn((
                        Request::Signature {
                            path: layout.app.clone(),
                            binary,
                            stamp: debug.join(".vmux-dev-desktop-signed"),
                            identifier: None,
                            suffix: format!(
                                "\n{}\n{}",
                                debug.join("deps").display(),
                                stdlib.trim()
                            ),
                        },
                        CopyFrom(debug.join("vmux_desktop")),
                        Linking(debug.join("deps")),
                        checkout.clone(),
                        settings.clone(),
                        Root(roots.get(entity).unwrap().0),
                        Phase::Start,
                        Sequence(0),
                        Sources,
                        Owner(entity),
                        ChildOf(entity),
                    ))
                    .id();
                for path in [debug.join("deps"), stdlib.trim().into()] {
                    commands.spawn((Rpath(path), Order(0), ChildOf(signature)));
                }
                for (order, path) in [
                    debug.join("vmux_desktop"),
                    layout.app.join("Contents/MacOS/vmux"),
                    layout.plist.clone(),
                    layout.app.join("Contents/Resources/Vmux.icns"),
                    debug.join(".vmux-dev-service-signed"),
                ]
                .into_iter()
                .enumerate()
                {
                    commands.spawn((Input(path), Order(order as u32), ChildOf(signature)));
                }
                *phase = Phase::BundleWait;
            }
            _ => {}
        }

        if *phase != previous_phase {
            wake.0.wake_by_ref();
        }
    }
}

fn publish(
    mut requests: Requests,
    facts: Facts,
    files: Query<(&File, &Order, &ChildOf)>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if !matches!(request, Request::Publish) || facts.pending(entity) {
            continue;
        }
        let previous_phase = *phase;
        let debug = checkout.directory("debug");
        let mut previous = None;
        match *phase {
            Phase::Start => {
                let mut paths = vec![
                    debug.join("vmux"),
                    debug.join("vmux_service"),
                    debug.join("vmux_desktop"),
                ];
                if cfg!(target_os = "macos") {
                    let product = match settings.0.product(&checkout.root) {
                        Ok(product) => product,
                        Err(error) => {
                            commands
                                .entity(entity)
                                .insert(Failed(Failure::message(error.to_string())));
                            continue;
                        }
                    };
                    let name = format!("{} Dev.app", product.value("VMUX_HOST_NAME"));
                    paths.push(debug.join(&name).join("Contents/MacOS/vmux_desktop"));
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        InspectPath(checkout.directory("release").join(name)),
                        Tag::Path,
                        false,
                    );
                }
                for (order, path) in paths.into_iter().enumerate() {
                    commands.spawn((File(path.clone()), Order(order as u32), ChildOf(entity)));
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        InspectPath(path.clone()),
                        Tag::Library(order as u32),
                        false,
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        HashFile(path),
                        Tag::Input(order as u32),
                        false,
                    );
                }
                *phase = Phase::PublishCheck;
            }
            Phase::PublishCheck => {
                let mut sources = files
                    .iter()
                    .filter(|(_, _, parent)| parent.parent() == entity)
                    .collect::<Vec<_>>();
                sources.sort_by_key(|(_, order, _)| order.0);
                let mut stamp = format!("{}\n", checkout.root.display());
                let mut failed = false;
                for (file, order, _) in sources {
                    let valid = matches!(facts.get(entity, Tag::Library(order.0)), Some(PathMetadata(Some(metadata))) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0);
                    if !valid {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(format!(
                                "Missing executable: {}",
                                file.0.display()
                            ))));
                        failed = true;
                        break;
                    }
                    let Some(checksum) = facts.text(entity, Tag::Input(order.0)) else {
                        failed = true;
                        break;
                    };
                    stamp.push_str(&format!("{checksum}  {}\n", file.0.display()));
                }
                if failed {
                    continue;
                }
                if cfg!(target_os = "macos") {
                    if !matches!(facts.get(entity, Tag::Path), Some(PathMetadata(None))) {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "Cannot publish debug app over existing artifact",
                        )));
                        continue;
                    }
                    let product = match settings.0.product(&checkout.root) {
                        Ok(product) => product,
                        Err(error) => {
                            commands
                                .entity(entity)
                                .insert(Failed(Failure::message(error.to_string())));
                            continue;
                        }
                    };
                    let name = format!("{} Dev.app", product.value("VMUX_HOST_NAME"));
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CreateDirectory(checkout.directory("release")),
                        Tag::None,
                        false,
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        LinkPath(debug.join(&name), checkout.directory("release").join(name)),
                        Tag::None,
                        false,
                    );
                }
                let temporary = debug.join(".vmux-dev-ready.tmp");
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    WriteFile(temporary.clone(), stamp.into_bytes()),
                    Tag::None,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    RenamePath(temporary, debug.join(".vmux-dev-ready")),
                    Tag::None,
                    false,
                );
                *phase = Phase::PublishDone;
            }
            Phase::PublishDone => {
                println!("Debug build ready: {}\nRun: vmux run", debug.display());
                let path = if cfg!(target_os = "macos") {
                    let product = match settings.0.product(&checkout.root) {
                        Ok(product) => product,
                        Err(error) => {
                            commands
                                .entity(entity)
                                .insert(Failed(Failure::message(error.to_string())));
                            continue;
                        }
                    };
                    debug.join(format!("{} Dev.app", product.value("VMUX_HOST_NAME")))
                } else {
                    debug.join("vmux_desktop")
                };
                commands
                    .entity(entity)
                    .insert(Complete(Outcome::Artifacts(vec![path])));
            }
            _ => {}
        }

        if *phase != previous_phase {
            wake.0.wake_by_ref();
        }
    }
}

fn run(
    mut requests: Requests,
    facts: Facts,
    targets: Query<&Target>,
    checks: Query<(&File, &Expected, &Order, &ChildOf)>,
    entries: Query<(&crate::local::Entry, &ChildOf)>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        let Request::Run { profile, platform } = request else {
            continue;
        };
        if facts.pending(entity) {
            continue;
        }
        let previous_phase = *phase;
        let mut previous = None;
        if platform != "desktop" {
            if *phase == Phase::Start {
                let cwd = if platform == "web" {
                    checkout.root.join("website")
                } else {
                    checkout.root.clone()
                };
                if platform == "web" {
                    let arguments = [
                        "-i",
                        "tailwind.input.css",
                        "-o",
                        "public/style.css",
                        "--watch",
                    ]
                    .map(OsString::from)
                    .to_vec();
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        SpawnProcess::new(
                            &settings.0,
                            &cwd,
                            settings
                                .0
                                .get("TAILWINDCSS_BIN")
                                .unwrap_or(OsStr::new("tailwindcss"))
                                .to_owned(),
                            arguments,
                        ),
                        Tag::None,
                        false,
                    );
                }
                let arguments = if platform == "web" {
                    vec!["serve".into(), "--platform".into(), "web".into()]
                } else {
                    vec![
                        "serve".into(),
                        "-p".into(),
                        "vmux_mobile".into(),
                        format!("--{platform}").into(),
                    ]
                };
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    RunProcess::new(
                        &settings.0,
                        &cwd,
                        settings
                            .0
                            .get("DX_BIN")
                            .unwrap_or(OsStr::new("dx"))
                            .to_owned(),
                        arguments,
                    ),
                    Tag::None,
                    false,
                );
                *phase = Phase::LaunchDone;
            } else {
                commands.entity(entity).insert(Complete(Outcome::Done));
            }
        } else {
            let product = match settings.0.product(&checkout.root) {
                Ok(product) => product,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert(Failed(Failure::message(error.to_string())));
                    continue;
                }
            };
            match *phase {
                Phase::Start => {
                    match profile.as_str() {
                        "local" => sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            InspectPath(checkout.directory("release")),
                            Tag::Directory,
                            false,
                        ),
                        "dev" => sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            ReadFile(checkout.directory("debug").join(".vmux-dev-ready")),
                            Tag::Stamp,
                            false,
                        ),
                        _ => {
                            let target = checkout
                                .directory("release")
                                .join(format!("{}.app", product.value("VMUX_HOST_NAME")));
                            sequence.request(
                                &mut commands,
                                entity,
                                &mut previous,
                                InspectPath(target.clone()),
                                Tag::Path,
                                false,
                            );
                            commands.entity(entity).insert(Target(target));
                        }
                    }
                    *phase = Phase::LaunchInspect;
                }
                Phase::LaunchInspect => match profile.as_str() {
                    "local" => {
                        if !matches!(facts.get(entity, Tag::Directory), Some(PathMetadata(Some(metadata))) if metadata.is_dir())
                        {
                            commands.entity(entity).insert(Failed(Failure::message(
                                "No completed local app; run vmux build -p local first",
                            )));
                            continue;
                        }
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            ReadDirectory(checkout.directory("release")),
                            Tag::Entries,
                            false,
                        );
                        *phase = Phase::LaunchInputs;
                    }
                    "dev" => {
                        let Some(stamp) = facts.text(entity, Tag::Stamp) else {
                            continue;
                        };
                        let mut lines = stamp.lines();
                        if lines.next() != checkout.root.to_str() {
                            commands.entity(entity).insert(Failed(Failure::message(
                                "Artifact belongs to another checkout",
                            )));
                            continue;
                        }
                        let mut failed = false;
                        let mut count = 0;
                        for line in lines {
                            let Some((checksum, path)) = line.split_once("  ") else {
                                commands
                                    .entity(entity)
                                    .insert(Failed(Failure::message("Invalid readiness stamp")));
                                failed = true;
                                break;
                            };
                            let path = PathBuf::from(path);
                            if !path.starts_with(checkout.directory("debug")) {
                                commands.entity(entity).insert(Failed(Failure::message("Development artifact is incomplete or has changed; run vmux build")));
                                failed = true;
                                break;
                            }
                            commands.spawn((
                                File(path.clone()),
                                Expected(checksum.into()),
                                Order(count),
                                ChildOf(entity),
                            ));
                            sequence.request(
                                &mut commands,
                                entity,
                                &mut previous,
                                HashFile(path),
                                Tag::Input(count),
                                false,
                            );
                            count += 1;
                        }
                        if failed {
                            continue;
                        }
                        if count != if cfg!(target_os = "macos") { 4 } else { 3 } {
                            commands
                                .entity(entity)
                                .insert(Failed(Failure::message("Incomplete readiness stamp")));
                            continue;
                        }
                        *phase = Phase::LaunchInputs;
                    }
                    _ => {
                        if !matches!(facts.get(entity, Tag::Path), Some(PathMetadata(Some(metadata))) if metadata.is_dir())
                        {
                            commands.entity(entity).insert(Failed(Failure::message(
                                "No completed app; run vmux build first",
                            )));
                            continue;
                        }
                        *phase = Phase::LaunchVerify;
                    }
                },
                Phase::LaunchInputs => {
                    if profile == "local" {
                        let Some(DirectoryEntries(snapshot)) = facts.get(entity, Tag::Entries)
                        else {
                            continue;
                        };
                        let mut failed = false;
                        for (path, metadata) in snapshot {
                            let modified = match metadata.modified() {
                                Ok(modified) => modified,
                                Err(error) => {
                                    commands
                                        .entity(entity)
                                        .insert(Failed(Failure::message(error.to_string())));
                                    failed = true;
                                    break;
                                }
                            };
                            commands.spawn((
                                crate::local::Entry {
                                    path: path.clone(),
                                    modified,
                                    directory: metadata.is_dir(),
                                    symlink: metadata.file_type().is_symlink(),
                                },
                                ChildOf(entity),
                            ));
                        }
                        if failed {
                            continue;
                        }
                        *phase = Phase::LaunchSelect;
                    } else {
                        let mut valid = true;
                        for (_, expected, order, parent) in &checks {
                            if parent.parent() != entity {
                                continue;
                            }
                            if facts.text(entity, Tag::Input(order.0)).as_deref()
                                != Some(expected.0.as_str())
                            {
                                valid = false;
                                break;
                            }
                        }
                        if !valid {
                            commands.entity(entity).insert(Failed(Failure::message(
                                "Development artifact is incomplete or has changed; run vmux build",
                            )));
                            continue;
                        }
                        let path = if cfg!(target_os = "macos") {
                            checkout
                                .directory("debug")
                                .join(format!("{} Dev.app", product.value("VMUX_HOST_NAME")))
                        } else {
                            checkout.directory("debug").join("vmux_desktop")
                        };
                        commands.entity(entity).insert(Target(path));
                        *phase = if cfg!(target_os = "macos") {
                            Phase::LaunchVerify
                        } else {
                            Phase::LaunchExecute
                        };
                    }
                }
                Phase::LaunchSelect => {
                    let prefix = format!("{} (", product.value("VMUX_HOST_NAME"));
                    let mut latest = None;
                    for (entry, parent) in &entries {
                        if parent.parent() != entity || !entry.directory || entry.symlink {
                            continue;
                        }
                        let Some(name) = entry.path.file_name().and_then(|name| name.to_str())
                        else {
                            continue;
                        };
                        if !name.starts_with(&prefix) || !name.ends_with(").app") {
                            continue;
                        }
                        if latest.is_none_or(|previous: &crate::local::Entry| {
                            entry.modified > previous.modified
                        }) {
                            latest = Some(entry);
                        }
                    }
                    let Some(latest) = latest else {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "No completed local app; run vmux build -p local first",
                        )));
                        continue;
                    };
                    commands.entity(entity).insert(Target(latest.path.clone()));
                    *phase = Phase::LaunchVerify;
                }
                Phase::LaunchVerify => {
                    let target = targets.get(entity).unwrap();
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "codesign".into(),
                            vec![
                                "--verify".into(),
                                "--strict".into(),
                                target.0.as_os_str().into(),
                            ],
                        ),
                        Tag::None,
                        false,
                    );
                    *phase = Phase::LaunchExecute;
                }
                Phase::LaunchExecute => {
                    let target = targets.get(entity).unwrap();
                    let mut environment = settings.0.clone();
                    let (program, arguments) = if cfg!(target_os = "macos") || profile != "dev" {
                        let mut arguments = Vec::new();
                        for key in [
                            "VMUX_PROFILE",
                            "VMUX_TEST",
                            "VMUX_DATA_DIR",
                            "VMUX_REMOTE_RELAY_URL",
                        ] {
                            if let Some(value) = environment.get(key) {
                                let mut argument = OsString::from(key);
                                argument.push("=");
                                argument.push(value);
                                arguments.extend(["--env".into(), argument]);
                            }
                        }
                        arguments.push(target.0.as_os_str().into());
                        (OsString::from("open"), arguments)
                    } else {
                        environment.remove("CEF_PATH");
                        (target.0.as_os_str().to_owned(), Vec::new())
                    };
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        RunProcess::new(&environment, &checkout.root, program, arguments),
                        Tag::None,
                        false,
                    );
                    *phase = Phase::LaunchDone;
                }
                Phase::LaunchDone => {
                    commands.entity(entity).insert(Complete(Outcome::Done));
                }
                _ => {}
            }
        }

        if *phase != previous_phase {
            wake.0.wake_by_ref();
        }
    }
}

fn stop(mut requests: Requests, facts: Facts, wake: Res<JobWake>, mut commands: Commands) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if !matches!(request, Request::Stop) || facts.pending(entity) {
            continue;
        }
        let previous_phase = *phase;
        let mut previous = None;
        match *phase {
            Phase::Start => {
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(
                        &settings.0,
                        &checkout.root,
                        settings
                            .0
                            .get("RUSTC")
                            .unwrap_or(OsStr::new("rustc"))
                            .to_owned(),
                        ["--print", "target-libdir"].map(OsString::from).into(),
                    ),
                    Tag::None,
                    false,
                );
                *phase = Phase::StopExecute;
            }
            Phase::StopExecute => {
                let Some(stdlib) = facts.text(entity, Tag::Stdlib) else {
                    continue;
                };
                let debug = checkout.directory("debug");
                let mut environment = settings.0.clone();
                environment.remove("CEF_PATH");
                environment.set(
                    "DYLD_LIBRARY_PATH",
                    format!(
                        "{}:{}:{}",
                        stdlib.trim(),
                        debug.join("deps").display(),
                        settings
                            .0
                            .get("DYLD_LIBRARY_PATH")
                            .unwrap_or(OsStr::new(""))
                            .to_string_lossy()
                    ),
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    RunProcess::new(
                        &environment,
                        &checkout.root,
                        debug.join("vmux").into_os_string(),
                        ["service", "stop"].map(OsString::from).into(),
                    ),
                    Tag::None,
                    false,
                );
                *phase = Phase::StopDone;
            }
            Phase::StopDone => {
                commands.entity(entity).insert(Complete(Outcome::Done));
            }
            _ => {}
        }

        if *phase != previous_phase {
            wake.0.wake_by_ref();
        }
    }
}

#[derive(SystemParam)]
struct SignatureData<'w, 's> {
    inputs: Query<'w, 's, (&'static Input, &'static Order, &'static ChildOf)>,
    expected: Query<'w, 's, &'static Expected>,
    preparation: Query<
        'w,
        's,
        (
            Option<&'static CopyFrom>,
            Option<&'static Copied>,
            Option<&'static Linking>,
        ),
    >,
    links: Query<
        'w,
        's,
        (
            Option<&'static Rpath>,
            Option<&'static Binding>,
            &'static Order,
            &'static ChildOf,
        ),
    >,
}

fn signature(
    mut requests: Requests,
    facts: Facts,
    spec: SignatureData,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        let Request::Signature {
            path,
            binary,
            stamp,
            identifier,
            suffix,
        } = request
        else {
            continue;
        };
        if facts.pending(entity) {
            continue;
        }
        let previous_phase = *phase;
        let product = match settings.0.product(&checkout.root) {
            Ok(product) => product,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(Failed(Failure::message(error.to_string())));
                continue;
            }
        };
        let mut previous = None;
        match *phase {
            Phase::Start | Phase::SignatureRefresh => {
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    HashFile(product.value("VMUX_HOST_DEV_ENTITLEMENTS").into()),
                    Tag::Entitlements,
                    false,
                );
                let mut sources = spec
                    .inputs
                    .iter()
                    .filter(|(_, _, parent)| parent.parent() == entity)
                    .collect::<Vec<_>>();
                sources.sort_by_key(|(_, order, _)| order.0);
                for (input, order, _) in sources {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        HashFile(input.0.clone()),
                        Tag::Input(order.0),
                        false,
                    );
                }
                if *phase == Phase::Start {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        InspectPath(stamp.clone()),
                        Tag::Stamp,
                        false,
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        InspectPath(binary.clone()),
                        Tag::Binary,
                        false,
                    );
                    *phase = Phase::SignatureInputs;
                } else {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        HashFile(binary.clone()),
                        Tag::Binary,
                        false,
                    );
                    *phase = Phase::SignatureRecord;
                }
            }
            Phase::SignatureInputs | Phase::SignatureRecord => {
                let identity = match settings.0.required("APPLE_SIGNING_IDENTITY") {
                    Ok(identity) => identity,
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(error.to_string())));
                        continue;
                    }
                };
                let mut hash = Sha256::new();
                hash.update(identity.as_encoded_bytes());
                let Some(entitlements) = facts.text(entity, Tag::Entitlements) else {
                    continue;
                };
                hash.update(entitlements.as_bytes());
                let mut sources = spec
                    .inputs
                    .iter()
                    .filter(|(_, _, parent)| parent.parent() == entity)
                    .collect::<Vec<_>>();
                sources.sort_by_key(|(_, order, _)| order.0);
                let mut pending = false;
                for (_, order, _) in sources {
                    let Some(input) = facts.text(entity, Tag::Input(order.0)) else {
                        pending = true;
                        break;
                    };
                    hash.update(input.as_bytes());
                }
                if pending {
                    continue;
                }
                let value = format!("{:x}{suffix}", hash.finalize());
                commands.entity(entity).insert(Expected(value.clone()));
                if *phase == Phase::SignatureRecord {
                    let Some(checksum) = facts.text(entity, Tag::Binary) else {
                        continue;
                    };
                    let temporary = stamp.with_extension("tmp");
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        WriteFile(
                            temporary.clone(),
                            format!("{value}\n{checksum}\n").into_bytes(),
                        ),
                        Tag::None,
                        false,
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        RenamePath(temporary, stamp.clone()),
                        Tag::None,
                        false,
                    );
                    *phase = Phase::SignatureDone;
                } else {
                    let exists = matches!(facts.get(entity, Tag::Stamp), Some(PathMetadata(Some(metadata))) if metadata.is_file())
                        && matches!(facts.get(entity, Tag::Binary), Some(PathMetadata(Some(metadata))) if metadata.is_file());
                    if exists {
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            ReadFile(stamp.clone()),
                            Tag::Stamp,
                            false,
                        );
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            HashFile(binary.clone()),
                            Tag::Binary,
                            false,
                        );
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            CaptureOutput::new(
                                &settings.0,
                                &checkout.root,
                                "codesign".into(),
                                vec![
                                    "--verify".into(),
                                    "--strict".into(),
                                    path.as_os_str().into(),
                                ],
                            ),
                            Tag::Signature,
                            true,
                        );
                        *phase = Phase::SignatureCache;
                    } else {
                        *phase = Phase::SignaturePrepare;
                    }
                }
            }
            Phase::SignatureCache => {
                let Some(actual) = facts.text(entity, Tag::Stamp) else {
                    continue;
                };
                let Some(checksum) = facts.text(entity, Tag::Binary) else {
                    continue;
                };
                let Some(ToolOutput(output)) = facts.get(entity, Tag::Signature) else {
                    continue;
                };
                if actual == format!("{}\n{checksum}\n", spec.expected.get(entity).unwrap().0)
                    && output.status.success()
                {
                    commands.entity(entity).insert(Complete(Outcome::Done));
                    continue;
                }
                *phase = Phase::SignaturePrepare;
            }
            Phase::SignaturePrepare => {
                let (copy, copied, linking) = spec.preparation.get(entity).unwrap();
                if let Some(copy) = copy
                    && copied.is_none()
                {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CreateDirectory(binary.parent().unwrap().to_owned()),
                        Tag::None,
                        false,
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CopyFile(copy.0.clone(), binary.clone()),
                        Tag::None,
                        false,
                    );
                    commands.entity(entity).insert(Copied);
                    if linking.is_some() {
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            CaptureOutput::new(
                                &settings.0,
                                &checkout.root,
                                "otool".into(),
                                vec!["-l".into(), binary.as_os_str().into()],
                            ),
                            Tag::Rpaths,
                            false,
                        );
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            CaptureOutput::new(
                                &settings.0,
                                &checkout.root,
                                "otool".into(),
                                vec!["-L".into(), binary.as_os_str().into()],
                            ),
                            Tag::Libraries,
                            false,
                        );
                        *phase = Phase::SignatureRpaths;
                    }
                } else if identifier.is_some() {
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "file".into(),
                            vec![binary.as_os_str().into()],
                        ),
                        Tag::Format,
                        false,
                    );
                    *phase = Phase::SignatureFormat;
                } else {
                    let identity = match settings.0.required("APPLE_SIGNING_IDENTITY") {
                        Ok(identity) => identity,
                        Err(error) => {
                            commands
                                .entity(entity)
                                .insert(Failed(Failure::message(error.to_string())));
                            continue;
                        }
                    };
                    let mut arguments = vec![
                        "--force".into(),
                        "--sign".into(),
                        identity.to_owned(),
                        "--options".into(),
                        "runtime".into(),
                        "--entitlements".into(),
                        product.value("VMUX_HOST_DEV_ENTITLEMENTS").into(),
                    ];
                    if let Some(keychain) = settings.0.get("CODESIGN_KEYCHAIN") {
                        arguments.extend(["--keychain".into(), keychain.to_owned()]);
                    }
                    arguments.push(path.as_os_str().into());
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "codesign".into(),
                            arguments,
                        ),
                        Tag::None,
                        false,
                    );
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "codesign".into(),
                            vec![
                                "--verify".into(),
                                "--strict".into(),
                                path.as_os_str().into(),
                            ],
                        ),
                        Tag::None,
                        false,
                    );
                    *phase = Phase::SignatureRefresh;
                }
            }
            Phase::SignatureFormat => {
                let Some(format) = facts.text(entity, Tag::Format) else {
                    continue;
                };
                if !format.contains("Mach-O") {
                    commands.entity(entity).insert(Failed(Failure::message(
                        "Cannot sign a non-Mach-O development binary",
                    )));
                    continue;
                }
                let identity = match settings.0.required("APPLE_SIGNING_IDENTITY") {
                    Ok(identity) => identity,
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(error.to_string())));
                        continue;
                    }
                };
                let mut arguments = vec![
                    "--force".into(),
                    "--sign".into(),
                    identity.to_owned(),
                    "--options".into(),
                    "runtime".into(),
                    "--entitlements".into(),
                    product.value("VMUX_HOST_DEV_ENTITLEMENTS").into(),
                ];
                if let Some(keychain) = settings.0.get("CODESIGN_KEYCHAIN") {
                    arguments.extend(["--keychain".into(), keychain.to_owned()]);
                }
                arguments.extend([
                    "--identifier".into(),
                    identifier.as_ref().unwrap().as_str().into(),
                    path.as_os_str().into(),
                ]);
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(&settings.0, &checkout.root, "codesign".into(), arguments),
                    Tag::None,
                    false,
                );
                sequence.request(
                    &mut commands,
                    entity,
                    &mut previous,
                    CaptureOutput::new(
                        &settings.0,
                        &checkout.root,
                        "codesign".into(),
                        vec![
                            "--verify".into(),
                            "--strict".into(),
                            path.as_os_str().into(),
                        ],
                    ),
                    Tag::None,
                    false,
                );
                *phase = Phase::SignatureRefresh;
            }
            Phase::SignatureDone => {
                commands.entity(entity).insert(Complete(Outcome::Done));
            }
            Phase::SignatureRpaths => {
                let Some(rpaths) = facts.text(entity, Tag::Rpaths) else {
                    continue;
                };
                let Some(libraries) = facts.text(entity, Tag::Libraries) else {
                    continue;
                };
                let (_, _, linking) = spec.preparation.get(entity).unwrap();
                let directory = &linking.unwrap().0;
                for (rpath, _, _, parent) in &spec.links {
                    let Some(rpath) = rpath else {
                        continue;
                    };
                    if parent.parent() != entity
                        || rpaths.contains(&format!("path {} (", rpath.0.display()))
                    {
                        continue;
                    }
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        CaptureOutput::new(
                            &settings.0,
                            &checkout.root,
                            "install_name_tool".into(),
                            vec![
                                "-add_rpath".into(),
                                rpath.0.as_os_str().into(),
                                binary.as_os_str().into(),
                            ],
                        ),
                        Tag::None,
                        false,
                    );
                }
                let mut order = 0;
                for line in libraries.lines() {
                    if !line.contains("libbevy_dylib") {
                        continue;
                    }
                    let Some(original) = line.trim().split(" (compatibility").next() else {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message("Malformed library path")));
                        break;
                    };
                    let Some(name) = Path::new(original).file_name() else {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message("Missing library name")));
                        break;
                    };
                    let replacement = format!("@rpath/{}", name.to_string_lossy());
                    let path = directory.join(name);
                    commands.spawn((
                        Binding {
                            original: original.into(),
                            replacement,
                        },
                        Order(order),
                        ChildOf(entity),
                    ));
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        InspectPath(path),
                        Tag::Library(order),
                        false,
                    );
                    order += 1;
                }
                *phase = Phase::SignatureBindings;
            }
            Phase::SignatureBindings => {
                for (_, binding, order, parent) in &spec.links {
                    let Some(binding) = binding else {
                        continue;
                    };
                    if parent.parent() != entity {
                        continue;
                    }
                    if !matches!(facts.get(entity, Tag::Library(order.0)), Some(PathMetadata(Some(metadata))) if metadata.is_file())
                    {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message("Missing shared Bevy library")));
                        break;
                    }
                    if binding.original != binding.replacement {
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            CaptureOutput::new(
                                &settings.0,
                                &checkout.root,
                                "install_name_tool".into(),
                                vec![
                                    "-change".into(),
                                    binding.original.as_str().into(),
                                    binding.replacement.as_str().into(),
                                    binary.as_os_str().into(),
                                ],
                            ),
                            Tag::None,
                            false,
                        );
                    }
                }
                *phase = Phase::SignaturePrepare;
            }
            _ => {}
        }

        if *phase != previous_phase {
            wake.0.wake_by_ref();
        }
    }
}

type WakeChanges<'w, 's> =
    Query<'w, 's, Entity, Or<(Changed<Phase>, Added<Complete>, Added<Failed>)>>;

fn provider(mut requests: Requests, facts: Facts, wake: Res<JobWake>, mut commands: Commands) {
    for (entity, request, checkout, settings, mut phase, mut sequence) in &mut requests {
        if facts.pending(entity) {
            continue;
        }
        let previous_phase = *phase;
        let mut previous = None;
        match request {
            Request::ProviderCompile {
                package,
                binary,
                manifest,
            } => {
                if *phase == Phase::Start {
                    let mut environment = settings.0.clone();
                    environment.remove("CARGO_BUILD_TARGET");
                    let arguments = vec![
                        "build".into(),
                        "--locked".into(),
                        "--manifest-path".into(),
                        manifest.as_os_str().into(),
                        "-p".into(),
                        package.as_str().into(),
                        "--bin".into(),
                        binary.as_str().into(),
                        "--target".into(),
                        env!("VMUX_DX_HOST_TARGET").into(),
                        "--target-dir".into(),
                        checkout.target.as_os_str().into(),
                    ];
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        RunProcess::new(&environment, &checkout.root, "cargo".into(), arguments),
                        Tag::None,
                        false,
                    );
                    *phase = Phase::ProviderCompiled;
                } else {
                    commands.entity(entity).insert(Complete(Outcome::Done));
                }
            }
            Request::ProviderSnapshot { binary } => {
                let source = checkout.directory("debug").join(binary);
                match *phase {
                    Phase::Start => {
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            InspectPath(source),
                            Tag::Path,
                            false,
                        );
                        *phase = Phase::SnapshotInspect;
                    }
                    Phase::SnapshotInspect => {
                        let valid = matches!(facts.get(entity, Tag::Path), Some(PathMetadata(Some(metadata))) if metadata.is_file() && metadata.len() <= 64 * 1024 * 1024 && metadata.permissions().mode() & 0o111 != 0);
                        if !valid {
                            commands.entity(entity).insert(Failed(Failure::message(
                                "Workspace provider build produced no bounded executable",
                            )));
                            continue;
                        }
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            CreateTemporary,
                            Tag::Temporary,
                            false,
                        );
                        *phase = Phase::SnapshotCopy;
                    }
                    Phase::SnapshotCopy => {
                        let Some(TemporaryValue(temporary)) = facts.get(entity, Tag::Temporary)
                        else {
                            continue;
                        };
                        let path = temporary.path().join("provider");
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            CopyFile(source, path.clone()),
                            Tag::None,
                            false,
                        );
                        sequence.request(
                            &mut commands,
                            entity,
                            &mut previous,
                            InspectPath(path),
                            Tag::Path,
                            false,
                        );
                        *phase = Phase::SnapshotCheck;
                    }
                    Phase::SnapshotCheck => {
                        let valid = matches!(facts.get(entity, Tag::Path), Some(PathMetadata(Some(metadata))) if metadata.is_file() && metadata.len() <= 64 * 1024 * 1024 && metadata.permissions().mode() & 0o111 != 0);
                        if !valid {
                            commands.entity(entity).insert(Failed(Failure::message(
                                "Workspace executable snapshot is incomplete",
                            )));
                            continue;
                        }
                        let Some(TemporaryValue(temporary)) = facts.get(entity, Tag::Temporary)
                        else {
                            continue;
                        };
                        let executable = crate::workspace_io::Executable::new(
                            temporary.path().join("provider"),
                            temporary.clone(),
                        );
                        commands
                            .entity(entity)
                            .insert(Complete(Outcome::Executable(executable)));
                    }
                    _ => {}
                }
            }
            Request::ProviderInvoke { arguments } => {
                if *phase == Phase::Start {
                    let Some(program) = settings.0.get("VMUX_DX_PROVIDER_EXECUTABLE") else {
                        commands.entity(entity).insert(Failed(Failure::message(
                            "Workspace executable snapshot is missing",
                        )));
                        continue;
                    };
                    let mut environment = settings.0.clone();
                    environment.set("VMUX_DX_PROVIDER_CHILD", "1");
                    sequence.request(
                        &mut commands,
                        entity,
                        &mut previous,
                        RunProcess::new(
                            &environment,
                            &checkout.root,
                            program.to_owned(),
                            arguments.clone(),
                        ),
                        Tag::None,
                        false,
                    );
                    *phase = Phase::ProviderInvoked;
                } else {
                    commands.entity(entity).insert(Complete(Outcome::Done));
                }
            }
            _ => continue,
        }

        if *phase != previous_phase {
            wake.0.wake_by_ref();
        }
    }
}

fn wake(changes: WakeChanges, wake: Res<JobWake>) {
    if !changes.is_empty() {
        wake.0.wake_by_ref();
    }
}

type Pending<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ChildOf, Option<&'static After>),
    (
        With<Io>,
        Without<Submitted>,
        Without<Admitted>,
        Or<(
            With<CaptureOutput>,
            With<RunProcess>,
            With<SpawnProcess>,
            With<ReadFile>,
            With<HashFile>,
            With<InspectPath>,
            With<ReadDirectory>,
            With<CreateDirectory>,
            With<CopyFile>,
            With<RemoveFile>,
            With<LinkPath>,
            With<WriteFile>,
            With<RenamePath>,
            With<CreateTemporary>,
        )>,
    ),
>;

#[derive(SystemParam)]
struct Readiness<'w, 's> {
    active: Query<'w, 's, &'static OperationState, With<Io>>,
    owners: Query<'w, 's, (Option<&'static Failed>, Option<&'static Complete>), With<Request>>,
    states: Query<'w, 's, &'static OperationState>,
}

#[derive(SystemParam)]
struct Loans<'w, 's> {
    leases: Query<'w, 's, &'static Lease>,
    temporary: Query<'w, 's, (&'static TemporaryValue, &'static ChildOf)>,
}

fn admit(pending: Pending, readiness: Readiness, scope: Scope, mut commands: Commands) {
    if readiness
        .active
        .iter()
        .any(|state| *state == OperationState::Running)
    {
        return;
    }
    for (entity, parent, predecessor) in &pending {
        let Ok((failed, complete)) = readiness.owners.get(parent.parent()) else {
            continue;
        };
        if !scope.live(parent.parent()) || failed.is_some() || complete.is_some() {
            continue;
        }
        if predecessor.is_some_and(|previous| {
            !readiness
                .states
                .get(previous.0)
                .is_ok_and(|state| *state == OperationState::Succeeded)
        }) {
            continue;
        }
        commands
            .entity(entity)
            .insert((Admitted, OperationState::Running));
        break;
    }
}

type Accepted<'w, 's, J> = Query<
    'w,
    's,
    (Entity, &'static J, &'static ChildOf),
    (With<Io>, With<Admitted>, Without<Submitted>),
>;

fn submit<J: IoJob>(
    pending: Accepted<J>,
    scope: Scope,
    loans: Loans,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, job, parent) in &pending {
        if !scope.live(parent.parent()) {
            continue;
        }
        let mut current = parent.parent();
        let mut target = None;
        let mut temporary = Vec::new();
        loop {
            if let Ok(lease) = loans.leases.get(current) {
                target = lease.0.clone();
            }
            for (directory, owner) in &loans.temporary {
                if owner.parent() == current {
                    temporary.push(directory.0.clone());
                }
            }
            let Ok((root, owner, _, _)) = scope.requests.get(current) else {
                break;
            };
            if owner.0 == root.0 {
                break;
            }
            current = owner.0;
        }
        let (task, flight) = Task::new(job.clone(), target, temporary, wake.0.clone());
        commands.entity(entity).insert((
            Submitted,
            Root(scope.requests.get(parent.parent()).unwrap().0.0),
            flight,
            JobRequest(task),
        ));
        wake.0.wake_by_ref();
    }
}

type Releasing<'w, 's> = Query<'w, 's, Entity, (With<Request>, Or<(With<Complete>, With<Failed>)>)>;

fn release(
    requests: Releasing,
    results: Query<(Entity, &ChildOf), With<Io>>,
    flights: Query<&Flight>,
    mut commands: Commands,
) {
    if Signals::active() || flights.iter().any(Flight::pending) {
        return;
    }
    for entity in &requests {
        commands.entity(entity).remove::<Lease>();
    }
    for (entity, parent) in &results {
        if requests.contains(parent.parent()) {
            commands
                .entity(entity)
                .remove::<(TemporaryValue, ProcessValue, PendingValue)>();
        }
    }
}

fn finish(
    requests: Finishing,
    parents: Query<&ChildOf>,
    flights: Query<(&Root, &Flight)>,
    mut completions: MessageWriter<Completion>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, captured, complete, failed) in &requests {
        if complete.is_none() && failed.is_none() {
            continue;
        }
        if failed.is_some() && Signals::active()
            || flights
                .iter()
                .any(|(root, flight)| root.0 == captured.root && flight.pending())
        {
            continue;
        }
        if !parents
            .get(entity)
            .is_ok_and(|parent| parent.parent() == captured.root)
        {
            continue;
        }
        let result = match failed {
            Some(failed) => Err(failed.0.clone()),
            None => Ok(complete.unwrap().0.clone()),
        };
        completions.write(Completion {
            entity,
            attempt: captured.attempt,
            result,
        });
        commands.entity(entity).insert(Sent);
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::environment::Environment;
    use crate::policy::Policy;
    use crate::storage::Storage;
    use crate::work::Work;
    use vmux_ecs::cli::CliResult;

    struct Fixture {
        _directory: tempfile::TempDir,
        app: App,
        root: PathBuf,
        target: PathBuf,
        operation: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().canonicalize().unwrap();
            let target = root.join("target");
            fs::create_dir_all(target.join("debug")).unwrap();
            let packaging = root.join("packaging/macos");
            fs::create_dir_all(&packaging).unwrap();
            for name in [
                "Vmux.icns",
                "tray.png",
                "Vmux.entitlements",
                "VmuxDev.entitlements",
                "usage-descriptions.plist",
                "dmg-background.png",
            ] {
                fs::write(packaging.join(name), b"fixture asset").unwrap();
            }
            for name in [
                "Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png",
                "Info.plist",
                "LaunchScreen.storyboard",
                "PrivacyInfo.xcprivacy",
                "Vmux.entitlements",
            ] {
                let path = root.join("packaging/ios").join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, b"fixture asset").unwrap();
            }
            let bin = root.join("bin");
            fs::create_dir_all(&bin).unwrap();
            for (name, body) in [
                ("codesign", "#!/bin/sh\nexit 0\n"),
                ("open", "#!/bin/sh\nprintf opened > \"$TEST_ROOT/opened\"\n"),
            ] {
                let path = bin.join(name);
                fs::write(&path, body).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            let checkout = Checkout {
                root: root.clone(),
                target: target.clone(),
                build_target: None,
            };
            let mut environment = Environment::current();
            environment.set("VMUX_BUILD_STORAGE_MIN_GIB", "0");
            environment.set("VMUX_BUILD_STORAGE_WARN_GIB", "0");
            environment.set("PATH", format!("{}:/usr/bin:/bin", bin.display()));
            environment.set("TEST_ROOT", root.as_os_str());
            let storage = Storage::new(
                root.clone(),
                target.clone(),
                Policy::configured(&environment).unwrap(),
            )
            .unwrap();
            let mut app = App::new();
            app.add_plugins(crate::Plugin);
            let operation = app
                .world_mut()
                .spawn((
                    crate::Accepted,
                    checkout,
                    Settings(environment),
                    storage,
                    OperationState::Running,
                ))
                .id();
            Self {
                _directory: directory,
                app,
                root,
                target,
                operation,
            }
        }

        fn run(&mut self, work: Work) -> u8 {
            self.app
                .world_mut()
                .entity_mut(self.operation)
                .remove::<CliResult>()
                .insert(OperationState::Running);
            self.app.world_mut().spawn((
                work,
                crate::Attempt(1),
                OperationState::Pending,
                ChildOf(self.operation),
            ));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(result) = self.app.world().get::<CliResult>(self.operation) {
                    return result.0.as_ref().copied().unwrap_or(1);
                }
                assert!(Instant::now() < deadline, "debug operation did not finish");
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }

        fn io_owner(&mut self) -> Entity {
            let checkout = self
                .app
                .world()
                .get::<Checkout>(self.operation)
                .unwrap()
                .clone();
            let settings = self
                .app
                .world()
                .get::<Settings>(self.operation)
                .unwrap()
                .clone();
            self.app
                .world_mut()
                .spawn((
                    Request::Clear,
                    Captured {
                        root: self.operation,
                        attempt: 1,
                    },
                    Root(self.operation),
                    Owner(self.operation),
                    Phase::ClearFinish,
                    Sequence(0),
                    checkout,
                    settings,
                    ChildOf(self.operation),
                ))
                .id()
        }

        fn settle_io(&mut self, owner: Entity) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if self.app.world().get::<Sent>(owner).is_some() {
                    return;
                }
                assert!(Instant::now() < deadline, "typed IO did not finish");
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }

        fn executable(&self, path: &Path, bytes: &[u8]) {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn typed_io_preserves_dependency_order_and_releases_temporary_files_before_completion() {
        let mut fixture = Fixture::new();
        let owner = fixture.io_owner();
        let path = fixture.root.join("ordered-io");
        let world = fixture.app.world_mut();
        let write = world
            .spawn((
                Operation::bundle("dx.debug.io").unwrap(),
                Io,
                WriteFile(path.clone(), b"updated".to_vec()),
                Tag::Config,
                Order(0),
                ChildOf(owner),
            ))
            .id();
        let read = world
            .spawn((
                Operation::bundle("dx.debug.io").unwrap(),
                Io,
                ReadFile(path.clone()),
                Tag::Config,
                Order(1),
                ChildOf(owner),
                After(write),
            ))
            .id();
        let hash = world
            .spawn((
                Operation::bundle("dx.debug.io").unwrap(),
                Io,
                HashFile(path.clone()),
                Tag::Binary,
                Order(2),
                ChildOf(owner),
                After(read),
            ))
            .id();
        fixture.settle_io(owner);
        let world = fixture.app.world();
        assert_eq!(world.get::<FileBytes>(read).unwrap().0, b"updated");
        assert_eq!(
            world.get::<FileDigest>(hash).unwrap().0,
            format!("{:x}", Sha256::digest(b"updated"))
        );
        for entity in [write, read, hash] {
            assert_eq!(
                world.get::<OperationState>(entity),
                Some(&OperationState::Succeeded)
            );
            assert!(!world.get::<Flight>(entity).unwrap().pending());
        }
        assert!(!path.exists());
    }

    #[test]
    fn executable_snapshot_survives_removing_the_compiler_output() {
        let mut fixture = Fixture::new();
        let source = fixture.target.join("debug/provider");
        let bytes = b"#!/bin/sh\nprintf 'independent fixture'\n";
        fixture.executable(&source, bytes);
        assert_eq!(fixture.run(Work::WorkspaceSnapshot("provider".into())), 0);
        let executable = fixture
            .app
            .world()
            .get::<crate::workspace_io::Executable>(fixture.operation)
            .unwrap();
        fs::remove_file(source).unwrap();
        assert_eq!(fs::read(executable.path()).unwrap(), bytes);
        assert_ne!(
            fs::metadata(executable.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }

    #[test]
    fn published_readiness_covers_all_binaries_and_refuses_tampering_before_launch() {
        let mut fixture = Fixture::new();
        let debug = fixture.target.join("debug");
        for name in ["vmux", "vmux_service", "vmux_desktop"] {
            fixture.executable(&debug.join(name), name.as_bytes());
        }
        if cfg!(target_os = "macos") {
            fixture.executable(
                &debug.join("Vmux Dev.app/Contents/MacOS/vmux_desktop"),
                b"vmux_desktop",
            );
        }
        assert_eq!(fixture.run(Work::PublishDebug), 0);
        let stamp = fs::read_to_string(debug.join(".vmux-dev-ready")).unwrap();
        let mut lines = stamp.lines();
        assert_eq!(lines.next(), fixture.root.to_str());
        let mut count = 0;
        for line in lines {
            let (hash, path) = line.split_once("  ").unwrap();
            assert_eq!(
                hash,
                format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
            );
            count += 1;
        }
        assert_eq!(count, if cfg!(target_os = "macos") { 4 } else { 3 });
        fixture.executable(&debug.join("vmux_desktop"), b"changed executable");
        assert_ne!(fixture.run(Work::Run("dev".into(), "desktop".into())), 0);
        assert!(!fixture.root.join("opened").exists());
    }
}
