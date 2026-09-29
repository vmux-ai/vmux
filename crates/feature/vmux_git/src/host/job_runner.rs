use std::path::Path;
use std::thread::JoinHandle;

use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy::{ecs::system::SystemParam, prelude::*};

use super::GitUpdateSet;
use super::job::{
    AmendJob, BranchLogJob, CheckoutCommitJob, CherryPickJob, CommitJob, CreateBranchJob,
    DeleteBranchJob, DiffJob, DiscardJob, FastForwardJob, FetchJob, GitJobEmit, HunkJob, MergeJob,
    PullJob, PushJob, RebaseJob, RepositoryJob, RevertJob, StageAllJob, StageJob, StashDropJob,
    StashPopJob, StashPushJob, UnstageJob,
};

pub(super) struct JobPlugin;

impl Plugin for JobPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(queue_git_job_failure).add_systems(
            Update,
            (
                poll_git_jobs,
                deliver_git_outputs,
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
struct RunningGitJob {
    thread: Option<JoinHandle<Vec<GitJobEmit>>>,
}

#[derive(Component)]
struct GitJobOutput {
    emits: Vec<GitJobEmit>,
}

fn queue_git_job_failure(trigger: On<GitJobFailure>, mut commands: Commands) {
    commands.spawn((
        GitJob::new(trigger.event().webview),
        GitJobOutput {
            emits: vec![GitJobEmit::Error(crate::event::GitOperationError {
                message: trigger.event().message.clone(),
            })],
        },
    ));
}

fn poll_git_jobs(mut jobs: Query<(Entity, &mut RunningGitJob)>, mut commands: Commands) {
    for (entity, mut running) in &mut jobs {
        if !running.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            continue;
        }
        let emits = match running.thread.take().unwrap().join() {
            Ok(emits) => emits,
            Err(_) => vec![GitJobEmit::Error(crate::event::GitOperationError {
                message: "Git job worker panicked".to_string(),
            })],
        };
        commands
            .entity(entity)
            .remove::<RunningGitJob>()
            .insert(GitJobOutput { emits });
    }
}

fn deliver_git_outputs(
    mut outputs: Query<(Entity, &GitJob, &mut GitJobOutput)>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut views: Query<(
        &mut super::state::GitState,
        &mut super::controller::GitController,
    )>,
    mut files: Query<&mut super::status::FileGit>,
    diffs: Query<&super::diff::GitDiffQuery>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, job, mut output) in &mut outputs {
        let webview = job.webview;
        for emit in std::mem::take(&mut output.emits) {
            match emit {
                GitJobEmit::Repository(event) => {
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
                        controller.reconcile_repository(&event);
                        view.set_repository(event);
                        if let Some(payload) = controller.branch_log_request(&view) {
                            commands.trigger(bevy_cef::prelude::UiInput { webview, payload });
                        }
                    }
                }
                GitJobEmit::BranchLog(event) => {
                    if let Ok((mut view, _)) = views.get_mut(webview) {
                        view.set_branch_log(event);
                    }
                }
                GitJobEmit::Status(event) => {
                    if let Ok(mut file) = files.get_mut(webview) {
                        file.apply_status(event);
                    }
                }
                GitJobEmit::DiffViewport(event) => {
                    let Ok(query) = diffs.get(webview) else {
                        continue;
                    };
                    if let Ok((mut view, _)) = views.get_mut(webview) {
                        if query.accepts(event.generation) {
                            view.set_diff_viewport(event);
                        }
                        continue;
                    }
                    let Ok(mut file) = files.get_mut(webview) else {
                        continue;
                    };
                    if !query.accepts_file(event.generation, &file) {
                        continue;
                    }
                    file.apply_diff(event);
                }
                GitJobEmit::Result(event) => {
                    if let Ok((mut view, mut controller)) = views.get_mut(webview) {
                        let branch = controller.apply_result(&event);
                        view.apply_result(&event);
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
                        if !view.workspace().is_empty() {
                            commands.spawn((
                                GitJob::new(webview),
                                RepositoryJob {
                                    path: view.workspace().into(),
                                },
                            ));
                        }
                    } else if let Ok(mut file) = files.get_mut(webview) {
                        let refresh = file.apply_result(event, wake.clone());
                        commands.entity(webview).insert(refresh);
                    }
                }
                GitJobEmit::Error(event) => {
                    if let Ok((mut view, _)) = views.get_mut(webview) {
                        view.apply_error(&event);
                    } else if let Ok(mut file) = files.get_mut(webview) {
                        file.apply_error(event.message);
                    }
                }
            }
        }
        commands.entity(entity).despawn();
    }
}

