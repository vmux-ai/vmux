use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy::{ecs::system::SystemParam, prelude::*};

use super::GitUpdateSet;
use super::job::{
    AmendJob, BranchFilesJob, CatalogJob, CheckoutCommitJob, CherryPickJob, CommitJob,
    CreateBranchJob, DeleteBranchJob, DiffJob, DiffScope, DiscardJob, FastForwardJob, FetchJob,
    HunkJob, MergeJob, PullJob, PushJob, RebaseJob, RepositoryJob, RevertJob, StageAllJob,
    StageJob, StashDropJob, StashPopJob, StashPushJob, UnstageJob,
};
use super::read::{
    PageRepositorySource, ReadPlugin, ReadSet, RepositoryReadRequest, RepositoryRequestIdentity,
    SourceIdentity,
};
use super::repository::{
    BranchFilesCatalog, DiffSource, GitCatalogPage, GitCatalogSnapshot, GitError, GitRepository,
    RepositorySummary,
};
use super::runtime::{GitRuntime, GitTask};
#[cfg(test)]
use crate::event::GitRepositorySnapshot;
use crate::event::{
    DiffKind, DiffLine, GitCatalog, GitDiffViewport, GitFileStatus, GitLineMarker, GitLineStatus,
    GitOperationError, GitOperationResult,
};
use crate::state::{
    GitBranchFilesPagePatch, GitBranchFilesPatch, GitCatalogEntries, GitCatalogPatch, GitUiState,
};
use vmux_ecs::{
    Operation, OperationCompleted, OperationPlugin, OperationRetained, OperationState,
    OperationTarget,
};
use vmux_path::PathIdentity;

pub(super) struct JobPlugin;

