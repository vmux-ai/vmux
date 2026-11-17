use super::{Effects, custody::Locations, io, lease};
use bevy::prelude::*;
use std::path::PathBuf;
use vmux_ecs::{Cwd, JobResult, Operation, OperationState};
use vmux_git::state::CheckedOutBranch;

pub(super) struct RecoveryPlugin;

impl Plugin for RecoveryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RecoverCheckouts>()
            .add_message::<CheckoutRecoveryComplete>()
            .add_systems(
                Update,
                (
                    accept,
                    repositories,
                    journals,
                    read,
                    plan_locations,
                    locate,
                    materialize,
                    complete,
                )
                    .chain()
                    .in_set(RecoverySet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct RecoverySet;

#[derive(Message)]
pub struct RecoverCheckouts {
    pub operation: Entity,
    pub managed_root: PathBuf,
}

#[derive(Message)]
pub struct CheckoutRecoveryComplete {
    pub operation: Entity,
    pub imported: usize,
    pub unreadable: usize,
}

#[derive(Component)]
struct Recovery;
#[derive(Component)]
struct RepositoryDirectory;
#[derive(Component)]
struct JournalFile;
#[derive(Component)]
struct Pending;
#[derive(Component)]
struct NativeJob(Entity);
#[derive(Component)]
struct Unreadable(String);
#[derive(Component)]
struct Imported;
#[derive(Component)]
struct Located;
#[derive(Component, serde::Deserialize)]
struct Record {
    owner: uuid::Uuid,
    repository: PathBuf,
    common_dir: PathBuf,
    path: PathBuf,
    branch: String,
    delete_branch: bool,
    observed_identity: Option<lease::Identity>,
    creation_revision: Option<String>,
    #[serde(default)]
    durable: bool,
    #[serde(default)]
    custody: Option<PathBuf>,
    #[serde(default)]
    move_source: Option<PathBuf>,
    #[serde(default)]
    move_target: Option<PathBuf>,
    #[serde(default)]
    move_candidates: Option<Vec<PathBuf>>,
}

fn accept(
    mut requests: MessageReader<RecoverCheckouts>,
    operations: Query<&OperationState, (With<Operation>, Without<Recovery>)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !operations
            .get(request.operation)
            .is_ok_and(|state| *state == OperationState::Pending)
        {
            continue;
        }
        commands.entity(request.operation).insert((
            Recovery,
            Pending,
            Cwd(request.managed_root.clone()),
            OperationState::Running,
        ));
    }
}

fn repositories(
    operations: Query<(Entity, &Cwd, Option<&NativeJob>), (With<Recovery>, With<Pending>)>,
    completed: Query<&JobResult<io::DirectoryPaths>>,
    mut effects: Effects,
) {
    for (operation, root, job) in &operations {
        let Some(job) = job else {
            let task = effects.request(operation, io::DirectoryPaths(root.0.clone()));
            effects.commands.entity(operation).insert(NativeJob(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        if let Ok(Ok(Some(paths))) = &result.0 {
            for path in paths {
                effects.commands.spawn((
                    Name::new("Checkout journal repository directory"),
                    RepositoryDirectory,
                    Cwd(path.join(".leases")),
                    Pending,
                    ChildOf(operation),
                ));
            }
        } else if let Ok(Err(error)) | Err(error) = &result.0 {
            effects
                .commands
                .entity(operation)
                .insert(Unreadable(error.clone()));
        }
        effects.commands.entity(job.0).try_despawn();
        effects
            .commands
            .entity(operation)
            .remove::<(NativeJob, Pending)>();
    }
}

fn journals(
    directories: Query<
        (Entity, &Cwd, &ChildOf, Option<&NativeJob>),
        (With<RepositoryDirectory>, With<Pending>),
    >,
    completed: Query<&JobResult<io::DirectoryPaths>>,
    mut effects: Effects,
) {
    for (directory, cwd, root, job) in &directories {
        let Some(job) = job else {
            let task = effects.request(root.parent(), io::DirectoryPaths(cwd.0.clone()));
            effects.commands.entity(directory).insert(NativeJob(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        if let Ok(Ok(Some(paths))) = &result.0 {
            for path in paths {
                if path.extension().is_none_or(|extension| extension != "json") {
                    continue;
                }
                effects.commands.spawn((
                    Name::new("Persisted checkout journal"),
                    JournalFile,
                    Cwd(path.clone()),
                    Pending,
                    ChildOf(root.parent()),
                ));
            }
        } else if let Ok(Err(error)) | Err(error) = &result.0 {
            effects
                .commands
                .entity(directory)
                .insert(Unreadable(error.clone()))
                .remove::<(NativeJob, Pending)>();
            effects.commands.entity(job.0).try_despawn();
            continue;
        }
        effects.commands.entity(job.0).try_despawn();
        effects.commands.entity(directory).try_despawn();
    }
}

fn read(
    files: Query<(Entity, &Cwd, &ChildOf, Option<&NativeJob>), (With<JournalFile>, With<Pending>)>,
    completed: Query<&JobResult<io::ReadFile>>,
    mut effects: Effects,
) {
    for (file, cwd, root, job) in &files {
        let Some(job) = job else {
            let task = effects.request(
                root.parent(),
                io::ReadFile {
                    path: cwd.0.clone(),
                    max_bytes: 1_048_576,
                },
            );
            effects.commands.entity(file).insert(NativeJob(task));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        let record = match &result.0 {
            Ok(Ok(bytes)) if bytes.len() <= 1_048_576 => {
                serde_json::from_slice::<Record>(bytes).map_err(|error| error.to_string())
            }
            Ok(Ok(_)) => Err("checkout journal exceeds the read limit".to_string()),
            Ok(Err(error)) | Err(error) => Err(error.clone()),
        };
        match record {
            Ok(record)
                if record.repository.is_absolute()
                    && record.common_dir.is_absolute()
                    && record.path.is_absolute()
                    && !record.branch.is_empty() =>
            {
                effects.commands.entity(file).insert(record);
            }
            Ok(_) => {
                effects.commands.entity(file).insert(Unreadable(
                    "checkout journal contains invalid paths or branch".into(),
                ));
            }
            Err(error) => {
                effects.commands.entity(file).insert(Unreadable(error));
            }
        }
        effects.commands.entity(job.0).try_despawn();
        effects
            .commands
            .entity(file)
            .remove::<(NativeJob, Pending)>();
    }
}

fn plan_locations(
    records: Query<(Entity, &Record), (With<JournalFile>, Without<Located>, Without<Locations>)>,
    mut effects: Effects,
) {
    for (file, record) in &records {
        let mut locations = match (&record.move_source, &record.move_target, &record.custody) {
            (Some(source), Some(target), _) => Locations::new(source.clone(), target.clone()),
            (None, None, Some(custody))
                if custody.is_absolute() && custody.parent() == record.path.parent() =>
            {
                let mut locations = Locations::new(record.path.clone(), custody.clone());
                if let Ok(locations) = &mut locations
                    && let Some(name) = custody.file_name()
                {
                    let returned = record.path.join(name);
                    if !locations.candidates.contains(&returned) {
                        locations.candidates.push(returned);
                    }
                }
                locations
            }
            (None, None, None) => {
                effects.commands.entity(file).insert(Located);
                continue;
            }
            _ => Err("checkout custody journal contains invalid move paths".into()),
        };
        if let Ok(candidate_locations) = &locations
            && (!candidate_locations.candidates.contains(&record.path)
                || record.observed_identity.is_none()
                || record
                    .move_candidates
                    .as_ref()
                    .is_some_and(|candidates| *candidates != candidate_locations.candidates))
        {
            locations = Err("checkout custody journal contains invalid location evidence".into());
        }
        match locations {
            Ok(locations) => {
                effects.commands.entity(file).insert(locations);
            }
            Err(error) => {
                effects
                    .commands
                    .entity(file)
                    .insert(Unreadable(error))
                    .remove::<Record>();
            }
        }
    }
}

fn locate(
    mut records: Query<
        (
            Entity,
            &mut Record,
            &mut Locations,
            &ChildOf,
            Option<&NativeJob>,
        ),
        (With<JournalFile>, Without<Located>),
    >,
    completed: Query<&JobResult<io::Inspect>>,
    mut effects: Effects,
) {
    for (file, mut record, mut locations, root, job) in &mut records {
        let Some(job) = job else {
            let Some(path) = locations.current() else {
                continue;
            };
            let task = effects.request(root.parent(), io::Inspect(path.clone()));
            effects
                .commands
                .entity(file)
                .insert((Pending, NativeJob(task)));
            continue;
        };
        let Ok(result) = completed.get(job.0) else {
            continue;
        };
        effects.commands.entity(job.0).try_despawn();
        effects.commands.entity(file).remove::<NativeJob>();
        let Some(identity) = record.observed_identity else {
            effects
                .commands
                .entity(file)
                .insert(Unreadable(
                    "checkout custody identity is unavailable".into(),
                ))
                .remove::<(Record, Pending)>();
            continue;
        };
        locations.observe(&result.0, &identity);
        if locations.current().is_some() {
            continue;
        }
        match locations.resolved() {
            Ok(actual) => {
                record.path = actual.clone();
                effects.commands.entity(file).insert(Located);
            }
            Err(error) => {
                effects
                    .commands
                    .entity(file)
                    .insert(Unreadable(error))
                    .remove::<Record>();
            }
        }
        effects.commands.entity(file).remove::<Pending>();
    }
}

fn materialize(
    records: Query<(Entity, &Cwd, &Record), (With<JournalFile>, With<Located>)>,
    repositories: Query<(Entity, &lease::CommonDirectory), With<lease::Repository>>,
    leases: lease::Leases,
    mut effects: Effects,
) {
    let mut accepted = std::collections::HashMap::new();
    let mut imported = std::collections::HashSet::new();
    for (file, journal, record) in &records {
        if leases.find(&record.path).is_some() || !imported.insert(record.path.clone()) {
            effects.commands.entity(file).remove::<Record>();
            continue;
        }
        let repository = match repositories
            .iter()
            .find(|(_, directory)| directory.0 == record.common_dir)
        {
            Some((repository, _)) => repository,
            None => match accepted.get(&record.common_dir) {
                Some(repository) => *repository,
                None => {
                    let repository = effects
                        .commands
                        .spawn((
                            Name::new("Recovered checkout repository"),
                            lease::Repository,
                            Cwd(record.common_dir.clone()),
                            lease::CommonDirectory(record.common_dir.clone()),
                        ))
                        .id();
                    accepted.insert(record.common_dir.clone(), repository);
                    repository
                }
            },
        };
        let mut checkout = effects.commands.spawn((
            Name::new(format!("Recovered managed checkout from {}", record.owner)),
            vmux_ecs::Active,
            lease::Checkout,
            Cwd(record.path.clone()),
            CheckedOutBranch(record.branch.clone()),
            lease::Lease(lease::Phase::Retained),
            lease::JournalPath(journal.0.clone()),
            ChildOf(repository),
        ));
        if let Some(identity) = record.observed_identity {
            checkout.insert(identity);
        }
        if let Some(revision) = &record.creation_revision {
            checkout.insert(lease::InitialRevision(revision.clone()));
        }
        if record.delete_branch {
            checkout.insert(lease::DeleteBranch);
        }
        if record.durable {
            checkout.insert(lease::Durable);
        }
        effects
            .commands
            .entity(file)
            .insert(Imported)
            .remove::<Record>();
    }
}

fn complete(
    operations: Query<
        (Entity, &OperationState, Option<&Unreadable>),
        (With<Recovery>, Without<Pending>),
    >,
    pending: Query<&ChildOf, With<Pending>>,
    files: Query<(&ChildOf, Option<&Unreadable>, Has<Imported>)>,
    mut completed: MessageWriter<CheckoutRecoveryComplete>,
    mut commands: Commands,
) {
    for (operation, state, failure) in &operations {
        if *state != OperationState::Running
            || pending.iter().any(|parent| parent.parent() == operation)
        {
            continue;
        }
        let mut imported = 0;
        let mut unreadable = usize::from(failure.is_some());
        for (parent, error, loaded) in &files {
            if parent.parent() != operation {
                continue;
            }
            if loaded {
                imported += 1;
            }
            if let Some(error) = error {
                unreadable += 1;
                bevy::log::warn!(reason = %error.0, "checkout journal was retained without recovery");
            }
        }
        completed.write(CheckoutRecoveryComplete {
            operation,
            imported,
            unreadable,
        });
        let state = if failure.is_some() {
            OperationState::Failed
        } else {
            OperationState::Succeeded
        };
        commands.entity(operation).insert(state);
    }
}
