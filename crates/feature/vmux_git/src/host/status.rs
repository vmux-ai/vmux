use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::{UiFileUpdates, UiFileWrite};

use vmux_path::PathIdentity;

use crate::event::{
    DiffKind, FileGitBranchesRequest, FileGitState, FileStatus, GitCommitDraftRequest,
    GitCommitRequest, GitCommitSubmitRequest, GitDiffRevealRequest, GitDiffRow, GitDiffViewport,
    GitFileStatus, GitOperationResult,
};

use super::GitDiffSource;
use super::GitUpdateSet;
use super::repository::GitRepository;
use super::runtime::{GitRuntime, GitTask};
use super::watch::GitWatch;

const STATUS_DEBOUNCE: Duration = Duration::from_millis(120);
const DIFF_CONTEXT_LINES: usize = 3;

pub(super) struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .init_resource::<GitStatusScheduler>()
            .add_plugins(UiEventPlugin::<(FileGitBranchesRequest,)>::default())
            .add_observer(file_added)
            .add_observer(load_branches)
            .add_observer(edit_commit)
            .add_observer(submit_commit)
            .add_systems(
                Update,
                (
                    refresh_changed_sources,
                    start_refreshes,
                    poll_refreshes,
                    cancel_stale_tasks,
                    bevy::ecs::schedule::ApplyDeferred,
                    poll_tasks,
                    start_branch_tasks,
                    poll_branch_tasks,
                    dispatch_requests,
                    publish_file_state,
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
    status_applied: bool,
    revealed_diff: super::state::GitDiffRevealRanges,
    state: FileGitState,
}

impl FileGit {
    pub fn new(path: impl AsRef<Path>, document: u64) -> Self {
        Self {
            document,
            generation: 0,
            status_applied: false,
            revealed_diff: Default::default(),
            state: FileGitState {
                path: path.as_ref().to_string_lossy().into_owned(),
                ..Default::default()
            },
        }
    }

    pub(super) fn path(&self) -> &Path {
        Path::new(&self.state.path)
    }

    pub fn has_changes(&self) -> Option<bool> {
        self.status_applied.then_some(self.state.has_diff)
    }

    pub(super) fn repo_root(&self) -> Option<PathBuf> {
        (!self.state.repo_root.is_empty()).then(|| PathBuf::from(&self.state.repo_root))
    }

    pub(super) fn identity(&self) -> (u64, u64) {
        (self.document, self.generation)
    }

    pub(super) fn accepts(&self, document: u64, revision: u64) -> bool {
        self.document == document && self.generation == revision
    }
}

#[derive(SystemParam)]
pub(super) struct FileGitStates<'w, 's> {
    files: Query<'w, 's, &'static mut FileGit>,
}

impl FileGitStates<'_, '_> {
    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.files.contains(entity)
    }

    pub(super) fn repository_matches(&self, entity: Entity, repository: &Path) -> bool {
        self.files.get(entity).is_ok_and(|file| {
            !file.state.repo_root.is_empty()
                && PathIdentity::lexical(Path::new(&file.state.repo_root))
                    == PathIdentity::lexical(repository)
        })
    }

    pub(super) fn accepts(&self, entity: Entity, document: u64, revision: u64) -> bool {
        self.files
            .get(entity)
            .is_ok_and(|file| file.accepts(document, revision))
    }

    pub(super) fn accepts_diff(
        &self,
        entity: Entity,
        generation: u64,
        query: &super::diff::GitDiffQuery,
    ) -> bool {
        self.files
            .get(entity)
            .is_ok_and(|file| query.accepts_file(generation, file))
    }

    pub(super) fn schedule(
        &mut self,
        entity: Entity,
        delay: Duration,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        discover_repository: bool,
    ) -> Option<GitStatusRefresh> {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return None;
        };
        Some(Self::refresh(
            file.bypass_change_detection(),
            delay,
            wake,
            discover_repository,
        ))
    }

    pub(super) fn schedule_changed(
        &mut self,
        entity: Entity,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> Option<GitStatusRefresh> {
        self.schedule(entity, STATUS_DEBOUNCE, wake, true)
    }

    pub(super) fn apply_status(&mut self, entity: Entity, event: GitFileStatus) {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return;
        };
        if event.path != file.state.path {
            return;
        }
        let repository_changed = file.state.repo_root != event.repo_root;
        let branch_changed = file.state.branch != event.branch;
        file.status_applied = true;
        file.state.repo_root = event.repo_root;
        file.state.has_diff = matches!(
            event.file_status,
            FileStatus::Modified
                | FileStatus::Staged
                | FileStatus::StagedModified
                | FileStatus::Conflicted
                | FileStatus::Deleted
        );
        file.state.branch = event.branch;
        if repository_changed
            || branch_changed
            || !file.state.branches.is_empty()
                && !file
                    .state
                    .branches
                    .iter()
                    .any(|branch| branch.name == file.state.branch)
        {
            file.state.branches.clear();
            file.state.branches_loaded = false;
            file.state.branches_loading = false;
        }
        file.state.ahead = event.ahead;
        file.state.behind = event.behind;
        file.state.staged_count = event.staged_count;
        file.state.message.clear();
        if !file.state.has_diff {
            Self::clear_diff(&mut file);
        }
    }

    pub(super) fn apply_result(
        &mut self,
        entity: Entity,
        event: GitOperationResult,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
    ) -> Option<GitStatusRefresh> {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return None;
        };
        if event.operation == "commit" {
            if event.ok && file.state.commit_message.trim() == file.state.commit_pending {
                file.state.commit_message.clear();
            }
            file.state.commit_pending.clear();
        }
        file.state.message = if event.ok {
            String::new()
        } else {
            event.message.clone()
        };
        file.state.result = Some(event);
        file.state.result_sequence = file.state.result_sequence.wrapping_add(1).max(1);
        Some(Self::refresh(&mut file, Duration::ZERO, wake, true))
    }

    pub(super) fn apply_error(&mut self, entity: Entity, message: String) {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return;
        };
        file.state.message = message;
    }

    pub(super) fn fail_diff(&mut self, entity: Entity, generation: u64, message: String) {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return;
        };
        file.state.diff_loading = false;
        file.state.diff_rows.clear();
        file.state.diff_viewport = Some(GitDiffViewport {
            generation,
            revision: 0,
            first_line: 0,
            total_lines: 0,
            has_more: false,
            lines: Vec::new(),
            markers: Vec::new(),
            error: message,
        });
    }

    pub(super) fn start_diff(&mut self, entity: Entity, target_changed: bool) {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return;
        };
        file.state.diff_loading = target_changed || file.state.diff_viewport.is_none();
        if target_changed {
            Self::clear_diff(&mut file);
            file.state.diff_loading = true;
        }
    }

    pub(super) fn apply_diff(&mut self, entity: Entity, event: GitDiffViewport) {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return;
        };
        file.state.diff_loading = false;
        file.state.diff_viewport = Some(event);
        file.revealed_diff.0.clear();
        Self::project_diff(&mut file);
    }

    pub(super) fn reveal_diff(&mut self, entity: Entity, request: &GitDiffRevealRequest) -> bool {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return false;
        };
        let file = &mut *file;
        if !file.revealed_diff.reveal(
            file.state.diff_viewport.as_ref(),
            &file.state.diff_rows,
            request,
        ) {
            return false;
        }
        Self::project_diff(file);
        true
    }

    pub(super) fn settle(
        &mut self,
        entity: Entity,
        revision: u64,
        discover_repository: bool,
    ) -> Option<(PathBuf, u64, Option<PathBuf>)> {
        let Ok(mut file) = self.files.get_mut(entity) else {
            return None;
        };
        if file.generation != revision {
            return None;
        }
        file.state.refresh_revision = file.state.refresh_revision.wrapping_add(1).max(1);
        Some((
            PathBuf::from(&file.state.path),
            file.document,
            (!discover_repository).then(|| file.repo_root()).flatten(),
        ))
    }

    fn refresh(
        file: &mut FileGit,
        delay: Duration,
        wake: Option<EventLoopProxy<WinitUserEvent>>,
        discover_repository: bool,
    ) -> GitStatusRefresh {
        file.generation = file.generation.wrapping_add(1).max(1);
        GitStatusRefresh {
            revision: file.generation,
            due: Instant::now() + delay,
            wake,
            discover_repository,
        }
    }

    fn clear_diff(file: &mut FileGit) {
        file.state.diff_loading = false;
        file.state.diff_viewport = None;
        file.state.diff_rows.clear();
        file.revealed_diff.0.clear();
    }

    fn project_diff(file: &mut FileGit) {
        let Some(viewport) = file.state.diff_viewport.as_ref() else {
            file.state.diff_rows.clear();
            return;
        };
        let lines = &viewport.lines;
        let mut visible = vec![false; lines.len()];
        for (index, line) in lines.iter().enumerate() {
            if matches!(line.kind, DiffKind::Context) {
                continue;
            }
            let start = index.saturating_sub(DIFF_CONTEXT_LINES);
            let end = (index + DIFF_CONTEXT_LINES + 1).min(lines.len());
            visible[start..end].fill(true);
        }
        for (start, end) in &file.revealed_diff.0 {
            let start = (*start).min(lines.len());
            let end = (*end).min(lines.len());
            if start < end {
                visible[start..end].fill(true);
            }
        }

        let mut rows = Vec::new();
        let mut index = 0;
        while index < lines.len() {
            if visible[index] {
                rows.push(GitDiffRow::Line(index as u32));
                index += 1;
                continue;
            }
            let start = index;
            while index < lines.len() && !visible[index] {
                index += 1;
            }
            let hidden = index - start;
            let chunk = GitDiffRevealRequest::CHUNK as usize;
            let (reveal_start, reveal_end) = if hidden <= chunk {
                (start, index)
            } else if start == 0 {
                (index - chunk, index)
            } else {
                (start, start + chunk)
            };
            rows.push(GitDiffRow::Gap {
                start: start as u32,
                end: index as u32,
                reveal_start: reveal_start as u32,
                reveal_end: reveal_end as u32,
            });
        }
        file.state.diff_rows = rows;
    }
}

