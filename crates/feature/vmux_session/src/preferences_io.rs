use bevy_app::{App, AppExit, Last, Plugin as BevyPlugin, PostUpdate, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::TryLockError;
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vmux_ecs::{
    Active, BaseRevision, EntityTarget, Operation, OperationError, OperationState, Revision,
};
use vmux_job::{Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet};
use vmux_path::AtomicFile;

use crate::preferences::{
    AgentPreferences, Drain, File, FilePath, Kind, Loaded, ModelSelectionSet, Queued, Ready, Root,
    Selection, SourceUrl,
};
use vmux_api::protocol::AcpModeOption;
use vmux_api::session::ModelOptionEntry;

pub(crate) struct PreferencesIoPlugin;

impl BevyPlugin for PreferencesIoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(JobPlugin::<FileJob>::default())
            .configure_sets(Update, SubmitSet.before(JobSet::Submit))
            .add_systems(Update, (rearm, read).chain().in_set(SubmitSet))
            .add_systems(
                Update,
                finish.after(JobSet::Apply).before(ModelSelectionSet),
            )
            .add_systems(PostUpdate, (settle, write, retire).chain())
            .add_message::<AppExit>()
            .add_observer(finalize)
            .add_observer(read_exit)
            .add_observer(write_exit)
            .add_systems(Last, flush_exit);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SubmitSet;

#[derive(Component)]
struct Unavailable;

#[derive(EntityEvent)]
struct Finalize {
    #[event_target]
    file: Entity,
    deadline: Instant,
}

#[derive(EntityEvent)]
struct ReadBeforeExit {
    #[event_target]
    file: Entity,
    deadline: Instant,
}

#[derive(EntityEvent)]
struct WriteBeforeExit {
    #[event_target]
    file: Entity,
    deadline: Instant,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct StoredModel {
    #[serde(default)]
    pub(crate) url: String,
    pub(crate) selected: String,
    #[serde(default)]
    pub(crate) models: Vec<ModelOptionEntry>,
}

impl StoredModel {
    fn capture(selected: &Selection, url: &SourceUrl, models: Vec<ModelOptionEntry>) -> Self {
        Self {
            url: url.0.clone(),
            selected: selected.0.clone(),
            models,
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct StoredMode {
    #[serde(default)]
    pub(crate) url: String,
    pub(crate) selected: String,
    pub(crate) modes: Vec<AcpModeOption>,
}

impl StoredMode {
    fn capture(selected: &Selection, url: &SourceUrl, modes: Vec<AcpModeOption>) -> Self {
        Self {
            url: url.0.clone(),
            selected: selected.0.clone(),
            modes,
        }
    }
}

pub(crate) enum Content {
    Models(BTreeMap<String, StoredModel>),
    Modes(BTreeMap<String, StoredMode>),
    Written,
}

impl Content {
    fn decode(kind: Kind, bytes: &[u8]) -> Result<Self, String> {
        match kind {
            Kind::Models => serde_json::from_slice(bytes)
                .map(Self::Models)
                .map_err(|error| error.to_string()),
            Kind::Modes => serde_json::from_slice(bytes)
                .map(Self::Modes)
                .map_err(|error| error.to_string()),
        }
    }

    fn empty(kind: Kind) -> Self {
        match kind {
            Kind::Models => Self::Models(BTreeMap::new()),
            Kind::Modes => Self::Modes(BTreeMap::new()),
        }
    }
}

impl AgentPreferences<'_, '_> {
    fn encode(&self, file: Entity, kind: Kind) -> Result<Vec<u8>, String> {
        let bytes = match kind {
            Kind::Models => {
                let mut rows = BTreeMap::new();
                for (entity, agent, selected, url) in self.records(file) {
                    rows.insert(
                        agent.0.clone(),
                        StoredModel::capture(selected, url, self.model_options(entity)),
                    );
                }
                serde_json::to_vec_pretty(&rows).map_err(|error| error.to_string())
            }
            Kind::Modes => {
                let mut rows = BTreeMap::new();
                for (entity, agent, selected, url) in self.records(file) {
                    rows.insert(
                        agent.0.clone(),
                        StoredMode::capture(selected, url, self.mode_options(entity)),
                    );
                }
                serde_json::to_vec_pretty(&rows).map_err(|error| error.to_string())
            }
        }?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("preference snapshot exceeds the 16 MiB decoder limit".to_string());
        }
        Ok(bytes)
    }
}

#[derive(Clone)]
struct FileJob {
    path: std::path::PathBuf,
    kind: Kind,
    bytes: Option<Arc<[u8]>>,
}

impl FileJob {
    const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

    fn read(path: &FilePath, kind: Kind) -> Self {
        Self {
            path: path.0.clone(),
            kind,
            bytes: None,
        }
    }

    fn write(path: &FilePath, kind: Kind, bytes: Vec<u8>) -> Self {
        Self {
            path: path.0.clone(),
            kind,
            bytes: Some(bytes.into()),
        }
    }

    fn execute(&self) -> Result<Content, String> {
        self.execute_until(Instant::now() + Self::LOCK_TIMEOUT)
    }

    fn execute_until(&self, deadline: Instant) -> Result<Content, String> {
        if vmux_job::BlockingCancellation::canceled() {
            return Err("preference IO canceled before dispatch".to_string());
        }
        let parent = self.path.parent().ok_or("preference file has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let name = self
            .path
            .file_name()
            .ok_or("preference file has no name")?
            .to_string_lossy();
        let lock_path = parent.join(format!(".{name}.lock"));
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|error| error.to_string())?;
        self.acquire(&lock, deadline.min(Instant::now() + Self::LOCK_TIMEOUT))?;
        if vmux_job::BlockingCancellation::canceled() {
            return Err("preference IO canceled before file access".to_string());
        }
        if let Some(bytes) = &self.bytes {
            AtomicFile::write(&self.path, bytes).map_err(|error| error.to_string())?;
            return Ok(Content::Written);
        }
        let source = match std::fs::File::open(&self.path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Content::empty(self.kind));
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        source
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("preference file exceeds the 16 MiB decoder limit".to_string());
        }
        Content::decode(self.kind, &bytes)
    }

    fn acquire(&self, lock: &std::fs::File, deadline: Instant) -> Result<(), String> {
        loop {
            if vmux_job::BlockingCancellation::canceled() {
                return Err("preference IO canceled while waiting for the file lock".to_string());
            }
            if Instant::now() >= deadline {
                return Err("preference file lock timed out".to_string());
            }
            match lock.try_lock() {
                Ok(()) => return Ok(()),
                Err(TryLockError::Error(error)) => return Err(error.to_string()),
                Err(TryLockError::WouldBlock) => {}
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Job for FileJob {
    type Output = Result<Content, String>;

    fn run(self) -> Self::Output {
        self.execute()
    }
}

fn rearm(
    roots: Query<Ref<Active>, With<Root>>,
    mut files: Query<(
        Entity,
        &ChildOf,
        &mut File,
        &Revision,
        &BaseRevision,
        Has<Unavailable>,
    )>,
    mut commands: Commands,
) {
    for (entity, parent, mut file, revision, saved, unavailable) in &mut files {
        if roots
            .get(parent.parent())
            .is_ok_and(|active| active.is_added())
            && !file.retry_allowed
        {
            file.retry_allowed = true;
            if unavailable && revision.0 == saved.0 {
                commands.entity(entity).try_remove::<(Ready, Unavailable)>();
            }
        }
    }
}

fn read(
    roots: Query<(), (With<Root>, With<Active>)>,
    mut files: Query<(Entity, &ChildOf, &Kind, &FilePath, &Revision, &mut File), Without<Ready>>,
    mut commands: Commands,
) {
    for (entity, parent, kind, path, revision, mut file) in &mut files {
        if file.pending.is_some() || !file.retry_allowed || !roots.contains(parent.parent()) {
            continue;
        }
        let operation = commands
            .spawn((
                Operation::bundle("agent-preferences.read").unwrap(),
                JobRequest(FileJob::read(path, *kind)),
                EntityTarget::<File>::new(entity),
                BaseRevision(revision.0),
                *kind,
                ChildOf(parent.parent()),
            ))
            .id();
        file.pending = Some(operation);
    }
}

fn write(
    preferences: AgentPreferences,
    mut files: Query<
        (
            Entity,
            &ChildOf,
            &Kind,
            &FilePath,
            &Revision,
            &BaseRevision,
            &mut File,
        ),
        With<Ready>,
    >,
    wake: Option<Res<vmux_ecs::JobWake>>,
    mut commands: Commands,
) {
    let mut queued = false;
    for (entity, parent, kind, path, revision, saved, mut file) in &mut files {
        if file.pending.is_some() || !file.retry_allowed || revision.0 == saved.0 {
            continue;
        }
        let bytes = match preferences.encode(entity, *kind) {
            Ok(bytes) => bytes,
            Err(error) => {
                file.retry_allowed = false;
                tracing::warn!("preference snapshot encoding failed: {error}");
                continue;
            }
        };
        let operation = commands
            .spawn((
                Operation::bundle("agent-preferences.write").unwrap(),
                JobRequest(FileJob::write(path, *kind, bytes)),
                EntityTarget::<File>::new(entity),
                BaseRevision(revision.0),
                *kind,
                ChildOf(parent.parent()),
            ))
            .id();
        file.pending = Some(operation);
        queued = true;
    }
    if queued && let Some(wake) = wake {
        wake.0.wake_by_ref();
    }
}

fn finish(
    mut jobs: Query<
        (
            Entity,
            &EntityTarget<File>,
            &BaseRevision,
            &Kind,
            &ChildOf,
            &JobOwner<FileJob>,
            &mut JobResult<FileJob>,
            &mut OperationState,
        ),
        Without<File>,
    >,
    mut files: Query<(
        &ChildOf,
        &Kind,
        &mut File,
        &Revision,
        &mut BaseRevision,
        Has<Ready>,
    )>,
    roots: Query<(), (With<Root>, With<Active>)>,
    mut commands: Commands,
) {
    for (operation, target, captured, kind, owner, job_owner, mut result, mut state) in &mut jobs {
        if !matches!(*state, OperationState::Pending | OperationState::Running) {
            continue;
        }
        let Ok((parent, current_kind, mut file, revision, mut saved, ready)) =
            files.get_mut(target.entity())
        else {
            *state = OperationState::Canceled;
            continue;
        };
        if !job_owner.accepts(Some(owner))
            || owner.parent() != parent.parent()
            || *kind != *current_kind
            || file.pending != Some(operation)
        {
            *state = OperationState::Canceled;
            continue;
        }
        file.pending = None;
        let value = std::mem::replace(&mut result.0, Err("preference result consumed".to_string()));
        match value {
            Ok(Ok(Content::Written)) => {
                saved.0 = captured.0;
                *state = OperationState::Succeeded;
                commands.entity(target.entity()).try_remove::<Unavailable>();
            }
            Ok(Ok(content)) => {
                *state = OperationState::Succeeded;
                if roots.contains(parent.parent()) && revision.0 == captured.0 {
                    commands.trigger(Loaded {
                        file: target.entity(),
                        content,
                    });
                    commands
                        .entity(target.entity())
                        .try_remove::<(Unavailable, OperationError)>();
                }
            }
            Ok(Err(error)) | Err(error) => {
                file.retry_allowed = false;
                *state = OperationState::Failed;
                if let Ok(failure) = OperationError::new(error.clone()) {
                    commands.entity(operation).insert(failure);
                }
                if !ready {
                    commands.entity(target.entity()).insert(Unavailable);
                    if let Ok(failure) = OperationError::new(error.clone()) {
                        commands.entity(target.entity()).insert(failure);
                    }
                }
                tracing::warn!("preference file IO failed: {error}");
            }
        }
    }
}

fn settle(
    mut files: Query<(Entity, &mut File, Has<Ready>)>,
    operations: Query<&OperationState, With<Operation>>,
    mut commands: Commands,
) {
    for (entity, mut file, ready) in &mut files {
        let Some(operation) = file.pending else {
            continue;
        };
        if operations
            .get(operation)
            .is_ok_and(|state| matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        file.pending = None;
        file.retry_allowed = false;
        if !ready {
            commands.entity(entity).insert(Unavailable);
            commands
                .entity(entity)
                .insert(OperationError::new("preference initial read was canceled").unwrap());
        }
    }
}

fn flush_exit(
    mut exits: MessageReader<AppExit>,
    files: Query<
        (
            Entity,
            Option<&ChildOf>,
            Option<&Revision>,
            Option<&BaseRevision>,
            Has<Ready>,
        ),
        With<File>,
    >,
    active: Query<(), With<Active>>,
    mut commands: Commands,
) {
    if exits.read().next().is_none() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut ordered = Vec::new();
    for (file, parent, revision, saved, ready) in &files {
        let active = parent.is_some_and(|parent| active.contains(parent.parent()));
        let dirty = revision.map(|revision| revision.0) != saved.map(|saved| saved.0);
        ordered.push((!active, !dirty, !ready, file));
    }
    ordered.sort_unstable();
    for (_, _, _, file) in ordered {
        commands.trigger(Finalize { file, deadline });
    }
}

fn finalize(
    request: On<Finalize>,
    mut files: Query<&mut File, (With<Kind>, With<FilePath>)>,
    mut commands: Commands,
) {
    if Instant::now() >= request.deadline {
        tracing::warn!("preference exit finalization exceeded its lock budget");
        return;
    }
    let Ok(mut file) = files.get_mut(request.file) else {
        return;
    };
    if let Some(operation) = file.pending.take() {
        commands.entity(operation).try_despawn();
    }
    commands.trigger(ReadBeforeExit {
        file: request.file,
        deadline: request.deadline,
    });
}

fn read_exit(
    request: On<ReadBeforeExit>,
    mut files: Query<(
        &mut File,
        &Kind,
        &FilePath,
        Option<&Revision>,
        Option<&BaseRevision>,
        Has<Ready>,
    )>,
    queued: Query<&ChildOf, With<Queued>>,
    mut commands: Commands,
) {
    let Ok((mut file, kind, path, revision, saved, ready)) = files.get_mut(request.file) else {
        return;
    };
    let queued = queued.iter().any(|parent| parent.parent() == request.file);
    let dirty = revision.map(|revision| revision.0) != saved.map(|saved| saved.0);
    if !queued && !dirty {
        return;
    }
    if !ready {
        match FileJob::read(path, *kind).execute_until(request.deadline) {
            Ok(content) => {
                commands.trigger(Loaded {
                    file: request.file,
                    content,
                });
                commands
                    .entity(request.file)
                    .remove::<(Unavailable, OperationError)>();
            }
            Err(error) => {
                file.retry_allowed = false;
                commands.entity(request.file).insert(Unavailable);
                tracing::warn!("preference exit read failed: {error}");
                return;
            }
        }
    }
    commands.trigger(Drain { file: request.file });
    commands.trigger(WriteBeforeExit {
        file: request.file,
        deadline: request.deadline,
    });
}

fn write_exit(
    request: On<WriteBeforeExit>,
    files: Query<(&Kind, &FilePath, &Revision, &BaseRevision), With<File>>,
    preferences: AgentPreferences,
    mut commands: Commands,
) {
    let Ok((kind, path, revision, saved)) = files.get(request.file) else {
        return;
    };
    if revision.0 == saved.0 {
        return;
    }
    let bytes = match preferences.encode(request.file, *kind) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!("preference exit encoding failed: {error}");
            return;
        }
    };
    match FileJob::write(path, *kind, bytes).execute_until(request.deadline) {
        Ok(Content::Written) => {
            commands
                .entity(request.file)
                .insert(BaseRevision(revision.0))
                .remove::<Unavailable>();
        }
        Ok(_) => {}
        Err(error) => tracing::warn!("preference exit flush failed: {error}"),
    }
}

fn retire(
    roots: Query<Entity, (With<Root>, Without<Active>)>,
    files: Query<(Entity, &ChildOf, &File, &Revision, &BaseRevision)>,
    queued: Query<&ChildOf, With<Queued>>,
    operations: Query<&ChildOf, With<Operation>>,
    mut commands: Commands,
) {
    for root in &roots {
        if operations.iter().any(|parent| parent.parent() == root) {
            continue;
        }
        if files.iter().any(|(entity, parent, file, revision, saved)| {
            parent.parent() == root
                && (file.pending.is_some()
                    || revision.0 != saved.0
                    || queued.iter().any(|parent| parent.parent() == entity))
        }) {
            continue;
        }
        commands.entity(root).despawn();
    }
}
