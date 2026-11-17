use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::task::Waker;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use vmux_ecs::blocking::{BlockingRuntime, BlockingTask, Lane, SpawnError};
use vmux_ecs::{Operation, OperationState};

use crate::host::fold_preferences::{
    FileId, FoldSet, LoadCompleted, LoadRequest, Root, SaveCompleted, SaveRequest, Scope, Snapshot,
    Ticket,
};

pub struct FilesystemPlugin;

impl Plugin for FilesystemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockingRuntime>()
            .add_systems(Update, (loaded, saved).before(FoldSet::Restore))
            .add_systems(
                Update,
                (queue_loads, queue_saves, load, save)
                    .chain()
                    .after(FoldSet::Save)
                    .before(FoldSet::Project),
            );
    }
}

#[derive(Resource, Clone)]
pub struct FileWake(pub Waker);

impl FileWake {
    fn notify(&self) {
        let _ = catch_unwind(AssertUnwindSafe(|| self.0.wake_by_ref()));
    }
}

pub struct FileStore;

impl FileStore {
    pub fn read(path: &Path) -> Result<Snapshot, String> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Snapshot::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        ron::from_str(&source).map_err(|error| error.to_string())
    }

    pub fn write(path: &Path, snapshot: &Snapshot) -> Result<(), String> {
        let source = ron::ser::to_string(snapshot).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(path, source.as_bytes()).map_err(|error| error.to_string())
    }
}

#[derive(Component)]
struct Load(Ticket);

#[derive(Component)]
struct Save(SaveRequest);

#[derive(Component)]
struct Loading(BlockingTask<Result<Snapshot, String>>);

#[derive(Component)]
struct Saving(BlockingTask<Result<(), String>>);

type ActiveFiles<'w, 's> = Query<'w, 's, &'static FileId, Or<(With<Loading>, With<Saving>)>>;

fn queue_loads(
    mut reads: MessageReader<LoadRequest>,
    roots: Query<(Entity, &Root, &Scope)>,
    wake: Option<Res<FileWake>>,
    mut completed: MessageWriter<LoadCompleted>,
    mut commands: Commands,
) {
    for request in reads.read() {
        let ticket = &request.0;
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(_, root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            continue;
        }
        let path = vmux_path::PathIdentity::resolve(&ticket.path).into_path_buf();
        if roots.iter().any(|(owner, root, _)| {
            owner != ticket.owner && vmux_path::PathIdentity::resolve(&root.0).as_path() == path
        }) {
            completed.write(LoadCompleted {
                ticket: ticket.clone(),
                result: Err("fold store path has multiple owners".into()),
            });
            if let Some(wake) = &wake {
                wake.notify();
            }
            continue;
        }
        commands.spawn((
            Operation::bundle("editor.fold.load").unwrap(),
            Name::new("Load fold preferences"),
            FileId(path),
            Load(ticket.clone()),
        ));
    }
}

fn queue_saves(
    mut writes: MessageReader<SaveRequest>,
    roots: Query<(&Root, &Scope)>,
    mut commands: Commands,
) {
    for request in writes.read() {
        let ticket = &request.ticket;
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            continue;
        }
        let path = vmux_path::PathIdentity::resolve(&ticket.path).into_path_buf();
        commands.spawn((
            Operation::bundle("editor.fold.save").unwrap(),
            Name::new("Save fold preferences"),
            FileId(path),
            Save(request.clone()),
        ));
    }
}

fn load(
    queued: Query<(Entity, &FileId, &Load, &OperationState), Without<Loading>>,
    active: ActiveFiles,
    roots: Query<(&Root, &Scope)>,
    runtime: Res<BlockingRuntime>,
    wake: Option<Res<FileWake>>,
    mut completed: MessageWriter<LoadCompleted>,
    mut commands: Commands,
) {
    let mut occupied = HashSet::new();
    for path in &active {
        occupied.insert(path.0.clone());
    }
    for (entity, path, queued, state) in &queued {
        let ticket = &queued.0;
        if *state != OperationState::Pending
            || !roots
                .get(ticket.owner)
                .is_ok_and(|(root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(path.0.clone()) {
            continue;
        }
        let path = path.0.clone();
        match runtime.try_spawn(
            Lane::Short,
            wake.as_ref().map(|wake| wake.0.clone()),
            move || FileStore::read(&path),
        ) {
            Ok(task) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Running, Loading(task)));
            }
            Err(SpawnError::Full) => {}
            Err(error) => {
                completed.write(LoadCompleted {
                    ticket: ticket.clone(),
                    result: Err(error.to_string()),
                });
                if let Some(wake) = &wake {
                    wake.notify();
                }
                commands.entity(entity).despawn();
            }
        }
    }
}

