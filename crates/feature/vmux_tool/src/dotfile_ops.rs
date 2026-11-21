use std::marker::PhantomData;
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::{
    EntityTarget, JobRequest, JobResult, Operation, OperationRetained, OperationState,
    OperationTarget,
};

use crate::dotfiles::{
    DiscoverDotfilePackages, DiscoveredDotfilePackages, DotfileLink, DotfileLinkState, DotfilePlan,
    PlanDotfilePackage,
};
use crate::io::{InspectLink, ReadDirectory};
use crate::storage::{ReadRequest, ReadResult, StoreLease};
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
                ),
                (
                    cancel::<PlanDotfilePackage>,
                    cancel::<DiscoverDotfilePackages>,
                ),
                begin,
                loaded,
                (plan, discover),
                directories,
                inspected,
                planned,
                discovered,
            )
                .chain()
                .in_set(ToolTaskSet::Start),
        )
        .add_systems(PostUpdate, finish.after(vmux_ecs::JobSet::Cleanup));
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
    work: Query<(Entity, &Work, &Captured<R>, &ToolStore, &OperationState)>,
    requests: Query<(
        Option<&R>,
        Option<&ToolStoreTarget>,
        Option<&ToolStore>,
        Option<&OperationState>,
    )>,
    stores: Query<&ToolStore>,
    mut commands: Commands,
) {
    for (entity, original, captured, store, state) in &work {
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
        if !valid {
            commands.entity(entity).insert((
                OperationState::Canceled,
                Failure("dotfile operation source changed".into()),
            ));
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
            Ok(_) => {
                commands.entity(entity).insert(Loaded);
            }
            Err(error) => {
                commands.entity(entity).insert(Failure(error.clone()));
            }
        }
    }
}

fn plan(
    work: Query<
        (
            Entity,
            &Captured<PlanDotfilePackage>,
            &ToolStore,
            &OperationState,
        ),
        (With<Loaded>, Without<Selected>, Without<Failure>),
    >,
    mut commands: Commands,
) {
    for (entity, request, store, state) in &work {
        if *state != OperationState::Running {
            continue;
        }
        let package = &request.0.package;
        if !Package::valid(package) {
            commands
                .entity(entity)
                .insert(Failure(format!("invalid dotfile package name: {package}")));
            continue;
        }
        let root = store.dotfiles_dir().join(package);
        let package = commands
            .spawn((
                Name::new(package.clone()),
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
    mut work: Query<(&ToolStore, &OperationState, &mut EntryCount)>,
    mut commands: Commands,
) {
    for (entity, owner, package, directory, result) in &directories {
        commands
            .entity(entity)
            .insert(DirectoryDone)
            .remove::<JobResult<ReadDirectory>>();
        let Ok((store, state, mut count)) = work.get_mut(owner.parent()) else {
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
        (Entity, &Captured<PlanDotfilePackage>, &OperationState),
        (With<Selected>, Without<Ready>, Without<Failure>),
    >,
    directories: Query<(&ChildOf, Has<DirectoryDone>), With<Directory>>,
    files: Query<(&ChildOf, &Source, &Target, Option<&LinkState>), With<File>>,
    mut commands: Commands,
) {
    for (entity, request, state) in &work {
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
        links.sort_by(|left, right| left.source.cmp(&right.source));
        commands.entity(entity).insert((
            Ready,
            DotfilePlan {
                package: request.0.package.clone(),
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

fn finish(
    work: Query<(
        Entity,
        &Work,
        &OperationState,
        Option<&Children>,
        Option<&Failure>,
        Option<&DotfilePlan>,
        Option<&DiscoveryOutput>,
        Has<Ready>,
        Has<ReadRequest>,
    )>,
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
