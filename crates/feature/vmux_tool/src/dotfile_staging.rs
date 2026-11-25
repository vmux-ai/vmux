use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::Context;
use std::time::Duration;

use bevy::prelude::*;
use vmux_ecs::{EntityTarget, Operation, OperationRetained, OperationState, OperationTarget};
use vmux_job::{JobRequest, JobResult};

use crate::dotfiles::{ImportDotfiles, ImportedDotfiles};
use crate::io::{
    BytesCopied, CloseTemporary, CopyFile, CreateDirectory, CreateTemporary, FileIdentity,
    MovePath, ObservePath, PathFacts, PathReceipt, ReadDirectory, TemporaryDirectory,
    TemporaryTransfer,
};
use crate::storage::{
    ManifestOnly, ManifestReceipt, ReadRequest, ReadResult, StoreLease, WriteRequest, WriteResult,
};
use crate::{
    ToolOperationFailed, ToolOperationFinished, ToolOperationSucceeded, ToolStore,
    ToolStoreOperation, ToolStoreTarget, ToolsManifest,
};

pub(crate) struct DotfileImportPlugin;

impl Plugin for DotfileImportPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_job::JobPlugin<CreateTemporary>>() {
            app.add_plugins(vmux_job::JobPlugin::<CreateTemporary>::default());
        }
        if !app.is_plugin_added::<vmux_job::JobPlugin<CloseTemporary>>() {
            app.add_plugins(vmux_job::JobPlugin::<CloseTemporary>::default());
        }
        app.add_systems(
            Update,
            (
                (accept, cancel, cancel_children).chain(),
                (begin, loaded).chain(),
                (observe_roots, roots_observed, enumerate, select_packages).chain(),
                (target_observed, directory_read, file_observed, preflight).chain(),
                (create_staging, staging_created).chain(),
                (copy, directories_created, files_copied, copied).chain(),
                (observe_staged, staged_observed, activate, activated).chain(),
                (save, saved).chain(),
                (
                    rollback,
                    restored_configuration,
                    return_packages,
                    returned_packages,
                )
                    .chain(),
                (close_staging, staging_closed, retry).chain(),
            )
                .chain()
                .in_set(crate::ToolTaskSet::Start),
        )
        .add_systems(PostUpdate, finish.after(vmux_job::JobSet::Cleanup))
        .add_observer(cleanup_failed)
        .add_observer(configuration_failed);
    }
}

#[derive(Component)]
struct Work {
    requester: Entity,
    source: Option<Entity>,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    request: ImportDotfiles,
    retained: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ImportDrain<'w, 's> {
    work: Query<'w, 's, &'static Work>,
}

impl ImportDrain<'_, '_> {
    pub(crate) fn is_drained(&self, requester: Entity) -> bool {
        !self.work.iter().any(|work| work.requester == requester)
    }
}

#[derive(Component)]
struct Accepted;
#[derive(Component)]
struct Loading;
#[derive(Component)]
struct Before(ToolsManifest);
#[derive(Component)]
struct Failure(String);
#[derive(Component)]
struct Observing;
#[derive(Component)]
struct InputRoot;
#[derive(Component)]
struct OutputRoot;
#[derive(Component)]
struct Source(PathBuf);
#[derive(Component)]
struct Destination(PathBuf);
#[derive(Component)]
struct Enumerating;
#[derive(Component)]
struct Enumerated;
#[derive(Component)]
struct Package;
#[derive(Component)]
struct TargetChecked;
#[derive(Component)]
struct Directory(usize);
#[derive(Component)]
struct Relative(PathBuf);
#[derive(Component)]
struct Walked;
#[derive(Component)]
struct File;
#[derive(Component, Default)]
struct EntryCount(usize);
#[derive(Component)]
struct Preflight;
#[derive(Component)]
struct CreatingStaging;
#[derive(Component)]
struct Staging(TemporaryDirectory);
#[derive(Component)]
struct Copying;
#[derive(Component)]
struct CreatingDirectory;
#[derive(Component)]
struct Copied;
#[derive(Component)]
struct CopiesComplete;
#[derive(Component)]
struct ObservingStaged;
#[derive(Component)]
struct StagedPath(PathBuf);
#[derive(Component, Default)]
struct Receipt(PathReceipt);
#[derive(Component)]
struct Activating;
#[derive(Component)]
struct Activated;
#[derive(Component)]
struct Saving;
#[derive(Component)]
struct ConfigurationPublished;
#[derive(Component)]
struct Committed;
#[derive(Component)]
struct RefusedDelivery;
#[derive(Component)]
struct Rollback;
#[derive(Component)]
struct RestoringConfiguration;
#[derive(Component)]
struct ConfigurationRestored;
#[derive(Component)]
struct ConfigurationFailure(String);
#[derive(Component)]
struct Returning;
#[derive(Component)]
struct Returned;
#[derive(Component)]
struct Closing;
#[derive(Component)]
struct Done;
#[derive(Component)]
struct CleanupFailed(String);
#[derive(Component)]
struct RetryDelay(Duration);
#[derive(Component)]
struct RetryAlarm(bevy::tasks::Task<()>);

fn accept(
    requests: Query<
        (
            Entity,
            &ImportDotfiles,
            Option<&ToolStoreTarget>,
            Option<&ToolStore>,
            Option<&OperationState>,
            Has<OperationRetained>,
        ),
        (Without<Accepted>, Without<ToolOperationFinished>),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, request, target, direct, state, retained) in &requests {
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        let store = match target {
            Some(target) => stores.get(target.entity()).ok(),
            None => direct,
        };
        let Some(store) = store else {
            commands.entity(entity).insert((
                ToolOperationFailed("tool store is unavailable".into()),
                ToolOperationFinished,
            ));
            continue;
        };
        commands.spawn((
            Name::new("Import dotfile packages"),
            Operation::bundle("tool.dotfile.import").unwrap(),
            OperationRetained,
            OperationTarget::new(entity),
            Work {
                requester: entity,
                source: target.map(ToolStoreTarget::entity),
                request: request.clone(),
                retained,
            },
            store.clone(),
            EntryCount::default(),
        ));
        commands
            .entity(entity)
            .insert((Accepted, ToolStoreOperation, OperationRetained));
    }
}