#[derive(SystemParam)]
struct GitJobStarter<'w, 's, J: Component> {
    queues: Query<'w, 's, &'static GitJobs>,
    pending: Query<'w, 's, &'static J>,
    running: Query<'w, 's, (), With<RunningGitJob>>,
    wake: Option<Res<'w, EventLoopProxyWrapper>>,
    commands: Commands<'w, 's>,
}

impl<J: Component + Clone> GitJobStarter<'_, '_, J> {
    fn start(&mut self, run: fn(J) -> Vec<GitJobEmit>) {
        let wake = self.wake.as_deref().map(|wake| (**wake).clone());
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
            let job = job.clone();
            let wake = wake.clone();
            let thread = std::thread::spawn(move || {
                let emits = run(job);
                if let Some(wake) = wake {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
                emits
            });
            self.commands
                .entity(entity)
                .remove::<J>()
                .insert(RunningGitJob {
                    thread: Some(thread),
                });
        }
    }
}

fn start_repository_jobs(mut jobs: GitJobStarter<RepositoryJob>) {
    jobs.start(
        |job| match crate::event::GitRepositorySnapshot::load(&job.path) {
            Ok(event) => vec![GitJobEmit::Repository(event)],
            Err(error) => vec![GitJobEmit::Error(crate::event::GitOperationError {
                message: error.0,
            })],
        },
    );
}

fn start_branch_log_jobs(mut jobs: GitJobStarter<BranchLogJob>) {
    jobs.start(|job| {
        match crate::event::GitCommitEntry::for_reference(&job.repo_root, &job.branch) {
            Ok(commits) => vec![GitJobEmit::BranchLog(crate::event::GitBranchLog {
                repo_root: job.repo_root.to_string_lossy().into_owned(),
                branch: job.branch,
                commits,
            })],
            Err(error) => vec![GitJobEmit::Error(crate::event::GitOperationError {
                message: error.0,
            })],
        }
    });
}

fn start_diff_jobs(mut jobs: GitJobStarter<DiffJob>) {
    jobs.start(|job| {
        if !super::runner::has_repository(&job.repo_root) {
            return vec![GitJobEmit::DiffViewport(crate::event::GitDiffViewport {
                generation: job.generation,
                first_line: job.top_line,
                total_lines: 0,
                lines: Vec::new(),
                markers: Vec::new(),
                error: String::new(),
            })];
        }
        let result = if job.reference.is_empty() {
            match job.content.as_deref() {
                Some(content) => {
                    super::runner::diff_lines_with_content(&job.repo_root, &job.path, content)
                }
                None => super::runner::diff_lines(&job.repo_root, &job.path),
            }
        } else {
            super::runner::commit_diff_lines(&job.repo_root, &job.reference)
        };
        match result {
            Ok(all_lines) => {
                let markers = super::diff::GitDiffMarkers::from_lines(&all_lines).into_inner();
                let (total_lines, lines) = super::parse::window(&all_lines, job.top_line, job.rows);
                vec![GitJobEmit::DiffViewport(crate::event::GitDiffViewport {
                    generation: job.generation,
                    first_line: job.top_line.min(total_lines),
                    total_lines,
                    lines,
                    markers,
                    error: String::new(),
                })]
            }
            Err(error) => vec![GitJobEmit::DiffViewport(crate::event::GitDiffViewport {
                generation: job.generation,
                first_line: job.top_line,
                total_lines: 0,
                lines: Vec::new(),
                markers: Vec::new(),
                error: error.0,
            })],
        }
    });
}

fn start_stage_jobs(mut jobs: GitJobStarter<StageJob>) {
    jobs.start(|job| mutate(&job.repo_root, &job.path, "stage", super::runner::stage));
}

fn start_unstage_jobs(mut jobs: GitJobStarter<UnstageJob>) {
    jobs.start(|job| mutate(&job.repo_root, &job.path, "unstage", super::runner::unstage));
}

fn start_discard_jobs(mut jobs: GitJobStarter<DiscardJob>) {
    jobs.start(|job| mutate(&job.repo_root, &job.path, "discard", super::runner::discard));
}

fn start_commit_jobs(mut jobs: GitJobStarter<CommitJob>) {
    jobs.start(|job| match super::runner::commit(&job.path, &job.message) {
        Ok(()) => result_then_status(&job.path, &job.path, "commit", "committed"),
        Err(error) => failed_operation("commit", error),
    });
}