#[derive(Component)]
struct FileBranchesTask {
    webview: Entity,
    document: u64,
    repo_root: String,
    branch: String,
    task: GitTask<Result<Vec<crate::event::FileGitBranch>, String>>,
}

#[derive(Component)]
struct PendingFileBranches;

fn load_branches(
    trigger: On<UiInput<FileGitBranchesRequest>>,
    files: Query<&FileGit>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(file) = files.get(webview) else {
        return;
    };
    if file.state.branches_loading || file.state.repo_root.is_empty() {
        return;
    }
    commands.entity(webview).try_insert(PendingFileBranches);
}

fn start_branch_tasks(
    mut files: Query<(Entity, &mut FileGit), With<PendingFileBranches>>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (webview, mut file) in &mut files {
        if file.state.branches_loading || file.state.repo_root.is_empty() {
            commands.entity(webview).remove::<PendingFileBranches>();
            continue;
        }
        let document = file.document;
        let repo_root = file.state.repo_root.clone();
        let branch = file.state.branch.clone();
        let root = PathBuf::from(&repo_root);
        let Some(task) = runtime.spawn(wake.clone(), move || {
            GitRepository::at(root)
                .file_branches()
                .map_err(|error| error.0)
        }) else {
            continue;
        };
        file.state.branches_loading = true;
        commands.entity(webview).remove::<PendingFileBranches>();
        commands.spawn(FileBranchesTask {
            webview,
            document,
            repo_root,
            branch,
            task,
        });
    }
}

