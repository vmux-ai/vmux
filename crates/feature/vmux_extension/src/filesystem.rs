use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
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
        app.add_plugins((
            JobPlugin::<ReadJob>::default(),
            JobPlugin::<WriteJob>::default(),
            JobPlugin::<CleanupJob>::default(),
            JobPlugin::<MaterializeJob>::default(),
        ))
        .add_message::<CleanupCompleted>()
        .add_message::<MaterializeRequest>()
        .add_message::<MaterializeCompleted>()
        .init_resource::<Clock>()
        .add_systems(
            Update,
            (
                finish_reads,
                finish_writes,
                finish_cleanups,
                finish_materializations,
            )
                .in_set(JobSet::Apply)
                .before(ExtensionSet::Step),
        )
        .add_systems(
            Update,
            (
                receive_reads,
                receive_cleanups,
                receive_writes,
                receive_materializations,
                start,
            )
                .chain()
                .after(ExtensionSet::Step),
        )
        .add_observer(read)
        .add_observer(write)
        .add_observer(cleanup)
        .add_observer(materialize);
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

    #[cfg(all(host, feature = "host"))]
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
    pub fn source_dir(directory: &Path, id: &str, version: &str) -> std::path::PathBuf {
        directory
            .join("packages")
            .join(id)
            .join(version)
            .join("source")
    }

    pub fn runtime_dir(directory: &Path, profile: &str, id: &str) -> std::path::PathBuf {
        directory.join("runtime").join(profile).join(id)
    }

    pub fn read_loaded_ids(directory: &Path, profile: &str) -> Vec<String> {
        let loaded_path = directory.join("loaded").join(format!("{profile}.txt"));
        std::fs::read_to_string(loaded_path)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    std::fs::read_to_string(directory.join("loaded.txt"))
                } else {
                    Err(error)
                }
            })
            .ok()
            .map(|contents| {
                contents
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn write_loaded_ids(directory: &Path, profile: &str, ids: &[String]) -> Result<(), String> {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let path = directory.join("loaded").join(format!("{profile}.txt"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, ids.join("\n")).map_err(|error| error.to_string())
    }

    pub fn migrate_legacy_package(
        directory: &Path,
        id: &str,
        version: &str,
        expected_source_hash: &str,
    ) -> Result<std::path::PathBuf, String> {
        let source = Self::source_dir(directory, id, version);
        if source.exists() {
            Self::validate_manifest_object(&source)?;
            let hash = Self::source_hash(&source)?;
            if !expected_source_hash.is_empty() && hash != expected_source_hash {
                return Err(format!("source hash mismatch for {id}"));
            }
            return Ok(source);
        }
        let legacy = directory.join(id);
        if !legacy.is_dir() {
            return Err(format!("legacy extension package not found: {id}"));
        }
        let parent = source.parent().ok_or("source directory has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = parent.join("source.tmp");
        if temporary.exists() {
            std::fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
        }
        Self::copy_legacy_tree(&legacy, &temporary)?;
        Self::restore_legacy_worker(&temporary)?;
        Self::validate_manifest_object(&temporary)?;
        Self::source_hash(&temporary)?;
        std::fs::rename(&temporary, &source).map_err(|error| error.to_string())?;
        Ok(source)
    }

    fn copy_legacy_tree(source: &Path, destination: &Path) -> Result<(), String> {
        std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            let target = destination.join(entry.file_name());
            if file_type.is_dir() {
                Self::copy_legacy_tree(&entry.path(), &target)?;
            } else if file_type.is_file() {
                std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
            } else {
                return Err(format!(
                    "unsupported legacy package entry: {}",
                    entry.path().display()
                ));
            }
        }
        Ok(())
    }

    fn is_generated_shim_file(name: &str) -> bool {
        name == "vmux_patch.js"
            || name == "vmux_shim.js"
            || name == "vmux_shim.json"
            || name.starts_with("vmux_sw_") && name.ends_with(".js")
    }

    fn remove_generated_shim_files(dir: &Path) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                Self::remove_generated_shim_files(&path)?;
            } else if Self::is_generated_shim_file(&entry.file_name().to_string_lossy()) {
                std::fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn restore_legacy_worker(dir: &Path) -> Result<(), String> {
        let sidecar_path = dir.join("vmux_shim.json");
        if sidecar_path.exists() {
            let sidecar: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&sidecar_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let original = sidecar
                .get("original")
                .and_then(serde_json::Value::as_str)
                .ok_or("legacy shim sidecar has no original worker")?;
            if Self::is_generated_shim_file(original) {
                return Err("legacy shim sidecar points to a generated worker".into());
            }
            let manifest_path = dir.join("manifest.json");
            let mut manifest: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let background = manifest
                .get_mut("background")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or("legacy manifest has no background object")?;
            background.insert(
                "service_worker".into(),
                serde_json::Value::String(original.into()),
            );
            std::fs::write(
                manifest_path,
                serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
        Self::remove_generated_shim_files(dir)
    }

    fn validate_manifest_object(dir: &Path) -> Result<(), String> {
        let manifest_path = dir.join("manifest.json");
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&manifest_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if !manifest.is_object() {
            return Err("manifest is not an object".into());
        }
        Ok(())
    }

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

#[derive(Component)]
struct Queued(u64);

impl Queued {
    fn bundle(ticket: Ticket, directory: std::path::PathBuf, order: u64) -> impl Bundle {
        (
            Cwd(directory),
            Operation::bundle("extension.file").unwrap(),
            Self(order),
            Lease(ticket),
        )
    }
}

#[derive(Component)]
struct Lease(Ticket);

#[derive(Component)]
struct Executing;

#[derive(EntityEvent)]
struct Starting {
    entity: Entity,
}

#[derive(Component, Clone)]
struct ReadJob {
    directory: std::path::PathBuf,
    request: LoadRequest,
}

impl Job for ReadJob {
    type Output = Result<Index, String>;

    fn run(self) -> Self::Output {
        FileStore::read(&self.directory)
    }
}

#[derive(Component, Clone)]
struct WriteJob {
    directory: std::path::PathBuf,
    request: WriteTaskRequest,
}

impl Job for WriteJob {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        FileStore::write(&self.directory, &self.request.snapshot)
    }
}

#[derive(Component, Clone)]
struct CleanupJob {
    directory: std::path::PathBuf,
    request: CleanupRequest,
}

impl Job for CleanupJob {
    type Output = Result<(), String>;

    fn run(self) -> Self::Output {
        FileStore::cleanup(&self.directory, &self.request)
    }
}

#[derive(Component, Clone)]
struct MaterializeJob {
    directory: std::path::PathBuf,
    request: MaterializeRequest,
}

impl Job for MaterializeJob {
    type Output = Result<ExtEntry, String>;

    fn run(self) -> Self::Output {
        FileStore::materialize(&self.directory, &self.request)
    }
}

fn receive_reads(
    mut requests: MessageReader<LoadRequest>,
    roots: Query<(Entity, &Root, &Scope)>,
    mut completed: MessageWriter<LoadCompleted>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    for request in requests.read() {
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
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(_, root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            continue;
        }
        commands.spawn((
            Queued::bundle(ticket.clone(), directory.clone(), clock.next()),
            ReadJob {
                directory,
                request: request.clone(),
            },
        ));
    }
}

fn receive_cleanups(
    mut requests: MessageReader<CleanupRequest>,
    roots: Query<(&Root, &Scope)>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let ticket = &request.ticket;
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            continue;
        }
        let directory = vmux_path::PathIdentity::resolve(&ticket.directory).into_path_buf();
        commands.spawn((
            Queued::bundle(ticket.clone(), directory.clone(), clock.next()),
            CleanupJob {
                directory,
                request: request.clone(),
            },
        ));
    }
}

fn receive_writes(
    mut requests: MessageReader<WriteTaskRequest>,
    roots: Query<(&Root, &Scope)>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let ticket = &request.ticket;
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            continue;
        }
        let directory = vmux_path::PathIdentity::resolve(&ticket.directory).into_path_buf();
        commands.spawn((
            Queued::bundle(ticket.clone(), directory.clone(), clock.next()),
            WriteJob {
                directory,
                request: request.clone(),
            },
        ));
    }
}

