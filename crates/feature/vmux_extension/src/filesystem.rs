use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use sha2::{Digest, Sha256};
use vmux_ecs::{
    Cwd, Job, JobPlugin, JobRequest, JobResult, JobSet, JobWake, Operation, OperationCompleted,
};

use crate::core::{
    CleanupCompleted, CleanupRequest, ExtensionSet, InstallOwner, InstallRequest, LoadCompleted,
    LoadRequest, Root, Scope, Ticket, WriteCompleted, WriteTaskRequest,
};
use crate::snapshot::{ExtEntry, Index};

pub struct FilesystemPlugin;

impl Plugin for FilesystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<FileJob>::default())
            .add_message::<CleanupCompleted>()
            .add_message::<MaterializeRequest>()
            .add_message::<MaterializeCompleted>()
            .init_resource::<Clock>()
            .add_systems(
                Update,
                finish.in_set(JobSet::Apply).before(ExtensionSet::Step),
            )
            .add_systems(Update, (receive, start).chain().after(ExtensionSet::Step));
    }
}

pub struct FileStore;

#[derive(Message, Clone)]
pub struct MaterializeRequest {
    pub ticket: Ticket,
    pub operation: Entity,
    pub entry: Box<ExtEntry>,
    pub source: std::path::PathBuf,
    _lifetime: Option<Arc<dyn Send + Sync>>,
}

impl MaterializeRequest {
    pub fn new(
        ticket: Ticket,
        operation: Entity,
        entry: Box<ExtEntry>,
        source: std::path::PathBuf,
    ) -> Self {
        Self {
            ticket,
            operation,
            entry,
            source,
            _lifetime: None,
        }
    }

    pub(crate) fn with_lifetime(mut self, lifetime: Arc<dyn Send + Sync>) -> Self {
        self._lifetime = Some(lifetime);
        self
    }
}

#[derive(Message)]
pub struct MaterializeCompleted {
    pub ticket: Ticket,
    pub operation: Entity,
    pub result: Result<ExtEntry, String>,
}

