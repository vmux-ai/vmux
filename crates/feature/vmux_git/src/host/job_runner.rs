use std::collections::HashMap;
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
use super::repository::{BranchFilesCatalog, GitCatalogPage, GitRepository, RepositorySummary};
use super::runtime::{GitRuntime, GitTask};
use crate::event::{
    DiffLine, GitCatalog, GitDiffViewport, GitFileStatus, GitOperationError, GitOperationResult,
    GitRepositorySnapshot,
};
use crate::state::UiGit;
use vmux_path::PathIdentity;

const REPOSITORY_LOAD_CACHE_CAPACITY: usize = 128;

pub(super) struct JobPlugin;

impl Plugin for JobPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .init_resource::<RepositoryLoads>()
            .init_resource::<RepositoryLoadEpoch>()
            .add_observer(queue_job_failure)
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

#[derive(EntityEvent)]
pub(super) struct GitJobFailure {
    #[event_target]
    pub(super) webview: Entity,
    pub(super) message: String,
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
struct RepositoryLoads(Arc<Mutex<HashMap<PathBuf, Arc<RepositoryLoad>>>>);

struct RepositoryLoad {
    state: Mutex<RepositoryLoadState>,
    ready: Condvar,
}

struct RepositoryLoadState {
    epoch: u64,
    loading: bool,
    result: Option<Result<Arc<RepositorySummary>, GitOperationError>>,
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
        if leader {
            return self.finish(repository, is_worktree);
        }
        let mut state = self.state.lock().map_err(|_| Self::coordinator_error())?;
        loop {
            if !state.loading {
                if epoch <= state.epoch {
                    return state
                        .result
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| Err(Self::coordinator_error()));
                }
                state.epoch = epoch;
                state.loading = true;
                state.result = None;
                drop(state);
                return self.finish(repository, is_worktree);
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
    ) -> Result<Arc<RepositorySummary>, GitOperationError> {
        let result = catch_unwind(AssertUnwindSafe(|| {
            GitRepositorySnapshot::load_discovered_summary(
                repository.path(),
                repository,
                is_worktree,
            )
            .map(Arc::new)
            .map_err(|error| GitOperationError { message: error.0 })
        }))
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
}

impl RepositoryLoads {
    fn read(&self, path: &Path, epoch: u64) -> Result<Arc<RepositorySummary>, GitOperationError> {
        let (repository, is_worktree) = GitRepository::discover_with_worktree(path)
            .map_err(|error| GitOperationError { message: error.0 })?;
        let root = PathIdentity::resolve(repository.path()).into_path_buf();
        let (load, leader) = {
            let mut loads = self
                .0
                .lock()
                .map_err(|_| RepositoryLoad::coordinator_error())?;
            match loads.get(&root) {
                Some(load) => (Arc::clone(load), false),
                None => {
                    if loads.len() >= REPOSITORY_LOAD_CACHE_CAPACITY {
                        loads.retain(|_, load| load.loading());
                    }
                    let load = Arc::new(RepositoryLoad::new(epoch));
                    loads.insert(root.clone(), Arc::clone(&load));
                    (load, true)
                }
            }
        };
        load.read(&repository, is_worktree, epoch, leader)
    }
}

#[derive(Resource, Default)]
struct RepositoryLoadEpoch(u64);

#[derive(Component)]
struct CatalogOutput {
    catalog: GitCatalog,
    repo_root: String,
    offset: usize,
    revision: u32,
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
    branch_lines: Option<Arc<Vec<DiffLine>>>,
    branch_top_line: u32,
    branch_page_complete: bool,
}

#[derive(Component, Clone, Debug)]
struct OperationOutput {
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

    fn from_result(operation: &str, result: Result<String, super::repository::GitError>) -> Self {
        match result {
            Ok(message) => Self {
                result: GitOperationResult {
                    operation: operation.to_string(),
                    ok: true,
                    message,
                },
                status: None,
            },
            Err(error) => Self::failure(operation, error),
        }
    }

