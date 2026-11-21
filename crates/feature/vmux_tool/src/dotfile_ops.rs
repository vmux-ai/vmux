use bevy::tasks::{IoTaskPool, Task};
use std::future::Future;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::Context;
use std::time::Duration;

use bevy::prelude::*;
use vmux_ecs::{
    EntityTarget, JobRequest, JobResult, Operation, OperationRetained, OperationState,
    OperationTarget,
};

use crate::dotfiles::{
    AdoptDotfile, AdoptedDotfile, AppliedEnabledDotfiles, ApplyEnabledDotfiles,
    DisableDotfilePackage, DisabledDotfilePackage, DiscoverDotfilePackages,
    DiscoveredDotfilePackages, DotfileLink, DotfileLinkState, DotfilePlan, ImportAvailableDotfiles,
    ImportedAvailableDotfiles, LinkDotfilePackage, LinkedDotfilePackage, PlanDotfilePackage,
    UnlinkDotfilePackage, UnlinkedDotfilePackage,
};
use crate::io::{
    CleanupPath, FileIdentity, InspectLink, LinkPath, MovePath, ObservePath, PathReceipt,
    ReadDirectory, UnlinkPath,
};
use crate::storage::{
    ManifestOnly, ManifestReceipt, ReadRequest, ReadResult, StoreLease, WriteRequest, WriteResult,
};
use crate::{
    ExternalToolOperation, ToolOperationFailed, ToolOperationFinished, ToolOperationSucceeded,
    ToolStore, ToolStoreOperation, ToolStoreTarget, ToolTaskSet,
};

pub(crate) struct DotfileOperationsPlugin;

impl Plugin for DotfileOperationsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (
                    accept::<PlanDotfilePackage>,
                    accept::<DiscoverDotfilePackages>,
                    accept::<LinkDotfilePackage>,
                    accept::<DisableDotfilePackage>,
                    accept::<UnlinkDotfilePackage>,
                    accept::<ApplyEnabledDotfiles>,
                    accept::<ImportAvailableDotfiles>,
                    accept::<AdoptDotfile>,
                ),
                (
                    cancel::<PlanDotfilePackage>,
                    cancel::<DiscoverDotfilePackages>,
                    cancel::<LinkDotfilePackage>,
                    cancel::<DisableDotfilePackage>,
                    cancel::<UnlinkDotfilePackage>,
                    cancel::<ApplyEnabledDotfiles>,
                    cancel::<ImportAvailableDotfiles>,
                    cancel::<AdoptDotfile>,
                ),
                begin,
                loaded,
                (
                    select_plan,
                    select_link,
                    select_disable,
                    select_unlink,
                    select_apply,
                    select_adopt,
                ),
                (plan, discover, adopt_observed, adopt_selected).chain(),
                directories,
                inspected,
                planned,
                (discovered, import_available, relocated).chain(),
                retry_cleanup,
                cancel_files,
                mutate,
                (
                    effects::<LinkPath, Linking>,
                    effects::<UnlinkPath, Unlinking>,
                ),
                save,
                saved,
                rollback,
                (restored_config, return_adopted, returned_adopted).chain(),
                (
                    reverted::<LinkPath, Linking>,
                    reverted::<UnlinkPath, Unlinking>,
                ),
                (recover_paths, recovered_paths, mutation_completed).chain(),
            )
                .chain()
                .in_set(ToolTaskSet::Start),
        )
        .add_systems(
            PostUpdate,
            (finish, finish_mutations, finish_available, finish_adoption)
                .after(vmux_ecs::JobSet::Cleanup),
        )
        .add_observer(cleanup_failed)
        .add_observer(config_failed);
    }
}

#[derive(Component)]
struct Work {
    requester: Entity,
    store: Option<Entity>,
    retained: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct DotfileDrain<'w, 's> {
    work: Query<'w, 's, &'static Work>,
}

impl DotfileDrain<'_, '_> {
    pub(crate) fn is_drained(&self, requester: Entity) -> bool {
        !self.work.iter().any(|work| work.requester == requester)
    }
}

#[derive(Component, Clone)]
pub(crate) struct SharedReadLease {
    pub(crate) store: ToolStore,
    pub(crate) lease: StoreLease,
}

#[derive(Component)]
struct Captured<R: Component>(R);

#[derive(Component)]
struct Accepted<R: Component>(PhantomData<R>);

#[derive(Component)]
struct Loading;

#[derive(Component)]
struct Loaded;

#[derive(Component)]
struct Selected;

#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct Names(Vec<String>);

#[derive(Component)]
struct Mutation;
#[derive(Component)]
struct RemoveLinks;
#[derive(Component)]
struct Enable;
#[derive(Component)]
struct Disable;
#[derive(Component)]
struct Before(crate::ToolsManifest);
#[derive(Component)]
struct Saving;
#[derive(Component)]
struct Saved;
#[derive(Component)]
struct Rollback;
#[derive(Component)]
struct RestoringConfig;
#[derive(Component)]
struct ConfigRestored;
#[derive(Component)]
struct ConfigPublished;
#[derive(Component)]
struct ConfigFailure(String);
#[derive(Component)]
struct RetryDelay(Duration);
#[derive(Component)]
struct RetryAlarm(Task<()>);

#[derive(Component)]
struct NativePaths {
    packages: PathBuf,
    home: PathBuf,
    boundary: Option<PathBuf>,
}

impl NativePaths {
    fn capture(store: &ToolStore) -> Result<Self, String> {
        let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
        let packages = if store.dotfiles_dir().is_absolute() {
            store.dotfiles_dir()
        } else {
            cwd.join(store.dotfiles_dir())
        };
        let home = if store.home().is_absolute() {
            store.home().to_path_buf()
        } else {
            cwd.join(store.home())
        };
        let isolated = std::env::var_os("VMUX_DATA_DIR").is_some_and(|value| !value.is_empty());
        let boundary = isolated.then(|| vmux_profile::ProfilePaths::current().shared_data());
        Ok(Self {
            packages,
            home,
            boundary,
        })
    }
}

#[derive(Component)]
struct CleanupFailed(String);
#[derive(Component)]
struct MutationFinished(usize);
#[derive(Component)]
struct ImportedCount(usize);
#[derive(Component)]
struct Adopting;
#[derive(Component)]
struct AdoptHome;
#[derive(Component)]
struct AdoptRoot;
#[derive(Component)]
struct AdoptInput(crate::io::PathFacts);
#[derive(Component)]
struct AdoptHomePath(PathBuf);
#[derive(Component)]
struct AdoptStorePath(PathBuf);
#[derive(Component)]
struct MovingFile;
#[derive(Component, Default)]
struct MovementReceipt(PathReceipt);
#[derive(Component)]
struct Relocated;
#[derive(Component)]
struct ReturningFile;
#[derive(Component)]
struct ReturnedFile;
#[derive(Component)]
struct Changing;
#[derive(Component)]
struct Linking;
#[derive(Component)]
struct Unlinking;
#[derive(Component)]
struct Changed;
#[derive(Component)]
struct EffectDone;
#[derive(Component)]
struct Reverting;
#[derive(Component)]
struct Reverted;
#[derive(Component)]
struct ForwardReceipt(PathReceipt);
#[derive(Component)]
struct ReverseReceipt(PathReceipt);
#[derive(Component)]
struct CleaningPaths;
#[derive(Component)]
struct CleaningReversePaths;

#[derive(Component)]
struct Ready;

#[derive(Component)]
struct PackageRoot(PathBuf);

#[derive(Component)]
struct Package;

#[derive(Component)]
struct Directory {
    depth: usize,
}

#[derive(Component)]
struct DirectoryDone;

#[derive(Component, Default)]
struct EntryCount(usize);

#[derive(Component)]
struct File;

#[derive(Component)]
struct Source(PathBuf);

#[derive(Component)]
struct Target(PathBuf);

#[derive(Component)]
struct LinkState(DotfileLinkState);

#[derive(Component)]
struct Discovery;

#[derive(Component)]
struct DiscoveryOutput(Vec<String>);

fn accept<R: Component + Clone + PartialEq>(
    requests: Query<
        (
            Entity,
            &R,
            Option<&ToolStoreTarget>,
            Option<&ToolStore>,
            Option<&OperationState>,
            Has<OperationRetained>,
            Option<&SharedReadLease>,
        ),
        (Without<Accepted<R>>, Without<ToolOperationFinished>),
    >,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, request, target, direct, state, retained, shared) in &requests {
        if state.is_some_and(|state| {
            !matches!(state, OperationState::Pending | OperationState::Running)
        }) {
            continue;
        }
        let source = match target {
            Some(target) => stores.get(target.entity()).ok(),
            None => direct,
        };
        let Some(store) = source else {
            commands.entity(entity).insert((
                ToolOperationFailed("tool store is unavailable".into()),
                ToolOperationFinished,
            ));
            continue;
        };
        let read_only = std::any::TypeId::of::<R>() == std::any::TypeId::of::<PlanDotfilePackage>()
            || std::any::TypeId::of::<R>() == std::any::TypeId::of::<DiscoverDotfilePackages>();
        if let Some(shared) = shared
            && (!read_only || shared.store != *store || !shared.lease._lease.matches(store))
        {
            commands.entity(entity).insert((
                ToolOperationFailed(
                    "dotfile read lease does not match the accepted request".into(),
                ),
                ToolOperationFinished,
            ));
            continue;
        }
        let paths = match NativePaths::capture(store) {
            Ok(paths) => paths,
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((ToolOperationFailed(error), ToolOperationFinished));
                continue;
            }
        };
        let mut observation = commands.spawn((
            Name::new("Observe dotfile operation"),
            paths,
            Operation::bundle("tool.dotfile.observe").unwrap(),
            OperationRetained,
            OperationTarget::new(entity),
            Work {
                requester: entity,
                store: target.map(ToolStoreTarget::entity),
                retained,
            },
            store.clone(),
            Captured(request.clone()),
            EntryCount::default(),
        ));
        if let Some(shared) = shared {
            observation.insert(shared.lease.clone());
        }
        commands
            .entity(entity)
            .insert((
                Accepted::<R>(PhantomData),
                ToolStoreOperation,
                OperationRetained,
            ))
            .remove::<ExternalToolOperation>();
    }
}

