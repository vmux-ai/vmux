use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use vmux_core::host::{FileUiStateUpdates, FileUiStateWrite};

use crate::event::{FileGitState, FileStatus, GitDiffViewport, GitFileStatus, GitOperationResult};

use super::GitDiffSource;
use super::GitUpdateSet;
use super::watch::GitWatch;

const STATUS_DEBOUNCE: Duration = Duration::from_millis(120);

pub(super) struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_file_git_added).add_systems(
            Update,
            (
                refresh_changed_sources,
                start_status_refreshes,
                poll_status_refreshes,
                poll_status_tasks,
                dispatch_status_requests,
                publish_file_git_state,
            )
                .chain()
                .in_set(GitUpdateSet::Status),
        );
    }
}

#[derive(Component, Clone, Debug)]
pub struct FileGit {
    document: u64,
    generation: u64,
    state: FileGitState,
}

impl FileGit {
    pub fn new(path: impl AsRef<Path>, document: u64) -> Self {
        Self {
            document,
            generation: 0,
            state: FileGitState {
                path: path.as_ref().to_string_lossy().into_owned(),
                ..Default::default()
            },
        }
    }

    pub(super) fn refresh(
        &mut self,
        delay: Duration,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> GitStatusRefresh {
        self.generation = self.generation.wrapping_add(1).max(1);
        GitStatusRefresh {
            revision: self.generation,
            delay,
            wake,
        }
    }

    pub(super) fn apply_status(&mut self, event: GitFileStatus) {
        if event.path != self.state.path {
            return;
        }
        self.state.repo_root = event.repo_root;
        self.state.has_diff = matches!(
            event.file_status,
            FileStatus::Modified
                | FileStatus::Staged
                | FileStatus::StagedModified
                | FileStatus::Conflicted
                | FileStatus::Deleted
        );
        self.state.branch = event.branch;
        self.state.ahead = event.ahead;
        self.state.behind = event.behind;
        self.state.staged_count = event.staged_count;
        self.state.message.clear();
        if !self.state.has_diff {
            self.clear_diff();
        }
    }

    pub(super) fn apply_result(
        &mut self,
        event: GitOperationResult,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> GitStatusRefresh {
        self.state.message = if event.ok {
            String::new()
        } else {
            event.message.clone()
        };
        self.state.result = Some(event);
        self.state.result_sequence = self.state.result_sequence.wrapping_add(1).max(1);
        self.refresh(Duration::ZERO, wake)
    }

    pub(super) fn apply_error(&mut self, message: String) {
        self.state.message = message;
    }

    pub(super) fn path(&self) -> &Path {
        Path::new(&self.state.path)
    }

    pub(super) fn repo_root(&self) -> Option<PathBuf> {
        (!self.state.repo_root.is_empty()).then(|| PathBuf::from(&self.state.repo_root))
    }

    pub(super) fn start_diff(&mut self, target_changed: bool) {
        self.state.diff_loading = target_changed || self.state.diff_viewport.is_none();
        if target_changed {
            self.state.diff_viewport = None;
        }
    }

    pub(super) fn apply_diff(&mut self, event: GitDiffViewport) {
        self.state.diff_loading = false;
        self.state.diff_viewport = Some(event);
    }

    fn clear_diff(&mut self) {
        self.state.diff_loading = false;
        self.state.diff_viewport = None;
    }

    pub(super) fn changed(
        &mut self,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> GitStatusRefresh {
        self.refresh(STATUS_DEBOUNCE, wake)
    }

    pub(super) fn identity(&self) -> (u64, u64) {
        (self.document, self.generation)
    }

    pub(super) fn accepts(&self, document: u64, revision: u64) -> bool {
        self.document == document && self.generation == revision
    }

    fn settle(&mut self, revision: u64) -> bool {
        if self.generation != revision {
            return false;
        }
        self.state.refresh_revision = self.state.refresh_revision.wrapping_add(1).max(1);
        true
    }
}

#[derive(Component)]
pub(super) struct GitStatusRefresh {
    revision: u64,
    delay: Duration,
    wake: Option<EventLoopProxy<WinitUserEvent>>,
}

#[derive(Component)]
struct GitStatusRefreshTask {
    revision: u64,
    task: Task<()>,
}

#[derive(Component, Clone, Debug)]
struct PendingGitStatus {
    repo_root: PathBuf,
    path: PathBuf,
    document: u64,
    revision: u64,
    dirty: bool,
}

struct GitStatusRequestInput {
    webview: Entity,
    path: PathBuf,
    document: u64,
    revision: u64,
    dirty: bool,
}

#[derive(Component)]
struct GitStatusTask {
    repo_root: PathBuf,
    requests: Vec<GitStatusRequestIdentity>,
    thread: Option<JoinHandle<GitStatusResults>>,
}

#[derive(Clone, Copy)]
struct GitStatusRequestIdentity {
    webview: Entity,
    document: u64,
    revision: u64,
}

struct GitStatusResult {
    identity: GitStatusRequestIdentity,
    status: Result<GitFileStatus, String>,
}

struct GitStatusResults(Vec<GitStatusResult>);

fn on_file_git_added(
    trigger: On<Add, FileGit>,
    mut files: Query<&mut FileGit>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Ok(mut file) = files.get_mut(trigger.entity) else {
        return;
    };
    let refresh = file.refresh(Duration::ZERO, wake.as_deref().map(|wake| (**wake).clone()));
    commands.entity(trigger.entity).insert(refresh);
}

fn refresh_changed_sources(
    mut sources: Query<(Entity, Ref<GitDiffSource>, &mut FileGit), Changed<GitDiffSource>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, source, mut file) in &mut sources {
        if source.is_added() {
            continue;
        }
        let refresh = file
            .bypass_change_detection()
            .refresh(STATUS_DEBOUNCE, wake.clone());
        commands.entity(entity).insert(refresh);
    }
}

fn start_status_refreshes(
    refreshes: Query<(Entity, &GitStatusRefresh), Added<GitStatusRefresh>>,
    mut commands: Commands,
) {
    for (entity, refresh) in &refreshes {
        let revision = refresh.revision;
        let delay = refresh.delay;
        let wake = refresh.wake.clone();
        let task = IoTaskPool::get().spawn(async move {
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
        });
        commands
            .entity(entity)
            .remove::<GitStatusRefresh>()
            .insert(GitStatusRefreshTask { revision, task });
    }
}

fn poll_status_refreshes(
    mut refreshes: Query<(
        Entity,
        &mut GitStatusRefreshTask,
        &mut FileGit,
        Option<&GitDiffSource>,
    )>,
    mut watch: Option<NonSendMut<GitWatch>>,
    mut commands: Commands,
) {
    for (entity, mut refresh, mut file, source) in &mut refreshes {
        if future::block_on(future::poll_once(&mut refresh.task)).is_none() {
            continue;
        }
        commands.entity(entity).remove::<GitStatusRefreshTask>();
        if !file.settle(refresh.revision) {
            continue;
        }
        let path = PathBuf::from(&file.state.path);
        if !super::runner::has_repository(&path) {
            file.apply_status(super::runner::non_repository_status(&path));
            commands.entity(entity).remove::<PendingGitStatus>();
            continue;
        }
        let repo_root = if let Some(watch) = watch.as_deref_mut() {
            watch.subscribe(entity, &path)
        } else {
            super::runner::repo_root(&path)
        };
        match repo_root {
            Ok(repo_root) => {
                file.state.repo_root = repo_root.to_string_lossy().into_owned();
                commands.entity(entity).insert(PendingGitStatus {
                    repo_root,
                    path,
                    document: file.document,
                    revision: refresh.revision,
                    dirty: source.is_some_and(|source| source.dirty),
                });
                commands.trigger(super::diff::FileDiffRefresh { entity });
            }
            Err(error) => {
                commands.entity(entity).remove::<PendingGitStatus>();
                file.apply_error(error.0);
            }
        }
    }
}

fn poll_status_tasks(
    mut tasks: Query<(Entity, &mut GitStatusTask)>,
    mut files: Query<&mut FileGit>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        if !task.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            continue;
        }
        let results = match task.thread.take().unwrap().join() {
            Ok(results) => results,
            Err(_) => GitStatusResults(
                task.requests
                    .iter()
                    .copied()
                    .map(|identity| GitStatusResult {
                        identity,
                        status: Err("Git status worker panicked".to_string()),
                    })
                    .collect(),
            ),
        };
        for result in results.0 {
            let Ok(mut file) = files.get_mut(result.identity.webview) else {
                continue;
            };
            if !file.accepts(result.identity.document, result.identity.revision) {
                continue;
            }
            match result.status {
                Ok(status) => file.apply_status(status),
                Err(message) => file.apply_error(message),
            }
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
                document: request.document,
                revision: request.revision,
                dirty: request.dirty,
            });
        commands.entity(webview).remove::<PendingGitStatus>();
    }
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (repo_root, requests) in batches {
        let task_root = repo_root.clone();
        let identities = requests
            .iter()
            .map(|request| GitStatusRequestIdentity {
                webview: request.webview,
                document: request.document,
                revision: request.revision,
            })
            .collect();
        let wake = wake.clone();
        let thread = std::thread::spawn(move || {
            let paths = requests
                .iter()
                .map(|request| request.path.clone())
                .collect::<Vec<_>>();
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
                        GitStatusResult {
                            identity: GitStatusRequestIdentity {
                                webview: request.webview,
                                document: request.document,
                                revision: request.revision,
                            },
                            status: Ok(event),
                        }
                    })
                    .collect(),
                Err(error) => requests
                    .into_iter()
                    .map(|request| GitStatusResult {
                        identity: GitStatusRequestIdentity {
                            webview: request.webview,
                            document: request.document,
                            revision: request.revision,
                        },
                        status: Err(error.0.clone()),
                    })
                    .collect(),
            };
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            GitStatusResults(results)
        });
        commands.spawn(GitStatusTask {
            repo_root,
            requests: identities,
            thread: Some(thread),
        });
    }
}

