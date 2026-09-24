use std::path::Path;
use std::thread::JoinHandle;

use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};

use super::GitUpdateSet;
use super::job::{Emit, JobKind};

pub(super) struct JobPlugin;

impl Plugin for JobPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(queue_git_job)
            .add_observer(queue_git_job_failure)
            .add_systems(
                Update,
                (poll_git_jobs, deliver_git_outputs, start_git_jobs)
                    .chain()
                    .in_set(GitUpdateSet::Jobs),
            );
    }
}

#[derive(EntityEvent)]
pub(super) struct GitJobRequest {
    #[event_target]
    pub(super) webview: Entity,
    pub(super) job: JobKind,
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

#[derive(Component)]
#[relationship_target(relationship = GitJob)]
pub(super) struct GitJobs(Vec<Entity>);

#[derive(Component)]
struct PendingGitJob(JobKind);

#[derive(Component)]
struct RunningGitJob {
    thread: Option<JoinHandle<Vec<Emit>>>,
}

#[derive(Component)]
struct GitJobOutput {
    webview: Entity,
    emits: Vec<Emit>,
}

fn queue_git_job(trigger: On<GitJobRequest>, mut commands: Commands) {
    commands.spawn((
        GitJob {
            webview: trigger.event().webview,
        },
        PendingGitJob(trigger.event().job.clone()),
    ));
}

fn queue_git_job_failure(trigger: On<GitJobFailure>, mut commands: Commands) {
    commands.spawn(GitJobOutput {
        webview: trigger.event().webview,
        emits: vec![Emit::Error(crate::event::GitOperationError {
            message: trigger.event().message.clone(),
        })],
    });
}

fn poll_git_jobs(mut jobs: Query<(Entity, &GitJob, &mut RunningGitJob)>, mut commands: Commands) {
    for (entity, job, mut running) in &mut jobs {
        if !running.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            continue;
        }
        let emits = match running.thread.take().unwrap().join() {
            Ok(emits) => emits,
            Err(_) => vec![Emit::Error(crate::event::GitOperationError {
                message: "Git job worker panicked".to_string(),
            })],
        };
        commands
            .entity(entity)
            .remove::<RunningGitJob>()
            .insert(GitJobOutput {
                webview: job.webview,
                emits,
            });
    }
}

fn deliver_git_outputs(
    mut outputs: Query<(Entity, &mut GitJobOutput)>,
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
    for (entity, mut output) in &mut outputs {
        let webview = output.webview;
        for emit in std::mem::take(&mut output.emits) {
            match emit {
                Emit::Repository(event) => {
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
                Emit::BranchLog(event) => {
                    if let Ok((mut view, _)) = views.get_mut(webview) {
                        view.set_branch_log(event);
                    }
                }
                Emit::Status(event) => {
                    if let Ok(mut file) = files.get_mut(webview) {
                        file.apply_status(event);
                    }
                }
                Emit::DiffViewport(event) => {
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
                Emit::Result(event) => {
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
                            commands.trigger(GitJobRequest {
                                webview,
                                job: JobKind::Repository {
                                    path: view.workspace().into(),
                                },
                            });
                        }
                    } else if let Ok(mut file) = files.get_mut(webview) {
                        let refresh = file.apply_result(event, wake.clone());
                        commands.entity(webview).insert(refresh);
                    }
                }
                Emit::Error(event) => {
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

fn start_git_jobs(
    queues: Query<&GitJobs>,
    pending: Query<&PendingGitJob>,
    running: Query<(), With<RunningGitJob>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for queue in &queues {
        let Some(entity) = queue.iter().next() else {
            continue;
        };
        if running.contains(entity) {
            continue;
        }
        let Ok(job) = pending.get(entity) else {
            continue;
        };
        let kind = job.0.clone();
        let wake = wake.clone();
        let thread = std::thread::spawn(move || {
            let emits = kind.run();
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            emits
        });
        commands
            .entity(entity)
            .remove::<PendingGitJob>()
            .insert(RunningGitJob {
                thread: Some(thread),
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_for_one_webview_run_in_relationship_order() {
        let mut app = App::new();
        app.add_plugins(JobPlugin);
        let webview = app.world_mut().spawn_empty().id();
        let root = tempfile::tempdir().unwrap();
        let first = app
            .world_mut()
            .spawn((
                GitJob { webview },
                PendingGitJob(JobKind::Repository {
                    path: root.path().join("first"),
                }),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                GitJob { webview },
                PendingGitJob(JobKind::Repository {
                    path: root.path().join("second"),
                }),
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
        assert!(app.world().get::<PendingGitJob>(second).is_some());
        assert!(app.world().get::<RunningGitJob>(second).is_none());
    }
}
