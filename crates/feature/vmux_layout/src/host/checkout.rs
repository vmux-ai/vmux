use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;
use vmux_ecs::{
    Cwd, JobRequest, JobResult, Operation, OperationPlugin, OperationState, OperationTarget,
    OperationTargetRevision,
};
use vmux_git::state::CheckedOutBranch;
use vmux_git::worktree::CheckoutInfo;

use super::deadline::{DeadlineOwner, DeadlineRegistration, DeadlineScheduler};
use crate::worktree::{TabWorktreeActivation, WorktreeName};

pub(crate) use io::{Canonicalize, Identify, Inspect};
pub use recovery::{CheckoutRecoveryComplete, RecoverCheckouts};

mod cleanup;
mod custody;
mod io;
mod lease;
mod recovery;

pub struct CheckoutPlugin;

impl Plugin for CheckoutPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.init_resource::<DeadlineScheduler>()
            .add_plugins((
                io::IoPlugin,
                cleanup::CleanupPlugin,
                custody::CustodyPlugin,
                recovery::RecoveryPlugin,
            ))
            .add_message::<CreateCheckout>()
            .add_message::<CreateBranchCheckout>()
            .add_message::<SelectBranchCheckout>()
            .add_message::<RestoreCheckout>()
            .add_message::<CommitCheckout>()
            .add_message::<CancelCheckout>()
            .add_message::<CheckoutPrepared>()
            .add_message::<CheckoutFailed>()
            .add_systems(
                Update,
                (
                    (
                        accept_create,
                        accept_branch,
                        accept_select,
                        accept_restore,
                        cancel,
                        retry_lock,
                        input,
                        identity,
                        repository,
                        lock,
                        validate_branch,
                        head,
                        initial_commit,
                        read_head,
                    )
                        .chain(),
                    (
                        base_remote,
                        base_candidates,
                        base_configuration,
                        verify_base,
                    )
                        .chain(),
                    (
                        registrations,
                        branches,
                        prune_registration,
                        resume_waiters,
                        destination,
                        storage,
                        canonical_storage,
                        choose_path,
                        inspect_destination,
                        canonical_destination,
                        normalize_recovery,
                        reserve,
                        claim,
                        adopt,
                        resolve_base,
                        observe_identity,
                        evidence,
                    )
                        .chain(),
                    (
                        journal_directory,
                        journal,
                        add,
                        created,
                        record_base,
                        inspect,
                        verify_link,
                        verify_branch,
                        execution,
                        commit,
                        abort,
                        cleanup_journal,
                    )
                        .chain(),
                )
                    .chain()
                    .after(recovery::RecoverySet)
                    .before(cleanup::CleanupSet),
            )
            .add_systems(Update, finish_operations.after(custody::CustodySet))
            .add_systems(
                Update,
                (retire_released, retire_repositories)
                    .chain()
                    .after(finish_operations),
            )
            .add_observer(retire_target);
    }
}

#[derive(Message)]
pub struct CreateCheckout {
    pub operation: Entity,
    pub directory: PathBuf,
    pub managed_root: PathBuf,
    pub slug_hint: String,
}

#[derive(Message)]
pub struct CreateBranchCheckout {
    pub operation: Entity,
    pub directory: PathBuf,
    pub managed_root: PathBuf,
    pub branch: String,
}

#[derive(Message)]
pub struct SelectBranchCheckout {
    pub operation: Entity,
    pub directory: PathBuf,
    pub managed_root: PathBuf,
    pub branch: String,
}

#[derive(Message)]
pub struct RestoreCheckout {
    pub operation: Entity,
    pub directory: PathBuf,
    pub managed_root: PathBuf,
    pub checkout: PathBuf,
    pub branch: String,
    pub base_ref: String,
}

#[derive(Message)]
pub struct CommitCheckout {
    pub operation: Entity,
    pub target: Entity,
    pub revision: u64,
}

#[derive(Message)]
pub struct CancelCheckout {
    pub operation: Entity,
}

#[derive(Message)]
pub struct CheckoutPrepared {
    pub operation: Entity,
}

#[derive(Message)]
pub struct CheckoutFailed {
    pub operation: Entity,
    pub error: String,
}

#[derive(Component)]
struct Generated;
#[derive(Component)]
struct BranchCreated;
#[derive(Component)]
struct BranchSelected;
#[derive(Component)]
struct Restored;
#[derive(Component)]
struct AbortRequested;
#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct RequestedBranch(String);
#[derive(Component)]
struct BaseRef(String);
#[derive(Component)]
struct SourceBranch(String);
#[derive(Component)]
struct DefaultBase(String);
#[derive(Component)]
struct ConfiguredBase(String);
#[derive(Component)]
struct BaseAttempt(usize);
#[derive(Component)]
struct PrunedRegistration;
#[derive(Component)]
struct RestoreMissing;
#[derive(Component)]
struct RecoveryRegistration(PathBuf);
#[derive(Component)]
struct StorageRoot(PathBuf);
#[derive(Component)]
struct Destination(PathBuf);
#[derive(Component)]
struct RelativeDirectory(PathBuf);
#[derive(Component)]
struct RepositoryIdentity(CheckoutInfo);
#[derive(Component)]
struct SourceRepository(Entity);
#[derive(Component)]
struct RepositoryGuard(Arc<File>);
#[derive(Component)]
struct Job(Entity);
#[derive(Component)]
struct RetryDeadline {
    at: std::time::Instant,
    _registration: DeadlineRegistration,
}
#[derive(Component)]
struct LeaseCheckout(Entity);
#[derive(Component)]
struct Committed;

#[derive(Component, Clone, Copy)]
pub struct CheckoutTargetRevision(pub u64);
#[derive(Component)]
struct BranchValidated;
#[derive(Component)]
struct Registration;
#[derive(Component)]
struct Prunable;
#[derive(Component)]
struct LocalBranch;
#[derive(Component)]
struct StorageDirectory(PathBuf);
#[derive(Component)]
struct PathAttempt(u64);
#[derive(Component)]
struct Existing;
#[derive(Component)]
struct SourceDirectory(PathBuf);
#[derive(Component)]
struct JournalPath(PathBuf);
#[derive(Component)]
struct MutationMade;
#[derive(Component)]
struct MutationAttempted;
#[derive(Component)]
struct IdentityObserved;
#[derive(Component)]
struct CleanupFinished;
#[derive(Component)]
struct RetainedReason(String);
#[derive(Component)]
struct CreationRevision(String);
#[derive(Component)]
struct JournalWritten;
#[derive(Component)]
struct AddedCheckout(CheckoutInfo);
#[derive(Component)]
struct ActivationMetadata(crate::tab::TabWorktree);

#[derive(Component)]
pub struct PreparedCheckout {
    pub activation: TabWorktreeActivation,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct Progress(Phase);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Input,
    Identity,
    Repository,
    Lock,
    LockPause,
    Head,
    Registrations,
    ValidateBranch,
    InitialCommit,
    ReadHead,
    ReadDetached,
    DefaultBase,
    BaseCandidates,
    BaseConfiguration,
    VerifyBase,
    Branches,
    Destination,
    Storage,
    CanonicalStorage,
    ChoosePath,
    InspectDestination,
    CanonicalDestination,
    NormalizeRecovery,
    PruneRegistration,
    Claim,
    Adopt,
    ResolveBase,
    ObserveIdentity,
    Evidence,
    JournalDirectory,
    Journal,
    Add,
    RecordBase,
    Inspect,
    VerifyLink,
    VerifyCheckoutBranch,
    Execution,
    Prepared,
    Waiting,
    CleanupLock,
    CleanupLockPause,
    CleanupRegistration,
    CleanupIdentity,
    CleanupHead,
    CleanupRevision,
    CleanupStatus,
    CleanupCustodyPlan,
    CleanupCustodyJournal,
    CleanupCustodyMove,
    CleanupCustodyInspect,
    CleanupCustodyRecord,
    CleanupCustodyReturnCheck,
    CleanupCustodyReturnPlan,
    CleanupCustodyReturn,
    CleanupReferences,
    CleanupReferenceReview,
    CleanupReferencePause,
    CleanupRemove,
    CleanupBranch,
    CleanupJournal,
    Retained,
    Complete,
}

fn accept_create(
    mut requests: MessageReader<CreateCheckout>,
    eligible: Query<&OperationState, (With<Operation>, Without<Progress>)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !eligible
            .get(request.operation)
            .is_ok_and(|state| *state == OperationState::Pending)
        {
            continue;
        }
        commands.entity(request.operation).insert((
            OperationState::Running,
            Cwd(request.directory.clone()),
            StorageRoot(request.managed_root.clone()),
            WorktreeName(request.slug_hint.clone()),
            Generated,
            Progress(Phase::Input),
        ));
    }
}

fn accept_branch(
    mut requests: MessageReader<CreateBranchCheckout>,
    eligible: Query<&OperationState, (With<Operation>, Without<Progress>)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !eligible
            .get(request.operation)
            .is_ok_and(|state| *state == OperationState::Pending)
        {
            continue;
        }
        commands.entity(request.operation).insert((
            OperationState::Running,
            Cwd(request.directory.clone()),
            StorageRoot(request.managed_root.clone()),
            RequestedBranch(request.branch.trim().to_string()),
            BranchCreated,
            Progress(Phase::Input),
        ));
    }
}

fn accept_select(
    mut requests: MessageReader<SelectBranchCheckout>,
    eligible: Query<&OperationState, (With<Operation>, Without<Progress>)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !eligible
            .get(request.operation)
            .is_ok_and(|state| *state == OperationState::Pending)
        {
            continue;
        }
        commands.entity(request.operation).insert((
            OperationState::Running,
            Cwd(request.directory.clone()),
            StorageRoot(request.managed_root.clone()),
            RequestedBranch(request.branch.trim().to_string()),
            BranchSelected,
            Progress(Phase::Input),
        ));
    }
}

fn accept_restore(
    mut requests: MessageReader<RestoreCheckout>,
    eligible: Query<&OperationState, (With<Operation>, Without<Progress>)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !eligible
            .get(request.operation)
            .is_ok_and(|state| *state == OperationState::Pending)
        {
            continue;
        }
        commands.entity(request.operation).insert((
            OperationState::Running,
            Cwd(request.directory.clone()),
            StorageRoot(request.managed_root.clone()),
            RequestedBranch(request.branch.trim().to_string()),
            BaseRef(request.base_ref.clone()),
            Destination(request.checkout.clone()),
            Restored,
            Progress(Phase::Input),
        ));
    }
}

fn cancel(mut requests: MessageReader<CancelCheckout>, mut commands: Commands) {
    for request in requests.read() {
        commands
            .entity(request.operation)
            .try_insert(AbortRequested);
    }
}

fn abort(
    operations: Query<
        (
            Entity,
            &Progress,
            Option<&Job>,
            Has<MutationAttempted>,
            Has<IdentityObserved>,
        ),
        Or<(With<AbortRequested>, With<Failure>)>,
    >,
    mut commands: Commands,
) {
    for (operation, progress, job, attempted, observed) in &operations {
        if job.is_some()
            || progress.0.cleanup()
            || matches!(progress.0, Phase::Complete | Phase::Evidence)
        {
            continue;
        }
        let next = if attempted && !observed {
            Phase::ObserveIdentity
        } else {
            Phase::Complete
        };
        commands
            .entity(operation)
            .insert(Progress(next))
            .remove::<PreparedCheckout>();
    }
}

impl Phase {
    fn cleanup(self) -> bool {
        matches!(
            self,
            Self::CleanupLock
                | Self::CleanupLockPause
                | Self::CleanupRegistration
                | Self::CleanupIdentity
                | Self::CleanupHead
                | Self::CleanupRevision
                | Self::CleanupStatus
                | Self::CleanupCustodyPlan
                | Self::CleanupCustodyJournal
                | Self::CleanupCustodyMove
                | Self::CleanupCustodyInspect
                | Self::CleanupCustodyRecord
                | Self::CleanupCustodyReturnCheck
                | Self::CleanupCustodyReturnPlan
                | Self::CleanupCustodyReturn
                | Self::CleanupReferences
                | Self::CleanupReferenceReview
                | Self::CleanupReferencePause
                | Self::CleanupRemove
                | Self::CleanupBranch
                | Self::CleanupJournal
                | Self::Retained
        )
    }
}

fn retire_target(
    removed: On<Remove, OperationTarget>,
    operations: Query<(), With<Progress>>,
    mut commands: Commands,
) {
    let operation = removed.event_target();
    if operations.contains(operation) {
        commands.entity(operation).try_insert(AbortRequested);
    }
}