fn poll_branch_tasks(
    mut tasks: Query<(Entity, &mut FileBranchesTask)>,
    mut files: Query<&mut FileGit>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(result) = task.task.poll() else {
            continue;
        };
        if let Ok(mut file) = files.get_mut(task.webview)
            && file.document == task.document
            && file.state.repo_root == task.repo_root
            && file.state.branch == task.branch
        {
            file.state.branches_loading = false;
            match result {
                Ok(Ok(branches)) => {
                    file.state.branches = branches;
                    file.state.branches_loaded = true;
                }
                Ok(Err(message)) => file.state.message = message,
                Err(_) => {}
            }
        }
        commands.entity(entity).despawn();
    }
}

fn edit_commit(trigger: On<UiInput<GitCommitDraftRequest>>, mut files: Query<&mut FileGit>) {
    let Ok(mut file) = files.get_mut(trigger.event().webview) else {
        return;
    };
    file.state
        .commit_message
        .clone_from(&trigger.event().payload.message);
}

fn submit_commit(
    trigger: On<UiInput<GitCommitSubmitRequest>>,
    mut files: Query<&mut FileGit>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let Ok(mut file) = files.get_mut(webview) else {
        return;
    };
    let message = file.state.commit_message.trim().to_string();
    if message.is_empty() || file.state.staged_count == 0 || !file.state.commit_pending.is_empty() {
        return;
    }
    let path = file.state.repo_root.clone();
    if path.is_empty() {
        return;
    }
    file.state.commit_pending.clone_from(&message);
    commands.trigger(UiInput {
        webview,
        payload: GitCommitRequest { path, message },
    });
}