impl Plugin for JobPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OperationPlugin>() {
            app.add_plugins(OperationPlugin);
        }
        app.add_observer(accept)
            .add_observer(running)
            .add_observer(cancel)
            .init_resource::<GitRuntime>()
            .add_plugins(ReadPlugin)
            .add_systems(PostUpdate, (finish_retired_jobs, evict_catalog_snapshots))
            .add_systems(
                Update,
                (
                    cancel_orphaned_jobs.in_set(RetirementSet),
                    bevy::ecs::schedule::ApplyDeferred,
                    (
                        poll_jobs::<RepositoryOutput>,
                        poll_jobs::<CatalogOutput>,
                        poll_jobs::<CatalogSnapshotOutput>,
                        poll_jobs::<BranchFilesOutput>,
                        poll_jobs::<DiffOutput>,
                        poll_jobs::<OperationOutput>,
                    )
                        .after(ReadSet),
                    (
                        deliver_repository_outputs,
                        (materialize_catalog_snapshots, deliver_catalog_outputs).chain(),
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
pub(super) struct GitJobRunning;

#[derive(Component)]
struct GitJobRetired;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct RetirementSet;

type AcceptedJobs<'w, 's> = Query<
    'w,
    's,
    (
        &'static GitJob,
        Has<super::job::GitReadJob>,
        Option<&'static RepositoryJob>,
        Has<RepositoryRequestIdentity>,
    ),
>;

fn accept(trigger: On<Add, GitJob>, jobs: AcceptedJobs, mut commands: Commands) {
    let entity = trigger.event_target();
    let Ok((job, read, repository, identified)) = jobs.get(entity) else {
        return;
    };
    if read {
        if !identified && let Some(repository) = repository {
            commands
                .entity(entity)
                .insert(RepositoryRequestIdentity::capture(&repository.path));
        }
        return;
    }
    commands.entity(entity).insert((
        Operation::bundle("git.change").unwrap(),
        OperationTarget::new(job.webview),
    ));
}

fn running(
    trigger: On<Add, GitJobRunning>,
    operations: Query<(), With<Operation>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if operations.contains(entity) {
        commands
            .entity(entity)
            .insert((OperationState::Running, OperationRetained));
    }
}

fn cancel(
    trigger: On<Remove, GitJob>,
    jobs: Query<(Option<&OperationState>, Has<GitJobRunning>)>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok((state, running)) = jobs.get(entity) else {
        return;
    };
    if state
        .is_some_and(|state| !matches!(state, OperationState::Pending | OperationState::Running))
    {
        return;
    }
    let mut entity = commands.entity(entity);
    if running {
        entity.try_insert(GitJobRetired);
    }
    if state.is_some() {
        entity.try_insert(OperationState::Canceled);
    }
}

#[derive(SystemParam)]
pub(super) struct QueuedGitJobs<'w, 's, J: Component> {
    queues: Query<'w, 's, &'static GitJobs>,
    matching: Query<'w, 's, (Has<GitJobRunning>, Has<Operation>), With<J>>,
}

impl<J: Component> QueuedGitJobs<'_, '_, J> {
    pub(super) fn cancel_for(&self, webview: Entity, commands: &mut Commands) {
        let Ok(queue) = self.queues.get(webview) else {
            return;
        };
        for entity in queue.iter() {
            let Ok((running, operation)) = self.matching.get(entity) else {
                continue;
            };
            if running || operation {
                commands.entity(entity).remove::<GitJob>();
            } else {
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
pub(super) struct RepositoryOutput(pub(super) Result<Arc<RepositorySummary>, GitOperationError>);

#[derive(Component)]
struct CatalogSnapshot {
    owner: Entity,
    source: SourceIdentity,
    catalog: GitCatalog,
    branch: String,
    revision: u32,
    snapshot: Arc<GitCatalogSnapshot>,
    touched: Instant,
}

impl CatalogSnapshot {
    const CAPACITY: usize = 128;

    fn matches(&self, owner: Entity, repository: &GitRepository, job: &CatalogJob) -> bool {
        self.owner == owner
            && self.source.matches(repository.path())
            && self.catalog == job.catalog
            && self.branch == job.current_branch
            && self.revision == job.revision
    }
}

#[derive(Component)]
struct CatalogReadSource(SourceIdentity);

#[derive(Component)]
struct CatalogSnapshotOutput(Result<Arc<GitCatalogSnapshot>, GitOperationError>);

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

    fn read(job: DiffJob, repository: &GitRepository) -> Self {
        if !GitRepository::has_repository(repository.path()) {
            return Self {
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
            None => Self::load_source(&job, repository),
        };
        let source = match result {
            Ok(source) => source,
            Err(error) => return Self::failure(&job, error),
        };
        let window_top = if matches!(job.scope, DiffScope::Branch(_)) {
            job.branch_top_line
        } else {
            job.top_line
        };
        let (source_total, lines) = match source.window(repository, window_top, job.rows) {
            Ok(window) => window,
            Err(error) => return Self::failure(&job, error),
        };
        let (first_line, total_lines, has_more, branch_page_complete) =
            Self::window_bounds(&job, source_total, &lines);
        let markers = if matches!(job.scope, DiffScope::File) {
            Self::markers_from_lines(&lines)
        } else {
            Vec::new()
        };
        Self {
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

    fn load_source(job: &DiffJob, repository: &GitRepository) -> Result<DiffSource, GitError> {
        match &job.scope {
            DiffScope::File => match job.content.as_deref() {
                Some(content) => repository.diff_source_with_content(&job.path, content),
                None => repository.diff_source(&job.path),
            },
            DiffScope::Branch(reference) => {
                if !job.branch_source.as_os_str().is_empty() && !job.branch_merge_base.is_empty() {
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
        }
    }

    fn window_bounds(
        job: &DiffJob,
        source_total: u32,
        lines: &[DiffLine],
    ) -> (u32, u32, bool, bool) {
        let count = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        if matches!(job.scope, DiffScope::Branch(_)) {
            let branch_loaded = job.branch_top_line.saturating_add(count);
            let branch_page_complete = branch_loaded >= source_total;
            return (
                job.top_line,
                job.top_line.saturating_add(count),
                !branch_page_complete || job.branch_has_more,
                branch_page_complete,
            );
        }
        let loaded = job.top_line.saturating_add(count);
        (
            job.top_line.min(source_total),
            source_total,
            loaded < source_total,
            true,
        )
    }

    fn markers_from_lines(lines: &[DiffLine]) -> Vec<GitLineMarker> {
        let mut hunk_kinds = HashMap::<u32, (bool, bool)>::new();
        for line in lines {
            let Some(hunk) = line.hunk else {
                continue;
            };
            let kinds = hunk_kinds.entry(hunk).or_default();
            match line.kind {
                DiffKind::Add => kinds.0 = true,
                DiffKind::Remove => kinds.1 = true,
                _ => {}
            }
        }
        let replacements = Self::replacement_lines(lines);
        let mut markers = HashMap::new();
        for (index, line) in lines.iter().enumerate() {
            match line.kind {
                DiffKind::Add => {
                    let Some(line_number) = line.new_no else {
                        continue;
                    };
                    let modified = line
                        .hunk
                        .and_then(|hunk| hunk_kinds.get(&hunk))
                        .is_some_and(|(added, removed)| *added && *removed)
                        || replacements.contains(&index);
                    let status = if modified {
                        GitLineStatus::Modified
                    } else {
                        GitLineStatus::Added
                    };
                    Self::insert_marker(&mut markers, line_number, status);
                }
                DiffKind::Remove => {
                    let next = lines[index + 1..].iter().find_map(|next| next.new_no);
                    let previous = lines[..index]
                        .iter()
                        .rev()
                        .find_map(|previous| previous.new_no);
                    if let Some(line_number) = next.or(previous) {
                        Self::insert_marker(&mut markers, line_number, GitLineStatus::Deleted);
                    }
                }
                DiffKind::Staged => {
                    if let Some(line_number) = line.new_no {
                        Self::insert_marker(&mut markers, line_number, GitLineStatus::Staged);
                    }
                }
                DiffKind::Context | DiffKind::Hunk => {}
            }
        }
        let mut markers = markers
            .into_iter()
            .map(|(line, status)| GitLineMarker { line, status })
            .collect::<Vec<_>>();
        markers.sort_by_key(|marker| marker.line);
        markers
    }

    fn replacement_lines(lines: &[DiffLine]) -> HashSet<usize> {
        let mut replacements = HashSet::new();
        let mut index = 0;
        while index < lines.len() {
            if matches!(lines[index].kind, DiffKind::Context | DiffKind::Staged)
                || lines[index].hunk.is_some()
            {
                index += 1;
                continue;
            }
            let start = index;
            while index < lines.len()
                && !matches!(lines[index].kind, DiffKind::Context | DiffKind::Staged)
                && lines[index].hunk.is_none()
            {
                index += 1;
            }
            let mut has_add = false;
            let mut has_remove = false;
            for line in &lines[start..index] {
                match line.kind {
                    DiffKind::Add => has_add = true,
                    DiffKind::Remove => has_remove = true,
                    _ => {}
                }
            }
            if has_add && has_remove {
                replacements.extend(start..index);
            }
        }
        replacements
    }

    fn insert_marker(markers: &mut HashMap<u32, GitLineStatus>, line: u32, status: GitLineStatus) {
        let priority = |status| match status {
            GitLineStatus::Staged => 0,
            GitLineStatus::Deleted => 1,
            GitLineStatus::Added => 2,
            GitLineStatus::Modified => 3,
        };
        match markers.get(&line) {
            Some(current) if priority(*current) >= priority(status) => {}
            _ => {
                markers.insert(line, status);
            }
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

type PollingJobs<'w, 's, T> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut GitJobTask<T>,
        Has<GitJobRetired>,
        Has<Operation>,
    ),
>;

fn poll_jobs<T: Component>(mut jobs: PollingJobs<T>, mut commands: Commands) {
    for (entity, mut task, retired, operation) in &mut jobs {
        if retired {
            task.task.cancel();
        }
        if !task.task.is_finished() {
            continue;
        }
        if retired {
            if operation {
                commands.entity(entity).remove::<(
                    GitJobRunning,
                    GitJobTask<T>,
                    GitJobRetired,
                    GitRepository,
                    OperationRetained,
                )>();
            } else {
                commands.entity(entity).despawn();
            }
            continue;
        }
        let output = task.task.poll();
        let mut entity = commands.entity(entity);
        entity.remove::<(GitJobRunning, GitJobTask<T>, OperationRetained)>();
        match output {
            Some(Ok(output)) => {
                entity.insert(output);
            }
            _ => {
                entity.insert(FailureOutput(GitOperationError {
                    message: "Git job worker panicked".to_string(),
                }));
            }
        }
    }
}

type DrainedJobs<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<GitJobRetired>,
        Without<GitJobRunning>,
        Without<Operation>,
    ),
>;

fn finish_retired_jobs(jobs: DrainedJobs, mut commands: Commands) {
    for entity in &jobs {
        commands.entity(entity).despawn();
    }
}

fn cancel_orphaned_jobs(
    jobs: Query<(Entity, &GitJob, Option<&RepositoryJob>, Has<GitJobRunning>)>,
    owners: Query<(
        Option<&super::state::GitState>,
        Option<&super::status::FileGit>,
    )>,
    load_revisions: Query<&super::state::GitRepositoryLoadRevision>,
    operations: Query<(), With<Operation>>,
    mut commands: Commands,
) {
    for (entity, job, repository, running) in &jobs {
        let Ok((state, file)) = owners.get(job.webview) else {
            if running || operations.contains(entity) {
                commands.entity(entity).remove::<GitJob>();
            } else {
                commands.entity(entity).despawn();
            }
            continue;
        };
        if state.is_none() && file.is_none() {
            if running || operations.contains(entity) {
                commands.entity(entity).remove::<GitJob>();
            } else {
                commands.entity(entity).despawn();
            }
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
            if running {
                commands.entity(entity).remove::<GitJob>();
            } else {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn deliver_repository_outputs(
    outputs: Query<(
        Entity,
        &GitJob,
        &RepositoryJob,
        &RepositoryOutput,
        Option<&PageRepositorySource>,
    )>,
    sources: Query<&PageRepositorySource>,
    presented: Query<(), With<vmux_ecs::PagePresented>>,
    mut watch: Option<NonSendMut<super::watch::GitWatch>>,
    mut pages: Query<&mut vmux_ecs::PageMetadata>,
    mut states: super::state::GitStates,
    mut commands: Commands,
) {
    for (entity, owner, request, output, source) in &outputs {
        let webview = owner.webview;
        if !states.repository_requested_at(webview, &request.path, request.revision) {
            commands.entity(entity).despawn();
            continue;
        }
        match &output.0 {
            Ok(event) => {
                if let Some(source) = source {
                    let mut source = source.clone();
                    if let Ok(previous) = sources.get(webview) {
                        source.preserve_request(previous);
                    }
                    commands.entity(webview).insert(source);
                }
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
                    commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                        webview, &patch,
                    ));
                }
            }
            Err(error) => {
                states.fail_repository(webview, error);
            }
        }
        commands.entity(entity).despawn();
    }
}

impl CatalogOutput {
    fn take_patch(&mut self, current: crate::state::GitCatalogProgress) -> GitCatalogPatch {
        let catalog = self.catalog;
        let offset = u32::try_from(self.offset).unwrap_or(u32::MAX);
        match &mut self.result {
            Ok(page) => {
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
                    initialized: current.initialized,
                    loaded: false,
                    loading: false,
                    has_more: false,
                    failed: true,
                },
                message: Some(error.message.clone()),
            },
        }
    }
}

fn deliver_catalog_outputs(
    mut outputs: Query<(Entity, &GitJob, &mut CatalogOutput)>,
    mut states: Query<(
        &mut super::state::GitState,
        &mut super::state::GitCatalogRevision,
    )>,
    mut commands: Commands,
) {
    for (entity, job, mut output) in &mut outputs {
        commands.entity(entity).despawn();
        let Ok((mut state, mut revision)) = states.get_mut(job.webview) else {
            continue;
        };
        if state.snapshot.workspace != output.repo_root {
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
                let patch = GitCatalogPatch {
                    catalog,
                    offset,
                    entries: None,
                    progress,
                    message: None,
                };
                commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                    job.webview,
                    &patch,
                ));
            }
            continue;
        }
        if !revision.finish(catalog, output.generation) {
            continue;
        }
        if output.result.is_ok() && state.snapshot.repository.is_none() {
            continue;
        }
        let patch = output.take_patch(state.snapshot.catalog(catalog));
        state
            .bypass_change_detection()
            .snapshot
            .apply_catalog_patch(&patch);
        commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
            job.webview,
            &patch,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn deliver_diff_outputs(
    mut outputs: Query<(Entity, &GitJob, &mut DiffOutput)>,
    presented: Query<Has<super::diff::PendingGitDiffSeek>, With<vmux_ecs::PagePresented>>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
        &mut super::state::GitDiffProjectionState,
    )>,
    mut files: super::status::FileGitStates,
    mut diffs: Query<(
        &mut super::diff::GitDiffQuery,
        Option<&mut super::diff::PrefetchBudget>,
    )>,
    mut branch_diffs: super::diff::BranchDiffs,
    mut commands: Commands,
) {
    for (entity, job, mut output) in &mut outputs {
        let webview = job.webview;
        let Ok((mut query, budget)) = diffs.get_mut(webview) else {
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
        let branch_has_more = branch_diffs.complete(
            webview,
            &output.branch_paths,
            output.branch_top_line,
            loaded_lines,
            output.branch_page_complete,
            succeeded,
            output.source.clone(),
            &mut commands,
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
                commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
                    webview, &append,
                ));
            } else {
                view.snapshot.diff_loading = false;
                view.snapshot.diff_viewport = Some(event);
                revealed.0.clear();
            }
            if presented.get(webview).is_ok_and(|seeking| !seeking)
                && branch_has_more
                && succeeded
                && budget.is_some_and(|mut budget| budget.continue_after(loaded_lines))
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
        commands.trigger(vmux_ecs::UiStateWrite::<GitUiState>::from_event(
            job.webview,
            &patch,
        ));
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
        let result = if output.result.ok {
            Ok(())
        } else {
            Err(output.result.message.clone())
        };
        commands.trigger(OperationCompleted { entity, result });
        commands
            .entity(entity)
            .remove::<(GitJob, OperationOutput, GitRepository)>();
        let webview = job.webview;
        if states.contains(webview) {
            if !states.repository_matches(webview, &output.repository) {
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
                    RepositoryRequestIdentity::capture(Path::new(&workspace)),
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
    operations: Query<(), With<Operation>>,
    mut commands: Commands,
) {
    for (entity, job, output, repository, catalog, branch_files, diff, authority) in &outputs {
        let operation = operations.contains(entity);
        if operation {
            commands.trigger(OperationCompleted {
                entity,
                result: Err(output.0.message.clone()),
            });
            commands
                .entity(entity)
                .remove::<(GitJob, FailureOutput, GitRepository)>();
        }
        if let Some(repository) = repository {
            if states.repository_requested_at(job.webview, &repository.path, repository.revision) {
                states.fail_repository(job.webview, &output.0);
            }
        } else if states.contains(job.webview) {
            let current = authority.is_some_and(|repository| {
                states.repository_matches(job.webview, repository.path())
            });
            if !current {
                if !operation {
                    commands.entity(entity).despawn();
                }
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
                if !operation {
                    commands.entity(entity).despawn();
                }
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
        if !operation {
            commands.entity(entity).despawn();
        }
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

fn start_repository_jobs(jobs: PendingGitJobs<RepositoryJob>, mut commands: Commands) {
    for (entity, _, _) in jobs.ready() {
        commands.trigger(RepositoryReadRequest { entity });
    }
}

fn start_catalog_jobs(
    jobs: PendingGitJobs<CatalogJob>,
    snapshots: Query<&CatalogSnapshot>,
    mut commands: Commands,
) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let Some(owner) = jobs.owner(entity) else {
            continue;
        };
        let repo_root = repository.path().to_string_lossy().into_owned();
        let mut cached = None;
        for snapshot in &snapshots {
            if snapshot.matches(owner, &repository, &job) {
                cached = Some(Arc::clone(&snapshot.snapshot));
                break;
            }
        }
        if let Some(snapshot) = cached {
            commands.entity(entity).insert((
                GitJobRunning,
                CatalogOutput {
                    catalog: job.catalog,
                    repo_root,
                    offset: job.offset,
                    revision: job.revision,
                    generation: job.generation,
                    result: Ok(snapshot.page(job.offset, job.limit)),
                },
            ));
            continue;
        }
        if job.catalog == GitCatalog::Commits {
            let Some(task) = jobs.spawn(wake, move || CatalogOutput {
                catalog: job.catalog,
                repo_root,
                offset: job.offset,
                revision: job.revision,
                generation: job.generation,
                result: repository
                    .catalog_page(job.catalog, &job.current_branch, job.offset, job.limit)
                    .map_err(|error| GitOperationError { message: error.0 }),
            }) else {
                continue;
            };
            commands.entity(entity).insert((GitJobRunning, task));
            continue;
        }
        let source = SourceIdentity::capture(repository.path());
        let Some(task) = jobs.spawn(wake, move || {
            let result = repository
                .catalog_snapshot(job.catalog, &job.current_branch)
                .map_err(|error| GitOperationError { message: error.0 })
                .and_then(|snapshot| {
                    snapshot.ok_or_else(|| GitOperationError {
                        message: "catalog does not support snapshots".into(),
                    })
                });
            CatalogSnapshotOutput(result.map(Arc::new))
        }) else {
            continue;
        };
        commands
            .entity(entity)
            .insert((GitJobRunning, CatalogReadSource(source), task));
    }
}

fn materialize_catalog_snapshots(
    outputs: Query<(
        Entity,
        &GitJob,
        &CatalogJob,
        &GitRepository,
        &CatalogReadSource,
        &CatalogSnapshotOutput,
    )>,
    states: Query<(&super::state::GitState, &super::state::GitCatalogRevision)>,
    snapshots: Query<(Entity, &CatalogSnapshot)>,
    mut commands: Commands,
) {
    for (entity, owner, job, repository, source, output) in &outputs {
        let valid = source.0.matches(repository.path());
        let result = match &output.0 {
            Ok(snapshot) if valid => {
                if states.get(owner.webview).is_ok_and(|(state, revision)| {
                    state.snapshot.nonce == job.revision
                        && PathIdentity::resolve(&state.snapshot.workspace)
                            == PathIdentity::resolve(repository.path())
                        && revision.accepts(job.catalog, job.generation)
                }) {
                    for (previous, snapshot) in &snapshots {
                        if snapshot.owner == owner.webview && snapshot.catalog == job.catalog {
                            commands.entity(previous).despawn();
                        }
                    }
                    commands.spawn((
                        Name::new("git.catalog.snapshot"),
                        CatalogSnapshot {
                            owner: owner.webview,
                            source: source.0.clone(),
                            catalog: job.catalog,
                            branch: job.current_branch.clone(),
                            revision: job.revision,
                            snapshot: Arc::clone(snapshot),
                            touched: Instant::now(),
                        },
                    ));
                }
                Ok(snapshot.page(job.offset, job.limit))
            }
            Ok(_) => Err(GitOperationError {
                message: "Git repository source changed".into(),
            }),
            Err(error) => Err(error.clone()),
        };
        commands
            .entity(entity)
            .remove::<CatalogSnapshotOutput>()
            .insert(CatalogOutput {
                catalog: job.catalog,
                repo_root: repository.path().to_string_lossy().into_owned(),
                offset: job.offset,
                revision: job.revision,
                generation: job.generation,
                result,
            });
    }
}

fn evict_catalog_snapshots(
    snapshots: Query<(Entity, &CatalogSnapshot)>,
    pages: Query<&super::state::GitState>,
    mut commands: Commands,
) {
    let mut retained = Vec::new();
    for (entity, snapshot) in &snapshots {
        if !pages
            .get(snapshot.owner)
            .is_ok_and(|page| page.snapshot.nonce == snapshot.revision)
        {
            commands.entity(entity).despawn();
            continue;
        }
        retained.push((snapshot.touched, entity));
    }
    retained.sort_by_key(|(touched, _)| *touched);
    let excess = retained.len().saturating_sub(CatalogSnapshot::CAPACITY);
    for (_, entity) in retained.into_iter().take(excess) {
        commands.entity(entity).despawn();
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
        let Some(task) = jobs.spawn(wake, move || DiffOutput::read(job, &repository)) else {
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
            .add_plugins(vmux_ecs::UiStatePlugin::<GitUiState>::default())
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
        app.add_plugins(JobPlugin);
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

        assert!(app.world().get::<GitJob>(job).is_none());
        assert_eq!(
            app.world().get::<OperationState>(job),
            Some(&OperationState::Succeeded)
        );
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
        app.add_plugins(JobPlugin);
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
        let operation = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                GitRepository::at(first.path()),
                FailureOutput(GitOperationError {
                    message: "worker failed".to_string(),
                }),
            ))
            .id();

        app.update();

        let state = app
            .world()
            .get::<super::super::state::GitState>(webview)
            .unwrap();
        assert!(state.snapshot.message.is_empty());
        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Failed)
        );
        assert_eq!(
            app.world()
                .get::<vmux_ecs::OperationError>(operation)
                .unwrap()
                .message(),
            "worker failed"
        );
        assert!(app.world().get::<GitJob>(operation).is_none());
    }

    #[test]
    fn removing_a_git_owner_retains_its_canceled_operation() {
        let mut app = App::new();
        app.add_plugins(JobPlugin);
        let owner = app.world_mut().spawn_empty().id();
        let operation = app.world_mut().spawn(GitJob::new(owner)).id();
        app.world_mut().entity_mut(owner).despawn();

        app.update();

        assert_eq!(
            app.world().get::<OperationState>(operation),
            Some(&OperationState::Canceled)
        );
        assert!(app.world().get::<GitJob>(operation).is_none());
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

    #[test]
    fn markers_classify_modified_added_and_deleted_lines() {
        let context = |no| DiffLine {
            kind: DiffKind::Context,
            old_no: Some(no),
            new_no: Some(no),
            hunk: None,
            spans: Vec::new(),
        };
        let lines = vec![
            DiffLine {
                kind: DiffKind::Remove,
                old_no: Some(2),
                new_no: None,
                hunk: None,
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(2),
                hunk: None,
                spans: Vec::new(),
            },
            context(3),
            DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(8),
                hunk: Some(1),
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Remove,
                old_no: Some(12),
                new_no: None,
                hunk: Some(2),
                spans: Vec::new(),
            },
            context(12),
        ];

        let markers = DiffOutput::markers_from_lines(&lines);

        assert!(markers.contains(&GitLineMarker {
            line: 2,
            status: GitLineStatus::Modified,
        }));
        assert!(markers.contains(&GitLineMarker {
            line: 8,
            status: GitLineStatus::Added,
        }));
        assert!(markers.contains(&GitLineMarker {
            line: 12,
            status: GitLineStatus::Deleted,
        }));
    }
}
