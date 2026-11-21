use super::install_io::{Remove, Unlink};
use super::install_runtime::{ConfigurationPending, Lease, Released};
use super::package_path::PackageName;
use super::store::{PackageStore, Receipt};
use bevy::prelude::*;
use std::collections::HashSet;
use vmux_ecs::{
    JobPlugin, JobRequest, JobResult, Operation, OperationRetained, OperationState, OperationTarget,
};
use vmux_tool::io::ReadFile;

pub(super) struct RemovePlugin;

impl Plugin for RemovePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_tool::io::ObservationIoPlugin>() {
            app.add_plugins(vmux_tool::io::ObservationIoPlugin);
        }
        if !app.is_plugin_added::<JobPlugin<Remove>>() {
            app.add_plugins(JobPlugin::<Remove>::default());
        }
        if !app.is_plugin_added::<JobPlugin<Unlink>>() {
            app.add_plugins(JobPlugin::<Unlink>::default());
        }
        app.add_systems(
            Update,
            (cancel, begin, read, unlinked, removed)
                .chain()
                .after(super::install_runtime::InstallSet)
                .before(vmux_ecs::JobSet::Submit),
        )
        .add_systems(
            PostUpdate,
            (drain, wake).chain().after(vmux_ecs::JobSet::Cleanup),
        );
    }
}

#[derive(Component)]
pub(super) struct Request {
    pub store: PackageStore,
    pub name: PackageName,
}

#[derive(Component)]
pub(super) struct Completed(pub Result<(), String>);

#[derive(Component)]
struct Reading;
#[derive(Component)]
struct Unlinking;
#[derive(Component)]
struct Deleting;
#[derive(Component)]
struct UnlinkFor(Entity);

fn cancel(
    requests: Query<(Entity, &OperationState), With<Request>>,
    children: Query<(Entity, &UnlinkFor)>,
    mut commands: Commands,
) {
    for (owner, state) in &requests {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        for (entity, parent) in &children {
            if parent.0 == owner {
                commands.entity(entity).try_insert(OperationState::Canceled);
            }
        }
    }
}

type BeginRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Request, &'static OperationState),
    (Without<Lease>, Without<Completed>, Without<Released>),
>;

fn begin(requests: BeginRequests<'_, '_>, leases: Query<&Lease>, mut commands: Commands) {
    let mut busy = HashSet::new();
    for lease in &leases {
        busy.insert(lease.0.clone());
    }
    for (entity, request, state) in &requests {
        if !matches!(state, OperationState::Pending | OperationState::Running)
            || !busy.insert(request.store.path().to_path_buf())
        {
            continue;
        }
        commands.entity(entity).insert((
            Lease(request.store.path().to_path_buf()),
            Reading,
            OperationState::Running,
            JobRequest(ReadFile {
                path: request
                    .store
                    .package_dir(&request.name)
                    .join("vmux-receipt.json"),
                limit: 1024 * 1024,
            }),
        ));
    }
}

fn read(
    requests: Query<(Entity, &Request, &JobResult<ReadFile>, &OperationState), With<Reading>>,
    mut commands: Commands,
) {
    for (entity, request, result, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Reading, JobResult<ReadFile>)>();
        if *state != OperationState::Running {
            continue;
        }
        let bytes = match &result.0 {
            Ok(Ok(bytes)) => &bytes.0,
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((Completed(Err(error.clone())), OperationState::Failed));
                continue;
            }
        };
        if let Some(bytes) = bytes {
            let receipt = match serde_json::from_slice::<Receipt>(bytes) {
                Ok(receipt) => receipt,
                Err(error) => {
                    commands
                        .entity(entity)
                        .insert((Completed(Err(error.to_string())), OperationState::Failed));
                    continue;
                }
            };
            if receipt.name != request.name {
                commands.entity(entity).insert((
                    Completed(Err("package receipt owner differs".into())),
                    OperationState::Failed,
                ));
                continue;
            }
            for (name, file) in &receipt.bin {
                commands.spawn((
                    UnlinkFor(entity),
                    Operation::bundle("lsp.package.unlink").unwrap().running(),
                    OperationRetained,
                    OperationTarget::new(entity),
                    JobRequest(Unlink {
                        path: request.store.bin_dir().join(name.as_str()),
                        expected: request
                            .store
                            .package_dir(&request.name)
                            .join(file.as_path()),
                    }),
                ));
            }
            commands.entity(entity).insert(receipt);
        }
        commands.entity(entity).insert(Unlinking);
    }
}