#[derive(Component)]
pub(super) struct GitStatusRefresh {
    revision: u64,
    due: Instant,
    wake: Option<EventLoopProxy<WinitUserEvent>>,
    discover_repository: bool,
}

#[derive(Resource)]
struct GitStatusScheduler {
    sender: mpsc::Sender<ScheduledGitStatusWake>,
}

struct ScheduledGitStatusWake {
    target: Entity,
    revision: u64,
    due: Instant,
    wake: Option<EventLoopProxy<WinitUserEvent>>,
}

impl PartialEq for ScheduledGitStatusWake {
    fn eq(&self, other: &Self) -> bool {
        self.due == other.due && self.target == other.target && self.revision == other.revision
    }
}

impl Eq for ScheduledGitStatusWake {}

impl PartialOrd for ScheduledGitStatusWake {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledGitStatusWake {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.due
            .cmp(&other.due)
            .then(self.target.cmp(&other.target))
            .then(self.revision.cmp(&other.revision))
    }
}

impl Default for GitStatusScheduler {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("vmux-git-status-debounce".to_string())
            .spawn(move || Self::run(receiver));
        Self { sender }
    }
}

impl GitStatusScheduler {
    fn schedule(&self, target: Entity, refresh: &GitStatusRefresh) {
        let _ = self.sender.send(ScheduledGitStatusWake {
            target,
            revision: refresh.revision,
            due: refresh.due,
            wake: refresh.wake.clone(),
        });
    }

    fn run(receiver: mpsc::Receiver<ScheduledGitStatusWake>) {
        let mut pending = BinaryHeap::<Reverse<ScheduledGitStatusWake>>::new();
        let mut latest = HashMap::<Entity, u64>::new();
        loop {
            let received = match pending.peek() {
                Some(Reverse(next)) => {
                    let timeout = next.due.saturating_duration_since(Instant::now());
                    receiver.recv_timeout(timeout)
                }
                None => match receiver.recv() {
                    Ok(value) => {
                        latest.insert(value.target, value.revision);
                        pending.push(Reverse(value));
                        continue;
                    }
                    Err(_) => return,
                },
            };
            match received {
                Ok(value) => {
                    latest.insert(value.target, value.revision);
                    pending.push(Reverse(value));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let now = Instant::now();
                    let mut wake = None;
                    while pending.peek().is_some_and(|next| next.0.due <= now) {
                        let Some(Reverse(next)) = pending.pop() else {
                            break;
                        };
                        if latest.get(&next.target) != Some(&next.revision) {
                            continue;
                        }
                        latest.remove(&next.target);
                        wake = wake.or(next.wake);
                    }
                    if let Some(wake) = wake {
                        let _ = wake.send_event(WinitUserEvent::WakeUp);
                    }
                }
            }
        }
    }
}

