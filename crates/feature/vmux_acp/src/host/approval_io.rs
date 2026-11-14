use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::TryLockError;
use std::hash::{DefaultHasher, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vmux_ecs::jobs::{Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet};
use vmux_ecs::{Active, BaseRevision, Cwd, EntityTarget, Operation, OperationState, Revision};

use crate::approval::{File, Loaded, Reconcile, Records, Saved, ScopeRequest, Scoped};

pub(super) struct Plugin;

impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<FileJob>::default(),
            JobPlugin::<ScopeJob>::default(),
        ))
        .configure_sets(
            Update,
            crate::approval::Prepare.after(vmux_ecs::service::ServiceMessageSet),
        )
        .add_systems(
            Update,
            (read, scope)
                .after(crate::approval::Prepare)
                .before(JobSet::Submit),
        )
        .add_systems(
            Update,
            (finish, finish_scope)
                .after(JobSet::Apply)
                .before(crate::approval::Resolve),
        )
        .add_systems(PostUpdate, write)
        .add_message::<AppExit>()
        .add_systems(Last, flush);
    }
}

#[derive(Default, Deserialize, Serialize)]
struct Json {
    by_agent: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
}

impl Json {
    fn decode(bytes: &[u8]) -> Result<Vec<(String, String, String)>, String> {
        let data: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let mut records = Vec::new();
        for (agent, scopes) in data.by_agent {
            for (scope, tools) in scopes {
                for tool in tools {
                    records.push((agent.clone(), scope.clone(), tool));
                }
            }
        }
        Ok(records)
    }

    fn encode(grants: &Records, file: Entity) -> Result<Vec<u8>, String> {
        let mut data = Self::default();
        for (agent, scope, tool) in grants.records(file) {
            data.by_agent
                .entry(agent.into())
                .or_default()
                .entry(scope.into())
                .or_default()
                .insert(tool.into());
        }
        let bytes = serde_json::to_vec_pretty(&data).map_err(|error| error.to_string())?;
        if bytes.len() > FileJob::LIMIT {
            return Err("approval snapshot exceeds the 16 MiB decoder limit".into());
        }
        Ok(bytes)
    }
}

enum Content {
    Read {
        records: Vec<(String, String, String)>,
        fingerprint: Option<u64>,
    },
    Written(u64),
}

#[derive(Clone)]
struct FileJob {
    path: PathBuf,
    bytes: Option<Arc<[u8]>>,
    expected: Option<u64>,
}

impl FileJob {
    const LIMIT: usize = 16 * 1024 * 1024;

    fn read(path: PathBuf) -> Self {
        Self {
            path,
            bytes: None,
            expected: None,
        }
    }

    fn write(path: PathBuf, bytes: Vec<u8>, expected: Option<u64>) -> Self {
        Self {
            path,
            bytes: Some(bytes.into()),
            expected,
        }
    }

    fn execute(&self, deadline: Instant) -> Result<Content, String> {
        if vmux_ecs::BlockingCancellation::canceled() {
            return Err("approval IO canceled before dispatch".into());
        }
        let parent = self.path.parent().ok_or("approval file has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let name = self
            .path
            .file_name()
            .ok_or("approval file has no name")?
            .to_string_lossy();
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(parent.join(format!(".{name}.lock")))
            .map_err(|error| error.to_string())?;
        let deadline = deadline.min(Instant::now() + Duration::from_secs(2));
        loop {
            if vmux_ecs::BlockingCancellation::canceled() {
                return Err("approval lock canceled".into());
            }
            if Instant::now() >= deadline {
                return Err("approval file lock timed out".into());
            }
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
                Err(TryLockError::Error(error)) => return Err(error.to_string()),
            }
        }
        if vmux_ecs::BlockingCancellation::canceled() {
            return Err("approval IO canceled before file access".into());
        }
        let original = self.read_bytes()?;
        let fingerprint = original.as_ref().map(|bytes| Self::fingerprint(bytes));
        if let Some(bytes) = &self.bytes {
            if fingerprint != self.expected {
                return Err("approval file changed before replacement write".into());
            }
            vmux_path::AtomicFile::write(&self.path, bytes).map_err(|error| error.to_string())?;
            return Ok(Content::Written(Self::fingerprint(bytes)));
        }
        let records = if let Some(bytes) = original {
            Json::decode(&bytes)?
        } else {
            Vec::new()
        };
        Ok(Content::Read {
            records,
            fingerprint,
        })
    }