    fn failure(operation: &str, error: super::repository::GitError) -> Self {
        Self {
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

fn queue_job_failure(trigger: On<GitJobFailure>, mut commands: Commands) {
    commands.spawn((
        GitJob::new(trigger.event().webview),
        FailureOutput(GitOperationError {
            message: trigger.event().message.clone(),
        }),
    ));
}

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
        if state.snapshot.nonce != repository.revision
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
                states.set_repository(webview, snapshot, request.refresh_catalogs);
            }
            Err(error) => {
                states.apply_error(webview, error);
            }
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_catalog_outputs(
    outputs: Query<(Entity, &GitJob, &CatalogOutput)>,
    mut states: Query<&mut super::state::GitState>,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        let Ok(mut state) = states.get_mut(job.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        if state.snapshot.workspace != output.repo_root || state.snapshot.nonce != output.revision {
            commands.entity(entity).despawn();
            continue;
        }
        match &output.result {
            Ok(page) => {
                let Some(repository) = state.snapshot.repository.as_mut() else {
                    commands.entity(entity).despawn();
                    continue;
                };
                let has_more = match page {
                    GitCatalogPage::LocalBranches(entries, has_more) => {
                        if output.offset == 0 {
                            if repository.branches.as_slice() != entries.as_slice() {
                                repository.branches.clone_from(entries);
                            }
                        } else {
                            repository.branches.extend(entries.clone());
                        }
                        *has_more
                    }
                    GitCatalogPage::RemoteBranches(entries, has_more) => {
                        if output.offset == 0 {
                            if repository.remote_branches.as_slice() != entries.as_slice() {
                                repository.remote_branches.clone_from(entries);
                            }
                        } else {
                            repository.remote_branches.extend(entries.clone());
                        }
                        *has_more
                    }
                    GitCatalogPage::Tags(entries, has_more) => {
                        if output.offset == 0 {
                            if repository.tags.as_slice() != entries.as_slice() {
                                repository.tags.clone_from(entries);
                            }
                        } else {
                            repository.tags.extend(entries.clone());
                        }
                        *has_more
                    }
                    GitCatalogPage::Commits(entries, has_more) => {
                        if output.offset == 0 {
                            if repository.commits.as_slice() != entries.as_slice() {
                                repository.commits.clone_from(entries);
                            }
                        } else {
                            repository.commits.extend(entries.clone());
                        }
                        *has_more
                    }
                    GitCatalogPage::Stashes(entries, has_more) => {
                        if output.offset == 0 {
                            if repository.stashes.as_slice() != entries.as_slice() {
                                repository.stashes.clone_from(entries);
                            }
                        } else {
                            repository.stashes.extend(entries.clone());
                        }
                        *has_more
                    }
                };
                let progress = state.snapshot.catalog_mut(output.catalog);
                progress.initialized = true;
                progress.loaded = true;
                progress.loading = false;
                progress.has_more = has_more;
                progress.failed = false;
            }
            Err(error) => {
                let progress = state.snapshot.catalog_mut(output.catalog);
                progress.loaded = false;
                progress.loading = false;
                progress.has_more = false;
                progress.failed = true;
                state.snapshot.message.clone_from(&error.message);
            }
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_diff_outputs(
    outputs: Query<(Entity, &GitJob, &DiffOutput)>,
    presented: Query<(), With<vmux_ecs::PagePresented>>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
        &mut super::state::GitDiffProjectionState,
    )>,
    mut files: super::status::FileGitStates,
    mut diffs: Query<&mut super::diff::GitDiffQuery>,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        let webview = job.webview;
        let Ok(mut query) = diffs.get_mut(webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        let succeeded = output.viewport.error.is_empty();
        let mut event = output.viewport.clone();
        if !query.accepts(event.generation) {
            commands.entity(entity).despawn();
            continue;
        }
        query.complete_load();
        let loaded_lines = u32::try_from(event.lines.len()).unwrap_or(u32::MAX);
        let branch_has_more = query.complete_branch(
            &output.branch_paths,
            output.branch_top_line,
            loaded_lines,
            output.branch_page_complete,
            succeeded,
            output.branch_lines.clone(),
        );
        if !output.branch_paths.is_empty() {
            event.has_more = branch_has_more;
            if !succeeded {
                event.total_lines = event.first_line;
            }
        }
        if let Ok((mut view, mut revealed, mut projection)) = views.get_mut(webview) {
            if view.snapshot.can_append_diff(&event) {
                let snapshot = &mut view.bypass_change_detection().snapshot;
                let append = snapshot
                    .append_diff_patch(event, &revealed.0)
                    .expect("appendability was checked");
                projection.capture(snapshot, &revealed.0);
                commands.trigger(vmux_ecs::UiStateWrite::<UiGit>::from_event(
                    webview, &append,
                ));
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
                    payload: crate::event::GitDiffNextRequest,
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
    )>,
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
        let Ok((mut state, controller)) = states.get_mut(job.webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        match &mut output.result {
            Ok(catalog) => {
                let base_ref = catalog.base_ref().to_string();
                let diff_paths = catalog.path_labels();
                let page = query.replace(std::mem::take(catalog));
                state
                    .snapshot
                    .branch_files_reference
                    .clone_from(&output.reference);
                state.snapshot.branch_files_base_ref = base_ref;
                state.snapshot.branch_diff_paths = diff_paths;
                state.snapshot.branch_files = page.files;
                state.snapshot.branch_files_total = page.total;
                state.snapshot.branch_files_has_more = page.has_more;
                state.snapshot.branch_files_error.clear();
            }
            Err(error) => {
                state.snapshot.branch_files_has_more = false;
                state.snapshot.branch_files_error = std::mem::take(&mut error.message);
                if controller.focused_panel == crate::state::GitPanel::Branches
                    && controller.selected_branch == output.reference
                {
                    state.snapshot.diff_loading = false;
                }
            }
        }
        state.snapshot.branch_files_loading = false;
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
        } else if files.contains(webview) {
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

fn deliver_failure_outputs(
    outputs: Query<(Entity, &GitJob, &FailureOutput)>,
    mut states: super::state::GitStates,
    mut files: super::status::FileGitStates,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        if states.contains(job.webview) {
            states.apply_error(job.webview, &output.0);
        } else if files.contains(job.webview) {
            files.apply_error(job.webview, output.0.message.clone());
        }
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct PendingGitJobs<'w, 's, J: Component> {
    queues: Query<'w, 's, (Entity, &'static GitJobs)>,
    pending: Query<'w, 's, &'static J>,
    repositories: Query<'w, 's, &'static GitRepository>,
    running: Query<'w, 's, (), With<GitJobRunning>>,
    reads: Query<'w, 's, (), With<super::job::GitReadJob>>,
    presented: Query<'w, 's, (), With<vmux_ecs::PagePresented>>,
    wake: Option<Res<'w, EventLoopProxyWrapper>>,
    runtime: Res<'w, GitRuntime>,
}

impl<J: Component + Clone> PendingGitJobs<'_, '_, J> {
    fn candidate(&self, owner: Entity, queue: &GitJobs) -> Option<Entity> {
        if queue.iter().any(|entity| self.running.contains(entity)) {
            return None;
        }
        for entity in queue.iter() {
            if self.reads.contains(entity) && !self.presented.contains(owner) {
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
        let Some(task) = jobs.spawn(wake, move || RepositoryOutput(loads.read(&job.path, epoch)))
        else {
            continue;
        };
        commands.entity(entity).insert((GitJobRunning, task));
    }
}

fn start_catalog_jobs(jobs: PendingGitJobs<CatalogJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let repo_root = repository.path().to_string_lossy().into_owned();
        let Some(task) = jobs.spawn(wake, move || CatalogOutput {
            catalog: job.catalog,
            repo_root,
            offset: job.offset,
            revision: job.revision,
            result: repository
                .catalog_page(job.catalog, &job.current_branch, job.offset, job.limit)
                .map_err(|error| GitOperationError { message: error.0 }),
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CatalogJob>()
            .insert((GitJobRunning, task));
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
        commands
            .entity(entity)
            .remove::<BranchFilesJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_diff_jobs(jobs: PendingGitJobs<DiffJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || {
            if !GitRepository::has_repository(repository.path()) {
                return DiffOutput {
                    viewport: GitDiffViewport {
                        generation: job.generation,
                        first_line: job.top_line,
                        total_lines: 0,
                        has_more: false,
                        lines: Vec::new(),
                        markers: Vec::new(),
                        error: String::new(),
                    },
                    branch_paths: job.branch_paths,
                    branch_lines: None,
                    branch_top_line: job.branch_top_line,
                    branch_page_complete: true,
                };
            }
            let result = match &job.scope {
                DiffScope::File => match job.content.as_deref() {
                    Some(content) => repository
                        .diff_lines_with_content(&job.path, content)
                        .map(Arc::new),
                    None => repository.diff_lines(&job.path).map(Arc::new),
                },
                DiffScope::Branch(reference) => match job.branch_lines.clone() {
                    Some(lines) => Ok(lines),
                    None if !job.branch_source.as_os_str().is_empty()
                        && !job.branch_merge_base.is_empty() =>
                    {
                        GitRepository::at(job.branch_source.clone())
                            .branch_diff_lines_for_paths_against(
                                reference,
                                &job.branch_merge_base,
                                &job.branch_paths,
                            )
                            .map(Arc::new)
                    }
                    None => repository
                        .branch_diff_lines_for_paths(reference, &job.branch_paths)
                        .map(Arc::new),
                },
                DiffScope::Commit(reference) => {
                    repository.commit_diff_lines(reference).map(Arc::new)
                }
                DiffScope::Stash(reference) => repository.stash_diff_lines(reference).map(Arc::new),
            };
            match result {
                Ok(all_lines) => {
                    let (first_line, total_lines, has_more, lines, branch_page_complete) =
                        if matches!(job.scope, DiffScope::Branch(_)) {
                            let (branch_total, lines) = super::parse::DiffParser::window(
                                &all_lines,
                                job.branch_top_line,
                                job.rows,
                            );
                            let page_lines = u32::try_from(lines.len()).unwrap_or(u32::MAX);
                            let branch_loaded = job.branch_top_line.saturating_add(page_lines);
                            let branch_page_complete = branch_loaded >= branch_total;
                            (
                                job.top_line,
                                job.top_line.saturating_add(page_lines),
                                !branch_page_complete || job.branch_has_more,
                                lines,
                                branch_page_complete,
                            )
                        } else {
                            let (total_lines, lines) = super::parse::DiffParser::window(
                                &all_lines,
                                job.top_line,
                                job.rows,
                            );
                            let loaded = job
                                .top_line
                                .saturating_add(u32::try_from(lines.len()).unwrap_or(u32::MAX));
                            (
                                job.top_line.min(total_lines),
                                total_lines,
                                loaded < total_lines,
                                lines,
                                true,
                            )
                        };
                    let markers = super::diff::GitDiffMarkers::from_lines(&lines).into_inner();
                    DiffOutput {
                        viewport: GitDiffViewport {
                            generation: job.generation,
                            first_line,
                            total_lines,
                            has_more,
                            lines,
                            markers,
                            error: String::new(),
                        },
                        branch_paths: job.branch_paths,
                        branch_lines: matches!(job.scope, DiffScope::Branch(_))
                            .then_some(all_lines),
                        branch_top_line: job.branch_top_line,
                        branch_page_complete,
                    }
                }
                Err(error) => DiffOutput {
                    viewport: GitDiffViewport {
                        generation: job.generation,
                        first_line: job.top_line,
                        total_lines: 0,
                        has_more: false,
                        lines: Vec::new(),
                        markers: Vec::new(),
                        error: error.0,
                    },
                    branch_paths: job.branch_paths,
                    branch_lines: None,
                    branch_top_line: job.branch_top_line,
                    branch_page_complete: false,
                },
            }
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<DiffJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stage_jobs(jobs: PendingGitJobs<StageJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || match repository.stage(&job.path) {
            Ok(()) => OperationOutput::success_with_status(&repository, &job.path, "stage", "ok"),
            Err(error) => OperationOutput::failure("stage", error),
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<StageJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_unstage_jobs(jobs: PendingGitJobs<UnstageJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || match repository.unstage(&job.path) {
            Ok(()) => OperationOutput::success_with_status(&repository, &job.path, "unstage", "ok"),
            Err(error) => OperationOutput::failure("unstage", error),
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<UnstageJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_discard_jobs(jobs: PendingGitJobs<DiscardJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn(wake, move || match repository.discard(&job.path) {
            Ok(()) => OperationOutput::success_with_status(&repository, &job.path, "discard", "ok"),
            Err(error) => OperationOutput::failure("discard", error),
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<DiscardJob>()
            .insert((GitJobRunning, task));
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
                    Err(error) => OperationOutput::failure("commit", error),
                },
                Err(error) => OperationOutput::failure("commit", error),
            })
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CommitJob>()
            .insert((GitJobRunning, task));
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
                    Err(error) => OperationOutput::failure("fetch", error),
                },
                Err(error) => OperationOutput::failure("fetch", error),
            })
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<FetchJob>()
            .insert((GitJobRunning, task));
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
                    Err(error) => OperationOutput::failure("pull", error),
                },
                Err(error) => OperationOutput::failure("pull", error),
            })
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<PullJob>()
            .insert((GitJobRunning, task));
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
                    Err(error) => OperationOutput::failure("push", error),
                },
                Err(error) => OperationOutput::failure("push", error),
            })
        else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<PushJob>()
            .insert((GitJobRunning, task));
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
                Err(error) => OperationOutput::failure("stage all", error),
            },
            Err(error) => OperationOutput::failure("stage all", error),
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<StageAllJob>()
            .insert((GitJobRunning, task));
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
                Err(error) => OperationOutput::failure("hunk", error),
            }
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<HunkJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_amend_jobs(jobs: PendingGitJobs<AmendJob>, mut commands: Commands) {
    for (entity, _, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("amend", repository.amend())
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<AmendJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_checkout_commit_jobs(jobs: PendingGitJobs<CheckoutCommitJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("checkout commit", repository.checkout_commit(&job.commit))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CheckoutCommitJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_cherry_pick_jobs(jobs: PendingGitJobs<CherryPickJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("cherry-pick", repository.cherry_pick(&job.commit))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CherryPickJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_create_branch_jobs(jobs: PendingGitJobs<CreateBranchJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result(
                "new branch",
                repository.create_branch(&job.branch, &job.start_point),
            )
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<CreateBranchJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_delete_branch_jobs(jobs: PendingGitJobs<DeleteBranchJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("delete branch", repository.delete_branch(&job.branch))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<DeleteBranchJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_fast_forward_jobs(jobs: PendingGitJobs<FastForwardJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("fast-forward", repository.fast_forward(&job.branch))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<FastForwardJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_merge_jobs(jobs: PendingGitJobs<MergeJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("merge", repository.merge(&job.branch))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<MergeJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_rebase_jobs(jobs: PendingGitJobs<RebaseJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("rebase", repository.rebase(&job.branch))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<RebaseJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_revert_jobs(jobs: PendingGitJobs<RevertJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("revert", repository.revert(&job.commit))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<RevertJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stash_drop_jobs(jobs: PendingGitJobs<StashDropJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("stash drop", repository.stash_drop(&job.reference))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<StashDropJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stash_pop_jobs(jobs: PendingGitJobs<StashPopJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("stash pop", repository.stash_pop(&job.reference))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<StashPopJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stash_push_jobs(jobs: PendingGitJobs<StashPushJob>, mut commands: Commands) {
    for (entity, _, repository, wake) in jobs.ready_with_repository() {
        let Some(task) = jobs.spawn_operation(wake, move || {
            OperationOutput::from_result("stash", repository.stash_push())
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<StashPushJob>()
            .insert((GitJobRunning, task));
    }
}

#[cfg(test)]
mod tests {
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
        let job = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: "/tmp/repository".into(),
                    refresh_catalogs: false,
                    revision: 1,
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
                branch_lines: None,
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
                branch_lines: None,
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
