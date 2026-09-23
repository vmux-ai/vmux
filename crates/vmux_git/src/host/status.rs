use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{BinReceive, UiEventPlugin};

use crate::event::{FileStatus, GitErrorEvent, GitStatusRequest};

use super::GitDiffSource;
use super::GitUpdateSet;
use super::job::Emit;
use super::outbox::GitOutbox;
use super::watch::GitWatch;

pub(super) struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitStatusRequest,)>::default())
            .add_observer(on_status_request)
            .add_systems(
                Update,
                (poll_status_tasks, dispatch_status_requests)
                    .chain()
                    .in_set(GitUpdateSet::Status),
            );
    }
}

#[derive(Component, Clone, Debug)]
struct PendingGitStatus {
    repo_root: PathBuf,
    path: PathBuf,
    dirty: bool,
}

struct GitStatusRequestInput {
    webview: Entity,
    path: PathBuf,
    dirty: bool,
}

#[derive(Component)]
struct GitStatusTask {
    repo_root: PathBuf,
    task: Task<Vec<(Entity, Vec<Emit>)>>,
}

fn on_status_request(
    trigger: On<BinReceive<GitStatusRequest>>,
    sources: Query<&GitDiffSource>,
    outbox: Res<GitOutbox>,
    watch: Option<NonSendMut<GitWatch>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    if !super::runner::has_repository(&path) {
        commands.entity(webview).remove::<PendingGitStatus>();
        outbox.events(
            webview,
            vec![Emit::Status(super::runner::non_repository_status(&path))],
        );
        return;
    }
    let repo_root = if let Some(mut watch) = watch {
        watch.subscribe(webview, &path)
    } else {
        super::runner::repo_root(&path)
    };
    match repo_root {
        Ok(repo_root) => {
            commands.entity(webview).insert(PendingGitStatus {
                repo_root,
                path,
                dirty: sources.get(webview).is_ok_and(|source| source.dirty),
            });
        }
        Err(error) => {
            commands.entity(webview).remove::<PendingGitStatus>();
            outbox.error(webview, error.0);
        }
    }
}

fn poll_status_tasks(
    mut tasks: Query<(Entity, &mut GitStatusTask)>,
    outbox: Res<GitOutbox>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(results) = future::block_on(future::poll_once(&mut task.task)) else {
            continue;
        };
        for (webview, emits) in results {
            outbox.events(webview, emits);
        }
        commands.entity(entity).despawn();
    }
}

fn dispatch_status_requests(
    pending: Query<(Entity, &PendingGitStatus)>,
    tasks: Query<&GitStatusTask>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let active: HashSet<PathBuf> = tasks.iter().map(|task| task.repo_root.clone()).collect();
    let mut batches: HashMap<PathBuf, Vec<GitStatusRequestInput>> = HashMap::new();
    for (webview, request) in &pending {
        if active.contains(&request.repo_root) {
            continue;
        }
        batches
            .entry(request.repo_root.clone())
            .or_default()
            .push(GitStatusRequestInput {
                webview,
                path: request.path.clone(),
                dirty: request.dirty,
            });
        commands.entity(webview).remove::<PendingGitStatus>();
    }
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (repo_root, requests) in batches {
        let task = GitStatusTask::spawn(repo_root, requests, wake.clone());
        commands.spawn(task);
    }
}

impl GitStatusTask {
    fn spawn(
        repo_root: PathBuf,
        requests: Vec<GitStatusRequestInput>,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Self {
        let task_root = repo_root.clone();
        let task = IoTaskPool::get().spawn(async move {
            let paths: Vec<PathBuf> = requests
                .iter()
                .map(|request| request.path.clone())
                .collect();
            let results = match super::runner::statuses(&task_root, &paths) {
                Ok(events) => requests
                    .into_iter()
                    .zip(events)
                    .map(|(request, mut event)| {
                        if request.dirty {
                            event.file_status = match event.file_status {
                                FileStatus::Clean => FileStatus::Modified,
                                FileStatus::Staged => FileStatus::StagedModified,
                                status => status,
                            };
                        }
                        (request.webview, vec![Emit::Status(event)])
                    })
                    .collect(),
                Err(error) => requests
                    .into_iter()
                    .map(|request| {
                        (
                            request.webview,
                            vec![Emit::Error(GitErrorEvent {
                                message: error.0.clone(),
                            })],
                        )
                    })
                    .collect(),
            };
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            results
        });
        Self { repo_root, task }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::runner::test_repo;

    #[test]
    fn status_task_batches_repository_paths_and_preserves_dirty_buffers() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        let first = test_repo::write(repo.path(), "a.txt", "one\n");
        let second = test_repo::write(repo.path(), "b.txt", "two\n");
        test_repo::run(repo.path(), &["add", "a.txt", "b.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let first_webview = Entity::from_bits(1);
        let second_webview = Entity::from_bits(2);
        let mut task = GitStatusTask::spawn(
            repo.path().to_path_buf(),
            vec![
                GitStatusRequestInput {
                    webview: first_webview,
                    path: first,
                    dirty: true,
                },
                GitStatusRequestInput {
                    webview: second_webview,
                    path: second,
                    dirty: false,
                },
            ],
            None,
        );

        let results = loop {
            if let Some(results) = future::block_on(future::poll_once(&mut task.task)) {
                break results;
            }
            std::thread::yield_now();
        };

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|(webview, emits)| {
            *webview == first_webview
                && matches!(
                    emits.as_slice(),
                    [Emit::Status(event)] if event.file_status == FileStatus::Modified
                )
        }));
        assert!(results.iter().any(|(webview, emits)| {
            *webview == second_webview
                && matches!(
                    emits.as_slice(),
                    [Emit::Status(event)] if event.file_status == FileStatus::Clean
                )
        }));
    }
}