#[derive(Component, Clone, Debug)]
struct PendingGitStatus {
    repo_root: Option<PathBuf>,
    path: PathBuf,
    document: u64,
    revision: u64,
    dirty: bool,
    prepare_watch: bool,
}

struct GitStatusRequestInput {
    webview: Entity,
    path: PathBuf,
    document: u64,
    revision: u64,
    dirty: bool,
    repo_root: Option<PathBuf>,
    prepare_watch: bool,
}

impl GitStatusRequestInput {
    fn identity(&self) -> GitStatusRequestIdentity {
        GitStatusRequestIdentity {
            webview: self.webview,
            document: self.document,
            revision: self.revision,
        }
    }

    fn non_repository(self) -> GitStatusResult {
        let status = GitRepository::non_repository_status(&self.path);
        GitStatusResult {
            identity: self.identity(),
            status: Ok(status),
            watch: Ok(None),
        }
    }

    fn failed(self, message: String) -> GitStatusResult {
        GitStatusResult {
            identity: self.identity(),
            status: Err(message),
            watch: Ok(None),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum GitStatusBatchKey {
    Repository(PathBuf),
    Discover,
}

#[derive(Component)]
struct GitStatusTask {
    key: GitStatusBatchKey,
    requests: Vec<GitStatusRequestIdentity>,
    task: GitTask<GitStatusResults>,
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
    watch: Result<Option<super::watch::GitWatchSubscription>, String>,
}

struct GitStatusResults {
    values: Vec<GitStatusResult>,
}

impl GitStatusResults {
    fn read(requests: Vec<GitStatusRequestInput>) -> Self {
        let mut values = Vec::new();
        let mut repositories =
            HashMap::<PathBuf, (GitRepository, Vec<GitStatusRequestInput>)>::new();
        for request in requests {
            let repository = match request.repo_root.as_ref() {
                Some(root) => Ok(GitRepository::at(root.clone())),
                None => GitRepository::discover(&request.path),
            };
            match repository {
                Ok(repository) => {
                    let key = PathIdentity::resolve(repository.path()).into_path_buf();
                    repositories
                        .entry(key)
                        .or_insert_with(|| (repository, Vec::new()))
                        .1
                        .push(request);
                }
                Err(error) if GitRepository::has_repository(&request.path) => {
                    values.push(request.failed(error.0));
                }
                Err(_) => values.push(request.non_repository()),
            }
        }
        for (_, (repository, requests)) in repositories {
            values.extend(Self::read_repository(repository, requests));
        }
        Self { values }
    }

    fn read_repository(
        repository: GitRepository,
        requests: Vec<GitStatusRequestInput>,
    ) -> Vec<GitStatusResult> {
        let prepare_watch = requests.iter().any(|request| request.prepare_watch);
        let watch = if prepare_watch {
            super::watch::GitWatchSubscription::read(repository.path())
                .map(Some)
                .map_err(|error| error.0)
        } else {
            Ok(None)
        };
        let paths = requests
            .iter()
            .map(|request| request.path.clone())
            .collect::<Vec<_>>();
        match repository.statuses(&paths) {
            Ok(events) => requests
                .into_iter()
                .zip(events)
                .map(|(request, mut status)| {
                    if request.dirty {
                        status.file_status = match status.file_status {
                            FileStatus::Clean => FileStatus::Modified,
                            FileStatus::Staged => FileStatus::StagedModified,
                            value => value,
                        };
                    }
                    GitStatusResult {
                        identity: request.identity(),
                        status: Ok(status),
                        watch: watch.clone(),
                    }
                })
                .collect(),
            Err(error) => requests
                .into_iter()
                .map(|request| GitStatusResult {
                    identity: request.identity(),
                    status: Err(error.0.clone()),
                    watch: watch.clone(),
                })
                .collect(),
        }
    }
}

type ChangedDiffSource<'a> = (Entity, Ref<'a, GitDiffSource>);
type ChangedDiffSourceFilter = (Changed<GitDiffSource>, With<FileGit>);

#[derive(SystemParam)]
struct ChangedDiffSources<'w, 's> {
    values: Query<'w, 's, ChangedDiffSource<'static>, ChangedDiffSourceFilter>,
}

fn file_added(
    trigger: On<Add, FileGit>,
    mut files: FileGitStates,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(refresh) = files.schedule(
        trigger.entity,
        Duration::ZERO,
        wake.as_deref().map(|wake| (**wake).clone()),
        true,
    ) else {
        return;
    };
    commands.entity(trigger.entity).insert(refresh);
}

fn refresh_changed_sources(
    sources: ChangedDiffSources,
    mut files: FileGitStates,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, source) in &sources.values {
        if source.is_added() {
            continue;
        }
        if let Some(refresh) = files.schedule(entity, STATUS_DEBOUNCE, wake.clone(), false) {
            commands.entity(entity).insert(refresh);
        }
    }
}

fn start_refreshes(
    refreshes: Query<(Entity, &GitStatusRefresh), Changed<GitStatusRefresh>>,
    scheduler: Res<GitStatusScheduler>,
) {
    for (entity, refresh) in &refreshes {
        scheduler.schedule(entity, refresh);
    }
}

fn poll_refreshes(
    refreshes: Query<(Entity, &GitStatusRefresh, Option<&GitDiffSource>), With<FileGit>>,
    mut files: FileGitStates,
    mut commands: Commands,
) {
    for (entity, refresh, source) in &refreshes {
        if Instant::now() < refresh.due {
            continue;
        }
        commands.entity(entity).remove::<GitStatusRefresh>();
        let Some((path, document, repo_root)) =
            files.settle(entity, refresh.revision, refresh.discover_repository)
        else {
            continue;
        };
        let prepare_watch = refresh.discover_repository || repo_root.is_none();
        commands.entity(entity).insert(PendingGitStatus {
            repo_root,
            path,
            document,
            revision: refresh.revision,
            dirty: source.is_some_and(|source| source.dirty),
            prepare_watch,
        });
    }
}

fn poll_tasks(
    mut tasks: Query<(Entity, &mut GitStatusTask)>,
    mut files: FileGitStates,
    mut watch: Option<NonSendMut<GitWatch>>,
    mut commands: Commands,
) {
    for (entity, mut task) in &mut tasks {
        let Some(output) = task.task.poll() else {
            continue;
        };
        let results = match output {
            Ok(results) => results,
            Err(_) => GitStatusResults {
                values: task
                    .requests
                    .iter()
                    .copied()
                    .map(|identity| GitStatusResult {
                        identity,
                        status: Err("Git status worker panicked".to_string()),
                        watch: Ok(None),
                    })
                    .collect(),
            },
        };
        for result in results.values {
            if !files.accepts(
                result.identity.webview,
                result.identity.document,
                result.identity.revision,
            ) {
                continue;
            }
            match result.status {
                Ok(status) => {
                    let path = status.path.clone();
                    files.apply_status(result.identity.webview, status);
                    let watch_error = match result.watch {
                        Ok(Some(subscription)) => watch.as_deref_mut().and_then(|watch| {
                            watch
                                .subscribe_file(
                                    result.identity.webview,
                                    Path::new(&path),
                                    subscription,
                                )
                                .err()
                                .map(|error| error.0)
                        }),
                        Ok(None) => None,
                        Err(message) => Some(message),
                    };
                    if let Some(message) = watch_error {
                        files.apply_error(result.identity.webview, message);
                    }
                    commands.trigger(super::diff::FileDiffRefresh {
                        entity: result.identity.webview,
                    });
                }
                Err(message) => files.apply_error(result.identity.webview, message),
            }
        }
        commands.entity(entity).despawn();
    }
}

type OrphanedGitStatusFilter = (
    Or<(With<GitStatusRefresh>, With<PendingGitStatus>)>,
    Without<FileGit>,
);

fn cancel_stale_tasks(
    refreshes: Query<Entity, OrphanedGitStatusFilter>,
    tasks: Query<(Entity, &GitStatusTask)>,
    files: Query<&FileGit>,
    mut commands: Commands,
) {
    for entity in &refreshes {
        commands
            .entity(entity)
            .remove::<(GitStatusRefresh, PendingGitStatus)>();
    }
    for (entity, task) in &tasks {
        let current = task.requests.iter().any(|identity| {
            files
                .get(identity.webview)
                .is_ok_and(|file| file.accepts(identity.document, identity.revision))
        });
        if !current {
            commands.entity(entity).despawn();
        }
    }
}

fn dispatch_requests(
    pending: Query<(Entity, &PendingGitStatus)>,
    tasks: Query<&GitStatusTask>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    runtime: Res<GitRuntime>,
    mut commands: Commands,
) {
    let active: HashSet<GitStatusBatchKey> = tasks.iter().map(|task| task.key.clone()).collect();
    let mut batches: HashMap<GitStatusBatchKey, Vec<GitStatusRequestInput>> = HashMap::new();
    for (webview, request) in &pending {
        let key = match request.repo_root.as_ref() {
            Some(root) => {
                GitStatusBatchKey::Repository(PathIdentity::lexical(root).into_path_buf())
            }
            None => GitStatusBatchKey::Discover,
        };
        if active.contains(&key) {
            continue;
        }
        batches.entry(key).or_default().push(GitStatusRequestInput {
            webview,
            path: request.path.clone(),
            document: request.document,
            revision: request.revision,
            dirty: request.dirty,
            repo_root: request.repo_root.clone(),
            prepare_watch: request.prepare_watch,
        });
    }
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (key, requests) in batches {
        let identities = requests
            .iter()
            .map(GitStatusRequestInput::identity)
            .collect::<Vec<_>>();
        let pending = identities
            .iter()
            .map(|identity| identity.webview)
            .collect::<Vec<_>>();
        let Some(task) = runtime.spawn(wake.clone(), move || GitStatusResults::read(requests))
        else {
            continue;
        };
        for webview in pending {
            commands.entity(webview).remove::<PendingGitStatus>();
        }
        commands.spawn(GitStatusTask {
            key,
            requests: identities,
            task,
        });
    }
}

fn publish_file_state(
    files: Query<(Entity, &FileGit), Changed<FileGit>>,
    pages: Query<(), With<UiFileUpdates>>,
    mut commands: Commands,
) {
    for (entity, file) in &files {
        if pages.contains(entity) {
            commands.trigger(UiFileWrite::from_event(entity, &file.state));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::repository::test_repo;
    use bevy_cef::prelude::{
        BinHostEmitEvent, BinIpcEpoch, BinIpcNavigationRevision, BinIpcSurfaceGeneration, Browsers,
    };
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_api::{UiEventPermissions, UiState as _};
    use vmux_ecs::event::UiFile;

    #[derive(Resource, Default)]
    struct Emitted(Vec<UiFile>);

    #[derive(Resource)]
    struct TestDiff(Option<GitDiffViewport>);

    #[derive(Message)]
    struct TestReveal {
        entity: Entity,
        start: u32,
        end: u32,
    }

    impl Emitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != UiFile::id() {
                return;
            }
            trigger.event().mark_delivered();
            let frame = rkyv::from_bytes::<
                UiStateFrame<UiFile, <UiFile as vmux_api::UiState>::Update>,
                rkyv::rancor::Error,
            >(trigger.event().payload())
            .unwrap();
            let previous = emitted.0.pop();
            let state = match frame {
                UiStateFrame::Snapshot(state) => state,
                UiStateFrame::Updates(updates) => UiFile::from_updates(previous, updates),
            };
            emitted.0.push(state);
        }
    }

    fn project_test_diff(
        entities: Query<Entity, With<FileGit>>,
        mut files: FileGitStates,
        mut diff: ResMut<TestDiff>,
    ) {
        let Some(diff) = diff.0.take() else {
            return;
        };
        for entity in &entities {
            files.apply_diff(entity, diff.clone());
        }
    }

    fn reveal_test_diff(mut requests: MessageReader<TestReveal>, mut files: FileGitStates) {
        for request in requests.read() {
            files.reveal_diff(
                request.entity,
                &GitDiffRevealRequest {
                    generation: 1,
                    revision: 1,
                    start: request.start,
                    end: request.end,
                },
            );
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
    fn file_diff_projection_is_published_with_file_state() {
        let mut lines = Vec::new();
        for number in 1..=20 {
            lines.push(crate::event::DiffLine {
                kind: DiffKind::Context,
                old_no: Some(number),
                new_no: Some(number),
                hunk: None,
                spans: Vec::new(),
            });
        }
        lines[9].kind = DiffKind::Add;
        let mut app = App::new();
        app.insert_resource(TestDiff(Some(GitDiffViewport {
            generation: 1,
            revision: 1,
            first_line: 0,
            total_lines: 20,
            has_more: false,
            lines,
            markers: Vec::new(),
            error: String::new(),
        })))
        .add_message::<TestReveal>()
        .add_systems(Update, (project_test_diff, reveal_test_diff).chain());
        let entity = app.world_mut().spawn(FileGit::new("/repo/a.rs", 7)).id();
        app.update();
        assert_eq!(
            app.world()
                .get::<FileGit>(entity)
                .unwrap()
                .state
                .diff_rows
                .first(),
            Some(&GitDiffRow::Gap {
                start: 0,
                end: 6,
                reveal_start: 0,
                reveal_end: 6,
            })
        );
        assert!(
            app.world()
                .get::<FileGit>(entity)
                .unwrap()
                .state
                .diff_rows
                .contains(&GitDiffRow::Line(9))
        );
        app.world_mut().write_message(TestReveal {
            entity,
            start: 0,
            end: 6,
        });
        app.update();
        let file = app.world().get::<FileGit>(entity).unwrap();
        assert!(file.state.diff_rows.contains(&GitDiffRow::Line(0)));
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
            vmux_ecs::UiStatePlugin::<UiFile>::default(),
            StatusPlugin,
        ))
        .init_resource::<Emitted>()
        .add_observer(Emitted::record);
        let entity = app
            .world_mut()
            .spawn((
                UiFileUpdates::default(),
                FileGit::new(&file, 1),
                GitDiffSource::default(),
                vmux_ecs::PageMetadata {
                    url: format!("file://{}", file.to_string_lossy()),
                    ..Default::default()
                },
                BinIpcSurfaceGeneration(1),
                BinIpcNavigationRevision(1),
                BinIpcEpoch(1),
            ))
            .id();
        app.world_mut().spawn(UiEventPermissions {
            url: "file://",
            owns_subtree: true,
            permissions: &[UiFile::PERMISSION],
        });
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        for _ in 0..10_000 {
            app.update();
            let published = app.world().resource::<Emitted>().0.iter().any(|state| {
                state
                    .git
                    .git_state
                    .as_ref()
                    .is_some_and(|state| state.path == file.to_string_lossy() && state.has_diff)
            });
            if published {
                return;
            }
            std::thread::yield_now();
        }
        panic!("file git status did not publish");
    }
}