fn unlinked(
    requests: Query<(Entity, &Request, &OperationState), With<Unlinking>>,
    links: Query<(Entity, &UnlinkFor, Option<&JobResult<Unlink>>)>,
    mut commands: Commands,
) {
    for (entity, request, state) in &requests {
        if *state != OperationState::Running {
            continue;
        }
        let mut pending = false;
        let mut failure = None;
        for (_, owner, result) in &links {
            if owner.0 != entity {
                continue;
            }
            match result {
                None => pending = true,
                Some(JobResult(Ok(Ok(())))) => {}
                Some(JobResult(Ok(Err(error)) | Err(error))) => failure = Some(error.clone()),
            }
        }
        if pending {
            continue;
        }
        for (link, owner, _) in &links {
            if owner.0 == entity {
                commands.entity(link).despawn();
            }
        }
        commands.entity(entity).remove::<Unlinking>();
        if let Some(error) = failure {
            commands
                .entity(entity)
                .insert((Completed(Err(error)), OperationState::Failed));
        } else {
            commands.entity(entity).insert((
                Deleting,
                JobRequest(Remove {
                    store: request.store.clone(),
                    name: request.name.clone(),
                }),
            ));
        }
    }
}

fn removed(
    requests: Query<(Entity, &JobResult<Remove>, &OperationState), With<Deleting>>,
    mut commands: Commands,
) {
    for (entity, result, state) in &requests {
        commands
            .entity(entity)
            .remove::<(Deleting, JobResult<Remove>)>();
        if *state != OperationState::Running {
            continue;
        }
        match &result.0 {
            Ok(Ok(())) => {
                commands
                    .entity(entity)
                    .insert((Completed(Ok(())), OperationState::Succeeded));
            }
            Ok(Err(error)) | Err(error) => {
                commands
                    .entity(entity)
                    .insert((Completed(Err(error.clone())), OperationState::Failed));
            }
        }
    }
}

type DrainRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static OperationState,
        Has<Completed>,
        Has<ConfigurationPending>,
    ),
    (With<Request>, Without<Released>),
>;