fn cancel<R: Component + Clone + PartialEq>(
    work: Query<(
        Entity,
        &Work,
        &Captured<R>,
        &ToolStore,
        &OperationState,
        Has<Saving>,
        Has<Rollback>,
        Has<Failure>,
    )>,
    requests: Query<(
        Option<&R>,
        Option<&ToolStoreTarget>,
        Option<&ToolStore>,
        Option<&OperationState>,
    )>,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, original, captured, store, state, saving, rolling_back, failed) in &work {
        if !matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let valid =
            requests
                .get(original.requester)
                .is_ok_and(|(request, target, direct, state)| {
                    request == Some(&captured.0)
                        && state.is_none_or(|state| {
                            matches!(state, OperationState::Pending | OperationState::Running)
                        })
                        && match original.store {
                            Some(source) => {
                                target.is_some_and(|target| target.entity() == source)
                                    && stores.get(source).ok() == Some(store)
                            }
                            None => target.is_none() && direct == Some(store),
                        }
                });
        if !valid && !rolling_back && !failed {
            let mut work = commands.entity(entity);
            work.insert(Failure("dotfile operation source changed".into()));
            if !saving {
                work.insert(OperationState::Canceled);
            }
        }
    }
}

fn begin(
    work: Query<
        (
            Entity,
            &Work,
            &ToolStore,
            &OperationState,
            Option<&StoreLease>,
        ),
        (Without<Loading>, Without<Loaded>, Without<Failure>),
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, original, store, state, captured_lease) in &work {
        if *state != OperationState::Pending {
            continue;
        }
        if let Some(wake) = &wake {
            store.bind_waker(wake.0.clone());
        }
        let lease = match captured_lease {
            Some(lease) if lease._lease.matches(store) => lease.clone(),
            Some(_) => {
                commands.entity(entity).insert(Failure(
                    "dotfile shared lease does not match the store".into(),
                ));
                continue;
            }
            None => {
                let Some(lease) = store.lease() else {
                    continue;
                };
                StoreLease { _lease: lease }
            }
        };
        commands.entity(entity).insert((
            Loading,
            lease,
            ReadRequest { retain_lease: true },
            OperationState::Running,
        ));
        if let Ok(mut requester) = commands.get_entity(original.requester) {
            requester.insert(OperationState::Running);
        }
    }
}

fn loaded(
    work: Query<(Entity, &OperationState, &ReadResult), With<Loading>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &work {
        commands.entity(entity).remove::<(Loading, ReadResult)>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(manifest) => {
                commands
                    .entity(entity)
                    .insert((Loaded, Before(manifest.clone())));
            }
            Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
            }
        }
    }
}

fn select_plan(
    work: Query<(Entity, &Captured<PlanDotfilePackage>), (With<Loaded>, Without<Names>)>,
    mut commands: Commands,
) {
    for (entity, request) in &work {
        commands
            .entity(entity)
            .insert(Names(vec![request.0.package.clone()]));
    }
}

fn select_link(
    work: Query<(Entity, &Captured<LinkDotfilePackage>), (With<Loaded>, Without<Names>)>,
    mut commands: Commands,
) {
    for (entity, request) in &work {
        commands
            .entity(entity)
            .insert((Names(vec![request.0.package.clone()]), Mutation, Enable));
    }
}

fn select_disable(
    work: Query<(Entity, &Captured<DisableDotfilePackage>), (With<Loaded>, Without<Names>)>,
    mut commands: Commands,
) {
    for (entity, request) in &work {
        commands.entity(entity).insert((
            Names(vec![request.0.package.clone()]),
            Mutation,
            RemoveLinks,
            Disable,
        ));
    }
}

fn select_unlink(
    work: Query<(Entity, &Captured<UnlinkDotfilePackage>), (With<Loaded>, Without<Names>)>,
    mut commands: Commands,
) {
    for (entity, request) in &work {
        commands.entity(entity).insert((
            Names(vec![request.0.package.clone()]),
            Mutation,
            RemoveLinks,
        ));
    }
}

fn select_apply(
    work: Query<
        (Entity, &Before),
        (
            With<Captured<ApplyEnabledDotfiles>>,
            With<Loaded>,
            Without<Names>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, before) in &work {
        commands
            .entity(entity)
            .insert((Names(before.0.dotfiles.packages.clone()), Mutation));
    }
}

fn select_adopt(
    work: Query<
        (Entity, &Captured<AdoptDotfile>, &ToolStore, &OperationState),
        (With<Loaded>, Without<Adopting>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, request, store, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        if !Package::valid(&request.0.package) {
            commands
                .entity(entity)
                .insert(Failure("invalid dotfile package name".into()));
            continue;
        }
        let path = match request.0.path.strip_prefix("~") {
            Ok(relative) => store.home().join(relative),
            Err(_) => request.0.path.clone(),
        };
        commands.entity(entity).insert((
            Adopting,
            Selected,
            Mutation,
            Enable,
            Names(vec![request.0.package.clone()]),
            JobRequest(ObservePath { path }),
        ));
        commands.spawn((
            Name::new("Adopt home"),
            Operation::bundle("tool.dotfile.home").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            AdoptHome,
            JobRequest(ObservePath {
                path: store.home().to_path_buf(),
            }),
        ));
        commands.spawn((
            Name::new("Adopt root"),
            Operation::bundle("tool.dotfile.root").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            AdoptRoot,
            JobRequest(ObservePath {
                path: store.dotfiles_dir(),
            }),
        ));
    }
}

fn adopt_observed(
    work: Query<
        (
            Entity,
            &Captured<AdoptDotfile>,
            &OperationState,
            &JobResult<ObservePath>,
        ),
        With<Adopting>,
    >,
    paths: Query<(
        Entity,
        &ChildOf,
        &JobResult<ObservePath>,
        Has<AdoptHome>,
        Has<AdoptRoot>,
    )>,
    mut commands: Commands,
) {
    for (entity, _request, state, result) in &work {
        let mut home = None;
        let mut root = None;
        for (child, parent, facts, is_home, is_root) in &paths {
            if parent.parent() != entity {
                continue;
            }
            if is_home {
                home = Some((child, facts));
            }
            if is_root {
                root = Some((child, facts));
            }
        }
        let (Some((home_child, home_result)), Some((root_child, root_result))) = (home, root)
        else {
            continue;
        };
        commands.entity(entity).remove::<JobResult<ObservePath>>();
        commands
            .entity(home_child)
            .remove::<JobResult<ObservePath>>();
        commands
            .entity(root_child)
            .remove::<JobResult<ObservePath>>();
        if *state != OperationState::Running {
            continue;
        }
        let home = match &home_result.0 {
            Ok(Ok(facts)) => &facts.path,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
                continue;
            }
        };
        let root = match &root_result.0 {
            Ok(Ok(facts)) => &facts.path,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
                continue;
            }
        };
        let facts = match &result.0 {
            Ok(Ok(facts)) => facts,
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
                continue;
            }
        };
        commands.entity(entity).insert((
            AdoptInput(facts.clone()),
            AdoptHomePath(home.clone()),
            AdoptStorePath(root.clone()),
        ));
    }
}

fn adopt_selected(
    work: Query<
        (
            Entity,
            &Captured<AdoptDotfile>,
            &AdoptInput,
            &AdoptHomePath,
            &AdoptStorePath,
            &OperationState,
        ),
        (Without<Ready>, Without<Failure>),
    >,
    files: Query<&ChildOf, With<MovingFile>>,
    mut commands: Commands,
) {
    for (entity, request, input, home, root, state) in &work {
        if *state != OperationState::Running || files.iter().any(|owner| owner.parent() == entity) {
            continue;
        }
        let facts = &input.0;
        let home = &home.0;
        let root = &root.0;
        let Some(metadata) = &facts.metadata else {
            commands
                .entity(entity)
                .insert(Failure("adopt source does not exist".into()));
            continue;
        };
        if !metadata.is_file() || facts.path.starts_with(root) {
            commands.entity(entity).insert(Failure(
                "adopt source must be a regular home file outside Tools".into(),
            ));
            continue;
        }
        let Ok(relative) = facts.path.strip_prefix(home) else {
            commands.entity(entity).insert(Failure(
                "adopt source must be inside the accepted home".into(),
            ));
            continue;
        };
        if relative.as_os_str().is_empty() {
            commands
                .entity(entity)
                .insert(Failure("adopt source has no relative path".into()));
            continue;
        }
        let identity = match FileIdentity::from_metadata(metadata) {
            Ok(identity) => identity,
            Err(error) => {
                commands.entity(entity).insert(Failure(error));
                continue;
            }
        };
        let root = root.join(&request.0.package);
        let destination = root.join(relative);
        let package = commands
            .spawn((
                Name::new(request.0.package.clone()),
                Package,
                PackageRoot(root),
                ChildOf(entity),
            ))
            .id();
        let receipt = PathReceipt::default();
        commands.spawn((
            Name::new("Adopted file"),
            Operation::bundle("tool.dotfile.move").unwrap().running(),
            OperationRetained,
            ChildOf(entity),
            EntityTarget::<Package>::new(package),
            File,
            Source(destination.clone()),
            Target(facts.path.clone()),
            identity.clone(),
            LinkState(DotfileLinkState::Missing),
            MovingFile,
            MovementReceipt(receipt.clone()),
            JobRequest(MovePath {
                source: facts.path.clone(),
                target: destination,
                expected: identity,
                receipt,
            }),
        ));
    }
}

