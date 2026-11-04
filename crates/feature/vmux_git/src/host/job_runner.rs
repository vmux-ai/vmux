use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy::{ecs::system::SystemParam, prelude::*};

use super::GitUpdateSet;
use super::job::{
    AmendJob, BranchFilesJob, CatalogJob, CheckoutCommitJob, CherryPickJob, CommitJob,
    CreateBranchJob, DeleteBranchJob, DiffJob, DiffScope, DiscardJob, FastForwardJob, FetchJob,
    HunkJob, MergeJob, PullJob, PushJob, RebaseJob, RepositoryJob, RevertJob, StageAllJob,
    StageJob, StashDropJob, StashPopJob, StashPushJob, UnstageJob,
};
use super::repository::{
    BranchFilesCatalog, DiffSource, GitCatalogPage, GitCatalogSnapshot, GitError, GitRepository,
    RepositorySummary,
};
use super::runtime::{GitCancellationGroup, GitCancellationParticipant, GitRuntime, GitTask};
use crate::event::{
    GitCatalog, GitDiffViewport, GitFileStatus, GitOperationError, GitOperationResult,
    GitRepositorySnapshot,
};
use crate::state::{
    GitBranchFilesPagePatch, GitBranchFilesPatch, GitCatalogEntries, GitCatalogPatch, UiGit,
};
use vmux_path::PathIdentity;

const REPOSITORY_LOAD_CACHE_CAPACITY: usize = 128;
const REPOSITORY_LOAD_ALIAS_CAPACITY: usize = REPOSITORY_LOAD_CACHE_CAPACITY * 4;
const CATALOG_SNAPSHOT_CAPACITY: usize = 128;

pub(super) struct JobPlugin;

impl Plugin for JobPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .init_resource::<RepositoryLoads>()
            .init_resource::<RepositoryLoadEpoch>()
            .init_resource::<CatalogSnapshots>()
            .add_systems(
                Update,
                (
                    cancel_orphaned_jobs,
                    bevy::ecs::schedule::ApplyDeferred,
                    (
                        poll_jobs::<RepositoryOutput>,
                        poll_jobs::<CatalogOutput>,
                        poll_jobs::<BranchFilesOutput>,
                        poll_jobs::<DiffOutput>,
                        poll_jobs::<OperationOutput>,
                    ),
                    (
                        deliver_repository_outputs,
                        deliver_catalog_outputs,
                        deliver_branch_files_outputs,
                        deliver_diff_outputs,
                        deliver_operation_outputs,
                        deliver_failure_outputs,
                    ),
                    bevy::ecs::schedule::ApplyDeferred,
                    (
                        start_repository_jobs,
                        start_catalog_jobs,
                        start_branch_files_jobs,
                        start_diff_jobs,
                        start_stage_jobs,
                        start_unstage_jobs,
                        start_discard_jobs,
                        start_commit_jobs,
                        start_fetch_jobs,
                        start_pull_jobs,
                        start_push_jobs,
                        start_stage_all_jobs,
                        start_hunk_jobs,
                    ),
                    (
                        start_amend_jobs,
                        start_checkout_commit_jobs,
                        start_cherry_pick_jobs,
                        start_create_branch_jobs,
                        start_delete_branch_jobs,
                        start_fast_forward_jobs,
                        start_merge_jobs,
                        start_rebase_jobs,
                        start_revert_jobs,
                        start_stash_drop_jobs,
                        start_stash_pop_jobs,
                        start_stash_push_jobs,
                    ),
                )
                    .chain()
                    .in_set(GitUpdateSet::Jobs),
            );
    }
}

#[derive(Component)]
#[relationship(relationship_target = GitJobs)]
pub(super) struct GitJob {
    #[relationship]
    webview: Entity,
}

impl GitJob {
    pub(super) fn new(webview: Entity) -> Self {
        Self { webview }
    }

    pub(super) fn owner(&self) -> Entity {
        self.webview
    }
}

#[derive(Component)]
#[relationship_target(relationship = GitJob)]
pub(super) struct GitJobs(Vec<Entity>);

#[derive(Component)]
struct GitJobRunning;

#[derive(SystemParam)]
pub(super) struct QueuedGitJobs<'w, 's, J: Component> {
    queues: Query<'w, 's, &'static GitJobs>,
    matching: Query<'w, 's, (), With<J>>,
}

impl<J: Component> QueuedGitJobs<'_, '_, J> {
    pub(super) fn despawn_for(&self, webview: Entity, commands: &mut Commands) {
        let Ok(queue) = self.queues.get(webview) else {
            return;
        };
        for entity in queue.iter() {
            if self.matching.contains(entity) {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[derive(Component)]
struct GitJobTask<T: Send + Sync + 'static> {
    task: GitTask<T>,
}

impl<T: Send + Sync + 'static> GitJobTask<T> {
    fn spawn(
        runtime: &GitRuntime,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<Self> {
        runtime.spawn(wake, run).map(|task| Self { task })
    }
}

#[derive(Component, Clone)]
struct RepositoryOutput(Result<Arc<RepositorySummary>, GitOperationError>);

#[derive(Resource, Clone, Default)]
pub(super) struct RepositoryLoads(Arc<Mutex<RepositoryLoadCache>>);

#[derive(Default)]
struct RepositoryLoadCache {
    loads: HashMap<PathBuf, Arc<RepositoryLoad>>,
    aliases: HashMap<PathIdentity, RepositoryLoadAlias>,
    alias_order: VecDeque<PathIdentity>,
}

struct RepositoryLoadAlias {
    requested_resolved: PathIdentity,
    root: PathBuf,
}

impl RepositoryLoadCache {
    fn remember_alias(
        &mut self,
        requested: PathIdentity,
        requested_resolved: PathIdentity,
        root: PathBuf,
    ) {
        self.alias_order.retain(|alias| alias != &requested);
        self.alias_order.push_back(requested.clone());
        self.aliases.insert(
            requested,
            RepositoryLoadAlias {
                requested_resolved,
                root,
            },
        );
        while self.alias_order.len() > REPOSITORY_LOAD_ALIAS_CAPACITY {
            let Some(expired) = self.alias_order.pop_front() else {
                break;
            };
            self.aliases.remove(&expired);
        }
    }

    fn retain_loading(&mut self) {
        self.loads.retain(|_, load| load.loading());
        self.aliases
            .retain(|_, alias| self.loads.contains_key(&alias.root));
        self.alias_order
            .retain(|alias| self.aliases.contains_key(alias));
    }
}

struct RepositoryLoad {
    state: Mutex<RepositoryLoadState>,
    ready: Condvar,
    cancellation: GitCancellationGroup,
}

struct RepositoryLoadState {
    epoch: u64,
    loading: bool,
    result: Option<Result<Arc<RepositorySummary>, GitOperationError>>,
}

impl RepositoryLoadState {
    fn canceled(&self) -> bool {
        self.result.as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_err_and(|error| error.message == "git command canceled")
        })
    }
}

impl RepositoryLoad {
    fn new(epoch: u64) -> Self {
        Self {
            state: Mutex::new(RepositoryLoadState {
                epoch,
                loading: true,
                result: None,
            }),
            ready: Condvar::new(),
            cancellation: GitCancellationGroup::default(),
        }
    }

    fn loading(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.loading)
    }

    fn read(
        &self,
        repository: &GitRepository,
        is_worktree: bool,
        epoch: u64,
        leader: bool,
    ) -> Result<Arc<RepositorySummary>, GitOperationError> {
        let participant = self.cancellation.join();
        if leader {
            return self.finish(repository, is_worktree, participant);
        }
        let mut state = self.state.lock().map_err(|_| Self::coordinator_error())?;
        loop {
            if !state.loading {
                if epoch <= state.epoch && !state.canceled() {
                    return state
                        .result
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| Err(Self::coordinator_error()));
                }
                state.epoch = state.epoch.max(epoch);
                state.loading = true;
                state.result = None;
                drop(state);
                return self.finish(repository, is_worktree, participant);
            }
            if GitRuntime::canceled() {
                return Err(GitOperationError {
                    message: "git command canceled".to_string(),
                });
            }
            let waited = self
                .ready
                .wait_timeout(state, Duration::from_millis(20))
                .map_err(|_| Self::coordinator_error())?;
            state = waited.0;
        }
    }

    fn finish(
        &self,
        repository: &GitRepository,
        is_worktree: bool,
        participant: GitCancellationParticipant,
    ) -> Result<Arc<RepositorySummary>, GitOperationError> {
        let result = participant
            .run(|| {
                catch_unwind(AssertUnwindSafe(|| {
                    GitRepositorySnapshot::load_discovered_summary(
                        repository.path(),
                        repository,
                        is_worktree,
                    )
                    .map(Arc::new)
                    .map_err(|error| GitOperationError { message: error.0 })
                }))
            })
            .unwrap_or_else(|_| {
                Err(GitOperationError {
                    message: "Git repository load worker panicked".to_string(),
                })
            });
        let mut state = self.state.lock().map_err(|_| Self::coordinator_error())?;
        state.loading = false;
        state.result = Some(result.clone());
        self.ready.notify_all();
        result
    }

    fn coordinator_error() -> GitOperationError {
        GitOperationError {
            message: "Git repository load coordinator failed".to_string(),
        }
    }

    fn cached(&self) -> Option<Arc<RepositorySummary>> {
        self.state
            .lock()
            .ok()?
            .result
            .as_ref()?
            .as_ref()
            .ok()
            .cloned()
    }
}