    fn read_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        let source = match std::fs::File::open(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        source
            .take(Self::LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > Self::LIMIT {
            return Err("approval file exceeds the 16 MiB decoder limit".into());
        }
        Ok(Some(bytes))
    }

    fn fingerprint(bytes: &[u8]) -> u64 {
        let mut hash = DefaultHasher::new();
        hash.write(bytes);
        hash.finish()
    }
}

impl Job for FileJob {
    type Output = Result<Content, String>;

    fn run(self) -> Self::Output {
        self.execute(Instant::now() + Duration::from_secs(2))
    }
}

#[derive(Clone)]
struct ScopeJob(PathBuf);

impl ScopeJob {
    fn execute(&self) -> Result<(Option<PathBuf>, Option<PathBuf>), String> {
        if vmux_ecs::BlockingCancellation::canceled() {
            return Err("approval scope resolution canceled".into());
        }
        let repository = vmux_git::worktree::CheckoutInfo::try_from(self.0.as_path())
            .ok()
            .map(|checkout| checkout.common_dir);
        let directory = std::fs::canonicalize(&self.0).ok();
        Ok((repository, directory))
    }
}

impl Job for ScopeJob {
    type Output = Result<(Option<PathBuf>, Option<PathBuf>), String>;

    fn run(self) -> Self::Output {
        self.execute()
    }
}

fn read(mut files: Query<(Entity, &mut File)>, mut commands: Commands) {
    for (file, mut state) in &mut files {
        if state.loaded || state.read_failed || state.read.is_some() || state.write.is_some() {
            continue;
        }
        let Some(path) = state.path.clone() else {
            continue;
        };
        let operation = commands
            .spawn((
                Operation::bundle("approval.read").unwrap(),
                JobRequest(FileJob::read(path)),
                EntityTarget::<File>::new(file),
                BaseRevision(0),
            ))
            .id();
        state.read = Some(operation);
    }
}

fn scope(
    requests: Query<(Entity, &Cwd), (With<ScopeRequest>, Without<JobRequest<ScopeJob>>)>,
    mut commands: Commands,
) {
    for (entity, cwd) in &requests {
        commands
            .entity(entity)
            .insert(JobRequest(ScopeJob(cwd.0.clone())));
    }
}

fn write(
    grants: Records,
    requests: Query<&ScopeRequest>,
    mut files: Query<(Entity, &mut File, &Revision, &BaseRevision)>,
    mut commands: Commands,
) {
    for (file, mut state, revision, saved) in &mut files {
        if !state.loaded
            || state.write_failed
            || state.read.is_some()
            || state.write.is_some()
            || revision.0 == saved.0
        {
            continue;
        }
        if requests
            .iter()
            .any(|request| request.file == file && request.consent)
        {
            continue;
        }
        let Some(path) = state.path.clone() else {
            continue;
        };
        let bytes = match Json::encode(&grants, file) {
            Ok(bytes) => bytes,
            Err(error) => {
                state.write_failed = true;
                warn!("approval encoding failed: {error}");
                continue;
            }
        };
        let operation = commands
            .spawn((
                Operation::bundle("approval.write").unwrap(),
                JobRequest(FileJob::write(path, bytes, state.fingerprint)),
                EntityTarget::<File>::new(file),
                BaseRevision(revision.0),
            ))
            .id();
        state.write = Some(operation);
    }
}

fn finish(
    mut results: Query<(
        Entity,
        &EntityTarget<File>,
        &BaseRevision,
        &mut JobResult<FileJob>,
        &JobOwner<FileJob>,
        Option<&ChildOf>,
    )>,
    mut loaded: MessageWriter<Loaded>,
    mut saved: MessageWriter<Saved>,
    files: Query<&File>,
    mut commands: Commands,
) {
    for (operation, target, revision, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let Ok(file) = files.get(target.entity()) else {
            commands.entity(operation).despawn();
            continue;
        };
        let is_read = file.read == Some(operation);
        let is_write = file.write == Some(operation);
        if !is_read && !is_write {
            continue;
        }
        let output = std::mem::replace(&mut result.0, Err("approval result consumed".into()));
        match output {
            Ok(Ok(Content::Read {
                records,
                fingerprint,
            })) if is_read => {
                loaded.write(Loaded {
                    file: target.entity(),
                    operation,
                    fingerprint,
                    result: Ok(records),
                });
            }
            Ok(Ok(Content::Written(fingerprint))) if is_write => {
                saved.write(Saved {
                    file: target.entity(),
                    operation,
                    revision: revision.0,
                    fingerprint: Some(fingerprint),
                    result: Ok(()),
                });
            }
            Ok(Err(error)) | Err(error) => {
                warn!("approval file IO failed: {error}");
                if is_read {
                    loaded.write(Loaded {
                        file: target.entity(),
                        operation,
                        fingerprint: None,
                        result: Err(error),
                    });
                } else {
                    saved.write(Saved {
                        file: target.entity(),
                        operation,
                        revision: revision.0,
                        fingerprint: None,
                        result: Err(error),
                    });
                }
            }
            _ => {}
        }
        commands.entity(operation).remove::<JobResult<FileJob>>();
    }
}

fn finish_scope(
    mut results: Query<(Entity, &mut JobResult<ScopeJob>), With<ScopeRequest>>,
    mut scoped: MessageWriter<Scoped>,
    mut commands: Commands,
) {
    for (operation, mut result) in &mut results {
        let result = std::mem::replace(&mut result.0, Err("approval scope result consumed".into()));
        match result {
            Ok(Ok((repository, directory))) => {
                scoped.write(Scoped {
                    operation,
                    repository,
                    directory,
                });
            }
            Ok(Err(error)) | Err(error) => {
                warn!("approval scope IO failed: {error}");
                commands.entity(operation).insert((
                    OperationState::Failed,
                    vmux_ecs::OperationError::new(error).unwrap(),
                ));
            }
        }
        commands.entity(operation).remove::<JobResult<ScopeJob>>();
    }
}

fn flush(world: &mut World) {
    if world.resource::<Messages<AppExit>>().is_empty() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(4);
    world.run_schedule(Reconcile);
    let requests: Vec<_> = world
        .query::<(Entity, &ScopeRequest, &Cwd)>()
        .iter(world)
        .filter(|(_, request, _)| request.consent)
        .map(|(entity, _, cwd)| (entity, cwd.0.clone()))
        .collect();
    for (operation, path) in requests {
        if Instant::now() >= deadline {
            break;
        }
        match ScopeJob(path).execute() {
            Ok((repository, directory)) => {
                world.write_message(Scoped {
                    operation,
                    repository,
                    directory,
                });
            }
            Err(error) => warn!("approval exit scope failed: {error}"),
        }
    }
    world.run_schedule(Reconcile);
    let mut files: Vec<_> = world
        .query::<(Entity, &File, Has<Active>)>()
        .iter(world)
        .filter_map(|(entity, file, active)| file.path.clone().map(|path| (entity, path, active)))
        .collect();
    files.sort_unstable_by_key(|(entity, _, active)| (!*active, *entity));
    for (file, path, _) in files {
        if Instant::now() >= deadline {
            warn!("approval exit finalization exceeded its lock budget");
            break;
        }
        let dirty = world.get::<Revision>(file).map(|revision| revision.0)
            != world.get::<BaseRevision>(file).map(|saved| saved.0);
        if !dirty {
            continue;
        }
        let pending = [
            world.get::<File>(file).and_then(|state| state.read),
            world.get::<File>(file).and_then(|state| state.write),
        ];
        for operation in pending.into_iter().flatten() {
            world.despawn(operation);
        }
        let state = world.get_mut::<File>(file).unwrap().into_inner();
        state.read = None;
        state.write = None;
        {
            let operation = world
                .spawn((
                    Operation::bundle("approval.exit-read").unwrap().running(),
                    EntityTarget::<File>::new(file),
                ))
                .id();
            world.get_mut::<File>(file).unwrap().read = Some(operation);
            match FileJob::read(path.clone()).execute(deadline) {
                Ok(Content::Read {
                    records,
                    fingerprint,
                }) => {
                    world.write_message(Loaded {
                        file,
                        operation,
                        fingerprint,
                        result: Ok(records),
                    });
                    world.run_schedule(Reconcile);
                    world.despawn(operation);
                }
                Err(error) => {
                    world.write_message(Loaded {
                        file,
                        operation,
                        fingerprint: None,
                        result: Err(error.clone()),
                    });
                    world.run_schedule(Reconcile);
                    warn!("approval exit read failed: {error}");
                    continue;
                }
                _ => continue,
            }
        }
        let bytes = match world.run_system_once(move |grants: Records| Json::encode(&grants, file))
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error)) => {
                warn!("approval exit encoding failed: {error}");
                continue;
            }
            Err(error) => {
                warn!("approval exit snapshot failed: {error}");
                continue;
            }
        };
        let fingerprint = world.get::<File>(file).unwrap().fingerprint;
        let revision = world.get::<Revision>(file).unwrap().0;
        let operation = world
            .spawn((
                Operation::bundle("approval.exit-write").unwrap().running(),
                EntityTarget::<File>::new(file),
                BaseRevision(revision),
            ))
            .id();
        world.get_mut::<File>(file).unwrap().write = Some(operation);
        match FileJob::write(path, bytes, fingerprint).execute(deadline) {
            Ok(Content::Written(fingerprint)) => {
                world.write_message(Saved {
                    file,
                    operation,
                    revision,
                    fingerprint: Some(fingerprint),
                    result: Ok(()),
                });
                world.run_schedule(Reconcile);
                world
                    .entity_mut(operation)
                    .insert(OperationState::Succeeded);
            }
            Err(error) => {
                world.write_message(Saved {
                    file,
                    operation,
                    revision,
                    fingerprint: None,
                    result: Err(error.clone()),
                });
                world.run_schedule(Reconcile);
                world.entity_mut(operation).insert(OperationState::Failed);
                warn!("approval exit write failed: {error}");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_refuses_a_changed_file_and_preserves_the_external_update() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("approvals.json");
        let original = br#"{"by_agent":{"one":{"/scope":["edit"]}}}"#;
        std::fs::write(&path, original).unwrap();
        let Content::Read { fingerprint, .. } = FileJob::read(path.clone())
            .execute(Instant::now() + Duration::from_secs(2))
            .unwrap()
        else {
            panic!("read returned no records");
        };
        let replacement = br#"{"by_agent":{"other":{"/scope":["run"]}}}"#;
        std::fs::write(&path, replacement).unwrap();
        let result = FileJob::write(path.clone(), original.to_vec(), fingerprint)
            .execute(Instant::now() + Duration::from_secs(2));
        assert!(result.is_err());
        assert_eq!(std::fs::read(path).unwrap(), replacement);
    }

    #[test]
    fn oversized_input_is_refused_before_decoding_or_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("approvals.json");
        let source = std::fs::File::create(&path).unwrap();
        source.set_len(16 * 1024 * 1024 + 1).unwrap();
        assert!(
            FileJob::read(path.clone())
                .execute(Instant::now() + Duration::from_secs(2))
                .is_err()
        );
        assert_eq!(std::fs::metadata(path).unwrap().len(), 16 * 1024 * 1024 + 1);
    }

    #[test]
    fn a_held_lock_obeys_the_supplied_deadline_without_changing_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("approvals.json");
        let original = br#"{"by_agent":{}}"#;
        std::fs::write(&path, original).unwrap();
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.path().join(".approvals.json.lock"))
            .unwrap();
        lock.lock().unwrap();
        let start = Instant::now();
        assert!(
            FileJob::read(path.clone())
                .execute(start + Duration::from_millis(40))
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}