fn start_fetch_jobs(mut jobs: GitJobStarter<FetchJob>) {
    jobs.start(|job| match super::runner::fetch(&job.path) {
        Ok(()) => result_then_status(&job.path, &job.path, "fetch", "fetched"),
        Err(error) => failed_operation("fetch", error),
    });
}

fn start_pull_jobs(mut jobs: GitJobStarter<PullJob>) {
    jobs.start(|job| match super::runner::pull(&job.path) {
        Ok(()) => result_then_status(&job.path, &job.path, "pull", "pulled"),
        Err(error) => failed_operation("pull", error),
    });
}

fn start_push_jobs(mut jobs: GitJobStarter<PushJob>) {
    jobs.start(|job| match super::runner::push(&job.path) {
        Ok(()) => result_then_status(&job.path, &job.path, "push", "pushed"),
        Err(error) => failed_operation("push", error),
    });
}

fn start_stage_all_jobs(mut jobs: GitJobStarter<StageAllJob>) {
    jobs.start(|job| match super::runner::stage_all(&job.path) {
        Ok(()) => result_then_status(&job.path, &job.path, "stage all", "staged"),
        Err(error) => failed_operation("stage all", error),
    });
}

fn start_hunk_jobs(mut jobs: GitJobStarter<HunkJob>) {
    jobs.start(|job| {
        match super::runner::apply_hunk(&job.repo_root, &job.path, job.hunk, job.accept) {
            Ok(()) => result_then_status(
                &job.repo_root,
                &job.path,
                if job.accept { "accept" } else { "reject" },
                "ok",
            ),
            Err(error) => failed_operation("hunk", error),
        }
    });
}

fn start_amend_jobs(mut jobs: GitJobStarter<AmendJob>) {
    jobs.start(|job| operation("amend", super::runner::amend(&job.repo_root)));
}

fn start_checkout_commit_jobs(mut jobs: GitJobStarter<CheckoutCommitJob>) {
    jobs.start(|job| {
        operation(
            "checkout commit",
            super::runner::checkout_commit(&job.repo_root, &job.commit),
        )
    });
}

fn start_cherry_pick_jobs(mut jobs: GitJobStarter<CherryPickJob>) {
    jobs.start(|job| {
        operation(
            "cherry-pick",
            super::runner::cherry_pick(&job.repo_root, &job.commit),
        )
    });
}

fn start_create_branch_jobs(mut jobs: GitJobStarter<CreateBranchJob>) {
    jobs.start(|job| {
        operation(
            "new branch",
            super::runner::create_branch(&job.repo_root, &job.branch, &job.start_point),
        )
    });
}

fn start_delete_branch_jobs(mut jobs: GitJobStarter<DeleteBranchJob>) {
    jobs.start(|job| {
        operation(
            "delete branch",
            super::runner::delete_branch(&job.repo_root, &job.branch),
        )
    });
}

fn start_fast_forward_jobs(mut jobs: GitJobStarter<FastForwardJob>) {
    jobs.start(|job| {
        operation(
            "fast-forward",
            super::runner::fast_forward(&job.repo_root, &job.branch),
        )
    });
}

fn start_merge_jobs(mut jobs: GitJobStarter<MergeJob>) {
    jobs.start(|job| operation("merge", super::runner::merge(&job.repo_root, &job.branch)));
}

fn start_rebase_jobs(mut jobs: GitJobStarter<RebaseJob>) {
    jobs.start(|job| operation("rebase", super::runner::rebase(&job.repo_root, &job.branch)));
}

fn start_revert_jobs(mut jobs: GitJobStarter<RevertJob>) {
    jobs.start(|job| operation("revert", super::runner::revert(&job.repo_root, &job.commit)));
}

fn start_stash_drop_jobs(mut jobs: GitJobStarter<StashDropJob>) {
    jobs.start(|job| {
        operation(
            "stash drop",
            super::runner::stash_drop(&job.repo_root, &job.reference),
        )
    });
}

fn start_stash_pop_jobs(mut jobs: GitJobStarter<StashPopJob>) {
    jobs.start(|job| {
        operation(
            "stash pop",
            super::runner::stash_pop(&job.repo_root, &job.reference),
        )
    });
}

fn start_stash_push_jobs(mut jobs: GitJobStarter<StashPushJob>) {
    jobs.start(|job| operation("stash", super::runner::stash_push(&job.repo_root)));
}