impl RepositoryLoads {
    pub(super) fn cached(&self, path: &Path) -> Option<GitRepositorySnapshot> {
        let requested = PathIdentity::lexical(path);
        let requested_resolved = PathIdentity::resolve(path);
        let load = {
            let cache = self.0.lock().ok()?;
            let root = match cache.aliases.get(&requested) {
                Some(alias) if alias.requested_resolved == requested_resolved => alias.root.clone(),
                Some(_) => return None,
                None => requested.clone().into_path_buf(),
            };
            Arc::clone(cache.loads.get(&root)?)
        };
        let summary = load.cached()?;
        let mut snapshot = summary.snapshot.clone();
        snapshot.path = path.to_string_lossy().into_owned();
        Some(snapshot)
    }

    fn read(&self, path: &Path, epoch: u64) -> Result<Arc<RepositorySummary>, GitOperationError> {
        let (repository, is_worktree) = GitRepository::discover_with_worktree(path)
            .map_err(|error| GitOperationError { message: error.0 })?;
        let root = PathIdentity::resolve(repository.path()).into_path_buf();
        let requested = PathIdentity::lexical(path);
        let requested_resolved = PathIdentity::resolve(path);
        let (load, leader) = {
            let mut cache = self
                .0
                .lock()
                .map_err(|_| RepositoryLoad::coordinator_error())?;
            cache.remember_alias(requested, requested_resolved, root.clone());
            match cache.loads.get(&root) {
                Some(load) => (Arc::clone(load), false),
                None => {
                    if cache.loads.len() >= REPOSITORY_LOAD_CACHE_CAPACITY {
                        cache.retain_loading();
                    }
                    let load = Arc::new(RepositoryLoad::new(epoch));
                    cache.loads.insert(root.clone(), Arc::clone(&load));
                    (load, true)
                }
            }
        };
        load.read(&repository, is_worktree, epoch, leader)
    }
}

#[derive(Resource, Default)]
struct RepositoryLoadEpoch(u64);

#[derive(Resource, Clone, Default)]
struct CatalogSnapshots(Arc<Mutex<CatalogSnapshotCache>>);

#[derive(Default)]
struct CatalogSnapshotCache {
    snapshots: HashMap<CatalogSnapshotKey, Arc<GitCatalogSnapshot>>,
    order: VecDeque<CatalogSnapshotKey>,
}