fn cancel(
    work: Query<
        (Entity, &Work, &ToolStore, Has<Saving>, Has<Committed>),
        (Without<Failure>, Without<Rollback>),
    >,
    requests: Query<(
        Option<&ImportDotfiles>,
        Option<&ToolStoreTarget>,
        Option<&ToolStore>,
        Option<&OperationState>,
    )>,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, work, store, saving, committed) in &work {
        let valid = requests
            .get(work.requester)
            .is_ok_and(|(request, target, direct, state)| {
                request == Some(&work.request)
                    && state.is_none_or(|state| {
                        matches!(state, OperationState::Pending | OperationState::Running)
                    })
                    && match work.source {
                        Some(source) => {
                            target.is_some_and(|target| target.entity() == source)
                                && stores.get(source).ok() == Some(store)
                        }
                        None => target.is_none() && direct == Some(store),
                    }
            });
        if valid {
            continue;
        }
        if committed {
            commands.entity(entity).insert(RefusedDelivery);
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.insert(Failure("dotfile import source changed".into()));
        if !saving {
            entity.insert(OperationState::Canceled);
        }
    }
}

fn cancel_children(
    work: Query<(Entity, Has<Failure>, Has<Rollback>)>,
    mut children: Query<(&ChildOf, &mut OperationState), Without<Work>>,
) {
    for (entity, failed, rollback) in &work {
        if !failed || rollback {
            continue;
        }
        for (parent, mut state) in &mut children {
            if parent.parent() == entity {
                *state = OperationState::Canceled;
            }
        }
    }
}

fn begin(
    work: Query<
        (Entity, &Work, &ToolStore, &OperationState),
        (Without<Before>, Without<Loading>, Without<Failure>),
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, work, store, state) in &work {
        if *state != OperationState::Pending {
            continue;
        }
        if let Some(wake) = &wake {
            store.bind_waker(wake.0.clone());
        }
        let Some(lease) = store.lease() else {
            continue;
        };
        commands.entity(entity).insert((
            Loading,
            StoreLease { _lease: lease },
            ReadRequest { retain_lease: true },
            OperationState::Running,
        ));
        if let Ok(mut requester) = commands.get_entity(work.requester) {
            requester.insert(OperationState::Running);
        }
    }
}

fn loaded(work: Query<(Entity, &ReadResult), With<Loading>>, mut commands: Commands) {
    for (entity, result) in &work {
        commands.entity(entity).remove::<(Loading, ReadResult)>();
        match &result.0 {
            Ok(manifest) => {
                commands.entity(entity).insert(Before(manifest.clone()));
            }
            Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
            }
        }
    }
}

fn observe_roots(
    work: Query<
        (Entity, &Work, &ToolStore, &OperationState),
        (With<Before>, Without<Observing>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, work, store, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let path = match work.request.path.strip_prefix("~") {
            Ok(relative) => store.home().join(relative),
            Err(_) => work.request.path.clone(),
        };
        commands.spawn((
            Name::new("Import source"),
            Operation::bundle("tool.import.input").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            InputRoot,
            JobRequest(ObservePath { path }),
        ));
        commands.spawn((
            Name::new("Import destination"),
            Operation::bundle("tool.import.output").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            OutputRoot,
            JobRequest(ObservePath {
                path: store.dotfiles_dir(),
            }),
        ));
        commands.entity(entity).insert(Observing);
    }
}

fn roots_observed(
    paths: Query<
        (Entity, &ChildOf, &JobResult<ObservePath>),
        Or<(With<InputRoot>, With<OutputRoot>)>,
    >,
    mut commands: Commands,
) {
    for (entity, parent, result) in &paths {
        commands.entity(entity).remove::<JobResult<ObservePath>>();
        match &result.0 {
            Ok(Ok(facts)) => {
                commands.entity(entity).insert(facts.clone());
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(parent.parent())
                    .insert(Failure(error.clone()));
            }
        }
    }
}

fn enumerate(
    work: Query<
        (Entity, &OperationState),
        (
            With<Observing>,
            Without<Enumerating>,
            Without<Enumerated>,
            Without<Failure>,
        ),
    >,
    roots: Query<(&ChildOf, &PathFacts, Has<InputRoot>, Has<OutputRoot>)>,
    mut commands: Commands,
) {
    for (entity, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let mut source = None;
        let mut destination = None;
        for (parent, facts, input, output) in &roots {
            if parent.parent() != entity {
                continue;
            }
            if input {
                source = Some(&facts.path);
            }
            if output {
                destination = Some(&facts.path);
            }
        }
        let (Some(source), Some(destination)) = (source, destination) else {
            continue;
        };
        if source.starts_with(destination) || destination.starts_with(source) {
            commands
                .entity(entity)
                .insert(Failure("dotfile import overlaps the Tools root".into()));
            continue;
        }
        commands.entity(entity).insert((
            Source(source.clone()),
            Destination(destination.clone()),
            Enumerating,
            JobRequest(ReadDirectory {
                path: source.clone(),
                limit: 4096,
            }),
        ));
    }
}

fn select_packages(
    work: Query<
        (
            Entity,
            &OperationState,
            &Destination,
            &JobResult<ReadDirectory>,
        ),
        (With<Enumerating>, Without<Enumerated>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, state, destination, result) in &work {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(JobResult<ReadDirectory>, Enumerating)>();
        let entries = match &result.0 {
            Ok(Ok(entries)) if entries.exists && entries.is_directory => entries,
            Ok(Ok(_)) => {
                commands
                    .entity(entity)
                    .insert(Failure("import source is no longer a directory".into()));
                continue;
            }
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
                continue;
            }
        };
        let mut count = 0;
        for entry in &entries.entries {
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            if !entry.file_type.is_dir() || !Package::valid(name) {
                continue;
            }
            count += 1;
            let target = destination.0.join(name);
            let package = commands
                .spawn((
                    Name::new(name.to_string()),
                    Package,
                    Operation::bundle("tool.import.package").unwrap().running(),
                    OperationRetained,
                    ChildOf(entity),
                    Source(entry.path.clone()),
                    Destination(target.clone()),
                    Receipt::default(),
                    JobRequest(ObservePath { path: target }),
                ))
                .id();
            commands.spawn((
                Name::new("Read package directory"),
                Operation::bundle("tool.import.directory")
                    .unwrap()
                    .running(),
                OperationRetained,
                ChildOf(entity),
                EntityTarget::<Package>::new(package),
                Directory(0),
                Source(entry.path.clone()),
                Relative(PathBuf::new()),
                JobRequest(ReadDirectory {
                    path: entry.path.clone(),
                    limit: 4096,
                }),
            ));
        }
        if count == 0 {
            commands
                .entity(entity)
                .insert(Failure("no Stow packages found".into()));
        }
        commands.entity(entity).insert(Enumerated);
    }
}