fn publish_file_git_state(
    files: Query<(Entity, &FileGit), Changed<FileGit>>,
    pages: Query<(), With<FileUiStateUpdates>>,
    mut commands: Commands,
) {
    for (entity, file) in &files {
        if pages.contains(entity) {
            commands.trigger(FileUiStateWrite::from_event(entity, &file.state));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::runner::test_repo;
    use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
    use vmux_api::BinEvent;
    use vmux_core::event::{FileUiState, FileUiStatePatch};

    #[derive(Resource, Default)]
    struct Emitted(Vec<FileUiState>);

    impl Emitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != FileUiState::id() {
                return;
            }
            let state =
                rkyv::from_bytes::<FileUiState, rkyv::rancor::Error>(trigger.event().payload())
                    .unwrap();
            emitted.0.push(state);
        }
    }

    #[test]
    fn status_task_batches_repository_paths_and_preserves_dirty_buffers() {
        let repo = test_repo::init();
        let first = test_repo::write(repo.path(), "a.txt", "one\n");
        let second = test_repo::write(repo.path(), "b.txt", "two\n");
        test_repo::run(repo.path(), &["add", "a.txt", "b.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatusPlugin));
        let first_webview = app
            .world_mut()
            .spawn((
                FileGit::new(&first, 3),
                GitDiffSource {
                    content: String::new(),
                    dirty: true,
                },
            ))
            .id();
        let second_webview = app
            .world_mut()
            .spawn((FileGit::new(&second, 5), GitDiffSource::default()))
            .id();

        for _ in 0..10_000 {
            app.update();
            let first = app.world().get::<FileGit>(first_webview).unwrap();
            let second = app.world().get::<FileGit>(second_webview).unwrap();
            if first.state.has_diff
                && !first.state.branch.is_empty()
                && !second.state.branch.is_empty()
            {
                assert!(first.state.has_diff);
                assert!(!second.state.has_diff);
                return;
            }
            std::thread::yield_now();
        }
        panic!("Git status batch did not complete");
    }

    #[test]
    fn status_result_is_bound_to_document_and_refresh_revision() {
        let mut file = FileGit::new("/repo/a.rs", 7);
        file.generation = 9;

        assert!(file.accepts(7, 9));
        assert!(!file.accepts(8, 9));
        assert!(!file.accepts(7, 10));
    }

    #[test]
    fn file_git_component_publishes_host_owned_status() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_core::host::UiStatePlugin::<FileUiState>::default(),
            StatusPlugin,
        ))
        .init_resource::<Emitted>()
        .add_observer(Emitted::record);
        let entity = app
            .world_mut()
            .spawn((
                FileUiStateUpdates::default(),
                FileGit::new(&file, 1),
                GitDiffSource::default(),
            ))
            .id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        for _ in 0..10_000 {
            app.update();
            let published = app.world().resource::<Emitted>().0.iter().any(|state| {
                state.patches.iter().any(|patch| {
                    matches!(
                        patch,
                        FileUiStatePatch::GitState(state)
                            if state.path == file.to_string_lossy() && state.has_diff
                    )
                })
            });
            if published {
                return;
            }
            std::thread::yield_now();
        }
        panic!("file git status did not publish");
    }
}
