use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::task::Waker;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::{IoTaskPool, Task, TaskPool};
use crossbeam_channel::{Receiver, bounded};

use crate::{
    FileId, FoldSet, LoadCompleted, LoadRequest, Root, SaveCompleted, SaveRequest, Scope, Snapshot,
    Ticket,
};

pub struct FilesystemPlugin;

impl Plugin for FilesystemPlugin {
    fn build(&self, app: &mut App) {
        IoTaskPool::get_or_init(TaskPool::new);
        app.add_systems(Update, finish.before(FoldSet::Restore))
            .add_systems(
                Update,
                (queue, start)
                    .chain()
                    .after(FoldSet::Save)
                    .before(FoldSet::Project),
            );
    }
}

#[derive(Resource, Clone)]
pub struct FileWake(pub Waker);

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

#[derive(Clone)]
enum Work {
    Read(Ticket),
    Write(SaveRequest),
}

impl Work {
    fn ticket(&self) -> &Ticket {
        match self {
            Self::Read(ticket) => ticket,
            Self::Write(request) => &request.ticket,
        }
    }

    fn run(&self, path: &Path) -> Result<Snapshot, String> {
        match self {
            Self::Read(_) => FileStore::read(path),
            Self::Write(request) => {
                FileStore::write(path, &request.snapshot)?;
                Ok(Snapshot::default())
            }
        }
    }

    fn start(&self, path: &Path, wake: Option<Waker>) -> Running {
        let work = self.clone();
        let path = path.to_path_buf();
        let (sender, receiver) = bounded(1);
        let task = IoTaskPool::get().spawn(async move {
            let result = match catch_unwind(AssertUnwindSafe(|| work.run(&path))) {
                Ok(result) => result,
                Err(_) => Err("fold filesystem worker panicked".into()),
            };
            if sender.send(result).is_ok()
                && let Some(wake) = wake
            {
                let _ = catch_unwind(AssertUnwindSafe(|| wake.wake()));
            }
        });
        Running {
            work: self.clone(),
            receiver,
            _task: task,
        }
    }
}

#[derive(Component)]
struct Queued(Work);

#[derive(Component)]
struct Running {
    work: Work,
    receiver: Receiver<Result<Snapshot, String>>,
    _task: Task<()>,
}

fn queue(
    mut reads: MessageReader<LoadRequest>,
    mut writes: MessageReader<SaveRequest>,
    roots: Query<(Entity, &Root, &Scope)>,
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
            continue;
        }
        commands.spawn((FileId(path), Queued(Work::Read(ticket.clone()))));
    }
    for request in writes.read() {
        let ticket = &request.ticket;
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(_, root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            continue;
        }
        let path = vmux_path::PathIdentity::resolve(&ticket.path).into_path_buf();
        commands.spawn((FileId(path), Queued(Work::Write(request.clone()))));
    }
}

fn start(
    queued: Query<(Entity, &FileId, &Queued)>,
    active: Query<&FileId, With<Running>>,
    roots: Query<(&Root, &Scope)>,
    wake: Option<Res<FileWake>>,
    mut commands: Commands,
) {
    let mut occupied = HashSet::new();
    for path in &active {
        occupied.insert(path.0.clone());
    }
    for (entity, path, queued) in &queued {
        let ticket = queued.0.ticket();
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.path && scope.0 == *ticket)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(path.0.clone()) {
            continue;
        }
        let running = queued
            .0
            .start(&path.0, wake.as_ref().map(|wake| wake.0.clone()));
        commands.entity(entity).remove::<Queued>().insert(running);
    }
}

fn finish(
    tasks: Query<(Entity, &Running)>,
    mut loaded: MessageWriter<LoadCompleted>,
    mut saved: MessageWriter<SaveCompleted>,
    mut commands: Commands,
) {
    for (entity, running) in &tasks {
        let result = match running.receiver.try_recv() {
            Ok(result) => result,
            Err(crossbeam_channel::TryRecvError::Empty) => continue,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                Err("fold filesystem worker disconnected".into())
            }
        };
        match &running.work {
            Work::Read(ticket) => {
                loaded.write(LoadCompleted {
                    ticket: ticket.clone(),
                    result,
                });
            }
            Work::Write(request) => {
                saved.write(SaveCompleted {
                    ticket: request.ticket.clone(),
                    operation: request.operation,
                    revision: request.revision,
                    result: result.map(|_| ()),
                });
            }
        }
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::Duration;

    use super::*;
    use crate::{Collapsed, FoldPlugin, Loaded, Mutation, State, UpdateRequest};

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
            mutation: Mutation::Replace(Collapsed([9, 3].into())),
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
        assert_eq!(state.phase(), crate::Phase::Failed);
        assert!(state.error().is_some());
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }

    #[test]
    fn replacement_owner_waits_for_started_old_io_before_reading_its_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap().join("folds.ron");
        let mut host = Host::new(&path);
        host.app.update();
        host.awakened_frame();
        let ticket = host.app.world().get::<Scope>(host.root).unwrap().0.clone();
        let operation = host.app.world_mut().spawn_empty().id();
        let snapshot = Snapshot {
            files: [("/files/shared.rs".into(), vec![17])].into(),
        };
        let work = Work::Write(SaveRequest {
            ticket,
            operation,
            revision: 1,
            snapshot: snapshot.clone(),
        });
        let (sender, receiver) = bounded(1);
        let task = IoTaskPool::get().spawn(async {});
        host.app.world_mut().spawn((
            FileId(path.clone()),
            Running {
                work,
                receiver,
                _task: task,
            },
        ));
        host.app.world_mut().despawn(host.root);
        let replacement = host.app.world_mut().spawn(Root(path.clone())).id();
        host.app.update();
        assert!(host.app.world().get::<Loaded>(replacement).is_none());
        assert_eq!(
            host.app
                .world_mut()
                .query_filtered::<Entity, With<Queued>>()
                .iter(host.app.world())
                .count(),
            1
        );
        FileStore::write(&path, &snapshot).unwrap();
        sender.send(Ok(Snapshot::default())).unwrap();
        host.app.update();
        host.awakened_frame();
        assert!(host.app.world().get::<Loaded>(replacement).is_some());
        let mut records = host.app.world_mut().query::<(&Collapsed, &ChildOf)>();
        assert!(
            records
                .iter(host.app.world())
                .any(|(collapsed, parent)| parent.parent() == replacement
                    && collapsed.0 == [17].into())
        );
    }
}