impl Package {
    fn valid(name: &str) -> bool {
        !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\'])
    }
}

fn target_observed(
    packages: Query<
        (Entity, &ChildOf, &JobResult<ObservePath>),
        (With<Package>, Without<TargetChecked>),
    >,
    mut commands: Commands,
) {
    for (entity, owner, result) in &packages {
        commands
            .entity(entity)
            .remove::<JobResult<ObservePath>>()
            .insert(TargetChecked);
        match &result.0 {
            Ok(Ok(facts)) if facts.metadata.is_none() => {}
            Ok(Ok(_)) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure("Tools dotfile package already exists".into()));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
            }
        }
    }
}

fn directory_read(
    directories: Query<
        (
            Entity,
            &ChildOf,
            &EntityTarget<Package>,
            &Directory,
            &JobResult<ReadDirectory>,
        ),
        Without<Walked>,
    >,
    packages: Query<&Source, With<Package>>,
    mut work: Query<(&OperationState, &mut EntryCount)>,
    mut commands: Commands,
) {
    for (entity, owner, package, depth, result) in &directories {
        commands
            .entity(entity)
            .remove::<JobResult<ReadDirectory>>()
            .insert(Walked);
        let Ok((state, mut count)) = work.get_mut(owner.parent()) else {
            continue;
        };
        if *state != OperationState::Running {
            continue;
        }
        let entries = match &result.0 {
            Ok(Ok(entries)) if entries.exists && entries.is_directory => entries,
            Ok(Ok(_)) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure("import source directory disappeared".into()));
                continue;
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
                continue;
            }
        };
        let Ok(source) = packages.get(package.entity()) else {
            continue;
        };
        for entry in &entries.entries {
            count.0 += 1;
            if count.0 > 100_000 || depth.0 >= 128 {
                commands.entity(owner.parent()).insert(Failure(
                    "dotfile tree exceeds its entry or depth bound".into(),
                ));
                break;
            }
            let Ok(relative) = entry.path.strip_prefix(&source.0) else {
                continue;
            };
            if entry.file_type.is_dir() {
                commands.spawn((
                    Name::new("Read nested dotfile directory"),
                    Operation::bundle("tool.import.directory")
                        .unwrap()
                        .running(),
                    OperationRetained,
                    ChildOf(owner.parent()),
                    EntityTarget::<Package>::new(package.entity()),
                    Directory(depth.0 + 1),
                    Source(entry.path.clone()),
                    Relative(relative.to_path_buf()),
                    JobRequest(ReadDirectory {
                        path: entry.path.clone(),
                        limit: 4096,
                    }),
                ));
            } else if entry.file_type.is_file() {
                commands.spawn((
                    Name::new("Observe source file"),
                    Operation::bundle("tool.import.file").unwrap().running(),
                    OperationRetained,
                    ChildOf(owner.parent()),
                    EntityTarget::<Package>::new(package.entity()),
                    File,
                    Source(entry.path.clone()),
                    Relative(relative.to_path_buf()),
                    JobRequest(ObservePath {
                        path: entry.path.clone(),
                    }),
                ));
            } else {
                commands
                    .entity(owner.parent())
                    .insert(Failure("unsupported entry in dotfile package".into()));
                break;
            }
        }
    }
}

fn file_observed(
    files: Query<(Entity, &ChildOf, &JobResult<ObservePath>), With<File>>,
    mut commands: Commands,
) {
    for (entity, parent, result) in &files {
        commands.entity(entity).remove::<JobResult<ObservePath>>();
        let metadata = match &result.0 {
            Ok(Ok(facts)) => facts
                .metadata
                .as_ref()
                .filter(|metadata| metadata.is_file()),
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(parent.parent())
                    .insert(Failure(error.clone()));
                continue;
            }
        };
        let Some(metadata) = metadata else {
            commands
                .entity(parent.parent())
                .insert(Failure("import file is no longer regular".into()));
            continue;
        };
        match FileIdentity::from_metadata(metadata) {
            Ok(identity) => {
                commands.entity(entity).insert(identity);
            }
            Err(error) => {
                commands.entity(parent.parent()).insert(Failure(error));
            }
        }
    }
}

fn preflight(
    work: Query<
        (Entity, &OperationState),
        (With<Enumerated>, Without<Preflight>, Without<Failure>),
    >,
    packages: Query<(&ChildOf, Has<TargetChecked>), With<Package>>,
    directories: Query<(&ChildOf, Has<Walked>), With<Directory>>,
    files: Query<(&ChildOf, Has<FileIdentity>), With<File>>,
    mut commands: Commands,
) {
    for (entity, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        if packages
            .iter()
            .any(|(owner, done)| owner.parent() == entity && !done)
            || directories
                .iter()
                .any(|(owner, done)| owner.parent() == entity && !done)
            || files
                .iter()
                .any(|(owner, done)| owner.parent() == entity && !done)
        {
            continue;
        }
        commands.entity(entity).insert(Preflight);
    }
}

fn create_staging(
    work: Query<
        (Entity, &Destination, &OperationState),
        (
            With<Preflight>,
            Without<CreatingStaging>,
            Without<Staging>,
            Without<Failure>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, destination, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let transfer = TemporaryTransfer::default();
        commands.entity(entity).insert((
            CreatingStaging,
            transfer.clone(),
            JobRequest(CreateTemporary {
                root: destination.0.clone(),
                transfer,
            }),
        ));
    }
}

