use super::{
    Destination, Effects, Job, JournalPath, LeaseCheckout, Phase, Progress, RepositoryGuard,
    RepositoryIdentity, RequestedBranch, RetainedReason, cleanup, io, lease,
};
use bevy::prelude::*;
use std::fs::Metadata;
use std::path::PathBuf;
use std::sync::Arc;
use vmux_ecs::{Cwd, JobResult};
use vmux_git::worktree::WorktreeCommand;

pub(super) struct CustodyPlugin;

impl Plugin for CustodyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                plan,
                plan_return,
                journal,
                move_directory,
                observe_move,
                check_return,
            )
                .chain()
                .in_set(CustodySet)
                .after(cleanup::CleanupSet),
        );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CustodySet;

#[derive(Component)]
pub(super) struct Custody {
    pub(super) original: PathBuf,
    pub(super) path: PathBuf,
}

#[derive(Component)]
pub(super) struct Moved;
#[derive(Component)]
pub(super) struct Returned;
#[derive(Component)]
pub(super) struct Inspected;
#[derive(Component)]
struct Returning;

#[derive(Component)]
pub(super) struct Locations {
    pub(super) source: PathBuf,
    pub(super) target: PathBuf,
    pub(super) candidates: Vec<PathBuf>,
    next: usize,
    found: Option<PathBuf>,
    error: Option<String>,
}

impl Locations {
    pub(super) fn new(source: PathBuf, target: PathBuf) -> Result<Self, String> {
        if !source.is_absolute() || !target.is_absolute() {
            return Err("checkout move paths must be absolute".into());
        }
        let Some(name) = source.file_name() else {
            return Err("checkout move source has no file name".into());
        };
        let nested = target.join(name);
        let mut candidates = vec![source.clone()];
        for path in [&target, &nested] {
            if !candidates.contains(path) {
                candidates.push(path.clone());
            }
        }
        Ok(Self {
            source,
            target,
            candidates,
            next: 0,
            found: None,
            error: None,
        })
    }

    pub(super) fn current(&self) -> Option<&PathBuf> {
        self.candidates.get(self.next)
    }

    pub(super) fn observe(
        &mut self,
        result: &Result<Result<Option<Metadata>, String>, String>,
        identity: &lease::Identity,
    ) {
        match result {
            Ok(Ok(Some(metadata))) if identity.matches(metadata) => {
                if self.found.is_some() {
                    self.error = Some("checkout move identity exists at multiple locations".into());
                } else {
                    self.found = self.current().cloned();
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) | Err(error) => self.error = Some(error.clone()),
        }
        self.next += 1;
    }

    pub(super) fn resolved(&self) -> Result<&PathBuf, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.found
            .as_ref()
            .ok_or_else(|| "checkout move identity was not found at its recorded locations".into())
    }
}

fn plan(
    operations: Query<(
        Entity,
        &vmux_ecs::Uuid,
        &Destination,
        &Progress,
        Option<&Custody>,
    )>,
    mut effects: Effects,
) {
    for (operation, id, destination, progress, custody) in &operations {
        if progress.0 != Phase::CleanupCustodyPlan {
            continue;
        }
        let Some(parent) = destination.0.parent() else {
            effects.retain(operation, None, "checkout has no custody parent".into());
            continue;
        };
        let target = match custody {
            Some(custody) => custody.path.clone(),
            None => parent.join(format!(".vmux-custody-{}", id.0)),
        };
        let locations = match Locations::new(destination.0.clone(), target.clone()) {
            Ok(locations) => locations,
            Err(error) => {
                effects.retain(operation, None, error);
                continue;
            }
        };
        effects.commands.entity(operation).insert((
            locations,
            Custody {
                original: destination.0.clone(),
                path: target,
            },
            Progress(Phase::CleanupCustodyJournal),
        ));
    }
}