fn result_then_status(
    repo_root: &Path,
    path: &Path,
    operation: &str,
    message: &str,
) -> Vec<GitJobEmit> {
    let result = GitJobEmit::Result(crate::event::GitOperationResult {
        operation: operation.to_string(),
        ok: true,
        message: message.to_string(),
    });
    match super::runner::status_at(repo_root, path) {
        Ok(event) => vec![result, GitJobEmit::Status(event)],
        Err(error) => vec![
            result,
            GitJobEmit::Error(crate::event::GitOperationError { message: error.0 }),
        ],
    }
}

fn mutate(
    repo_root: &Path,
    path: &Path,
    operation: &str,
    run: fn(&Path, &Path) -> Result<(), super::runner::GitError>,
) -> Vec<GitJobEmit> {
    match run(repo_root, path) {
        Ok(()) => result_then_status(repo_root, path, operation, "ok"),
        Err(error) => failed_operation(operation, error),
    }
}

fn operation(operation: &str, result: Result<String, super::runner::GitError>) -> Vec<GitJobEmit> {
    match result {
        Ok(message) => vec![GitJobEmit::Result(crate::event::GitOperationResult {
            operation: operation.to_string(),
            ok: true,
            message,
        })],
        Err(error) => failed_operation(operation, error),
    }
}

fn failed_operation(operation: &str, error: super::runner::GitError) -> Vec<GitJobEmit> {
    vec![GitJobEmit::Result(crate::event::GitOperationResult {
        operation: operation.to_string(),
        ok: false,
        message: error.0,
    })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{FileStatus, GitDiffViewport};
    use crate::host::runner::test_repo;

    #[derive(Resource, Default)]
    struct CapturedOutputs(Vec<GitJobEmit>);

    fn capture_outputs(
        mut outputs: Query<&mut GitJobOutput>,
        mut captured: ResMut<CapturedOutputs>,
    ) {
        for mut output in &mut outputs {
            captured.0.append(&mut output.emits);
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<CapturedOutputs>()
            .add_plugins(JobPlugin)
            .add_systems(
                Update,
                capture_outputs
                    .after(poll_git_jobs)
                    .before(deliver_git_outputs),
            );
        app
    }

    fn captured(app: &mut App) -> Vec<GitJobEmit> {
        for _ in 0..10_000 {
            app.update();
            let mut captured = app.world_mut().resource_mut::<CapturedOutputs>();
            if !captured.0.is_empty() {
                return std::mem::take(&mut captured.0);
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

        assert!(app.world().get::<RunningGitJob>(first).is_some());
        assert!(app.world().get::<RepositoryJob>(second).is_some());
        assert!(app.world().get::<RunningGitJob>(second).is_none());
    }

    #[test]
    fn diff_job_emits_projected_viewport() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            GitJob::new(webview),
            DiffJob {
                repo_root: repo.path().to_path_buf(),
                path: file,
                reference: String::new(),
                generation: 7,
                top_line: 0,
                rows: 50,
                content: None,
            },
        ));

        assert!(matches!(
            captured(&mut app).as_slice(),
            [GitJobEmit::DiffViewport(GitDiffViewport {
                generation: 7,
                ..
            })]
        ));
    }

    #[test]
    fn stage_job_emits_result_then_fresh_status() {
        let (repo, file) = dirty_repo();
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            GitJob::new(webview),
            StageJob {
                repo_root: repo.path().to_path_buf(),
                path: file,
            },
        ));

        match captured(&mut app).as_slice() {
            [GitJobEmit::Result(result), GitJobEmit::Status(status)] => {
                assert!(result.ok);
                assert_eq!(status.file_status, FileStatus::Staged);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn diff_on_non_repo_emits_empty_viewport() {
        let dir = tempfile::tempdir().unwrap();
        let file = test_repo::write(dir.path(), "loose.txt", "x");
        let mut app = app();
        let webview = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            GitJob::new(webview),
            DiffJob {
                repo_root: dir.path().to_path_buf(),
                path: file,
                reference: String::new(),
                generation: 7,
                top_line: 0,
                rows: 50,
                content: None,
            },
        ));

        assert!(matches!(
            captured(&mut app).as_slice(),
            [GitJobEmit::DiffViewport(GitDiffViewport {
                generation: 7,
                total_lines: 0,
                lines,
                markers,
                ..
            })] if lines.is_empty() && markers.is_empty()
        ));
    }
}