fn staging_created(
    work: Query<
        (
            Entity,
            &TemporaryTransfer,
            Option<&JobResult<CreateTemporary>>,
            Has<JobRequest<CreateTemporary>>,
        ),
        With<CreatingStaging>,
    >,
    captures: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, transfer, result, requested) in &work {
        if requested || captures.iter().any(|target| target.entity() == entity) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(CreatingStaging, JobResult<CreateTemporary>)>();
        if let Some(staging) = transfer.take() {
            commands.entity(entity).insert(Staging(staging));
        }
        match result.map(|result| &result.0) {
            Some(Ok(Ok(()))) => {}
            Some(Ok(Err(error))) | Some(Err(error)) => {
                commands.entity(entity).insert(Failure(error.clone()));
            }
            None => {
                commands
                    .entity(entity)
                    .insert(Failure("staging creation did not complete".into()));
            }
        }
    }
}

fn copy(
    work: Query<(Entity, &Staging, &OperationState), (Without<CopiesComplete>, Without<Failure>)>,
    entries: Query<(
        Entity,
        &ChildOf,
        &EntityTarget<Package>,
        &Source,
        &Relative,
        Option<&FileIdentity>,
        Has<Directory>,
        Has<CreatingDirectory>,
        Has<Copying>,
        Has<Copied>,
    )>,
    packages: Query<&Name, With<Package>>,
    mut commands: Commands,
) {
    for (entity, staging, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let mut next = None;
        let mut active = false;
        for (entry, owner, _, source, _, _, directory, creating, copying, copied) in &entries {
            if owner.parent() != entity || copied {
                continue;
            }
            if creating || copying {
                active = true;
                break;
            }
            let key = (!directory, source.0.clone());
            if next
                .as_ref()
                .is_none_or(|(_, previous): &(Entity, (bool, PathBuf))| key < *previous)
            {
                next = Some((entry, key));
            }
        }
        if active {
            continue;
        }
        let Some((entry, _)) = next else {
            continue;
        };
        let (_, _, package, source, relative, identity, directory, ..) =
            entries.get(entry).unwrap();
        let Ok(name) = packages.get(package.entity()) else {
            continue;
        };
        let target = staging.0.path().join(name.as_str()).join(&relative.0);
        if directory {
            commands.entity(entry).insert((
                CreatingDirectory,
                JobRequest(CreateDirectory { path: target }),
            ));
        } else if let Some(identity) = identity {
            commands.entity(entry).insert((
                Copying,
                JobRequest(CopyFile {
                    source: source.0.clone(),
                    target,
                    expected: identity.clone(),
                }),
            ));
        }
    }
}

fn directories_created(
    entries: Query<(Entity, &ChildOf, &JobResult<CreateDirectory>), With<CreatingDirectory>>,
    mut commands: Commands,
) {
    for (entity, owner, result) in &entries {
        commands
            .entity(entity)
            .remove::<(CreatingDirectory, JobResult<CreateDirectory>)>();
        match &result.0 {
            Ok(Ok(created)) => {
                commands.entity(entity).insert((Copied, created.clone()));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
            }
        }
    }
}

fn files_copied(
    entries: Query<(Entity, &ChildOf, &JobResult<CopyFile>), With<Copying>>,
    mut commands: Commands,
) {
    for (entity, owner, result) in &entries {
        commands
            .entity(entity)
            .remove::<(Copying, JobResult<CopyFile>)>();
        match &result.0 {
            Ok(Ok(bytes)) => {
                commands
                    .entity(entity)
                    .insert((Copied, BytesCopied(bytes.0)));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
            }
        }
    }
}

fn copied(
    work: Query<
        (Entity, &OperationState),
        (With<Staging>, Without<CopiesComplete>, Without<Failure>),
    >,
    entries: Query<(&ChildOf, Has<Copied>), Or<(With<File>, With<Directory>)>>,
    mut commands: Commands,
) {
    for (entity, state) in &work {
        if *state != OperationState::Running
            || entries
                .iter()
                .any(|(owner, copied)| owner.parent() == entity && !copied)
        {
            continue;
        }
        commands.entity(entity).insert(CopiesComplete);
    }
}

fn observe_staged(
    work: Query<(Entity, &Staging, &OperationState), (With<CopiesComplete>, Without<Failure>)>,
    packages: Query<
        (Entity, &ChildOf, &Name),
        (
            With<Package>,
            Without<ObservingStaged>,
            Without<FileIdentity>,
        ),
    >,
    mut commands: Commands,
) {
    for (owner, staging, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        for (entity, parent, name) in &packages {
            if parent.parent() != owner {
                continue;
            }
            let path = staging.0.path().join(name.as_str());
            commands.entity(entity).insert((
                ObservingStaged,
                StagedPath(path.clone()),
                JobRequest(ObservePath { path }),
            ));
        }
    }
}

fn staged_observed(
    packages: Query<(Entity, &ChildOf, &JobResult<ObservePath>), With<ObservingStaged>>,
    mut commands: Commands,
) {
    for (entity, owner, result) in &packages {
        commands
            .entity(entity)
            .remove::<(ObservingStaged, JobResult<ObservePath>)>();
        let metadata = match &result.0 {
            Ok(Ok(facts)) => facts.metadata.as_ref().filter(|metadata| metadata.is_dir()),
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
                continue;
            }
        };
        let Some(metadata) = metadata else {
            commands
                .entity(owner.parent())
                .insert(Failure("staged package directory is unavailable".into()));
            continue;
        };
        match FileIdentity::from_metadata(metadata) {
            Ok(identity) => {
                commands.entity(entity).insert(identity);
            }
            Err(error) => {
                commands.entity(owner.parent()).insert(Failure(error));
            }
        }
    }
}