fn relocated(
    files: Query<
        (
            Entity,
            &ChildOf,
            &MovementReceipt,
            Option<&JobResult<MovePath>>,
            Has<JobRequest<MovePath>>,
        ),
        With<MovingFile>,
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, owner, receipt, result, requested) in &files {
        if requested || captures.iter().any(|target| target.entity() == entity) {
            continue;
        }
        commands
            .entity(entity)
            .remove::<(MovingFile, JobResult<MovePath>)>();
        if receipt.0.changed() {
            commands.entity(entity).insert(Relocated);
        }
        match result.map(|result| &result.0) {
            Some(Ok(Ok(_))) => {
                commands.entity(owner.parent()).insert(Ready);
            }
            Some(Ok(Err(error))) | Some(Err(error)) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
            }
            None => {
                commands
                    .entity(owner.parent())
                    .insert(Failure("native adopt move result is unavailable".into()));
            }
        }
    }
}

fn return_adopted(
    work: Query<
        (
            Entity,
            Has<ConfigPublished>,
            Has<ConfigRestored>,
            Has<ConfigFailure>,
        ),
        (
            With<Captured<AdoptDotfile>>,
            With<Failure>,
            Without<CleanupFailed>,
            Without<Saving>,
            Without<RestoringConfig>,
            Without<ReadRequest>,
            Without<WriteRequest>,
        ),
    >,
    files: Query<
        (
            Entity,
            &ChildOf,
            &Source,
            &Target,
            &FileIdentity,
            Has<Changed>,
            Has<Reverted>,
        ),
        (
            With<Relocated>,
            Without<MovingFile>,
            Without<ReturningFile>,
            Without<ReturnedFile>,
            Without<Changing>,
            Without<Reverting>,
            Without<CleaningPaths>,
            Without<JobRequest<LinkPath>>,
            Without<JobRequest<UnlinkPath>>,
            Without<JobRequest<CleanupPath>>,
        ),
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
) {
    for (owner, saved, restored, config_failed) in &work {
        if saved && !restored && !config_failed {
            continue;
        }
        for (file, parent, source, target, identity, changed, reverted) in &files {
            if parent.parent() != owner
                || changed && !reverted
                || captures.iter().any(|capture| capture.entity() == file)
            {
                continue;
            }
            commands.entity(file).insert((
                ReturningFile,
                OperationState::Running,
                JobRequest(MovePath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    expected: identity.clone(),
                    receipt: PathReceipt::default(),
                }),
            ));
        }
    }
}

fn returned_adopted(
    files: Query<(Entity, &ChildOf, &JobResult<MovePath>), With<ReturningFile>>,
    mut commands: Commands,
) {
    for (entity, owner, result) in &files {
        commands
            .entity(entity)
            .remove::<(ReturningFile, JobResult<MovePath>)>();
        match &result.0 {
            Ok(Ok(_)) => {
                commands.entity(entity).insert(ReturnedFile);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(CleanupFailed(error.clone()));
            }
        }
    }
}

fn plan(
    work: Query<
        (Entity, &Names, &NativePaths, &OperationState),
        (With<Loaded>, Without<Selected>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, names, paths, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        for name in &names.0 {
            if !Package::valid(name) {
                commands
                    .entity(entity)
                    .insert(Failure(format!("invalid dotfile package name: {name}")));
                break;
            }
            let root = paths.packages.join(name);
            let package = commands
                .spawn((
                    Name::new(name.clone()),
                    Package,
                    PackageRoot(root.clone()),
                    ChildOf(entity),
                ))
                .id();
            commands.spawn((
                Name::new("Inspect dotfile package"),
                Operation::bundle("tool.dotfile.directory")
                    .unwrap()
                    .running(),
                OperationRetained,
                ChildOf(entity),
                EntityTarget::<Package>::new(package),
                Directory { depth: 0 },
                JobRequest(ReadDirectory {
                    path: root,
                    limit: 4096,
                }),
            ));
        }
        commands.entity(entity).insert(Selected);
    }
}

impl Package {
    fn valid(name: &str) -> bool {
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.starts_with('.')
            && !name.contains(['/', '\\'])
    }
}

fn discover(
    work: Query<
        (Entity, &NativePaths, &OperationState),
        (
            Or<(
                With<Captured<DiscoverDotfilePackages>>,
                With<Captured<ImportAvailableDotfiles>>,
            )>,
            With<Loaded>,
            Without<Selected>,
            Without<Failure>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, paths, state) in &work {
        if *state == OperationState::Running {
            commands.entity(entity).insert((
                Selected,
                Discovery,
                JobRequest(ReadDirectory {
                    path: paths.packages.clone(),
                    limit: 100_000,
                }),
            ));
        }
    }
}

fn directories(
    directories: Query<
        (
            Entity,
            &ChildOf,
            &EntityTarget<Package>,
            &Directory,
            &JobResult<ReadDirectory>,
        ),
        Without<DirectoryDone>,
    >,
    packages: Query<(&Name, &PackageRoot), With<Package>>,
    mut work: Query<(&NativePaths, &OperationState, &mut EntryCount, Has<Disable>)>,
    mut commands: Commands,
) {
    for (entity, owner, package, directory, result) in &directories {
        commands
            .entity(entity)
            .insert(DirectoryDone)
            .remove::<JobResult<ReadDirectory>>();
        let Ok((paths, state, mut count, disabling)) = work.get_mut(owner.parent()) else {
            continue;
        };
        if *state != OperationState::Running {
            continue;
        }
        let entries = match &result.0 {
            Ok(Ok(entries)) => entries,
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
                continue;
            }
        };
        let Ok((name, root)) = packages.get(package.entity()) else {
            continue;
        };
        if !entries.exists || !entries.is_directory {
            if disabling && directory.depth == 0 {
                continue;
            }
            commands.entity(owner.parent()).insert(Failure(format!(
                "dotfile package does not exist: {}",
                name.as_str()
            )));
            continue;
        }
        for entry in &entries.entries {
            count.0 += 1;
            if count.0 > 100_000 || directory.depth >= 128 {
                commands.entity(owner.parent()).insert(Failure(
                    "dotfile directory traversal exceeds its entry or depth bound".into(),
                ));
                break;
            }
            if entry.file_type.is_dir() {
                commands.spawn((
                    Name::new("Inspect dotfile directory"),
                    Operation::bundle("tool.dotfile.directory")
                        .unwrap()
                        .running(),
                    OperationRetained,
                    ChildOf(owner.parent()),
                    EntityTarget::<Package>::new(package.entity()),
                    Directory {
                        depth: directory.depth + 1,
                    },
                    JobRequest(ReadDirectory {
                        path: entry.path.clone(),
                        limit: 4096,
                    }),
                ));
            } else if entry.file_type.is_file() {
                let Ok(relative) = entry.path.strip_prefix(&root.0) else {
                    continue;
                };
                let target = paths.home.join(relative);
                commands.spawn((
                    Name::new(relative.to_string_lossy().into_owned()),
                    File,
                    Source(entry.path.clone()),
                    Target(target.clone()),
                    ChildOf(owner.parent()),
                    EntityTarget::<Package>::new(package.entity()),
                    Operation::bundle("tool.dotfile.link.observe")
                        .unwrap()
                        .running(),
                    OperationRetained,
                    JobRequest(InspectLink {
                        source: entry.path.clone(),
                        target,
                    }),
                ));
            }
        }
    }
}

fn inspected(
    files: Query<(Entity, &ChildOf, &JobResult<InspectLink>), (With<File>, Without<LinkState>)>,
    mut commands: Commands,
) {
    for (entity, owner, result) in &files {
        commands.entity(entity).remove::<JobResult<InspectLink>>();
        let state = match &result.0 {
            Ok(Ok(facts)) => {
                if facts.target_type.is_none() {
                    DotfileLinkState::Missing
                } else if facts.target_type.is_some_and(|kind| kind.is_symlink())
                    && facts.target.as_ref() == Some(&facts.source)
                {
                    DotfileLinkState::Linked
                } else {
                    DotfileLinkState::Conflict
                }
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(Failure(error.clone()));
                continue;
            }
        };
        commands.entity(entity).insert(LinkState(state));
    }
}

fn planned(
    work: Query<
        (
            Entity,
            &Names,
            &OperationState,
            Has<Mutation>,
            Has<RemoveLinks>,
        ),
        (
            With<Selected>,
            Without<Ready>,
            Without<Failure>,
            Without<Adopting>,
        ),
    >,
    directories: Query<(&ChildOf, Has<DirectoryDone>), With<Directory>>,
    files: Query<(&ChildOf, &Source, &Target, Option<&LinkState>), With<File>>,
    mut commands: Commands,
) {
    for (entity, names, state, mutation, removing) in &work {
        if *state != OperationState::Running
            || directories
                .iter()
                .any(|(owner, done)| owner.parent() == entity && !done)
        {
            continue;
        }
        let mut links = Vec::new();
        let mut pending = false;
        for (owner, source, target, state) in &files {
            if owner.parent() != entity {
                continue;
            }
            match state {
                Some(state) => links.push(DotfileLink {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    state: state.0,
                }),
                None => {
                    pending = true;
                }
            }
        }
        if pending {
            continue;
        }
        if mutation
            && !removing
            && links
                .iter()
                .any(|link| link.state == DotfileLinkState::Conflict)
        {
            commands
                .entity(entity)
                .insert(Failure("dotfile plan contains conflicting targets".into()));
            continue;
        }
        links.sort_by(|left, right| left.source.cmp(&right.source));
        commands.entity(entity).insert((
            Ready,
            DotfilePlan {
                package: names.0.first().cloned().unwrap_or_default(),
                links,
            },
        ));
    }
}