fn plan_return(
    operations: Query<(Entity, &Custody, &Destination, &Progress)>,
    mut effects: Effects,
) {
    for (operation, custody, destination, progress) in &operations {
        if progress.0 != Phase::CleanupCustodyReturnPlan {
            continue;
        }
        let locations = match Locations::new(destination.0.clone(), custody.original.clone()) {
            Ok(locations) => locations,
            Err(error) => {
                effects.commands.entity(operation).insert(Returned);
                effects.retain(operation, None, error);
                continue;
            }
        };
        effects.commands.entity(operation).insert((
            locations,
            Returning,
            Progress(Phase::CleanupCustodyJournal),
        ));
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
        &Custody,
        &Locations,
        &Progress,
        Option<&Job>,
        Has<Returning>,
        Has<RetainedReason>,
        Has<Moved>,
    )>,
    identities: Query<(
        &lease::Identity,
        &lease::InitialRevision,
        Has<lease::DeleteBranch>,
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
        path,
        checkout,
        custody,
        locations,
        progress,
        job,
        returning,
        failed,
        moved,
    ) in &operations
    {
        if !matches!(
            progress.0,
            Phase::CleanupCustodyJournal | Phase::CleanupCustodyRecord
        ) {
            continue;
        }
        let Some(job) = job else {
            let Ok((identity, revision, delete)) = identities.get(checkout.0) else {
                effects.commands.entity(operation).insert(Returned);
                effects.retain(
                    operation,
                    None,
                    "checkout custody evidence is unavailable".into(),
                );
                continue;
            };
            let data = serde_json::json!({
                "owner": id.0, "repository": repository.0.root,
                "common_dir": repository.0.common_dir, "path": destination.0,
                "branch": branch.0, "delete_branch": delete,
                "observed_identity": identity, "creation_revision": revision.0,
                "custody": custody.path, "move_source": locations.source,
                "move_target": locations.target, "move_candidates": locations.candidates,
            });
            let bytes = match serde_json::to_vec(&data) {
                Ok(bytes) => bytes,
                Err(error) => {
                    effects.commands.entity(operation).insert(Returned);
                    effects.retain(operation, None, error.to_string());
                    continue;
                }
            };
            let task = effects.request(
                operation,
                io::WriteJournal {
                    path: path.0.clone(),
                    staging: path.0.with_extension("custody-staging"),
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
                let next = if progress.0 == Phase::CleanupCustodyJournal {
                    if returning {
                        Phase::CleanupCustodyReturn
                    } else {
                        Phase::CleanupCustodyMove
                    }
                } else if returning || !moved {
                    Phase::Retained
                } else if failed {
                    Phase::CleanupCustodyReturnCheck
                } else {
                    Phase::CleanupReferences
                };
                effects.finish(operation, job.0, next);
            }
            Ok(Err(error)) | Err(error) => {
                effects.commands.entity(operation).insert(Returned);
                effects.retain(operation, Some(job.0), error.clone());
            }
        }
    }
}

fn move_directory(
    operations: Query<(
        Entity,
        &RepositoryIdentity,
        &RepositoryGuard,
        &Locations,
        &Progress,
        Option<&Job>,
    )>,
    completed: Query<&JobResult<WorktreeCommand>>,
    mut effects: Effects,
) {
    for (operation, repository, guard, locations, progress, job) in &operations {
        if !matches!(
            progress.0,
            Phase::CleanupCustodyMove | Phase::CleanupCustodyReturn
        ) {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(
                operation,
                WorktreeCommand {
                    directory: repository.0.root.clone(),
                    arguments: vec![
                        "worktree".into(),
                        "move".into(),
                        locations.source.to_string_lossy().into_owned(),
                        locations.target.to_string_lossy().into_owned(),
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
        let error = match &result.0 {
            Ok(Ok((_, _, true))) => None,
            Ok(Ok((_, error, _))) => Some(error.trim().to_string()),
            Ok(Err(error)) => Some(error.0.clone()),
            Err(error) => Some(error.clone()),
        };
        if let Some(error) = error {
            effects
                .commands
                .entity(operation)
                .insert(RetainedReason(error));
        }
        effects.finish(operation, job.0, Phase::CleanupCustodyInspect);
    }
}

fn observe_move(
    mut operations: Query<(
        Entity,
        &mut Custody,
        &mut Locations,
        &LeaseCheckout,
        &Progress,
        Option<&Job>,
        Has<Returning>,
    )>,
    identities: Query<&lease::Identity>,
    completed: Query<&JobResult<io::Inspect>>,
    mut effects: Effects,
) {
    for (operation, mut custody, mut locations, checkout, progress, job, returning) in
        &mut operations
    {
        if progress.0 != Phase::CleanupCustodyInspect {
            continue;
        }
        let Some(job) = job else {
            let Some(path) = locations.current() else {
                continue;
            };
            let task = effects.request(operation, io::Inspect(path.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        if returning {
            effects.commands.entity(operation).insert(Returned);
        }
        let Ok(identity) = identities.get(checkout.0) else {
            effects.retain(
                operation,
                Some(job.0),
                "checkout move identity is unavailable".into(),
            );
            continue;
        };
        locations.observe(&result.0, identity);
        if locations.current().is_some() {
            effects.finish(operation, job.0, Phase::CleanupCustodyInspect);
            continue;
        }
        let actual = match locations.resolved() {
            Ok(path) => path.clone(),
            Err(error) => {
                effects.commands.entity(operation).insert(Returned);
                effects.retain(operation, Some(job.0), error);
                continue;
            }
        };
        custody.path = actual.clone();
        effects
            .commands
            .entity(operation)
            .insert(Destination(actual.clone()));
        effects
            .commands
            .entity(checkout.0)
            .insert(Cwd(actual.clone()));
        if actual == custody.original {
            effects.commands.entity(operation).remove::<Moved>();
            if !returning {
                effects
                    .commands
                    .entity(operation)
                    .insert(RetainedReason(format!(
                        "checkout custody move did not relocate {}",
                        actual.display()
                    )));
            }
        } else {
            effects.commands.entity(operation).insert(Moved);
            if returning {
                effects
                    .commands
                    .entity(operation)
                    .insert(RetainedReason(format!(
                        "checkout retained at {} because its original path was not restored",
                        actual.display()
                    )));
            }
        }
        effects.finish(operation, job.0, Phase::CleanupCustodyRecord);
    }
}

fn check_return(
    operations: Query<(Entity, &Custody, &Progress, Option<&Job>)>,
    completed: Query<&JobResult<io::Inspect>>,
    mut effects: Effects,
) {
    for (operation, custody, progress, job) in &operations {
        if progress.0 != Phase::CleanupCustodyReturnCheck {
            continue;
        }
        let Some(job) = job else {
            let task = effects.request(operation, io::Inspect(custody.original.clone()));
            effects.commands.entity(operation).insert(Job(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        match &result.0 {
            Ok(Ok(None)) => effects.finish(operation, job.0, Phase::CleanupCustodyReturnPlan),
            _ => {
                effects.commands.entity(operation).insert(Returned);
                effects.retain(operation, Some(job.0), format!(
                    "checkout retained in {} because its original path is occupied or unavailable",
                    custody.path.display(),
                ));
            }
        }
    }
}