fn activate(
    work: Query<(Entity, &OperationState), (With<CopiesComplete>, Without<Failure>)>,
    packages: Query<(
        Entity,
        &ChildOf,
        &Name,
        &StagedPath,
        &Destination,
        &FileIdentity,
        &Receipt,
        Has<Activating>,
        Has<Activated>,
    )>,
    mut commands: Commands,
) {
    for (owner, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let mut next = None;
        let mut pending = false;
        for (entity, parent, name, _, _, _, _, activating, activated) in &packages {
            if parent.parent() != owner || activated {
                continue;
            }
            if activating {
                pending = true;
                break;
            }
            if next
                .as_ref()
                .is_none_or(|(_, old): &(Entity, String)| name.as_str() < old.as_str())
            {
                next = Some((entity, name.as_str().to_string()));
            }
        }
        if pending {
            continue;
        }
        let Some((entity, _)) = next else {
            continue;
        };
        let (_, _, _, staged, target, identity, receipt, ..) = packages.get(entity).unwrap();
        commands.entity(entity).insert((
            Activating,
            JobRequest(MovePath {
                source: staged.0.clone(),
                target: target.0.clone(),
                expected: identity.clone(),
                receipt: receipt.0.clone(),
            }),
        ));
    }
}

fn activated(
    packages: Query<
        (
            Entity,
            &ChildOf,
            &Receipt,
            Option<&JobResult<MovePath>>,
            Has<JobRequest<MovePath>>,
        ),
        With<Activating>,
    >,
    captures: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, parent, receipt, result, requested) in &packages {
        if requested || captures.iter().any(|target| target.entity() == entity) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(Activating, JobResult<MovePath>)>();
        if receipt.0.changed() {
            commands.entity(entity).insert(Activated);
        }
        match result.map(|result| &result.0) {
            Some(Ok(Ok(_))) => {}
            Some(Ok(Err(error))) | Some(Err(error)) => {
                commands
                    .entity(parent.parent())
                    .insert(Failure(error.clone()));
            }
            None => {
                commands
                    .entity(parent.parent())
                    .insert(Failure("package activation result is unavailable".into()));
            }
        }
    }
}

fn save(
    work: Query<
        (Entity, &Before, &OperationState),
        (
            With<CopiesComplete>,
            Without<Saving>,
            Without<Committed>,
            Without<Failure>,
        ),
    >,
    packages: Query<(&ChildOf, &Name, Has<Activated>), With<Package>>,
    mut commands: Commands,
) {
    for (entity, before, state) in &work {
        if *state != OperationState::Running
            || packages
                .iter()
                .any(|(owner, _, active)| owner.parent() == entity && !active)
        {
            continue;
        }
        let mut manifest = before.0.clone();
        for (owner, name, _) in &packages {
            if owner.parent() == entity {
                manifest.set_dotfile_package(name.as_str(), true);
            }
        }
        commands.entity(entity).insert((
            Saving,
            ManifestOnly,
            WriteRequest {
                manifest,
                retain_lease: true,
            },
        ));
    }
}

fn saved(
    work: Query<(Entity, &WriteResult, Option<&ManifestReceipt>, Has<Failure>), With<Saving>>,
    mut commands: Commands,
) {
    for (entity, result, receipt, failed) in &work {
        commands.entity(entity).remove::<(Saving, WriteResult)>();
        if receipt.is_some_and(ManifestReceipt::published) {
            commands.entity(entity).insert(ConfigurationPublished);
        }
        match &result.0 {
            Ok(()) if !failed => {
                commands.entity(entity).insert(Committed);
            }
            Ok(()) => {}
            Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
            }
        }
    }
}

fn rollback(
    work: Query<
        (
            Entity,
            Option<&Before>,
            Option<&Children>,
            Has<ConfigurationPublished>,
            Has<ConfigurationRestored>,
            Has<RestoringConfiguration>,
            Has<ReadRequest>,
            Has<WriteRequest>,
            Has<ConfigurationFailure>,
        ),
        (With<Failure>, Without<CleanupFailed>),
    >,
    captures: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, before, children, started, restored, restoring, reading, writing, failed) in &work
    {
        if reading
            || writing
            || captures.iter().any(|target| {
                target.entity() == entity
                    || children.is_some_and(|children| children.contains(&target.entity()))
            })
        {
            continue;
        }
        commands
            .entity(entity)
            .insert((Rollback, OperationState::Running));
        if !started {
            commands.entity(entity).insert(ConfigurationRestored);
        } else if !restored
            && !restoring
            && !failed
            && let Some(before) = before
        {
            commands.entity(entity).insert((
                RestoringConfiguration,
                ManifestOnly,
                WriteRequest {
                    manifest: before.0.clone(),
                    retain_lease: true,
                },
            ));
        }
    }
}

fn restored_configuration(
    work: Query<(Entity, &WriteResult), With<RestoringConfiguration>>,
    mut commands: Commands,
) {
    for (entity, result) in &work {
        commands
            .entity(entity)
            .remove::<(RestoringConfiguration, WriteResult)>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(ConfigurationRestored);
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(ConfigurationFailure(error.clone()));
            }
        }
    }
}

fn return_packages(
    work: Query<
        (
            Entity,
            Has<ConfigurationRestored>,
            Has<ConfigurationFailure>,
        ),
        (
            With<Failure>,
            Without<CleanupFailed>,
            Without<Saving>,
            Without<RestoringConfiguration>,
            Without<ReadRequest>,
            Without<WriteRequest>,
        ),
    >,
    packages: Query<
        (
            Entity,
            &ChildOf,
            &Name,
            &StagedPath,
            &Destination,
            &FileIdentity,
            Has<Returning>,
            Has<Returned>,
        ),
        With<Activated>,
    >,
    mut commands: Commands,
) {
    for (owner, restored, failed) in &work {
        if !restored && !failed {
            continue;
        }
        let mut next = None;
        let mut pending = false;
        for (entity, parent, name, _, _, _, returning, returned) in &packages {
            if parent.parent() != owner || returned {
                continue;
            }
            if returning {
                pending = true;
                break;
            }
            if next
                .as_ref()
                .is_none_or(|(_, old): &(Entity, String)| name.as_str() > old.as_str())
            {
                next = Some((entity, name.as_str().to_string()));
            }
        }
        if pending {
            continue;
        }
        let Some((entity, _)) = next else {
            continue;
        };
        let (_, _, _, staged, destination, identity, ..) = packages.get(entity).unwrap();
        commands.entity(entity).insert((
            Returning,
            OperationState::Running,
            JobRequest(MovePath {
                source: destination.0.clone(),
                target: staged.0.clone(),
                expected: identity.clone(),
                receipt: PathReceipt::default(),
            }),
        ));
    }
}