fn discovered(
    work: Query<(Entity, &OperationState, &JobResult<ReadDirectory>), With<Discovery>>,
    mut commands: Commands,
) {
    for (entity, state, result) in &work {
        commands.entity(entity).remove::<JobResult<ReadDirectory>>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(Ok(entries)) => {
                if entries.exists && !entries.is_directory {
                    commands
                        .entity(entity)
                        .insert(Failure("dotfile package catalog is not a directory".into()));
                    continue;
                }
                let mut names = Vec::new();
                for entry in &entries.entries {
                    if !entry.file_type.is_dir() {
                        continue;
                    }
                    if let Some(name) = entry.name.to_str()
                        && Package::valid(name)
                    {
                        names.push(name.to_string());
                    }
                }
                names.sort_by_key(|name| name.to_ascii_lowercase());
                commands
                    .entity(entity)
                    .insert((Ready, DiscoveryOutput(names)));
            }
            Ok(Err(error)) | Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
            }
        }
    }
}

fn import_available(
    work: Query<
        (Entity, &DiscoveryOutput, &Before, &OperationState),
        (With<Captured<ImportAvailableDotfiles>>, Without<Mutation>),
    >,
    mut commands: Commands,
) {
    for (entity, packages, before, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let mut imported = 0;
        for package in &packages.0 {
            imported += usize::from(!before.0.dotfiles.packages.contains(package));
        }
        commands.entity(entity).insert((
            Mutation,
            Enable,
            Names(packages.0.clone()),
            ImportedCount(imported),
        ));
    }
}

fn retry_cleanup(
    failed: Query<
        (Entity, Option<&RetryDelay>),
        (
            With<Work>,
            Or<(With<CleanupFailed>, With<ConfigFailure>)>,
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
            .remove::<(RetryAlarm, CleanupFailed, ConfigFailure)>()
            .insert(RetryDelay(next));
    }
    for (entity, delay) in &failed {
        let delay = delay
            .map(|delay| delay.0)
            .unwrap_or(Duration::from_millis(100));
        let waker = waker.clone();
        commands
            .entity(entity)
            .insert(RetryAlarm(IoTaskPool::get().spawn(async move {
                async_io::Timer::after(delay).await;
                waker.wake();
            })));
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
    let Ok(mut requester) = commands.get_entity(work.requester) else {
        return;
    };
    requester.insert((
        ToolOperationFailed(format!("dotfile rollback failed: {}", failure.0)),
        ToolOperationFinished,
        OperationState::Failed,
    ));
}

fn config_failed(
    trigger: On<Add, ConfigFailure>,
    work: Query<(&Work, &ConfigFailure)>,
    mut commands: Commands,
) {
    let Ok((work, failure)) = work.get(trigger.event_target()) else {
        return;
    };
    let Ok(mut requester) = commands.get_entity(work.requester) else {
        return;
    };
    requester.insert((
        ToolOperationFailed(format!(
            "dotfile configuration restoration failed: {}",
            failure.0
        )),
        ToolOperationFinished,
        OperationState::Failed,
    ));
}

fn cancel_files(
    work: Query<(Entity, &OperationState, Has<Failure>, Has<Rollback>), With<Work>>,
    mut files: Query<
        (&ChildOf, &mut OperationState),
        (
            Or<(
                With<File>,
                With<Directory>,
                With<AdoptHome>,
                With<AdoptRoot>,
            )>,
            Without<Work>,
        ),
    >,
) {
    for (entity, state, failed, rollback) in &work {
        if rollback || !failed && *state != OperationState::Canceled {
            continue;
        }
        for (owner, mut state) in &mut files {
            if owner.parent() == entity {
                *state = OperationState::Canceled;
            }
        }
    }
}

fn mutate(
    work: Query<
        (Entity, &OperationState, Has<RemoveLinks>, &NativePaths),
        (
            With<Mutation>,
            With<Ready>,
            Without<Failure>,
            Without<Saving>,
            Without<Saved>,
        ),
    >,
    files: Query<(
        Entity,
        &ChildOf,
        &Source,
        &Target,
        &LinkState,
        Has<Changing>,
        Has<EffectDone>,
    )>,
    mut commands: Commands,
) {
    for (entity, state, removing, paths) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let mut next = None;
        let mut active = false;
        for (file, owner, source, _, _, changing, done) in &files {
            if owner.parent() != entity || done {
                continue;
            }
            if changing {
                active = true;
                break;
            }
            if next
                .as_ref()
                .is_none_or(|(_, path): &(Entity, PathBuf)| source.0 < *path)
            {
                next = Some((file, source.0.clone()));
            }
        }
        if active {
            continue;
        }
        let Some((file, _)) = next else {
            continue;
        };
        let (_, _, source, target, state, _, _) = files.get(file).unwrap();
        if removing && state.0 != DotfileLinkState::Linked
            || !removing
                && !matches!(
                    state.0,
                    DotfileLinkState::Missing | DotfileLinkState::Linked
                )
        {
            commands.entity(file).insert(EffectDone);
            continue;
        }
        let receipt = PathReceipt::default();
        let mut file = commands.entity(file);
        file.insert((
            Changing,
            receipt.clone(),
            ForwardReceipt(receipt.clone()),
            OperationState::Running,
        ));
        if removing {
            file.insert((
                Unlinking,
                JobRequest(UnlinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    boundary: paths.boundary.clone(),
                    receipt,
                }),
            ));
        } else {
            file.insert((
                Linking,
                JobRequest(LinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    boundary: paths.boundary.clone(),
                    receipt,
                }),
            ));
        }
    }
}

fn effects<J: vmux_ecs::Job<Output = Result<crate::io::PathChanged, String>>, M: Component>(
    files: Query<
        (
            Entity,
            &ChildOf,
            &PathReceipt,
            Option<&JobResult<J>>,
            Has<JobRequest<J>>,
        ),
        (With<Changing>, With<M>),
    >,
    work: Query<Has<Failure>, With<Work>>,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
) {
    for (file, owner, receipt, result, requested) in &files {
        if captures.iter().any(|target| target.entity() == file) || requested {
            continue;
        }
        let failed = work.get(owner.parent()).unwrap_or(true);
        match result {
            Some(result) => match &result.0 {
                Ok(Ok(_)) => {}
                Ok(Err(error)) | Err(error) => {
                    commands
                        .entity(owner.parent())
                        .insert(Failure(error.clone()));
                }
            },
            None if !failed => {
                commands
                    .entity(owner.parent())
                    .insert(Failure("dotfile mutation result is unavailable".into()));
            }
            None => {}
        }
        let mut file = commands.entity(file);
        file.remove::<(Changing, JobResult<J>, M)>()
            .insert(EffectDone);
        if receipt.changed() {
            file.insert(Changed);
        }
    }
}

fn save(
    work: Query<
        (
            Entity,
            &Names,
            &Before,
            &OperationState,
            Has<Enable>,
            Has<Disable>,
        ),
        (
            With<Mutation>,
            With<Ready>,
            Without<Saved>,
            Without<Saving>,
            Without<Failure>,
        ),
    >,
    files: Query<(&ChildOf, Has<EffectDone>), With<File>>,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, names, before, state, enable, disable) in &work {
        if *state != OperationState::Running
            || files
                .iter()
                .any(|(owner, done)| owner.parent() == entity && !done)
            || captures.iter().any(|target| target.entity() == entity)
        {
            continue;
        }
        if !enable && !disable {
            commands.entity(entity).insert(Saved);
            continue;
        }
        let mut desired = before.0.clone();
        for name in &names.0 {
            desired.set_dotfile_package(name, enable);
        }
        commands.entity(entity).insert((
            Saving,
            ManifestOnly,
            WriteRequest {
                manifest: desired,
                retain_lease: true,
            },
        ));
    }
}