fn save(
    queued: Query<(Entity, &FileId, &Save, &OperationState), Without<Saving>>,
    active: ActiveFiles,
    roots: Query<(&Root, &Scope)>,
    runtime: Res<BlockingRuntime>,
    wake: Option<Res<FileWake>>,
    mut completed: MessageWriter<SaveCompleted>,
    mut commands: Commands,
) {
    let mut occupied = HashSet::new();
    for path in &active {
        occupied.insert(path.0.clone());
    }
    for (entity, path, queued, state) in &queued {
        let request = &queued.0;
        let ticket = &request.ticket;
        if *state != OperationState::Pending
            || !roots
                .get(ticket.owner)
                .is_ok_and(|(root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(path.0.clone()) {
            continue;
        }
        let path = path.0.clone();
        let snapshot = request.snapshot.clone();
        match runtime.try_spawn(
            Lane::Short,
            wake.as_ref().map(|wake| wake.0.clone()),
            move || FileStore::write(&path, &snapshot),
        ) {
            Ok(task) => {
                commands
                    .entity(entity)
                    .insert((OperationState::Running, Saving(task)));
            }
            Err(SpawnError::Full) => {}
            Err(error) => {
                completed.write(SaveCompleted {
                    ticket: ticket.clone(),
                    operation: request.operation,
                    revision: request.revision,
                    result: Err(error.to_string()),
                });
                if let Some(wake) = &wake {
                    wake.notify();
                }
                commands.entity(entity).despawn();
            }
        }
    }
}

fn loaded(
    mut tasks: Query<(Entity, &Load, &mut Loading)>,
    mut loaded: MessageWriter<LoadCompleted>,
    mut commands: Commands,
) {
    for (entity, request, mut loading) in &mut tasks {
        if !loading.0.is_finished() {
            continue;
        }
        let result = match loading.0.poll() {
            Some(Ok(result)) => result,
            Some(Err(_)) => Err("fold filesystem worker panicked".into()),
            None => Err("fold filesystem worker disconnected".into()),
        };
        loaded.write(LoadCompleted {
            ticket: request.0.clone(),
            result,
        });
        commands.entity(entity).despawn();
    }
}

fn saved(
    mut tasks: Query<(Entity, &Save, &mut Saving)>,
    mut saved: MessageWriter<SaveCompleted>,
    mut commands: Commands,
) {
    for (entity, request, mut saving) in &mut tasks {
        if !saving.0.is_finished() {
            continue;
        }
        let result = match saving.0.poll() {
            Some(Ok(result)) => result,
            Some(Err(_)) => Err("fold filesystem worker panicked".into()),
            None => Err("fold filesystem worker disconnected".into()),
        };
        saved.write(SaveCompleted {
            ticket: request.0.ticket.clone(),
            operation: request.0.operation,
            revision: request.0.revision,
            result,
        });
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::Duration;
    use vmux_ecs::blocking::BlockingPool;

    use super::*;
    use crate::host::fold_preferences::{
        Collapsed, FoldPlugin, Loaded, Mutation, State, UpdateRequest,
    };

    struct HostWake(mpsc::Sender<()>);

    impl Wake for HostWake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    struct Host {
        app: App,
        root: Entity,
        wake: mpsc::Receiver<()>,
    }

    impl Host {
        fn new(path: &Path) -> Self {
            let (sender, wake) = mpsc::channel();
            let mut app = App::new();
            app.add_plugins((FoldPlugin, FilesystemPlugin))
                .insert_resource(FileWake(Arc::new(HostWake(sender)).into()));
            let root = app.world_mut().spawn(Root(path.to_path_buf())).id();
            Self { app, root, wake }
        }

        fn awakened_frame(&mut self) {
            self.wake.recv_timeout(Duration::from_secs(5)).unwrap();
            self.app.update();
        }
    }

    #[test]
    fn legacy_snapshot_restores_and_commits_on_the_frame_awakened_by_io() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        std::fs::write(&path, r#"(files:{"/files/shared.rs":[7,2,7]})"#).unwrap();
        let mut host = Host::new(&path);
        host.app.update();
        host.awakened_frame();
        assert!(host.app.world().get::<Loaded>(host.root).is_some());
        let ticket = host.app.world().get::<Scope>(host.root).unwrap().0.clone();
        host.app.world_mut().write_message(UpdateRequest {
            ticket,
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [9, 3].into(),
                expand: [2, 7].into(),
            },
        });
        host.app.update();
        host.awakened_frame();
        assert!(
            host.app
                .world()
                .get::<State>(host.root)
                .unwrap()
                .loaded()
                .is_some()
        );
        assert_eq!(
            FileStore::read(&path).unwrap().files["/files/shared.rs"],
            [3, 9]
        );
    }

    #[test]
    fn malformed_legacy_data_is_reported_without_replacing_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        let source = "not a fold snapshot";
        std::fs::write(&path, source).unwrap();
        let mut host = Host::new(&path);
        host.app.update();
        host.awakened_frame();
        let state = host.app.world().get::<State>(host.root).unwrap();
        assert_eq!(state.phase(), crate::host::fold_preferences::Phase::Failed);
        assert!(state.error().is_some());
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }

    #[test]
    fn replacement_owner_waits_for_started_old_io_before_reading_its_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        let mut host = Host::new(&path);
        host.app.insert_resource(BlockingRuntime::new(
            BlockingPool::new("fold-replacement", 1, 1),
            BlockingPool::new("fold-long", 0, 0),
            BlockingPool::new("fold-process", 0, 0),
        ));
        host.app.update();
        host.awakened_frame();
        let ticket = host.app.world().get::<Scope>(host.root).unwrap().0.clone();
        let (started, running) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let blocker = host
            .app
            .world()
            .resource::<BlockingRuntime>()
            .try_spawn(Lane::Short, None, move || {
                started.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        host.app.world_mut().write_message(UpdateRequest {
            ticket,
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [17].into(),
                expand: Default::default(),
            },
        });
        host.app.update();
        host.app.world_mut().despawn(host.root);
        let replacement = host.app.world_mut().spawn(Root(path.clone())).id();
        host.app.update();
        assert!(host.app.world().get::<Loaded>(replacement).is_none());
        assert!(!path.exists());
        release.send(()).unwrap();
        host.awakened_frame();
        host.awakened_frame();
        assert!(host.app.world().get::<Loaded>(replacement).is_some());
        let mut records = host.app.world_mut().query::<(&Collapsed, &ChildOf)>();
        assert!(
            records
                .iter(host.app.world())
                .any(|(collapsed, parent)| parent.parent() == replacement
                    && collapsed.0 == [17].into())
        );
        assert_eq!(
            FileStore::read(&path).unwrap().files["/files/shared.rs"],
            [17]
        );
        drop(blocker);
    }

    #[test]
    fn saturated_native_queue_wakes_and_retries_pending_load() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        std::fs::write(&path, r#"(files:{"/files/shared.rs":[23]})"#).unwrap();
        let runtime = BlockingRuntime::new(
            BlockingPool::new("fold-queue", 1, 1),
            BlockingPool::new("fold-long", 0, 0),
            BlockingPool::new("fold-process", 0, 0),
        );
        let (started, running) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let first = runtime
            .try_spawn(Lane::Short, None, move || {
                started.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = runtime.try_spawn(Lane::Short, None, || ()).unwrap();
        let mut host = Host::new(&path);
        host.app.insert_resource(runtime);
        host.app.update();
        assert!(host.app.world().get::<Loaded>(host.root).is_none());
        release.send(()).unwrap();
        host.awakened_frame();
        host.awakened_frame();
        assert!(host.app.world().get::<Loaded>(host.root).is_some());
        let mut records = host.app.world_mut().query::<(&Collapsed, &ChildOf)>();
        assert!(records.iter(host.app.world()).any(|(collapsed, parent)| {
            parent.parent() == host.root && collapsed.0 == [23].into()
        }));
        drop((first, second));
    }

    #[test]
    fn queued_save_for_retired_owner_cannot_overwrite_replacement_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        std::fs::write(&path, r#"(files:{"/files/shared.rs":[7]})"#).unwrap();
        let mut host = Host::new(&path);
        host.app.insert_resource(BlockingRuntime::new(
            BlockingPool::new("fold-stale", 1, 1),
            BlockingPool::new("fold-long", 0, 0),
            BlockingPool::new("fold-process", 0, 0),
        ));
        host.app.update();
        host.awakened_frame();
        let (started, running) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let runtime = host.app.world().resource::<BlockingRuntime>();
        let first = runtime
            .try_spawn(Lane::Short, None, move || {
                started.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = runtime.try_spawn(Lane::Short, None, || ()).unwrap();
        let ticket = host.app.world().get::<Scope>(host.root).unwrap().0.clone();
        host.app.world_mut().write_message(UpdateRequest {
            ticket,
            file: FileId("/files/shared.rs".into()),
            mutation: Mutation {
                collapse: [17].into(),
                expand: [7].into(),
            },
        });
        host.app.update();
        host.app.world_mut().despawn(host.root);
        let replacement = host.app.world_mut().spawn(Root(path.clone())).id();
        host.app.update();
        assert!(host.app.world().get::<Loaded>(replacement).is_none());
        release.send(()).unwrap();
        host.awakened_frame();
        host.awakened_frame();
        assert!(host.app.world().get::<Loaded>(replacement).is_some());
        assert_eq!(
            FileStore::read(&path).unwrap().files["/files/shared.rs"],
            [7]
        );
        let mut records = host.app.world_mut().query::<(&Collapsed, &ChildOf)>();
        assert!(records.iter(host.app.world()).any(|(collapsed, parent)| {
            parent.parent() == replacement && collapsed.0 == [7].into()
        }));
        drop((first, second));
    }

    #[test]
    fn unavailable_native_pool_reports_load_failure_without_touching_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        let source = r#"(files:{"/files/shared.rs":[29]})"#;
        std::fs::write(&path, source).unwrap();
        let runtime = BlockingRuntime::new(
            BlockingPool::new("fold-short", 0, 0),
            BlockingPool::new("fold-long", 0, 0),
            BlockingPool::new("fold-process", 0, 0),
        );
        let mut host = Host::new(&path);
        host.app.insert_resource(runtime);
        host.app.update();
        host.awakened_frame();
        let state = host.app.world().get::<State>(host.root).unwrap();
        assert_eq!(state.phase(), crate::host::fold_preferences::Phase::Failed);
        assert!(state.error().is_some());
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }
}