fn returned_packages(
    packages: Query<(Entity, &ChildOf, &JobResult<MovePath>), With<Returning>>,
    mut commands: Commands,
) {
    for (entity, parent, result) in &packages {
        commands
            .entity(entity)
            .remove::<(Returning, JobResult<MovePath>)>();
        match &result.0 {
            Ok(Ok(_)) => {
                commands.entity(entity).insert(Returned);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(parent.parent())
                    .insert(CleanupFailed(error.clone()));
            }
        }
    }
}

fn close_staging(
    work: Query<
        (
            Entity,
            Option<&Staging>,
            Option<&Children>,
            Has<Committed>,
            Has<Failure>,
            Has<ConfigurationRestored>,
            Has<ReadRequest>,
            Has<WriteRequest>,
            Has<CreatingStaging>,
        ),
        (Without<Closing>, Without<Done>, Without<CleanupFailed>),
    >,
    packages: Query<(&ChildOf, Has<Activated>, Has<Returned>), With<Package>>,
    captures: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, staging, children, committed, failed, restored, reading, writing, creating) in
        &work
    {
        if !committed && (!failed || !restored) {
            continue;
        }
        if reading
            || writing
            || creating
            || captures.iter().any(|target| {
                target.entity() == entity
                    || children.is_some_and(|children| children.contains(&target.entity()))
            })
        {
            continue;
        }
        if failed
            && packages
                .iter()
                .any(|(owner, active, returned)| owner.parent() == entity && active && !returned)
        {
            continue;
        }
        if let Some(staging) = staging {
            commands.entity(entity).insert((
                Closing,
                JobRequest(CloseTemporary {
                    staging: staging.0.clone(),
                }),
            ));
        } else {
            commands.entity(entity).insert(Done);
        }
    }
}

fn staging_closed(
    work: Query<(Entity, &JobResult<CloseTemporary>), With<Closing>>,
    mut commands: Commands,
) {
    for (entity, result) in &work {
        commands
            .entity(entity)
            .remove::<(Closing, JobResult<CloseTemporary>)>();
        match &result.0 {
            Ok(Ok(())) => {
                commands.entity(entity).insert(Done);
            }
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(CleanupFailed(error.clone()));
            }
        }
    }
}

fn retry(
    failed: Query<
        (Entity, Option<&RetryDelay>),
        (
            With<Work>,
            Or<(With<CleanupFailed>, With<ConfigurationFailure>)>,
            Without<RetryAlarm>,
        ),
    >,
    mut alarms: Query<(Entity, &mut RetryAlarm, Option<&RetryDelay>)>,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let waker = wake
        .as_deref()
        .map(|wake| &wake.0)
        .unwrap_or(std::task::Waker::noop());
    let mut context = Context::from_waker(waker);
    for (entity, mut alarm, delay) in &mut alarms {
        if Pin::new(&mut alarm.0).poll(&mut context).is_pending() {
            continue;
        }
        let next = delay
            .map(|delay| delay.0.saturating_mul(2))
            .unwrap_or(Duration::from_millis(200))
            .min(Duration::from_secs(2));
        commands
            .entity(entity)
            .remove::<(RetryAlarm, CleanupFailed, ConfigurationFailure)>()
            .insert(RetryDelay(next));
    }
    for (entity, delay) in &failed {
        let delay = delay
            .map(|delay| delay.0)
            .unwrap_or(Duration::from_millis(100));
        let waker = waker.clone();
        commands
            .entity(entity)
            .insert(RetryAlarm(bevy::tasks::IoTaskPool::get().spawn(
                async move {
                    async_io::Timer::after(delay).await;
                    waker.wake();
                },
            )));
    }
}

fn cleanup_failed(
    trigger: On<Add, CleanupFailed>,
    work: Query<(&Work, &CleanupFailed)>,
    mut commands: Commands,
) {
    let Ok((work, failure)) = work.get(trigger.event_target()) else {
        return;
    };
    if let Ok(mut requester) = commands.get_entity(work.requester) {
        requester.insert((
            ToolOperationFailed(format!("dotfile import cleanup failed: {}", failure.0)),
            ToolOperationFinished,
            OperationState::Failed,
        ));
    }
}

fn configuration_failed(
    trigger: On<Add, ConfigurationFailure>,
    work: Query<(&Work, &ConfigurationFailure)>,
    mut commands: Commands,
) {
    let Ok((work, failure)) = work.get(trigger.event_target()) else {
        return;
    };
    if let Ok(mut requester) = commands.get_entity(work.requester) {
        requester.insert((
            ToolOperationFailed(format!(
                "dotfile import configuration recovery failed: {}",
                failure.0
            )),
            ToolOperationFinished,
            OperationState::Failed,
        ));
    }
}

