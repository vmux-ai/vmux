use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::task::Waker;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::Commands as WorldCommands;
use bevy_tasks::{IoTaskPool, Task, TaskPool, futures_lite::future};
use serde::{Deserialize, Serialize};
use vmux_ecs::Cwd;

use crate::{
    AliasCompleted, AliasRequest, Error, LoadCompleted, LoadRequest, Record, Root, Snapshot,
    StoreSet, Ticket, WriteResult, WriteTaskRequest,
};

pub struct FilesystemPlugin;

impl Plugin for FilesystemPlugin {
    fn build(&self, app: &mut App) {
        IoTaskPool::get_or_init(TaskPool::new);
        app.add_message::<AliasRequest>()
            .add_message::<AliasCompleted>()
            .configure_sets(Update, FileSet.after(StoreSet::Write))
            .add_systems(Update, read.after(StoreSet::Load).before(FileSet))
            .add_systems(Update, write.after(StoreSet::Write).before(FileSet))
            .add_systems(Update, aliases.after(StoreSet::Write).before(FileSet))
            .add_systems(
                Update,
                (finish_reads, finish_aliases, finish_writes)
                    .chain()
                    .before(StoreSet::Load),
            )
            .add_systems(Update, start.in_set(FileSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FileSet;

#[derive(Resource, Clone)]
pub struct FileWake(pub Waker);

impl FileWake {
    fn poll<T>(&self, task: &mut Task<T>) -> Option<T> {
        let mut context = std::task::Context::from_waker(&self.0);
        match std::future::Future::poll(std::pin::Pin::new(task), &mut context) {
            std::task::Poll::Ready(value) => Some(value),
            std::task::Poll::Pending => None,
        }
    }
}

#[derive(Component)]
struct QueuedRead {
    ticket: Ticket,
}

impl QueuedRead {
    fn start(&self, directory: &Path, wake: Option<Waker>) -> ReadTask {
        let ticket = self.ticket.clone();
        let root = directory.to_path_buf();
        let task = IoTaskPool::get().spawn(async move {
            let result = FileStore::read(&root);
            if let Some(wake) = wake {
                wake.wake();
            }
            result
        });
        ReadTask { ticket, task }
    }
}

#[derive(Component)]
struct QueuedWrite {
    ticket: Ticket,
    snapshot: Snapshot,
}

impl QueuedWrite {
    fn start(&self, directory: &Path, wake: Option<Waker>) -> WriteTask {
        let ticket = self.ticket.clone();
        let root = directory.to_path_buf();
        let request = WriteTaskRequest {
            ticket: ticket.clone(),
            snapshot: self.snapshot.clone(),
        };
        let task = IoTaskPool::get().spawn(async move {
            let result = FileStore::write(request, &root);
            if let Some(wake) = wake {
                wake.wake();
            }
            result
        });
        WriteTask { ticket, task }
    }
}

#[derive(Component)]
struct QueuedAliases {
    ticket: Ticket,
    directory: PathBuf,
}

impl QueuedAliases {
    fn start(&self, root: &Path, wake: Option<Waker>) -> AliasTask {
        let ticket = self.ticket.clone();
        let root = root.to_path_buf();
        let directory = self.directory.clone();
        let task = IoTaskPool::get().spawn(async move {
            let result = FileStore::aliases(&root, &directory);
            if let Some(wake) = wake {
                wake.wake();
            }
            result
        });
        AliasTask { ticket, task }
    }
}

#[derive(Component)]
struct ReadTask {
    ticket: Ticket,
    task: Task<Result<Snapshot, Error>>,
}

#[derive(Component)]
struct WriteTask {
    ticket: Ticket,
    task: Task<Result<BTreeSet<String>, Error>>,
}

#[derive(Component)]
struct AliasTask {
    ticket: Ticket,
    task: Task<Result<Vec<PathBuf>, Error>>,
}

pub struct FileStore;

impl FileStore {
    pub fn read(root: &Path) -> Result<Snapshot, Error> {
        let stored: StoredSnapshot = match std::fs::read_to_string(root.join(".projects.ron")) {
            Ok(source) => {
                ron::from_str(&source).map_err(|error| Error::Unavailable(error.to_string()))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoredSnapshot::default(),
            Err(error) => return Err(Error::Unavailable(error.to_string())),
        };
        if stored.version > 1 {
            return Err(Error::Unavailable(
                "unsupported project store version".to_string(),
            ));
        }
        let mut snapshot = Snapshot {
            projects: stored.projects,
            hidden: stored.hidden,
            archived: stored.archived,
        };
        if stored.version == 0 {
            let archive: LegacyArchive = match std::fs::read_to_string(root.join(".archive.ron")) {
                Ok(source) => {
                    ron::from_str(&source).map_err(|error| Error::Unavailable(error.to_string()))?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    LegacyArchive::default()
                }
                Err(error) => return Err(Error::Unavailable(error.to_string())),
            };
            snapshot.archived.extend(archive.projects);
        }
        Ok(snapshot)
    }

    pub fn write(request: WriteTaskRequest, root: &Path) -> Result<BTreeSet<String>, Error> {
        let snapshot = request.snapshot;
        let hidden = snapshot.hidden.clone();
        let stored = StoredSnapshot {
            projects: snapshot.projects,
            hidden: snapshot.hidden,
            archived: snapshot.archived,
            version: 1,
        };
        let source =
            ron::ser::to_string(&stored).map_err(|error| Error::Unavailable(error.to_string()))?;
        vmux_path::AtomicFile::write(root.join(".projects.ron"), source.as_bytes())
            .map_err(|error| Error::Unavailable(error.to_string()))?;
        Ok(hidden)
    }

    fn aliases(root: &Path, directory: &Path) -> Result<Vec<PathBuf>, Error> {
        let directory = vmux_path::PathIdentity::resolve(directory);
        let mut paths = Vec::new();
        if let Ok(relative) = directory.as_path().strip_prefix(root) {
            paths.push(relative.to_path_buf());
        }
        let imports = root.join("imported");
        let entries = match std::fs::read_dir(&imports) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(paths);
            }
            Err(error) => return Err(Error::Unavailable(error.to_string())),
        };
        for entry in entries {
            let entry = entry.map_err(|error| Error::Unavailable(error.to_string()))?;
            let path = entry.path();
            let Ok(target) = std::fs::read_link(&path) else {
                continue;
            };
            let target = if target.is_absolute() {
                target
            } else {
                imports.join(target)
            };
            if vmux_path::PathIdentity::resolve(&target) != directory {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(root) {
                paths.push(relative.to_path_buf());
            }
        }
        Ok(paths)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct StoredSnapshot {
    #[serde(default)]
    projects: BTreeMap<String, Record>,
    #[serde(default)]
    hidden: BTreeSet<String>,
    #[serde(default)]
    archived: BTreeSet<String>,
    #[serde(default)]
    version: u8,
}

#[derive(Default, Deserialize)]
struct LegacyArchive {
    projects: BTreeSet<String>,
}

fn read(
    mut requests: MessageReader<LoadRequest>,
    roots: Query<(Entity, &Root)>,
    mut completed: MessageWriter<LoadCompleted>,
    mut commands: WorldCommands,
) {
    for request in requests.read() {
        if !roots
            .get(request.ticket.owner)
            .is_ok_and(|(_, root)| root.0 == request.ticket.directory)
        {
            continue;
        }
        let directory = vmux_path::PathIdentity::resolve(&request.ticket.directory).into_path_buf();
        if roots.iter().any(|(owner, root)| {
            owner != request.ticket.owner
                && vmux_path::PathIdentity::resolve(&root.0).as_path() == directory
        }) {
            completed.write(LoadCompleted {
                ticket: request.ticket.clone(),
                result: Err(Error::Duplicate),
            });
            continue;
        }
        commands.spawn((
            Cwd(directory),
            QueuedRead {
                ticket: request.ticket.clone(),
            },
        ));
    }
}

fn write(
    mut requests: MessageReader<WriteTaskRequest>,
    roots: Query<&Root>,
    mut commands: WorldCommands,
) {
    for request in requests.read() {
        if !roots
            .get(request.ticket.owner)
            .is_ok_and(|root| root.0 == request.ticket.directory)
        {
            continue;
        }
        let directory = vmux_path::PathIdentity::resolve(&request.ticket.directory).into_path_buf();
        commands.spawn((
            Cwd(directory),
            QueuedWrite {
                ticket: request.ticket.clone(),
                snapshot: request.snapshot.clone(),
            },
        ));
    }
}

fn aliases(
    mut requests: MessageReader<AliasRequest>,
    roots: Query<&Root>,
    mut commands: WorldCommands,
) {
    for request in requests.read() {
        if !roots
            .get(request.ticket.owner)
            .is_ok_and(|root| root.0 == request.ticket.directory)
        {
            continue;
        }
        let root = vmux_path::PathIdentity::resolve(&request.ticket.directory).into_path_buf();
        commands.spawn((
            Cwd(root),
            QueuedAliases {
                ticket: request.ticket.clone(),
                directory: request.directory.clone(),
            },
        ));
    }
}

type ActiveIo<'w, 's> =
    Query<'w, 's, &'static Cwd, Or<(With<ReadTask>, With<AliasTask>, With<WriteTask>)>>;

fn start(
    reads: Query<(Entity, &Cwd, &QueuedRead)>,
    aliases: Query<(Entity, &Cwd, &QueuedAliases)>,
    writes: Query<(Entity, &Cwd, &QueuedWrite)>,
    active: ActiveIo,
    roots: Query<&Root>,
    wake: Option<Res<FileWake>>,
    mut commands: WorldCommands,
) {
    let mut occupied = HashSet::new();
    for directory in &active {
        occupied.insert(directory.0.clone());
    }
    for (entity, directory, request) in &reads {
        if !roots
            .get(request.ticket.owner)
            .is_ok_and(|root| root.0 == request.ticket.directory)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(directory.0.clone()) {
            continue;
        }
        let task = request.start(&directory.0, wake.as_deref().map(|wake| wake.0.clone()));
        commands.entity(entity).remove::<QueuedRead>().insert(task);
    }
    for (entity, directory, request) in &writes {
        if !roots
            .get(request.ticket.owner)
            .is_ok_and(|root| root.0 == request.ticket.directory)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(directory.0.clone()) {
            continue;
        }
        let task = request.start(&directory.0, wake.as_deref().map(|wake| wake.0.clone()));
        commands.entity(entity).remove::<QueuedWrite>().insert(task);
    }
    for (entity, directory, request) in &aliases {
        if !roots
            .get(request.ticket.owner)
            .is_ok_and(|root| root.0 == request.ticket.directory)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(directory.0.clone()) {
            continue;
        }
        let task = request.start(&directory.0, wake.as_deref().map(|wake| wake.0.clone()));
        commands
            .entity(entity)
            .remove::<QueuedAliases>()
            .insert(task);
    }
}

fn finish_reads(
    mut tasks: Query<(Entity, &mut ReadTask)>,
    mut results: MessageWriter<LoadCompleted>,
    wake: Option<Res<FileWake>>,
    mut commands: WorldCommands,
) {
    for (entity, mut task) in &mut tasks {
        let result = match &wake {
            Some(wake) => wake.poll(&mut task.task),
            None => future::block_on(future::poll_once(&mut task.task)),
        };
        let Some(result) = result else {
            continue;
        };
        results.write(LoadCompleted {
            ticket: task.ticket.clone(),
            result,
        });
        commands.entity(entity).despawn();
    }
}

fn finish_writes(
    mut tasks: Query<(Entity, &mut WriteTask)>,
    mut results: MessageWriter<WriteResult>,
    wake: Option<Res<FileWake>>,
    mut commands: WorldCommands,
) {
    for (entity, mut task) in &mut tasks {
        let result = match &wake {
            Some(wake) => wake.poll(&mut task.task),
            None => future::block_on(future::poll_once(&mut task.task)),
        };
        let Some(result) = result else {
            continue;
        };
        results.write(WriteResult {
            ticket: task.ticket.clone(),
            result,
        });
        commands.entity(entity).despawn();
    }
}

fn finish_aliases(
    mut tasks: Query<(Entity, &mut AliasTask)>,
    mut results: MessageWriter<AliasCompleted>,
    wake: Option<Res<FileWake>>,
    mut commands: WorldCommands,
) {
    for (entity, mut task) in &mut tasks {
        let result = match &wake {
            Some(wake) => wake.poll(&mut task.task),
            None => future::block_on(future::poll_once(&mut task.task)),
        };
        let Some(result) = result else {
            continue;
        };
        results.write(AliasCompleted {
            ticket: task.ticket.clone(),
            result,
        });
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use super::*;
    use crate::Repository;
    use crate::{
        LoadError, Loaded, Mutation, ProjectId, ProjectSaved, Registered, Registrations, Revision,
        WriteCompleted, WriteRequest,
    };

    struct Store {
        app: App,
        root: PathBuf,
        owner: Entity,
    }

    struct WakeCounter(AtomicUsize);

    impl WakeCounter {
        fn host(counter: Arc<Self>) -> FileWake {
            FileWake(Waker::from(counter))
        }
    }

    impl std::task::Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[derive(Clone, Default)]
    struct Signal {
        released: Arc<AtomicBool>,
        waker: Arc<Mutex<Option<Waker>>>,
    }

    impl Signal {
        fn release(&self) {
            self.released.store(true, Ordering::Release);
            if let Some(waker) = self.waker.lock().unwrap().take() {
                waker.wake();
            }
        }
    }

    impl std::future::Future for Signal {
        type Output = ();

        fn poll(
            self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            let mut waker = self.waker.lock().unwrap();
            if self.released.load(Ordering::Acquire) {
                return std::task::Poll::Ready(());
            }
            *waker = Some(context.waker().clone());
            std::task::Poll::Pending
        }
    }

    #[test]
    fn pending_io_completion_wakes_the_registered_host_after_publication() {
        IoTaskPool::get_or_init(TaskPool::new);
        let counter = Arc::new(WakeCounter(AtomicUsize::new(0)));
        let wake = WakeCounter::host(counter.clone());
        let signal = Signal::default();
        let gate = signal.clone();
        let mut task = IoTaskPool::get().spawn(async move {
            gate.await;
            true
        });
        assert!(wake.poll(&mut task).is_none());
        let before = counter.0.load(Ordering::SeqCst);
        signal.release();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !task.is_finished() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(counter.0.load(Ordering::SeqCst) > before);
        assert!(wake.poll(&mut task).is_some());
    }

    #[test]
    fn completed_io_commits_on_the_frame_awakened_by_the_worker() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().canonicalize().unwrap();
        let url = "vmux://projects/ready";
        let mut app = App::new();
        app.add_plugins(crate::ProjectPlugin);
        let owner = app.world_mut().spawn(Root(directory.clone())).id();
        app.update();
        let request = app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap();
        app.add_plugins(FilesystemPlugin);
        let mut snapshot = Snapshot::default();
        snapshot.projects.insert(
            url.into(),
            Record {
                name: "Ready".into(),
                directory: directory.clone(),
                repository: None,
                revision: 1,
            },
        );
        let task = IoTaskPool::get().spawn(async move { Ok(snapshot) });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !task.is_finished() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.world_mut().spawn((
            Cwd(directory.clone()),
            ReadTask {
                ticket: request.ticket,
                task,
            },
        ));
        app.update();
        assert!(app.world().get::<Loaded>(owner).is_some());
        let project = app
            .world_mut()
            .query_filtered::<Entity, With<Registered>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<Name>(project).unwrap().as_str(), "Ready");
        app.world_mut().spawn(WriteRequest {
            root: directory,
            mutation: Mutation::Archive {
                url: url.into(),
                archived: true,
            },
        });
        app.update();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let finished = app
                .world_mut()
                .query::<&WriteTask>()
                .single(app.world())
                .unwrap()
                .task
                .is_finished();
            if finished {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        app.update();
        assert!(app.world().get::<crate::Archived>(project).is_some());
        let completed = app
            .world_mut()
            .resource_mut::<Messages<WriteCompleted>>()
            .drain()
            .next()
            .unwrap();
        assert!(completed.result.is_ok());
    }

    #[test]
    fn a_replacement_root_waits_for_accepted_io_before_restoring_the_same_directory() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let mut store = Store::at(root.path());
        let released = Signal::default();
        let gate = released.clone();
        let directory = store.root.clone();
        let ticket = Ticket {
            owner: store.owner,
            directory: directory.clone(),
            generation: 0,
        };
        let mut snapshot = Snapshot::default();
        snapshot.projects.insert(
            "accepted".to_string(),
            Record {
                name: "Accepted IO".to_string(),
                directory: project.canonicalize().unwrap(),
                repository: None,
                revision: 1,
            },
        );
        let request = WriteTaskRequest {
            ticket: ticket.clone(),
            snapshot,
        };
        let task = IoTaskPool::get().spawn(async move {
            gate.await;
            FileStore::write(request, &directory)
        });
        store
            .app
            .world_mut()
            .spawn((Cwd(store.root.clone()), WriteTask { ticket, task }));
        store.app.world_mut().despawn(store.owner);
        let replacement = store.app.world_mut().spawn(Root(store.root.clone())).id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
        while std::time::Instant::now() < deadline {
            store.app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let waited = store.app.world().get::<Loaded>(replacement).is_none();
        released.release();
        assert!(waited);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while store.app.world().get::<Loaded>(replacement).is_none() {
            store.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(store.snapshot().projects["accepted"].name, "Accepted IO");
        assert_eq!(
            FileStore::read(&store.root).unwrap().projects["accepted"].revision,
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_aliases_cannot_create_a_second_owner_for_the_same_store() {
        let root = tempfile::tempdir().unwrap();
        let links = tempfile::tempdir().unwrap();
        let alias = links.path().join("alias");
        std::os::unix::fs::symlink(root.path(), &alias).unwrap();
        let mut store = Store::at(root.path());
        let owner = store.app.world_mut().spawn(Root(alias.clone())).id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while store.app.world().get::<LoadError>(owner).is_none() {
            store.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        store.app.world_mut().spawn(WriteRequest {
            root: alias.clone(),
            mutation: Mutation::Archive {
                url: "duplicate".into(),
                archived: true,
            },
        });
        assert_eq!(store.complete(1)[0].result, Err(Error::Duplicate));
        assert!(store.snapshot().archived.is_empty());
        assert!(!root.path().join(".projects.ron").exists());
    }

    #[test]
    fn legacy_archive_migrates_once_and_unarchive_survives_restart() {
        let root = tempfile::tempdir().unwrap();
        let url = "vmux://projects/discovered";
        let legacy = format!("(projects:[{url:?}])");
        std::fs::write(root.path().join(".archive.ron"), &legacy).unwrap();
        let mut store = Store::at(root.path());
        assert!(store.snapshot().archived.contains(url));
        store.app.world_mut().spawn(WriteRequest {
            root: store.root.clone(),
            mutation: Mutation::Archive {
                url: url.into(),
                archived: false,
            },
        });
        assert!(store.complete(1)[0].result.is_ok());
        drop(store);
        let mut restored = Store::at(root.path());
        assert!(!restored.snapshot().archived.contains(url));
        assert_eq!(
            std::fs::read_to_string(root.path().join(".archive.ron")).unwrap(),
            legacy
        );
    }

    impl Store {
        fn at(root: &Path) -> Self {
            let mut app = App::new();
            app.add_plugins((crate::ProjectPlugin, FilesystemPlugin));
            std::fs::create_dir_all(root).unwrap();
            let root = root.canonicalize().unwrap();
            let owner = app.world_mut().spawn(Root(root.clone())).id();
            let mut store = Self { app, root, owner };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                store.app.update();
                if store.app.world().get::<Loaded>(owner).is_some()
                    || store.app.world().get::<LoadError>(owner).is_some()
                {
                    return store;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "registrations did not load"
                );
                std::thread::yield_now();
            }
        }

        fn queue(&mut self, url: &str, revision: u64, name: &str, directory: &Path) -> Entity {
            self.app
                .world_mut()
                .spawn(WriteRequest {
                    root: self.root.clone(),
                    mutation: Mutation::Save {
                        expected: revision,
                        project: ProjectSaved {
                            root: self.root.clone(),
                            id: ProjectId(url.into()),
                            directory: directory.canonicalize().unwrap(),
                            name: name.into(),
                            repository: None,
                            revision: 0,
                        },
                    },
                })
                .id()
        }

        fn complete(&mut self, count: usize) -> Vec<WriteCompleted> {
            let mut results = Vec::new();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                self.app.update();
                results.extend(
                    self.app
                        .world_mut()
                        .resource_mut::<Messages<WriteCompleted>>()
                        .drain(),
                );
                if results.len() == count {
                    return results;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "writes did not complete"
                );
                std::thread::yield_now();
            }
        }

        fn save(&mut self, url: &str, revision: u64, name: &str, directory: &Path) {
            self.queue(url, revision, name, directory);
            assert!(self.complete(1)[0].result.is_ok());
        }

        fn entity(&mut self, url: &str) -> Entity {
            self.app
                .world_mut()
                .query_filtered::<(Entity, &ProjectId), With<Registered>>()
                .iter(self.app.world())
                .find(|(_, id)| id.0 == url)
                .unwrap()
                .0
        }

        fn snapshot(&mut self) -> Snapshot {
            let mut state =
                bevy_ecs::system::SystemState::<Registrations>::new(self.app.world_mut());
            state.get(self.app.world()).unwrap().snapshot(&self.root)
        }
    }

    #[test]
    fn existing_ron_registrations_restore_into_project_components() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("repo");
        std::fs::create_dir_all(directory.join(".git")).unwrap();
        let url = "vmux://projects/stable";
        let remote = "https://github.com/example/repo.git";
        let source = format!(
            "(projects:{{{url:?}:(name:\"Legacy\",directory:{:?},repository:Some({remote:?}),revision:7)}},hidden:[\"vmux://projects/removed\"])",
            directory.canonicalize().unwrap().to_str().unwrap(),
        );
        std::fs::write(root.path().join(".projects.ron"), source).unwrap();
        let mut store = Store::at(root.path());
        let entity = store.entity(url);
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Legacy"
        );
        assert_eq!(store.app.world().get::<Revision>(entity).unwrap().0, 7);
        assert_eq!(
            store
                .app
                .world()
                .get::<Repository>(entity)
                .unwrap()
                .0
                .as_deref(),
            Some(remote)
        );
        assert!(store.snapshot().hidden.contains("vmux://projects/removed"));
    }

    #[test]
    fn writes_and_queries_stay_with_their_own_projects_root() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_project = first.path().join("repo");
        let second_project = second.path().join("repo");
        std::fs::create_dir_all(first_project.join(".git")).unwrap();
        std::fs::create_dir_all(second_project.join(".git")).unwrap();
        let mut store = Store::at(first.path());
        let second_root = second.path().canonicalize().unwrap();
        store.app.world_mut().spawn(Root(second_root.clone()));
        store.queue("vmux://projects/same", 0, "First root", &first_project);
        store.app.world_mut().spawn(WriteRequest {
            root: second_root.clone(),
            mutation: Mutation::Save {
                expected: 0,
                project: ProjectSaved {
                    root: second_root.clone(),
                    id: ProjectId("vmux://projects/same".into()),
                    directory: second_project.canonicalize().unwrap(),
                    name: "Second root".into(),
                    repository: None,
                    revision: 0,
                },
            },
        });
        assert!(
            store
                .complete(2)
                .iter()
                .all(|completed| completed.result.is_ok())
        );
        let first_snapshot = store.snapshot();
        assert_eq!(first_snapshot.projects.len(), 1);
        assert_eq!(
            first_snapshot.projects["vmux://projects/same"].name,
            "First root"
        );
        let mut state = bevy_ecs::system::SystemState::<Registrations>::new(store.app.world_mut());
        let second_snapshot = state.get(store.app.world()).unwrap().snapshot(&second_root);
        assert_eq!(second_snapshot.projects.len(), 1);
        assert_eq!(
            second_snapshot.projects["vmux://projects/same"].name,
            "Second root"
        );
        assert_eq!(
            FileStore::read(&second_root).unwrap().projects["vmux://projects/same"].name,
            "Second root"
        );
    }

    #[test]
    fn source_edits_preserve_the_entity_and_restore_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(first.join(".git")).unwrap();
        std::fs::create_dir_all(second.join(".git")).unwrap();
        std::fs::write(first.join("work.txt"), "keep").unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "First", &first);
        let entity = store.entity(url);
        store.save(url, 1, "Renamed", &second);
        assert_eq!(store.entity(url), entity);
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Renamed"
        );
        assert_eq!(
            store.app.world().get::<Cwd>(entity).unwrap().0,
            second.canonicalize().unwrap()
        );
        drop(store);
        let mut restored = Store::at(root.path());
        let entity = restored.entity(url);
        assert_eq!(restored.app.world().get::<Revision>(entity).unwrap().0, 2);
        let mut state =
            bevy_ecs::system::SystemState::<Registrations>::new(restored.app.world_mut());
        let registrations = state.get(restored.app.world()).unwrap();
        assert_eq!(
            registrations
                .directory(&restored.root, url)
                .unwrap()
                .unwrap(),
            second.canonicalize().unwrap()
        );
        assert_eq!(
            registrations
                .url_for_directory(&restored.root, &second.canonicalize().unwrap().join("src")),
            Some(url.into())
        );
        assert_eq!(
            std::fs::read_to_string(first.join("work.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn concurrent_writes_reject_stale_edits_and_keep_independent_projects() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(first.join(".git")).unwrap();
        std::fs::create_dir_all(second.join(".git")).unwrap();
        let url = "vmux://projects/first";
        let mut store = Store::at(root.path());
        store.save(url, 0, "First", &first);
        store.queue(url, 1, "Winner", &first);
        store.queue(url, 1, "Other edit", &first);
        store.queue("vmux://projects/second", 0, "Second", &second);
        let results = store.complete(3);
        assert_eq!(
            results
                .iter()
                .filter(|result| result.result.is_ok())
                .count(),
            2
        );
        let snapshot = store.snapshot();
        assert_eq!(snapshot.projects.len(), 2);
        assert_eq!(snapshot.projects[url].revision, 2);
        assert_eq!(FileStore::read(&store.root).unwrap().projects.len(), 2);
        store.queue("vmux://projects/duplicate", 0, "Duplicate", &first);
        assert!(store.complete(1)[0].result.is_err());
        assert_eq!(store.snapshot().projects.len(), 2);
    }

    #[test]
    fn an_accepted_edit_preserves_the_canonical_project_identity_before_commit() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("repo");
        std::fs::create_dir_all(directory.join(".git")).unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Original", &directory);
        let entity = store.entity(url);
        store.queue(url, 1, "Edited", &directory);

        store.app.update();

        let index = store
            .app
            .world()
            .resource::<vmux_ecs::IdentityIndex<ProjectId>>();
        assert_eq!(index.entity(&ProjectId(url.into())).unwrap(), Some(entity));
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Original"
        );
        assert!(store.complete(1)[0].result.is_ok());
        assert_eq!(store.entity(url), entity);
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Edited"
        );
    }

    #[test]
    fn ambiguous_canonical_project_identity_refuses_mutation_before_io() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("repo");
        let other = root.path().join("other");
        std::fs::create_dir_all(directory.join(".git")).unwrap();
        std::fs::create_dir_all(other.join(".git")).unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Original", &directory);
        let source = std::fs::read(store.root.join(".projects.ron")).unwrap();
        store.app.world_mut().spawn((
            crate::Project,
            Registered,
            ProjectId(url.into()),
            Name::new("Duplicate"),
            Cwd(other),
            Repository(None),
            Revision(7),
            ChildOf(store.owner),
        ));
        store.queue(url, 1, "Rejected", &directory);

        assert_eq!(store.complete(1)[0].result, Err(Error::Duplicate));
        assert_eq!(
            std::fs::read(store.root.join(".projects.ron")).unwrap(),
            source
        );
    }

    #[test]
    fn failed_persistence_keeps_the_committed_entity() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        let url = "vmux://projects/repo";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Original", &project);
        let file = store.root.join(".projects.ron");
        let source = std::fs::read_to_string(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        store.queue(url, 1, "Rejected", &project);
        assert!(store.complete(1)[0].result.is_err());
        let entity = store.entity(url);
        assert_eq!(
            store.app.world().get::<Name>(entity).unwrap().as_str(),
            "Original"
        );
        assert_eq!(store.app.world().get::<Revision>(entity).unwrap().0, 1);
        std::fs::remove_dir(&file).unwrap();
        std::fs::write(&file, source).unwrap();
        assert_eq!(
            FileStore::read(&store.root).unwrap().projects[url].name,
            "Original"
        );
    }

    #[test]
    fn removal_restores_hidden_urls_without_deleting_checkout_files() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::write(project.join("work.txt"), "keep").unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Project", &project);
        store.app.world_mut().spawn(WriteRequest {
            root: store.root.clone(),
            mutation: Mutation::Remove(url.into()),
        });
        assert!(store.complete(1)[0].result.is_ok());
        drop(store);
        let mut restored = Store::at(root.path());
        assert!(restored.snapshot().projects.is_empty());
        let mut state =
            bevy_ecs::system::SystemState::<Registrations>::new(restored.app.world_mut());
        let registrations = state.get(restored.app.world()).unwrap();
        for url in [url, "vmux://projects/repo"] {
            assert!(registrations.directory(&restored.root, url).is_err());
        }
        assert_eq!(
            std::fs::read_to_string(project.join("work.txt")).unwrap(),
            "keep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn removal_hides_relative_aliases_and_preserves_registered_alias_identities() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        let other = root.path().join("other");
        let imports = root.path().join("imported");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(other.join(".git")).unwrap();
        std::fs::create_dir_all(&imports).unwrap();
        std::os::unix::fs::symlink("../repo", imports.join("relative")).unwrap();
        std::os::unix::fs::symlink("../repo", imports.join("registered")).unwrap();
        let url = "vmux://projects/stable";
        let retained = "vmux://projects/imported/registered";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Removed", &project);
        store.save(retained, 0, "Retained", &other);
        store.app.world_mut().spawn(WriteRequest {
            root: store.root.clone(),
            mutation: Mutation::Remove(url.into()),
        });
        assert!(store.complete(1)[0].result.is_ok());
        drop(store);

        let mut restored = Store::at(root.path());
        let snapshot = restored.snapshot();
        assert!(
            snapshot
                .hidden
                .contains("vmux://projects/imported/relative")
        );
        assert!(snapshot.hidden.contains("vmux://projects/repo"));
        assert!(!snapshot.hidden.contains(retained));
        assert_eq!(snapshot.projects[retained].name, "Retained");
        assert_eq!(
            snapshot.projects[retained].directory,
            other.canonicalize().unwrap()
        );
        assert!(project.join(".git").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_sole_symlink_root_preserves_alias_hides_after_removal() {
        let root = tempfile::tempdir().unwrap();
        let links = tempfile::tempdir().unwrap();
        let project = root.path().join("repo");
        let imports = root.path().join("imported");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(&imports).unwrap();
        std::os::unix::fs::symlink("../repo", imports.join("alias")).unwrap();
        let url = "vmux://projects/stable";
        let mut store = Store::at(root.path());
        store.save(url, 0, "Project", &project);
        let alias = links.path().join("root");
        std::os::unix::fs::symlink(root.path(), &alias).unwrap();
        store.app.world_mut().despawn(store.owner);
        store.root = alias.clone();
        store.owner = store.app.world_mut().spawn(Root(alias)).id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while store.app.world().get::<Loaded>(store.owner).is_none() {
            store.app.update();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        store.app.world_mut().spawn(WriteRequest {
            root: store.root.clone(),
            mutation: Mutation::Remove(url.into()),
        });
        assert!(store.complete(1)[0].result.is_ok());
        let mut restored = Store::at(root.path());
        let snapshot = restored.snapshot();
        for url in [
            url,
            "vmux://projects/repo",
            "vmux://projects/imported/alias",
        ] {
            assert!(snapshot.hidden.contains(url));
        }
        assert!(snapshot.projects.is_empty());
        assert!(project.join(".git").is_dir());
    }

    #[test]
    fn alias_read_failure_retains_committed_state_and_never_dispatches_a_write() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().canonicalize().unwrap();
        let url = "vmux://projects/stable";
        let mut app = App::new();
        app.add_plugins(crate::ProjectPlugin);
        let owner = app.world_mut().spawn(Root(directory.clone())).id();
        app.update();
        let load = app
            .world_mut()
            .resource_mut::<Messages<LoadRequest>>()
            .drain()
            .next()
            .unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.projects.insert(
            url.into(),
            Record {
                name: "Retained".into(),
                directory: directory.join("repo"),
                repository: None,
                revision: 1,
            },
        );
        app.world_mut().write_message(LoadCompleted {
            ticket: load.ticket,
            result: Ok(snapshot),
        });
        app.update();
        let project = app
            .world_mut()
            .query_filtered::<Entity, With<Registered>>()
            .single(app.world())
            .unwrap();
        let requester = app
            .world_mut()
            .spawn(WriteRequest {
                root: directory,
                mutation: Mutation::Remove(url.into()),
            })
            .id();
        app.update();
        let request = app
            .world_mut()
            .resource_mut::<Messages<AliasRequest>>()
            .drain()
            .next()
            .unwrap();
        let error = Error::Unavailable("alias lookup failed".into());
        app.world_mut().write_message(AliasCompleted {
            ticket: request.ticket,
            result: Err(error.clone()),
        });
        app.update();

        assert!(app.world().get::<Registered>(project).is_some());
        assert!(app.world().get::<crate::Hidden>(project).is_none());
        assert_eq!(
            app.world().get::<Name>(project).unwrap().as_str(),
            "Retained"
        );
        assert_eq!(app.world().get::<Revision>(project).unwrap().0, 1);
        assert!(
            app.world()
                .resource::<Messages<WriteTaskRequest>>()
                .is_empty()
        );
        let completed = app
            .world_mut()
            .resource_mut::<Messages<WriteCompleted>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(completed.requester, requester);
        assert_eq!(completed.result, Err(error));
        assert!(app.world().get::<Root>(owner).is_some());
    }

    #[test]
    fn corrupt_registrations_are_reported_without_overwriting_them() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join(".projects.ron");
        std::fs::write(&file, "invalid").unwrap();
        let store = Store::at(root.path());
        assert!(store.app.world().get::<LoadError>(store.owner).is_some());
        assert!(store.app.world().get::<Loaded>(store.owner).is_none());
        assert_eq!(std::fs::read_to_string(file).unwrap(), "invalid");
    }
}