fn drain(
    requests: DrainRequests<'_, '_>,
    captured: Query<&OperationTarget, With<vmux_ecs::jobs::JobCapture>>,
    links: Query<(Entity, &UnlinkFor)>,
    mut commands: Commands,
) {
    for (entity, state, completed, configuration) in &requests {
        if matches!(state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        if captured.iter().any(|target| {
            target.entity() == entity
                || links
                    .get(target.entity())
                    .is_ok_and(|(_, owner)| owner.0 == entity)
        }) {
            continue;
        }
        for (link, owner) in &links {
            if owner.0 == entity {
                commands.entity(link).despawn();
            }
        }
        let mut request = commands.entity(entity);
        if !completed {
            request.insert(Completed(Err("LSP package removal canceled".into())));
        }
        if !configuration {
            request.remove::<Lease>();
        }
        request.insert(Released);
    }
}

type WakeReady<'w, 's> = Query<'w, 's, Entity, Or<(Added<Released>, Added<Completed>)>>;

fn wake(ready: WakeReady<'_, '_>, wake: Option<Res<vmux_ecs::JobWake>>) {
    if !ready.is_empty()
        && let Some(wake) = wake
    {
        wake.0.wake_by_ref();
    }
}

#[cfg(test)]
mod tests {
    use super::super::package_path::PackagePath;
    use super::*;
    use bevy::prelude::{App, Entity};
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    struct Fixture {
        app: App,
        directory: tempfile::TempDir,
        store: PackageStore,
        operation: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = PackageStore::at(directory.path());
            let name = PackageName::parse("server").unwrap();
            std::fs::create_dir_all(store.package_dir(&name)).unwrap();
            std::fs::write(store.package_dir(&name).join("server"), b"package bytes").unwrap();
            let mut app = App::new();
            app.add_plugins(RemovePlugin);
            let operation = app
                .world_mut()
                .spawn((
                    Operation::bundle("lsp.removal.fixture").unwrap(),
                    OperationRetained,
                    Request {
                        store: store.clone(),
                        name,
                    },
                ))
                .id();
            Self {
                app,
                directory,
                store,
                operation,
            }
        }

        fn finish(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.app.world().get::<Released>(self.operation).is_none() {
                self.app.update();
                assert!(Instant::now() < deadline, "LSP removal did not drain");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn receipt(&self, name: &str, links: &[&str]) {
            let name = PackageName::parse(name).unwrap();
            let mut bin = BTreeMap::new();
            for link in links {
                bin.insert(
                    PackageName::parse(link).unwrap(),
                    PackagePath::parse("server").unwrap(),
                );
            }
            Receipt {
                name,
                version: Some("1".into()),
                source_id: "fixture".into(),
                bin,
            }
            .write_to(
                &self
                    .store
                    .package_dir(&PackageName::parse("server").unwrap()),
            )
            .unwrap();
        }
    }

    #[test]
    fn removal_unlinks_owned_bins_and_preserves_replaced_entries() {
        let mut fixture = Fixture::new();
        fixture.receipt("server", &["owned", "foreign", "regular"]);
        std::fs::create_dir_all(fixture.store.bin_dir()).unwrap();
        let name = PackageName::parse("server").unwrap();
        let owned = fixture.store.bin_dir().join("owned");
        let foreign = fixture.store.bin_dir().join("foreign");
        let regular = fixture.store.bin_dir().join("regular");
        let other = fixture.directory.path().join("foreign-executable");
        std::fs::write(&other, b"foreign bytes").unwrap();
        std::os::unix::fs::symlink(fixture.store.package_dir(&name).join("server"), &owned)
            .unwrap();
        std::os::unix::fs::symlink(&other, &foreign).unwrap();
        std::fs::write(&regular, b"regular bytes").unwrap();
        fixture.finish();
        assert!(
            fixture
                .app
                .world()
                .get::<Completed>(fixture.operation)
                .unwrap()
                .0
                .is_ok()
        );
        assert!(!owned.is_symlink());
        assert_eq!(std::fs::read_link(foreign).unwrap(), other);
        assert_eq!(std::fs::read(regular).unwrap(), b"regular bytes");
        assert!(!fixture.store.package_dir(&name).exists());
    }

    #[test]
    fn a_mismatched_receipt_refuses_removal_and_preserves_bytes() {
        let mut fixture = Fixture::new();
        fixture.receipt("different-owner", &["server"]);
        fixture.finish();
        assert!(
            fixture
                .app
                .world()
                .get::<Completed>(fixture.operation)
                .unwrap()
                .0
                .is_err()
        );
        assert_eq!(
            std::fs::read(
                fixture
                    .store
                    .package_dir(&PackageName::parse("server").unwrap())
                    .join("server")
            )
            .unwrap(),
            b"package bytes"
        );
    }

    #[test]
    fn canceled_queued_removal_never_deletes_the_package_after_the_lane_opens() {
        let mut fixture = Fixture::new();
        fixture.receipt("server", &[]);
        fixture.app.insert_resource(vmux_ecs::BlockingRuntime::new(
            vmux_ecs::blocking::BlockingPool::new("removal-short", 1, 4),
            vmux_ecs::blocking::BlockingPool::new("removal-long", 1, 4),
            vmux_ecs::blocking::BlockingPool::new("removal-process", 1, 4),
        ));
        let (release, wait) = std::sync::mpsc::channel();
        let (started, entered) = std::sync::mpsc::sync_channel(1);
        let held = fixture
            .app
            .world()
            .resource::<vmux_ecs::BlockingRuntime>()
            .try_spawn(vmux_ecs::blocking::Lane::Long, None, move || {
                started.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while fixture
            .app
            .world()
            .get::<Deleting>(fixture.operation)
            .is_none()
        {
            fixture.app.update();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.operation)
            .insert(OperationState::Canceled);
        fixture.app.update();
        release.send(()).unwrap();
        fixture.finish();
        assert_eq!(
            std::fs::read(
                fixture
                    .store
                    .package_dir(&PackageName::parse("server").unwrap())
                    .join("server")
            )
            .unwrap(),
            b"package bytes"
        );
        assert!(
            fixture
                .app
                .world()
                .get::<Completed>(fixture.operation)
                .unwrap()
                .0
                .is_err()
        );
        drop(held);
    }
}
