use super::super::deadline::DeadlineOwner;
use super::{
    CleanupFinished, CleanupHold, Destination, Effects, Job, JournalPath, KeepCheckout,
    LeaseCheckout, Phase, Progress, RepositoryGuard, RepositoryIdentity, RequestedBranch,
    RetainedReason, RetryDeadline, custody, io, lease,
};
use bevy::prelude::*;
use std::os::fd::AsRawFd;
use std::sync::Arc;
use vmux_ecs::{JobResult, OperationTarget};
use vmux_git::worktree::WorktreeCommand;

pub(super) struct CleanupPlugin;

impl Plugin for CleanupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                hold,
                lock,
                registration,
                identity,
                head,
                revision,
                status,
                references,
                reference_results,
                review_references,
                remove,
                branch,
                retain,
            )
                .chain()
                .in_set(CleanupSet),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct CleanupSet;

#[derive(Component)]
struct ReferenceObservation {
    referenced: bool,
    observed_guard: bool,
}

#[derive(Component)]
struct ReferenceAttempts(u8);

fn hold(
    operations: Query<(Entity, &Progress, &CleanupHold)>,
    owners: Query<&vmux_ecs::OperationState>,
    mut effects: Effects,
) {
    for (operation, progress, hold) in &operations {
        if progress.0 != Phase::CleanupLock {
            continue;
        }
        if owners.get(hold.0).is_ok_and(|state| {
            matches!(
                state,
                vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
            )
        }) {
            continue;
        }
        effects.retain(
            operation,
            None,
            "checkout retained because its native cleanup hold ended without a release".into(),
        );
    }
}