impl FileStore {
    pub fn source_hash(source: &Path) -> Result<String, String> {
        let mut files = Vec::new();
        Self::collect_files(source, source, &mut files)?;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut hasher = Sha256::new();
        for (relative, absolute) in files {
            hasher.update(relative.as_bytes());
            hasher.update([0]);
            hasher.update(std::fs::read(absolute).map_err(|error| error.to_string())?);
            hasher.update([0]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn collect_files(
        root: &Path,
        current: &Path,
        files: &mut Vec<(String, std::path::PathBuf)>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(current).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            let path = entry.path();
            if kind.is_dir() {
                Self::collect_files(root, &path, files)?;
            } else if kind.is_file() {
                let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
                files.push((relative.to_string_lossy().replace('\\', "/"), path));
            } else {
                return Err("extension source contains an unsupported file type".into());
            }
        }
        Ok(())
    }

    fn materialize(directory: &Path, request: &MaterializeRequest) -> Result<ExtEntry, String> {
        let entry = request.entry.as_ref();
        let mut snapshot = Index::default();
        snapshot.entries.push(entry.clone());
        snapshot.validate()?;
        let source = vmux_path::PathIdentity::resolve(&request.source).into_path_buf();
        let staging = vmux_path::PathIdentity::resolve(directory.join("staging")).into_path_buf();
        if !source.starts_with(&staging) || source == staging {
            return Err("extension source is outside its staging directory".into());
        }
        let hash = Self::source_hash(&source)?;
        if hash != entry.source_hash {
            return Err("staged extension source hash mismatch".into());
        }
        let destination = directory
            .join("packages")
            .join(&entry.id)
            .join(&entry.version)
            .join("source");
        if destination.exists() {
            if Self::source_hash(&destination)? != hash {
                return Err("extension version already contains different source".into());
            }
            std::fs::remove_dir_all(&source).map_err(|error| error.to_string())?;
        } else {
            std::fs::create_dir_all(destination.parent().unwrap())
                .map_err(|error| error.to_string())?;
            std::fs::rename(&source, &destination).map_err(|error| error.to_string())?;
        }
        Ok(entry.clone())
    }

    pub fn read(directory: &Path) -> Result<Index, String> {
        let source = match std::fs::read_to_string(directory.join("index.json")) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Index::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        Index::decode(&source)
    }

    pub fn write(directory: &Path, snapshot: &Index) -> Result<(), String> {
        snapshot.validate()?;
        let source = serde_json::to_string_pretty(snapshot).map_err(|error| error.to_string())?;
        vmux_path::AtomicFile::write(directory.join("index.json"), source.as_bytes())
            .map_err(|error| error.to_string())
    }

    fn cleanup(directory: &Path, request: &CleanupRequest) -> Result<(), String> {
        if request.id.len() != 32 || !request.id.bytes().all(|byte| (b'a'..=b'p').contains(&byte)) {
            return Err("invalid extension cleanup identity".into());
        }
        let mut profile = Path::new(&request.profile).components();
        if !matches!(profile.next(), Some(std::path::Component::Normal(_)))
            || profile.next().is_some()
        {
            return Err("invalid extension cleanup profile".into());
        }
        let runtime = directory
            .join("runtime")
            .join(&request.profile)
            .join(&request.id);
        if runtime.exists() {
            std::fs::remove_dir_all(runtime).map_err(|error| error.to_string())?;
        }
        if request.package {
            for path in [
                directory.join(&request.id),
                directory.join("packages").join(&request.id),
            ] {
                if path.exists() {
                    std::fs::remove_dir_all(path).map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Resource, Default)]
struct Clock(u64);

impl Clock {
    fn next(&mut self) -> u64 {
        self.0 = self.0.checked_add(1).expect("extension IO order exhausted");
        self.0
    }
}

#[derive(Clone)]
enum Work {
    Read(Ticket),
    Write(WriteTaskRequest),
    Cleanup(CleanupRequest),
    Materialize(MaterializeRequest),
}

impl Work {
    fn ticket(&self) -> &Ticket {
        match self {
            Self::Read(ticket) => ticket,
            Self::Write(request) => &request.ticket,
            Self::Cleanup(request) => &request.ticket,
            Self::Materialize(request) => &request.ticket,
        }
    }
}

#[derive(Component)]
struct Queued {
    order: u64,
    work: Work,
}

#[derive(Component)]
struct Executing(Work);

#[derive(Clone)]
struct FileJob {
    directory: std::path::PathBuf,
    work: Work,
}

#[derive(Clone)]
enum FileOutput {
    Index(Index),
    Materialized(Box<ExtEntry>),
}

impl Job for FileJob {
    type Output = Result<FileOutput, String>;

    fn run(self) -> Self::Output {
        match self.work {
            Work::Read(_) => FileStore::read(&self.directory).map(FileOutput::Index),
            Work::Write(request) => {
                FileStore::write(&self.directory, &request.snapshot)?;
                Ok(FileOutput::Index(Index::default()))
            }
            Work::Cleanup(request) => {
                FileStore::cleanup(&self.directory, &request)?;
                Ok(FileOutput::Index(Index::default()))
            }
            Work::Materialize(request) => FileStore::materialize(&self.directory, &request)
                .map(|entry| FileOutput::Materialized(Box::new(entry))),
        }
    }
}

#[derive(SystemParam)]
struct FileRequests<'w, 's> {
    reads: MessageReader<'w, 's, LoadRequest>,
    writes: MessageReader<'w, 's, WriteTaskRequest>,
    cleanup: MessageReader<'w, 's, CleanupRequest>,
    materialize: MessageReader<'w, 's, MaterializeRequest>,
}

fn receive(
    mut requests: FileRequests,
    roots: Query<(Entity, &Root, &Scope)>,
    mut completed: MessageWriter<LoadCompleted>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    let mut work = Vec::new();
    for request in requests.reads.read() {
        let ticket = &request.0;
        let directory = vmux_path::PathIdentity::resolve(&ticket.directory).into_path_buf();
        if roots.iter().any(|(owner, root, _)| {
            owner != ticket.owner
                && vmux_path::PathIdentity::resolve(&root.0).as_path() == directory
        }) {
            completed.write(LoadCompleted {
                ticket: ticket.clone(),
                result: Err("extension store has multiple owners".into()),
            });
            continue;
        }
        work.push(Work::Read(ticket.clone()));
    }
    for request in requests.cleanup.read() {
        work.push(Work::Cleanup(request.clone()));
    }
    for request in requests.writes.read() {
        work.push(Work::Write(request.clone()));
    }
    for request in requests.materialize.read() {
        work.push(Work::Materialize(request.clone()));
    }
    for work in work {
        let ticket = work.ticket();
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(_, root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            continue;
        }
        let directory = vmux_path::PathIdentity::resolve(&ticket.directory).into_path_buf();
        commands.spawn((
            Cwd(directory),
            Operation::bundle("extension.file").unwrap(),
            Queued {
                order: clock.next(),
                work,
            },
        ));
    }
}

fn start(
    queued: Query<(Entity, &Cwd, &Queued)>,
    active: Query<&Cwd, With<Executing>>,
    roots: Query<(&Root, &Scope)>,
    installs: Query<(&InstallRequest, &InstallOwner, &vmux_ecs::OperationState)>,
    wake: Option<Res<JobWake>>,
    mut commands: Commands,
) {
    let mut occupied = HashSet::new();
    for directory in &active {
        occupied.insert(directory.0.clone());
    }
    let mut pending = Vec::new();
    for (entity, directory, queued) in &queued {
        pending.push((queued.order, entity, directory, &queued.work));
    }
    pending.sort_by_key(|pending| pending.0);
    for (_, entity, directory, work) in pending {
        let ticket = work.ticket();
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if let Work::Materialize(request) = work
            && !installs
                .get(request.operation)
                .is_ok_and(|(install, _, state)| {
                    install.ticket == request.ticket
                        && install.id == request.entry.id
                        && matches!(
                            state,
                            vmux_ecs::OperationState::Pending | vmux_ecs::OperationState::Running
                        )
                })
        {
            commands.entity(entity).despawn();
            continue;
        }
        if !occupied.insert(directory.0.clone()) {
            continue;
        }
        commands.entity(entity).remove::<Queued>().insert((
            Executing(work.clone()),
            JobRequest(FileJob {
                directory: directory.0.clone(),
                work: work.clone(),
            }),
        ));
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn finish(
    tasks: Query<(Entity, &Executing, &JobResult<FileJob>)>,
    mut loaded: MessageWriter<LoadCompleted>,
    mut written: MessageWriter<WriteCompleted>,
    mut cleaned: MessageWriter<CleanupCompleted>,
    mut materialized: MessageWriter<MaterializeCompleted>,
    mut commands: Commands,
) {
    for (entity, executing, result) in &tasks {
        let result = match &result.0 {
            Ok(result) => result.clone(),
            Err(error) => Err(error.clone()),
        };
        let completion = match &result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        match &executing.0 {
            Work::Read(ticket) => {
                let result = match result {
                    Ok(FileOutput::Index(index)) => Ok(index),
                    Ok(_) => Err("unexpected extension read result".into()),
                    Err(error) => Err(error),
                };
                loaded.write(LoadCompleted {
                    ticket: ticket.clone(),
                    result,
                });
            }
            Work::Write(request) => {
                written.write(WriteCompleted {
                    ticket: request.ticket.clone(),
                    operation: request.operation,
                    revision: request.revision,
                    result: completion.clone(),
                });
            }
            Work::Cleanup(request) => {
                cleaned.write(CleanupCompleted {
                    ticket: request.ticket.clone(),
                    id: request.id.clone(),
                    profile: request.profile.clone(),
                    result: completion.clone(),
                });
            }
            Work::Materialize(request) => {
                let result = match result {
                    Ok(FileOutput::Materialized(entry)) => Ok(*entry),
                    Ok(_) => Err("unexpected extension materialization result".into()),
                    Err(error) => Err(error),
                };
                materialized.write(MaterializeCompleted {
                    ticket: request.ticket.clone(),
                    operation: request.operation,
                    result,
                });
            }
        }
        commands.trigger(OperationCompleted {
            entity,
            result: completion,
        });
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::task::Wake;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::{Catalog, CorePlugin, Loaded, Mutation, Request};
    use bevy_ecs::system::SystemState;
    use vmux_api::extension::{ExtPinRequest, ExtUninstallRequest};
    use vmux_ecs::OperationState;

    const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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
        fn new(directory: &Path) -> Self {
            let (sender, wake) = mpsc::channel();
            let mut app = App::new();
            app.add_plugins((CorePlugin, FilesystemPlugin))
                .insert_resource(JobWake(Arc::new(HostWake(sender)).into()));
            let root = app.world_mut().spawn(Root(directory.to_path_buf())).id();
            Self { app, root, wake }
        }

        fn load(&mut self) {
            self.app.update();
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.app.world().get::<Loaded>(self.root).is_none() {
                assert!(Instant::now() < deadline, "store load did not complete");
                self.wake.recv_timeout(Duration::from_secs(2)).unwrap();
                self.app.update();
            }
        }

        fn submit(&mut self, profile: &str, mutation: Mutation) -> Entity {
            let mut state = SystemState::<Catalog>::new(self.app.world_mut());
            let ticket = state
                .get(self.app.world())
                .unwrap()
                .ticket(self.root)
                .unwrap()
                .clone();
            let operation = self
                .app
                .world_mut()
                .spawn(
                    Request {
                        ticket,
                        profile: profile.into(),
                        mutation,
                    }
                    .bundle(),
                )
                .id();
            self.app.update();
            operation
        }

        fn complete(&mut self, operation: Entity) -> OperationState {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let state = self.app.world().get::<OperationState>(operation).unwrap();
                if matches!(
                    state,
                    OperationState::Succeeded | OperationState::Failed | OperationState::Canceled
                ) {
                    return state.clone();
                }
                assert!(Instant::now() < deadline, "operation did not complete");
                self.wake.recv_timeout(Duration::from_secs(2)).unwrap();
                self.app.update();
            }
        }

        fn stage(&mut self, source: &Path, entry: ExtEntry) -> MaterializeCompleted {
            let mut state = SystemState::<Catalog>::new(self.app.world_mut());
            let ticket = state
                .get(self.app.world())
                .unwrap()
                .ticket(self.root)
                .unwrap()
                .clone();
            let operation = self
                .app
                .world_mut()
                .spawn((
                    Operation::bundle("extension.install").unwrap().running(),
                    InstallRequest {
                        ticket: ticket.clone(),
                        profile: "personal".into(),
                        id: entry.id.clone(),
                    },
                ))
                .id();
            self.app.update();
            self.app.world_mut().write_message(MaterializeRequest::new(
                ticket,
                operation,
                Box::new(entry),
                source.to_path_buf(),
            ));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                self.app.update();
                let result = self
                    .app
                    .world_mut()
                    .resource_mut::<Messages<MaterializeCompleted>>()
                    .drain()
                    .next();
                if let Some(result) = result {
                    return result;
                }
                assert!(
                    Instant::now() < deadline,
                    "materialization did not complete"
                );
                self.wake.recv_timeout(Duration::from_secs(2)).unwrap();
            }
        }

        fn hold_file_jobs(&mut self) -> (vmux_ecs::BlockingTask<()>, mpsc::Sender<()>) {
            self.app.insert_resource(vmux_ecs::BlockingRuntime::new(
                vmux_ecs::BlockingPool::new("test-extension-short", 1, 8),
                vmux_ecs::BlockingPool::new("test-extension-long", 1, 8),
                vmux_ecs::BlockingPool::new("test-extension-process", 1, 8),
            ));
            let (started, entered) = mpsc::channel();
            let (release, wait) = mpsc::channel();
            let task = self
                .app
                .world()
                .resource::<vmux_ecs::BlockingRuntime>()
                .try_spawn(vmux_ecs::blocking::Lane::Short, None, move || {
                    started.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                })
                .unwrap();
            entered.recv_timeout(Duration::from_secs(2)).unwrap();
            (task, release)
        }

        fn settle(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let running = self
                    .app
                    .world_mut()
                    .query_filtered::<Entity, Or<(With<Queued>, With<Executing>)>>()
                    .iter(self.app.world())
                    .next()
                    .is_some();
                let dirty = self
                    .app
                    .world()
                    .get::<crate::State>(self.root)
                    .unwrap()
                    .is_dirty();
                if !running && !dirty {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "replacement store did not settle"
                );
                self.wake.recv_timeout(Duration::from_secs(2)).unwrap();
                self.app.update();
            }
        }
    }

    #[test]
    fn staged_source_is_immutable_and_catalog_commit_waits_for_index_io() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        let source = directory.path().join("staging/first/unpacked");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("manifest.json"),
            r#"{"name":"Example","version":"1"}"#,
        )
        .unwrap();
        let entry = ExtEntry {
            id: ID.into(),
            name: "Example".into(),
            version: "1".into(),
            source_hash: FileStore::source_hash(&source).unwrap(),
            ..Default::default()
        };
        let staged = host.stage(&source, entry.clone());
        let entry = staged.result.unwrap();
        assert!(
            FileStore::read(directory.path())
                .unwrap()
                .entries
                .is_empty()
        );
        host.app
            .world_mut()
            .entity_mut(staged.operation)
            .insert(Request {
                ticket: staged.ticket,
                profile: "personal".into(),
                mutation: Mutation::Install(Box::new(entry.clone())),
            });
        host.app.update();
        assert_eq!(
            host.complete(staged.operation),
            vmux_ecs::OperationState::Succeeded
        );
        let index = FileStore::read(directory.path()).unwrap();
        assert!(index.entries[0].installed_for("personal"));
        let destination = directory.path().join("packages").join(ID).join("1/source");
        let before = std::fs::read(destination.join("manifest.json")).unwrap();
        let source = directory.path().join("staging/second/unpacked");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("manifest.json"),
            r#"{"name":"Changed","version":"1"}"#,
        )
        .unwrap();
        let mut changed = entry;
        changed.source_hash = FileStore::source_hash(&source).unwrap();
        let refused = host.stage(&source, changed);
        assert!(refused.result.unwrap_err().contains("different source"));
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            before
        );
        assert_eq!(FileStore::read(directory.path()).unwrap(), index);
    }

