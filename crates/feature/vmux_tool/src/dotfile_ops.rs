use std::marker::PhantomData;
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::{
    EntityTarget, JobRequest, JobResult, Operation, OperationRetained, OperationState,
    OperationTarget,
};

use crate::dotfiles::{
    AppliedEnabledDotfiles, ApplyEnabledDotfiles, DisableDotfilePackage, DisabledDotfilePackage,
    DiscoverDotfilePackages, DiscoveredDotfilePackages, DotfileLink, DotfileLinkState, DotfilePlan,
    LinkDotfilePackage, LinkedDotfilePackage, PlanDotfilePackage, UnlinkDotfilePackage,
    UnlinkedDotfilePackage,
};
use crate::io::{InspectLink, LinkPath, PathReceipt, ReadDirectory, UnlinkPath};
use crate::storage::{ReadRequest, ReadResult, StoreLease, WriteRequest, WriteResult};
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
                ),
                (
                    cancel::<PlanDotfilePackage>,
                    cancel::<DiscoverDotfilePackages>,
                    cancel::<LinkDotfilePackage>,
                    cancel::<DisableDotfilePackage>,
                    cancel::<UnlinkDotfilePackage>,
                    cancel::<ApplyEnabledDotfiles>,
                ),
                begin,
                loaded,
                (
                    select_plan,
                    select_link,
                    select_disable,
                    select_unlink,
                    select_apply,
                ),
                (plan, discover),
                directories,
                inspected,
                planned,
                discovered,
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
                restored_config,
                (
                    reverted::<LinkPath, Linking>,
                    reverted::<UnlinkPath, Unlinking>,
                ),
                mutation_completed,
            )
                .chain()
                .in_set(ToolTaskSet::Start),
        )
        .add_systems(
            PostUpdate,
            (finish, finish_mutations).after(vmux_ecs::JobSet::Cleanup),
        )
        .add_observer(cleanup_failed);
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
struct SaveStarted;
#[derive(Component)]
struct Saved;
#[derive(Component)]
struct Rollback;
#[derive(Component)]
struct RestoringConfig;
#[derive(Component)]
struct ConfigRestored;
#[derive(Component)]
struct CleanupFailed(String);
#[derive(Component)]
struct MutationFinished(usize);
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
        ),
        (Without<Accepted<R>>, Without<ToolOperationFinished>),
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
        commands.spawn((
            Name::new("Observe dotfile operation"),
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
        (Entity, &Work, &ToolStore, &OperationState),
        (Without<Loading>, Without<Loaded>, Without<Failure>),
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    for (entity, original, store, state) in &work {
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

fn plan(
    work: Query<
        (Entity, &Names, &ToolStore, &OperationState),
        (With<Loaded>, Without<Selected>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, names, store, state) in &work {
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
            let root = store.dotfiles_dir().join(name);
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
                JobRequest(ReadDirectory { path: root }),
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
        (Entity, &ToolStore, &OperationState),
        (
            With<Captured<DiscoverDotfilePackages>>,
            With<Loaded>,
            Without<Selected>,
            Without<Failure>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, store, state) in &work {
        if *state == OperationState::Running {
            commands.entity(entity).insert((
                Selected,
                Discovery,
                JobRequest(ReadDirectory {
                    path: store.dotfiles_dir(),
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
    mut work: Query<(&ToolStore, &OperationState, &mut EntryCount, Has<Disable>)>,
    mut commands: Commands,
) {
    for (entity, owner, package, directory, result) in &directories {
        commands
            .entity(entity)
            .insert(DirectoryDone)
            .remove::<JobResult<ReadDirectory>>();
        let Ok((store, state, mut count, disabling)) = work.get_mut(owner.parent()) else {
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
        if !entries.exists {
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
                    }),
                ));
            } else if entry.file_type.is_file() {
                let Ok(relative) = entry.path.strip_prefix(&root.0) else {
                    continue;
                };
                let target = store.home().join(relative);
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
        (With<Selected>, Without<Ready>, Without<Failure>),
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

fn retry_cleanup(
    added: Query<(Entity, &ToolStore), (With<Work>, Added<Work>)>,
    failed: Query<(Entity, &ToolStore), With<CleanupFailed>>,
    mut commands: Commands,
) {
    for (new, store) in &added {
        for (old, captured) in &failed {
            if new != old && store == captured {
                commands.entity(old).remove::<CleanupFailed>();
            }
        }
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

fn cancel_files(
    work: Query<(Entity, &OperationState, Has<Failure>, Has<Rollback>), With<Work>>,
    mut files: Query<
        (&ChildOf, &mut OperationState),
        (Or<(With<File>, With<Directory>)>, Without<Work>),
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
        (Entity, &OperationState, Has<RemoveLinks>),
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
    for (entity, state, removing) in &work {
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
            || !removing && state.0 != DotfileLinkState::Missing
        {
            commands.entity(file).insert(EffectDone);
            continue;
        }
        let receipt = PathReceipt::default();
        let mut file = commands.entity(file);
        file.insert((Changing, receipt.clone(), OperationState::Running));
        if removing {
            file.insert((
                Unlinking,
                JobRequest(UnlinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    receipt,
                }),
            ));
        } else {
            file.insert((
                Linking,
                JobRequest(LinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
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
            SaveStarted,
            WriteRequest {
                manifest: desired,
                retain_lease: true,
            },
        ));
    }
}

fn saved(work: Query<(Entity, &WriteResult), With<Saving>>, mut commands: Commands) {
    for (entity, result) in &work {
        commands.entity(entity).remove::<(Saving, WriteResult)>();
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
            Option<&Children>,
            Has<SaveStarted>,
            Has<ConfigRestored>,
            Has<RestoringConfig>,
            Has<RemoveLinks>,
            Has<ReadRequest>,
            Has<WriteRequest>,
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
    )>,
    mut commands: Commands,
) {
    for (entity, before, children, saved, restored, restoring, removing, reading, writing) in &work
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
        if saved && !restored {
            if !restoring {
                commands.entity(entity).insert((
                    RestoringConfig,
                    WriteRequest {
                        manifest: before.0.clone(),
                        retain_lease: true,
                    },
                ));
            }
            continue;
        }
        let mut next = None;
        let mut active = false;
        for (file, owner, source, _, changed, reverting, reverted) in &files {
            if owner.parent() != entity || !changed || reverted {
                continue;
            }
            if reverting {
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
        file.insert((Reverting, OperationState::Running, receipt.clone()));
        if removing {
            file.insert((
                Linking,
                JobRequest(LinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
                    receipt,
                }),
            ));
        } else {
            file.insert((
                Unlinking,
                JobRequest(UnlinkPath {
                    source: source.0.clone(),
                    target: target.0.clone(),
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
                commands.entity(entity).insert(CleanupFailed(error.clone()));
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

fn mutation_completed(
    work: Query<
        (
            Entity,
            Option<&Children>,
            Has<Failure>,
            Has<SaveStarted>,
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
    files: Query<(&ChildOf, Has<Changed>, Has<Reverted>, Has<EffectDone>), With<File>>,
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
        for (owner, changed, reverted, done) in &files {
            if owner.parent() != entity {
                continue;
            }
            count += usize::from(changed);
            pending |= if failed { changed && !reverted } else { !done };
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
        (With<Mutation>, Without<CleanupFailed>),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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
                crate::ToolRuntimePlugin,
                DotfileOperationsPlugin,
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
                vmux_path::PathIdentity::resolve(&root.join(relative))
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