fn finish(
    work: Query<
        (
            Entity,
            &Work,
            Option<&Children>,
            Option<&Failure>,
            Has<RefusedDelivery>,
        ),
        (With<Done>, Without<CleanupFailed>),
    >,
    packages: Query<&ChildOf, With<Package>>,
    captures: Query<&OperationTarget, With<vmux_job::JobCapture>>,
    mut commands: Commands,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    for (entity, work, children, failure, refused) in &work {
        if captures.iter().any(|target| {
            target.entity() == entity
                || children.is_some_and(|children| children.contains(&target.entity()))
        }) {
            continue;
        }
        if let Ok(mut requester) = commands.get_entity(work.requester) {
            if let Some(failure) = failure {
                requester.insert((
                    ToolOperationFailed(failure.0.clone()),
                    ToolOperationFinished,
                ));
            } else if refused {
                requester.insert((
                    ToolOperationFailed("import result owner changed after commit".into()),
                    ToolOperationFinished,
                ));
            } else {
                let count = packages
                    .iter()
                    .filter(|owner| owner.parent() == entity)
                    .count();
                requester.insert((
                    ImportedDotfiles { packages: count },
                    ToolOperationSucceeded(format!("imported {count} dotfile package(s)")),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            }
            if !work.retained {
                requester.remove::<OperationRetained>();
            }
        }
        commands.entity(entity).despawn();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use vmux_ecs::cli::{CliInvocation, CliResult};

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        store: ToolStore,
        source: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = ToolStore::new(
                directory.path().join("config"),
                directory.path().join("home"),
            );
            ToolsManifest::default()
                .write_to(&store.manifest_path())
                .unwrap();
            let mut app = App::new();
            app.add_plugins((
                bevy::app::TaskPoolPlugin::default(),
                crate::cli::ToolCliPlugin,
            ));
            let source = app.world_mut().spawn(store.clone()).id();
            Self {
                app,
                directory,
                store,
                source,
            }
        }

        fn input(&self) -> PathBuf {
            let input = self.directory.path().join("input");
            std::fs::create_dir_all(input.join("shell/.config/empty")).unwrap();
            std::fs::write(input.join("shell/.config/editor"), [0, 1, 255, 13, 10]).unwrap();
            std::fs::write(input.join("shell/.zshrc"), b"shell").unwrap();
            std::fs::create_dir_all(input.join("editor")).unwrap();
            std::fs::write(input.join("editor/.editor"), b"editor").unwrap();
            input
        }

        fn import(&mut self, path: PathBuf) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("test.physical.import").unwrap(),
                    ToolStoreTarget::new(self.source),
                    ImportDotfiles::new(path),
                ))
                .id()
        }

        fn finish(&mut self, request: Entity) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.app.update();
                let pending = self
                    .app
                    .world_mut()
                    .query::<&Work>()
                    .iter(self.app.world())
                    .any(|work| work.requester == request);
                if !pending
                    && (self.app.world().get_entity(request).is_err()
                        || self
                            .app
                            .world()
                            .get::<ToolOperationFinished>(request)
                            .is_some())
                {
                    return;
                }
                assert!(Instant::now() < deadline, "physical import did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn physical_import_copies_all_packages_before_committing_and_cleans_staging() {
        let mut fixture = Fixture::new();
        let brewfile = fixture.store.brewfile_path();
        let expected = b"# preserve comments\nbrew \"kept\", link: false";
        let mut manifest = ToolsManifest::default();
        manifest.set_package("homebrew-formula", "kept", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        std::fs::write(&brewfile, expected).unwrap();
        let input = fixture.input();
        let request = fixture.import(input.clone());
        fixture.finish(request);
        assert_eq!(std::fs::read(&brewfile).unwrap(), expected);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(request)
                .is_some(),
            "{:?}",
            fixture.app.world().get::<ToolOperationFailed>(request)
        );
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ImportedDotfiles>(request)
                .unwrap()
                .packages,
            2
        );
        assert_eq!(
            std::fs::read(fixture.store.dotfiles_dir().join("shell/.config/editor")).unwrap(),
            [0, 1, 255, 13, 10]
        );
        assert!(
            fixture
                .store
                .dotfiles_dir()
                .join("shell/.config/empty")
                .is_dir()
        );
        assert_eq!(std::fs::read(input.join("shell/.zshrc")).unwrap(), b"shell");
        assert_eq!(
            ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .dotfiles
                .packages,
            ["editor", "shell"]
        );
        let mut names: Vec<_> = std::fs::read_dir(fixture.store.dotfiles_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["editor", "shell"]);
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn cancellation_after_activation_rolls_back_packages_and_releases_the_store() {
        for interruption in ["cancel", "store", "requester"] {
            let mut fixture = Fixture::new();
            let input = fixture.input();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let request = fixture.import(input.clone());
            let deadline = Instant::now() + Duration::from_secs(10);
            while !fixture.store.dotfiles_dir().join("editor/.editor").exists() {
                fixture.app.update();
                assert!(Instant::now() < deadline, "package did not activate");
                std::thread::sleep(Duration::from_millis(1));
            }
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(request)
                        .insert(OperationState::Canceled);
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.source)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.store.home(),
                        ));
                }
                "requester" => {
                    fixture.app.world_mut().despawn(request);
                }
                _ => unreachable!(),
            }
            fixture.finish(request);
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before,
                "{interruption}"
            );
            assert_eq!(
                std::fs::read(input.join("editor/.editor")).unwrap(),
                b"editor"
            );
            assert!(
                std::fs::read_dir(fixture.store.dotfiles_dir())
                    .unwrap()
                    .next()
                    .is_none(),
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some());
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ImportedDotfiles>(request)
                    .is_none()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unsupported_tree_entries_refuse_before_staging_or_persistence() {
        let mut fixture = Fixture::new();
        let input = fixture.input();
        std::os::unix::fs::symlink(
            input.join("editor/.editor"),
            input.join("shell/unsupported"),
        )
        .unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture.import(input);
        fixture.finish(request);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(request)
                .is_some()
        );
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(!fixture.store.dotfiles_dir().exists());
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn physical_import_refuses_occupied_targets_without_partial_copy_or_configuration() {
        let mut fixture = Fixture::new();
        let input = fixture.input();
        let foreign = fixture.store.dotfiles_dir().join("shell");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join("keep"), b"foreign").unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture.import(input);
        fixture.finish(request);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(request)
                .is_some()
        );
        assert_eq!(std::fs::read(foreign.join("keep")).unwrap(), b"foreign");
        assert!(!fixture.store.dotfiles_dir().join("editor").exists());
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn queued_import_cancellation_drains_without_copying_or_saving() {
        let mut fixture = Fixture::new();
        let input = fixture.input();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let lease = fixture.store.lease().unwrap();
        let request = fixture.import(input);
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .entity_mut(request)
            .insert(OperationState::Canceled);
        fixture.finish(request);
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(!fixture.store.dotfiles_dir().exists());
        assert!(
            fixture
                .app
                .world()
                .get::<ImportedDotfiles>(request)
                .is_none()
        );
        drop(lease);
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn failed_save_before_publication_returns_packages_without_configuration_retry() {
        let mut fixture = Fixture::new();
        let input = fixture.input();
        let manifest = fixture.store.manifest_path();
        let blocked = manifest.clone();
        fixture.app.add_observer(move |_: On<Add, Saving>| {
            std::fs::remove_file(&blocked).unwrap();
            std::fs::create_dir(&blocked).unwrap();
        });
        let request = fixture.import(input.clone());
        fixture.finish(request);
        assert!(manifest.is_dir());
        assert!(!fixture.store.dotfiles_dir().join("editor").exists());
        assert!(!fixture.store.dotfiles_dir().join("shell").exists());
        assert_eq!(
            std::fs::read(input.join("editor/.editor")).unwrap(),
            b"editor"
        );
        assert_eq!(std::fs::read(input.join("shell/.zshrc")).unwrap(), b"shell");
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(request)
                .is_some()
        );
        assert!(
            fixture
                .app
                .world()
                .get::<ImportedDotfiles>(request)
                .is_none()
        );
        assert!(fixture.store.lease().is_some());
        assert!(
            std::fs::read_dir(fixture.store.root())
                .unwrap()
                .all(|entry| {
                    !entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".vmux-")
                })
        );
    }

    #[test]
    fn cli_waits_for_published_configuration_recovery_after_packages_return() {
        let mut fixture = Fixture::new();
        let input = fixture.input();
        let manifest = fixture.store.manifest_path();
        let before = ToolsManifest::read(&manifest).unwrap();
        fixture.app.add_observer(
            |trigger: On<Add, Saving>, work: Query<&Work>, mut commands: Commands| {
                commands
                    .entity(work.get(trigger.event_target()).unwrap().requester)
                    .insert(OperationState::Canceled);
            },
        );
        let blocked = manifest.clone();
        fixture
            .app
            .add_observer(move |_: On<Add, ConfigurationPublished>| {
                std::fs::remove_file(&blocked).unwrap();
                std::fs::create_dir(&blocked).unwrap();
            });
        let request = fixture
            .app
            .world_mut()
            .spawn((
                fixture.store.clone(),
                CliInvocation {
                    command: "tool.import.dotfiles".into(),
                    arguments: std::collections::BTreeMap::from([(
                        "path".into(),
                        vec![input.clone().into_os_string()],
                    )]),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            fixture.app.update();
            if manifest.is_dir()
                && !fixture.store.dotfiles_dir().join("editor").exists()
                && !fixture.store.dotfiles_dir().join("shell").exists()
                && fixture
                    .app
                    .world()
                    .get::<ToolOperationFailed>(request)
                    .is_some()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "blocked configuration prevented package return"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(fixture.app.world().get::<CliResult>(request).is_none());
        assert!(fixture.store.lease().is_none());
        assert_eq!(
            std::fs::read(input.join("editor/.editor")).unwrap(),
            b"editor"
        );
        std::fs::remove_dir(&manifest).unwrap();
        fixture.finish(request);
        while fixture.app.world().get::<CliResult>(request).is_none() {
            fixture.app.update();
            assert!(
                Instant::now() < deadline,
                "CLI did not complete after recovery"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            fixture
                .app
                .world()
                .get::<CliResult>(request)
                .unwrap()
                .0
                .is_err()
        );
        assert_eq!(ToolsManifest::read(&manifest).unwrap(), before);
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn owner_changes_after_commit_refuse_delivery_and_preserve_the_committed_import() {
        for interruption in ["cancel", "store", "requester"] {
            let mut fixture = Fixture::new();
            let input = fixture.input();
            let request = fixture.import(input);
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                fixture.app.update();
                if fixture
                    .app
                    .world_mut()
                    .query_filtered::<Entity, With<Committed>>()
                    .iter(fixture.app.world())
                    .next()
                    .is_some()
                {
                    break;
                }
                assert!(Instant::now() < deadline, "import did not commit");
                std::thread::sleep(Duration::from_millis(1));
            }
            match interruption {
                "cancel" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(request)
                        .insert(OperationState::Canceled);
                }
                "store" => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.source)
                        .insert(ToolStore::new(
                            fixture.directory.path().join("replacement"),
                            fixture.store.home(),
                        ));
                }
                "requester" => {
                    fixture.app.world_mut().despawn(request);
                }
                _ => unreachable!(),
            }
            fixture.finish(request);
            assert_eq!(
                ToolsManifest::read(&fixture.store.manifest_path())
                    .unwrap()
                    .dotfiles
                    .packages,
                ["editor", "shell"],
                "{interruption}"
            );
            assert_eq!(
                std::fs::read(fixture.store.dotfiles_dir().join("shell/.zshrc")).unwrap(),
                b"shell"
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ImportedDotfiles>(request)
                    .is_none(),
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some(), "{interruption}");
        }
    }

    #[test]
    fn rollback_preserves_a_replaced_package_and_holds_the_lease_until_retry() {
        let mut fixture = Fixture::new();
        let input = fixture.input();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture.import(input.clone());
        let package = fixture.store.dotfiles_dir().join("editor");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !package.join(".editor").exists() {
            fixture.app.update();
            assert!(Instant::now() < deadline, "package did not activate");
            std::thread::sleep(Duration::from_millis(1));
        }
        let held = fixture.directory.path().join("held");
        std::fs::rename(&package, &held).unwrap();
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("keep"), b"foreign").unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(request)
            .insert(OperationState::Canceled);
        let retry = fixture.import(input);
        loop {
            fixture.app.update();
            if fixture
                .app
                .world_mut()
                .query_filtered::<Entity, With<CleanupFailed>>()
                .iter(fixture.app.world())
                .next()
                .is_some()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "rollback did not refuse the replacement"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(std::fs::read(package.join("keep")).unwrap(), b"foreign");
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_none());
        std::fs::rename(&package, fixture.directory.path().join("foreign")).unwrap();
        std::fs::rename(held, &package).unwrap();
        fixture.finish(request);
        fixture.finish(retry);
        assert!(
            fixture
                .app
                .world()
                .get::<ImportedDotfiles>(request)
                .is_none()
        );
        assert!(fixture.app.world().get::<ImportedDotfiles>(retry).is_some());
        assert_eq!(
            std::fs::read(fixture.directory.path().join("foreign/keep")).unwrap(),
            b"foreign"
        );
        assert!(fixture.store.lease().is_some());
    }
}