impl CatalogSnapshotCache {
    fn insert(&mut self, key: CatalogSnapshotKey, snapshot: Arc<GitCatalogSnapshot>) {
        self.order.retain(|candidate| !candidate.same_catalog(&key));
        self.snapshots
            .retain(|candidate, _| !candidate.same_catalog(&key));
        self.order.push_back(key.clone());
        self.snapshots.insert(key, snapshot);
        while self.order.len() > CATALOG_SNAPSHOT_CAPACITY {
            if let Some(expired) = self.order.pop_front() {
                self.snapshots.remove(&expired);
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct CatalogSnapshotKey {
    owner: Entity,
    repository: PathBuf,
    catalog: u8,
    branch: String,
    revision: u32,
}

impl CatalogSnapshotKey {
    fn same_catalog(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.repository == other.repository
            && self.catalog == other.catalog
            && self.branch == other.branch
    }
}

impl CatalogSnapshots {
    fn page(
        &self,
        owner: Entity,
        repository: &GitRepository,
        job: &CatalogJob,
    ) -> Result<GitCatalogPage, GitOperationError> {
        if job.catalog == GitCatalog::Commits {
            return repository
                .catalog_page(job.catalog, &job.current_branch, job.offset, job.limit)
                .map_err(|error| GitOperationError { message: error.0 });
        }
        let key = CatalogSnapshotKey {
            owner,
            repository: repository.path().to_path_buf(),
            catalog: match job.catalog {
                GitCatalog::LocalBranches => 0,
                GitCatalog::RemoteBranches => 1,
                GitCatalog::Tags => 2,
                GitCatalog::Commits => 3,
                GitCatalog::Stashes => 4,
            },
            branch: if job.catalog == GitCatalog::LocalBranches {
                job.current_branch.clone()
            } else {
                String::new()
            },
            revision: job.revision,
        };
        let cached = self
            .0
            .lock()
            .ok()
            .and_then(|cache| cache.snapshots.get(&key).cloned());
        let snapshot = match cached {
            Some(snapshot) => snapshot,
            None => {
                let snapshot = repository
                    .catalog_snapshot(job.catalog, &job.current_branch)
                    .map_err(|error| GitOperationError { message: error.0 })?
                    .ok_or_else(|| GitOperationError {
                        message: "catalog does not support snapshots".to_string(),
                    })?;
                let snapshot = Arc::new(snapshot);
                if let Ok(mut cache) = self.0.lock() {
                    cache.insert(key, Arc::clone(&snapshot));
                }
                snapshot
            }
        };
        Ok(snapshot.page(job.offset, job.limit))
    }
}

#[derive(Component)]
struct CatalogOutput {
    catalog: GitCatalog,
    repo_root: String,
    offset: usize,
    revision: u32,
    generation: u64,
    result: Result<GitCatalogPage, GitOperationError>,
}

#[derive(Component)]
struct BranchFilesOutput {
    generation: u64,
    reference: String,
    result: Result<BranchFilesCatalog, GitOperationError>,
}

#[derive(Component)]
struct DiffOutput {
    viewport: GitDiffViewport,
    branch_paths: Vec<Vec<u8>>,
    source: Option<DiffSource>,
    branch_top_line: u32,
    branch_page_complete: bool,
}

impl DiffOutput {
    fn take_viewport(&mut self) -> GitDiffViewport {
        std::mem::replace(
            &mut self.viewport,
            GitDiffViewport {
                generation: 0,
                revision: 0,
                first_line: 0,
                total_lines: 0,
                has_more: false,
                lines: Vec::new(),
                markers: Vec::new(),
                error: String::new(),
            },
        )
    }

    fn failure(job: &DiffJob, error: GitError) -> Self {
        Self {
            viewport: GitDiffViewport {
                generation: job.generation,
                revision: 0,
                first_line: job.top_line,
                total_lines: 0,
                has_more: false,
                lines: Vec::new(),
                markers: Vec::new(),
                error: error.0,
            },
            branch_paths: job.branch_paths.clone(),
            source: None,
            branch_top_line: job.branch_top_line,
            branch_page_complete: false,
        }
    }
}

#[derive(Component, Clone, Debug)]
struct OperationOutput {
    repository: PathBuf,
    result: GitOperationResult,
    status: Option<Result<GitFileStatus, GitOperationError>>,
}

impl OperationOutput {
    fn success_with_status(
        repository: &GitRepository,
        path: &Path,
        operation: &str,
        message: &str,
    ) -> Self {
        Self {
            repository: repository.path().to_path_buf(),
            result: GitOperationResult {
                operation: operation.to_string(),
                ok: true,
                message: message.to_string(),
            },
            status: Some(
                repository
                    .status(path)
                    .map_err(|error| GitOperationError { message: error.0 }),
            ),
        }
    }

    fn from_result(
        repository: &GitRepository,
        operation: &str,
        result: Result<String, GitError>,
    ) -> Self {
        match result {
            Ok(message) => Self {
                repository: repository.path().to_path_buf(),
                result: GitOperationResult {
                    operation: operation.to_string(),
                    ok: true,
                    message,
                },
                status: None,
            },
            Err(error) => Self::failure(repository.path(), operation, error),
        }
    }

    fn failure(repository: impl Into<PathBuf>, operation: &str, error: GitError) -> Self {
        Self {
            repository: repository.into(),
            result: GitOperationResult {
                operation: operation.to_string(),
                ok: false,
                message: error.0,
            },
            status: None,
        }
    }
}

#[derive(Component)]
struct FailureOutput(GitOperationError);

fn poll_jobs<T: Component>(mut jobs: Query<(Entity, &mut GitJobTask<T>)>, mut commands: Commands) {
    for (entity, mut task) in &mut jobs {
        let Some(output) = task.task.poll() else {
            continue;
        };
        let mut entity = commands.entity(entity);
        entity.remove::<(GitJobRunning, GitJobTask<T>)>();
        match output {
            Ok(output) => {
                entity.insert(output);
            }
            Err(_) => {
                entity.insert(FailureOutput(GitOperationError {
                    message: "Git job worker panicked".to_string(),
                }));
            }
        }
    }
}

fn cancel_orphaned_jobs(
    jobs: Query<(Entity, &GitJob, Option<&RepositoryJob>)>,
    owners: Query<(
        Option<&super::state::GitState>,
        Option<&super::status::FileGit>,
    )>,
    load_revisions: Query<&super::state::GitRepositoryLoadRevision>,
    mut commands: Commands,
) {
    for (entity, job, repository) in &jobs {
        let Ok((state, file)) = owners.get(job.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        if state.is_none() && file.is_none() {
            commands.entity(entity).despawn();
            continue;
        }
        let Some((state, repository)) = state.zip(repository) else {
            continue;
        };
        if !load_revisions
            .get(job.webview)
            .is_ok_and(|revision| revision.matches(repository.revision))
            || PathIdentity::lexical(Path::new(&state.snapshot.workspace))
                != PathIdentity::lexical(&repository.path)
        {
            commands.entity(entity).despawn();
        }
    }
}

fn deliver_repository_outputs(
    outputs: Query<(Entity, &GitJob, &RepositoryJob, &RepositoryOutput)>,
    presented: Query<(), With<vmux_ecs::PagePresented>>,
    mut watch: Option<NonSendMut<super::watch::GitWatch>>,
    mut pages: Query<&mut vmux_ecs::PageMetadata>,
    mut states: super::state::GitStates,
    mut commands: Commands,
) {
    for (entity, owner, request, output) in &outputs {
        let webview = owner.webview;
        if !states.repository_requested_at(webview, &request.path, request.revision) {
            commands.entity(entity).despawn();
            continue;
        }
        match &output.0 {
            Ok(event) => {
                let mut snapshot = event.snapshot.clone();
                snapshot.path = request.path.to_string_lossy().into_owned();
                if presented.contains(webview)
                    && let Some(watch) = watch.as_deref_mut()
                    && let Err(error) =
                        watch.subscribe_page(webview, &request.path, event.watch.clone())
                {
                    states.apply_error(webview, &GitOperationError { message: error.0 });
                }
                if let Ok(mut page) = pages.get_mut(webview)
                    && crate::GitUrl::is_page(&page.url)
                {
                    if let Some(url) = crate::GitUrl::from_path_preserving_commit(
                        Path::new(&snapshot.repo_root),
                        &page.url,
                    ) {
                        page.url = url;
                    }
                    page.title = match snapshot.branch.is_empty() {
                        true => format!("Git · {}", snapshot.repo_name),
                        false => format!("Git · {} · {}", snapshot.repo_name, snapshot.branch),
                    };
                }
                if let Some(patch) =
                    states.set_repository(webview, snapshot, request.refresh_catalogs)
                {
                    commands.trigger(vmux_ecs::UiStateWrite::<UiGit>::from_event(webview, &patch));
                }
            }
            Err(error) => {
                states.fail_repository(webview, error);
            }
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_catalog_outputs(
    mut outputs: Query<(Entity, &GitJob, &mut CatalogOutput)>,
    mut states: Query<(
        &mut super::state::GitState,
        &mut super::state::GitCatalogRevision,
    )>,
    mut ui_state: Option<vmux_ecs::UiStateWriter<UiGit>>,
    mut commands: Commands,
) {
    for (entity, job, mut output) in &mut outputs {
        let Ok((mut state, mut revision)) = states.get_mut(job.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        if state.snapshot.workspace != output.repo_root {
            commands.entity(entity).despawn();
            continue;
        }
        let catalog = output.catalog;
        let offset = u32::try_from(output.offset).unwrap_or(u32::MAX);
        if state.snapshot.nonce != output.revision {
            if revision.finish(catalog, output.generation) {
                let mut progress = state.snapshot.catalog(catalog);
                progress.loading = false;
                state
                    .bypass_change_detection()
                    .snapshot
                    .catalog_mut(catalog)
                    .clone_from(&progress);
                if let Some(ui_state) = ui_state.as_mut() {
                    ui_state.push(
                        job.webview,
                        GitCatalogPatch {
                            catalog,
                            offset,
                            entries: None,
                            progress,
                            message: None,
                        },
                    );
                }
            }
            commands.entity(entity).despawn();
            continue;
        }
        if !revision.finish(catalog, output.generation) {
            commands.entity(entity).despawn();
            continue;
        }
        let patch = match &mut output.result {
            Ok(page) => {
                if state.snapshot.repository.is_none() {
                    commands.entity(entity).despawn();
                    continue;
                }
                let (entries, has_more) = match page {
                    GitCatalogPage::LocalBranches(entries, has_more) => (
                        GitCatalogEntries::LocalBranches(std::mem::take(entries)),
                        *has_more,
                    ),
                    GitCatalogPage::RemoteBranches(entries, has_more) => (
                        GitCatalogEntries::RemoteBranches(std::mem::take(entries)),
                        *has_more,
                    ),
                    GitCatalogPage::Tags(entries, has_more) => {
                        (GitCatalogEntries::Tags(std::mem::take(entries)), *has_more)
                    }
                    GitCatalogPage::Commits(entries, has_more) => (
                        GitCatalogEntries::Commits(std::mem::take(entries)),
                        *has_more,
                    ),
                    GitCatalogPage::Stashes(entries, has_more) => (
                        GitCatalogEntries::Stashes(std::mem::take(entries)),
                        *has_more,
                    ),
                };
                GitCatalogPatch {
                    catalog,
                    offset,
                    entries: Some(entries),
                    progress: crate::state::GitCatalogProgress {
                        initialized: true,
                        loaded: true,
                        loading: false,
                        has_more,
                        failed: false,
                    },
                    message: None,
                }
            }
            Err(error) => GitCatalogPatch {
                catalog,
                offset,
                entries: None,
                progress: crate::state::GitCatalogProgress {
                    initialized: state.snapshot.catalog(catalog).initialized,
                    loaded: false,
                    loading: false,
                    has_more: false,
                    failed: true,
                },
                message: Some(error.message.clone()),
            },
        };
        state
            .bypass_change_detection()
            .snapshot
            .apply_catalog_patch(&patch);
        if let Some(ui_state) = ui_state.as_mut() {
            ui_state.push(job.webview, patch);
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_diff_outputs(
    mut outputs: Query<(Entity, &GitJob, &mut DiffOutput)>,
    presented: Query<(), With<vmux_ecs::PagePresented>>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
        &mut super::state::GitDiffProjectionState,
    )>,
    mut files: super::status::FileGitStates,
    mut diffs: Query<&mut super::diff::GitDiffQuery>,
    mut ui_state: Option<vmux_ecs::UiStateWriter<UiGit>>,
    mut commands: Commands,
) {
    for (entity, job, mut output) in &mut outputs {
        let webview = job.webview;
        let Ok(mut query) = diffs.get_mut(webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        let succeeded = output.viewport.error.is_empty();
        let mut event = output.take_viewport();
        if !query.accepts(event.generation) {
            commands.entity(entity).despawn();
            continue;
        }
        event.revision = query.complete_load();
        let next_request = crate::event::GitDiffNextRequest {
            generation: event.generation,
            revision: event.revision,
        };
        let loaded_lines = u32::try_from(event.lines.len()).unwrap_or(u32::MAX);
        let branch_has_more = query.complete_branch(
            &output.branch_paths,
            output.branch_top_line,
            loaded_lines,
            output.branch_page_complete,
            succeeded,
            output.source.clone(),
        );
        if !output.branch_paths.is_empty() {
            event.has_more = branch_has_more;
            if !succeeded {
                event.total_lines = event.first_line;
            }
        }
        query.cache_source(output.source.clone(), event.has_more);
        if let Ok((mut view, mut revealed, mut projection)) = views.get_mut(webview) {
            if view.snapshot.can_append_diff(&event) {
                let snapshot = &mut view.bypass_change_detection().snapshot;
                let append = snapshot
                    .append_diff_patch(event, &revealed.0)
                    .expect("appendability was checked");
                projection.capture(snapshot, &revealed.0);
                if let Some(ui_state) = ui_state.as_mut() {
                    ui_state.push(webview, append);
                }
            } else {
                view.snapshot.diff_loading = false;
                view.snapshot.diff_viewport = Some(event);
                revealed.0.clear();
            }
            if presented.contains(webview)
                && branch_has_more
                && succeeded
                && query.continue_prefetch(loaded_lines)
            {
                commands.trigger(bevy_cef::prelude::UiInput {
                    webview,
                    payload: next_request,
                });
            }
            commands.entity(entity).despawn();
            continue;
        }
        if files.accepts_diff(webview, event.generation, &query) {
            files.apply_diff(webview, event);
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_branch_files_outputs(
    mut outputs: Query<(Entity, &GitJob, &mut BranchFilesOutput)>,
    mut queries: Query<&mut super::diff::GitBranchFilesQuery>,
    mut states: Query<(
        &mut super::state::GitState,
        &crate::state::GitPageControllerState,
        &mut super::state::GitCatalogRevision,
    )>,
    mut ui_state: Option<vmux_ecs::UiStateWriter<UiGit>>,
    mut commands: Commands,
) {
    for (entity, job, mut output) in &mut outputs {
        let Ok(mut query) = queries.get_mut(job.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        if !query.accepts(output.generation, &output.reference) {
            commands.entity(entity).despawn();
            continue;
        }
        let Ok((mut state, controller, mut revision)) = states.get_mut(job.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        let reference = output.reference.clone();
        let patch = match &mut output.result {
            Ok(catalog) => {
                let base_ref = catalog.base_ref().to_string();
                let diff_paths = catalog.path_labels();
                let page = query.replace(std::mem::take(catalog));
                GitBranchFilesPatch {
                    reference: reference.clone(),
                    base_ref: Some(base_ref),
                    diff_paths: Some(diff_paths),
                    page: Some(GitBranchFilesPagePatch {
                        offset: 0,
                        files: page.files,
                        total: page.total,
                        has_more: page.has_more,
                    }),
                    diff_loading: None,
                    loading: false,
                    error: String::new(),
                }
            }
            Err(error) => GitBranchFilesPatch {
                reference: reference.clone(),
                base_ref: None,
                diff_paths: None,
                page: None,
                diff_loading: (controller.focused_panel == crate::state::GitPanel::Branches
                    && controller.selected_branch == reference)
                    .then_some(false),
                loading: false,
                error: std::mem::take(&mut error.message),
            },
        };
        state
            .bypass_change_detection()
            .snapshot
            .apply_branch_files_patch(&patch);
        revision.advance();
        if let Some(ui_state) = ui_state.as_mut() {
            ui_state.push(job.webview, patch);
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_operation_outputs(
    outputs: Query<(Entity, &GitJob, &OperationOutput)>,
    mut pending_branches: Query<&mut super::controller::PendingBranchCheckout>,
    mut states: super::state::GitStates,
    mut files: super::status::FileGitStates,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, job, output) in &outputs {
        let webview = job.webview;
        if states.contains(webview) {
            if !states.repository_matches(webview, &output.repository) {
                commands.entity(entity).despawn();
                continue;
            }
            let branch = if output.result.operation == "new branch" && output.result.ok {
                pending_branches
                    .get_mut(webview)
                    .ok()
                    .map(|mut pending| std::mem::take(&mut pending.0))
                    .filter(|branch| !branch.is_empty())
            } else {
                None
            };
            let (workspace, revision) = states
                .apply_result(webview, &output.result)
                .unwrap_or_default();
            if let Some(branch) = branch {
                commands.trigger(bevy_cef::prelude::UiInput {
                    webview,
                    payload: vmux_ecs::event::space::ProjectActivateRequest {
                        path: workspace.clone(),
                        branch,
                        checkout: String::new(),
                        pane_id: None,
                    },
                });
            }
            if let Some(Err(error)) = &output.status {
                states.apply_error(webview, error);
            }
            if !workspace.is_empty() {
                commands.spawn((
                    GitJob::new(webview),
                    RepositoryJob {
                        path: workspace.into(),
                        refresh_catalogs: true,
                        revision,
                    },
                ));
            }
        } else if files.repository_matches(webview, &output.repository) {
            let refresh = files.apply_result(webview, output.result.clone(), wake.clone());
            match &output.status {
                Some(Ok(status)) => files.apply_status(webview, status.clone()),
                Some(Err(error)) => files.apply_error(webview, error.message.clone()),
                None => {}
            }
            if let Some(refresh) = refresh {
                commands.entity(webview).insert(refresh);
            }
        }
        commands.entity(entity).despawn();
    }
}

type Failures<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GitJob,
        &'static FailureOutput,
        Option<&'static RepositoryJob>,
        Option<&'static CatalogJob>,
        Option<&'static BranchFilesJob>,
        Option<&'static DiffJob>,
        Option<&'static GitRepository>,
    ),
>;

fn deliver_failure_outputs(
    outputs: Failures,
    branch_queries: Query<&super::diff::GitBranchFilesQuery>,
    diff_queries: Query<&super::diff::GitDiffQuery>,
    mut states: super::state::GitStates,
    mut files: super::status::FileGitStates,
    mut commands: Commands,
) {
    for (entity, job, output, repository, catalog, branch_files, diff, authority) in &outputs {
        if let Some(repository) = repository {
            if states.repository_requested_at(job.webview, &repository.path, repository.revision) {
                states.fail_repository(job.webview, &output.0);
            }
        } else if states.contains(job.webview) {
            let current = authority.is_some_and(|repository| {
                states.repository_matches(job.webview, repository.path())
            });
            if !current {
                commands.entity(entity).despawn();
                continue;
            }
            if let Some(catalog) = catalog {
                states.fail_catalog(job.webview, catalog.catalog, catalog.generation, &output.0);
            } else if let Some(branch_files) = branch_files
                && branch_queries.get(job.webview).is_ok_and(|query| {
                    query.accepts(branch_files.generation, &branch_files.reference)
                })
            {
                states.fail_branch_files(job.webview, &output.0);
            } else if let Some(diff) = diff
                && diff_queries
                    .get(job.webview)
                    .is_ok_and(|query| query.accepts(diff.generation))
            {
                states.fail_diff(job.webview, diff.generation, &output.0);
            } else if catalog.is_none() && branch_files.is_none() && diff.is_none() {
                states.apply_error(job.webview, &output.0);
            }
        } else if files.contains(job.webview) {
            let current = authority
                .is_some_and(|repository| files.repository_matches(job.webview, repository.path()));
            if !current {
                commands.entity(entity).despawn();
                continue;
            }
            if let Some(diff) = diff
                && diff_queries
                    .get(job.webview)
                    .is_ok_and(|query| files.accepts_diff(job.webview, diff.generation, query))
            {
                files.fail_diff(job.webview, diff.generation, output.0.message.clone());
            } else if diff.is_none() {
                files.apply_error(job.webview, output.0.message.clone());
            }
        }
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct PendingGitJobs<'w, 's, J: Component> {
    queues: Query<'w, 's, (Entity, &'static GitJobs)>,
    pending: Query<'w, 's, &'static J>,
    owners: Query<'w, 's, &'static GitJob>,
    repositories: Query<'w, 's, &'static GitRepository>,
    running: Query<'w, 's, (), With<GitJobRunning>>,
    reads: Query<'w, 's, (), With<super::job::GitReadJob>>,
    presented: Query<'w, 's, (), With<vmux_ecs::PagePresented>>,
    transitioning: Query<'w, 's, (), With<vmux_ecs::PageTransition>>,
    keyboard_owners: Query<'w, 's, (), With<vmux_ecs::KeyboardOwner>>,
    wake: Option<Res<'w, EventLoopProxyWrapper>>,
    runtime: Res<'w, GitRuntime>,
}

impl<J: Component + Clone> PendingGitJobs<'_, '_, J> {
    fn owner(&self, entity: Entity) -> Option<Entity> {
        self.owners.get(entity).ok().map(|job| job.webview)
    }

    fn owner_accepts_reads(&self, owner: Entity) -> bool {
        self.presented.contains(owner)
            || self.transitioning.contains(owner)
            || self.keyboard_owners.contains(owner)
    }

    fn candidate(&self, owner: Entity, queue: &GitJobs) -> Option<Entity> {
        if queue.iter().any(|entity| self.running.contains(entity)) {
            return None;
        }
        for entity in queue.iter() {
            if self.reads.contains(entity) && !self.owner_accepts_reads(owner) {
                continue;
            }
            return Some(entity);
        }
        None
    }

    fn spawn<T: Send + Sync + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitJobTask<T>> {
        GitJobTask::spawn(&self.runtime, wake, run)
    }

    fn spawn_operation<T: Send + Sync + 'static>(
        &self,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Option<GitJobTask<T>> {
        self.runtime
            .spawn_operation(wake, run)
            .map(|task| GitJobTask { task })
    }

    fn ready(
        &self,
    ) -> Vec<(
        Entity,
        J,
        Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    )> {
        let wake = self.wake.as_deref().map(|wake| (**wake).clone());
        let mut ready = Vec::new();
        for (owner, queue) in &self.queues {
            if ready.len() >= 8 {
                break;
            }
            let Some(entity) = self.candidate(owner, queue) else {
                continue;
            };
            let Ok(job) = self.pending.get(entity) else {
                continue;
            };
            ready.push((entity, job.clone(), wake.clone()));
        }
        ready
    }

    fn ready_with_repository(
        &self,
    ) -> Vec<(
        Entity,
        J,
        GitRepository,
        Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    )> {
        let wake = self.wake.as_deref().map(|wake| (**wake).clone());
        let mut ready = Vec::new();
        for (owner, queue) in &self.queues {
            if ready.len() >= 8 {
                break;
            }
            let Some(entity) = self.candidate(owner, queue) else {
                continue;
            };
            let Ok(job) = self.pending.get(entity) else {
                continue;
            };
            let Ok(repository) = self.repositories.get(entity) else {
                continue;
            };
            ready.push((entity, job.clone(), repository.clone(), wake.clone()));
        }
        ready
    }
}

fn start_repository_jobs(
    jobs: PendingGitJobs<RepositoryJob>,
    loads: Res<RepositoryLoads>,
    mut epoch: ResMut<RepositoryLoadEpoch>,
    mut commands: Commands,
) {
    let ready = jobs.ready();
    if ready.is_empty() {
        return;
    }
    epoch.0 = epoch.0.wrapping_add(1).max(1);
    let epoch = epoch.0;
    for (entity, job, wake) in ready {
        let loads = loads.clone();
        let Some(task) = jobs.spawn(wake, move || {
            let result = loads.read(&job.path, epoch);
            RepositoryOutput(result)
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_catalog_jobs(
    jobs: PendingGitJobs<CatalogJob>,
    snapshots: Res<CatalogSnapshots>,
    mut commands: Commands,
) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(owner) = jobs.owner(entity) else {
            continue;
        };
        let repo_root = repository.path().to_string_lossy().into_owned();
        let snapshots = snapshots.as_ref().clone();
        let Some(task) = jobs.spawn(wake, move || CatalogOutput {
            catalog: job.catalog,
            repo_root,
            offset: job.offset,
            revision: job.revision,
            generation: job.generation,
            result: snapshots.page(owner, &repository, &job),
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_branch_files_jobs(jobs: PendingGitJobs<BranchFilesJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || BranchFilesOutput {
            generation: job.generation,
            reference: job.reference.clone(),
            result: repository
                .branch_files_catalog(&job.reference)
                .map_err(|error| GitOperationError { message: error.0 }),
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_diff_jobs(jobs: PendingGitJobs<DiffJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || {
            if !GitRepository::has_repository(repository.path()) {
                return DiffOutput {
                    viewport: GitDiffViewport {
                        generation: job.generation,
                        revision: 0,
                        first_line: job.top_line,
                        total_lines: 0,
                        has_more: false,
                        lines: Vec::new(),
                        markers: Vec::new(),
                        error: String::new(),
                    },
                    branch_paths: job.branch_paths,
                    source: None,
                    branch_top_line: job.branch_top_line,
                    branch_page_complete: true,
                };
            }
            let result = match job.cached_source.clone() {
                Some(source) => Ok(source),
                None => match &job.scope {
                    DiffScope::File => match job.content.as_deref() {
                        Some(content) => repository.diff_source_with_content(&job.path, content),
                        None => repository.diff_source(&job.path),
                    },
                    DiffScope::Branch(reference) => {
                        if !job.branch_source.as_os_str().is_empty()
                            && !job.branch_merge_base.is_empty()
                        {
                            GitRepository::at(job.branch_source.clone())
                                .branch_diff_source_for_paths_against(
                                    reference,
                                    &job.branch_merge_base,
                                    &job.branch_paths,
                                )
                        } else {
                            repository.branch_diff_source_for_paths(reference, &job.branch_paths)
                        }
                    }
                    DiffScope::Commit(reference) => repository.commit_diff_source(reference),
                    DiffScope::Stash(reference) => repository.stash_diff_source(reference),
                },
            };
            match result {
                Ok(source) => {
                    let window_top = if matches!(job.scope, DiffScope::Branch(_)) {
                        job.branch_top_line
                    } else {
                        job.top_line
                    };
                    let (source_total, lines) =
                        match source.window(&repository, window_top, job.rows) {
                            Ok(window) => window,
                            Err(error) => return DiffOutput::failure(&job, error),
                        };
                    let (first_line, total_lines, has_more, lines, branch_page_complete) =
                        if matches!(job.scope, DiffScope::Branch(_)) {
                            let page_lines = u32::try_from(lines.len()).unwrap_or(u32::MAX);
                            let branch_loaded = job.branch_top_line.saturating_add(page_lines);
                            let branch_page_complete = branch_loaded >= source_total;
                            (
                                job.top_line,
                                job.top_line.saturating_add(page_lines),
                                !branch_page_complete || job.branch_has_more,
                                lines,
                                branch_page_complete,
                            )
                        } else {
                            let loaded = job
                                .top_line
                                .saturating_add(u32::try_from(lines.len()).unwrap_or(u32::MAX));
                            (
                                job.top_line.min(source_total),
                                source_total,
                                loaded < source_total,
                                lines,
                                true,
                            )
                        };
                    let markers = if matches!(job.scope, DiffScope::File) {
                        super::diff::GitDiffMarkers::from_lines(&lines).into_inner()
                    } else {
                        Vec::new()
                    };
                    DiffOutput {
                        viewport: GitDiffViewport {
                            generation: job.generation,
                            revision: 0,
                            first_line,
                            total_lines,
                            has_more,
                            lines,
                            markers,
                            error: String::new(),
                        },
                        branch_paths: job.branch_paths,
                        source: Some(source),
                        branch_top_line: job.branch_top_line,
                        branch_page_complete,
                    }
                }
                Err(error) => DiffOutput::failure(&job, error),
            }
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_stage_jobs(jobs: PendingGitJobs<StageJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || match repository.stage(&job.path) {
            Ok(()) => OperationOutput::success_with_status(&repository, &job.path, "stage", "ok"),
            Err(error) => OperationOutput::failure(repository.path(), "stage", error),
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_unstage_jobs(jobs: PendingGitJobs<UnstageJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || match repository.unstage(&job.path) {
            Ok(()) => OperationOutput::success_with_status(&repository, &job.path, "unstage", "ok"),
            Err(error) => OperationOutput::failure(repository.path(), "unstage", error),
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_discard_jobs(jobs: PendingGitJobs<DiscardJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || match repository.discard(&job.path) {
            Ok(()) => OperationOutput::success_with_status(&repository, &job.path, "discard", "ok"),
            Err(error) => OperationOutput::failure(repository.path(), "discard", error),
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_commit_jobs(jobs: PendingGitJobs<CommitJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let Some(task) =
            jobs.spawn_operation(wake, move || match GitRepository::discover(&job.path) {
                Ok(repository) => match repository.commit(&job.message) {
                    Ok(()) => OperationOutput::success_with_status(
                        &repository,
                        &job.path,
                        "commit",
                        "committed",
                    ),
                    Err(error) => OperationOutput::failure(repository.path(), "commit", error),
                },
                Err(error) => OperationOutput::failure(job.path, "commit", error),
            })
        else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_fetch_jobs(jobs: PendingGitJobs<FetchJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let Some(task) =
            jobs.spawn_operation(wake, move || match GitRepository::discover(&job.path) {
                Ok(repository) => match repository.fetch() {
                    Ok(()) => OperationOutput::success_with_status(
                        &repository,
                        &job.path,
                        "fetch",
                        "fetched",
                    ),
                    Err(error) => OperationOutput::failure(repository.path(), "fetch", error),
                },
                Err(error) => OperationOutput::failure(job.path, "fetch", error),
            })
        else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_pull_jobs(jobs: PendingGitJobs<PullJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let Some(task) =
            jobs.spawn_operation(wake, move || match GitRepository::discover(&job.path) {
                Ok(repository) => match repository.pull() {
                    Ok(()) => OperationOutput::success_with_status(
                        &repository,
                        &job.path,
                        "pull",
                        "pulled",
                    ),
                    Err(error) => OperationOutput::failure(repository.path(), "pull", error),
                },
                Err(error) => OperationOutput::failure(job.path, "pull", error),
            })
        else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_push_jobs(jobs: PendingGitJobs<PushJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let Some(task) =
            jobs.spawn_operation(wake, move || match GitRepository::discover(&job.path) {
                Ok(repository) => match repository.push() {
                    Ok(()) => OperationOutput::success_with_status(
                        &repository,
                        &job.path,
                        "push",
                        "pushed",
                    ),
                    Err(error) => OperationOutput::failure(repository.path(), "push", error),
                },
                Err(error) => OperationOutput::failure(job.path, "push", error),
            })
        else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_stage_all_jobs(jobs: PendingGitJobs<StageAllJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let Some(task) = jobs.spawn(wake, move || match GitRepository::discover(&job.path) {
            Ok(repository) => match repository.stage_all() {
                Ok(()) => OperationOutput::success_with_status(
                    &repository,
                    &job.path,
                    "stage all",
                    "staged",
                ),
                Err(error) => OperationOutput::failure(repository.path(), "stage all", error),
            },
            Err(error) => OperationOutput::failure(job.path, "stage all", error),
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_hunk_jobs(jobs: PendingGitJobs<HunkJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || {
            match repository.apply_hunk(&job.path, job.hunk, job.accept) {
                Ok(()) => OperationOutput::success_with_status(
                    &repository,
                    &job.path,
                    if job.accept { "accept" } else { "reject" },
                    "ok",
                ),
                Err(error) => OperationOutput::failure(repository.path(), "hunk", error),
            }
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_amend_jobs(jobs: PendingGitJobs<AmendJob>, mut commands: Commands) {
    for (entity, _, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(&repository, "amend", repository.amend())
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_checkout_commit_jobs(jobs: PendingGitJobs<CheckoutCommitJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "checkout commit",
                repository.checkout_commit(&job.commit),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_cherry_pick_jobs(jobs: PendingGitJobs<CherryPickJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "cherry-pick",
                repository.cherry_pick(&job.commit),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_create_branch_jobs(jobs: PendingGitJobs<CreateBranchJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "new branch",
                repository.create_branch(&job.branch, &job.start_point),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_delete_branch_jobs(jobs: PendingGitJobs<DeleteBranchJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "delete branch",
                repository.delete_branch(&job.branch),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_fast_forward_jobs(jobs: PendingGitJobs<FastForwardJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "fast-forward",
                repository.fast_forward(&job.branch),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_merge_jobs(jobs: PendingGitJobs<MergeJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(&repository, "merge", repository.merge(&job.branch))
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_rebase_jobs(jobs: PendingGitJobs<RebaseJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(&repository, "rebase", repository.rebase(&job.branch))
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_revert_jobs(jobs: PendingGitJobs<RevertJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(&repository, "revert", repository.revert(&job.commit))
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_stash_drop_jobs(jobs: PendingGitJobs<StashDropJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "stash drop",
                repository.stash_drop(&job.reference),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_stash_pop_jobs(jobs: PendingGitJobs<StashPopJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                &repository,
                "stash pop",
                repository.stash_pop(&job.reference),
            )
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_stash_push_jobs(jobs: PendingGitJobs<StashPushJob>, mut commands: Commands) {
    for (entity, _, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(&repository, "stash", repository.stash_push())
        }) else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::event::{FileStatus, GitDiffViewport};
    use crate::host::repository::test_repo;

    #[derive(Resource, Default)]
    struct CapturedOutputs {
        diffs: Vec<GitDiffViewport>,
        operations: Vec<OperationOutput>,
    }

    fn capture_outputs(
        diffs: Query<&DiffOutput>,
        operations: Query<&OperationOutput>,
        mut captured: ResMut<CapturedOutputs>,
    ) {
        for output in &diffs {
            captured.diffs.push(output.viewport.clone());
        }
        for output in &operations {
            captured.operations.push(output.clone());
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<CapturedOutputs>()
            .add_plugins(vmux_ecs::UiStatePlugin::<UiGit>::default())
            .add_plugins(JobPlugin)
            .add_systems(
                Update,
                capture_outputs
                    .after(poll_jobs::<DiffOutput>)
                    .after(poll_jobs::<OperationOutput>)
                    .before(deliver_diff_outputs)
                    .before(deliver_operation_outputs),
            );
        app
    }

    fn captured(app: &mut App) -> CapturedOutputs {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            app.update();
            let mut captured = app.world_mut().resource_mut::<CapturedOutputs>();
            if !captured.diffs.is_empty() || !captured.operations.is_empty() {
                return std::mem::take(&mut *captured);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("git job did not finish");
    }

    fn dirty_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");
        (repo, file)
    }

    fn owner(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                super::super::state::GitState::default(),
                vmux_ecs::PagePresented,
            ))
            .id()
    }

    #[test]
    fn catalog_snapshots_are_scoped_by_owner_and_replace_old_revisions() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let key = |owner, revision| CatalogSnapshotKey {
            owner,
            repository: PathBuf::from("/repo"),
            catalog: 2,
            branch: String::new(),
            revision,
        };
        let snapshot = || Arc::new(GitCatalogSnapshot::Tags(Vec::new()));
        let mut cache = CatalogSnapshotCache::default();

        cache.insert(key(first, 1), snapshot());
        cache.insert(key(second, 1), snapshot());
        cache.insert(key(first, 2), snapshot());

        assert_eq!(cache.snapshots.len(), 2);
        assert!(!cache.snapshots.contains_key(&key(first, 1)));
        assert!(cache.snapshots.contains_key(&key(first, 2)));
        assert!(cache.snapshots.contains_key(&key(second, 1)));
    }

    #[test]
    fn hidden_pages_defer_read_jobs_until_presented() {
        let mut app = app();
        let webview = owner(&mut app);
        app.world_mut()
            .entity_mut(webview)
            .remove::<vmux_ecs::PagePresented>();
        {
            let mut state = app
                .world_mut()
                .get_mut::<super::super::state::GitState>(webview)
                .unwrap();
            state.snapshot.workspace = "/tmp/repository".to_string();
            state.snapshot.nonce = 1;
        }
        let revision = app
            .world_mut()
            .get_mut::<super::super::state::GitRepositoryLoadRevision>(webview)
            .unwrap()
            .advance();
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: "/tmp/repository".into(),
                    refresh_catalogs: false,
                    revision,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<RepositoryJob>(job).is_some());
        assert!(app.world().get::<GitJobRunning>(job).is_none());

        app.world_mut()
            .entity_mut(webview)
            .insert(vmux_ecs::PagePresented);
        app.update();

        assert!(app.world().get::<GitJobRunning>(job).is_some());
    }

    #[test]
    fn opening_page_starts_read_jobs_before_presentation() {
        let mut app = app();
        let webview = app
            .world_mut()
            .spawn((
                super::super::state::GitState::default(),
                vmux_ecs::PageTransition,
            ))
            .id();
        {
            let mut state = app
                .world_mut()
                .get_mut::<super::super::state::GitState>(webview)
                .unwrap();
            state.snapshot.workspace = "/tmp/repository".to_string();
            state.snapshot.nonce = 1;
        }
        let revision = app
            .world_mut()
            .get_mut::<super::super::state::GitRepositoryLoadRevision>(webview)
            .unwrap()
            .advance();
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: "/tmp/repository".into(),
                    refresh_catalogs: false,
                    revision,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<GitJobRunning>(job).is_some());
    }

    #[test]
    fn foreground_repository_load_waits_until_the_page_is_transitioning() {
        let mut app = app();
        let webview = owner(&mut app);
        app.world_mut()
            .entity_mut(webview)
            .remove::<vmux_ecs::PagePresented>();
        {
            let mut state = app
                .world_mut()
                .get_mut::<super::super::state::GitState>(webview)
                .unwrap();
            state.snapshot.workspace = "/tmp/repository".to_string();
            state.snapshot.nonce = 1;
        }
        let revision = app
            .world_mut()
            .get_mut::<super::super::state::GitRepositoryLoadRevision>(webview)
            .unwrap()
            .advance();
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                super::super::job::GitForegroundRead,
                RepositoryJob {
                    path: "/tmp/repository".into(),
                    refresh_catalogs: false,
                    revision,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<GitJobRunning>(job).is_none());

        app.world_mut()
            .entity_mut(webview)
            .insert(vmux_ecs::PageTransition);
        app.update();

        assert!(app.world().get::<GitJobRunning>(job).is_some());
    }

    #[test]
    fn hidden_reads_do_not_block_an_accepted_mutation() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = owner(&mut app);
        app.world_mut()
            .entity_mut(webview)
            .remove::<vmux_ecs::PagePresented>();
        let read = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: repo.path().to_path_buf(),
                    refresh_catalogs: false,
                    revision: 1,
                },
            ))
            .id();
        let mutation = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                GitRepository::at(repo.path()),
                StageJob { path: file },
            ))
            .id();

        app.update();

        assert!(app.world().get::<GitJobRunning>(read).is_none());
        assert!(app.world().get::<GitJobRunning>(mutation).is_some());
    }

    #[test]
    fn jobs_for_one_webview_run_in_relationship_order() {
        let mut app = app();
        let webview = owner(&mut app);
        let root = tempfile::tempdir().unwrap();
        app.world_mut()
            .get_mut::<super::super::state::GitState>(webview)
            .unwrap()
            .snapshot
            .workspace = root.path().join("first").to_string_lossy().into_owned();
        let first = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: root.path().join("first"),
                    refresh_catalogs: true,
                    revision: 0,
                },
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: root.path().join("first"),
                    refresh_catalogs: true,
                    revision: 0,
                },
            ))
            .id();

        assert_eq!(
            app.world()
                .get::<GitJobs>(webview)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        app.update();

        assert!(app.world().get::<GitJobRunning>(first).is_some());
        assert!(app.world().get::<RepositoryJob>(first).is_some());
        assert!(app.world().get::<RepositoryJob>(second).is_some());
        assert!(app.world().get::<GitJobRunning>(second).is_none());
    }

    #[test]
    fn repository_requests_start_together_for_runtime_coalescing() {
        let repo = test_repo::init();
        let mut app = app();
        let first = owner(&mut app);
        let second = owner(&mut app);
        for owner in [first, second] {
            app.world_mut()
                .get_mut::<super::super::state::GitState>(owner)
                .unwrap()
                .snapshot
                .workspace = repo.path().to_string_lossy().into_owned();
        }
        app.world_mut().spawn((
            GitJob::new(first),
            RepositoryJob {
                path: repo.path().to_path_buf(),
                refresh_catalogs: true,
                revision: 0,
            },
        ));
        app.world_mut().spawn((
            GitJob::new(second),
            RepositoryJob {
                path: repo.path().to_path_buf(),
                refresh_catalogs: true,
                revision: 0,
            },
        ));

        app.update();

        let mut running = app
            .world_mut()
            .query_filtered::<Entity, (With<GitJobRunning>, With<RepositoryJob>)>();
        assert_eq!(running.iter(app.world()).count(), 2);
        let mut pending = app
            .world_mut()
            .query_filtered::<&RepositoryJob, Without<GitJobRunning>>();
        assert_eq!(pending.iter(app.world()).count(), 0);
    }

    #[test]
    fn repository_loads_share_root_and_subdirectory_work_per_epoch() {
        let repo = test_repo::init();
        let nested = repo.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let loads = RepositoryLoads::default();

        let root = loads.read(repo.path(), 7).unwrap();
        let child = loads.read(&nested, 7).unwrap();
        let refreshed = loads.read(repo.path(), 8).unwrap();

        assert!(Arc::ptr_eq(&root, &child));
        assert!(!Arc::ptr_eq(&root, &refreshed));
        let cached = loads.cached(&nested).unwrap();
        assert_eq!(cached.path, nested.to_string_lossy());
        assert_eq!(
            PathIdentity::resolve(&cached.repo_root),
            PathIdentity::resolve(repo.path())
        );
    }

    #[test]
    fn repository_load_aliases_are_bounded() {
        let mut cache = RepositoryLoadCache::default();
        let root = PathBuf::from("/tmp/repository");
        for index in 0..=REPOSITORY_LOAD_ALIAS_CAPACITY {
            cache.remember_alias(
                PathIdentity::lexical(format!("/tmp/repository/{index}")),
                PathIdentity::resolve(format!("/tmp/repository/{index}")),
                root.clone(),
            );
        }

        assert_eq!(cache.aliases.len(), REPOSITORY_LOAD_ALIAS_CAPACITY);
        assert!(
            !cache
                .aliases
                .contains_key(&PathIdentity::lexical("/tmp/repository/0"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn repository_load_cache_rejects_a_retargeted_symlink_alias() {
        let first = test_repo::init();
        let second = test_repo::init();
        let parent = tempfile::tempdir().unwrap();
        let alias = parent.path().join("repository");
        symlink(first.path(), &alias).unwrap();
        let loads = RepositoryLoads::default();

        loads.read(&alias, 1).unwrap();
        assert!(loads.cached(&alias).is_some());

        std::fs::remove_file(&alias).unwrap();
        symlink(second.path(), &alias).unwrap();

        assert!(loads.cached(&alias).is_none());
    }

    #[test]
    fn stale_repository_output_does_not_replace_a_newer_request() {
        let first = test_repo::init();
        let mut app = app();
        let webview = owner(&mut app);
        let mut state = app
            .world_mut()
            .get_mut::<super::super::state::GitState>(webview)
            .unwrap();
        state.snapshot.workspace = first.path().to_string_lossy().into_owned();
        state.snapshot.nonce = 2;
        let output = GitRepositorySnapshot::load_summary(first.path())
            .map(Arc::new)
            .map_err(|error| GitOperationError { message: error.0 });
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: first.path().to_path_buf(),
                    refresh_catalogs: true,
                    revision: 1,
                },
                RepositoryOutput(output),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(job).is_err());
        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert_eq!(state.snapshot.workspace, first.path().to_string_lossy());
        assert!(state.snapshot.repository.is_none());
    }

    #[test]
    fn stale_catalog_output_releases_loading_state() {
        let mut app = app();
        let webview = owner(&mut app);
        {
            let mut state = app
                .world_mut()
                .get_mut::<super::super::state::GitState>(webview)
                .unwrap();
            state.snapshot.workspace = "/tmp/repository".to_string();
            state.snapshot.nonce = 2;
            state.snapshot.local_branches.loading = true;
        }
        let generation = app
            .world_mut()
            .get_mut::<super::super::state::GitCatalogRevision>(webview)
            .unwrap()
            .start(GitCatalog::LocalBranches);
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                CatalogJob {
                    catalog: GitCatalog::LocalBranches,
                    current_branch: String::new(),
                    offset: 0,
                    limit: 50,
                    revision: 1,
                    generation,
                },
                CatalogOutput {
                    catalog: GitCatalog::LocalBranches,
                    repo_root: "/tmp/repository".to_string(),
                    offset: 0,
                    revision: 1,
                    generation,
                    result: Err(GitOperationError {
                        message: "stale".to_string(),
                    }),
                },
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(job).is_err());
        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(!state.snapshot.local_branches.loading);
    }

    #[test]
    fn stale_catalog_output_does_not_release_a_newer_load() {
        let mut app = app();
        let webview = owner(&mut app);
        {
            let mut state = app
                .world_mut()
                .get_mut::<super::super::state::GitState>(webview)
                .unwrap();
            state.snapshot.workspace = "/tmp/repository".to_string();
            state.snapshot.nonce = 2;
            state.snapshot.local_branches.loading = true;
        }
        let old_generation = {
            let mut revision = app
                .world_mut()
                .get_mut::<super::super::state::GitCatalogRevision>(webview)
                .unwrap();
            let old = revision.start(GitCatalog::LocalBranches);
            let _ = revision.start(GitCatalog::LocalBranches);
            old
        };
        app.world_mut().spawn((
            GitJob::new(webview),
            CatalogOutput {
                catalog: GitCatalog::LocalBranches,
                repo_root: "/tmp/repository".to_string(),
                offset: 0,
                revision: 1,
                generation: old_generation,
                result: Err(GitOperationError {
                    message: "stale".to_string(),
                }),
            },
        ));

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(state.snapshot.local_branches.loading);
    }

    #[test]
    fn failed_diff_job_clears_loading_with_visible_error() {
        let mut app = app();
        let webview = owner(&mut app);
        let repo = test_repo::init();
        let request =
            crate::event::GitRepositoryRequest::at(repo.path().to_string_lossy().into_owned());
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.repo_root = repo.path().to_string_lossy().into_owned();
        repository.path.clone_from(&repository.repo_root);
        {
            let mut state = app
                .world_mut()
                .get_mut::<super::super::state::GitState>(webview)
                .unwrap();
            state.snapshot.workspace.clone_from(&repository.repo_root);
            state.snapshot.repository = Some(repository);
            state.snapshot.diff_loading = true;
            state.snapshot.loading = false;
        }
        app.world_mut()
            .entity_mut(webview)
            .insert(super::super::diff::GitDiffQuery::for_test(7));
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                GitRepository::at(repo.path()),
                DiffJob {
                    path: PathBuf::new(),
                    scope: DiffScope::File,
                    branch_paths: Vec::new(),
                    cached_source: None,
                    branch_source: PathBuf::new(),
                    branch_merge_base: String::new(),
                    branch_top_line: 0,
                    branch_has_more: false,
                    generation: 7,
                    top_line: 0,
                    rows: 50,
                    content: None,
                },
                FailureOutput(GitOperationError {
                    message: "worker failed".to_string(),
                }),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(job).is_err());
        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(!state.snapshot.diff_loading);
        let viewport = state.snapshot.diff_viewport.as_ref().unwrap();
        assert_eq!(viewport.generation, 7);
        assert_eq!(viewport.error, "worker failed");
    }

    #[test]
    fn stale_operation_output_does_not_mutate_a_different_repository() {
        let first = test_repo::init();
        let second = test_repo::init();
        let mut app = App::new();
        app.add_systems(Update, deliver_operation_outputs);
        let request =
            crate::event::GitRepositoryRequest::at(second.path().to_string_lossy().into_owned());
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.path = second.path().to_string_lossy().into_owned();
        repository.repo_root = repository.path.clone();
        let webview = app
            .world_mut()
            .spawn(super::super::state::GitState {
                snapshot: crate::state::GitPageSnapshot {
                    workspace: repository.repo_root.clone(),
                    repository: Some(repository),
                    ..Default::default()
                },
            })
            .id();
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                OperationOutput {
                    repository: first.path().to_path_buf(),
                    result: GitOperationResult {
                        operation: "fetch".to_string(),
                        ok: true,
                        message: "fetched".to_string(),
                    },
                    status: None,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(job).is_err());
        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(state.snapshot.result.is_none());
        assert_eq!(state.snapshot.nonce, 0);
    }

    #[test]
    fn stale_operation_failure_does_not_mutate_a_different_repository() {
        let first = test_repo::init();
        let second = test_repo::init();
        let mut app = App::new();
        app.add_systems(Update, deliver_failure_outputs);
        let request =
            crate::event::GitRepositoryRequest::at(second.path().to_string_lossy().into_owned());
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.path = second.path().to_string_lossy().into_owned();
        repository.repo_root = repository.path.clone();
        let webview = app
            .world_mut()
            .spawn(super::super::state::GitState {
                snapshot: crate::state::GitPageSnapshot {
                    workspace: repository.repo_root.clone(),
                    repository: Some(repository),
                    loading: false,
                    ..Default::default()
                },
            })
            .id();
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(first.path()),
            FailureOutput(GitOperationError {
                message: "worker failed".to_string(),
            }),
        ));

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(state.snapshot.message.is_empty());
    }

    #[test]
    fn stale_catalog_failure_does_not_release_a_newer_load() {
        let repo = test_repo::init();
        let mut app = App::new();
        app.add_systems(Update, deliver_failure_outputs);
        let request =
            crate::event::GitRepositoryRequest::at(repo.path().to_string_lossy().into_owned());
        let mut repository = GitRepositorySnapshot::loading(&request);
        repository.path = repo.path().to_string_lossy().into_owned();
        repository.repo_root = repository.path.clone();
        let webview = app
            .world_mut()
            .spawn(super::super::state::GitState {
                snapshot: crate::state::GitPageSnapshot {
                    workspace: repository.repo_root.clone(),
                    repository: Some(repository),
                    local_branches: crate::state::GitCatalogProgress {
                        loading: true,
                        ..Default::default()
                    },
                    loading: false,
                    ..Default::default()
                },
            })
            .id();
        let stale = {
            let mut revision = app
                .world_mut()
                .get_mut::<super::super::state::GitCatalogRevision>(webview)
                .unwrap();
            let stale = revision.start(GitCatalog::LocalBranches);
            revision.start(GitCatalog::LocalBranches);
            stale
        };
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(repo.path()),
            CatalogJob {
                catalog: GitCatalog::LocalBranches,
                current_branch: "main".to_string(),
                offset: 0,
                limit: 40,
                revision: 1,
                generation: stale,
            },
            FailureOutput(GitOperationError {
                message: "worker failed".to_string(),
            }),
        ));

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(state.snapshot.local_branches.loading);
        assert!(state.snapshot.message.is_empty());
    }

    #[test]
    fn jobs_are_despawned_when_their_owner_loses_git_state() {
        let mut app = app();
        let webview = owner(&mut app);
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: PathBuf::from("/tmp/repository"),
                    refresh_catalogs: true,
                    revision: 0,
                },
            ))
            .id();
        app.world_mut()
            .entity_mut(webview)
            .remove::<super::super::state::GitState>();

        app.update();

        assert!(app.world().get_entity(job).is_err());
    }

    #[test]
    fn running_diff_job_keeps_its_cancel_marker() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = owner(&mut app);
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                GitRepository::at(repo.path()),
                DiffJob {
                    path: file,
                    scope: DiffScope::File,
                    branch_paths: Vec::new(),
                    cached_source: None,
                    branch_source: PathBuf::new(),
                    branch_merge_base: String::new(),
                    branch_top_line: 0,
                    branch_has_more: false,
                    generation: 7,
                    top_line: 0,
                    rows: 50,
                    content: None,
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<GitJobRunning>(job).is_some());
        assert!(app.world().get::<DiffJob>(job).is_some());
    }

    #[test]
    fn diff_job_emits_projected_viewport() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = owner(&mut app);
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(repo.path()),
            DiffJob {
                path: file,
                scope: DiffScope::File,
                branch_paths: Vec::new(),
                cached_source: None,
                branch_source: PathBuf::new(),
                branch_merge_base: String::new(),
                branch_top_line: 0,
                branch_has_more: false,
                generation: 7,
                top_line: 0,
                rows: 50,
                content: None,
            },
        ));

        assert!(matches!(
            captured(&mut app).diffs.as_slice(),
            [GitDiffViewport { generation: 7, .. }]
        ));
    }

    #[test]
    fn stage_job_emits_result_then_fresh_status() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = owner(&mut app);
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(repo.path()),
            StageJob { path: file },
        ));

        let captured = captured(&mut app);
        let [output] = captured.operations.as_slice() else {
            panic!("unexpected: {:?}", captured.operations);
        };
        assert!(output.result.ok);
        assert!(matches!(
            &output.status,
            Some(Ok(status)) if status.file_status == FileStatus::Staged
        ));
    }

    #[test]
    fn diff_on_non_repo_emits_empty_viewport() {
        let dir = tempfile::tempdir().unwrap();
        let file = test_repo::write(dir.path(), "loose.txt", "x");
        let mut app = app();
        let webview = owner(&mut app);
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(dir.path()),
            DiffJob {
                path: file,
                scope: DiffScope::File,
                branch_paths: Vec::new(),
                cached_source: None,
                branch_source: PathBuf::new(),
                branch_merge_base: String::new(),
                branch_top_line: 0,
                branch_has_more: false,
                generation: 7,
                top_line: 0,
                rows: 50,
                content: None,
            },
        ));

        assert!(matches!(
            captured(&mut app).diffs.as_slice(),
            [GitDiffViewport {
                generation: 7,
                total_lines: 0,
                lines,
                markers,
                ..
            }] if lines.is_empty() && markers.is_empty()
        ));
    }
}