fn saved(
    work: Query<(Entity, &WriteResult, Option<&ManifestReceipt>), With<Saving>>,
    mut commands: Commands,
) {
    for (entity, result, receipt) in &work {
        commands.entity(entity).remove::<(Saving, WriteResult)>();
        if receipt.is_some_and(ManifestReceipt::published) {
            commands.entity(entity).insert(ConfigPublished);
        }
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(Saved);
            }
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
            &Before,
            &NativePaths,
            Option<&Children>,
            Has<ConfigPublished>,
            Has<ConfigRestored>,
            Has<RestoringConfig>,
            Has<RemoveLinks>,
            Has<ReadRequest>,
            Has<WriteRequest>,
            Has<ConfigFailure>,
        ),
        (With<Mutation>, With<Failure>, Without<CleanupFailed>),
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    files: Query<(
        Entity,
        &ChildOf,
        &Source,
        &Target,
        Has<Changed>,
        Has<Reverting>,
        Has<Reverted>,
        Option<&ReverseReceipt>,
    )>,
    mut commands: Commands,
) {
    for (
        entity,
        before,
        paths,
        children,
        saved,
        restored,
        restoring,
        removing,
        reading,
        writing,
        config_failed,
    ) in &work
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
        if saved && !restored && !restoring && !config_failed {
            commands.entity(entity).insert((
                RestoringConfig,
                ManifestOnly,
                WriteRequest {
                    manifest: before.0.clone(),
                    retain_lease: true,
                },
            ));
        }
        let mut next = None;
        let mut active = false;
        for (file, owner, source, _, changed, reverting, reverted, receipt) in &files {
            if owner.parent() != entity || !changed || reverted {
                continue;
            }
            if reverting || receipt.is_some_and(|receipt| receipt.0.pending()) {
                active = true;
                break;
            }
            if next
                .as_ref()
                .is_none_or(|(_, path): &(Entity, PathBuf)| source.0 > *path)
            {
                next = Some((file, source.0.clone()));
            }
        }
        if active {
            continue;
        }
        let Some((file, _)) = next else {
            continue;
        };
        let (_, _, source, target, ..) = files.get(file).unwrap();
        let receipt = PathReceipt::default();
        let mut file = commands.entity(file);
        file.insert((
            Reverting,
            OperationState::Running,
            receipt.clone(),
            ReverseReceipt(receipt.clone()),
        ));
        if removing {
            file.insert((
                Linking,
                JobRequest(LinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    boundary: paths.boundary.clone(),
                    receipt,
                }),
            ));
        } else {
            file.insert((
                Unlinking,
                JobRequest(UnlinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    boundary: paths.boundary.clone(),
                    receipt,
                }),
            ));
        }
    }
}

fn restored_config(
    work: Query<(Entity, &WriteResult), With<RestoringConfig>>,
    mut commands: Commands,
) {
    for (entity, result) in &work {
        commands
            .entity(entity)
            .remove::<(WriteResult, RestoringConfig)>();
        match &result.0 {
            Ok(()) => {
                commands.entity(entity).insert(ConfigRestored);
            }
            Err(error) => {
                commands.entity(entity).insert(ConfigFailure(error.clone()));
            }
        }
    }
}

fn reverted<J: vmux_ecs::Job<Output = Result<crate::io::PathChanged, String>>, M: Component>(
    files: Query<(Entity, &ChildOf, &JobResult<J>), (With<Reverting>, With<M>)>,
    mut commands: Commands,
) {
    for (file, owner, result) in &files {
        commands
            .entity(file)
            .remove::<(JobResult<J>, Reverting, M)>();
        match &result.0 {
            Ok(Ok(_)) => {
                commands.entity(file).insert(Reverted);
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(owner.parent())
                    .insert(CleanupFailed(error.clone()));
            }
        }
    }
}

fn recover_paths(
    work: Query<(Entity, Has<Failure>, Has<Saved>), (With<Mutation>, Without<CleanupFailed>)>,
    files: Query<
        (
            Entity,
            &ChildOf,
            &ForwardReceipt,
            Option<&ReverseReceipt>,
            Has<Changed>,
            Has<Reverted>,
            Has<Reverting>,
        ),
        Without<CleaningPaths>,
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, failed, saved) in &work {
        for (file, parent, receipt, reverse, changed, reverted, reverting) in &files {
            if parent.parent() != entity {
                continue;
            }
            if !failed && saved {
                receipt.0.commit();
                if !receipt.0.pending() {
                    continue;
                }
            }
            if reverting || captures.iter().any(|capture| capture.entity() == file) {
                continue;
            }
            if let Some(reverse) = reverse {
                if reverted {
                    reverse.0.commit();
                }
                if reverse.0.pending() {
                    commands.entity(file).insert((
                        CleaningPaths,
                        CleaningReversePaths,
                        OperationState::Running,
                        JobRequest(CleanupPath {
                            receipt: reverse.0.clone(),
                        }),
                    ));
                    continue;
                }
                commands.entity(file).remove::<ReverseReceipt>();
            }
            if failed && changed && !reverted
                || !receipt.0.pending()
                || captures.iter().any(|capture| capture.entity() == file)
            {
                continue;
            }
            commands.entity(file).insert((
                CleaningPaths,
                OperationState::Running,
                JobRequest(CleanupPath {
                    receipt: receipt.0.clone(),
                }),
            ));
        }
    }
}

fn recovered_paths(
    files: Query<
        (
            Entity,
            &ChildOf,
            &JobResult<CleanupPath>,
            Has<CleaningReversePaths>,
        ),
        With<CleaningPaths>,
    >,
    mut commands: Commands,
) {
    for (file, parent, result, reverse) in &files {
        commands
            .entity(file)
            .remove::<(CleaningPaths, CleaningReversePaths, JobResult<CleanupPath>)>();
        if let Ok(Ok(_)) = &result.0 {
            if reverse {
                commands.entity(file).remove::<ReverseReceipt>();
            }
            continue;
        }
        let error = match &result.0 {
            Ok(Err(error)) | Err(error) => error.clone(),
            _ => unreachable!(),
        };
        commands
            .entity(parent.parent())
            .insert(CleanupFailed(error));
    }
}

fn mutation_completed(
    work: Query<
        (
            Entity,
            Option<&Children>,
            Has<Failure>,
            Has<ConfigPublished>,
            Has<ConfigRestored>,
            Has<Saved>,
            Has<ReadRequest>,
            Has<WriteRequest>,
        ),
        (
            With<Mutation>,
            Without<MutationFinished>,
            Without<CleanupFailed>,
        ),
    >,
    files: Query<
        (
            &ChildOf,
            Has<Changed>,
            Has<Reverted>,
            Has<EffectDone>,
            Option<&ForwardReceipt>,
            Option<&ReverseReceipt>,
        ),
        With<File>,
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
) {
    for (entity, children, failed, started_save, restored, saved, reading, writing) in &work {
        if reading
            || writing
            || captures.iter().any(|target| {
                target.entity() == entity
                    || children.is_some_and(|children| children.contains(&target.entity()))
            })
        {
            continue;
        }
        if failed && started_save && !restored || !failed && !saved {
            continue;
        }
        let mut count = 0;
        let mut pending = false;
        for (owner, changed, reverted, done, receipt, reverse) in &files {
            if owner.parent() != entity {
                continue;
            }
            count += usize::from(changed);
            pending |= if failed {
                changed && !reverted
                    || receipt.is_some_and(|receipt| receipt.0.pending())
                    || reverse.is_some_and(|receipt| receipt.0.pending())
            } else {
                !done
                    || receipt.is_some_and(|receipt| receipt.0.pending())
                    || reverse.is_some_and(|receipt| receipt.0.pending())
            };
        }
        if !pending {
            commands.entity(entity).insert(MutationFinished(count));
        }
    }
}