fn commit(
    mut requests: MessageReader<CommitCheckout>,
    operations: Query<
        (
            &OperationTarget,
            &OperationTargetRevision,
            &Progress,
            &PreparedCheckout,
            Option<&LeaseCheckout>,
            Option<&JournalPath>,
        ),
        (Without<AbortRequested>, Without<Failure>),
    >,
    targets: Query<(
        &CheckoutTargetRevision,
        Option<&crate::tab::TabWorktree>,
        Option<&vmux_command::CommandBarWorkDirectory>,
    )>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for request in requests.read() {
        let Ok((target, revision, progress, prepared, checkout, journal)) =
            operations.get(request.operation)
        else {
            continue;
        };
        if target.entity() != request.target
            || revision.0 != request.revision
            || progress.0 != Phase::Prepared
        {
            continue;
        }
        let Ok((current, worktree, directory)) = targets.get(request.target) else {
            continue;
        };
        if current.0 != request.revision {
            continue;
        }
        let activation = &prepared.activation;
        let applied = match worktree {
            Some(worktree) => {
                worktree.checkout_dir == activation.metadata.checkout_dir
                    && worktree.branch == activation.metadata.branch
            }
            None => directory
                .is_some_and(|directory| directory.0 == activation.execution_dir.to_string_lossy()),
        };
        if !applied || checkout.is_some_and(|checkout| !leases.owns(checkout.0, request.operation))
        {
            continue;
        }
        if let Some(checkout) = checkout {
            effects
                .commands
                .entity(checkout.0)
                .insert(lease::Lease(lease::Phase::Committed))
                .remove::<(OperationTarget, lease::DirectoryGuard)>();
        }
        let next = if journal.is_some() {
            Phase::CleanupJournal
        } else {
            Phase::Complete
        };
        effects
            .commands
            .entity(request.operation)
            .insert((Committed, Progress(next)));
    }
}

#[derive(SystemParam)]
struct Effects<'w, 's> {
    commands: Commands<'w, 's>,
    deadlines: Res<'w, DeadlineScheduler>,
    proxy: Option<Res<'w, bevy::winit::EventLoopProxyWrapper>>,
    stopped: Query<'w, 's, (), Or<(With<AbortRequested>, With<Failure>)>>,
    failures: Query<'w, 's, &'static Failure>,
}

impl Effects<'_, '_> {
    fn request<J: vmux_ecs::Job>(&mut self, owner: Entity, request: J) -> Entity {
        self.commands
            .spawn((
                Name::new("Managed checkout IO"),
                vmux_ecs::Active,
                Operation::bundle("managed-checkout-io").expect("fixed checkout IO kind"),
                JobRequest(request),
                ChildOf(owner),
            ))
            .id()
    }

    fn finish(&mut self, owner: Entity, job: Entity, next: Phase) {
        self.commands.entity(job).try_despawn();
        self.commands
            .entity(owner)
            .remove::<Job>()
            .insert(Progress(next));
    }

    fn fail(&mut self, owner: Entity, job: Entity, error: String) {
        let error = match self.failures.get(owner) {
            Ok(previous) => format!("{}; {error}", previous.0),
            Err(_) => error,
        };
        self.commands.entity(job).try_despawn();
        self.commands
            .entity(owner)
            .remove::<Job>()
            .insert((Failure(error), Progress(Phase::Complete)));
    }

    fn retain(&mut self, owner: Entity, job: Option<Entity>, reason: String) {
        if let Some(job) = job {
            self.commands.entity(job).try_despawn();
        }
        self.commands
            .entity(owner)
            .remove::<Job>()
            .insert((RetainedReason(reason), Progress(Phase::Retained)));
    }
}