fn receive_materializations(
    mut requests: MessageReader<MaterializeRequest>,
    roots: Query<(&Root, &Scope)>,
    mut clock: ResMut<Clock>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let ticket = &request.ticket;
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            continue;
        }
        let directory = vmux_path::PathIdentity::resolve(&ticket.directory).into_path_buf();
        commands.spawn((
            Queued::bundle(ticket.clone(), directory.clone(), clock.next()),
            MaterializeJob {
                directory,
                request: request.clone(),
            },
        ));
    }
}

fn start(
    queued: Query<(Entity, &Cwd, &Queued, &Lease, Option<&MaterializeJob>)>,
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
    for (entity, directory, queued, lease, materialize) in &queued {
        pending.push((queued.0, entity, directory, &lease.0, materialize));
    }
    pending.sort_by_key(|pending| pending.0);
    for (_, entity, directory, ticket, materialize) in pending {
        if !roots
            .get(ticket.owner)
            .is_ok_and(|(root, scope)| root.0 == ticket.directory && scope.0 == *ticket)
        {
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(job) = materialize
            && !installs
                .get(job.request.operation)
                .is_ok_and(|(install, _, state)| {
                    install.ticket == job.request.ticket
                        && install.id == job.request.entry.id
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
        commands.entity(entity).remove::<Queued>().insert(Executing);
        commands.trigger(Starting { entity });
        if let Some(wake) = &wake {
            wake.0.wake_by_ref();
        }
    }
}

fn read(event: On<Starting>, jobs: Query<&ReadJob>, mut commands: Commands) {
    if let Ok(job) = jobs.get(event.entity) {
        commands
            .entity(event.entity)
            .insert(JobRequest(job.clone()));
    }
}

fn write(event: On<Starting>, jobs: Query<&WriteJob>, mut commands: Commands) {
    if let Ok(job) = jobs.get(event.entity) {
        commands
            .entity(event.entity)
            .insert(JobRequest(job.clone()));
    }
}

fn cleanup(event: On<Starting>, jobs: Query<&CleanupJob>, mut commands: Commands) {
    if let Ok(job) = jobs.get(event.entity) {
        commands
            .entity(event.entity)
            .insert(JobRequest(job.clone()));
    }
}

fn materialize(event: On<Starting>, jobs: Query<&MaterializeJob>, mut commands: Commands) {
    if let Ok(job) = jobs.get(event.entity) {
        commands
            .entity(event.entity)
            .insert(JobRequest(job.clone()));
    }
}

fn finish_reads(
    tasks: Query<(Entity, &ReadJob, &JobResult<ReadJob>), With<Executing>>,
    mut completed: MessageWriter<LoadCompleted>,
    mut commands: Commands,
) {
    for (entity, job, result) in &tasks {
        let result = match &result.0 {
            Ok(result) => result.clone(),
            Err(error) => Err(error.clone()),
        };
        let completion = match &result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        completed.write(LoadCompleted {
            ticket: job.request.0.clone(),
            result,
        });
        commands.trigger(OperationCompleted {
            entity,
            result: completion,
        });
        commands.entity(entity).despawn();
    }
}

fn finish_writes(
    tasks: Query<(Entity, &WriteJob, &JobResult<WriteJob>), With<Executing>>,
    mut completed: MessageWriter<WriteCompleted>,
    mut commands: Commands,
) {
    for (entity, job, result) in &tasks {
        let result = match &result.0 {
            Ok(result) => result.clone(),
            Err(error) => Err(error.clone()),
        };
        let completion = match &result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        completed.write(WriteCompleted {
            ticket: job.request.ticket.clone(),
            operation: job.request.operation,
            revision: job.request.revision,
            result,
        });
        commands.trigger(OperationCompleted {
            entity,
            result: completion,
        });
        commands.entity(entity).despawn();
    }
}

fn finish_cleanups(
    tasks: Query<(Entity, &CleanupJob, &JobResult<CleanupJob>), With<Executing>>,
    mut completed: MessageWriter<CleanupCompleted>,
    mut commands: Commands,
) {
    for (entity, job, result) in &tasks {
        let result = match &result.0 {
            Ok(result) => result.clone(),
            Err(error) => Err(error.clone()),
        };
        let completion = match &result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        completed.write(CleanupCompleted {
            ticket: job.request.ticket.clone(),
            id: job.request.id.clone(),
            profile: job.request.profile.clone(),
            result,
        });
        commands.trigger(OperationCompleted {
            entity,
            result: completion,
        });
        commands.entity(entity).despawn();
    }
}

fn finish_materializations(
    tasks: Query<(Entity, &MaterializeJob, &JobResult<MaterializeJob>), With<Executing>>,
    mut completed: MessageWriter<MaterializeCompleted>,
    mut commands: Commands,
) {
    for (entity, job, result) in &tasks {
        let result = match &result.0 {
            Ok(result) => result.clone(),
            Err(error) => Err(error.clone()),
        };
        let completion = match &result {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        };
        completed.write(MaterializeCompleted {
            ticket: job.request.ticket.clone(),
            operation: job.request.operation,
            result,
        });
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
    use crate::{Catalog, CorePlugin, Install, Loaded, Pin, Uninstall};
    use bevy_ecs::system::SystemState;
    use vmux_api::extension::{ExtPinRequest, ExtUninstallRequest};
    use vmux_ecs::OperationState;

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

    struct ManualStore {
        app: App,
        root: Entity,
    }

    impl ManualStore {
        fn new(directory: &Path, snapshot: Index) -> Self {
            FileStore::write(directory, &snapshot).unwrap();
            let mut app = App::new();
            app.add_plugins(CorePlugin);
            let root = app
                .world_mut()
                .spawn((Root(directory.to_path_buf()), crate::Seed(Ok(snapshot))))
                .id();
            app.update();
            Self { app, root }
        }

        fn ticket(&self) -> Ticket {
            self.app.world().get::<Scope>(self.root).unwrap().0.clone()
        }

        fn submit<B: Bundle>(&mut self, request: B) -> WriteTaskRequest {
            self.app.world_mut().spawn(request);
            self.app.update();
            self.write()
        }

        fn write(&mut self) -> WriteTaskRequest {
            self.app
                .world_mut()
                .resource_mut::<Messages<WriteTaskRequest>>()
                .drain()
                .next()
                .unwrap()
        }

        fn finish(&mut self, request: &WriteTaskRequest, result: Result<(), String>) {
            self.app.world_mut().write_message(WriteCompleted {
                ticket: request.ticket.clone(),
                operation: request.operation,
                revision: request.revision,
                result,
            });
            self.app.update();
        }

        fn reseed(&mut self, snapshot: Index) {
            self.app
                .world_mut()
                .entity_mut(self.root)
                .insert(crate::Seed(Ok(snapshot)));
            self.app.update();
        }

        fn snapshot(&mut self) -> Index {
            let mut catalog = SystemState::<Catalog>::new(self.app.world_mut());
            catalog
                .get(self.app.world())
                .unwrap()
                .snapshot(self.root)
                .unwrap()
        }
    }

    impl Host {
        const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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

        fn ticket(&self) -> Ticket {
            self.app.world().get::<Scope>(self.root).unwrap().0.clone()
        }

        fn submit<B: Bundle>(&mut self, request: B) -> Entity {
            let operation = self.app.world_mut().spawn(request).id();
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
    fn read_priority_and_directory_serialization_survive_distinct_job_types() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        host.app
            .world_mut()
            .resource_mut::<Messages<LoadCompleted>>()
            .drain()
            .for_each(drop);
        let mut catalog = SystemState::<Catalog>::new(host.app.world_mut());
        let ticket = catalog
            .get(host.app.world())
            .unwrap()
            .ticket(host.root)
            .unwrap()
            .clone();
        let mut next = Index::default();
        next.entries.push(ExtEntry {
            id: Host::ID.into(),
            name: "Queued write".into(),
            version: "1".into(),
            ..Default::default()
        });
        let operation = host
            .app
            .world_mut()
            .spawn(Operation::bundle("extension.write").unwrap())
            .id();
        let (blocker, release) = host.hold_file_jobs();
        host.app.world_mut().write_message(WriteTaskRequest {
            ticket: ticket.clone(),
            operation,
            revision: 1,
            snapshot: next.clone(),
        });
        host.app.world_mut().write_message(LoadRequest(ticket));
        host.app.update();
        host.app.update();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        while reads.is_empty() || writes.is_empty() {
            assert!(
                Instant::now() < deadline,
                "typed file jobs did not complete"
            );
            host.wake.recv_timeout(Duration::from_secs(2)).unwrap();
            host.app.update();
            reads.extend(
                host.app
                    .world_mut()
                    .resource_mut::<Messages<LoadCompleted>>()
                    .drain(),
            );
            writes.extend(
                host.app
                    .world_mut()
                    .resource_mut::<Messages<WriteCompleted>>()
                    .drain(),
            );
        }
        assert_eq!(reads.len(), 1);
        assert!(reads.pop().unwrap().result.unwrap().entries.is_empty());
        assert_eq!(writes.len(), 1);
        assert!(writes.pop().unwrap().result.is_ok());
        assert_eq!(FileStore::read(directory.path()).unwrap(), next);
        drop(blocker);
    }

    #[test]
    fn materialization_retains_source_ownership_until_io_completion() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        let staging = directory.path().join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let temporary = Arc::new(tempfile::tempdir_in(&staging).unwrap());
        let temporary_path = temporary.path().to_path_buf();
        let source = temporary.path().join("unpacked");
        std::fs::create_dir_all(&source).unwrap();
        let manifest = r#"{"name":"Owned source","version":"1"}"#;
        std::fs::write(source.join("manifest.json"), manifest).unwrap();
        let entry = ExtEntry {
            id: Host::ID.into(),
            name: "Owned source".into(),
            version: "1".into(),
            source_hash: FileStore::source_hash(&source).unwrap(),
            ..Default::default()
        };
        let mut catalog = SystemState::<Catalog>::new(host.app.world_mut());
        let ticket = catalog
            .get(host.app.world())
            .unwrap()
            .ticket(host.root)
            .unwrap()
            .clone();
        let operation = host
            .app
            .world_mut()
            .spawn((
                Operation::bundle("extension.install").unwrap().running(),
                InstallRequest {
                    ticket: ticket.clone(),
                    profile: "personal".into(),
                    id: Host::ID.into(),
                },
            ))
            .id();
        host.app.update();
        let (blocker, release) = host.hold_file_jobs();
        let mut request =
            MaterializeRequest::new(ticket, operation, Box::new(entry), source.clone());
        request._lifetime = Some(temporary);
        host.app.world_mut().write_message(request);
        host.app.update();
        host.app
            .world_mut()
            .resource_mut::<Messages<MaterializeRequest>>()
            .drain()
            .for_each(drop);
        host.app.update();
        assert!(temporary_path.is_dir());
        assert_eq!(
            std::fs::read_to_string(source.join("manifest.json")).unwrap(),
            manifest
        );
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let completion = loop {
            assert!(
                Instant::now() < deadline,
                "owned materialization did not complete"
            );
            host.wake.recv_timeout(Duration::from_secs(2)).unwrap();
            host.app.update();
            if let Some(completion) = host
                .app
                .world_mut()
                .resource_mut::<Messages<MaterializeCompleted>>()
                .drain()
                .next()
            {
                break completion;
            }
        };
        assert!(completion.result.is_ok());
        assert!(!temporary_path.exists());
        let destination = directory
            .path()
            .join("packages")
            .join(Host::ID)
            .join("1/source/manifest.json");
        assert_eq!(std::fs::read_to_string(destination).unwrap(), manifest);
        drop(blocker);
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
            id: Host::ID.into(),
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
            .insert(Install::prepared(
                staged.ticket,
                "personal".into(),
                entry.clone(),
            ));
        host.app.update();
        assert_eq!(
            host.complete(staged.operation),
            vmux_ecs::OperationState::Succeeded
        );
        let index = FileStore::read(directory.path()).unwrap();
        assert!(index.entries[0].installed_for("personal"));
        let destination = directory
            .path()
            .join("packages")
            .join(Host::ID)
            .join("1/source");
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
            id: Host::ID.into(),
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
        let operation = host.submit(Pin::bundle(
            host.ticket(),
            "personal".into(),
            ExtPinRequest {
                id: Host::ID.into(),
                pinned: true,
            },
        ));
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
                id: Host::ID.into(),
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
            host.submit(Pin::bundle(
                host.ticket(),
                "personal".into(),
                ExtPinRequest {
                    id: Host::ID.into(),
                    pinned: true,
                },
            ));
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
    fn failed_compensation_stays_suspended_and_reseed_retries_after_io_recovers() {
        let directory = tempfile::tempdir().unwrap();
        let mut original = Index::default();
        let mut entry = ExtEntry {
            id: Host::ID.into(),
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
        let mut host = ManualStore::new(directory.path(), original.clone());
        let old = host.submit(Pin::bundle(
            host.ticket(),
            "personal".into(),
            ExtPinRequest {
                id: Host::ID.into(),
                pinned: true,
            },
        ));
        FileStore::write(directory.path(), &old.snapshot).unwrap();
        host.reseed(original.clone());
        let repair = host.write();
        host.finish(&old, Ok(()));
        let path = directory.path().join("index.json");
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let failure = FileStore::write(directory.path(), &repair.snapshot);
        assert!(failure.is_err());
        host.finish(&repair, failure);
        for _ in 0..3 {
            host.app.update();
        }
        assert!(
            host.app
                .world_mut()
                .resource_mut::<Messages<WriteTaskRequest>>()
                .drain()
                .next()
                .is_none()
        );
        assert!(
            host.app
                .world()
                .get::<crate::State>(host.root)
                .unwrap()
                .is_dirty()
        );
        std::fs::remove_dir(&path).unwrap();
        FileStore::write(directory.path(), &old.snapshot).unwrap();
        assert!(FileStore::read(directory.path()).unwrap().entries[0].pinned_for("personal"));
        host.reseed(original.clone());
        let retry = host.write();
        let result = FileStore::write(directory.path(), &retry.snapshot);
        host.finish(&retry, result);
        let snapshot = host.snapshot();
        assert_eq!(FileStore::read(directory.path()).unwrap(), snapshot);
        assert_eq!(snapshot, original);
        assert!(
            !host
                .app
                .world()
                .get::<crate::State>(host.root)
                .unwrap()
                .is_dirty()
        );
    }

    #[test]
    fn legacy_index_restores_and_atomic_io_commits_the_selected_profile() {
        let directory = tempfile::tempdir().unwrap();
        let source = format!(
            r#"{{"entries":[{{"id":"{}","name":"Legacy","version":"1","popup":null,"icon":null,"enabled":true,"permissions":["tabs"]}}]}}"#,
            Host::ID
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
        let operation = host.submit(Pin::bundle(
            host.ticket(),
            "personal".into(),
            ExtPinRequest {
                id: Host::ID.into(),
                pinned: true,
            },
        ));
        assert_eq!(host.complete(operation), OperationState::Succeeded);
        let snapshot = FileStore::read(directory.path()).unwrap();
        assert!(snapshot.entries[0].pinned_for("personal"));
        assert!(!snapshot.entries[0].installed_for("work"));
    }

    #[test]
    fn deleting_one_association_preserves_another_profile_and_shared_package_files() {
        let directory = tempfile::tempdir().unwrap();
        let source = format!(
            r#"{{"version":3,"entries":[{{"id":"{}","name":"Shared","version":"1","popup":null,"icon":null,"enabled":false,"profile_enabled":{{"personal":false,"work":false}}}}]}}"#,
            Host::ID
        );
        std::fs::write(directory.path().join("index.json"), source).unwrap();
        let personal = directory.path().join("runtime/personal").join(Host::ID);
        let work = directory.path().join("runtime/work").join(Host::ID);
        let package = directory.path().join("packages").join(Host::ID);
        for path in [&personal, &work, &package] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut host = Host::new(directory.path());
        host.load();
        let operation = host.submit(Uninstall::bundle(
            host.ticket(),
            "personal".into(),
            ExtUninstallRequest {
                id: Host::ID.into(),
            },
        ));
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
            r#"{{"version":3,"entries":[{{"id":"{}","name":"Shared","version":"1","popup":null,"icon":null,"enabled":false,"profile_enabled":{{"personal":false}}}}]}}"#,
            Host::ID
        );
        std::fs::write(&path, &source).unwrap();
        let mut host = Host::new(directory.path());
        host.load();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let operation = host.submit(Pin::bundle(
            host.ticket(),
            "personal".into(),
            ExtPinRequest {
                id: Host::ID.into(),
                pinned: true,
            },
        ));
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
        let retry = host.submit(Pin::bundle(
            host.ticket(),
            "personal".into(),
            ExtPinRequest {
                id: Host::ID.into(),
                pinned: true,
            },
        ));
        assert_eq!(host.complete(retry), OperationState::Succeeded);
        assert!(FileStore::read(directory.path()).unwrap().entries[0].pinned_for("personal"));
    }

    #[test]
    fn migrates_legacy_package_without_generated_files() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(Host::ID);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(
            legacy.join("manifest.json"),
            serde_json::json!({
                "manifest_version": 3,
                "name": "test",
                "version": "1",
                "background": { "service_worker": "vmux_sw_deadbeef.js" },
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(legacy.join("background.js"), "original").unwrap();
        std::fs::write(legacy.join("vmux_patch.js"), "patch").unwrap();
        std::fs::write(legacy.join("vmux_sw_deadbeef.js"), "loader").unwrap();
        std::fs::write(
            legacy.join("vmux_shim.json"),
            serde_json::json!({
                "original": "background.js",
                "loader": "vmux_sw_deadbeef.js",
            })
            .to_string(),
        )
        .unwrap();

        let migrated = FileStore::migrate_legacy_package(root.path(), Host::ID, "1", "").unwrap();
        assert_eq!(migrated, FileStore::source_dir(root.path(), Host::ID, "1"));
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(migrated.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["background"]["service_worker"], "background.js");
        assert!(!migrated.join("vmux_patch.js").exists());
        assert!(!migrated.join("vmux_shim.json").exists());
        assert_eq!(FileStore::source_hash(&migrated).unwrap().len(), 64);
        assert!(legacy.join("vmux_shim.json").exists());
    }
}
