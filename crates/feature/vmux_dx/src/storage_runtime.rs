use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::{App, PreUpdate, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_ecs::jobs::{JobOwner, JobPlugin, JobRequest, JobResult, JobSet, JobWake};
use vmux_ecs::{Cwd, Operation as WorkOperation, OperationState};

use crate::debug_io::{Flight, Task};
use crate::execution::Failure;
use crate::policy::{GIB, Policy};
use crate::repository::Checkout;
use crate::signals::Signals;
use crate::storage::Storage;
use crate::storage_io::{
    CanonicalPath, Canonicalize, Capture, DirectoryBatch, DirectoryCursor, Done, FileBytes,
    Inspect, Io, Open, OpenedFile, PathMetadata, ReadDirectoryBatch, ReadFile, Remove, StorageJob,
    ToolOutput,
};
use crate::{Completion, Settings};

pub(crate) struct StorageRuntimePlugin;

impl bevy_app::Plugin for StorageRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            IoPlugin::<Inspect>::default(),
            IoPlugin::<Canonicalize>::default(),
            IoPlugin::<ReadDirectoryBatch>::default(),
            IoPlugin::<ReadFile>::default(),
            IoPlugin::<Open>::default(),
            IoPlugin::<Remove>::default(),
            IoPlugin::<Capture>::default(),
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
                status,
                initialize,
                cancel,
                validate,
                capacity,
                (
                    directory,
                    scan_roots,
                    scan_profiles,
                    scan_caches,
                    collect_caches,
                )
                    .chain(),
                processes,
                (collect_usage, usage).chain(),
                protect,
                locks,
                evict,
                (select, collect_deps, select_caches, retain_apps).chain(),
                (collect_removals, remove).chain(),
                report,
                wake,
            )
                .chain()
                .in_set(Stage::Decide),
        )
        .add_systems(Update, admit.in_set(Stage::Admit))
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

#[derive(Component, Clone)]
pub(crate) struct Request {
    kind: RequestKind,
    profile: Option<String>,
    apply: bool,
}

impl Request {
    pub(crate) fn new(kind: RequestKind) -> Self {
        Self {
            kind,
            profile: None,
            apply: false,
        }
    }

    pub(crate) fn finish(profile: String) -> Self {
        Self {
            kind: RequestKind::Finish,
            profile: Some(profile),
            apply: false,
        }
    }

    pub(crate) fn clean(apply: bool) -> Self {
        Self {
            kind: RequestKind::Clean,
            profile: None,
            apply,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestKind {
    Validate,
    Check,
    Guard,
    Preflight,
    Finish,
    Report,
    Clean,
}

#[derive(Component)]
#[require(Phase, ScanBudget)]
pub(crate) struct Captured {
    pub root: Entity,
    pub attempt: u64,
}

#[derive(Component, Clone, Copy)]
struct ScanBudget {
    batch: usize,
    limit: usize,
}

impl Default for ScanBudget {
    fn default() -> Self {
        Self {
            batch: ReadDirectoryBatch::DEFAULT_BATCH,
            limit: ReadDirectoryBatch::DEFAULT_LIMIT,
        }
    }
}

impl ScanBudget {
    fn next_batch(&self, total: usize) -> Option<usize> {
        self.limit
            .checked_sub(total)
            .filter(|remaining| *remaining > 0)
            .map(|remaining| remaining.min(self.batch))
    }
}

#[derive(Component, Default)]
struct ScannedEntries(usize);

#[derive(Component, Default)]
struct DepsBuffer(Vec<(PathBuf, std::fs::Metadata)>);

#[derive(Component)]
pub(crate) struct Loan(pub Option<Arc<crate::target_lock::TargetLock>>);

#[derive(Component, Clone, Copy)]
struct Root(Entity);

#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Target,
    Canonical,
    Capacity,
    Tag,
    TagRead,
    Roots,
    Profiles,
    Caches,
    Processes,
    Usage,
    Protection,
    Locks,
    Select,
    Removing,
    FinalCapacity,
    Reporting,
}

#[derive(Component)]
struct Failed(Failure);

#[derive(Component)]
struct Complete;

#[derive(Component)]
struct Sent;

#[derive(Component)]
struct Submitted;

#[derive(Component)]
struct Received;

#[derive(Component)]
struct Admitted;

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
    Target,
    Canonical,
    Capacity,
    Tag,
    Roots,
    Profiles,
    Caches,
    CacheEntries,
    Increments,
    Usage,
    Revision,
    Lock,
    Delete,
    Seed,
}

#[derive(Component)]
struct Metadata(std::fs::Metadata);

#[derive(Component)]
struct Profile;

#[derive(Component)]
struct Cache;

#[derive(Component)]
struct Incremental;

#[derive(Component)]
struct Deps;

#[derive(Component)]
struct Increment;

#[derive(Component)]
struct IncrementalUsage(u64);

#[derive(Component)]
struct DepsUsage(u64);

#[derive(Component)]
struct Checked;

#[derive(Component)]
struct Artifact;

#[derive(Component)]
struct Expanded;

#[derive(Component)]
struct Protected;

#[derive(Component)]
struct Selected;

#[derive(Component)]
struct Removed;

#[derive(Component)]
struct Lock(Arc<File>);

#[derive(Component)]
struct Opening;

#[derive(Component)]
struct Observation(Entity);

#[derive(Component)]
struct Capacity(u64);

#[derive(Component)]
struct Usage(u64);

#[derive(Component)]
struct Current(String);

#[derive(Component)]
struct Pruning;

#[derive(Component)]
struct Seed;

#[derive(Component)]
struct Tagged;

#[derive(Component)]
struct Choosing;

#[derive(Component)]
struct Queued;

#[derive(Component)]
struct SeedChecked;

#[derive(Component)]
struct Processed;

#[derive(Component)]
struct Search;

#[derive(Component)]
struct Candidate;

#[derive(Component)]
struct Missing;

#[derive(Component)]
struct Anchor(PathBuf);

#[derive(SystemParam)]
struct IoWriter<'w, 's> {
    commands: Commands<'w, 's>,
}

impl IoWriter<'_, '_> {
    fn fail(&mut self, entity: Entity, failure: Failure) {
        self.commands.entity(entity).insert(Failed(failure));
    }

    fn request<J: StorageJob>(&mut self, job: J, root: Entity, purpose: Purpose) {
        self.commands.spawn((
            WorkOperation::bundle("dx.storage.io").unwrap(),
            job,
            purpose,
            Root(root),
            ChildOf(root),
        ));
    }

    fn request_batch(
        &mut self,
        path: PathBuf,
        cursor: Option<DirectoryCursor>,
        limit: usize,
        root: Entity,
        purpose: Purpose,
    ) {
        self.request(
            ReadDirectoryBatch {
                path,
                cursor,
                limit,
            },
            root,
            purpose,
        );
    }
}

type MetadataReplies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Purpose,
        &'static PathMetadata,
    ),
>;

type CanonicalReplies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Purpose,
        &'static CanonicalPath,
    ),
>;

type ByteReplies<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static Purpose, &'static FileBytes)>;

type EntryReplies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Purpose,
        &'static DirectoryBatch,
    ),
>;

type FileReplies<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static Purpose, &'static OpenedFile)>;

type OutputReplies<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static Purpose, &'static ToolOutput)>;

type DoneReplies<'w, 's> = Query<'w, 's, (Entity, &'static Root, &'static Purpose, &'static Done)>;

type PendingIo<'w, 's> =
    Query<'w, 's, (&'static Root, &'static Purpose), (With<Io>, Without<Received>)>;

type SearchRows<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static Cwd), (With<Search>, Without<Expanded>)>;

type UnexpandedProfiles<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static Cwd), (With<Profile>, Without<Expanded>)>;

type MissingRoots<'w, 's> = Query<'w, 's, (), With<Missing>>;

type Paths<'w, 's> = Query<'w, 's, &'static Cwd>;

type ProfilesToOpen<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Root, &'static Cwd),
    (With<Profile>, Without<Lock>, Without<Opening>),
>;

type ProfilePaths<'w, 's> = Query<'w, 's, &'static Cwd, With<Profile>>;

type HeldLocks<'w, 's> = Query<'w, 's, &'static Lock>;

type Parents<'w, 's> = Query<'w, 's, &'static ChildOf>;

type ProtectedRows<'w, 's> = Query<'w, 's, (), With<Protected>>;

type PruningRoots<'w, 's> = Query<'w, 's, (), With<Pruning>>;

type TaggedRoots<'w, 's> = Query<'w, 's, (), With<Tagged>>;

type IncrementalValues<'w, 's> = Query<'w, 's, &'static IncrementalUsage>;

type DepsValues<'w, 's> = Query<'w, 's, &'static DepsUsage>;

type Revisions<'w, 's> = Query<'w, 's, &'static Current>;

type ChoosingRoots<'w, 's> = Query<'w, 's, (), With<Choosing>>;

type SeedChecks<'w, 's> = Query<'w, 's, (), With<SeedChecked>>;

type CacheRows<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Root, &'static Cwd, &'static ChildOf),
    (With<Cache>, Without<Expanded>),
>;

type AppRows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Name,
        &'static Metadata,
        &'static ChildOf,
    ),
    With<Artifact>,
>;

type IncrementRows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Cwd,
        &'static Metadata,
        &'static ChildOf,
        &'static Usage,
    ),
    With<Increment>,
>;

type SelectedRows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Cwd,
        &'static Metadata,
        Has<Seed>,
        Has<Artifact>,
    ),
    (
        With<Selected>,
        Without<Protected>,
        Without<Removed>,
        Without<Queued>,
    ),
>;

type MeasuredRows<'w, 's> =
    Query<'w, 's, (Entity, &'static Root), Or<(With<Cache>, With<Increment>)>>;

type MeasurementRows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Root,
        &'static Cwd,
        Has<Cache>,
        Has<Incremental>,
        Has<Deps>,
        Option<&'static Usage>,
    ),
    Or<(With<Cache>, With<Increment>)>,
>;

type CapacityValues<'w, 's> = Query<'w, 's, &'static Capacity>;

type MeasuredTags<'w, 's> = Query<'w, 's, (), With<Tagged>>;

type ProcessedRoots<'w, 's> = Query<'w, 's, (), With<Processed>>;

type RemovedRows<'w, 's> = Query<'w, 's, (), With<Removed>>;