fn lock(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &LeaseCheckout,
        &Progress,
        Option<&RepositoryGuard>,
        Option<&Job>,
        Has<KeepCheckout>,
        Option<&RetainedReason>,
        Has<CleanupHold>,
    )>,
    leases: lease::Leases,
    completed: Query<&JobResult<io::Acquire>>,
    durable: Query<(), With<lease::Durable>>,
    mut effects: Effects,
) {
    for (operation, repository, checkout, progress, guard, job, keep, retained, held) in &operations
    {
        if progress.0 != Phase::CleanupLock || held {
            continue;
        }
        if !leases.owns(checkout.0, operation) {
            effects
                .commands
                .entity(operation)
                .insert((CleanupFinished, Progress(Phase::Complete)));
            continue;
        }
        if durable.contains(checkout.0) {
            effects.retain(
                operation,
                None,
                "checkout retained for its persisted workspace".into(),
            );
            continue;
        }
        if keep {
            let reason = retained.map_or_else(
                || "checkout retained by its current owner".to_string(),
                |retained| retained.0.clone(),
            );
            effects.retain(operation, None, reason);
            continue;
        }
        effects
            .commands
            .entity(checkout.0)
            .insert(lease::Lease(lease::Phase::Cleaning));
        if guard.is_some() && job.is_none() {
            effects
                .commands
                .entity(operation)
                .insert(Progress(Phase::CleanupRegistration));
            continue;
        }
        let Some(job) = job else {
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
                effects.finish(operation, job.0, Phase::CleanupRegistration);
            }
            Ok(Ok(None)) => {
                effects.finish(operation, job.0, Phase::CleanupLockPause);
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
            Ok(Err(error)) | Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn registration(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Destination,
        &RequestedBranch,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, destination, expected, progress, job) in &operations {
        if progress.0 != Phase::CleanupRegistration {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
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
            Ok(Ok((output, _, true))) => {
                let mut path = None;
                let mut branch = None;
                let mut matching = false;
                for field in output.split('\0').chain(std::iter::once("")) {
                    if field.is_empty() {
                        if path.as_deref() == Some(destination.0.as_path())
                            && branch == Some(expected.0.as_str())
                        {
                            matching = true;
                        }
                        path = None;
                        branch = None;
                    } else if let Some(value) = field.strip_prefix("worktree ") {
                        path = Some(std::path::PathBuf::from(value));
                    } else if let Some(value) = field.strip_prefix("branch refs/heads/") {
                        branch = Some(value);
                    }
                }
                if matching {
                    effects.finish(operation, job.0, Phase::CleanupIdentity);
                } else {
                    effects.retain(
                        operation,
                        Some(job.0),
                        "worktree registration changed before cleanup".into(),
                    );
                }
            }
            Ok(Ok((_, error, _))) => {
                effects.retain(operation, Some(job.0), error.trim().to_string())
            }
            Ok(Err(error)) => effects.retain(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn identity(
    operations: Query<(
        Entity,
        &Destination,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
    )>,
    identities: Query<(&lease::Identity, Has<lease::DirectoryGuard>)>,
    completed: Query<&JobResult<io::Inspect>>,
    mut effects: Effects,
) {
    for (operation, destination, checkout, progress, job) in &operations {
        if progress.0 != Phase::CleanupIdentity {
            continue;
        }
        let Ok((identity, true)) = identities.get(checkout.0) else {
            effects.retain(
                operation,
                None,
                "checkout has no verified creation identity".into(),
            );
            continue;
        };
        let Some(job) = job else {
            let task = effects.request(operation, io::Inspect(destination.0.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(Some(metadata))) if identity.matches(metadata) => {
                effects.finish(operation, job.0, Phase::CleanupHead);
            }
            Ok(Ok(_)) => effects.retain(
                operation,
                Some(job.0),
                "checkout directory changed before cleanup".into(),
            ),
            Ok(Err(error)) | Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn head(
    operations: Query<(
        Entity,
        &Destination,
        &RepositoryGuard,
        &RequestedBranch,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, destination, guard, expected, progress, job) in &operations {
        if progress.0 != Phase::CleanupHead {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
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
            Ok(Ok((current, _, true))) if current.trim() == expected.0 => {
                effects.finish(operation, job.0, Phase::CleanupRevision);
            }
            Ok(Ok(_)) => effects.retain(
                operation,
                Some(job.0),
                "worktree HEAD changed before cleanup".into(),
            ),
            Ok(Err(error)) => effects.retain(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn revision(
    operations: Query<(
        Entity,
        &Destination,
        &RepositoryGuard,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
    )>,
    revisions: Query<&lease::InitialRevision>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, destination, guard, checkout, progress, job) in &operations {
        if progress.0 != Phase::CleanupRevision {
            continue;
        }
        let Ok(expected) = revisions.get(checkout.0) else {
            effects.retain(
                operation,
                None,
                "checkout has no verified creation revision".into(),
            );
            continue;
        };
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
                    directory: destination.0.clone(),
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
            Ok(Ok((current, _, true))) if current.trim() == expected.0 => {
                effects.finish(operation, job.0, Phase::CleanupStatus);
            }
            Ok(Ok(_)) => effects.retain(
                operation,
                Some(job.0),
                "worktree revision changed before cleanup".into(),
            ),
            Ok(Err(error)) => effects.retain(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn status(
    operations: Query<(
        Entity,
        &Destination,
        &RepositoryGuard,
        &Progress,
        Option<&Job>,
        Has<custody::Moved>,
        Has<custody::Inspected>,
    )>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, destination, guard, progress, job, moved, inspected) in &operations {
        if progress.0 != Phase::CleanupStatus {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
                    directory: destination.0.clone(),
                    arguments: vec![
                        "status".into(),
                        "--porcelain=v1".into(),
                        "--untracked-files=all".into(),
                        "--ignored=matching".into(),
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
            Ok(Ok((output, _, true))) if output.trim().is_empty() => {
                let next = if inspected {
                    Phase::CleanupRemove
                } else if moved {
                    Phase::CleanupReferences
                } else {
                    Phase::CleanupCustodyPlan
                };
                effects.finish(operation, job.0, next);
            }
            Ok(Ok((_, error, true))) => effects.retain(
                operation,
                Some(job.0),
                format!("checkout contains local files and was retained: {error}"),
            ),
            Ok(Ok((_, error, _))) => {
                effects.retain(operation, Some(job.0), error.trim().to_string())
            }
            Ok(Err(error)) => effects.retain(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn references(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Destination,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
    )>,
    directories: Query<&lease::DirectoryGuard>,
    mut effects: Effects,
) {
    for (operation, repository, guard, destination, checkout, progress, job) in &operations {
        if progress.0 != Phase::CleanupReferences {
            continue;
        }
        let Ok(_) = directories.get(checkout.0) else {
            effects.retain(
                operation,
                None,
                "checkout directory handle is unavailable".into(),
            );
            continue;
        };
        if job.is_none() {
            let task = effects.request(
                operation,
                io::References {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "-n".into(),
                        "-P".into(),
                        "-F0pfn".into(),
                        "-a".into(),
                        "+D".into(),
                        destination.0.to_string_lossy().into_owned(),
                    ],
                    guard: Arc::clone(&guard.0),
                },
            );
            effects.commands.entity(operation).insert(Job(task));
        }
    }
}

fn reference_results(
    operations: Query<(Entity, &LeaseCheckout, &Progress, &Job)>,
    directories: Query<&lease::DirectoryGuard>,
    completed: Query<&JobResult<io::References>>,
    mut effects: Effects,
) {
    for (operation, checkout, progress, job) in &operations {
        if progress.0 != Phase::CleanupReferences {
            continue;
        }
        let Ok(directory) = directories.get(checkout.0) else {
            effects.retain(
                operation,
                Some(job.0),
                "checkout directory handle is unavailable".into(),
            );
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        let output = match &result.0 {
            Ok(Ok(output)) => output,
            Ok(Err(error)) | Err(error) => {
                effects.retain(operation, Some(job.0), error.clone());
                continue;
            }
        };
        if !output.stderr.is_empty()
            || output
                .status
                .code()
                .is_none_or(|code| code != 0 && code != 1)
        {
            effects.retain(
                operation,
                Some(job.0),
                format!(
                    "native checkout references could not be inspected: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            );
            continue;
        }
        let own_pid = std::process::id();
        let own_fd = directory._directory.as_raw_fd().to_string();
        let mut pid = None;
        let mut descriptor = None;
        let mut referenced = false;
        let mut observed_guard = false;
        for field in output.stdout.split(|byte| *byte == 0) {
            let field = match std::str::from_utf8(field) {
                Ok(field) => field.trim_start_matches('\n'),
                Err(_) => {
                    referenced = true;
                    break;
                }
            };
            if let Some(value) = field.strip_prefix('p') {
                pid = value.parse::<u32>().ok();
                descriptor = None;
            } else if let Some(value) = field.strip_prefix('f') {
                descriptor = Some(value);
            } else if field.starts_with('n') {
                if pid == Some(own_pid) && descriptor == Some(own_fd.as_str()) {
                    observed_guard = true;
                } else {
                    referenced = true;
                    break;
                }
            }
        }
        effects
            .commands
            .entity(operation)
            .insert(ReferenceObservation {
                referenced,
                observed_guard,
            });
        effects.finish(operation, job.0, Phase::CleanupReferenceReview);
    }
}

fn review_references(
    operations: Query<(
        Entity,
        &Progress,
        &ReferenceObservation,
        Option<&ReferenceAttempts>,
    )>,
    mut effects: Effects,
) {
    for (operation, progress, observation, attempts) in &operations {
        if progress.0 != Phase::CleanupReferenceReview {
            continue;
        }
        effects
            .commands
            .entity(operation)
            .remove::<ReferenceObservation>();
        if observation.observed_guard && !observation.referenced {
            effects
                .commands
                .entity(operation)
                .insert((custody::Inspected, Progress(Phase::CleanupRegistration)));
            continue;
        }
        let attempts = attempts.map_or(0, |attempts| attempts.0);
        if attempts >= 2 {
            effects.retain(
                operation,
                None,
                "checkout retained because native references could not be cleared".into(),
            );
            continue;
        }
        let at = std::time::Instant::now() + std::time::Duration::from_millis(20);
        let registration = effects.deadlines.register(
            DeadlineOwner::Checkout(operation),
            at,
            effects.proxy.as_deref(),
        );
        effects.commands.entity(operation).insert((
            ReferenceAttempts(attempts + 1),
            RetryDeadline {
                at,
                _registration: registration,
            },
            Progress(Phase::CleanupReferencePause),
        ));
    }
}

fn remove(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Destination,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, destination, progress, job) in &operations {
        if progress.0 != Phase::CleanupRemove {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "worktree".into(),
                        "remove".into(),
                        destination.0.to_string_lossy().into_owned(),
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
            Ok(Ok((_, _, true))) => effects.finish(operation, job.0, Phase::CleanupBranch),
            Ok(Ok((_, error, _))) => {
                effects.retain(operation, Some(job.0), error.trim().to_string())
            }
            Ok(Err(error)) => effects.retain(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn branch(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &RequestedBranch,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
        Option<&JournalPath>,
    )>,
    branches: Query<(), With<lease::DeleteBranch>>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, branch, checkout, progress, job, journal) in &operations {
        if progress.0 != Phase::CleanupBranch {
            continue;
        }
        let next = if journal.is_some() {
            Phase::CleanupJournal
        } else {
            Phase::Complete
        };
        if !branches.contains(checkout.0) {
            effects
                .commands
                .entity(checkout.0)
                .insert(lease::Lease(lease::Phase::Released))
                .remove::<(OperationTarget, lease::DirectoryGuard)>();
            effects
                .commands
                .entity(operation)
                .insert((CleanupFinished, Progress(next)));
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec!["branch".into(), "-d".into(), branch.0.clone()],
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
                    .entity(checkout.0)
                    .insert(lease::Lease(lease::Phase::Released))
                    .remove::<(OperationTarget, lease::DirectoryGuard)>();
                effects.commands.entity(operation).insert(CleanupFinished);
                effects.finish(operation, job.0, next);
            }
            Ok(Ok((_, error, _))) => {
                effects.retain(operation, Some(job.0), error.trim().to_string())
            }
            Ok(Err(error)) => effects.retain(operation, Some(job.0), error.0.clone()),
            Err(error) => effects.retain(operation, Some(job.0), error.clone()),
        }
    }
}

fn retain(
    operations: Query<(
        Entity,
        &Progress,
        &LeaseCheckout,
        &RetainedReason,
        Has<custody::Moved>,
        Has<custody::Returned>,
    )>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    for (operation, progress, checkout, reason, moved, returned) in &operations {
        if progress.0 != Phase::Retained {
            continue;
        }
        if moved && !returned {
            effects
                .commands
                .entity(operation)
                .insert(Progress(Phase::CleanupCustodyReturnCheck));
            continue;
        }
        if leases.owns(checkout.0, operation) {
            effects
                .commands
                .entity(checkout.0)
                .insert(lease::Lease(lease::Phase::Retained))
                .remove::<(OperationTarget, lease::DirectoryGuard)>();
        }
        bevy::log::warn!(operation = ?operation, checkout = ?checkout.0, reason = %reason.0, "managed checkout retained after cleanup refusal");
        effects
            .commands
            .entity(operation)
            .insert((CleanupFinished, Progress(Phase::Complete)))
            .remove::<RepositoryGuard>();
    }
}