fn input(
    operations: Query<(Entity, &Cwd, &Progress, Option<&Job>), With<Operation>>,
    completed: Query<&JobResult<Canonicalize>>,
    targets: Query<&OperationTarget>,
    mut effects: Effects,
) {
    for (operation, directory, progress, job) in &operations {
        if progress.0 != Phase::Input {
            continue;
        }
        if targets.get(operation).is_err() {
            effects.commands.entity(operation).insert(AbortRequested);
            if job.is_none() {
                continue;
            }
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Canonicalize(directory.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert((Cwd(path.clone()), SourceDirectory(path.clone())));
                effects.finish(operation, job.0, Phase::Identity);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn identity(
    operations: Query<(Entity, &Cwd, &Progress, Option<&Job>), With<Operation>>,
    completed: Query<&JobResult<Identify>>,
    mut effects: Effects,
) {
    for (operation, directory, progress, job) in &operations {
        if progress.0 != Phase::Identity {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Identify(directory.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(checkout)) => match directory.0.strip_prefix(&checkout.root) {
                Ok(relative) => {
                    effects.commands.entity(operation).insert((
                        RepositoryIdentity(checkout.clone()),
                        RelativeDirectory(relative.to_path_buf()),
                    ));
                    effects.finish(operation, job.0, Phase::Repository);
                }
                Err(_) => effects.fail(
                    operation,
                    job.0,
                    "project directory escapes checkout".into(),
                ),
            },
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn repository(
    operations: Query<(Entity, &RepositoryIdentity, &Progress)>,
    repositories: Query<(Entity, &lease::CommonDirectory), With<lease::Repository>>,
    mut effects: Effects,
) {
    let mut accepted = std::collections::HashMap::new();
    for (operation, checkout, progress) in &operations {
        if progress.0 != Phase::Repository || effects.stopped.contains(operation) {
            continue;
        }
        let repository = match repositories
            .iter()
            .find(|(_, directory)| directory.0 == checkout.0.common_dir)
        {
            Some((repository, _)) => repository,
            None => match accepted.get(&checkout.0.common_dir) {
                Some(repository) => *repository,
                None => {
                    let repository = effects
                        .commands
                        .spawn((
                            Name::new("Managed checkout repository"),
                            vmux_ecs::Active,
                            lease::Repository,
                            lease::CommonDirectory(checkout.0.common_dir.clone()),
                            Cwd(checkout.0.common_dir.clone()),
                        ))
                        .id();
                    accepted.insert(checkout.0.common_dir.clone(), repository);
                    repository
                }
            },
        };
        effects
            .commands
            .entity(operation)
            .insert((SourceRepository(repository), Progress(Phase::Lock)));
    }
}

fn lock(
    operations: Query<(Entity, &RepositoryIdentity, &Progress, Option<&Job>), With<Operation>>,
    completed: Query<&JobResult<io::Acquire>>,
    mut effects: Effects,
) {
    for (operation, repository, progress, job) in &operations {
        if progress.0 != Phase::Lock {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, io::Acquire(repository.0.common_dir.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(guard))) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(RepositoryGuard(Arc::clone(guard)));
                effects.finish(operation, job.0, Phase::Head);
            }
            Ok(Ok(None)) => {
                effects.finish(operation, job.0, Phase::LockPause);
                let at = std::time::Instant::now() + std::time::Duration::from_millis(20);
                let registration = effects.deadlines.register(
                    DeadlineOwner::Checkout(operation),
                    at,
                    effects.proxy.as_deref(),
                );
                effects.commands.entity(operation).insert(RetryDeadline {
                    at,
                    _registration: registration,
                });
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn validate_branch(
    operations: Query<
        (
            Entity,
            &RepositoryIdentity,
            &RepositoryGuard,
            &RequestedBranch,
            &Progress,
            Option<&Job>,
        ),
        Without<BranchValidated>,
    >,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, branch, progress, job) in &operations {
        if !matches!(progress.0, Phase::Head | Phase::ValidateBranch) {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "check-ref-format".into(),
                        "--branch".into(),
                        branch.0.clone(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects
                .commands
                .entity(operation)
                .insert((Job(task), Progress(Phase::ValidateBranch)));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => {
                effects.commands.entity(operation).insert(BranchValidated);
                effects.finish(operation, job.0, Phase::Head);
            }
            Ok(Ok((_, error, _))) => effects.fail(operation, job.0, error.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn head(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Progress,
        Option<&Job>,
        Has<BranchSelected>,
        Has<Restored>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job, selected, restored) in &operations {
        if progress.0 != Phase::Head {
            continue;
        }
        if restored {
            effects
                .commands
                .entity(operation)
                .insert(Progress(Phase::Registrations));
            continue;
        }
        if selected {
            effects
                .commands
                .entity(operation)
                .insert(Progress(Phase::ReadHead));
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec!["rev-parse".into(), "--verify".into(), "HEAD".into()],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => effects.finish(operation, job.0, Phase::ReadHead),
            Ok(Ok(_)) => effects.finish(operation, job.0, Phase::InitialCommit),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn initial_commit(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job) in &operations {
        if progress.0 != Phase::InitialCommit {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "commit".into(),
                        "--allow-empty".into(),
                        "--no-gpg-sign".into(),
                        "--message".into(),
                        "Initial commit".into(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => effects.finish(operation, job.0, Phase::ReadHead),
            Ok(Ok((_, error, _))) => effects.fail(operation, job.0, error.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn read_head(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Progress,
        Option<&Job>,
        Has<Restored>,
        Has<BranchSelected>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job, restored, selected) in &operations {
        if !matches!(progress.0, Phase::ReadHead | Phase::ReadDetached) {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let arguments = if progress.0 == Phase::ReadDetached {
                vec!["rev-parse".into(), "--short".into(), "HEAD".into()]
            } else {
                vec![
                    "symbolic-ref".into(),
                    "--quiet".into(),
                    "--short".into(),
                    "HEAD".into(),
                ]
            };
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments,
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((reference, _, true))) => {
                if progress.0 == Phase::ReadHead {
                    effects
                        .commands
                        .entity(operation)
                        .insert(SourceBranch(reference.trim().to_string()));
                }
                if !restored {
                    effects
                        .commands
                        .entity(operation)
                        .insert(BaseRef(reference.trim().to_string()));
                }
                let next = if selected {
                    Phase::DefaultBase
                } else {
                    Phase::Registrations
                };
                effects.finish(operation, job.0, next);
            }
            Ok(Ok(_)) if progress.0 == Phase::ReadHead => {
                let next = if selected {
                    Phase::DefaultBase
                } else {
                    Phase::ReadDetached
                };
                effects.finish(operation, job.0, next)
            }
            Ok(Ok((_, error, _))) => effects.fail(operation, job.0, error.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn retry_lock(operations: Query<(Entity, &Progress, &RetryDeadline)>, mut effects: Effects) {
    for (operation, progress, deadline) in &operations {
        let next = match progress.0 {
            Phase::LockPause => Phase::Lock,
            Phase::CleanupLockPause => Phase::CleanupLock,
            Phase::CleanupReferencePause => Phase::CleanupReferences,
            _ => continue,
        };
        if std::time::Instant::now() < deadline.at {
            continue;
        }
        effects
            .commands
            .entity(operation)
            .insert(Progress(next))
            .remove::<RetryDeadline>();
    }
}

impl BaseRef {
    fn native(reference: &str) -> Self {
        let reference = reference
            .strip_prefix("refs/remotes/origin/")
            .or_else(|| reference.strip_prefix("refs/heads/"))
            .unwrap_or(reference);
        Self(reference.to_string())
    }
}

fn base_remote(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job) in &operations {
        if progress.0 != Phase::DefaultBase {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec!["symbolic-ref".into(), "refs/remotes/origin/HEAD".into()],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((reference, _, true))) if !reference.trim().is_empty() => {
                effects
                    .commands
                    .entity(operation)
                    .insert(DefaultBase(reference.trim().to_string()));
                effects.finish(operation, job.0, Phase::BaseConfiguration);
            }
            _ => {
                effects.commands.entity(operation).insert(BaseAttempt(0));
                effects.finish(operation, job.0, Phase::BaseCandidates);
            }
        }
    }
}

fn base_candidates(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &BaseAttempt,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, attempt, progress, job) in &operations {
        if progress.0 != Phase::BaseCandidates {
            continue;
        }
        let candidate = match attempt.0 {
            0 => "refs/remotes/origin/main",
            1 => "refs/remotes/origin/master",
            2 => "refs/heads/main",
            3 => "refs/heads/master",
            _ => {
                effects.commands.entity(operation).insert((
                    DefaultBase(String::new()),
                    Progress(Phase::BaseConfiguration),
                ));
                continue;
            }
        };
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "rev-parse".into(),
                        "--verify".into(),
                        "--quiet".into(),
                        candidate.into(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(DefaultBase(candidate.into()));
                effects.finish(operation, job.0, Phase::BaseConfiguration);
            }
            _ => {
                effects
                    .commands
                    .entity(operation)
                    .insert(BaseAttempt(attempt.0 + 1));
                effects.finish(operation, job.0, Phase::BaseCandidates);
            }
        }
    }
}

fn base_configuration(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &DefaultBase,
        Option<&SourceBranch>,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, default, branch, progress, job) in &operations {
        if progress.0 != Phase::BaseConfiguration {
            continue;
        }
        let Some(branch) = branch else {
            effects
                .commands
                .entity(operation)
                .insert((BaseRef::native(&default.0), Progress(Phase::Registrations)));
            continue;
        };
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "config".into(),
                        "--get".into(),
                        format!("branch.{}.vmux-base", branch.0),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((reference, _, true))) if !reference.trim().is_empty() => {
                effects
                    .commands
                    .entity(operation)
                    .insert(ConfiguredBase(reference.trim().to_string()));
                effects.finish(operation, job.0, Phase::VerifyBase);
            }
            _ => {
                effects
                    .commands
                    .entity(operation)
                    .insert(BaseRef::native(&default.0));
                effects.finish(operation, job.0, Phase::Registrations);
            }
        }
    }
}

fn verify_base(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &DefaultBase,
        &ConfiguredBase,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, default, configured, progress, job) in &operations {
        if progress.0 != Phase::VerifyBase {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "rev-parse".into(),
                        "--verify".into(),
                        "--quiet".into(),
                        configured.0.clone(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        let reference = match &result.0 {
            Ok(Ok((_, _, true))) => &configured.0,
            _ => &default.0,
        };
        effects
            .commands
            .entity(operation)
            .insert(BaseRef::native(reference));
        effects.finish(operation, job.0, Phase::Registrations);
    }
}

fn registrations(
    operations: Query<
        (
            Entity,
            &RepositoryIdentity,
            &RepositoryGuard,
            &Progress,
            Option<&Job>,
        ),
        With<Operation>,
    >,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job) in &operations {
        if progress.0 != Phase::Registrations {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "worktree".into(),
                        "list".into(),
                        "--porcelain".into(),
                        "-z".into(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((stdout, _, true))) => {
                let mut path = None;
                let mut branch = None;
                let mut prunable = false;
                for field in stdout.split('\0').chain(std::iter::once("")) {
                    if field.is_empty() {
                        if let Some(path) = path.take() {
                            let mut row = effects.commands.spawn((
                                Name::new("Git worktree registration"),
                                Registration,
                                Cwd(path),
                                ChildOf(operation),
                            ));
                            if let Some(branch) = branch.take() {
                                row.insert(CheckedOutBranch(branch));
                            }
                            if prunable {
                                row.insert(Prunable);
                            }
                        }
                        prunable = false;
                    } else if let Some(value) = field.strip_prefix("worktree ") {
                        path = Some(PathBuf::from(value));
                    } else if let Some(value) = field.strip_prefix("branch refs/heads/") {
                        branch = Some(value.to_string());
                    } else if field == "prunable" || field.starts_with("prunable ") {
                        prunable = true;
                    }
                }
                effects.finish(operation, job.0, Phase::Branches);
            }
            Ok(Ok((_, stderr, _))) => effects.fail(operation, job.0, stderr.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn branches(
    operations: Query<
        (
            Entity,
            &RepositoryIdentity,
            &RepositoryGuard,
            &Progress,
            Option<&Job>,
        ),
        With<Operation>,
    >,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job) in &operations {
        if progress.0 != Phase::Branches {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "branch".into(),
                        "--sort=-committerdate".into(),
                        "--format=%(refname:short)".into(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((stdout, _, true))) => {
                for branch in stdout.lines() {
                    if !branch.is_empty() {
                        effects.commands.spawn((
                            Name::new("Git local branch observation"),
                            LocalBranch,
                            CheckedOutBranch(branch.to_string()),
                            ChildOf(operation),
                        ));
                    }
                }
                effects.finish(operation, job.0, Phase::Destination);
            }
            Ok(Ok((_, stderr, _))) => effects.fail(operation, job.0, stderr.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

impl StorageRoot {
    fn repository(&self, repository: &CheckoutInfo) -> PathBuf {
        let name = repository
            .common_dir
            .parent()
            .and_then(|path| path.file_name())
            .or_else(|| repository.root.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("repository");
        let digest = Sha256::digest(repository.common_dir.as_os_str().as_bytes());
        let hash = format!("{digest:x}");
        self.0
            .join(format!("{}-{}", WorktreeName::sanitize(name), &hash[..12]))
    }
}

fn destination(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &StorageRoot,
        &Progress,
        Option<&RequestedBranch>,
        Has<BranchCreated>,
        Has<BranchSelected>,
        Has<Restored>,
    )>,
    registrations: Query<(&Cwd, Option<&CheckedOutBranch>, &ChildOf), With<Registration>>,
    local: Query<(&CheckedOutBranch, &ChildOf), With<LocalBranch>>,
    mut effects: Effects,
) {
    for (operation, repository, storage, progress, branch, created, selected, restored) in
        &operations
    {
        if progress.0 != Phase::Destination {
            continue;
        }
        if !storage.0.is_absolute() {
            effects.commands.entity(operation).insert((
                Failure("managed worktree root must be absolute".into()),
                Progress(Phase::Complete),
            ));
            continue;
        }
        let matching = branch.and_then(|branch| {
            registrations.iter().find(|(_, seen, parent)| {
                parent.parent() == operation && seen.is_some_and(|seen| seen.0 == branch.0)
            })
        });
        if created
            && (matching.is_some()
                || branch.is_some_and(|branch| {
                    local
                        .iter()
                        .any(|(seen, parent)| parent.parent() == operation && seen.0 == branch.0)
                }))
        {
            effects.commands.entity(operation).insert((
                Failure("Branch already exists or is checked out".into()),
                Progress(Phase::Complete),
            ));
            continue;
        }
        if selected && let Some((path, _, _)) = matching {
            effects.commands.entity(operation).insert((
                Destination(path.0.clone()),
                Existing,
                Progress(Phase::InspectDestination),
            ));
            continue;
        }
        if restored {
            effects
                .commands
                .entity(operation)
                .insert(StorageDirectory(storage.repository(&repository.0)));
            effects
                .commands
                .entity(operation)
                .insert(Progress(Phase::InspectDestination));
            continue;
        }
        effects.commands.entity(operation).insert((
            StorageDirectory(storage.repository(&repository.0)),
            PathAttempt(1),
            Progress(Phase::Storage),
        ));
    }
}

fn storage(
    operations: Query<(Entity, &StorageDirectory, &Progress, Option<&Job>)>,
    completed: Query<&JobResult<io::CreateDirectory>>,
    mut effects: Effects,
) {
    for (operation, storage, progress, job) in &operations {
        if progress.0 != Phase::Storage {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, io::CreateDirectory(storage.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(())) => effects.finish(operation, job.0, Phase::CanonicalStorage),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn canonical_storage(
    operations: Query<(
        Entity,
        &StorageDirectory,
        &Progress,
        Option<&Job>,
        Has<RestoreMissing>,
    )>,
    completed: Query<&JobResult<Canonicalize>>,
    mut effects: Effects,
) {
    for (operation, storage, progress, job, restoring) in &operations {
        if progress.0 != Phase::CanonicalStorage {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Canonicalize(storage.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(StorageDirectory(path.clone()));
                let next = if restoring {
                    Phase::NormalizeRecovery
                } else {
                    Phase::ChoosePath
                };
                effects.finish(operation, job.0, next);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn choose_path(
    operations: Query<(
        Entity,
        &StorageDirectory,
        &Progress,
        &PathAttempt,
        Option<&WorktreeName>,
        Option<&RequestedBranch>,
        Has<Generated>,
    )>,
    registrations: Query<(&Cwd, &ChildOf), With<Registration>>,
    branches: Query<(&CheckedOutBranch, &ChildOf), With<LocalBranch>>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (operation, storage, progress, attempt, hint, branch, generated) in &operations {
        if progress.0 != Phase::ChoosePath {
            continue;
        }
        let base = match (hint, branch) {
            (Some(hint), _) => WorktreeName::sanitize(&hint.0),
            (_, Some(branch)) => {
                WorktreeName::sanitize(branch.0.strip_prefix("vmux/").unwrap_or(&branch.0))
            }
            _ => "task".into(),
        };
        let mut candidate = attempt.0;
        let (path, branch) = loop {
            let slug = if candidate == 1 {
                base.clone()
            } else {
                format!("{base}-{}", candidate)
            };
            let path = storage.0.join(&slug);
            let branch = if generated {
                format!("vmux/{slug}")
            } else {
                branch.unwrap().0.clone()
            };
            let collision = registrations
                .iter()
                .any(|(seen, parent)| parent.parent() == operation && seen.0 == path)
                || generated
                    && branches
                        .iter()
                        .any(|(seen, parent)| parent.parent() == operation && seen.0 == branch);
            let retained_path = generated
                && leases
                    .find(&path)
                    .is_some_and(|(_, phase, _)| phase != lease::Phase::Released);
            if collision || retained_path {
                candidate += 1;
                continue;
            }
            break (path, branch);
        };
        effects.commands.entity(operation).insert((
            PathAttempt(candidate),
            Destination(path),
            RequestedBranch(branch),
            Progress(Phase::InspectDestination),
        ));
    }
}

fn inspect_destination(
    operations: Query<(
        Entity,
        &Destination,
        &Progress,
        Option<&Job>,
        Option<&PathAttempt>,
        Has<Existing>,
        Has<Restored>,
        Has<BranchCreated>,
        Has<PrunedRegistration>,
    )>,
    completed: Query<&JobResult<Inspect>>,
    mut effects: Effects,
) {
    for (operation, destination, progress, job, attempt, existing, restored, created, pruned) in
        &operations
    {
        if progress.0 != Phase::InspectDestination {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Inspect(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(metadata)))
                if (metadata.is_dir() || metadata.is_symlink()) && (existing || restored) =>
            {
                effects.commands.entity(operation).insert(Existing);
                effects.finish(operation, job.0, Phase::CanonicalDestination);
            }
            Ok(Ok(Some(_))) if !existing && !restored && !created => {
                effects
                    .commands
                    .entity(operation)
                    .insert(PathAttempt(attempt.unwrap().0 + 1));
                effects.finish(operation, job.0, Phase::ChoosePath);
            }
            Ok(Ok(Some(_))) => effects.fail(
                operation,
                job.0,
                "worktree destination already exists or is not a directory".into(),
            ),
            Ok(Ok(None)) if existing && !pruned => {
                effects.finish(operation, job.0, Phase::PruneRegistration)
            }
            Ok(Ok(None)) if existing => effects.fail(
                operation,
                job.0,
                "branch is registered to a missing worktree".into(),
            ),
            Ok(Ok(None)) if restored => {
                effects.commands.entity(operation).insert(RestoreMissing);
                effects.finish(operation, job.0, Phase::Storage);
            }
            Ok(Ok(None)) => effects.finish(operation, job.0, Phase::Claim),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn journal_directory(
    operations: Query<(Entity, &StorageDirectory, &Progress, Option<&Job>)>,
    completed: Query<&JobResult<io::CreateDirectory>>,
    mut effects: Effects,
) {
    for (operation, storage, progress, job) in &operations {
        if progress.0 != Phase::JournalDirectory {
            continue;
        }
        let path = storage.0.join(".leases");
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, io::CreateDirectory(path));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(())) => {
                effects.commands.entity(operation).insert(JournalPath(path));
                effects.finish(operation, job.0, Phase::Journal);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn canonical_destination(
    operations: Query<(Entity, &Destination, &Progress, Option<&Job>)>,
    completed: Query<&JobResult<Canonicalize>>,
    mut effects: Effects,
) {
    for (operation, destination, progress, job) in &operations {
        if progress.0 != Phase::CanonicalDestination {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Canonicalize(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(path)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert(Destination(path.clone()));
                effects.finish(operation, job.0, Phase::Claim);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn prune_registration(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Progress,
        Option<&Job>,
        Has<PrunedRegistration>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    observations: Query<(Entity, &ChildOf), Or<(With<Registration>, With<LocalBranch>)>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, progress, job, pruned) in &operations {
        if progress.0 != Phase::PruneRegistration {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            if pruned {
                effects.commands.entity(operation).insert((
                    Failure("missing registration could not be pruned".into()),
                    Progress(Phase::Complete),
                ));
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "worktree".into(),
                        "prune".into(),
                        "--expire".into(),
                        "now".into(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => {
                for (entity, parent) in &observations {
                    if parent.parent() == operation {
                        effects.commands.entity(entity).try_despawn();
                    }
                }
                effects
                    .commands
                    .entity(operation)
                    .insert(PrunedRegistration)
                    .remove::<Existing>();
                effects.finish(operation, job.0, Phase::Registrations);
            }
            Ok(Ok((_, error, _))) => effects.fail(operation, job.0, error.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn normalize_recovery(
    operations: Query<(
        Entity,
        &Destination,
        &StorageDirectory,
        &RequestedBranch,
        &Progress,
        Option<&Job>,
        Has<PrunedRegistration>,
        Option<&RecoveryRegistration>,
    )>,
    registrations: Query<(&Cwd, Option<&CheckedOutBranch>, &ChildOf), With<Registration>>,
    completed: Query<&JobResult<io::CanonicalParent>>,
    mut effects: Effects,
) {
    for (operation, destination, storage, expected, progress, job, pruned, authorized) in
        &operations
    {
        if progress.0 != Phase::NormalizeRecovery {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            if !destination.0.is_absolute() {
                effects.commands.entity(operation).insert((
                    Failure("managed worktree path must be absolute".into()),
                    Progress(Phase::Complete),
                ));
                continue;
            }
            let task = effects.request(operation, io::CanonicalParent(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        let path = match &result.0 {
            Ok(Ok(path)) => path,
            Ok(Err(error)) | Err(error) => {
                effects.fail(operation, job.0, error.clone());
                continue;
            }
        };
        let registration = registrations.iter().find(|(path_seen, branch, parent)| {
            parent.parent() == operation
                && (path_seen.0 == *path || branch.is_some_and(|branch| branch.0 == expected.0))
        });
        match registration {
            Some((seen, Some(branch), _))
                if seen.0 == *path && branch.0 == expected.0 && !pruned =>
            {
                effects.commands.entity(operation).insert((
                    Destination(path.clone()),
                    RecoveryRegistration(path.clone()),
                ));
                effects.finish(operation, job.0, Phase::PruneRegistration);
            }
            Some(_) => effects.fail(
                operation,
                job.0,
                "worktree registration does not allow recovery at this path".into(),
            ),
            None if path.parent() == Some(storage.0.as_path())
                || authorized.is_some_and(|authorized| authorized.0 == *path) =>
            {
                effects
                    .commands
                    .entity(operation)
                    .insert(Destination(path.clone()));
                effects.finish(operation, job.0, Phase::Claim);
            }
            None => effects.fail(
                operation,
                job.0,
                "managed worktree path escapes its repository storage directory".into(),
            ),
        }
    }
}

fn journal(
    operations: Query<(
        Entity,
        &vmux_ecs::Uuid,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Destination,
        &RequestedBranch,
        &JournalPath,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
        Has<Generated>,
        Has<BranchCreated>,
    )>,
    completed: Query<&JobResult<io::WriteJournal>>,
    mut effects: Effects,
) {
    for (
        operation,
        id,
        repository,
        guard,
        destination,
        branch,
        directory,
        checkout,
        progress,
        job,
        generated,
        created,
    ) in &operations
    {
        if progress.0 != Phase::Journal {
            continue;
        }
        let path = directory.0.join(format!("{}.json", id.0));
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let data = serde_json::json!({
                "owner": id.0, "repository": repository.0.root, "common_dir": repository.0.common_dir,
                "path": destination.0, "branch": branch.0, "delete_branch": generated || created,
                "observed_identity": serde_json::Value::Null,
            });
            let bytes = match serde_json::to_vec(&data) {
                Ok(bytes) => bytes,
                Err(error) => {
                    effects
                        .commands
                        .entity(operation)
                        .insert((Failure(error.to_string()), Progress(Phase::Complete)));
                    continue;
                }
            };
            let task = effects.request(
                operation,
                io::WriteJournal {
                    path: path.clone(),
                    staging: directory.0.join(format!("{}.staging", id.0)),
                    bytes,
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(())) => {
                effects
                    .commands
                    .entity(operation)
                    .insert((JournalPath(path), JournalWritten));
                effects
                    .commands
                    .entity(checkout.0)
                    .insert(lease::JournalPath(
                        directory.0.join(format!("{}.json", id.0)),
                    ));
                effects.finish(operation, job.0, Phase::Add);
            }
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn add(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Destination,
        &RequestedBranch,
        &CreationRevision,
        &Progress,
        Option<&Job>,
        Has<Generated>,
        Has<BranchCreated>,
    )>,
    mut effects: Effects,
) {
    for (
        operation,
        repository,
        guard,
        destination,
        branch,
        base,
        progress,
        job,
        generated,
        created,
    ) in &operations
    {
        if progress.0 != Phase::Add {
            continue;
        }
        if job.is_none() {
            if effects.stopped.contains(operation) {
                continue;
            }
            let mut arguments = vec![
                "worktree".into(),
                "add".into(),
                destination.0.to_string_lossy().into_owned(),
            ];
            if generated || created {
                arguments.extend(["-b".into(), branch.0.clone(), base.0.clone()]);
            } else {
                arguments.push(branch.0.clone());
            }
            let task = effects.request(
                operation,
                io::Create {
                    command: vmux_git::worktree::WorktreeCommand {
                        directory: repository.0.root.clone(),
                        arguments,
                        guard: Some(Arc::clone(&guard.0)),
                    },
                    destination: destination.0.clone(),
                },
            );
            effects
                .commands
                .entity(operation)
                .insert((Job(task), MutationAttempted));
        }
    }
}

fn created(
    operations: Query<(Entity, &Job, &LeaseCheckout, &Progress)>,
    completed: Query<&JobResult<io::Create>>,
    mut effects: Effects,
) {
    for (operation, job, checkout, progress) in &operations {
        if progress.0 != Phase::Add {
            continue;
        }
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        effects.commands.entity(operation).insert(IdentityObserved);
        let created = match &result.0 {
            Ok(created) => created,
            Err(error) => {
                effects.fail(operation, job.0, error.clone());
                continue;
            }
        };
        match &created.command {
            Ok((_, _, true)) => {
                effects.commands.entity(operation).insert(MutationMade);
            }
            Ok((_, error, _)) => {
                effects.fail(operation, job.0, error.trim().to_string());
                continue;
            }
            Err(error) => {
                effects.fail(operation, job.0, error.clone());
                continue;
            }
        }
        match &created.directory {
            Ok((guard, metadata)) if metadata.is_dir() => {
                effects.commands.entity(checkout.0).insert((
                    lease::Identity::from_metadata(metadata),
                    lease::DirectoryGuard {
                        _directory: Arc::clone(guard),
                    },
                ));
                effects.finish(operation, job.0, Phase::RecordBase);
            }
            Ok(_) => effects.fail(
                operation,
                job.0,
                "created checkout is not a directory".into(),
            ),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn record_base(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &RequestedBranch,
        &BaseRef,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, branch, base, progress, job) in &operations {
        if progress.0 != Phase::RecordBase {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "config".into(),
                        "--local".into(),
                        format!("branch.{}.vmux-base", branch.0),
                        base.0.clone(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((_, _, true))) => effects.finish(operation, job.0, Phase::Evidence),
            Ok(Ok((_, error, _))) => effects.fail(operation, job.0, error.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn inspect(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &Destination,
        &SourceDirectory,
        &RequestedBranch,
        &BaseRef,
        &Progress,
        Option<&Job>,
        Has<Restored>,
    )>,
    completed: Query<&JobResult<Identify>>,
    mut effects: Effects,
) {
    for (operation, repository, destination, _source, branch, base, progress, job, restored) in
        &operations
    {
        if progress.0 != Phase::Inspect {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Identify(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(checkout))
                if checkout.common_dir == repository.0.common_dir
                    && (checkout.root == destination.0
                        || restored && destination.0.starts_with(&checkout.root)) =>
            {
                effects.commands.entity(operation).insert((
                    Destination(checkout.root.clone()),
                    AddedCheckout(checkout.clone()),
                    ActivationMetadata(crate::tab::TabWorktree {
                        repo_root: repository.0.root.to_string_lossy().into_owned(),
                        checkout_dir: checkout.root.to_string_lossy().into_owned(),
                        branch: branch.0.clone(),
                        base_ref: base.0.clone(),
                    }),
                ));
                let next = if restored {
                    Phase::VerifyLink
                } else {
                    Phase::VerifyCheckoutBranch
                };
                effects.finish(operation, job.0, next);
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                job.0,
                "managed worktree belongs to another repository".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn execution(
    operations: Query<(
        Entity,
        &AddedCheckout,
        &RelativeDirectory,
        &SourceDirectory,
        &ActivationMetadata,
        &Progress,
        Option<&Job>,
        Option<&LeaseCheckout>,
    )>,
    completed: Query<&JobResult<Canonicalize>>,
    mut prepared: MessageWriter<CheckoutPrepared>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (operation, checkout, relative, source, metadata, progress, job, lease) in &operations {
        if progress.0 != Phase::Execution {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Canonicalize(checkout.0.root.join(&relative.0)));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        if effects.stopped.contains(operation)
            || lease.is_some_and(|checkout| !leases.owns(checkout.0, operation))
        {
            effects.finish(operation, job.0, Phase::Complete);
            continue;
        }
        match &result.0 {
            Ok(Ok(path)) if path.starts_with(&checkout.0.root) => {
                let ready = match crate::worktree::TabWorktreeReady::new(
                    path,
                    &source.0.to_string_lossy(),
                    &metadata.0,
                    &checkout.0,
                ) {
                    Ok(ready) => ready,
                    Err(error) => {
                        effects.fail(operation, job.0, error);
                        continue;
                    }
                };
                effects
                    .commands
                    .entity(operation)
                    .insert(PreparedCheckout {
                        activation: TabWorktreeActivation {
                            execution_dir: path.clone(),
                            metadata: metadata.0.clone(),
                            ready,
                        },
                    })
                    .remove::<RepositoryGuard>();
                if let Some(checkout) = lease {
                    effects
                        .commands
                        .entity(checkout.0)
                        .insert(lease::Lease(lease::Phase::Prepared));
                }
                effects.finish(operation, job.0, Phase::Prepared);
                prepared.write(CheckoutPrepared { operation });
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                job.0,
                "project directory escapes worktree".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn verify_link(
    operations: Query<(Entity, &Destination, &Progress, Option<&Job>)>,
    completed: Query<&JobResult<Inspect>>,
    mut effects: Effects,
) {
    for (operation, destination, progress, job) in &operations {
        if progress.0 != Phase::VerifyLink {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Inspect(destination.0.join(".git")));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(metadata))) if metadata.is_file() => {
                effects.finish(operation, job.0, Phase::VerifyCheckoutBranch)
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                job.0,
                "managed worktree directory is not a linked worktree".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn verify_branch(
    operations: Query<(
        Entity,
        &Destination,
        &RepositoryGuard,
        &RequestedBranch,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, destination, guard, expected, progress, job) in &operations {
        if progress.0 != Phase::VerifyCheckoutBranch {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: destination.0.clone(),
                    arguments: vec![
                        "symbolic-ref".into(),
                        "--quiet".into(),
                        "--short".into(),
                        "HEAD".into(),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((branch, _, true))) if branch.trim() == expected.0 => {
                effects.finish(operation, job.0, Phase::Execution)
            }
            Ok(Ok(_)) => effects.fail(
                operation,
                job.0,
                format!("worktree is no longer on expected branch {}", expected.0),
            ),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn reserve(
    operations: Query<(
        Entity,
        &Destination,
        &RequestedBranch,
        &SourceRepository,
        &Progress,
        Has<Existing>,
        Has<Restored>,
        Has<RestoreMissing>,
        Has<BranchSelected>,
    )>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (
        operation,
        destination,
        branch,
        repository,
        progress,
        existing,
        restored,
        missing,
        selected,
    ) in &operations
    {
        if progress.0 != Phase::Claim || effects.stopped.contains(operation) {
            continue;
        }
        let checkout = match leases.find(&destination.0) {
            Some((checkout, lease::Phase::Released, _)) => checkout,
            Some((checkout, lease::Phase::Committed | lease::Phase::Retained, _))
                if !existing
                    && (restored && missing || selected)
                    && leases
                        .checkouts
                        .get(checkout)
                        .is_ok_and(|(_, _, seen, _, _)| seen.0 == branch.0) =>
            {
                checkout
            }
            None if !existing => effects
                .commands
                .spawn((
                    Name::new("Managed checkout"),
                    vmux_ecs::Active,
                    lease::Checkout,
                    ChildOf(repository.0),
                    Cwd(destination.0.clone()),
                    CheckedOutBranch(branch.0.clone()),
                    lease::Lease(lease::Phase::Reserved),
                ))
                .id(),
            _ => continue,
        };
        effects
            .commands
            .entity(checkout)
            .insert((
                OperationTarget::new(operation),
                lease::Lease(lease::Phase::Reserved),
                CheckedOutBranch(branch.0.clone()),
            ))
            .remove::<(
                lease::Identity,
                lease::InitialRevision,
                lease::DeleteBranch,
                lease::JournalPath,
            )>();
        if restored {
            effects.commands.entity(checkout).insert(lease::Durable);
        } else {
            effects.commands.entity(checkout).remove::<lease::Durable>();
        }
        effects
            .commands
            .entity(operation)
            .insert((LeaseCheckout(checkout), Progress(Phase::ResolveBase)));
    }
}

fn claim(
    operations: Query<(
        Entity,
        &Destination,
        &Progress,
        Has<Existing>,
        Has<Restored>,
    )>,
    leases: lease::Leases,
    journals: Query<&lease::JournalPath, With<lease::Checkout>>,
    mut effects: Effects,
) {
    for (operation, destination, progress, existing, restored) in &operations {
        if progress.0 != Phase::Claim {
            continue;
        }
        if effects.stopped.contains(operation) {
            continue;
        }
        match leases.find(&destination.0) {
            Some((checkout, lease::Phase::Prepared, _)) if existing => {
                if restored {
                    effects.commands.entity(checkout).insert(lease::Durable);
                }
                effects
                    .commands
                    .entity(checkout)
                    .insert((
                        OperationTarget::new(operation),
                        lease::Lease(lease::Phase::Reserved),
                    ))
                    .remove::<lease::DeleteBranch>();
                effects.commands.entity(operation).insert((
                    LeaseCheckout(checkout),
                    MutationAttempted,
                    MutationMade,
                    IdentityObserved,
                    Progress(Phase::Adopt),
                ));
                if let Ok(journal) = journals.get(checkout) {
                    effects
                        .commands
                        .entity(operation)
                        .insert((JournalPath(journal.0.clone()), JournalWritten));
                }
            }
            Some((checkout, lease::Phase::Prepared, _)) => {
                effects
                    .commands
                    .entity(operation)
                    .insert((LeaseCheckout(checkout), Progress(Phase::Waiting)))
                    .remove::<RepositoryGuard>();
            }
            Some((checkout, lease::Phase::Reserved | lease::Phase::Cleaning, owner))
                if owner != Some(operation) =>
            {
                effects
                    .commands
                    .entity(operation)
                    .insert((LeaseCheckout(checkout), Progress(Phase::Waiting)))
                    .remove::<RepositoryGuard>();
            }
            Some((_, lease::Phase::Committed | lease::Phase::Retained, _)) if existing => {
                effects
                    .commands
                    .entity(operation)
                    .insert(Progress(Phase::Inspect));
            }
            Some((checkout, _, owner)) if owner == Some(operation) => {
                effects
                    .commands
                    .entity(operation)
                    .insert((LeaseCheckout(checkout), Progress(Phase::ResolveBase)));
            }
            None if existing => {
                effects
                    .commands
                    .entity(operation)
                    .insert(Progress(Phase::Inspect));
            }
            _ => {
                effects.commands.entity(operation).insert((
                    Failure("managed checkout ownership is unavailable".into()),
                    Progress(Phase::Complete),
                ));
            }
        }
    }
}

fn resume_waiters(
    operations: Query<(Entity, &Progress, &LeaseCheckout)>,
    states: Query<&lease::Lease>,
    observations: Query<(Entity, &ChildOf), Or<(With<Registration>, With<LocalBranch>)>>,
    mut commands: Commands,
) {
    for (operation, progress, checkout) in &operations {
        if progress.0 != Phase::Waiting {
            continue;
        }
        if states
            .get(checkout.0)
            .is_ok_and(|state| matches!(state.0, lease::Phase::Reserved | lease::Phase::Cleaning))
        {
            continue;
        }
        for (observation, parent) in &observations {
            if parent.parent() == operation {
                commands.entity(observation).try_despawn();
            }
        }
        commands
            .entity(operation)
            .remove::<(LeaseCheckout, RepositoryGuard, Existing)>()
            .insert(Progress(Phase::Identity));
    }
}

fn adopt(
    operations: Query<(
        Entity,
        &LeaseCheckout,
        &Destination,
        &Progress,
        Option<&Job>,
    )>,
    identities: Query<&lease::Identity>,
    completed: Query<&JobResult<Inspect>>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (operation, checkout, destination, progress, job) in &operations {
        if progress.0 != Phase::Adopt {
            continue;
        }
        if !leases.owns(checkout.0, operation) {
            effects.commands.entity(operation).insert((
                AbortRequested,
                CleanupFinished,
                Progress(Phase::Complete),
            ));
            continue;
        }
        let Ok(identity) = identities.get(checkout.0) else {
            effects.retain(
                operation,
                job.map(|job| job.0),
                "adopted checkout has no verified identity".into(),
            );
            continue;
        };
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let task = effects.request(operation, Inspect(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(metadata))) if identity.matches(metadata) => {
                effects.finish(operation, job.0, Phase::Evidence)
            }
            Ok(Ok(_)) => effects.retain(
                operation,
                Some(job.0),
                "checkout identity changed before adoption".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn resolve_base(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &BaseRef,
        &RequestedBranch,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
        Has<Generated>,
        Has<BranchCreated>,
    )>,
    completed: Query<&JobResult<vmux_git::worktree::WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, base, branch, checkout, progress, job, generated, created) in
        &operations
    {
        if progress.0 != Phase::ResolveBase {
            continue;
        }
        let Some(job) = job else {
            if effects.stopped.contains(operation) {
                continue;
            }
            let reference = if generated || created {
                &base.0
            } else {
                &branch.0
            };
            let task = effects.request(
                operation,
                vmux_git::worktree::WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "rev-parse".into(),
                        "--verify".into(),
                        format!("{reference}^{{commit}}"),
                    ],
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok((revision, _, true))) => {
                let revision = revision.trim().to_string();
                effects
                    .commands
                    .entity(checkout.0)
                    .insert(lease::InitialRevision(revision.clone()));
                if generated || created {
                    effects
                        .commands
                        .entity(checkout.0)
                        .insert(lease::DeleteBranch);
                }
                effects
                    .commands
                    .entity(operation)
                    .insert(CreationRevision(revision));
                effects.finish(operation, job.0, Phase::JournalDirectory);
            }
            Ok(Ok((_, error, _))) => effects.fail(operation, job.0, error.trim().to_string()),
            Ok(Err(error)) => effects.fail(operation, job.0, error.0.clone()),
            Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn observe_identity(
    operations: Query<(
        Entity,
        &Destination,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<Inspect>>,
    mut effects: Effects,
) {
    for (operation, destination, checkout, progress, job) in &operations {
        if progress.0 != Phase::ObserveIdentity {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, Inspect(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(metadata))) if metadata.is_dir() => {
                effects
                    .commands
                    .entity(checkout.0)
                    .insert((lease::Identity::from_metadata(metadata),));
                effects.commands.entity(operation).insert(IdentityObserved);
                effects.finish(operation, job.0, Phase::Evidence);
            }
            Ok(Ok(_)) => {
                effects.commands.entity(operation).insert(IdentityObserved);
                effects.fail(
                    operation,
                    job.0,
                    "created checkout identity is unavailable".into(),
                );
            }
            Ok(Err(error)) | Err(error) => {
                effects.commands.entity(operation).insert(IdentityObserved);
                effects.fail(operation, job.0, error.clone());
            }
        }
    }
}

fn evidence(
    operations: Query<(
        Entity,
        &vmux_ecs::Uuid,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Destination,
        &RequestedBranch,
        &JournalPath,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
    )>,
    identities: Query<(
        &lease::Identity,
        &lease::InitialRevision,
        Has<lease::DeleteBranch>,
    )>,
    completed: Query<&JobResult<io::WriteJournal>>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (operation, id, repository, guard, destination, branch, journal, checkout, progress, job) in
        &operations
    {
        if progress.0 != Phase::Evidence {
            continue;
        }
        if !leases.owns(checkout.0, operation) {
            effects.commands.entity(operation).insert((
                AbortRequested,
                CleanupFinished,
                Progress(Phase::Complete),
            ));
            continue;
        }
        let Ok((identity, revision, delete)) = identities.get(checkout.0) else {
            effects.retain(
                operation,
                job.map(|job| job.0),
                "checkout evidence is unavailable".into(),
            );
            continue;
        };
        let Some(job) = job else {
            let data = serde_json::json!({
                "owner": id.0, "repository": repository.0.root, "common_dir": repository.0.common_dir,
                "path": destination.0, "branch": branch.0, "delete_branch": delete,
                "observed_identity": identity, "creation_revision": revision.0,
            });
            let bytes = match serde_json::to_vec(&data) {
                Ok(bytes) => bytes,
                Err(error) => {
                    effects.retain(operation, None, error.to_string());
                    continue;
                }
            };
            let task = effects.request(
                operation,
                io::WriteJournal {
                    path: journal.0.clone(),
                    staging: journal.0.with_extension("staging"),
                    bytes,
                    guard: Some(Arc::clone(&guard.0)),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(())) => effects.finish(operation, job.0, Phase::Inspect),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn cleanup_journal(
    operations: Query<(Entity, &JournalPath, &Progress, Option<&Job>)>,
    completed: Query<&JobResult<io::RemoveJournal>>,
    mut effects: Effects,
) {
    for (operation, path, progress, job) in &operations {
        if progress.0 != Phase::CleanupJournal {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, io::RemoveJournal(path.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(())) => effects.finish(operation, job.0, Phase::Complete),
            Ok(Err(error)) | Err(error) => effects.fail(operation, job.0, error.clone()),
        }
    }
}

fn finish_operations(
    operations: Query<(
        Entity,
        &Progress,
        Option<&Failure>,
        Has<AbortRequested>,
        Has<Committed>,
        Has<MutationMade>,
        Has<MutationAttempted>,
        Has<CleanupFinished>,
        Has<IdentityObserved>,
        Has<JournalWritten>,
        Option<&LeaseCheckout>,
        Option<&RetainedReason>,
    )>,
    mut failures: MessageWriter<CheckoutFailed>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (
        operation,
        progress,
        error,
        abort,
        committed,
        made,
        attempted,
        cleaned,
        observed,
        journal,
        checkout,
        retained,
    ) in &operations
    {
        if progress.0 != Phase::Complete {
            continue;
        }
        if (made || attempted) && !committed && !cleaned {
            let next = if !observed {
                Phase::ObserveIdentity
            } else {
                Phase::CleanupLock
            };
            effects.commands.entity(operation).insert(Progress(next));
            continue;
        }
        if !attempted && !committed && !cleaned {
            if let Some(checkout) = checkout
                && leases.owns(checkout.0, operation)
            {
                effects
                    .commands
                    .entity(checkout.0)
                    .insert(lease::Lease(lease::Phase::Released))
                    .remove::<(OperationTarget, lease::DirectoryGuard)>();
            }
            if journal {
                effects
                    .commands
                    .entity(operation)
                    .insert((CleanupFinished, Progress(Phase::CleanupJournal)));
                continue;
            }
        }
        let state = if abort {
            OperationState::Canceled
        } else if error.is_some() || retained.is_some() {
            OperationState::Failed
        } else {
            OperationState::Succeeded
        };
        if let Some(error) = error {
            failures.write(CheckoutFailed {
                operation,
                error: error.0.clone(),
            });
        } else if let Some(reason) = retained {
            failures.write(CheckoutFailed {
                operation,
                error: reason.0.clone(),
            });
        }
        effects.commands.entity(operation).insert(state).remove::<(
            Progress,
            RepositoryGuard,
            Job,
            RetryDeadline,
        )>();
    }
}

fn retire_released(
    checkouts: Query<(Entity, &lease::Lease), With<lease::Checkout>>,
    mut commands: Commands,
) {
    for (entity, lease) in &checkouts {
        if lease.0 == lease::Phase::Released {
            commands.entity(entity).try_despawn();
        }
    }
}

fn retire_repositories(
    repositories: Query<(Entity, Option<&Children>), With<lease::Repository>>,
    operations: Query<(&SourceRepository, &OperationState)>,
    mut commands: Commands,
) {
    for (entity, children) in &repositories {
        if children.is_some_and(|children| !children.is_empty()) {
            continue;
        }
        if operations.iter().any(|(repository, state)| {
            repository.0 == entity
                && matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        commands.entity(entity).try_despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::process::Command;
    use std::time::{Duration, Instant};

    struct CheckoutFixture {
        app: App,
        repository: tempfile::TempDir,
        storage: tempfile::TempDir,
        operation: Entity,
    }

    impl CheckoutFixture {
        fn new() -> Self {
            let repository = tempfile::tempdir().unwrap();
            let storage = tempfile::tempdir().unwrap();
            std::fs::write(repository.path().join("seed.txt"), "seed\n").unwrap();
            for args in [
                vec!["init", "--quiet", "-b", "main"],
                vec!["add", "seed.txt"],
                vec![
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "--allow-empty",
                    "--no-gpg-sign",
                    "--quiet",
                    "--message",
                    "seed",
                ],
            ] {
                let status = Command::new("git")
                    .current_dir(repository.path())
                    .args(args)
                    .env_remove("GIT_DIR")
                    .env_remove("GIT_WORK_TREE")
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            let hook = repository.path().join(".git/hooks/post-checkout");
            std::fs::write(
                &hook,
                "#!/bin/sh\n\
                 common=$(git rev-parse --path-format=absolute --git-common-dir) || exit 1\n\
                 touch \"$common/vmux-checkout-started\" || exit 1\n\
                 attempts=1000\n\
                 while [ ! -f \"$common/vmux-checkout-release\" ]; do\n\
                 [ \"$attempts\" -gt 0 ] || exit 1\n\
                 attempts=$((attempts - 1))\n\
                 sleep 0.01\n\
                 done\n",
            )
            .unwrap();
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CheckoutPlugin));
            let target = app.world_mut().spawn_empty().id();
            let operation = app
                .world_mut()
                .spawn((
                    Name::new("Checkout request"),
                    vmux_ecs::Active,
                    Operation::bundle("checkout-test").unwrap(),
                    OperationTarget::new(target),
                    OperationTargetRevision(1),
                ))
                .id();
            app.world_mut().write_message(CreateCheckout {
                operation,
                directory: repository.path().to_path_buf(),
                managed_root: storage.path().to_path_buf(),
                slug_hint: "work".into(),
            });
            Self {
                app,
                repository,
                storage,
                operation,
            }
        }

        fn wait_for_creation(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if self
                    .repository
                    .path()
                    .join(".git/vmux-checkout-started")
                    .exists()
                {
                    return;
                }
                if let Some(failure) = self.app.world().get::<Failure>(self.operation) {
                    panic!("checkout failed: {}", failure.0);
                }
                assert!(Instant::now() < deadline, "Git checkout hook did not start");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn finish_creation(&mut self) -> TabWorktreeActivation {
            self.release_creation();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if let Some(prepared) = self.app.world().get::<PreparedCheckout>(self.operation) {
                    return prepared.activation.clone();
                }
                if let Some(failure) = self.app.world().get::<Failure>(self.operation) {
                    panic!("checkout failed: {}", failure.0);
                }
                assert!(Instant::now() < deadline, "checkout was not prepared");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn release_creation(&self) {
            std::fs::write(
                self.repository.path().join(".git/vmux-checkout-release"),
                [],
            )
            .unwrap();
        }

        fn git(&self, directory: &std::path::Path, arguments: &[&str]) -> std::process::Output {
            Command::new("git")
                .current_dir(directory)
                .args(arguments)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .output()
                .unwrap()
        }

        fn checkout_path(&self) -> PathBuf {
            let output = self.git(
                self.repository.path(),
                &["worktree", "list", "--porcelain", "-z"],
            );
            assert!(output.status.success());
            let root = self.repository.path().canonicalize().unwrap();
            for field in String::from_utf8(output.stdout).unwrap().split('\0') {
                if let Some(path) = field.strip_prefix("worktree ") {
                    let path = PathBuf::from(path);
                    if path != root {
                        return path;
                    }
                }
            }
            panic!("Git did not register the created worktree");
        }

        fn cancel(&mut self) {
            self.app.world_mut().write_message(CancelCheckout {
                operation: self.operation,
            });
        }

        fn complete(&mut self) -> OperationState {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let state = self
                    .app
                    .world()
                    .get::<OperationState>(self.operation)
                    .unwrap();
                if matches!(
                    state,
                    OperationState::Succeeded | OperationState::Failed | OperationState::Canceled
                ) {
                    return state.clone();
                }
                assert!(
                    Instant::now() < deadline,
                    "checkout did not finish: {state:?}"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn wait_for_custody(&mut self) -> PathBuf {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                if self
                    .app
                    .world()
                    .get::<custody::Moved>(self.operation)
                    .is_some()
                {
                    return self
                        .app
                        .world()
                        .get::<custody::Custody>(self.operation)
                        .unwrap()
                        .path
                        .clone();
                }
                assert!(
                    Instant::now() < deadline,
                    "checkout custody move did not complete"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn wait_for_phase(&mut self, expected: Phase) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let progress = self.app.world().get::<Progress>(self.operation).unwrap();
                if progress.0 == expected {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "checkout did not reach the expected cleanup phase"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn select(&mut self, branch: &str) -> Entity {
            let target = self
                .app
                .world()
                .get::<OperationTarget>(self.operation)
                .unwrap()
                .entity();
            self.app
                .world_mut()
                .entity_mut(target)
                .insert(CheckoutTargetRevision(2));
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Name::new("Checkout selection"),
                    vmux_ecs::Active,
                    Operation::bundle("checkout-select-test").unwrap(),
                    OperationTarget::new(target),
                    OperationTargetRevision(2),
                ))
                .id();
            self.app.world_mut().write_message(SelectBranchCheckout {
                operation,
                directory: self.repository.path().to_path_buf(),
                managed_root: self.storage.path().to_path_buf(),
                branch: branch.into(),
            });
            self.operation = operation;
            operation
        }

        fn commit(&mut self, activation: &TabWorktreeActivation) {
            let target = self
                .app
                .world()
                .get::<OperationTarget>(self.operation)
                .unwrap()
                .entity();
            let revision = self
                .app
                .world()
                .get::<OperationTargetRevision>(self.operation)
                .unwrap()
                .0;
            self.app.world_mut().entity_mut(target).insert((
                CheckoutTargetRevision(revision),
                activation.metadata.clone(),
            ));
            self.app.world_mut().write_message(CommitCheckout {
                operation: self.operation,
                target,
                revision,
            });
            assert_eq!(self.complete(), OperationState::Succeeded);
        }

        fn restore(&mut self, metadata: &crate::tab::TabWorktree) {
            let target = self
                .app
                .world()
                .get::<OperationTarget>(self.operation)
                .unwrap()
                .entity();
            self.app
                .world_mut()
                .entity_mut(target)
                .insert((CheckoutTargetRevision(3), metadata.clone()));
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Name::new("Persisted checkout restore"),
                    vmux_ecs::Active,
                    Operation::bundle("checkout-restore-test").unwrap(),
                    OperationTarget::new(target),
                    OperationTargetRevision(3),
                ))
                .id();
            self.app.world_mut().write_message(RestoreCheckout {
                operation,
                directory: self.repository.path().to_path_buf(),
                managed_root: self.storage.path().to_path_buf(),
                checkout: metadata.checkout_dir.clone().into(),
                branch: metadata.branch.clone(),
                base_ref: metadata.base_ref.clone(),
            });
            self.operation = operation;
        }

        fn restart_and_recover(&mut self) {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, CheckoutPlugin));
            self.app = app;
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Name::new("Checkout journal recovery"),
                    Operation::bundle("checkout-recovery-test").unwrap(),
                ))
                .id();
            self.app.world_mut().write_message(RecoverCheckouts {
                operation,
                managed_root: self.storage.path().to_path_buf(),
            });
            self.operation = operation;
            assert_eq!(self.complete(), OperationState::Succeeded);
        }
    }

    impl Drop for CheckoutFixture {
        fn drop(&mut self) {
            let _ = std::fs::write(
                self.repository.path().join(".git/vmux-checkout-release"),
                [],
            );
        }
    }

    #[test]
    fn repository_mutation_lock_is_held_until_worktree_creation_finishes() {
        let mut fixture = CheckoutFixture::new();
        fixture.wait_for_creation();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(fixture.repository.path().join(".git/vmux-worktrees.lock"))
            .unwrap();
        assert_ne!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let activation = fixture.finish_creation();
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        assert!(
            activation
                .execution_dir
                .starts_with(fixture.storage.path().canonicalize().unwrap())
        );
        assert!(activation.execution_dir.join(".git").is_file());
        let branch = Command::new("git")
            .current_dir(&activation.execution_dir)
            .args(["branch", "--show-current"])
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(branch.status.success());
        assert_eq!(
            String::from_utf8(branch.stdout).unwrap().trim(),
            "vmux/work"
        );
    }

    #[test]
    fn checkout_lock_retry_progresses_while_the_long_io_lane_is_busy() {
        let mut fixture = CheckoutFixture::new();
        fixture.release_creation();
        let mut tasks = Vec::new();
        let mut releases = Vec::new();
        let (entered, started) = std::sync::mpsc::channel();
        for _ in 0..2 {
            let (release, waiting) = std::sync::mpsc::channel();
            let entered = entered.clone();
            let task = fixture
                .app
                .world()
                .resource::<vmux_ecs::blocking::BlockingRuntime>()
                .try_spawn(vmux_ecs::blocking::Lane::Long, None, move || {
                    entered.send(()).unwrap();
                    let _ = waiting.recv_timeout(Duration::from_secs(10));
                })
                .unwrap();
            tasks.push(task);
            releases.push(release);
        }
        for _ in 0..2 {
            started.recv_timeout(Duration::from_secs(1)).unwrap();
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(fixture.repository.path().join(".git/vmux-worktrees.lock"))
            .unwrap();
        assert_eq!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while fixture
            .app
            .world()
            .get::<RetryDeadline>(fixture.operation)
            .is_none()
        {
            fixture.app.update();
            assert!(
                Instant::now() < deadline,
                "checkout did not wait for the repository lock"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(lock);
        let activation = fixture.finish_creation();
        assert!(activation.execution_dir.join("seed.txt").is_file());
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        for release in releases {
            release.send(()).unwrap();
        }
        drop(tasks);
    }

    #[test]
    fn completed_cancellation_retires_checkout_and_unused_repository_entities() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(
            !activation.execution_dir.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
        fixture.app.update();
        let world = fixture.app.world_mut();
        let mut checkouts = world.query_filtered::<Entity, With<lease::Checkout>>();
        assert_eq!(checkouts.iter(world).count(), 0);
        let mut repositories = world.query_filtered::<Entity, With<lease::Repository>>();
        assert_eq!(repositories.iter(world).count(), 0);
    }

    #[test]
    fn canceled_create_does_not_start_repository_mutation() {
        let mut fixture = CheckoutFixture::new();
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        let branches = fixture.git(
            fixture.repository.path(),
            &["branch", "--format=%(refname:short)"],
        );
        assert!(branches.status.success());
        assert_eq!(String::from_utf8(branches.stdout).unwrap().trim(), "main");
        assert_eq!(
            std::fs::read_dir(fixture.storage.path()).unwrap().count(),
            0
        );
    }

    #[test]
    fn cancel_during_git_add_drains_before_cleaning_the_created_checkout() {
        let mut fixture = CheckoutFixture::new();
        fixture.wait_for_creation();
        let checkout = fixture.checkout_path();
        fixture.cancel();
        fixture.app.update();
        assert!(checkout.exists());
        assert_ne!(
            fixture.app.world().get::<OperationState>(fixture.operation),
            Some(&OperationState::Canceled)
        );
        fixture.release_creation();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(
            !checkout.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
        let branch = fixture.git(
            fixture.repository.path(),
            &["rev-parse", "--verify", "refs/heads/vmux/work"],
        );
        assert!(!branch.status.success());
        assert_eq!(
            std::fs::read_dir(checkout.parent().unwrap().join(".leases"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn cancel_removes_only_a_clean_uncommitted_checkout() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(
            !activation.execution_dir.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
        let branches = fixture.git(
            fixture.repository.path(),
            &["branch", "--format=%(refname:short)"],
        );
        assert!(branches.status.success());
        assert_eq!(String::from_utf8(branches.stdout).unwrap().trim(), "main");
    }

    #[test]
    fn canceled_checkout_retains_tracked_untracked_and_ignored_files() {
        for name in ["seed.txt", "untracked.txt", "ignored.txt"] {
            let mut fixture = CheckoutFixture::new();
            std::fs::write(
                fixture.repository.path().join(".git/info/exclude"),
                "ignored.txt\n",
            )
            .unwrap();
            let activation = fixture.finish_creation();
            std::fs::write(activation.execution_dir.join(name), "user work\n").unwrap();
            fixture.cancel();
            assert_eq!(fixture.complete(), OperationState::Canceled);
            assert_eq!(
                std::fs::read_to_string(activation.execution_dir.join(name)).unwrap(),
                "user work\n"
            );
            let branch = fixture.git(&activation.execution_dir, &["branch", "--show-current"]);
            assert!(branch.status.success());
            assert_eq!(
                String::from_utf8(branch.stdout).unwrap().trim(),
                "vmux/work"
            );
            assert_eq!(
                std::fs::read_dir(activation.execution_dir.parent().unwrap().join(".leases"))
                    .unwrap()
                    .count(),
                1
            );
        }
    }

    #[test]
    fn canceled_checkout_retains_commits_created_after_preparation() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        std::fs::write(activation.execution_dir.join("seed.txt"), "user commit\n").unwrap();
        assert!(
            fixture
                .git(&activation.execution_dir, &["add", "seed.txt"])
                .status
                .success()
        );
        let commit = fixture.git(
            &activation.execution_dir,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--no-gpg-sign",
                "--quiet",
                "--message",
                "user work",
            ],
        );
        assert!(commit.status.success());
        let before = fixture.git(&activation.execution_dir, &["rev-parse", "HEAD"]);
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        let after = fixture.git(&activation.execution_dir, &["rev-parse", "HEAD"]);
        assert!(after.status.success());
        assert_eq!(before.stdout, after.stdout);
        assert_eq!(
            std::fs::read_to_string(activation.execution_dir.join("seed.txt")).unwrap(),
            "user commit\n"
        );
    }

    #[test]
    fn canceled_checkout_does_not_delete_a_replacement_directory() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let retained = activation.execution_dir.with_extension("original");
        std::fs::rename(&activation.execution_dir, &retained).unwrap();
        std::fs::create_dir(&activation.execution_dir).unwrap();
        std::fs::write(activation.execution_dir.join("user.txt"), "replacement\n").unwrap();
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(retained.join("seed.txt").is_file());
        assert_eq!(
            std::fs::read_to_string(activation.execution_dir.join("user.txt")).unwrap(),
            "replacement\n"
        );
    }

    #[test]
    fn removing_the_request_target_cancels_and_cleans_its_checkout() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let target = fixture
            .app
            .world()
            .get::<OperationTarget>(fixture.operation)
            .unwrap()
            .entity();
        fixture.app.world_mut().entity_mut(target).despawn();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(
            !activation.execution_dir.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
    }

    #[test]
    fn failure_after_git_creates_the_checkout_drains_and_retains_unverified_ownership() {
        let mut fixture = CheckoutFixture::new();
        let hook = fixture.repository.path().join(".git/hooks/post-checkout");
        let script = std::fs::read_to_string(&hook).unwrap();
        std::fs::write(&hook, format!("{script}exit 1\n")).unwrap();
        fixture.wait_for_creation();
        let checkout = fixture.checkout_path();
        fixture.release_creation();
        assert_eq!(fixture.complete(), OperationState::Failed);
        assert!(checkout.exists());
        let branch = fixture.git(
            fixture.repository.path(),
            &["rev-parse", "--verify", "refs/heads/vmux/work"],
        );
        assert!(branch.status.success());
    }

    #[test]
    fn failed_add_retains_an_externally_created_checkout() {
        let mut fixture = CheckoutFixture::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world()
                .get::<Progress>(fixture.operation)
                .is_some_and(|progress| progress.0 == Phase::JournalDirectory)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "checkout reservation did not complete"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let path = fixture
            .app
            .world()
            .get::<Destination>(fixture.operation)
            .unwrap()
            .0
            .clone();
        let branch = fixture
            .app
            .world()
            .get::<RequestedBranch>(fixture.operation)
            .unwrap()
            .0
            .clone();
        let revision = fixture
            .app
            .world()
            .get::<CreationRevision>(fixture.operation)
            .unwrap()
            .0
            .clone();
        fixture.release_creation();
        let external = fixture.git(
            fixture.repository.path(),
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                path.to_str().unwrap(),
                &revision,
            ],
        );
        assert!(
            external.status.success(),
            "{}",
            String::from_utf8_lossy(&external.stderr)
        );
        assert_eq!(fixture.complete(), OperationState::Failed);
        assert!(path.join(".git").is_file());
        let reference = format!("refs/heads/{branch}");
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["rev-parse", "--verify", &reference]
                )
                .status
                .success()
        );
    }

    #[test]
    fn a_directory_handle_writer_preserves_ignored_bytes_after_the_clean_check() {
        let mut fixture = CheckoutFixture::new();
        std::fs::write(
            fixture.repository.path().join(".gitignore"),
            "*.generated\n",
        )
        .unwrap();
        assert!(
            fixture
                .git(fixture.repository.path(), &["add", ".gitignore"])
                .status
                .success()
        );
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &[
                        "-c",
                        "user.name=Test",
                        "-c",
                        "user.email=test@example.com",
                        "commit",
                        "--no-gpg-sign",
                        "-m",
                        "ignore generated files"
                    ]
                )
                .status
                .success()
        );
        let activation = fixture.finish_creation();
        let directory = std::fs::File::open(&activation.execution_dir).unwrap();
        fixture.cancel();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world()
                .get::<Progress>(fixture.operation)
                .is_some_and(|progress| progress.0 == Phase::CleanupReferences)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cleanup did not finish its clean check"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                c"late.generated".as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
        };
        assert!(descriptor >= 0, "{}", std::io::Error::last_os_error());
        let mut file = unsafe { std::fs::File::from_raw_fd(descriptor) };
        file.write_all(b"late ignored bytes\n").unwrap();
        file.sync_all().unwrap();
        drop(file);
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert_eq!(
            std::fs::read(activation.execution_dir.join("late.generated")).unwrap(),
            b"late ignored bytes\n"
        );
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["rev-parse", "--verify", "refs/heads/vmux/work"]
                )
                .status
                .success()
        );
        drop(directory);
    }

    #[test]
    fn a_file_handle_writer_is_preserved_and_returned_from_custody() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(activation.execution_dir.join("seed.txt"))
            .unwrap();
        fixture.cancel();
        let path = fixture.wait_for_custody();
        assert!(
            !activation.execution_dir.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
        assert!(path.join("seed.txt").is_file());
        file.seek(SeekFrom::Start(0)).unwrap();
        file.set_len(0).unwrap();
        file.write_all(b"writer bytes after custody\n").unwrap();
        file.sync_all().unwrap();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert_eq!(
            std::fs::read(activation.execution_dir.join("seed.txt")).unwrap(),
            b"writer bytes after custody\n"
        );
        assert!(!path.exists());
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["rev-parse", "--verify", "refs/heads/vmux/work"]
                )
                .status
                .success()
        );
    }

    #[test]
    fn custody_return_never_overwrites_a_replacement_at_the_original_path() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let directory = std::fs::File::open(&activation.execution_dir).unwrap();
        fixture.cancel();
        let path = fixture.wait_for_custody();
        std::fs::create_dir(&activation.execution_dir).unwrap();
        std::fs::write(
            activation.execution_dir.join("replacement.txt"),
            "replacement bytes\n",
        )
        .unwrap();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert_eq!(
            std::fs::read(activation.execution_dir.join("replacement.txt")).unwrap(),
            b"replacement bytes\n"
        );
        assert_eq!(std::fs::read(path.join("seed.txt")).unwrap(), b"seed\n");
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["rev-parse", "--verify", "refs/heads/vmux/work"]
                )
                .status
                .success()
        );
        drop(directory);
    }

    #[test]
    fn recovery_finds_custody_after_a_move_without_an_acknowledgement() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.cancel();
        let path = fixture.wait_for_custody();
        std::fs::write(path.join("recovered.txt"), "custody bytes\n").unwrap();
        fixture.restart_and_recover();
        assert!(
            !activation.execution_dir.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
        assert_eq!(
            std::fs::read(path.join("recovered.txt")).unwrap(),
            b"custody bytes\n"
        );
        let world = fixture.app.world_mut();
        let mut recovered = world.query_filtered::<&Cwd, With<lease::Checkout>>();
        assert_eq!(
            recovered
                .iter(world)
                .map(|cwd| cwd.0.clone())
                .collect::<Vec<_>>(),
            vec![path]
        );
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["rev-parse", "--verify", "refs/heads/vmux/work"]
                )
                .status
                .success()
        );
    }

    #[test]
    fn nested_custody_return_preserves_foreign_files_and_recovers_the_actual_checkout() {
        for crash_before_record in [false, true] {
            let mut fixture = CheckoutFixture::new();
            let activation = fixture.finish_creation();
            let identity = std::fs::metadata(&activation.execution_dir).unwrap();
            let directory = std::fs::File::open(&activation.execution_dir).unwrap();
            fixture.cancel();
            let custody = fixture.wait_for_custody();
            fixture.wait_for_phase(Phase::CleanupCustodyReturnPlan);
            std::fs::create_dir(&activation.execution_dir).unwrap();
            std::fs::write(
                activation.execution_dir.join("foreign.txt"),
                b"foreign bytes\n",
            )
            .unwrap();
            let foreign = std::fs::metadata(&activation.execution_dir).unwrap();
            let actual = activation.execution_dir.join(custody.file_name().unwrap());
            if crash_before_record {
                fixture.wait_for_phase(Phase::CleanupCustodyInspect);
            } else {
                assert_eq!(fixture.complete(), OperationState::Canceled);
                let checkout = fixture
                    .app
                    .world()
                    .get::<LeaseCheckout>(fixture.operation)
                    .unwrap();
                assert_eq!(
                    fixture.app.world().get::<Cwd>(checkout.0).unwrap().0,
                    actual
                );
                let journal = fixture
                    .app
                    .world()
                    .get::<JournalPath>(fixture.operation)
                    .unwrap();
                let bytes = std::fs::read(&journal.0).unwrap();
                let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(record["path"], actual.to_string_lossy().as_ref());
            }
            assert_eq!(
                std::fs::read(activation.execution_dir.join("foreign.txt")).unwrap(),
                b"foreign bytes\n"
            );
            let preserved = std::fs::metadata(&activation.execution_dir).unwrap();
            assert_eq!(
                (preserved.dev(), preserved.ino()),
                (foreign.dev(), foreign.ino())
            );
            let moved = std::fs::metadata(&actual).unwrap();
            assert_eq!((moved.dev(), moved.ino()), (identity.dev(), identity.ino()));
            std::fs::write(actual.join("retained.txt"), b"retained checkout bytes\n").unwrap();
            assert_eq!(fixture.checkout_path(), actual);
            fixture.restart_and_recover();
            assert_eq!(std::fs::read(actual.join("seed.txt")).unwrap(), b"seed\n");
            assert_eq!(
                std::fs::read(actual.join("retained.txt")).unwrap(),
                b"retained checkout bytes\n"
            );
            let world = fixture.app.world_mut();
            let mut recovered = world.query_filtered::<&Cwd, With<lease::Checkout>>();
            assert_eq!(
                recovered
                    .iter(world)
                    .map(|cwd| cwd.0.clone())
                    .collect::<Vec<_>>(),
                vec![actual]
            );
            assert!(
                fixture
                    .git(
                        fixture.repository.path(),
                        &["rev-parse", "--verify", "refs/heads/vmux/work"]
                    )
                    .status
                    .success()
            );
            drop(directory);
        }
    }

    #[test]
    fn nested_forward_custody_is_recoverable_before_its_observed_journal_is_written() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let identity = std::fs::metadata(&activation.execution_dir).unwrap();
        let directory = std::fs::File::open(&activation.execution_dir).unwrap();
        fixture.cancel();
        fixture.wait_for_phase(Phase::CleanupCustodyJournal);
        let target = fixture
            .app
            .world()
            .get::<custody::Custody>(fixture.operation)
            .unwrap()
            .path
            .clone();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("foreign.txt"), b"foreign custody parent\n").unwrap();
        let foreign = std::fs::metadata(&target).unwrap();
        fixture.wait_for_phase(Phase::CleanupCustodyRecord);
        let actual = target.join(activation.execution_dir.file_name().unwrap());
        let moved = std::fs::metadata(&actual).unwrap();
        assert_eq!((moved.dev(), moved.ino()), (identity.dev(), identity.ino()));
        assert_eq!(fixture.checkout_path(), actual);
        std::fs::write(actual.join("retained.txt"), b"nested custody bytes\n").unwrap();
        fixture.restart_and_recover();
        let preserved = std::fs::metadata(&target).unwrap();
        assert_eq!(
            (preserved.dev(), preserved.ino()),
            (foreign.dev(), foreign.ino())
        );
        assert_eq!(
            std::fs::read(target.join("foreign.txt")).unwrap(),
            b"foreign custody parent\n"
        );
        assert_eq!(
            std::fs::read(actual.join("retained.txt")).unwrap(),
            b"nested custody bytes\n"
        );
        let world = fixture.app.world_mut();
        let mut recovered = world.query_filtered::<&Cwd, With<lease::Checkout>>();
        assert_eq!(
            recovered
                .iter(world)
                .map(|cwd| cwd.0.clone())
                .collect::<Vec<_>>(),
            vec![actual]
        );
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["rev-parse", "--verify", "refs/heads/vmux/work"]
                )
                .status
                .success()
        );
        drop(directory);
    }

    #[test]
    fn released_native_references_allow_cleanup_only_after_local_files_are_rechecked() {
        for dirty in [false, true] {
            let mut fixture = CheckoutFixture::new();
            std::fs::write(
                fixture.repository.path().join(".git/info/exclude"),
                "late.ignored\n",
            )
            .unwrap();
            let activation = fixture.finish_creation();
            let directory = std::fs::File::open(&activation.execution_dir).unwrap();
            fixture.cancel();
            fixture.wait_for_phase(Phase::CleanupReferencePause);
            let custody = fixture
                .app
                .world()
                .get::<custody::Custody>(fixture.operation)
                .unwrap()
                .path
                .clone();
            if dirty {
                std::fs::write(custody.join("late.ignored"), b"late ignored bytes\n").unwrap();
            }
            drop(directory);
            assert_eq!(fixture.complete(), OperationState::Canceled);
            if dirty {
                assert_eq!(
                    std::fs::read(activation.execution_dir.join("late.ignored")).unwrap(),
                    b"late ignored bytes\n"
                );
                assert!(
                    fixture
                        .git(
                            fixture.repository.path(),
                            &["rev-parse", "--verify", "refs/heads/vmux/work"]
                        )
                        .status
                        .success()
                );
            } else {
                assert!(!activation.execution_dir.exists());
                assert!(!custody.exists());
                assert!(
                    !fixture
                        .git(
                            fixture.repository.path(),
                            &["rev-parse", "--verify", "refs/heads/vmux/work"]
                        )
                        .status
                        .success()
                );
            }
        }
    }

    #[test]
    fn adopting_a_checkout_transfers_cleanup_responsibility_to_the_latest_owner() {
        let mut fixture = CheckoutFixture::new();
        let original = fixture.operation;
        let activation = fixture.finish_creation();
        let adopted = fixture.select("vmux/work");
        let selected = fixture.finish_creation();
        assert_eq!(selected.execution_dir, activation.execution_dir);
        fixture.operation = original;
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(activation.execution_dir.join("seed.txt").is_file());
        fixture.operation = adopted;
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(
            !activation.execution_dir.exists(),
            "retained: {:?}",
            fixture
                .app
                .world()
                .get::<RetainedReason>(fixture.operation)
                .map(|reason| &reason.0)
        );
        let branch = fixture.git(
            fixture.repository.path(),
            &["rev-parse", "--verify", "refs/heads/vmux/work"],
        );
        assert!(branch.status.success());
    }

    #[test]
    fn a_canceled_selection_preserves_an_already_committed_checkout() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let target = fixture
            .app
            .world()
            .get::<OperationTarget>(fixture.operation)
            .unwrap()
            .entity();
        fixture
            .app
            .world_mut()
            .entity_mut(target)
            .insert((CheckoutTargetRevision(1), activation.metadata.clone()));
        fixture.app.world_mut().write_message(CommitCheckout {
            operation: fixture.operation,
            target,
            revision: 1,
        });
        assert_eq!(fixture.complete(), OperationState::Succeeded);
        fixture.select("vmux/work");
        let selected = fixture.finish_creation();
        assert_eq!(selected.execution_dir, activation.execution_dir);
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        let branch = fixture.git(&activation.execution_dir, &["branch", "--show-current"]);
        assert!(branch.status.success());
        assert_eq!(
            String::from_utf8(branch.stdout).unwrap().trim(),
            "vmux/work"
        );
    }

    #[test]
    fn competing_checkout_requests_do_not_exhaust_the_native_worker_pool() {
        let mut fixture = CheckoutFixture::new();
        fixture.release_creation();
        let mut operations = vec![fixture.operation];
        for _ in 0..5 {
            let target = fixture.app.world_mut().spawn_empty().id();
            let operation = fixture
                .app
                .world_mut()
                .spawn((
                    Name::new("Competing checkout request"),
                    vmux_ecs::Active,
                    Operation::bundle("checkout-contention-test").unwrap(),
                    OperationTarget::new(target),
                    OperationTargetRevision(1),
                ))
                .id();
            fixture.app.world_mut().write_message(CreateCheckout {
                operation,
                directory: fixture.repository.path().to_path_buf(),
                managed_root: fixture.storage.path().to_path_buf(),
                slug_hint: "work".into(),
            });
            operations.push(operation);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.app.update();
            if operations.iter().all(|operation| {
                fixture
                    .app
                    .world()
                    .get::<PreparedCheckout>(*operation)
                    .is_some()
            }) {
                break;
            }
            for operation in &operations {
                if let Some(error) = fixture.app.world().get::<Failure>(*operation) {
                    panic!("competing checkout failed: {}", error.0);
                }
            }
            assert!(
                Instant::now() < deadline,
                "competing checkouts did not finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let branches = fixture.git(
            fixture.repository.path(),
            &["branch", "--format=%(refname:short)"],
        );
        assert!(branches.status.success());
        let mut actual: Vec<_> = String::from_utf8(branches.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        actual.sort();
        assert_eq!(
            actual,
            [
                "main",
                "vmux/work",
                "vmux/work-2",
                "vmux/work-3",
                "vmux/work-4",
                "vmux/work-5",
                "vmux/work-6"
            ]
        );
        for operation in operations {
            fixture.operation = operation;
            fixture.cancel();
            assert_eq!(fixture.complete(), OperationState::Canceled);
        }
        let remaining = fixture.git(
            fixture.repository.path(),
            &["branch", "--format=%(refname:short)"],
        );
        assert!(remaining.status.success());
        assert_eq!(String::from_utf8(remaining.stdout).unwrap().trim(), "main");
    }

    #[test]
    fn a_finished_operation_cannot_be_restarted_by_a_duplicate_request() {
        let mut fixture = CheckoutFixture::new();
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        fixture.app.world_mut().write_message(CreateCheckout {
            operation: fixture.operation,
            directory: fixture.repository.path().into(),
            managed_root: fixture.storage.path().into(),
            slug_hint: "work".into(),
        });
        fixture.app.update();
        assert_eq!(
            fixture.app.world().get::<OperationState>(fixture.operation),
            Some(&OperationState::Canceled)
        );
    }

    #[test]
    fn target_removed_before_acceptance_does_not_create_a_checkout() {
        let mut fixture = CheckoutFixture::new();
        let target = fixture
            .app
            .world()
            .get::<OperationTarget>(fixture.operation)
            .unwrap()
            .entity();
        fixture.app.world_mut().entity_mut(target).despawn();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert_eq!(
            std::fs::read_dir(fixture.storage.path()).unwrap().count(),
            0
        );
    }

    #[test]
    fn branch_selection_uses_configured_and_default_base_refs() {
        for configured in [None, Some("refs/heads/stable"), Some("refs/heads/missing")] {
            let mut fixture = CheckoutFixture::new();
            fixture.cancel();
            assert_eq!(fixture.complete(), OperationState::Canceled);
            fixture.release_creation();
            assert!(
                fixture
                    .git(fixture.repository.path(), &["switch", "-c", "source"])
                    .status
                    .success()
            );
            assert!(
                fixture
                    .git(fixture.repository.path(), &["branch", "stable"])
                    .status
                    .success()
            );
            assert!(
                fixture
                    .git(fixture.repository.path(), &["branch", "feature"])
                    .status
                    .success()
            );
            if let Some(reference) = configured {
                assert!(
                    fixture
                        .git(
                            fixture.repository.path(),
                            &["config", "branch.source.vmux-base", reference]
                        )
                        .status
                        .success()
                );
            }
            fixture.select("feature");
            let activation = fixture.finish_creation();
            let expected = if configured == Some("refs/heads/stable") {
                "stable"
            } else {
                "main"
            };
            assert_eq!(activation.metadata.base_ref, expected);
            let stored = fixture.git(
                fixture.repository.path(),
                &["config", "--get", "branch.feature.vmux-base"],
            );
            assert!(stored.status.success());
            assert_eq!(String::from_utf8(stored.stdout).unwrap().trim(), expected);
            fixture.cancel();
            assert_eq!(fixture.complete(), OperationState::Canceled);
        }
    }

    #[test]
    fn creation_from_a_detached_source_uses_the_actual_source_commit() {
        let mut fixture = CheckoutFixture::new();
        fixture.release_creation();
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &["checkout", "--detach", "--quiet", "HEAD"]
                )
                .status
                .success()
        );
        let source = fixture.git(fixture.repository.path(), &["rev-parse", "HEAD"]);
        let activation = fixture.finish_creation();
        let created = fixture.git(&activation.execution_dir, &["rev-parse", "HEAD"]);
        assert!(source.status.success());
        assert!(created.status.success());
        assert_eq!(source.stdout, created.stdout);
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
    }

    #[test]
    fn selecting_a_missing_registered_checkout_prunes_and_recreates_it() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.commit(&activation);
        std::fs::remove_dir_all(&activation.execution_dir).unwrap();
        fixture.select("vmux/work");
        let recreated = fixture.finish_creation();
        assert_eq!(recreated.execution_dir, activation.execution_dir);
        assert_eq!(
            std::fs::read_to_string(recreated.execution_dir.join("seed.txt")).unwrap(),
            "seed\n"
        );
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
    }

    #[test]
    fn a_locked_missing_registration_fails_without_retrying_forever() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.commit(&activation);
        assert!(
            fixture
                .git(
                    fixture.repository.path(),
                    &[
                        "worktree",
                        "lock",
                        activation.execution_dir.to_str().unwrap()
                    ]
                )
                .status
                .success()
        );
        std::fs::remove_dir_all(&activation.execution_dir).unwrap();
        fixture.select("vmux/work");
        assert_eq!(fixture.complete(), OperationState::Failed);
        let branch = fixture.git(
            fixture.repository.path(),
            &["rev-parse", "--verify", "refs/heads/vmux/work"],
        );
        assert!(branch.status.success());
    }

    #[test]
    fn restoring_a_persisted_missing_checkout_retains_it_after_cancellation() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.commit(&activation);
        std::fs::remove_dir_all(&activation.execution_dir).unwrap();
        fixture.restore(&activation.metadata);
        let restored = fixture.finish_creation();
        assert_eq!(restored.execution_dir, activation.execution_dir);
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert_eq!(
            std::fs::read_to_string(restored.execution_dir.join("seed.txt")).unwrap(),
            "seed\n"
        );
    }

    #[test]
    fn recovery_rejects_an_unregistered_missing_path_outside_managed_storage() {
        let mut fixture = CheckoutFixture::new();
        fixture.cancel();
        assert_eq!(fixture.complete(), OperationState::Canceled);
        assert!(
            fixture
                .git(fixture.repository.path(), &["branch", "feature"])
                .status
                .success()
        );
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("foreign");
        fixture.restore(&crate::tab::TabWorktree {
            repo_root: fixture.repository.path().to_string_lossy().into_owned(),
            checkout_dir: path.to_string_lossy().into_owned(),
            branch: "feature".into(),
            base_ref: "main".into(),
        });
        assert_eq!(fixture.complete(), OperationState::Failed);
        assert!(!path.exists());
    }

    #[test]
    fn restore_rejects_a_changed_branch_without_removing_the_directory() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.commit(&activation);
        assert!(
            fixture
                .git(&activation.execution_dir, &["switch", "-c", "user-changes"])
                .status
                .success()
        );
        fixture.restore(&activation.metadata);
        assert_eq!(fixture.complete(), OperationState::Failed);
        let branch = fixture.git(&activation.execution_dir, &["branch", "--show-current"]);
        assert!(branch.status.success());
        assert_eq!(
            String::from_utf8(branch.stdout).unwrap().trim(),
            "user-changes"
        );
    }

    #[test]
    fn lost_app_state_restores_journal_ownership_as_retained_canonical_checkouts() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        fixture.restart_and_recover();
        let mut query = fixture
            .app
            .world_mut()
            .query_filtered::<(&Cwd, &lease::Lease), With<lease::Checkout>>();
        let rows: Vec<_> = query.iter(fixture.app.world()).collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.0, activation.execution_dir);
        assert!(rows[0].1.0 == lease::Phase::Retained);
        let branch = fixture.git(&activation.execution_dir, &["branch", "--show-current"]);
        assert!(branch.status.success());
        assert_eq!(
            String::from_utf8(branch.stdout).unwrap().trim(),
            "vmux/work"
        );
        assert_eq!(
            std::fs::read_dir(activation.execution_dir.parent().unwrap().join(".leases"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn unreadable_journals_do_not_block_valid_recovery_or_delete_recorded_paths() {
        let mut fixture = CheckoutFixture::new();
        let activation = fixture.finish_creation();
        let invalid = activation
            .execution_dir
            .parent()
            .unwrap()
            .join(".leases/unreadable.json");
        std::fs::write(&invalid, "{incomplete").unwrap();
        fixture.restart_and_recover();
        let mut cursor = bevy::ecs::message::MessageCursor::<CheckoutRecoveryComplete>::default();
        let reports = fixture
            .app
            .world()
            .resource::<Messages<CheckoutRecoveryComplete>>();
        let report = cursor
            .read(reports)
            .find(|report| report.operation == fixture.operation)
            .unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.unreadable, 1);
        assert_eq!(std::fs::read_to_string(&invalid).unwrap(), "{incomplete");
        assert!(activation.execution_dir.join("seed.txt").is_file());
    }
}