#[derive(SystemParam)]
struct Receipts<'w, 's> {
    metadata: MetadataReplies<'w, 's>,
    canonical: CanonicalReplies<'w, 's>,
    bytes: ByteReplies<'w, 's>,
    entries: EntryReplies<'w, 's>,
    file: FileReplies<'w, 's>,
    output: OutputReplies<'w, 's>,
    done: DoneReplies<'w, 's>,
    pending: PendingIo<'w, 's>,
}

#[derive(SystemParam)]
struct Scan<'w, 's> {
    searches: SearchRows<'w, 's>,
    profiles: UnexpandedProfiles<'w, 's>,
    missing: MissingRoots<'w, 's>,
    paths: Paths<'w, 's>,
}

#[derive(SystemParam)]
struct DirectoryScans<'w, 's> {
    budgets: Query<'w, 's, &'static ScanBudget>,
    scanned: Query<'w, 's, &'static mut ScannedEntries>,
    deps: Query<'w, 's, &'static mut DepsBuffer>,
}

impl DirectoryScans<'_, '_> {
    fn budget(&self, request: Entity) -> ScanBudget {
        self.budgets.get(request).copied().unwrap_or_default()
    }

    fn record(&mut self, target: Entity, count: usize) -> usize {
        if let Ok(mut scanned) = self.scanned.get_mut(target) {
            scanned.0 = scanned.0.saturating_add(count);
            scanned.0
        } else {
            count
        }
    }
}

#[derive(SystemParam)]
struct Profiles<'w, 's> {
    available: ProfilesToOpen<'w, 's>,
    paths: ProfilePaths<'w, 's>,
    locks: HeldLocks<'w, 's>,
    parents: Parents<'w, 's>,
    active: ProtectedRows<'w, 's>,
}

#[derive(SystemParam)]
struct Retention<'w, 's> {
    pruning: PruningRoots<'w, 's>,
    tagged: TaggedRoots<'w, 's>,
    incremental: IncrementalValues<'w, 's>,
    deps: DepsValues<'w, 's>,
    currents: Revisions<'w, 's>,
    choosing: ChoosingRoots<'w, 's>,
    seed_checked: SeedChecks<'w, 's>,
}

#[derive(SystemParam)]
struct Candidates<'w, 's> {
    caches: CacheRows<'w, 's>,
    artifacts: AppRows<'w, 's>,
    increments: IncrementRows<'w, 's>,
    selected: SelectedRows<'w, 's>,
    measured: MeasuredRows<'w, 's>,
}

impl Candidates<'_, '_> {
    fn next(&self, root: Entity) -> Option<Entity> {
        let mut chosen = None;
        for (entity, owner, _, _, seed, _) in &self.selected {
            if owner.0 != root {
                continue;
            }
            if seed {
                return Some(entity);
            }
            if chosen.is_none() {
                chosen = Some(entity);
            }
        }
        chosen
    }
}

#[derive(SystemParam)]
struct Metering<'w, 's> {
    paths: MeasurementRows<'w, 's>,
    capacities: CapacityValues<'w, 's>,
    tagged: MeasuredTags<'w, 's>,
    processed: ProcessedRoots<'w, 's>,
    removed: RemovedRows<'w, 's>,
}

type InitializeIncoming<'w, 's> =
    Query<'w, 's, (Entity, &'static Captured), (With<Request>, Without<Cwd>)>;

type ReceiveResults<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static JobResult<Task<J>>,
        &'static JobOwner<Task<J>>,
        &'static ChildOf,
        &'static Root,
    ),
    Without<Received>,
>;

type ReceiveRequests<'w, 's> =
    Query<'w, 's, (&'static Captured, &'static ChildOf, Option<&'static Failed>)>;

type StatusOutputs<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static Purpose, &'static ToolOutput), Without<Checked>>;

type ProtectCandidates<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Root, &'static Cwd),
    Or<(With<Profile>, With<Artifact>, With<Cache>, With<Increment>)>,
>;

type EvictRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Captured, &'static Phase),
    (With<Request>, Without<Failed>, Without<Complete>),
>;

type ReportRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, &'static Captured, &'static Phase),
    (Without<Complete>, Without<Failed>),
>;

type CancelRequests<'w, 's> =
    Query<'w, 's, (Entity, &'static Captured), (With<Request>, Without<Sent>)>;

type AdmitPending<'w, 's> =
    Query<'w, 's, (Entity, &'static Root, &'static ChildOf), (With<Io>, Without<Admitted>)>;

type AdmitRequests<'w, 's> =
    Query<'w, 's, (&'static Captured, &'static ChildOf, Option<&'static Failed>)>;

type IssuePending<'w, 's, J> =
    Query<'w, 's, (Entity, &'static J, &'static Root), (With<Admitted>, Without<Submitted>)>;

type WakeRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Request>, Without<Complete>, Without<Failed>)>;

type ReleaseRequests<'w, 's> =
    Query<'w, 's, Entity, (With<Request>, Or<(With<Failed>, With<Complete>)>)>;

type FinishRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Captured,
        Option<&'static ChildOf>,
        Option<&'static Failed>,
        Option<&'static Complete>,
    ),
    Without<Sent>,
>;

type Requests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Request,
        &'static Captured,
        &'static mut Phase,
    ),
    (Without<Complete>, Without<Failed>),
>;
type Facts<'w, 's> = Query<
    'w,
    's,
    (
        &'static Checkout,
        &'static Settings,
        &'static Storage,
        &'static OperationState,
    ),
>;

struct IoPlugin<J: StorageJob>(std::marker::PhantomData<fn() -> J>);

impl<J: StorageJob> Default for IoPlugin<J> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<J: StorageJob> bevy_app::Plugin for IoPlugin<J> {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<Task<J>>::default())
            .add_systems(Update, receive::<J>.in_set(Stage::Collect))
            .add_systems(Update, issue::<J>.in_set(Stage::Submit));
    }
}