    #[test]
    fn reseeding_the_same_store_compensates_an_older_write_before_becoming_clean() {
        let directory = tempfile::tempdir().unwrap();
        let mut original = Index::default();
        let mut entry = ExtEntry {
            id: ID.into(),
            name: "Example".into(),
            version: "1".into(),
            ..Default::default()
        };
        entry.profile_enabled.insert("personal".into(), false);
        entry.profile_pinned.insert("personal".into(), false);
        entry.approved_grants.insert(
            "personal".into(),
            crate::snapshot::ExtensionGrants::default(),
        );
        original.entries.push(entry);
        FileStore::write(directory.path(), &original).unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        let operation = host.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: ID.into(),
                pinned: true,
            }),
        );
        host.app
            .world_mut()
            .entity_mut(host.root)
            .insert(crate::Seed(Ok(original.clone())));
        host.app.update();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let running = host
                .app
                .world_mut()
                .query_filtered::<Entity, Or<(With<Queued>, With<Executing>)>>()
                .iter(host.app.world())
                .next()
                .is_some();
            let dirty = host
                .app
                .world()
                .get::<crate::State>(host.root)
                .unwrap()
                .is_dirty();
            if !running && !dirty {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "replacement store did not settle"
            );
            host.wake.recv_timeout(Duration::from_secs(2)).unwrap();
            host.app.update();
        }
        let mut catalog = SystemState::<Catalog>::new(host.app.world_mut());
        let snapshot = catalog
            .get(host.app.world())
            .unwrap()
            .snapshot(host.root)
            .unwrap();
        assert_eq!(FileStore::read(directory.path()).unwrap(), snapshot);
        assert_eq!(snapshot, original);
        assert_eq!(
            host.app.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
    }

    #[cfg(unix)]
    #[test]
    fn repeated_seed_and_an_alias_preserve_repair_until_started_io_settles() {
        for alias in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut original = Index::default();
            let mut entry = ExtEntry {
                id: ID.into(),
                name: "Example".into(),
                version: "1".into(),
                ..Default::default()
            };
            entry.profile_enabled.insert("personal".into(), false);
            entry.profile_pinned.insert("personal".into(), false);
            entry.approved_grants.insert(
                "personal".into(),
                crate::snapshot::ExtensionGrants::default(),
            );
            original.entries.push(entry);
            FileStore::write(directory.path(), &original).unwrap();
            let mut host = Host::new(directory.path());
            host.load();
            let (blocker, release) = host.hold_file_jobs();
            host.submit(
                "personal",
                Mutation::Pin(ExtPinRequest {
                    id: ID.into(),
                    pinned: true,
                }),
            );
            if alias {
                let path = directory.path().join("alias");
                std::os::unix::fs::symlink(directory.path(), &path).unwrap();
                host.app
                    .world_mut()
                    .entity_mut(host.root)
                    .insert(Root(path));
            }
            host.app
                .world_mut()
                .entity_mut(host.root)
                .insert(crate::Seed(Err("temporary read failure".into())));
            host.app.update();
            host.app
                .world_mut()
                .entity_mut(host.root)
                .insert(crate::Seed(Ok(original.clone())));
            host.app.update();
            release.send(()).unwrap();
            host.settle();
            drop(blocker);
            let mut catalog = SystemState::<Catalog>::new(host.app.world_mut());
            let snapshot = catalog
                .get(host.app.world())
                .unwrap()
                .snapshot(host.root)
                .unwrap();
            assert_eq!(FileStore::read(directory.path()).unwrap(), snapshot);
            assert_eq!(snapshot, original);
        }
    }

    #[test]
    fn legacy_index_restores_and_atomic_io_commits_the_selected_profile() {
        let directory = tempfile::tempdir().unwrap();
        let source = format!(
            r#"{{"entries":[{{"id":"{ID}","name":"Legacy","version":"1","popup":null,"icon":null,"enabled":true,"permissions":["tabs"]}}]}}"#
        );
        std::fs::write(directory.path().join("index.json"), source).unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        let snapshot = FileStore::read(directory.path()).unwrap();
        assert!(snapshot.entries[0].enabled_for("personal"));
        assert!(
            snapshot.entries[0]
                .grants_for("personal")
                .covers(&["tabs".into()], &[])
        );
        let operation = host.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: ID.into(),
                pinned: true,
            }),
        );
        assert_eq!(host.complete(operation), OperationState::Succeeded);
        let snapshot = FileStore::read(directory.path()).unwrap();
        assert!(snapshot.entries[0].pinned_for("personal"));
        assert!(!snapshot.entries[0].installed_for("work"));
    }

    #[test]
    fn deleting_one_association_preserves_another_profile_and_shared_package_files() {
        let directory = tempfile::tempdir().unwrap();
        let source = format!(
            r#"{{"version":3,"entries":[{{"id":"{ID}","name":"Shared","version":"1","popup":null,"icon":null,"enabled":false,"profile_enabled":{{"personal":false,"work":false}}}}]}}"#
        );
        std::fs::write(directory.path().join("index.json"), source).unwrap();
        let personal = directory.path().join("runtime/personal").join(ID);
        let work = directory.path().join("runtime/work").join(ID);
        let package = directory.path().join("packages").join(ID);
        for path in [&personal, &work, &package] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut host = Host::new(directory.path());
        host.load();
        let operation = host.submit(
            "personal",
            Mutation::Remove(ExtUninstallRequest { id: ID.into() }),
        );
        assert_eq!(host.complete(operation), OperationState::Succeeded);
        let deadline = Instant::now() + Duration::from_secs(5);
        while personal.exists() {
            assert!(
                Instant::now() < deadline,
                "runtime cleanup did not complete"
            );
            host.wake.recv_timeout(Duration::from_secs(2)).unwrap();
            host.app.update();
        }
        assert!(work.exists());
        assert!(package.exists());
        let snapshot = FileStore::read(directory.path()).unwrap();
        assert!(snapshot.entries[0].installed_for("work"));
        assert!(!snapshot.entries[0].installed_for("personal"));
    }

    #[test]
    fn failed_file_commit_keeps_current_state_and_retry_succeeds_after_io_recovers() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("index.json");
        let source = format!(
            r#"{{"version":3,"entries":[{{"id":"{ID}","name":"Shared","version":"1","popup":null,"icon":null,"enabled":false,"profile_enabled":{{"personal":false}}}}]}}"#
        );
        std::fs::write(&path, &source).unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let operation = host.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: ID.into(),
                pinned: true,
            }),
        );
        assert_eq!(host.complete(operation), OperationState::Failed);
        let mut state = SystemState::<Catalog>::new(host.app.world_mut());
        assert!(
            !state
                .get(host.app.world())
                .unwrap()
                .snapshot(host.root)
                .unwrap()
                .entries[0]
                .pinned_for("personal")
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, source).unwrap();
        let retry = host.submit(
            "personal",
            Mutation::Pin(ExtPinRequest {
                id: ID.into(),
                pinned: true,
            }),
        );
        assert_eq!(host.complete(retry), OperationState::Succeeded);
        assert!(FileStore::read(directory.path()).unwrap().entries[0].pinned_for("personal"));
    }
}
