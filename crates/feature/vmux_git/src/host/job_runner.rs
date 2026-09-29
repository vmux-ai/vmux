use std::path::Path;
use std::thread::JoinHandle;

use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy::{ecs::system::SystemParam, prelude::*};

use crate::event::{
    GitBranchLog, GitDiffViewport, GitFileStatus, GitOperationError, GitOperationResult,
    GitRepositorySnapshot,
};

use super::GitUpdateSet;
use super::job::{
    AmendJob, BranchLogJob, CheckoutCommitJob, CherryPickJob, CommitJob, CreateBranchJob,
    DeleteBranchJob, DiffJob, DiscardJob, FastForwardJob, FetchJob, HunkJob, MergeJob, PullJob,
    PushJob, RebaseJob, RepositoryJob, RevertJob, StageAllJob, StageJob, StashDropJob, StashPopJob,
    StashPushJob, UnstageJob,
};
use super::repository::GitRepository;

pub(super) struct JobPlugin;

impl Plugin for JobPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(queue_git_job_failure).add_systems(
            Update,
            (
                (
                    poll_git_jobs::<RepositoryOutput>,
                    poll_git_jobs::<BranchLogOutput>,
                    poll_git_jobs::<DiffOutput>,
                    poll_git_jobs::<OperationOutput>,
                ),
                (
                    deliver_repository_outputs,
                    deliver_branch_log_outputs,
                    deliver_diff_outputs,
                    deliver_operation_outputs,
                    deliver_failure_outputs,
                ),
                bevy::ecs::schedule::ApplyDeferred,
                (
                    start_repository_jobs,
                    start_branch_log_jobs,
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

#[derive(Component)]
struct GitJobTask<T: Send + Sync + 'static> {
    thread: Option<JoinHandle<T>>,
}

impl<T: Send + Sync + 'static> GitJobTask<T> {
    fn spawn(
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Self {
        let thread = std::thread::spawn(move || {
            let output = run();
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            output
        });
        Self {
            thread: Some(thread),
        }
    }
}

#[derive(Component)]
struct RepositoryOutput(Result<GitRepositorySnapshot, GitOperationError>);

#[derive(Component)]
struct BranchLogOutput(Result<GitBranchLog, GitOperationError>);

#[derive(Component)]
struct DiffOutput(GitDiffViewport);

#[derive(Component, Clone, Debug)]
struct OperationOutput {
    result: GitOperationResult,
    status: Option<Result<GitFileStatus, GitOperationError>>,
}

#[derive(Component)]
struct FailureOutput(GitOperationError);

fn queue_git_job_failure(trigger: On<GitJobFailure>, mut commands: Commands) {
    commands.spawn((
        GitJob::new(trigger.event().webview),
        FailureOutput(GitOperationError {
            message: trigger.event().message.clone(),
        }),
    ));
}

fn poll_git_jobs<T: Component>(
    mut jobs: Query<(Entity, &mut GitJobTask<T>)>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut jobs {
        if !task.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.remove::<(GitJobRunning, GitJobTask<T>)>();
        match task.thread.take().unwrap().join() {
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

fn deliver_repository_outputs(
    outputs: Query<(Entity, &GitJob, &RepositoryOutput)>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::controller::GitController,
    )>,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        let webview = job.webview;
        match &output.0 {
            Ok(event) => {
                if let Ok(mut page) = pages.get_mut(webview) {
                    if let Some(url) = crate::GitUrl::from_path(Path::new(&event.repo_root)) {
                        page.url = url;
                    }
                    page.title = match event.branch.is_empty() {
                        true => event.repo_name.clone(),
                        false => format!("{} · {}", event.repo_name, event.branch),
                    };
                }
                if let Ok((mut view, mut controller)) = views.get_mut(webview) {
                    controller.reconcile_repository(event);
                    view.set_repository(event.clone());
                    if let Some(payload) = controller.branch_log_request(&view) {
                        commands.trigger(bevy_cef::prelude::UiInput { webview, payload });
                    }
                }
            }
            Err(error) => {
                if let Ok((mut view, _)) = views.get_mut(webview) {
                    view.apply_error(error);
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_branch_log_outputs(
    outputs: Query<(Entity, &GitJob, &BranchLogOutput)>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::controller::GitController,
    )>,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        if let Ok((mut view, _)) = views.get_mut(job.webview) {
            match &output.0 {
                Ok(event) => view.set_branch_log(event.clone()),
                Err(error) => view.apply_error(error),
            }
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_diff_outputs(
    outputs: Query<(Entity, &GitJob, &DiffOutput)>,
    mut views: Query<&mut super::state::GitState>,
    mut files: Query<&mut super::status::FileGit>,
    diffs: Query<&super::diff::GitDiffQuery>,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        let webview = job.webview;
        let event = &output.0;
        let Ok(query) = diffs.get(webview) else {
            commands.entity(entity).despawn();
            continue;
        };
        if let Ok(mut view) = views.get_mut(webview) {
            if query.accepts(event.generation) {
                view.set_diff_viewport(event.clone());
            }
            commands.entity(entity).despawn();
            continue;
        }
        if let Ok(mut file) = files.get_mut(webview)
            && query.accepts_file(event.generation, &file)
        {
            file.apply_diff(event.clone());
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_operation_outputs(
    outputs: Query<(Entity, &GitJob, &OperationOutput)>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::controller::GitController,
    )>,
    mut files: Query<&mut super::status::FileGit>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, job, output) in &outputs {
        let webview = job.webview;
        if let Ok((mut view, mut controller)) = views.get_mut(webview) {
            let branch = controller.apply_result(&output.result);
            view.apply_result(&output.result);
            if let Some(branch) = branch {
                commands.trigger(bevy_cef::prelude::UiInput {
                    webview,
                    payload: vmux_core::event::space::ProjectActivateRequest {
                        path: view.workspace().to_string(),
                        branch,
                        checkout: String::new(),
                        pane_id: None,
                    },
                });
            }
            if let Some(Err(error)) = &output.status {
                view.apply_error(error);
            }
            if !view.workspace().is_empty() {
                commands.spawn((
                    GitJob::new(webview),
                    RepositoryJob {
                        path: view.workspace().into(),
                    },
                ));
            }
        } else if let Ok(mut file) = files.get_mut(webview) {
            let refresh = file.apply_result(output.result.clone(), wake.clone());
            match &output.status {
                Some(Ok(status)) => file.apply_status(status.clone()),
                Some(Err(error)) => file.apply_error(error.message.clone()),
                None => {}
            }
            commands.entity(webview).insert(refresh);
        }
        commands.entity(entity).despawn();
    }
}

fn deliver_failure_outputs(
    outputs: Query<(Entity, &GitJob, &FailureOutput)>,
    mut views: Query<&mut super::state::GitState>,
    mut files: Query<&mut super::status::FileGit>,
    mut commands: Commands,
) {
    for (entity, job, output) in &outputs {
        if let Ok(mut view) = views.get_mut(job.webview) {
            view.apply_error(&output.0);
        } else if let Ok(mut file) = files.get_mut(job.webview) {
            file.apply_error(output.0.message.clone());
        }
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct PendingGitJobs<'w, 's, J: Component> {
    queues: Query<'w, 's, &'static GitJobs>,
    pending: Query<'w, 's, &'static J>,
    repositories: Query<'w, 's, &'static GitRepository>,
    running: Query<'w, 's, (), With<GitJobRunning>>,
    wake: Option<Res<'w, EventLoopProxyWrapper>>,
}

impl<J: Component + Clone> PendingGitJobs<'_, '_, J> {
    fn ready(
        &self,
    ) -> Vec<(
        Entity,
        J,
        Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    )> {
        let wake = self.wake.as_deref().map(|wake| (**wake).clone());
        let mut ready = Vec::new();
        for queue in &self.queues {
            let Some(entity) = queue.iter().next() else {
                continue;
            };
            if self.running.contains(entity) {
                continue;
            }
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
        for queue in &self.queues {
            let Some(entity) = queue.iter().next() else {
                continue;
            };
            if self.running.contains(entity) {
                continue;
            }
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
    for (entity, job, wake) in jobs.ready() {
        let task = GitJobTask::spawn(wake, move || {
            RepositoryOutput(
                GitRepositorySnapshot::load(&job.path)
                    .map_err(|error| GitOperationError { message: error.0 }),
            )
        });
        commands
            .entity(entity)
            .remove::<RepositoryJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_branch_log_jobs(jobs: PendingGitJobs<BranchLogJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            BranchLogOutput(
                crate::event::GitCommitEntry::for_reference(repository.path(), &job.branch)
                    .map(|commits| GitBranchLog {
                        repo_root: repository.path().to_string_lossy().into_owned(),
                        branch: job.branch,
                        commits,
                    })
                    .map_err(|error| GitOperationError { message: error.0 }),
            )
        });
        commands
            .entity(entity)
            .remove::<BranchLogJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_diff_jobs(jobs: PendingGitJobs<DiffJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            if !GitRepository::has_repository(repository.path()) {
                return DiffOutput(GitDiffViewport {
                    generation: job.generation,
                    first_line: job.top_line,
                    total_lines: 0,
                    lines: Vec::new(),
                    markers: Vec::new(),
                    error: String::new(),
                });
            }
            let result = if job.reference.is_empty() {
                match job.content.as_deref() {
                    Some(content) => repository.diff_lines_with_content(&job.path, content),
                    None => repository.diff_lines(&job.path),
                }
            } else {
                repository.commit_diff_lines(&job.reference)
            };
            match result {
                Ok(all_lines) => {
                    let markers = super::diff::GitDiffMarkers::from_lines(&all_lines).into_inner();
                    let (total_lines, lines) =
                        super::parse::window(&all_lines, job.top_line, job.rows);
                    DiffOutput(GitDiffViewport {
                        generation: job.generation,
                        first_line: job.top_line.min(total_lines),
                        total_lines,
                        lines,
                        markers,
                        error: String::new(),
                    })
                }
                Err(error) => DiffOutput(GitDiffViewport {
                    generation: job.generation,
                    first_line: job.top_line,
                    total_lines: 0,
                    lines: Vec::new(),
                    markers: Vec::new(),
                    error: error.0,
                }),
            }
        });
        commands
            .entity(entity)
            .remove::<DiffJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stage_jobs(jobs: PendingGitJobs<StageJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || match repository.stage(&job.path) {
            Ok(()) => result_then_status(&repository, &job.path, "stage", "ok"),
            Err(error) => failed_operation("stage", error),
        });
        commands
            .entity(entity)
            .remove::<StageJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_unstage_jobs(jobs: PendingGitJobs<UnstageJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || match repository.unstage(&job.path) {
            Ok(()) => result_then_status(&repository, &job.path, "unstage", "ok"),
            Err(error) => failed_operation("unstage", error),
        });
        commands
            .entity(entity)
            .remove::<UnstageJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_discard_jobs(jobs: PendingGitJobs<DiscardJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || match repository.discard(&job.path) {
            Ok(()) => result_then_status(&repository, &job.path, "discard", "ok"),
            Err(error) => failed_operation("discard", error),
        });
        commands
            .entity(entity)
            .remove::<DiscardJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_commit_jobs(jobs: PendingGitJobs<CommitJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let task = GitJobTask::spawn(wake, move || match GitRepository::discover(&job.path) {
            Ok(repository) => match repository.commit(&job.message) {
                Ok(()) => result_then_status(&repository, &job.path, "commit", "committed"),
                Err(error) => failed_operation("commit", error),
            },
            Err(error) => failed_operation("commit", error),
        });
        commands
            .entity(entity)
            .remove::<CommitJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_fetch_jobs(jobs: PendingGitJobs<FetchJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let task = GitJobTask::spawn(wake, move || match GitRepository::discover(&job.path) {
            Ok(repository) => match repository.fetch() {
                Ok(()) => result_then_status(&repository, &job.path, "fetch", "fetched"),
                Err(error) => failed_operation("fetch", error),
            },
            Err(error) => failed_operation("fetch", error),
        });
        commands
            .entity(entity)
            .remove::<FetchJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_pull_jobs(jobs: PendingGitJobs<PullJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let task = GitJobTask::spawn(wake, move || match GitRepository::discover(&job.path) {
            Ok(repository) => match repository.pull() {
                Ok(()) => result_then_status(&repository, &job.path, "pull", "pulled"),
                Err(error) => failed_operation("pull", error),
            },
            Err(error) => failed_operation("pull", error),
        });
        commands
            .entity(entity)
            .remove::<PullJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_push_jobs(jobs: PendingGitJobs<PushJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let task = GitJobTask::spawn(wake, move || match GitRepository::discover(&job.path) {
            Ok(repository) => match repository.push() {
                Ok(()) => result_then_status(&repository, &job.path, "push", "pushed"),
                Err(error) => failed_operation("push", error),
            },
            Err(error) => failed_operation("push", error),
        });
        commands
            .entity(entity)
            .remove::<PushJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stage_all_jobs(jobs: PendingGitJobs<StageAllJob>, mut commands: Commands) {
    for (entity, job, wake) in jobs.ready() {
        let task = GitJobTask::spawn(wake, move || match GitRepository::discover(&job.path) {
            Ok(repository) => match repository.stage_all() {
                Ok(()) => result_then_status(&repository, &job.path, "stage all", "staged"),
                Err(error) => failed_operation("stage all", error),
            },
            Err(error) => failed_operation("stage all", error),
        });
        commands
            .entity(entity)
            .remove::<StageAllJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_hunk_jobs(jobs: PendingGitJobs<HunkJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            match repository.apply_hunk(&job.path, job.hunk, job.accept) {
                Ok(()) => result_then_status(
                    &repository,
                    &job.path,
                    if job.accept { "accept" } else { "reject" },
                    "ok",
                ),
                Err(error) => failed_operation("hunk", error),
            }
        });
        commands
            .entity(entity)
            .remove::<HunkJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_amend_jobs(jobs: PendingGitJobs<AmendJob>, mut commands: Commands) {
    for (entity, _, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || operation("amend", repository.amend()));
        commands
            .entity(entity)
            .remove::<AmendJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_checkout_commit_jobs(jobs: PendingGitJobs<CheckoutCommitJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("checkout commit", repository.checkout_commit(&job.commit))
        });
        commands
            .entity(entity)
            .remove::<CheckoutCommitJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_cherry_pick_jobs(jobs: PendingGitJobs<CherryPickJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("cherry-pick", repository.cherry_pick(&job.commit))
        });
        commands
            .entity(entity)
            .remove::<CherryPickJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_create_branch_jobs(jobs: PendingGitJobs<CreateBranchJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation(
                "new branch",
                repository.create_branch(&job.branch, &job.start_point),
            )
        });
        commands
            .entity(entity)
            .remove::<CreateBranchJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_delete_branch_jobs(jobs: PendingGitJobs<DeleteBranchJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("delete branch", repository.delete_branch(&job.branch))
        });
        commands
            .entity(entity)
            .remove::<DeleteBranchJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_fast_forward_jobs(jobs: PendingGitJobs<FastForwardJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("fast-forward", repository.fast_forward(&job.branch))
        });
        commands
            .entity(entity)
            .remove::<FastForwardJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_merge_jobs(jobs: PendingGitJobs<MergeJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("merge", repository.merge(&job.branch))
        });
        commands
            .entity(entity)
            .remove::<MergeJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_rebase_jobs(jobs: PendingGitJobs<RebaseJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("rebase", repository.rebase(&job.branch))
        });
        commands
            .entity(entity)
            .remove::<RebaseJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_revert_jobs(jobs: PendingGitJobs<RevertJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("revert", repository.revert(&job.commit))
        });
        commands
            .entity(entity)
            .remove::<RevertJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stash_drop_jobs(jobs: PendingGitJobs<StashDropJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("stash drop", repository.stash_drop(&job.reference))
        });
        commands
            .entity(entity)
            .remove::<StashDropJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stash_pop_jobs(jobs: PendingGitJobs<StashPopJob>, mut commands: Commands) {
    for (entity, job, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || {
            operation("stash pop", repository.stash_pop(&job.reference))
        });
        commands
            .entity(entity)
            .remove::<StashPopJob>()
            .insert((GitJobRunning, task));
    }
}

fn start_stash_push_jobs(jobs: PendingGitJobs<StashPushJob>, mut commands: Commands) {
    for (entity, _, repository, wake) in jobs.ready_with_repository() {
        let task = GitJobTask::spawn(wake, move || operation("stash", repository.stash_push()));
        commands
            .entity(entity)
            .remove::<StashPushJob>()
            .insert((GitJobRunning, task));
    }
}

fn result_then_status(
    repository: &GitRepository,
    path: &Path,
    operation: &str,
    message: &str,
) -> OperationOutput {
    OperationOutput {
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

fn operation(
    operation: &str,
    result: Result<String, super::repository::GitError>,
) -> OperationOutput {
    match result {
        Ok(message) => OperationOutput {
            result: GitOperationResult {
                operation: operation.to_string(),
                ok: true,
                message,
            },
            status: None,
        },
        Err(error) => failed_operation(operation, error),
    }
}

fn failed_operation(operation: &str, error: super::repository::GitError) -> OperationOutput {
    OperationOutput {
        result: GitOperationResult {
            operation: operation.to_string(),
            ok: false,
            message: error.0,
        },
        status: None,
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
            captured.diffs.push(output.0.clone());
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
                    .after(poll_git_jobs::<DiffOutput>)
                    .after(poll_git_jobs::<OperationOutput>)
                    .before(deliver_diff_outputs)
                    .before(deliver_operation_outputs),
            );
        app
    }

    fn captured(app: &mut App) -> CapturedOutputs {
        for _ in 0..10_000 {
            app.update();
            let mut captured = app.world_mut().resource_mut::<CapturedOutputs>();
            if !captured.diffs.is_empty() || !captured.operations.is_empty() {
                return std::mem::take(&mut *captured);
            }
            std::thread::yield_now();
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

    #[test]
    fn jobs_for_one_webview_run_in_relationship_order() {
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();
        let root = tempfile::tempdir().unwrap();
        let first = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: root.path().join("first"),
                },
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                GitJob::new(webview),
                RepositoryJob {
                    path: root.path().join("second"),
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
    fn diff_job_emits_projected_viewport() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(repo.path()),
            DiffJob {
                path: file,
                reference: String::new(),
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
        let webview = app.world_mut().spawn_empty().id();
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
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            GitJob::new(webview),
            GitRepository::at(dir.path()),
            DiffJob {
                path: file,
                reference: String::new(),
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