fn initialize(incoming: InitializeIncoming, facts: Facts, mut commands: Commands) {
    for (entity, captured) in &incoming {
        let Ok((checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        commands
            .entity(entity)
            .insert((Phase::Target, Cwd(checkout.target.clone())));
        commands.spawn((
            WorkOperation::bundle("dx.storage.io").unwrap(),
            Inspect(checkout.target.clone()),
            Purpose::new(PurposeKind::Target),
            Root(entity),
            ChildOf(entity),
        ));
    }
}

fn receive<J: StorageJob>(
    results: ReceiveResults<J>,
    requests: ReceiveRequests,
    attempts: Query<&crate::Attempt>,
    states: Query<&OperationState>,
    mut commands: Commands,
) {
    for (entity, result, owner, parent, root) in &results {
        let accepted = requests
            .get(root.0)
            .is_ok_and(|(captured, request_parent, failed)| {
                owner.accepts(Some(parent))
                    && parent.parent() == root.0
                    && request_parent.parent() == captured.root
                    && failed.is_none()
                    && attempts
                        .get(root.0)
                        .is_ok_and(|attempt| attempt.0 == captured.attempt)
                    && states
                        .get(captured.root)
                        .is_ok_and(|state| *state == OperationState::Running)
            });
        if !accepted {
            commands
                .entity(entity)
                .insert(OperationState::Canceled)
                .remove::<JobResult<Task<J>>>();
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
                commands.entity(root.0).insert(Failed(error));
                commands.entity(entity).insert(OperationState::Failed);
            }
        }
        commands.entity(entity).remove::<JobResult<Task<J>>>();
    }
}

fn status(outputs: StatusOutputs, mut commands: Commands) {
    for (entity, root, _, output) in &outputs {
        let success = output.0.status.success();
        if success {
            commands.entity(entity).insert(Checked);
            continue;
        }
        let mut error = Failure::status(output.0.status);
        if !output.0.stderr.is_empty() {
            error = error.context(String::from_utf8_lossy(&output.0.stderr).trim());
        }
        commands.entity(root.0).insert(Failed(error));
        commands.entity(entity).despawn();
    }
}

fn validate(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    missing: Query<(), With<Missing>>,
    mut io: IoWriter,
) {
    for (entity, request, captured, mut phase) in &mut requests {
        let Ok((checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        for (receipt, root, purpose, PathMetadata(metadata)) in &receipts.metadata {
            if root.0 != entity || *phase != Phase::Target || !purpose.is(PurposeKind::Target) {
                continue;
            }

            let path =
                match metadata {
                    Some(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                        checkout.target.clone()
                    }
                    Some(_) => {
                        io.fail(entity, Failure::message(
                        "Build storage requires a dedicated, nonsymlink Cargo target directory",
                    ));
                        io.commands.entity(receipt).despawn();
                        continue;
                    }
                    None => {
                        let Some(parent) = checkout.target.parent() else {
                            io.fail(entity, Failure::message("Missing target parent"));
                            io.commands.entity(receipt).despawn();
                            continue;
                        };
                        io.commands.entity(entity).insert(Missing);
                        parent.to_owned()
                    }
                };
            io.commands.entity(entity).insert(Anchor(path.clone()));
            io.request(
                Canonicalize(path),
                entity,
                Purpose::new(PurposeKind::Canonical),
            );
            *phase = Phase::Canonical;
            io.commands.entity(receipt).despawn();
        }
        for (receipt, root, purpose, CanonicalPath(path)) in &receipts.canonical {
            if root.0 != entity || *phase != Phase::Canonical || !purpose.is(PurposeKind::Canonical)
            {
                continue;
            }

            let canonical = if missing.contains(entity) {
                let Some(name) = checkout.target.file_name() else {
                    io.fail(entity, Failure::message("Missing target filename"));
                    io.commands.entity(receipt).despawn();
                    continue;
                };
                path.join(name)
            } else {
                path.clone()
            };
            if canonical != checkout.target || checkout.root.starts_with(&checkout.target) {
                io.fail(
                    entity,
                    Failure::message(
                        "Build storage requires a dedicated, nonsymlink Cargo target directory",
                    ),
                );
            } else if matches!(request.kind, RequestKind::Validate) {
                io.commands.entity(entity).insert(Complete);
            } else {
                *phase = Phase::Capacity;
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn capacity(
    mut requests: Requests,
    facts: Facts,
    anchors: Query<&Anchor>,
    receipts: Receipts,
    mut io: IoWriter,
) {
    for (entity, request, captured, mut phase) in &mut requests {
        if !matches!(*phase, Phase::Capacity | Phase::FinalCapacity) {
            continue;
        }
        let Ok((checkout, settings, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        let mut value = None;
        for (receipt, root, purpose, ToolOutput(output)) in &receipts.output {
            if root.0 != entity || !purpose.is(PurposeKind::Capacity) {
                continue;
            }
            let text = match std::str::from_utf8(&output.stdout) {
                Ok(text) => text,
                Err(error) => {
                    io.fail(entity, Failure::message(error.to_string()));
                    io.commands.entity(receipt).despawn();
                    continue;
                }
            };
            if let Some(line) = text.lines().nth(1)
                && let Some(blocks) = line.split_whitespace().nth(3)
                && let Ok(blocks) = blocks.parse::<u64>()
            {
                value = blocks.checked_mul(1024);
            }
            io.commands.entity(receipt).despawn();
            if value.is_none() {
                io.fail(entity, Failure::message("Invalid filesystem capacity"));
            }
        }
        if let Some(value) = value {
            io.commands.entity(entity).insert(Capacity(value));
            let policy = storage.policy();
            if *phase == Phase::FinalCapacity {
                *phase = Phase::Reporting;
            } else {
                match request.kind {
                    RequestKind::Check | RequestKind::Guard => {
                        if matches!(request.kind, RequestKind::Check) {
                            println!("Build storage: {:.1} GiB free", value as f64 / GIB as f64);
                            if value < policy.warning {
                                eprintln!(
                                    "Build storage: warning, less than {} GiB free",
                                    policy.warning as f64 / GIB as f64
                                );
                            }
                        }
                        if let Err(error) = policy.check(value) {
                            io.fail(entity, Failure::from_error(error));
                        } else {
                            io.commands.entity(entity).insert(Complete);
                        }
                    }
                    RequestKind::Preflight => *phase = Phase::Tag,
                    _ => *phase = Phase::Tag,
                }
            }
            continue;
        }
        if receipts
            .pending
            .iter()
            .any(|(root, purpose)| root.0 == entity && purpose.is(PurposeKind::Capacity))
        {
            continue;
        }
        let Ok(anchor) = anchors.get(entity) else {
            continue;
        };
        io.request(
            Capture {
                environment: settings.0.clone(),
                directory: checkout.root.clone(),
                program: "df".into(),
                arguments: vec!["-Pk".into(), anchor.0.clone().into_os_string()],
            },
            entity,
            Purpose::new(PurposeKind::Capacity),
        );
    }
}

fn directory(mut requests: Requests, facts: Facts, receipts: Receipts, mut io: IoWriter) {
    for (entity, request, captured, mut phase) in &mut requests {
        let Ok((checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        match *phase {
            Phase::Tag => {
                let mut received = false;
                for (receipt, root, purpose, PathMetadata(metadata)) in &receipts.metadata {
                    if root.0 != entity || !purpose.is(PurposeKind::Tag) {
                        continue;
                    }
                    received = true;
                    if metadata.as_ref().is_some_and(|metadata| metadata.is_file()) {
                        io.request(
                            ReadFile(checkout.target.join("CACHEDIR.TAG")),
                            entity,
                            Purpose::new(PurposeKind::Tag),
                        );
                        *phase = Phase::TagRead;
                    } else {
                        if matches!(
                            request.kind,
                            RequestKind::Preflight | RequestKind::Clean | RequestKind::Finish
                        ) {
                            println!("Build storage: no Cargo cache tag; cache deletion skipped");
                        }
                        *phase = Phase::Roots;
                    }
                    io.commands.entity(receipt).despawn();
                }
                if !received && !receipts.pending.iter().any(|(root, _)| root.0 == entity) {
                    io.request(
                        Inspect(checkout.target.join("CACHEDIR.TAG")),
                        entity,
                        Purpose::new(PurposeKind::Tag),
                    );
                }
            }
            Phase::TagRead => {
                for (receipt, root, purpose, FileBytes(bytes)) in &receipts.bytes {
                    if root.0 != entity || !purpose.is(PurposeKind::Tag) {
                        continue;
                    }
                    match std::str::from_utf8(bytes) {
                        Ok(text)
                            if text.contains("Signature: 8a477f597d28d172789f06886806bc55") =>
                        {
                            io.commands.entity(entity).insert(Tagged);
                        }
                        Ok(_) => {
                            if matches!(
                                request.kind,
                                RequestKind::Preflight | RequestKind::Clean | RequestKind::Finish
                            ) {
                                println!(
                                    "Build storage: no Cargo cache tag; cache deletion skipped"
                                );
                            }
                        }
                        Err(error) => {
                            io.fail(entity, Failure::message(error.to_string()));
                        }
                    }
                    io.commands.entity(receipt).despawn();
                    *phase = Phase::Roots;
                }
            }
            _ => {}
        }
    }
}

fn scan_roots(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    scan: Scan,
    mut directories: DirectoryScans,
    mut io: IoWriter,
) {
    for (entity, _request, captured, mut phase) in &mut requests {
        let Ok((checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        if *phase != Phase::Roots {
            continue;
        }
        if scan.missing.contains(entity) {
            io.commands.entity(entity).insert(Usage(0));
            *phase = Phase::Usage;
            continue;
        }
        let budget = directories.budget(entity);
        let mut received = false;
        for (receipt, root, purpose, batch) in &receipts.entries {
            if root.0 != entity || !purpose.is(PurposeKind::Roots) {
                continue;
            }
            received = true;
            for (path, metadata) in &batch.entries {
                if metadata.is_dir() {
                    io.commands.spawn((
                        Search,
                        Root(entity),
                        Cwd(path.clone()),
                        Name::new(path.file_name().unwrap().to_string_lossy().into_owned()),
                        Metadata(metadata.clone()),
                        ChildOf(entity),
                    ));
                }
            }
            let total = directories.record(entity, batch.entries.len());
            if let Some(next) = &batch.next {
                if let Some(limit) = budget.next_batch(total) {
                    io.request_batch(
                        batch.path.clone(),
                        Some(next.clone()),
                        limit,
                        entity,
                        Purpose::new(PurposeKind::Roots),
                    );
                } else {
                    eprintln!(
                        "Build storage: stopped scanning {} after {} entries",
                        batch.path.display(),
                        total
                    );
                    *phase = Phase::Profiles;
                }
            } else {
                *phase = Phase::Profiles;
            }
            io.commands.entity(receipt).despawn();
        }
        if !received
            && !receipts.pending.iter().any(|(root, _)| root.0 == entity)
            && !scan.searches.iter().any(|(_, root, _)| root.0 == entity)
        {
            io.commands.spawn((
                Search,
                Root(entity),
                Cwd(checkout.target.clone()),
                ChildOf(entity),
            ));
            io.commands.entity(entity).insert(ScannedEntries::default());
            if let Some(limit) = budget.next_batch(0) {
                io.request_batch(
                    checkout.target.clone(),
                    None,
                    limit,
                    entity,
                    Purpose::new(PurposeKind::Roots),
                );
            } else {
                *phase = Phase::Profiles;
            }
        }
    }
}

fn scan_profiles(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    scan: Scan,
    mut io: IoWriter,
) {
    for (entity, _request, captured, mut phase) in &mut requests {
        let Ok((_checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        if *phase == Phase::Profiles {
            let mut planned = false;
            for (search, root, path) in &scan.searches {
                if root.0 != entity {
                    continue;
                }
                planned = true;
                for name in ["debug", "release", "cef-helper"] {
                    let path = path.0.join(name);
                    let candidate = io
                        .commands
                        .spawn((
                            Candidate,
                            Root(entity),
                            Cwd(path.clone()),
                            Name::new(name),
                            ChildOf(search),
                        ))
                        .id();
                    io.request(
                        Inspect(path),
                        entity,
                        Purpose::targeted(PurposeKind::Profiles, candidate),
                    );
                }
                io.commands.entity(search).insert(Expanded);
            }
            for (receipt, root, purpose, PathMetadata(metadata)) in &receipts.metadata {
                if root.0 != entity {
                    continue;
                }
                let Some(candidate) = purpose.target(PurposeKind::Profiles) else {
                    continue;
                };
                match metadata {
                    Some(metadata) if metadata.is_dir() => {
                        io.commands
                            .entity(*candidate)
                            .insert((Profile, Metadata(metadata.clone())))
                            .remove::<Candidate>();
                    }
                    _ => {
                        io.commands.entity(*candidate).despawn();
                    }
                }
                io.commands.entity(receipt).despawn();
            }
            if !planned && !receipts.pending.iter().any(|(root, _)| root.0 == entity) {
                *phase = Phase::Caches;
            }
        }
    }
}

fn scan_caches(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    scan: Scan,
    directories: DirectoryScans,
    mut io: IoWriter,
) {
    for (entity, _request, captured, phase) in &mut requests {
        let Ok((_checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        if *phase != Phase::Caches {
            continue;
        }
        let budget = directories.budget(entity);

        let mut _planned = false;
        for (profile, root, path) in &scan.profiles {
            if root.0 != entity {
                continue;
            }
            _planned = true;
            for name in [".fingerprint", "deps", "build", "incremental", "examples"] {
                let path = path.0.join(name);
                let candidate = io
                    .commands
                    .spawn((
                        Candidate,
                        Root(entity),
                        Cwd(path.clone()),
                        Name::new(name),
                        ChildOf(profile),
                    ))
                    .id();
                io.request(
                    Inspect(path),
                    entity,
                    Purpose::targeted(PurposeKind::Caches, candidate),
                );
            }
            if path.0.file_name() == Some(OsStr::new("release")) {
                io.commands
                    .entity(profile)
                    .insert(ScannedEntries::default());
                if let Some(limit) = budget.next_batch(0) {
                    io.request_batch(
                        path.0.clone(),
                        None,
                        limit,
                        entity,
                        Purpose::targeted(PurposeKind::CacheEntries, profile),
                    );
                }
            }
            io.commands.entity(profile).insert(Expanded);
        }
        for (receipt, root, purpose, PathMetadata(metadata)) in &receipts.metadata {
            if root.0 != entity {
                continue;
            }
            let Some(candidate) = purpose.target(PurposeKind::Caches) else {
                continue;
            };

            match metadata {
                Some(metadata) if metadata.is_dir() => {
                    io.commands
                        .entity(*candidate)
                        .insert((Cache, Metadata(metadata.clone())))
                        .remove::<Candidate>();
                    if let Ok(path) = scan.paths.get(*candidate) {
                        if path.0.file_name() == Some(OsStr::new("incremental")) {
                            io.commands
                                .entity(*candidate)
                                .insert((Incremental, ScannedEntries::default()));
                            if let Some(limit) = budget.next_batch(0) {
                                io.request_batch(
                                    path.0.clone(),
                                    None,
                                    limit,
                                    entity,
                                    Purpose::targeted(PurposeKind::Increments, *candidate),
                                );
                            }
                        } else if path.0.file_name() == Some(OsStr::new("deps")) {
                            io.commands.entity(*candidate).insert(Deps);
                        }
                    }
                }
                _ => {
                    io.commands.entity(*candidate).despawn();
                }
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn collect_caches(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    mut directories: DirectoryScans,
    mut io: IoWriter,
) {
    for (entity, _request, captured, mut phase) in &mut requests {
        let Ok((_checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        if *phase != Phase::Caches {
            continue;
        }
        let budget = directories.budget(entity);
        let mut continued = false;
        for (receipt, root, purpose, batch) in &receipts.entries {
            if root.0 != entity {
                continue;
            }
            let target = match purpose {
                Purpose {
                    kind: PurposeKind::Increments,
                    entity: Some(cache),
                } => {
                    for (path, metadata) in &batch.entries {
                        if !metadata.is_dir() {
                            continue;
                        }
                        io.commands.spawn((
                            Increment,
                            Root(entity),
                            Cwd(path.clone()),
                            Metadata(metadata.clone()),
                            ChildOf(*cache),
                        ));
                    }
                    *cache
                }
                Purpose {
                    kind: PurposeKind::CacheEntries,
                    entity: Some(profile),
                } => {
                    for (path, metadata) in &batch.entries {
                        if !metadata.is_dir() {
                            continue;
                        }
                        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
                            continue;
                        };
                        let Some(revision) = name
                            .strip_prefix("Vmux (")
                            .and_then(|name| name.strip_suffix(").app"))
                        else {
                            continue;
                        };
                        if !(7..=40).contains(&revision.len())
                            || !revision
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                        {
                            continue;
                        }
                        io.commands.spawn((
                            Artifact,
                            Root(entity),
                            Cwd(path.clone()),
                            Name::new(name.to_owned()),
                            Metadata(metadata.clone()),
                            ChildOf(*profile),
                        ));
                    }
                    *profile
                }
                _ => continue,
            };
            let total = directories.record(target, batch.entries.len());
            if let Some(next) = &batch.next {
                if let Some(limit) = budget.next_batch(total) {
                    continued = true;
                    io.request_batch(
                        batch.path.clone(),
                        Some(next.clone()),
                        limit,
                        entity,
                        *purpose,
                    );
                } else {
                    eprintln!(
                        "Build storage: stopped scanning {} after {} entries",
                        batch.path.display(),
                        total
                    );
                }
            }
            io.commands.entity(receipt).despawn();
        }

        if !continued && !receipts.pending.iter().any(|(root, _)| root.0 == entity) {
            *phase = Phase::Usage;
        }
    }
}

fn processes(
    mut requests: Requests,
    observations: Query<&Observation>,
    facts: crate::process_observation::Facts,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, _, captured, mut phase) in &mut requests {
        if *phase != Phase::Processes {
            continue;
        }
        if let Ok(observation) = observations.get(entity) {
            if let Some(error) = facts.error(observation.0) {
                commands.entity(entity).insert(Failed(error.clone()));
            } else if facts.ready(observation.0) {
                *phase = Phase::Protection;
            }
            continue;
        }
        let observation = commands
            .spawn((
                crate::process_observation::Request {
                    context: captured.root,
                    attempt: captured.attempt,
                },
                Root(entity),
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert(Observation(observation));
        wake.0.wake_by_ref();
    }
}

fn collect_usage(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    metering: Metering,
    captures: Query<&Capture>,
    mut io: IoWriter,
) {
    for (entity, _request, captured, phase) in &mut requests {
        if *phase != Phase::Usage {
            continue;
        }
        let Ok((_checkout, _settings, _storage, _)) = facts.get(captured.root) else {
            continue;
        };
        let _received = false;
        for (receipt, owner, purpose, output) in &receipts.output {
            if owner.0 != entity || !purpose.is(PurposeKind::Usage) {
                continue;
            }
            let text = match std::str::from_utf8(&output.0.stdout) {
                Ok(text) => text,
                Err(error) => {
                    io.fail(entity, Failure::message(error.to_string()));
                    io.commands.entity(receipt).despawn();
                    continue;
                }
            };
            let Ok(capture) = captures.get(receipt) else {
                continue;
            };
            let expected_paths: std::collections::HashSet<_> = capture
                .arguments
                .iter()
                .skip(1)
                .map(PathBuf::from)
                .collect();
            let mut matched = std::collections::HashSet::new();
            let mut invalid = false;
            for line in text.lines() {
                let Some((blocks, path)) = line.split_once('\t') else {
                    invalid = true;
                    break;
                };
                let bytes = blocks
                    .parse::<u64>()
                    .ok()
                    .and_then(|blocks| blocks.checked_mul(1024));
                let Some(bytes) = bytes else {
                    invalid = true;
                    break;
                };
                let mut found = false;
                for (row, root, cwd, _, _, _, _) in &metering.paths {
                    if root.0 != entity
                        || metering.removed.contains(row)
                        || !expected_paths.contains(&cwd.0)
                        || cwd.0 != std::path::Path::new(path)
                    {
                        continue;
                    }
                    if !matched.insert(row) {
                        invalid = true;
                        break;
                    }
                    io.commands.entity(row).insert(Usage(bytes));
                    found = true;
                    break;
                }
                if !found {
                    invalid = true;
                    break;
                }
            }
            let expected = expected_paths.len();
            if invalid || matched.len() != expected {
                io.fail(entity, Failure::message("Invalid compile cache size"));
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn usage(
    mut requests: Requests,
    facts: Facts,
    receipts: Receipts,
    metering: Metering,
    mut io: IoWriter,
) {
    for (entity, request, captured, mut phase) in &mut requests {
        if *phase != Phase::Usage {
            continue;
        }
        let Ok((checkout, settings, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        if receipts.pending.iter().any(|(root, _)| root.0 == entity) {
            continue;
        }
        let (mut total, mut incremental, mut deps) = (0u64, 0u64, 0u64);
        let mut arguments = vec![OsString::from("-skl")];
        let (mut measured, mut overflow) = (true, false);
        for (row, root, path, cache, is_incremental, is_deps, size) in &metering.paths {
            if root.0 != entity || metering.removed.contains(row) {
                continue;
            }
            let Some(size) = size else {
                measured = false;
                let bytes: usize = arguments
                    .iter()
                    .map(|argument| argument.as_encoded_bytes().len() + 1)
                    .sum();
                if bytes + path.0.as_os_str().as_encoded_bytes().len() < 64 * 1024 {
                    arguments.push(path.0.clone().into_os_string());
                }
                continue;
            };
            for (enabled, sum) in [
                (cache, &mut total),
                (is_incremental, &mut incremental),
                (is_deps, &mut deps),
            ] {
                if enabled {
                    let Some(value) = sum.checked_add(size.0) else {
                        overflow = true;
                        break;
                    };
                    *sum = value;
                }
            }
            if overflow {
                break;
            }
        }
        if overflow {
            io.fail(entity, Failure::message("Compile cache size overflow"));
            continue;
        }
        if !measured {
            io.request(
                Capture {
                    environment: settings.0.clone(),
                    directory: checkout.root.clone(),
                    program: "du".into(),
                    arguments,
                },
                entity,
                Purpose::new(PurposeKind::Usage),
            );
            continue;
        }
        io.commands.entity(entity).insert((
            Usage(total),
            IncrementalUsage(incremental),
            DepsUsage(deps),
        ));
        if metering.processed.contains(entity) {
            *phase = Phase::FinalCapacity;
            continue;
        }
        let low = metering
            .capacities
            .get(entity)
            .is_ok_and(|capacity| capacity.0 < storage.policy().minimum);
        match request.kind {
            RequestKind::Report => *phase = Phase::Reporting,
            RequestKind::Clean if metering.tagged.contains(entity) => {
                io.commands.entity(entity).insert(Pruning);
                *phase = Phase::Processes;
            }
            RequestKind::Preflight | RequestKind::Finish if metering.tagged.contains(entity) => {
                if low
                    || matches!(request.kind, RequestKind::Finish)
                        && total > storage.policy().cache_limit
                {
                    io.commands.entity(entity).insert(Pruning);
                }
                *phase = Phase::Processes;
            }
            _ => *phase = Phase::FinalCapacity,
        }
    }
}

fn protect(
    mut requests: Requests,
    candidates: ProtectCandidates,
    observations: Query<&Observation>,
    facts: crate::process_observation::Facts,
    mut commands: Commands,
) {
    for (entity, _, _, mut phase) in &mut requests {
        if *phase != Phase::Protection {
            continue;
        }
        let Ok(observation) = observations.get(entity) else {
            continue;
        };
        for (candidate, root, path) in &candidates {
            if root.0 == entity && facts.uses(observation.0, &path.0) {
                commands.entity(candidate).insert(Protected);
            }
        }
        *phase = Phase::Locks;
    }
}

fn locks(
    mut requests: Requests,
    retention: Retention,
    facts: Facts,
    profiles: Profiles,
    receipts: Receipts,
    mut io: IoWriter,
) {
    for (entity, _, captured, mut phase) in &mut requests {
        if *phase != Phase::Locks {
            continue;
        }
        let Ok((_, _, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        if !retention.pruning.contains(entity)
            && !retention
                .incremental
                .get(entity)
                .is_ok_and(|usage| usage.0 > storage.policy().incremental_limit)
            && !retention
                .deps
                .get(entity)
                .is_ok_and(|usage| usage.0 > storage.policy().deps_limit)
        {
            *phase = Phase::Select;
            continue;
        }
        let mut planned = false;
        for (profile, root, path) in &profiles.available {
            if root.0 != entity {
                continue;
            }
            planned = true;
            let mut options = OpenOptions::new();
            options
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            io.commands.entity(profile).insert(Opening);
            io.request(
                Open(path.0.join(".cargo-lock"), options),
                entity,
                Purpose::targeted(PurposeKind::Lock, profile),
            );
        }
        for (receipt, root, purpose, OpenedFile(file)) in &receipts.file {
            if root.0 != entity {
                continue;
            }
            let Some(profile) = purpose.target(PurposeKind::Lock) else {
                continue;
            };
            match file.try_lock() {
                Ok(()) => {
                    io.commands.entity(*profile).insert(Lock(file.clone()));
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => {
                    io.fail(entity, Failure::message(error.to_string()));
                }
            }
            io.commands.entity(receipt).despawn();
        }
        if !planned && !receipts.pending.iter().any(|(root, _)| root.0 == entity) {
            *phase = Phase::Select;
        }
    }
}

fn evict(
    requests: EvictRequests,
    facts: Facts,
    retention: Retention,
    candidates: Candidates,
    profiles: Profiles,
    mut commands: Commands,
) {
    for (entity, captured, phase) in &requests {
        if *phase != Phase::Select {
            continue;
        }
        let Ok((_, _, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        if !retention.pruning.contains(entity)
            && let Ok(usage) = retention.incremental.get(entity)
        {
            let mut total = usage.0;
            let mut ordered = Vec::new();
            for (row, root, _, metadata, parent, size) in &candidates.increments {
                if root.0 != entity
                    || profiles.active.contains(row)
                    || profiles.active.contains(parent.parent())
                {
                    continue;
                }
                let Ok(profile) = profiles.parents.get(parent.parent()) else {
                    continue;
                };
                if profiles.locks.get(profile.parent()).is_err() {
                    continue;
                }
                match metadata.0.modified() {
                    Ok(modified) => ordered.push((modified, row, size.0)),
                    Err(error) => {
                        commands
                            .entity(entity)
                            .insert(Failed(Failure::message(error.to_string())));
                    }
                }
            }
            ordered.sort_by_key(|(modified, _, _)| *modified);
            for (_, row, size) in ordered {
                if total <= storage.policy().incremental_limit {
                    break;
                }
                commands.entity(row).insert(Selected);
                total = total.saturating_sub(size);
            }
            if total > storage.policy().incremental_limit {
                eprintln!(
                    "Build storage: incremental cache exceeds its budget; active caches were kept"
                );
            }
        }
    }
}

fn select(
    mut requests: Requests,
    facts: Facts,
    retention: Retention,
    receipts: Receipts,
    mut io: IoWriter,
) {
    for (entity, request, captured, phase) in &mut requests {
        if *phase != Phase::Select {
            continue;
        }
        let Ok((checkout, settings, _storage, _)) = facts.get(captured.root) else {
            continue;
        };
        if !retention.choosing.contains(entity) {
            io.commands.entity(entity).insert(Choosing);
            if matches!(request.kind, RequestKind::Finish if request.profile.as_deref() == Some("local"))
                && retention.tagged.contains(entity)
            {
                io.request(
                    Capture {
                        environment: settings.0.clone(),
                        directory: checkout.root.clone(),
                        program: "git".into(),
                        arguments: vec!["rev-parse".into(), "--short=7".into(), "HEAD".into()],
                    },
                    entity,
                    Purpose::new(PurposeKind::Revision),
                );
            } else {
                io.commands.entity(entity).insert(Current(String::new()));
            }
        }
        for (receipt, root, purpose, ToolOutput(output)) in &receipts.output {
            if root.0 != entity || !purpose.is(PurposeKind::Revision) {
                continue;
            }

            match std::str::from_utf8(&output.stdout) {
                Ok(text) => {
                    let revision = text.trim();
                    if revision.len() != 7
                        || !revision
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    {
                        io.fail(entity, Failure::message("Invalid current build revision"));
                    } else {
                        io.commands
                            .entity(entity)
                            .insert(Current(format!("Vmux ({revision}).app")));
                    }
                }
                Err(error) => {
                    io.fail(entity, Failure::message(error.to_string()));
                }
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn collect_deps(
    mut requests: Requests,
    facts: Facts,
    retention: Retention,
    receipts: Receipts,
    mut directories: DirectoryScans,
    mut io: IoWriter,
) {
    for (entity, _request, captured, phase) in &mut requests {
        if *phase != Phase::Select {
            continue;
        }
        let Ok((_checkout, _settings, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        let budget = directories.budget(entity);
        for (receipt, root, purpose, batch) in &receipts.entries {
            if root.0 != entity {
                continue;
            }
            let Some(cache) = purpose.target(PurposeKind::CacheEntries) else {
                continue;
            };
            let total = directories.record(*cache, batch.entries.len());
            if retention.pruning.contains(entity) {
                for (path, metadata) in storage.policy().prunable_deps(&batch.entries) {
                    io.commands.spawn((
                        Selected,
                        Root(entity),
                        Cwd(path),
                        Metadata(metadata),
                        ChildOf(*cache),
                    ));
                }
                if let Some(next) = &batch.next {
                    if let Some(limit) = budget.next_batch(total) {
                        io.request_batch(
                            batch.path.clone(),
                            Some(next.clone()),
                            limit,
                            entity,
                            *purpose,
                        );
                    } else {
                        eprintln!(
                            "Build storage: stopped scanning {} after {} entries; remaining entries deferred",
                            batch.path.display(),
                            total
                        );
                    }
                }
            } else if let Ok(mut buffer) = directories.deps.get_mut(*cache) {
                for (path, metadata) in &batch.entries {
                    if Policy::is_hashed_artifact(path) {
                        buffer.0.push((path.clone(), metadata.clone()));
                    }
                }
                if let Some(next) = &batch.next {
                    if let Some(limit) = budget.next_batch(total) {
                        io.request_batch(
                            batch.path.clone(),
                            Some(next.clone()),
                            limit,
                            entity,
                            *purpose,
                        );
                    } else {
                        eprintln!(
                            "Build storage: skipping stale deps pruning for {} after {} entries",
                            batch.path.display(),
                            total
                        );
                        buffer.0.clear();
                    }
                } else {
                    let current = retention.deps.get(entity).map(|usage| usage.0).unwrap_or(0);
                    let (stale, remaining) = storage.policy().stale_deps(&buffer.0, current);
                    io.commands.entity(entity).insert(DepsUsage(remaining));
                    for (path, metadata) in stale {
                        io.commands.spawn((
                            Selected,
                            Root(entity),
                            Cwd(path),
                            Metadata(metadata),
                            ChildOf(*cache),
                        ));
                    }
                    buffer.0.clear();
                }
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn select_caches(
    mut requests: Requests,
    facts: Facts,
    retention: Retention,
    candidates: Candidates,
    profiles: Profiles,
    directories: DirectoryScans,
    mut io: IoWriter,
) {
    for (entity, _request, captured, phase) in &mut requests {
        if *phase != Phase::Select {
            continue;
        }
        let Ok((_checkout, _settings, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        let budget = directories.budget(entity);
        if retention.pruning.contains(entity) {
            for (cache, root, path, parent) in &candidates.caches {
                if root.0 != entity {
                    continue;
                }
                io.commands.entity(cache).insert(Expanded);
                if profiles.locks.get(parent.parent()).is_err() || profiles.active.contains(cache) {
                    continue;
                }
                if profiles.active.contains(parent.parent())
                    && path.0.file_name() != Some(OsStr::new("incremental"))
                {
                    continue;
                }
                if path.0.file_name() == Some(OsStr::new("deps")) {
                    io.commands.entity(cache).insert(ScannedEntries::default());
                    if let Some(limit) = budget.next_batch(0) {
                        io.request_batch(
                            path.0.clone(),
                            None,
                            limit,
                            entity,
                            Purpose::targeted(PurposeKind::CacheEntries, cache),
                        );
                    }
                } else {
                    io.commands.entity(cache).insert(Selected);
                }
            }
        } else if retention
            .deps
            .get(entity)
            .is_ok_and(|usage| usage.0 > storage.policy().deps_limit)
        {
            for (cache, root, path, parent) in &candidates.caches {
                if root.0 != entity || path.0.file_name() != Some(OsStr::new("deps")) {
                    continue;
                }
                io.commands.entity(cache).insert(Expanded);
                if profiles.locks.get(parent.parent()).is_err() || profiles.active.contains(cache) {
                    continue;
                }
                io.commands
                    .entity(cache)
                    .insert((ScannedEntries::default(), DepsBuffer::default()));
                if let Some(limit) = budget.next_batch(0) {
                    io.request_batch(
                        path.0.clone(),
                        None,
                        limit,
                        entity,
                        Purpose::targeted(PurposeKind::CacheEntries, cache),
                    );
                }
            }
        }
    }
}

fn retain_apps(
    mut requests: Requests,
    facts: Facts,
    retention: Retention,
    candidates: Candidates,
    profiles: Profiles,
    receipts: Receipts,
    mut io: IoWriter,
) {
    for (entity, request, captured, mut phase) in &mut requests {
        if *phase != Phase::Select {
            continue;
        }
        let Ok((_checkout, _settings, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        let Ok(current) = retention.currents.get(entity) else {
            continue;
        };
        if matches!(request.kind, RequestKind::Finish if request.profile.as_deref() == Some("local"))
            && retention.tagged.contains(entity)
        {
            let mut ordered = Vec::new();
            for (artifact, root, name, metadata, parent) in &candidates.artifacts {
                if root.0 != entity {
                    continue;
                }
                let Ok(path) = profiles.paths.get(parent.parent()) else {
                    continue;
                };
                let modified = match metadata.0.modified() {
                    Ok(modified) => modified,
                    Err(error) => {
                        io.fail(entity, Failure::message(error.to_string()));
                        continue;
                    }
                };
                ordered.push((
                    modified,
                    path.0.clone(),
                    artifact,
                    name.as_str() == current.0,
                ));
            }
            ordered.sort_by_key(|(modified, _, _, _)| std::cmp::Reverse(*modified));
            let mut retained = std::collections::HashSet::new();
            for (_, _, artifact, is_current) in &ordered {
                if *is_current {
                    retained.insert(*artifact);
                }
            }
            let mut counts = std::collections::HashMap::new();
            for (_, profile, _, is_current) in &ordered {
                if *is_current {
                    *counts.entry(profile.clone()).or_insert(0usize) += 1;
                }
            }
            for (_, profile, artifact, is_current) in &ordered {
                let count = counts.entry(profile.clone()).or_insert(0usize);
                if *is_current {
                    continue;
                }
                if *count < storage.policy().local_keep && retained.insert(*artifact) {
                    *count += 1;
                }
            }
            for (_, _, artifact, _) in ordered {
                if !retained.contains(&artifact) {
                    io.commands.entity(artifact).insert(Selected);
                }
            }
        }
        if !receipts.pending.iter().any(|(root, _)| root.0 == entity) {
            *phase = Phase::Removing;
        }
    }
}

fn collect_removals(
    mut requests: Requests,
    facts: Facts,
    candidates: Candidates,
    profiles: Profiles,
    receipts: Receipts,
    mut io: IoWriter,
) {
    for (entity, _, captured, phase) in &mut requests {
        if *phase != Phase::Removing {
            continue;
        }
        let Ok((checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        for (receipt, root, purpose, _) in &receipts.done {
            if root.0 != entity {
                continue;
            }
            let Some(target) = purpose.target(PurposeKind::Delete) else {
                continue;
            };

            io.commands.entity(*target).insert(Removed);
            for (row, root) in &candidates.measured {
                if root.0 == entity
                    && profiles
                        .parents
                        .get(row)
                        .is_ok_and(|parent| parent.parent() == *target)
                {
                    io.commands.entity(row).insert(Removed);
                }
            }
            io.commands.entity(receipt).despawn();
        }
        for (receipt, root, purpose, PathMetadata(metadata)) in &receipts.metadata {
            if root.0 != entity {
                continue;
            }
            let Some(profile) = purpose.target(PurposeKind::Seed) else {
                continue;
            };
            if let Some(metadata) = metadata
                && metadata.is_dir()
            {
                io.commands.spawn((
                    Selected,
                    Seed,
                    Root(entity),
                    Cwd(checkout.target.join(".vmux-seed")),
                    Metadata(metadata.clone()),
                    ChildOf(*profile),
                ));
            }
            io.commands.entity(receipt).despawn();
        }
    }
}

fn remove(
    mut requests: Requests,
    facts: Facts,
    candidates: Candidates,
    profiles: Profiles,
    receipts: Receipts,
    retention: Retention,
    mut io: IoWriter,
) {
    for (entity, request, captured, mut phase) in &mut requests {
        if *phase != Phase::Removing {
            continue;
        }
        let Ok((checkout, _, _, _)) = facts.get(captured.root) else {
            continue;
        };
        let apply = match request.kind {
            RequestKind::Clean => request.apply,
            RequestKind::Preflight | RequestKind::Finish => true,
            _ => false,
        };
        if receipts.pending.iter().any(|(root, _)| root.0 == entity) {
            continue;
        }
        let Some(target) = candidates.next(entity) else {
            io.commands
                .entity(entity)
                .insert(Processed)
                .remove::<Usage>();
            for (row, root) in &candidates.measured {
                if root.0 == entity {
                    io.commands.entity(row).remove::<Usage>();
                }
            }
            *phase = if matches!(request.kind, RequestKind::Preflight) {
                Phase::FinalCapacity
            } else {
                Phase::Usage
            };
            continue;
        };
        let Ok((_, _, path, metadata, seed, artifact)) = candidates.selected.get(target) else {
            continue;
        };
        if !path.0.starts_with(&checkout.target) || path.0 == checkout.target {
            io.fail(
                entity,
                Failure::message(format!(
                    "Refusing to delete outside Cargo target: {}",
                    path.0.display()
                )),
            );
            continue;
        }
        let mut ancestor = target;
        let mut lock = None;
        let mut profile = None;
        while let Ok(parent) = profiles.parents.get(ancestor) {
            ancestor = parent.parent();
            if let Ok(value) = profiles.locks.get(ancestor) {
                lock = Some(value.0.clone());
                profile = Some(ancestor);
                break;
            }
            if ancestor == entity {
                break;
            }
        }
        if !artifact && !seed && apply && !retention.seed_checked.contains(entity) {
            io.commands.entity(entity).insert(SeedChecked);
            if let Some(profile) = profile {
                io.request(
                    Inspect(checkout.target.join(".vmux-seed")),
                    entity,
                    Purpose::targeted(PurposeKind::Seed, profile),
                );
                continue;
            }
        }
        if !apply {
            println!("Build storage: would remove {}", path.0.display());
            io.commands.entity(target).insert(Queued);
            continue;
        }
        if artifact {
            println!("Build storage: removing old local app {}", path.0.display());
        } else {
            println!("Build storage: removing {}", path.0.display());
        }
        io.commands.entity(target).insert(Queued);
        io.request(
            Remove {
                path: path.0.clone(),
                directory: metadata.0.is_dir(),
                lock,
            },
            entity,
            Purpose::targeted(PurposeKind::Delete, target),
        );
    }
}

fn report(
    requests: ReportRequests,
    facts: Facts,
    capacities: Query<&Capacity>,
    usages: Query<&Usage>,
    mut commands: Commands,
) {
    for (entity, request, captured, phase) in &requests {
        if *phase != Phase::Reporting {
            continue;
        }
        let Ok((_, _, storage, _)) = facts.get(captured.root) else {
            continue;
        };
        let Ok(capacity) = capacities.get(entity) else {
            continue;
        };
        let policy = storage.policy();
        match request.kind {
            RequestKind::Preflight => {
                println!(
                    "Build storage: {:.1} GiB free",
                    capacity.0 as f64 / GIB as f64
                );
                if capacity.0 < policy.minimum {
                    commands.entity(entity).insert(Failed(Failure::message(format!("Insufficient build space: need {} GiB free; stop unused apps/builds and run vmux cache clean --apply", policy.minimum as f64 / GIB as f64))));
                    continue;
                }
                if capacity.0 < policy.warning {
                    eprintln!(
                        "Build storage: warning, less than {} GiB free",
                        policy.warning as f64 / GIB as f64
                    );
                }
            }
            RequestKind::Finish => {
                let profile = request.profile.as_deref().unwrap();
                println!(
                    "Build storage: {:.1} GiB free after {profile} build",
                    capacity.0 as f64 / GIB as f64
                );
                if capacity.0 < policy.warning {
                    eprintln!(
                        "Build storage: free space remains low; active profiles and distribution artifacts were kept"
                    );
                }
            }
            RequestKind::Clean | RequestKind::Report => {
                let Ok(usage) = usages.get(entity) else {
                    continue;
                };
                println!(
                    "Build storage: {:.1} GiB free, {:.1} GiB of compile caches",
                    capacity.0 as f64 / GIB as f64,
                    usage.0 as f64 / GIB as f64
                );
            }
            _ => {}
        }
        commands.entity(entity).insert(Complete);
    }
}

fn cancel(
    requests: CancelRequests,
    parents: Query<&ChildOf>,
    attempts: Query<&crate::Attempt>,
    states: Query<&OperationState>,
    operations: Query<(Entity, &Root), With<Io>>,
    mut commands: Commands,
) {
    for (entity, captured) in &requests {
        let canceled = Signals::pending().is_some()
            || !parents
                .get(entity)
                .is_ok_and(|parent| parent.parent() == captured.root)
            || !attempts
                .get(entity)
                .is_ok_and(|attempt| attempt.0 == captured.attempt)
            || !states
                .get(captured.root)
                .is_ok_and(|state| *state == OperationState::Running);
        if !canceled {
            continue;
        }
        commands.entity(entity).insert(Failed(Failure::interrupted(
            Signals::pending().unwrap_or(15),
        )));
        for (operation, root) in &operations {
            if root.0 == entity {
                commands.entity(operation).insert(OperationState::Canceled);
            }
        }
    }
}

fn admit(
    pending: AdmitPending,
    requests: AdmitRequests,
    attempts: Query<&crate::Attempt>,
    states: Query<&OperationState>,
    active: Query<&OperationState, With<Io>>,
    mut commands: Commands,
) {
    if active.iter().any(|state| *state == OperationState::Running) {
        return;
    }
    for (entity, root, parent) in &pending {
        let Ok((captured, request_parent, failed)) = requests.get(root.0) else {
            continue;
        };
        if failed.is_some()
            || parent.parent() != root.0
            || request_parent.parent() != captured.root
            || !attempts
                .get(root.0)
                .is_ok_and(|attempt| attempt.0 == captured.attempt)
            || !states
                .get(captured.root)
                .is_ok_and(|state| *state == OperationState::Running)
        {
            commands
                .entity(entity)
                .insert((Admitted, Submitted, OperationState::Canceled));
            continue;
        }
        commands.entity(entity).insert(Admitted);
        break;
    }
}

fn issue<J: StorageJob>(
    pending: IssuePending<J>,
    loans: Query<&Loan>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, operation, root) in &pending {
        let loan = loans.get(root.0).ok().and_then(|loan| loan.0.clone());
        let (task, flight) = Task::new(operation.clone(), loan, Vec::new(), wake.0.clone());
        commands.entity(entity).insert((
            Submitted,
            flight,
            JobRequest(task),
            OperationState::Running,
        ));
        wake.0.wake_by_ref();
    }
}

fn wake(
    requests: WakeRequests,
    io: Query<(&Root, &OperationState), With<Io>>,
    observations: Query<&Observation>,
    facts: crate::process_observation::Facts,
    wake: Res<JobWake>,
) {
    for entity in &requests {
        if observations
            .get(entity)
            .is_ok_and(|observation| !facts.drained(observation.0))
        {
            continue;
        }
        if !io
            .iter()
            .any(|(root, state)| root.0 == entity && *state == OperationState::Running)
        {
            wake.0.wake_by_ref();
        }
    }
}

fn release(
    requests: ReleaseRequests,
    flights: Query<(&Root, &Flight)>,
    observations: Query<&Observation>,
    facts: crate::process_observation::Facts,
    rows: Query<(Entity, &Root, &ChildOf)>,
    mut commands: Commands,
) {
    for entity in &requests {
        if observations
            .get(entity)
            .is_ok_and(|observation| !facts.drained(observation.0))
        {
            continue;
        }
        if flights
            .iter()
            .any(|(root, flight)| root.0 == entity && flight.pending())
        {
            continue;
        }
        for (row, root, parent) in &rows {
            if root.0 == entity && parent.parent() == entity {
                commands.entity(row).despawn();
            }
        }
        commands.entity(entity).remove::<Loan>();
    }
}

fn finish(
    requests: FinishRequests,
    rows: Query<&Root>,
    mut completions: MessageWriter<Completion>,
    wake: Res<JobWake>,
    mut commands: Commands,
) {
    for (entity, request, captured, _parent, failed, complete) in &requests {
        if failed.is_none() && complete.is_none() || rows.iter().any(|root| root.0 == entity) {
            continue;
        }
        let result = match failed {
            Some(failed)
                if matches!(request.kind, RequestKind::Finish) && Signals::pending().is_none() =>
            {
                eprintln!(
                    "Build storage: cleanup skipped after successful build: {}",
                    failed.0
                );
                Ok(())
            }
            Some(failed) => Err(failed.0.clone()),
            None => Ok(()),
        };
        completions.write(Completion {
            entity,
            context: captured.root,
            attempt: captured.attempt,
            result,
        });
        commands.entity(entity).insert(Sent);
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File, FileTimes};
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::policy::Policy;
    use std::os::unix::fs::PermissionsExt;
    use vmux_ecs::cli::CliResult;

    struct Fixture {
        _directory: tempfile::TempDir,
        checkout: Checkout,
        storage: Storage,
        _permit: crate::testing::Permit,
    }

    impl Fixture {
        fn new() -> Self {
            let permit = crate::testing::Permit::acquire();
            let directory = tempfile::tempdir().unwrap();
            let base = directory.path().canonicalize().unwrap();
            let root = base.join("checkout with spaces");
            let target = base.join("target with spaces");
            fs::create_dir(&root).unwrap();
            fs::create_dir_all(&target).unwrap();
            fs::create_dir(root.join("bin")).unwrap();
            let checkout = Checkout {
                root,
                target,
                build_target: None,
            };
            let storage = Storage::new(Policy {
                minimum: 0,
                warning: 0,
                cache_limit: u64::MAX,
                incremental_limit: u64::MAX,
                deps_limit: u64::MAX,
                local_keep: 3,
            });
            let fixture = Self {
                _permit: permit,
                _directory: directory,
                checkout,
                storage,
            };
            fixture.write(
                "CACHEDIR.TAG",
                "Signature: 8a477f597d28d172789f06886806bc55",
            );
            fixture.observe("1 init\n", "p1\nn/fixture-unused\n");
            fixture.tool("ps", "exec /bin/cat .fixture-ps");
            fixture.tool("lsof", "exec /bin/cat .fixture-lsof");
            fixture.tool("id", "printf '501\n'");
            fixture.tool("git", "printf '0000001\n'");
            fixture
        }

        fn tool(&self, name: &str, body: &str) {
            let path = self.checkout.root.join("bin").join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn observe(&self, ps: &str, opened: &str) {
            fs::write(self.checkout.root.join(".fixture-ps"), ps).unwrap();
            fs::write(self.checkout.root.join(".fixture-lsof"), opened).unwrap();
        }

        fn run(&self, request: Request) -> u8 {
            self.run_with_budget(request, ScanBudget::default())
        }

        fn run_with_budget(&self, request: Request, budget: ScanBudget) -> u8 {
            let mut environment = crate::environment::Environment::current();
            let path = format!(
                "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                self.checkout.root.join("bin").display()
            );
            environment.set("PATH", path);
            let mut app = App::new();
            app.add_plugins(crate::DxPlugin);
            let operation = app
                .world_mut()
                .spawn((
                    crate::Accepted,
                    WorkOperation::bundle("dx.storage.fixture")
                        .unwrap()
                        .running(),
                    self.checkout.clone(),
                    Settings(environment),
                    self.storage.clone(),
                ))
                .id();
            let guard = matches!(request.kind, RequestKind::Guard);
            let step = app
                .world_mut()
                .spawn((
                    ChildOf(operation),
                    crate::Attempt(1),
                    OperationState::Pending,
                    budget,
                ))
                .id();
            match request.kind {
                RequestKind::Validate => unreachable!(),
                RequestKind::Check | RequestKind::Guard => {
                    app.world_mut()
                        .entity_mut(step)
                        .insert(crate::steps::StorageCheckStep);
                }
                RequestKind::Preflight => {
                    app.world_mut()
                        .entity_mut(step)
                        .insert(crate::steps::StoragePreflightStep);
                }
                RequestKind::Finish => {
                    app.world_mut()
                        .entity_mut(step)
                        .insert(crate::steps::StorageFinishStep(
                            request.profile.clone().unwrap(),
                        ));
                }
                RequestKind::Report => {
                    app.world_mut()
                        .entity_mut(step)
                        .insert(crate::steps::StorageStatusStep);
                }
                RequestKind::Clean => {
                    app.world_mut()
                        .entity_mut(step)
                        .insert(crate::steps::StorageCleanStep(request.apply));
                }
            }
            if guard {
                app.world_mut()
                    .entity_mut(step)
                    .remove::<crate::steps::StorageCheckStep>()
                    .insert((
                        crate::Step,
                        crate::Running,
                        crate::Started {
                            context: operation,
                            attempt: 1,
                        },
                        OperationState::Running,
                        Request::new(RequestKind::Guard),
                        Captured {
                            root: operation,
                            attempt: 1,
                        },
                        Loan(None),
                    ));
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                app.update();
                if let Some(CliResult(Ok(code))) = app.world().get::<CliResult>(operation) {
                    if *code == 0 {
                        assert!(
                            app.world().get::<Failed>(step).is_none(),
                            "storage cleanup failed despite a successful command"
                        );
                    }
                    return *code;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "storage request did not settle"
                );
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }

        fn write(&self, relative: &str, contents: &str) -> PathBuf {
            let path = self.checkout.target.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            path
        }

        fn caches(&self, profile: &str) {
            for relative in [
                ".fingerprint/current",
                "deps/object.o",
                "deps/library.rlib",
                "deps/current.dylib",
                "deps/current.so",
                "build/out/generated",
                "incremental/state",
            ] {
                self.write(&format!("{profile}/{relative}"), "compiled artifact");
            }
            self.write(&format!("{profile}/vmux_desktop"), "ready executable");
            self.write(
                &format!("{profile}/Vmux Dev.app/Contents/MacOS/vmux_desktop"),
                "signed app",
            );
            self.write(".vmux-seed/compatible", "compatible");
        }

        fn app(&self, revision: usize) -> PathBuf {
            let path = self
                .write(
                    &format!("release/Vmux ({revision:07x}).app/Contents/MacOS/vmux_desktop"),
                    "signed app",
                )
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(revision as u64);
            File::open(&path)
                .unwrap()
                .set_times(FileTimes::new().set_modified(time))
                .unwrap();
            path
        }

        fn incremental(&self, profile: &str, name: &str, timestamp: u64) -> PathBuf {
            let path = self
                .checkout
                .target
                .join(profile)
                .join("incremental")
                .join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("query-cache.bin"), vec![b'x'; 512 * 1024]).unwrap();
            File::open(&path)
                .unwrap()
                .set_times(
                    FileTimes::new()
                        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp)),
                )
                .unwrap();
            path
        }
    }

    #[test]
    fn cache_pruning_preserves_prepared_apps_and_runtime_libraries() {
        let fixture = Fixture::new();
        let profiles = ["debug", "release", "cef-helper", "aarch64-apple-ios/debug"];
        for profile in profiles {
            fixture.caches(profile);
        }
        assert_eq!(fixture.run(Request::clean(true)), 0);
        for profile in profiles {
            let path = fixture.checkout.target.join(profile);
            assert_eq!(
                fs::read_to_string(path.join("vmux_desktop")).unwrap(),
                "ready executable"
            );
            assert_eq!(
                fs::read_to_string(path.join("Vmux Dev.app/Contents/MacOS/vmux_desktop")).unwrap(),
                "signed app"
            );
            assert!(path.join("deps/current.dylib").is_file());
            assert!(path.join("deps/current.so").is_file());
            assert!(!path.join("deps/object.o").exists());
            assert!(!path.join("deps/library.rlib").exists());
            assert!(!path.join(".fingerprint").exists());
        }
        assert!(!fixture.checkout.target.join(".vmux-seed").exists());
    }

    #[test]
    fn active_app_keeps_runtime_and_build_outputs_but_releases_incremental_cache() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        fixture.caches("release");
        let opened = format!(
            "p12\nn{}\n",
            fixture
                .checkout
                .target
                .join("debug/Vmux Dev.app/Contents/MacOS/vmux_desktop")
                .display()
        );
        fixture.observe("12 vmux_desktop", &opened);
        assert_eq!(fixture.run(Request::clean(true)), 0);
        assert!(
            fixture
                .checkout
                .target
                .join("debug/deps/object.o")
                .is_file()
        );
        assert!(
            fixture
                .checkout
                .target
                .join("debug/deps/current.dylib")
                .is_file()
        );
        assert!(
            fixture
                .checkout
                .target
                .join("debug/build/out/generated")
                .is_file()
        );
        assert!(fixture.checkout.target.join("debug/vmux_desktop").is_file());
        assert!(!fixture.checkout.target.join("debug/incremental").exists());
        assert!(
            !fixture
                .checkout
                .target
                .join("release/deps/object.o")
                .exists()
        );
    }

    #[test]
    fn raw_cargo_lock_prevents_cache_pruning() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        let lock = File::create(fixture.checkout.target.join("debug/.cargo-lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(fixture.run(Request::clean(true)), 0);
        assert!(fixture.checkout.target.join("debug/deps/object.o").exists());
        assert!(
            fixture
                .checkout
                .target
                .join("debug/incremental/state")
                .exists()
        );
        assert!(
            fixture
                .checkout
                .target
                .join(".vmux-seed/compatible")
                .exists()
        );
    }

    #[test]
    fn untagged_target_is_not_pruned() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        fs::remove_file(fixture.checkout.target.join("CACHEDIR.TAG")).unwrap();
        assert_eq!(fixture.run(Request::clean(true)), 0);
        assert!(fixture.checkout.target.join("debug/deps/object.o").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_do_not_expose_external_directories_to_cleanup() {
        let mut fixture = Fixture::new();
        fixture.caches("debug");
        let outside = fixture.checkout.root.join("user-data");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("important"), "user data").unwrap();
        std::os::unix::fs::symlink(&outside, fixture.checkout.target.join("debug/examples"))
            .unwrap();
        assert_eq!(fixture.run(Request::clean(true)), 0);
        assert_eq!(
            fs::read_to_string(outside.join("important")).unwrap(),
            "user data"
        );
        let link = fixture.checkout.root.join("target-link");
        std::os::unix::fs::symlink(&fixture.checkout.target, &link).unwrap();
        fixture.checkout.target = link;
        assert_ne!(fixture.run(Request::clean(true)), 0);
        let dangling = fixture.checkout.root.join("dangling-target");
        std::os::unix::fs::symlink(fixture.checkout.root.join("missing"), &dangling).unwrap();
        fixture.checkout.target = dangling;
        assert_ne!(fixture.run(Request::clean(true)), 0);
    }

    #[test]
    fn cleanup_preview_keeps_cache_and_seed_marker() {
        let fixture = Fixture::new();
        fixture.caches("debug");
        assert_eq!(fixture.run(Request::clean(false)), 0);
        assert!(fixture.checkout.target.join("debug/deps/object.o").exists());
        assert!(
            fixture
                .checkout
                .target
                .join("debug/.fingerprint/current")
                .exists()
        );
        assert!(
            fixture
                .checkout
                .target
                .join(".vmux-seed/compatible")
                .exists()
        );
    }

    #[test]
    fn app_retention_keeps_current_and_running_bundles() {
        let fixture = Fixture::new();
        let mut apps = Vec::new();
        for index in 1..=7 {
            apps.push(fixture.app(index));
        }
        fixture.write(
            "release/Vmux.app/Contents/MacOS/vmux_desktop",
            "shipping app",
        );
        fixture.write(
            "release/Vmux Dev.app/Contents/MacOS/vmux_desktop",
            "debug app",
        );
        fixture.write("release/Vmux.dmg", "shipping image");
        let opened = format!(
            "p12\nn{}\n",
            apps[1].join("Contents/MacOS/vmux_desktop").display()
        );
        fixture.observe("12 vmux_desktop", &opened);
        assert_eq!(fixture.run(Request::finish("local".into())), 0);
        for (index, app) in apps.iter().enumerate() {
            assert_eq!(app.exists(), matches!(index, 0 | 1 | 5 | 6));
        }
        assert!(fixture.checkout.target.join("release/Vmux.app").exists());
        assert!(
            fixture
                .checkout
                .target
                .join("release/Vmux Dev.app")
                .exists()
        );
        assert_eq!(
            fs::read_to_string(fixture.checkout.target.join("release/Vmux.dmg")).unwrap(),
            "shipping image"
        );
    }

    #[test]
    fn cache_budget_cleanup_keeps_distribution_files() {
        let mut fixture = Fixture::new();
        fixture.caches("release");
        fixture.storage.policy.cache_limit = 1;
        fixture.write(
            "release/Vmux.app/Contents/MacOS/vmux_desktop",
            "shipping app",
        );
        fixture.write("release/Vmux.dmg", "shipping image");
        assert_eq!(fixture.run(Request::finish("release".into())), 0);
        assert!(
            !fixture
                .checkout
                .target
                .join("release/deps/object.o")
                .exists()
        );
        assert!(fixture.checkout.target.join("release/Vmux.app").exists());
        assert_eq!(
            fs::read_to_string(fixture.checkout.target.join("release/Vmux.dmg")).unwrap(),
            "shipping image"
        );
    }

    #[test]
    fn warning_only_capacity_preserves_warm_caches() {
        let mut fixture = Fixture::new();
        fixture.caches("debug");
        fixture.storage.policy.warning = u64::MAX;
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert!(fixture.checkout.target.join("debug/deps/object.o").exists());
        assert!(
            fixture
                .checkout
                .target
                .join("debug/.fingerprint/current")
                .exists()
        );
    }

    #[test]
    fn incremental_budget_evicts_old_crates_while_an_app_is_running() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.incremental_limit = 768 * 1024;
        let old = fixture.incremental("debug", "old-crate", 1);
        let recent = fixture.incremental("debug", "recent-crate", 2);
        let app = fixture.write(
            "debug/Vmux Dev.app/Contents/MacOS/vmux_desktop",
            "ready app",
        );
        fixture.observe("12 vmux_desktop", &format!("p12\nn{}\n", app.display()));
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert!(!old.exists());
        assert_eq!(
            fs::read(recent.join("query-cache.bin")).unwrap(),
            vec![b'x'; 512 * 1024]
        );
        assert_eq!(fs::read_to_string(app).unwrap(), "ready app");
    }

    #[test]
    fn incremental_budget_does_not_evict_a_locked_compilation() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.incremental_limit = 1;
        let cache = fixture.incremental("debug", "compiling-crate", 1);
        let lock = File::create(fixture.checkout.target.join("debug/.cargo-lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert_eq!(
            fs::read(cache.join("query-cache.bin")).unwrap(),
            vec![b'x'; 512 * 1024]
        );
    }

    #[test]
    fn preflight_evicts_over_budget_incremental_while_capacity_keeps_warm_outputs() {
        let mut fixture = Fixture::new();
        fixture.caches("debug");
        fixture.storage.policy.incremental_limit = 768 * 1024;
        fixture.storage.policy.cache_limit = 1;
        let old = fixture.incremental("debug", "old-crate", 1);
        let recent = fixture.incremental("debug", "recent-crate", 2);
        assert_eq!(fixture.run(Request::new(RequestKind::Preflight)), 0);
        assert!(!old.exists());
        assert_eq!(
            fs::read(recent.join("query-cache.bin")).unwrap(),
            vec![b'x'; 512 * 1024]
        );
        assert!(
            fixture
                .checkout
                .target
                .join("debug/deps/object.o")
                .is_file()
        );
        assert!(!fixture.checkout.target.join(".vmux-seed").exists());
    }

    #[test]
    fn incremental_budget_preserves_recent_caches_within_budget() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.incremental_limit = 2 * 1024 * 1024;
        let cache = fixture.incremental("debug", "warm-crate", 1);
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert_eq!(
            fs::read(cache.join("query-cache.bin")).unwrap(),
            vec![b'x'; 512 * 1024]
        );
    }

    #[test]
    fn incremental_budget_evicts_the_oldest_crate_across_profiles() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.incremental_limit = 768 * 1024;
        let recent = fixture.incremental("debug", "recent-crate", 2);
        let old = fixture.incremental("release", "old-crate", 1);
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert!(!old.exists());
        assert_eq!(
            fs::read(recent.join("query-cache.bin")).unwrap(),
            vec![b'x'; 512 * 1024]
        );
    }

    #[test]
    fn workspace_and_outside_deletion_are_refused() {
        let mut fixture = Fixture::new();
        let file = fixture.checkout.root.join("source");
        fs::write(&file, "source code").unwrap();
        fixture.checkout.target = fixture.checkout.root.clone();
        assert_ne!(fixture.run(Request::clean(true)), 0);
        assert_eq!(fs::read_to_string(file).unwrap(), "source code");
    }

    #[test]
    fn deps_budget_evicts_superseded_hashes_while_keeping_latest_and_running_app() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.deps_limit = 768 * 1024;
        let deps = fixture.checkout.target.join("debug/deps");
        fs::create_dir_all(&deps).unwrap();
        let old = deps.join("vmux_dx-0000000000000001");
        let old_dep = deps.join("vmux_dx-0000000000000001.d");
        let latest = deps.join("vmux_dx-0000000000000002");
        let runtime = deps.join("libvmux_core-0000000000000001.dylib");
        let runtime_new = deps.join("libvmux_core-0000000000000002.dylib");
        for (path, size, timestamp) in [
            (&old, 512 * 1024, 1u64),
            (&old_dep, 1024, 1),
            (&latest, 512 * 1024, 2),
            (&runtime, 64 * 1024, 1),
            (&runtime_new, 64 * 1024, 2),
        ] {
            fs::write(path, vec![b'x'; size]).unwrap();
            File::open(path)
                .unwrap()
                .set_times(
                    FileTimes::new()
                        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp)),
                )
                .unwrap();
        }
        let app = fixture.write(
            "debug/Vmux Dev.app/Contents/MacOS/vmux_desktop",
            "ready app",
        );
        fixture.observe("12 vmux_desktop", &format!("p12\nn{}\n", app.display()));
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert!(!old.exists());
        assert!(!old_dep.exists());
        assert!(latest.is_file());
        assert!(runtime.is_file());
        assert!(runtime_new.is_file());
        assert_eq!(fs::read_to_string(app).unwrap(), "ready app");
    }

    #[test]
    fn deps_budget_respects_cargo_lock_and_open_deps_files() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.deps_limit = 1;
        let deps = fixture.checkout.target.join("debug/deps");
        fs::create_dir_all(&deps).unwrap();
        let old = deps.join("policy-0000000000000001");
        let latest = deps.join("policy-0000000000000002");
        for (path, timestamp) in [(&old, 1u64), (&latest, 2)] {
            fs::write(path, vec![b'x'; 512 * 1024]).unwrap();
            File::open(path)
                .unwrap()
                .set_times(
                    FileTimes::new()
                        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp)),
                )
                .unwrap();
        }
        let lock = File::create(fixture.checkout.target.join("debug/.cargo-lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert!(old.is_file());
        drop(lock);
        fixture.observe("12 policy", &format!("p12\nn{}\n", old.display()));
        assert_eq!(fixture.run(Request::finish("dev".into())), 0);
        assert!(old.is_file());
    }

    #[test]
    fn deps_budget_evicts_superseded_hashes_across_multiple_directory_batches() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.deps_limit = 1;
        let deps = fixture.checkout.target.join("debug/deps");
        fs::create_dir_all(&deps).unwrap();
        let mut stale = Vec::new();
        let mut latest = Vec::new();
        for index in 0..4usize {
            let old = deps.join(format!("batch_{index}-0000000000000001"));
            let current = deps.join(format!("batch_{index}-0000000000000002"));
            for (path, timestamp) in [(&old, 1u64), (&current, 2)] {
                fs::write(path, vec![b'x'; 128 * 1024]).unwrap();
                File::open(path)
                    .unwrap()
                    .set_times(
                        FileTimes::new()
                            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp)),
                    )
                    .unwrap();
            }
            stale.push(old);
            latest.push(current);
        }
        assert_eq!(
            fixture.run_with_budget(
                Request::new(RequestKind::Preflight),
                ScanBudget {
                    batch: 2,
                    limit: 64
                }
            ),
            0
        );
        for path in stale {
            assert!(!path.exists(), "superseded dep should be removed: {path:?}");
        }
        for path in latest {
            assert!(path.is_file(), "latest dep should remain: {path:?}");
        }
    }

    #[test]
    fn oversized_deps_directory_stops_at_scan_budget_without_failing_preflight_or_clean() {
        let mut fixture = Fixture::new();
        fixture.storage.policy.deps_limit = 1;
        let deps = fixture.checkout.target.join("debug/deps");
        fs::create_dir_all(&deps).unwrap();
        let mut paths = Vec::new();
        for index in 0..6usize {
            let old = deps.join(format!("crate_{index}-0000000000000001"));
            let current = deps.join(format!("crate_{index}-0000000000000002"));
            for (path, timestamp) in [(&old, 1u64), (&current, 2)] {
                fs::write(path, vec![b'x'; 64 * 1024]).unwrap();
                File::open(path)
                    .unwrap()
                    .set_times(
                        FileTimes::new()
                            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp)),
                    )
                    .unwrap();
                paths.push(path.clone());
            }
        }
        assert_eq!(
            fixture.run_with_budget(
                Request::new(RequestKind::Preflight),
                ScanBudget { batch: 2, limit: 4 }
            ),
            0
        );
        assert_eq!(
            paths.iter().filter(|path| path.is_file()).count(),
            paths.len(),
            "truncated stale-deps scan must skip pruning rather than failing or deleting incomplete pairs"
        );

        assert_eq!(
            fixture.run_with_budget(Request::clean(true), ScanBudget { batch: 2, limit: 3 }),
            0
        );
        let remaining = paths.iter().filter(|path| path.is_file()).count();
        assert_eq!(
            remaining,
            paths.len() - 3,
            "bounded clean pass should cap continuation batches at the remaining scan budget"
        );
    }
}