fn finish(
    work: Query<
        (
            Entity,
            &Work,
            &OperationState,
            Option<&Children>,
            Option<&Failure>,
            Option<&DotfilePlan>,
            Option<&DiscoveryOutput>,
            Has<Ready>,
            Has<ReadRequest>,
        ),
        Without<Mutation>,
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    child_states: Query<&OperationState>,
    mut commands: Commands,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    for (entity, original, state, children, failure, plan, discovery, ready, reading) in &work {
        if failure.is_some() || *state == OperationState::Canceled {
            if let Some(children) = children {
                for child in children {
                    if child_states.contains(*child) {
                        commands.entity(*child).try_insert(OperationState::Canceled);
                    }
                }
            }
        } else if !ready {
            continue;
        }
        if reading
            || captures.iter().any(|target| {
                target.entity() == entity
                    || children.is_some_and(|children| children.contains(&target.entity()))
            })
        {
            continue;
        }
        if let Ok(mut requester) = commands.get_entity(original.requester) {
            if failure.is_some() || *state == OperationState::Canceled {
                requester.insert((
                    ToolOperationFailed(
                        failure
                            .map(|failure| failure.0.clone())
                            .unwrap_or_else(|| "dotfile operation canceled".into()),
                    ),
                    ToolOperationFinished,
                ));
            } else if let Some(plan) = plan {
                requester.insert((
                    plan.clone(),
                    OperationState::Succeeded,
                    ToolOperationSucceeded(String::new()),
                    ToolOperationFinished,
                ));
            } else if let Some(discovery) = discovery {
                requester.insert((
                    DiscoveredDotfilePackages {
                        packages: discovery.0.clone(),
                    },
                    OperationState::Succeeded,
                    ToolOperationSucceeded(String::new()),
                    ToolOperationFinished,
                ));
            }
            if !original.retained {
                requester.remove::<OperationRetained>();
            }
        }
        commands.entity(entity).despawn();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn finish_mutations(
    work: Query<
        (
            Entity,
            &Work,
            &MutationFinished,
            Option<&Failure>,
            Has<Enable>,
            Has<Disable>,
            Has<RemoveLinks>,
        ),
        (
            With<Mutation>,
            Without<CleanupFailed>,
            Without<Captured<ImportAvailableDotfiles>>,
            Without<Captured<AdoptDotfile>>,
        ),
    >,
    mut commands: Commands,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    for (entity, original, count, failure, enabling, disabling, removing) in &work {
        if let Ok(mut requester) = commands.get_entity(original.requester) {
            if let Some(failure) = failure {
                requester.insert((
                    ToolOperationFailed(failure.0.clone()),
                    ToolOperationFinished,
                ));
            } else if enabling {
                requester.insert((
                    LinkedDotfilePackage { files: count.0 },
                    ToolOperationSucceeded(format!("linked {} file(s)", count.0)),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            } else if disabling {
                requester.insert((
                    DisabledDotfilePackage { files: count.0 },
                    ToolOperationSucceeded(format!("unlinked {} file(s)", count.0)),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            } else if removing {
                requester.insert((
                    UnlinkedDotfilePackage { files: count.0 },
                    ToolOperationSucceeded(format!("unlinked {} file(s)", count.0)),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            } else {
                requester.insert((
                    AppliedEnabledDotfiles { files: count.0 },
                    ToolOperationSucceeded(String::new()),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            }
            if !original.retained {
                requester.remove::<OperationRetained>();
            }
        }
        commands.entity(entity).despawn();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn finish_adoption(
    work: Query<
        (Entity, &Work, Option<&Children>, Option<&Failure>),
        (
            With<Captured<AdoptDotfile>>,
            With<MutationFinished>,
            Without<CleanupFailed>,
        ),
    >,
    files: Query<
        (
            &ChildOf,
            &Source,
            Has<MovingFile>,
            Has<Relocated>,
            Has<ReturnedFile>,
        ),
        With<File>,
    >,
    captures: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    mut commands: Commands,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    for (entity, original, children, failure) in &work {
        let mut destination = None;
        let mut pending = false;
        for (owner, source, moving, relocated, returned) in &files {
            if owner.parent() != entity {
                continue;
            }
            destination = Some(source.0.clone());
            pending |= moving || failure.is_some() && relocated && !returned;
        }
        if pending
            || captures.iter().any(|target| {
                target.entity() == entity
                    || children.is_some_and(|children| children.contains(&target.entity()))
            })
        {
            continue;
        }
        if let Ok(mut requester) = commands.get_entity(original.requester) {
            if let Some(failure) = failure {
                requester.insert((
                    ToolOperationFailed(failure.0.clone()),
                    ToolOperationFinished,
                ));
            } else if let Some(path) = destination {
                requester.insert((
                    AdoptedDotfile { path: path.clone() },
                    ToolOperationSucceeded(path.display().to_string()),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            }
            if !original.retained {
                requester.remove::<OperationRetained>();
            }
        }
        commands.entity(entity).despawn();
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn finish_available(
    work: Query<
        (Entity, &Work, &ImportedCount, Option<&Failure>),
        (
            With<Captured<ImportAvailableDotfiles>>,
            With<MutationFinished>,
            Without<CleanupFailed>,
        ),
    >,
    mut commands: Commands,
    wake: Option<Res<vmux_ecs::JobWake>>,
) {
    for (entity, original, imported, failure) in &work {
        if let Ok(mut requester) = commands.get_entity(original.requester) {
            if let Some(failure) = failure {
                requester.insert((
                    ToolOperationFailed(failure.0.clone()),
                    ToolOperationFinished,
                ));
            } else {
                requester.insert((
                    ImportedAvailableDotfiles {
                        packages: imported.0,
                    },
                    ToolOperationSucceeded(format!("imported {} dotfile package(s)", imported.0)),
                    ToolOperationFinished,
                    OperationState::Succeeded,
                ));
            }
            if !original.retained {
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
            let config = directory.path().join("config");
            let home = directory.path().join("home");
            std::fs::create_dir_all(&home).unwrap();
            let store = ToolStore::new(config, home);
            crate::ToolsManifest::default()
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

        fn package(&self, name: &str) -> PathBuf {
            let root = self.store.dotfiles_dir().join(name);
            std::fs::create_dir_all(root.join(".config")).unwrap();
            std::fs::write(root.join(".zshrc"), b"shell").unwrap();
            std::fs::write(root.join(".config/editor"), b"editor").unwrap();
            root
        }

        fn plan(&mut self, name: &str) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.plan").unwrap(),
                    ToolStoreTarget::new(self.source),
                    PlanDotfilePackage::new(name),
                ))
                .id()
        }

        fn finish(&mut self, request: Entity) {
            let deadline = Instant::now() + Duration::from_secs(5);
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
                assert!(
                    Instant::now() < deadline,
                    "dotfile observation did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn adoption_moves_and_links_before_committing_configuration() {
        let mut fixture = Fixture::new();
        let source = fixture.store.home().join(".adopt-qa");
        std::fs::write(&source, b"original content").unwrap();
        let request = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.adoption").unwrap(),
                ToolStoreTarget::new(fixture.source),
                AdoptDotfile::new(&source, "shell"),
            ))
            .id();
        fixture.finish(request);
        let destination = fixture.store.dotfiles_dir().join("shell/.adopt-qa");
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationSucceeded>(request)
                .is_some(),
            "adoption failed: {:?}",
            fixture.app.world().get::<ToolOperationFailed>(request)
        );
        assert_eq!(std::fs::read(&destination).unwrap(), b"original content");
        assert!(source.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&source).unwrap(), b"original content");
        assert_eq!(
            vmux_path::PathIdentity::resolve(
                &fixture
                    .app
                    .world()
                    .get::<AdoptedDotfile>(request)
                    .unwrap()
                    .path
            ),
            vmux_path::PathIdentity::resolve(&destination)
        );
        assert_eq!(
            crate::ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .dotfiles
                .packages,
            ["shell"]
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn adoption_cancellation_after_move_restores_the_original_file_and_configuration() {
        for interruption in ["cancel", "store", "requester"] {
            let mut fixture = Fixture::new();
            let source = fixture.store.home().join(".adopt-qa");
            std::fs::write(&source, b"original content").unwrap();
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let destination = fixture.store.dotfiles_dir().join("shell/.adopt-qa");
            let request = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.adoption.cancel").unwrap(),
                    ToolStoreTarget::new(fixture.source),
                    AdoptDotfile::new(&source, "shell"),
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !destination.is_file() {
                fixture.app.update();
                assert!(Instant::now() < deadline, "adopt file was not moved");
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
                std::fs::read(&source).unwrap(),
                b"original content",
                "{interruption}"
            );
            assert!(source.symlink_metadata().unwrap().is_file());
            assert!(!destination.exists());
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before
            );
            assert!(fixture.app.world().get::<AdoptedDotfile>(request).is_none());
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn adoption_restores_the_file_while_published_configuration_recovery_is_blocked() {
        let mut fixture = Fixture::new();
        let source = fixture.store.home().join(".adopt-qa");
        let destination = fixture.store.dotfiles_dir().join("shell/.adopt-qa");
        let manifest = fixture.store.manifest_path();
        std::fs::write(&source, b"original content").unwrap();
        let before = crate::ToolsManifest::read(&manifest).unwrap();
        fixture.app.add_observer(
            |trigger: On<Add, Saving>, work: Query<&Work>, mut commands: Commands| {
                let owner = work.get(trigger.event_target()).unwrap();
                commands
                    .entity(owner.requester)
                    .insert(OperationState::Canceled);
            },
        );
        let blocked = manifest.clone();
        fixture
            .app
            .add_observer(move |_: On<Add, ConfigPublished>| {
                std::fs::remove_file(&blocked).unwrap();
                std::fs::create_dir(&blocked).unwrap();
            });
        let request = fixture
            .app
            .world_mut()
            .spawn((
                fixture.store.clone(),
                CliInvocation {
                    command: "tool.adopt".into(),
                    arguments: std::collections::BTreeMap::from([
                        ("path".into(), vec![source.clone().into_os_string()]),
                        ("package".into(), vec!["shell".into()]),
                    ]),
                },
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            if manifest.is_dir()
                && source.symlink_metadata().is_ok_and(|entry| entry.is_file())
                && !destination.exists()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "configuration failure blocked adopted-file restoration"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(std::fs::read(&source).unwrap(), b"original content");
        assert!(fixture.store.lease().is_none());
        assert!(fixture.app.world().get::<CliResult>(request).is_none());
        std::fs::remove_dir(&manifest).unwrap();
        fixture.finish(request);
        let deadline = Instant::now() + Duration::from_secs(5);
        while fixture.app.world().get::<CliResult>(request).is_none() {
            fixture.app.update();
            assert!(
                Instant::now() < deadline,
                "adoption CLI did not return after recovery"
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
        assert_eq!(crate::ToolsManifest::read(&manifest).unwrap(), before);
        assert!(fixture.app.world().get::<AdoptedDotfile>(request).is_none());
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn adoption_refuses_an_occupied_destination_without_replacing_either_file() {
        let mut fixture = Fixture::new();
        let source = fixture.store.home().join(".adopt-qa");
        let destination = fixture.store.dotfiles_dir().join("shell/.adopt-qa");
        std::fs::write(&source, b"original").unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"foreign").unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.adoption.refusal").unwrap(),
                ToolStoreTarget::new(fixture.source),
                AdoptDotfile::new(&source, "shell"),
            ))
            .id();
        fixture.finish(request);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(request)
                .is_some()
        );
        assert_eq!(std::fs::read(source).unwrap(), b"original");
        assert_eq!(std::fs::read(destination).unwrap(), b"foreign");
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn available_import_commits_selected_packages_and_is_idempotent() {
        let mut fixture = Fixture::new();
        fixture.package("shell");
        fixture.package("editor");
        fixture.package(".hidden");
        let mut manifest = crate::ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.set_package("npm", "preserved", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        for expected in [1, 0] {
            let request = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.available.import").unwrap(),
                    ToolStoreTarget::new(fixture.source),
                    ImportAvailableDotfiles,
                ))
                .id();
            fixture.finish(request);
            assert_eq!(
                fixture
                    .app
                    .world()
                    .get::<ImportedAvailableDotfiles>(request)
                    .unwrap()
                    .packages,
                expected
            );
            let saved = crate::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
            assert_eq!(saved.dotfiles.packages, ["editor", "shell"]);
            assert_eq!(saved.packages["npm"], ["preserved"]);
            assert_eq!(
                std::fs::read(fixture.store.dotfiles_dir().join("editor/.zshrc")).unwrap(),
                b"shell"
            );
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn queued_available_import_refuses_retired_owners_without_changing_configuration() {
        for interruption in ["cancel", "store", "requester"] {
            let mut fixture = Fixture::new();
            fixture.package("shell");
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let blocker = fixture.store.lease().unwrap();
            let request = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.available.refusal").unwrap(),
                    ToolStoreTarget::new(fixture.source),
                    ImportAvailableDotfiles,
                ))
                .id();
            fixture.app.update();
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
            drop(blocker);
            fixture.finish(request);
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before
            );
            assert!(
                fixture
                    .app
                    .world()
                    .get::<ImportedAvailableDotfiles>(request)
                    .is_none()
            );
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn link_and_disable_commit_only_after_native_links_complete() {
        let mut fixture = Fixture::new();
        let root = fixture.package("shell");
        let link = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.link").unwrap(),
                ToolStoreTarget::new(fixture.source),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(link);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<LinkedDotfilePackage>(link)
                .unwrap()
                .files,
            2
        );
        let manifest = crate::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        assert_eq!(manifest.dotfiles.packages, ["shell"]);
        for relative in [".zshrc", ".config/editor"] {
            let target = fixture.store.home().join(relative);
            let path = std::fs::read_link(&target).unwrap();
            assert!(!path.is_absolute());
            assert_eq!(
                vmux_path::PathIdentity::resolve(&target),
                vmux_path::PathIdentity::resolve(root.join(relative))
            );
        }
        let disable = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.disable").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(disable);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<DisabledDotfilePackage>(disable)
                .unwrap()
                .files,
            2
        );
        assert!(
            crate::ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .dotfiles
                .packages
                .is_empty()
        );
        assert!(
            fixture
                .store
                .home()
                .join(".zshrc")
                .symlink_metadata()
                .is_err()
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn interrupted_linking_rolls_back_owned_paths_before_releasing_the_store() {
        for interruption in ["cancel", "store", "requester", "foreign"] {
            let mut fixture = Fixture::new();
            fixture.package("shell");
            let before = std::fs::read(fixture.store.manifest_path()).unwrap();
            let request = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.interrupted.link").unwrap(),
                    ToolStoreTarget::new(fixture.source),
                    LinkDotfilePackage::new("shell"),
                ))
                .id();
            let target = fixture.store.home().join(".config/editor");
            let deadline = Instant::now() + Duration::from_secs(5);
            while target.symlink_metadata().is_err() {
                fixture.app.update();
                assert!(Instant::now() < deadline, "link was not created");
                std::thread::sleep(Duration::from_millis(1));
            }
            if interruption == "foreign" {
                std::fs::remove_file(&target).unwrap();
                std::fs::write(&target, b"foreign replacement").unwrap();
            }
            match interruption {
                "cancel" | "foreign" => {
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
            for relative in [".config/editor", ".zshrc"] {
                if interruption == "foreign" && relative == ".config/editor" {
                    assert_eq!(std::fs::read(&target).unwrap(), b"foreign replacement");
                    continue;
                }
                assert!(
                    fixture
                        .store
                        .home()
                        .join(relative)
                        .symlink_metadata()
                        .is_err(),
                    "{interruption}: {relative} survived rollback"
                );
            }
            assert_eq!(
                std::fs::read(fixture.store.manifest_path()).unwrap(),
                before,
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some(), "{interruption}");
            assert!(
                fixture
                    .app
                    .world()
                    .get::<LinkedDotfilePackage>(request)
                    .is_none(),
                "{interruption}"
            );
        }
    }

    #[test]
    fn conflicting_link_plans_preserve_configuration_and_foreign_targets() {
        let mut fixture = Fixture::new();
        fixture.package("shell");
        std::fs::write(fixture.store.home().join(".zshrc"), b"foreign").unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.conflict").unwrap(),
                ToolStoreTarget::new(fixture.source),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(request);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(request)
                .is_some()
        );
        assert_eq!(
            std::fs::read(fixture.store.home().join(".zshrc")).unwrap(),
            b"foreign"
        );
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(
            fixture
                .store
                .home()
                .join(".config/editor")
                .symlink_metadata()
                .is_err()
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn planning_observes_the_native_tree_and_refuses_foreign_link_authority() {
        let mut fixture = Fixture::new();
        let root = fixture.package("shell");
        std::fs::create_dir_all(fixture.store.home().join(".config")).unwrap();
        std::os::unix::fs::symlink(root.join(".zshrc"), fixture.store.home().join(".zshrc"))
            .unwrap();
        std::fs::write(fixture.store.home().join(".config/editor"), b"foreign").unwrap();
        std::fs::write(root.join("missing"), b"missing").unwrap();
        std::os::unix::fs::symlink(
            fixture.directory.path().join("outside"),
            root.join("ignored-link"),
        )
        .unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture.plan("shell");
        fixture.finish(request);
        let plan = fixture.app.world().get::<DotfilePlan>(request).unwrap();
        assert_eq!((plan.linked(), plan.missing(), plan.conflicts()), (1, 1, 1));
        assert_eq!(plan.links.len(), 3);
        assert_eq!(
            std::fs::read(fixture.store.home().join(".config/editor")).unwrap(),
            b"foreign"
        );
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn enabled_packages_are_all_preflighted_before_any_link_is_created() {
        let mut fixture = Fixture::new();
        fixture.package("shell");
        fixture.package("editor");
        std::fs::write(fixture.store.home().join(".zshrc"), b"foreign").unwrap();
        let mut manifest = crate::ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.set_dotfile_package("editor", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let request = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.apply.enabled").unwrap(),
                ToolStoreTarget::new(fixture.source),
                ApplyEnabledDotfiles,
            ))
            .id();
        fixture.finish(request);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(request)
                .is_some()
        );
        assert!(
            fixture
                .store
                .home()
                .join(".config/editor")
                .symlink_metadata()
                .is_err()
        );
        assert_eq!(
            std::fs::read(fixture.store.home().join(".zshrc")).unwrap(),
            b"foreign"
        );
        assert_eq!(
            std::fs::read(fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn dotfile_changes_preserve_brewfile_bytes() {
        let mut fixture = Fixture::new();
        fixture.package("shell");
        let brewfile = fixture.store.brewfile_path();
        let expected = b"# keep exact bytes\nbrew \"ripgrep\", link: false";
        std::fs::write(&brewfile, expected).unwrap();
        let mut manifest = crate::ToolsManifest::default();
        manifest.set_package("homebrew-formula", "ripgrep", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        let link = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.brewfile.link").unwrap(),
                ToolStoreTarget::new(fixture.source),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(link);
        assert!(
            fixture
                .app
                .world()
                .get::<LinkedDotfilePackage>(link)
                .is_some()
        );
        assert_eq!(std::fs::read(&brewfile).unwrap(), expected);
        let disable = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.brewfile.disable").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(disable);
        assert!(
            fixture
                .app
                .world()
                .get::<DisabledDotfilePackage>(disable)
                .is_some()
        );
        assert_eq!(std::fs::read(&brewfile).unwrap(), expected);
        assert!(
            crate::ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .dotfiles
                .packages
                .is_empty()
        );
    }

    #[test]
    fn disable_clears_a_package_replaced_by_an_ordinary_file() {
        let mut fixture = Fixture::new();
        std::fs::create_dir_all(fixture.store.dotfiles_dir()).unwrap();
        std::fs::write(fixture.store.dotfiles_dir().join("shell"), b"foreign root").unwrap();
        let mut manifest = crate::ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.write_to(&fixture.store.manifest_path()).unwrap();
        let disable = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.file-root.disable").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(disable);
        assert!(
            fixture
                .app
                .world()
                .get::<DisabledDotfilePackage>(disable)
                .is_some()
        );
        assert_eq!(
            std::fs::read(fixture.store.dotfiles_dir().join("shell")).unwrap(),
            b"foreign root"
        );
        assert!(
            crate::ToolsManifest::read(&fixture.store.manifest_path())
                .unwrap()
                .dotfiles
                .packages
                .is_empty()
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn relative_store_paths_share_one_absolute_native_context() {
        let cwd = std::env::current_dir().unwrap();
        let directory = tempfile::tempdir_in(&cwd).unwrap();
        let relative = directory.path().strip_prefix(&cwd).unwrap();
        let store = ToolStore::new(relative.join("config"), relative.join("home"));
        let root = directory.path().join("config/tools/dotfiles/shell");
        let home = directory.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(root.join(".zshrc"), b"shell").unwrap();
        crate::ToolsManifest::default()
            .write_to(&store.manifest_path())
            .unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            crate::ToolRuntimePlugin,
            DotfileOperationsPlugin,
        ));
        let request = app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.relative.link").unwrap(),
                store.clone(),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().get::<LinkedDotfilePackage>(request).is_none() {
            app.update();
            assert!(app.world().get::<ToolOperationFailed>(request).is_none());
            assert!(Instant::now() < deadline, "relative link did not complete");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(std::fs::read(home.join(".zshrc")).unwrap(), b"shell");
        assert_eq!(
            vmux_path::PathIdentity::resolve(home.join(".zshrc")),
            vmux_path::PathIdentity::resolve(root.join(".zshrc"))
        );
        assert!(
            crate::ToolsManifest::read(&store.manifest_path())
                .unwrap()
                .dotfiles
                .packages
                .contains(&"shell".to_string())
        );
        assert!(store.lease().is_some());
    }

    #[test]
    fn linking_revalidates_previously_linked_targets_before_commit() {
        for foreign in [false, true] {
            let mut fixture = Fixture::new();
            let root = fixture.package("shell");
            let target = fixture.store.home().join(".zshrc");
            std::os::unix::fs::symlink(root.join(".zshrc"), &target).unwrap();
            let replacement = target.clone();
            fixture
                .app
                .add_observer(move |trigger: On<Add, Changing>, files: Query<&Source>| {
                    if files.get(trigger.event_target()).is_ok_and(|source| {
                        source.0.file_name().is_some_and(|name| name == "editor")
                    }) {
                        std::fs::remove_file(&replacement).unwrap();
                        if foreign {
                            std::fs::write(&replacement, b"foreign replacement").unwrap();
                        }
                    }
                });
            let link = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.preflight.revalidation").unwrap(),
                    ToolStoreTarget::new(fixture.source),
                    LinkDotfilePackage::new("shell"),
                ))
                .id();
            fixture.finish(link);
            if foreign {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<ToolOperationFailed>(link)
                        .is_some()
                );
                assert_eq!(std::fs::read(&target).unwrap(), b"foreign replacement");
                assert!(
                    crate::ToolsManifest::read(&fixture.store.manifest_path())
                        .unwrap()
                        .dotfiles
                        .packages
                        .is_empty()
                );
                assert!(
                    fixture
                        .store
                        .home()
                        .join(".config/editor")
                        .symlink_metadata()
                        .is_err()
                );
            } else {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<LinkedDotfilePackage>(link)
                        .is_some()
                );
                assert_eq!(std::fs::read(&target).unwrap(), b"shell");
                assert!(
                    crate::ToolsManifest::read(&fixture.store.manifest_path())
                        .unwrap()
                        .dotfiles
                        .packages
                        .contains(&"shell".to_string())
                );
            }
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn failed_manifest_publication_restores_links_without_another_configuration_write() {
        let mut fixture = Fixture::new();
        let root = fixture.package("shell");
        let link = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.failed-save.setup").unwrap(),
                ToolStoreTarget::new(fixture.source),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(link);
        let manifest_path = fixture.store.manifest_path();
        let manifest = manifest_path.clone();
        fixture.app.add_observer(move |_: On<Add, Saving>| {
            std::fs::remove_file(&manifest).unwrap();
            std::fs::create_dir(&manifest).unwrap();
        });
        let disable = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.failed-save.disable").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(disable);
        assert!(
            fixture
                .app
                .world()
                .get::<ToolOperationFailed>(disable)
                .is_some()
        );
        assert!(manifest_path.is_dir());
        for path in [".zshrc", ".config/editor"] {
            assert_eq!(
                vmux_path::PathIdentity::resolve(fixture.store.home().join(path)),
                vmux_path::PathIdentity::resolve(root.join(path))
            );
        }
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn failed_rollback_retries_when_the_waiter_was_already_queued() {
        let mut fixture = Fixture::new();
        let root = fixture.package("shell");
        let link = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.retry.setup").unwrap(),
                ToolStoreTarget::new(fixture.source),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(link);
        let before = std::fs::read(fixture.store.manifest_path()).unwrap();
        let manifest = fixture.store.manifest_path();
        let target = fixture.store.home().join(".zshrc");
        let store = fixture.store.clone();
        let waiter = std::sync::Arc::new(std::sync::Mutex::new(None));
        let queued = waiter.clone();
        fixture
            .app
            .add_observer(move |_: On<Add, Saving>, mut commands: Commands| {
                std::fs::remove_file(&manifest).unwrap();
                std::fs::create_dir(&manifest).unwrap();
                std::fs::write(&target, b"rollback obstruction").unwrap();
                let request = commands
                    .spawn((
                        Operation::bundle("test.dotfile.retry.waiter").unwrap(),
                        store.clone(),
                        PlanDotfilePackage::new("shell"),
                    ))
                    .id();
                *queued.lock().unwrap() = Some(request);
            });
        let disable = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.retry.disable").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while fixture
            .app
            .world()
            .get::<ToolOperationFailed>(disable)
            .is_none()
        {
            fixture.app.update();
            assert!(
                Instant::now() < deadline,
                "rollback did not report its obstruction"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let queued = waiter.lock().unwrap().unwrap();
        assert!(fixture.app.world().get::<DotfilePlan>(queued).is_none());
        assert!(fixture.store.lease().is_none());
        std::fs::remove_file(fixture.store.home().join(".zshrc")).unwrap();
        std::fs::remove_dir(fixture.store.manifest_path()).unwrap();
        std::fs::write(fixture.store.manifest_path(), before).unwrap();
        fixture.finish(disable);
        fixture.finish(queued);
        let plan = fixture.app.world().get::<DotfilePlan>(queued).unwrap();
        assert_eq!((plan.linked(), plan.missing(), plan.conflicts()), (2, 0, 0));
        for path in [".zshrc", ".config/editor"] {
            assert_eq!(
                vmux_path::PathIdentity::resolve(fixture.store.home().join(path)),
                vmux_path::PathIdentity::resolve(root.join(path))
            );
        }
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn published_configuration_failure_does_not_block_link_restoration() {
        let mut fixture = Fixture::new();
        let root = fixture.package("shell");
        let link = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.config-recovery.setup").unwrap(),
                ToolStoreTarget::new(fixture.source),
                LinkDotfilePackage::new("shell"),
            ))
            .id();
        fixture.finish(link);
        let before = crate::ToolsManifest::read(&fixture.store.manifest_path()).unwrap();
        fixture.app.add_observer(
            |trigger: On<Add, Saving>, work: Query<&Work>, mut commands: Commands| {
                let owner = work.get(trigger.event_target()).unwrap();
                commands
                    .entity(owner.requester)
                    .insert(OperationState::Canceled);
            },
        );
        let manifest = fixture.store.manifest_path();
        fixture
            .app
            .add_observer(move |_: On<Add, ConfigPublished>| {
                std::fs::remove_file(&manifest).unwrap();
                std::fs::create_dir(&manifest).unwrap();
            });
        let disable = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.config-recovery.disable").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DisableDotfilePackage::new("shell"),
            ))
            .id();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            fixture.app.update();
            let restored = [".zshrc", ".config/editor"]
                .iter()
                .all(|path| fixture.store.home().join(path).symlink_metadata().is_ok());
            if restored
                && fixture
                    .app
                    .world()
                    .get::<ToolOperationFailed>(disable)
                    .is_some()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "configuration failure blocked link restoration"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(fixture.store.manifest_path().is_dir());
        assert!(fixture.store.lease().is_none());
        for path in [".zshrc", ".config/editor"] {
            assert_eq!(
                vmux_path::PathIdentity::resolve(fixture.store.home().join(path)),
                vmux_path::PathIdentity::resolve(root.join(path))
            );
        }
        std::fs::remove_dir(fixture.store.manifest_path()).unwrap();
        fixture.finish(disable);
        assert_eq!(
            crate::ToolsManifest::read(&fixture.store.manifest_path()).unwrap(),
            before
        );
        assert!(fixture.store.lease().is_some());
    }

    #[test]
    fn discovery_distinguishes_a_missing_catalog_from_a_non_directory() {
        for occupied in [false, true] {
            let mut fixture = Fixture::new();
            if occupied {
                std::fs::write(fixture.store.dotfiles_dir(), b"foreign catalog").unwrap();
            }
            let request = fixture
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("test.dotfile.catalog.type").unwrap(),
                    ToolStoreTarget::new(fixture.source),
                    DiscoverDotfilePackages,
                ))
                .id();
            fixture.finish(request);
            if occupied {
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
                        .get::<DiscoveredDotfilePackages>(request)
                        .is_none()
                );
                assert_eq!(
                    std::fs::read(fixture.store.dotfiles_dir()).unwrap(),
                    b"foreign catalog"
                );
            } else {
                assert!(
                    fixture
                        .app
                        .world()
                        .get::<DiscoveredDotfilePackages>(request)
                        .unwrap()
                        .packages
                        .is_empty()
                );
                assert!(!fixture.store.dotfiles_dir().exists());
            }
            assert!(fixture.store.lease().is_some());
        }
    }

    #[test]
    fn discovery_materializes_only_valid_native_package_directories() {
        let mut fixture = Fixture::new();
        fixture.package("shell");
        fixture.package("editor");
        fixture.package(".hidden");
        std::fs::write(fixture.store.dotfiles_dir().join("ordinary-file"), b"file").unwrap();
        let request = fixture
            .app
            .world_mut()
            .spawn((
                Operation::bundle("test.dotfile.discover").unwrap(),
                ToolStoreTarget::new(fixture.source),
                DiscoverDotfilePackages,
            ))
            .id();
        fixture.finish(request);
        assert_eq!(
            fixture
                .app
                .world()
                .get::<DiscoveredDotfilePackages>(request)
                .unwrap()
                .packages,
            ["editor", "shell"]
        );
    }

    #[test]
    fn queued_planning_rejects_cancelled_replaced_and_deleted_requesters() {
        for interruption in ["cancel", "store", "requester"] {
            let mut fixture = Fixture::new();
            fixture.package("shell");
            let blocker = fixture.store.lease().unwrap();
            let request = fixture.plan("shell");
            fixture.app.update();
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
            drop(blocker);
            fixture.finish(request);
            assert!(
                fixture.app.world().get::<DotfilePlan>(request).is_none(),
                "{interruption}"
            );
            assert!(fixture.store.lease().is_some(), "{interruption}");
        }
    }
}
