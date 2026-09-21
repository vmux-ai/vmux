pub mod highlight;
pub mod job;
pub mod parse;
pub mod runner;
pub mod worktree;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::host::page::NativelyHosted;
use vmux_core::{PageOpenRequest, PageOpenTarget};

use crate::event::{
    GIT_CHANGED_EVENT, GIT_DIRECTORY_EVENT, GIT_REPOSITORY_PICKED_EVENT, GitAppAction,
    GitAppActionRequest, GitBranchLogRequest, GitChangedEvent, GitCommitRequest, GitDiffRequest,
    GitDirectoryEvent, GitDirectoryRequest, GitDiscardRequest, GitFetchRequest, GitHunkRequest,
    GitOperationRequest, GitPullRequest, GitPushRequest, GitRepositoryPickedEvent,
    GitRepositoryPickerRequest, GitRepositoryRequest, GitStageAllRequest, GitStageRequest,
    GitStatusRequest, GitUnstageRequest,
};
use crate::host::job::{Emit, JobKind, emit_event_name, run_job};

pub struct GitPlugin;

#[derive(Message, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitCheckForUpdatesRequest;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn((
            PAGE_MANIFEST,
            NativelyHosted::subtree(crate::GIT_PAGE_URL, "Git"),
        ));
        app.world_mut()
            .spawn(NativelyHosted::page(crate::GIT_DOCUMENT_URL, "Git"));
        vmux_core::register_host_spawn(app, "git");
        vmux_core::register_scheme_spawn(app, "git");
        let (tx, rx) = mpsc::channel();
        let proxy = app
            .world()
            .get_resource::<bevy::winit::EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if !should_forward_git_watch_result(&result) {
                return;
            }
            let _ = tx.send(result);
            if let Some(proxy) = proxy.as_ref() {
                let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
            }
        }) {
            Ok(watcher) => {
                app.insert_non_send(GitWatch {
                    watcher,
                    rx,
                    watch_references: HashMap::new(),
                    subscriptions: HashMap::new(),
                    repo_info_subscriptions: HashMap::new(),
                });
            }
            Err(error) => bevy::log::warn!("git watcher init failed: {error}"),
        }
        let repo_info_wake = app
            .world()
            .get_resource::<EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        app.init_resource::<GitOutbox>()
            .init_resource::<GitStatusJobs>()
            .add_message::<GitCheckForUpdatesRequest>()
            .insert_resource(RepoInfoCache {
                entries: HashMap::new(),
                canonical: HashMap::new(),
                guessed: HashMap::new(),
                wake: repo_info_wake,
            })
            .add_plugins(BinEventEmitterPlugin::<(
                GitRepositoryRequest,
                GitRepositoryPickerRequest,
                GitBranchLogRequest,
                GitDirectoryRequest,
                GitStatusRequest,
                GitDiffRequest,
                GitStageRequest,
                GitUnstageRequest,
                GitDiscardRequest,
                GitCommitRequest,
                GitPushRequest,
                GitHunkRequest,
            )>::default())
            .add_plugins(BinEventEmitterPlugin::<(
                GitFetchRequest,
                GitAppActionRequest,
                GitOperationRequest,
                GitPullRequest,
                GitStageAllRequest,
            )>::default())
            .add_observer(on_repository_request)
            .add_observer(on_repository_picker_request)
            .add_observer(on_app_action_request)
            .add_observer(on_branch_log_request)
            .add_observer(on_directory_request)
            .add_observer(on_status_request)
            .add_observer(on_diff_request)
            .add_observer(on_stage_request)
            .add_observer(on_unstage_request)
            .add_observer(on_discard_request)
            .add_observer(on_commit_request)
            .add_observer(on_fetch_request)
            .add_observer(on_operation_request)
            .add_observer(on_pull_request)
            .add_observer(on_push_request)
            .add_observer(on_stage_all_request)
            .add_observer(on_hunk_request)
            .add_systems(
                Update,
                (
                    drain_git_watch,
                    poll_repo_info_cache,
                    poll_repository_pickers,
                    sync_repo_info_watches,
                    drain_git_outbox,
                    dispatch_status_jobs,
                )
                    .chain(),
            );
    }
}

pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "git",
    title: "Git",
    title_message_id: Some("git-title"),
    replaces_command: None,
    keywords: &[
        "repository",
        "changes",
        "commit",
        "branch",
        "source control",
    ],
    icon: Some(vmux_core::BuiltinIcon::GitBranch),
    command_bar: true,
};

#[derive(Component, Clone, Debug, Default)]
pub struct GitDiffSource {
    pub content: String,
    pub dirty: bool,
}

struct GitDirectory;

impl GitDirectory {
    fn initial_path(path: &Path) -> PathBuf {
        let mut current = path.to_path_buf();
        while !current.is_dir() && current.pop() {}
        if current.is_dir() && !current.as_os_str().is_empty() {
            return current;
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn entries(path: &Path) -> Vec<vmux_core::event::FileDirEntry> {
        let Ok(read) = std::fs::read_dir(path) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .map(|kind| {
                    kind.is_dir()
                        || kind.is_symlink()
                            && std::fs::metadata(&path)
                                .map(|metadata| metadata.is_dir())
                                .unwrap_or(false)
                })
                .unwrap_or(false);
            entries.push(vmux_core::event::FileDirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                is_dir,
            });
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        entries
    }

    fn event(path: &Path, preview: bool) -> GitDirectoryEvent {
        let path = Self::initial_path(path);
        let parent = path.parent().map(Path::to_path_buf);
        let parent_path = parent
            .as_ref()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parent_entries = parent.as_deref().map(Self::entries).unwrap_or_default();
        let repo_root = crate::host::runner::repo_root(&path)
            .map(|root| root.to_string_lossy().into_owned())
            .unwrap_or_default();
        GitDirectoryEvent {
            entries: Self::entries(&path),
            path: path.to_string_lossy().into_owned(),
            parent_path,
            parent_entries,
            repo_root,
            preview,
        }
    }
}

#[derive(Component)]
struct PendingGitRepositoryPicker {
    webview: Entity,
    task: Task<Option<PathBuf>>,
}

struct GitRepositoryPicker;

impl GitRepositoryPicker {
    fn initial_directory(path: &Path) -> PathBuf {
        let mut current = path.to_path_buf();
        while !current.is_dir() && current.pop() {}
        if current.is_dir() {
            return current;
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn task(
        path: PathBuf,
        proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> Task<Option<PathBuf>> {
        let initial = Self::initial_directory(&path);
        IoTaskPool::get().spawn(async move {
            let selected = rfd::AsyncFileDialog::new()
                .set_title("Choose Git repository")
                .set_directory(initial)
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf());
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
            selected
        })
    }
}

pub enum GitOutboxItem {
    Events {
        webview: Entity,
        emits: Vec<Emit>,
    },
    StatusBatch {
        repo_root: PathBuf,
        results: Vec<(Entity, Vec<Emit>)>,
    },
}

pub type OutboxQueue = Vec<GitOutboxItem>;

#[derive(Resource, Clone, Default)]
pub struct GitOutbox(pub Arc<Mutex<OutboxQueue>>);

#[derive(Clone, Debug)]
struct PendingStatusRequest {
    webview: Entity,
    path: PathBuf,
    dirty: bool,
}

#[derive(Resource, Default)]
struct GitStatusJobs {
    pending: HashMap<PathBuf, HashMap<Entity, PendingStatusRequest>>,
    in_flight: HashSet<PathBuf>,
}

impl GitStatusJobs {
    fn queue(&mut self, repo_root: PathBuf, request: PendingStatusRequest) {
        self.pending
            .entry(repo_root)
            .or_default()
            .insert(request.webview, request);
    }

    fn take_ready(&mut self) -> Vec<(PathBuf, Vec<PendingStatusRequest>)> {
        let roots: Vec<PathBuf> = self
            .pending
            .keys()
            .filter(|root| !self.in_flight.contains(*root))
            .cloned()
            .collect();
        roots
            .into_iter()
            .filter_map(|root| {
                let requests = self.pending.remove(&root)?;
                self.in_flight.insert(root.clone());
                Some((root, requests.into_values().collect()))
            })
            .collect()
    }

    fn complete(&mut self, repo_root: &Path) {
        self.in_flight.remove(repo_root);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GitWatchTarget {
    path: PathBuf,
    recursive: bool,
    kind: GitWatchKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GitWatchKind {
    Worktree,
    Metadata,
}

struct GitSubscription {
    path: PathBuf,
    repo_root: PathBuf,
    targets: Vec<GitWatchTarget>,
    complete: bool,
}

struct GitWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    watch_references: HashMap<GitWatchTarget, usize>,
    subscriptions: HashMap<Entity, GitSubscription>,
    repo_info_subscriptions: HashMap<PathBuf, Vec<GitWatchTarget>>,
}

struct RepoInfoCacheEntry {
    info: Option<crate::host::worktree::RepoInfo>,
    loaded: bool,
    dirty: bool,
    watched: bool,
    idle_syncs: u8,
    pending: Option<Task<Option<crate::host::worktree::RepoInfo>>>,
    ignore_events_until: Option<Instant>,
}

const UNRESOLVED_RETRY: Duration = Duration::from_secs(1);

struct GuessedPath {
    guess: PathBuf,
    made_at: Instant,
}

impl GuessedPath {
    fn of(path: &Path) -> Self {
        Self {
            guess: canon(path),
            made_at: Instant::now(),
        }
    }

    fn worth_reusing(&self) -> bool {
        self.made_at.elapsed() < UNRESOLVED_RETRY
    }
}

#[derive(Resource)]
pub struct RepoInfoCache {
    entries: HashMap<PathBuf, RepoInfoCacheEntry>,
    canonical: HashMap<PathBuf, PathBuf>,
    guessed: HashMap<PathBuf, GuessedPath>,
    wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
}

const WATCH_DRAIN_BUDGET: usize = 256;
const CANONICAL_CAP: usize = 4096;

impl RepoInfoCache {
    fn wake_soon(&self) {
        let Some(wake) = &self.wake else {
            return;
        };
        let _ = wake.send_event(WinitUserEvent::WakeUp);
    }

    fn canonical_path(&mut self, path: &Path) -> PathBuf {
        if let Some(known) = self.canonical.get(path) {
            return known.clone();
        }
        if self.canonical.len() >= CANONICAL_CAP {
            self.canonical.clear();
        }
        if let Some(guessed) = self.guessed.get(path)
            && guessed.worth_reusing()
        {
            return guessed.guess.clone();
        }
        let Ok(resolved) = path.canonicalize() else {
            let guessed = GuessedPath::of(path);
            let guess = guessed.guess.clone();
            self.guessed.insert(path.to_path_buf(), guessed);
            return guess;
        };
        self.guessed.remove(path);
        self.canonical.insert(path.to_path_buf(), resolved.clone());
        resolved
    }

    pub fn get(&mut self, path: &Path) -> Option<crate::host::worktree::RepoInfo> {
        let path = self.canonical_path(path);
        let wake = self.wake.clone();
        let entry = self
            .entries
            .entry(path.clone())
            .or_insert_with(|| RepoInfoCacheEntry {
                info: None,
                loaded: false,
                dirty: true,
                watched: false,
                idle_syncs: 0,
                pending: None,
                ignore_events_until: None,
            });
        entry.idle_syncs = 0;
        Self::poll_and_refresh(&path, entry, wake);
        entry.info.clone()
    }

    fn poll_and_refresh(
        path: &Path,
        entry: &mut RepoInfoCacheEntry,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) {
        if let Some(task) = entry.pending.as_mut()
            && let Some(info) = future::block_on(future::poll_once(task))
        {
            entry.info = info;
            entry.loaded = true;
            entry.watched = false;
            entry.idle_syncs = 0;
            entry.pending = None;
            entry.ignore_events_until = Some(Instant::now() + Duration::from_millis(500));
        }
        if entry.pending.is_none() && (entry.dirty || !entry.loaded) {
            entry.dirty = false;
            let path = path.to_path_buf();
            let delay = entry
                .ignore_events_until
                .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
                .unwrap_or_default();
            entry.pending = Some(IoTaskPool::get().spawn(async move {
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                let info = crate::host::worktree::repo_info(&path);
                if let Some(wake) = wake {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
                info
            }));
        }
    }

    fn poll(&mut self) {
        let wake = self.wake.clone();
        for (path, entry) in &mut self.entries {
            Self::poll_and_refresh(path, entry, wake.clone());
        }
    }

    fn invalidate(&mut self, path: &Path) {
        if let Some(entry) = self.entries.get_mut(path) {
            entry.dirty = true;
        }
    }

    fn inactive_paths(&mut self) -> Vec<PathBuf> {
        let mut inactive = Vec::new();
        for (path, entry) in &mut self.entries {
            if entry.pending.is_some() {
                continue;
            }
            entry.idle_syncs = entry.idle_syncs.saturating_add(1);
            if entry.idle_syncs > 1 {
                inactive.push(path.clone());
            }
        }
        inactive
    }

    fn remove(&mut self, path: &Path) {
        self.entries.remove(path);
        self.canonical.retain(|_, canonical| canonical != path);
        self.guessed.remove(path);
    }
}

fn canon(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf())
                .join(name),
            _ => path.to_path_buf(),
        })
}

fn resolve_git_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    canon(&path)
}

fn git_watch_targets(
    file: &Path,
) -> Result<(PathBuf, Vec<GitWatchTarget>), crate::host::runner::GitError> {
    let root = crate::host::runner::repo_root(file)?;
    let (stdout, stderr, ok) = crate::host::runner::git(
        &root,
        &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
    )?;
    if !ok {
        return Err(crate::host::runner::git_err(&stdout, &stderr));
    }
    let mut lines = stdout.lines();
    let git_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| crate::host::runner::GitError("missing git directory".into()))?;
    let common_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| crate::host::runner::GitError("missing common git directory".into()))?;
    let mut targets = vec![
        GitWatchTarget {
            path: canon(&root),
            recursive: true,
            kind: GitWatchKind::Worktree,
        },
        GitWatchTarget {
            path: git_dir.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        },
    ];
    if common_dir != git_dir {
        targets.push(GitWatchTarget {
            path: common_dir.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        });
    }
    targets.push(GitWatchTarget {
        path: common_dir.join("refs"),
        recursive: true,
        kind: GitWatchKind::Metadata,
    });
    Ok((root, targets))
}

fn is_git_lock_path(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".lock"))
}

impl GitWatchTarget {
    fn matches(&self, changed: &Path) -> bool {
        let matches = changed == self.path
            || if self.recursive {
                changed.starts_with(&self.path)
            } else {
                changed.parent() == Some(self.path.as_path())
            };
        if !matches {
            return false;
        }
        match self.kind {
            GitWatchKind::Metadata => !is_git_lock_path(changed),
            GitWatchKind::Worktree => {
                changed
                    .strip_prefix(&self.path)
                    .ok()
                    .is_none_or(|relative| {
                        !relative
                            .components()
                            .any(|component| component.as_os_str() == ".git")
                    })
            }
        }
    }
}

fn repo_info_watch_targets(
    path: &Path,
    info: Option<&crate::host::worktree::RepoInfo>,
) -> Vec<GitWatchTarget> {
    let Some(info) = info else {
        return vec![GitWatchTarget {
            path: canon(path),
            recursive: true,
            kind: GitWatchKind::Worktree,
        }];
    };
    let repo_root = canon(&info.repo_root);
    let git_dir = canon(&info.git_dir);
    let common_dir = canon(&info.common_dir);
    let mut targets = vec![
        GitWatchTarget {
            path: repo_root,
            recursive: true,
            kind: GitWatchKind::Worktree,
        },
        GitWatchTarget {
            path: git_dir.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        },
    ];
    if common_dir != git_dir {
        targets.push(GitWatchTarget {
            path: common_dir.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        });
    }
    targets.push(GitWatchTarget {
        path: common_dir.join("refs"),
        recursive: true,
        kind: GitWatchKind::Metadata,
    });
    targets
}

fn should_forward_git_watch_result(result: &notify::Result<notify::Event>) -> bool {
    match result {
        Ok(event) => {
            !matches!(event.kind, EventKind::Access(_))
                && event.paths.iter().any(|path| !is_git_lock_path(path))
        }
        Err(_) => true,
    }
}

impl GitWatch {
    #[cfg(test)]
    fn test() -> Self {
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |result| {
            let _ = tx.send(result);
        })
        .unwrap();
        Self {
            watcher,
            rx,
            watch_references: HashMap::new(),
            subscriptions: HashMap::new(),
            repo_info_subscriptions: HashMap::new(),
        }
    }

    fn acquire_targets(&mut self, targets: &[GitWatchTarget]) -> bool {
        let mut acquired = Vec::new();
        for target in targets {
            if let Some(references) = self.watch_references.get_mut(target) {
                *references += 1;
                acquired.push(target.clone());
                continue;
            }
            let mode = if target.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if self.watcher.watch(&target.path, mode).is_err() {
                self.release_targets(&acquired);
                return false;
            }
            self.watch_references.insert(target.clone(), 1);
            acquired.push(target.clone());
        }
        true
    }

    fn release_targets(&mut self, targets: &[GitWatchTarget]) {
        for target in targets {
            let remove = match self.watch_references.get_mut(target) {
                Some(references) if *references > 1 => {
                    *references -= 1;
                    false
                }
                Some(_) => true,
                None => false,
            };
            if !remove {
                continue;
            }
            self.watch_references.remove(target);
            if self
                .watch_references
                .keys()
                .all(|other| other.path != target.path)
            {
                let _ = self.watcher.unwatch(&target.path);
            }
        }
    }

    fn evict_inactive_repo_info(&mut self, repo_info: &mut RepoInfoCache) {
        for path in repo_info.inactive_paths() {
            if let Some(targets) = self.repo_info_subscriptions.remove(&path) {
                self.release_targets(&targets);
            }
            repo_info.remove(&path);
        }
    }

    fn subscribe(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Result<PathBuf, crate::host::runner::GitError> {
        let path = canon(path);
        if let Some(subscription) = self
            .subscriptions
            .get(&entity)
            .filter(|subscription| subscription.path == path && subscription.complete)
        {
            return Ok(subscription.repo_root.clone());
        }
        let (repo_root, targets) = match git_watch_targets(&path) {
            Ok(result) => result,
            Err(error) => {
                if let Some(previous) = self.subscriptions.remove(&entity) {
                    self.release_targets(&previous.targets);
                }
                return Err(error);
            }
        };
        let complete = self.acquire_targets(&targets);
        let previous = self.subscriptions.insert(
            entity,
            GitSubscription {
                path,
                repo_root: repo_root.clone(),
                targets,
                complete,
            },
        );
        if let Some(previous) = previous {
            self.release_targets(&previous.targets);
        }
        Ok(repo_root)
    }

    fn subscribe_repo_info(
        &mut self,
        path: &Path,
        info: Option<&crate::host::worktree::RepoInfo>,
    ) -> bool {
        let path = canon(path);
        let targets = repo_info_watch_targets(&path, info);
        if self.repo_info_subscriptions.get(&path) == Some(&targets) {
            return true;
        }
        if !self.acquire_targets(&targets) {
            return false;
        }
        let previous = self.repo_info_subscriptions.insert(path, targets);
        if let Some(previous) = previous {
            self.release_targets(&previous);
        }
        true
    }
}

fn spawn_job(outbox: &GitOutbox, webview: Entity, job: JobKind) {
    let sink = outbox.0.clone();
    std::thread::spawn(move || {
        let emits = run_job(job);
        sink.lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(GitOutboxItem::Events { webview, emits });
    });
}

fn spawn_status_batch(outbox: &GitOutbox, repo_root: PathBuf, requests: Vec<PendingStatusRequest>) {
    let sink = outbox.0.clone();
    std::thread::spawn(move || {
        let paths: Vec<PathBuf> = requests
            .iter()
            .map(|request| request.path.clone())
            .collect();
        let results = match crate::host::runner::statuses(&repo_root, &paths) {
            Ok(events) => requests
                .into_iter()
                .zip(events)
                .map(|(request, mut event)| {
                    if request.dirty {
                        event.file_status = match event.file_status {
                            crate::event::FileStatus::Clean => crate::event::FileStatus::Modified,
                            crate::event::FileStatus::Staged => {
                                crate::event::FileStatus::StagedModified
                            }
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
                        vec![Emit::Error(crate::event::GitErrorEvent {
                            message: error.0.clone(),
                        })],
                    )
                })
                .collect(),
        };
        sink.lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(GitOutboxItem::StatusBatch { repo_root, results });
    });
}

fn on_status_request(
    trigger: On<BinReceive<GitStatusRequest>>,
    sources: Query<&GitDiffSource>,
    outbox: Res<GitOutbox>,
    watch: Option<NonSendMut<GitWatch>>,
    mut jobs: ResMut<GitStatusJobs>,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    if !crate::host::runner::has_repository(&path) {
        outbox
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(GitOutboxItem::Events {
                webview,
                emits: vec![Emit::Status(crate::host::runner::non_repository_status(
                    &path,
                ))],
            });
        return;
    }
    let repo_root = if let Some(mut watch) = watch {
        watch.subscribe(webview, &path)
    } else {
        crate::host::runner::repo_root(&path)
    };
    match repo_root {
        Ok(repo_root) => jobs.queue(
            repo_root,
            PendingStatusRequest {
                webview,
                path,
                dirty: sources.get(webview).is_ok_and(|source| source.dirty),
            },
        ),
        Err(error) => {
            outbox
                .0
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(GitOutboxItem::Events {
                    webview,
                    emits: vec![Emit::Error(crate::event::GitErrorEvent {
                        message: error.0,
                    })],
                })
        }
    }
}

fn on_repository_request(
    trigger: On<BinReceive<GitRepositoryRequest>>,
    outbox: Res<GitOutbox>,
    watch: Option<NonSendMut<GitWatch>>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
) {
    let webview = trigger.event().webview;
    let path: PathBuf = trigger.event().payload.path.clone().into();
    let repo_root = if let Some(mut watch) = watch {
        match watch.subscribe(webview, &path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                outbox
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(GitOutboxItem::Events {
                        webview,
                        emits: vec![Emit::Error(crate::event::GitErrorEvent {
                            message: error.0,
                        })],
                    });
                return;
            }
        }
    } else {
        match crate::host::runner::repo_root(&path) {
            Ok(repo_root) => repo_root,
            Err(error) => {
                outbox
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(GitOutboxItem::Events {
                        webview,
                        emits: vec![Emit::Error(crate::event::GitErrorEvent {
                            message: error.0,
                        })],
                    });
                return;
            }
        }
    };
    if let Ok(mut page) = pages.get_mut(webview)
        && let Some(url) = crate::GitUrl::from_path(&repo_root)
        && page.url != url
    {
        page.url = url;
    }
    spawn_job(&outbox, webview, JobKind::Repository { path });
}

fn on_branch_log_request(trigger: On<BinReceive<GitBranchLogRequest>>, outbox: Res<GitOutbox>) {
    let request = &trigger.event().payload;
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::BranchLog {
            repo_root: request.repo_root.clone().into(),
            branch: request.branch.clone(),
        },
    );
}

fn on_app_action_request(
    trigger: On<BinReceive<GitAppActionRequest>>,
    child_of: Query<&ChildOf>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut update_requests: MessageWriter<GitCheckForUpdatesRequest>,
) {
    let target = child_of
        .get(trigger.event().webview)
        .ok()
        .and_then(|stack| child_of.get(stack.parent()).ok())
        .map(|pane| PageOpenTarget::NewStackInPane(pane.parent()))
        .unwrap_or(PageOpenTarget::ActiveStack);
    let url = match trigger.event().payload.action {
        GitAppAction::EditConfig => {
            let Ok(path) = runner::config_path(Path::new(&trigger.event().payload.repo_root))
            else {
                return;
            };
            let Ok(url) = url::Url::from_file_path(path) else {
                return;
            };
            url.to_string()
        }
        GitAppAction::CheckForUpdates => {
            update_requests.write(GitCheckForUpdatesRequest);
            return;
        }
    };
    page_open.write(PageOpenRequest {
        target,
        url,
        request_id: None,
    });
}

fn on_directory_request(
    trigger: On<BinReceive<GitDirectoryRequest>>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut commands: Commands,
) {
    let request = &trigger.event().payload;
    let event = GitDirectory::event(Path::new(&request.path), request.preview);
    if !request.preview
        && let Ok(mut page) = pages.get_mut(trigger.event().webview)
    {
        if let Some(url) = crate::GitUrl::from_path(Path::new(&event.path)) {
            page.url = url;
        }
        let name = Path::new(&event.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Git".to_string());
        page.title = format!("{name} · Git");
    }
    commands.trigger(BinHostEmitEvent::from_rkyv(
        trigger.event().webview,
        GIT_DIRECTORY_EVENT,
        &event,
    ));
}

fn on_repository_picker_request(
    trigger: On<BinReceive<GitRepositoryPickerRequest>>,
    pending: Query<&PendingGitRepositoryPicker>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    if pending.iter().any(|picker| picker.webview == webview) {
        return;
    }
    let path = PathBuf::from(&trigger.event().payload.path);
    let proxy = proxy.as_deref().map(|proxy| (**proxy).clone());
    commands.spawn(PendingGitRepositoryPicker {
        webview,
        task: GitRepositoryPicker::task(path, proxy),
    });
}

fn poll_repository_pickers(
    mut pending: Query<(Entity, &mut PendingGitRepositoryPicker)>,
    mut commands: Commands,
) {
    for (entity, mut picker) in &mut pending {
        let Some(selected) = future::block_on(future::poll_once(&mut picker.task)) else {
            continue;
        };
        if let Some(path) = selected {
            commands.trigger(BinHostEmitEvent::from_rkyv(
                picker.webview,
                GIT_REPOSITORY_PICKED_EVENT,
                &GitRepositoryPickedEvent {
                    path: path.to_string_lossy().into_owned(),
                },
            ));
        }
        commands.entity(entity).despawn();
    }
}

fn dispatch_status_jobs(mut jobs: ResMut<GitStatusJobs>, outbox: Res<GitOutbox>) {
    for (repo_root, requests) in jobs.take_ready() {
        spawn_status_batch(&outbox, repo_root, requests);
    }
}

fn drain_git_watch(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: ResMut<RepoInfoCache>,
    mut commands: Commands,
) {
    let Some(watch) = watch else {
        return;
    };
    let mut changed = HashSet::new();
    let mut drained = 0;
    while drained < WATCH_DRAIN_BUDGET {
        let Ok(result) = watch.rx.try_recv() else {
            break;
        };
        drained += 1;
        let Ok(event) = result else {
            continue;
        };
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        for path in event.paths {
            changed.insert(repo_info.canonical_path(&path));
        }
    }
    if drained == WATCH_DRAIN_BUDGET {
        repo_info.wake_soon();
    }
    if changed.is_empty() {
        return;
    }
    let affected: Vec<Entity> = watch
        .subscriptions
        .iter()
        .filter(|(_, subscription)| {
            subscription
                .targets
                .iter()
                .any(|target| changed.iter().any(|path| target.matches(path)))
        })
        .map(|(entity, _)| *entity)
        .collect();
    for entity in affected {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            GIT_CHANGED_EVENT,
            &GitChangedEvent {},
        ));
    }
    let affected_repo_info: Vec<PathBuf> = watch
        .repo_info_subscriptions
        .iter()
        .filter(|(_, targets)| {
            targets
                .iter()
                .any(|target| changed.iter().any(|path| target.matches(path)))
        })
        .map(|(path, _)| path.clone())
        .collect();
    for path in affected_repo_info {
        repo_info.invalidate(&path);
    }
}

fn poll_repo_info_cache(mut repo_info: ResMut<RepoInfoCache>) {
    repo_info.poll();
}

fn sync_repo_info_watches(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: ResMut<RepoInfoCache>,
) {
    let Some(mut watch) = watch else {
        for path in repo_info.inactive_paths() {
            repo_info.remove(&path);
        }
        return;
    };
    watch.evict_inactive_repo_info(&mut repo_info);
    let paths: Vec<PathBuf> = repo_info
        .entries
        .iter()
        .filter_map(|(path, entry)| (entry.loaded && !entry.watched).then_some(path.clone()))
        .collect();
    for path in paths {
        let info = repo_info
            .entries
            .get(&path)
            .and_then(|entry| entry.info.clone());
        let watched = watch.subscribe_repo_info(&path, info.as_ref());
        if let Some(entry) = repo_info.entries.get_mut(&path) {
            entry.watched = watched;
        }
    }
}

fn on_diff_request(
    trigger: On<BinReceive<GitDiffRequest>>,
    sources: Query<&GitDiffSource>,
    outbox: Res<GitOutbox>,
) {
    let p = &trigger.event().payload;
    let repo_root = PathBuf::from(&p.repo_root);
    let path = crate::host::runner::RequestPath::new(&p.path, &p.path_bytes).resolve(&repo_root);
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Diff {
            repo_root,
            path,
            reference: p.reference.clone(),
            generation: p.generation,
            top_line: p.top_line,
            rows: p.rows,
            content: sources
                .get(trigger.event().webview)
                .ok()
                .filter(|source| source.dirty)
                .map(|source| source.content.clone()),
        },
    );
}

fn on_stage_request(trigger: On<BinReceive<GitStageRequest>>, outbox: Res<GitOutbox>) {
    let payload = &trigger.event().payload;
    let repo_root = PathBuf::from(&payload.repo_root);
    let path = crate::host::runner::RequestPath::new(&payload.path, &payload.path_bytes)
        .resolve(&repo_root);
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Stage { repo_root, path },
    );
}

fn on_unstage_request(trigger: On<BinReceive<GitUnstageRequest>>, outbox: Res<GitOutbox>) {
    let payload = &trigger.event().payload;
    let repo_root = PathBuf::from(&payload.repo_root);
    let path = crate::host::runner::RequestPath::new(&payload.path, &payload.path_bytes)
        .resolve(&repo_root);
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Unstage { repo_root, path },
    );
}

fn on_discard_request(trigger: On<BinReceive<GitDiscardRequest>>, outbox: Res<GitOutbox>) {
    let payload = &trigger.event().payload;
    let repo_root = PathBuf::from(&payload.repo_root);
    let path = crate::host::runner::RequestPath::new(&payload.path, &payload.path_bytes)
        .resolve(&repo_root);
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Discard { repo_root, path },
    );
}

fn on_commit_request(trigger: On<BinReceive<GitCommitRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Commit {
            path: p.path.clone().into(),
            message: p.message.clone(),
        },
    );
}

fn on_fetch_request(trigger: On<BinReceive<GitFetchRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Fetch {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_operation_request(trigger: On<BinReceive<GitOperationRequest>>, outbox: Res<GitOutbox>) {
    let request = &trigger.event().payload;
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Operation {
            repo_root: request.repo_root.clone().into(),
            operation: request.operation.clone(),
        },
    );
}

fn on_pull_request(trigger: On<BinReceive<GitPullRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Pull {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_push_request(trigger: On<BinReceive<GitPushRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Push {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_stage_all_request(trigger: On<BinReceive<GitStageAllRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::StageAll {
            path: trigger.event().payload.path.clone().into(),
        },
    );
}

fn on_hunk_request(trigger: On<BinReceive<GitHunkRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    let repo_root = PathBuf::from(&p.repo_root);
    let path = crate::host::runner::RequestPath::new(&p.path, &p.path_bytes).resolve(&repo_root);
    spawn_job(
        &outbox,
        trigger.event().webview,
        JobKind::Hunk {
            repo_root,
            path,
            hunk: p.hunk,
            accept: p.accept,
        },
    );
}

fn emit_events(
    commands: &mut Commands,
    pages: &mut Query<&mut vmux_core::PageMetadata>,
    webview: Entity,
    emits: Vec<Emit>,
) {
    for emit in emits {
        let name = emit_event_name(&emit);
        match emit {
            Emit::Repository(ev) => {
                if let Ok(mut page) = pages.get_mut(webview) {
                    if let Some(url) = crate::GitUrl::from_path(Path::new(&ev.repo_root)) {
                        page.url = url;
                    }
                    page.title = match ev.branch.is_empty() {
                        true => ev.repo_name.clone(),
                        false => format!("{} · {}", ev.repo_name, ev.branch),
                    };
                }
                commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev))
            }
            Emit::BranchLog(ev) => {
                commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev))
            }
            Emit::Status(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
            Emit::DiffMeta(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
            Emit::DiffViewport(ev) => {
                commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev))
            }
            Emit::Result(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
            Emit::Error(ev) => commands.trigger(BinHostEmitEvent::from_rkyv(webview, name, &ev)),
        }
    }
}

fn drain_git_outbox(
    outbox: Res<GitOutbox>,
    mut jobs: ResMut<GitStatusJobs>,
    mut pages: Query<&mut vmux_core::PageMetadata>,
    mut commands: Commands,
) {
    let drained: OutboxQueue = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for item in drained {
        match item {
            GitOutboxItem::Events { webview, emits } => {
                emit_events(&mut commands, &mut pages, webview, emits);
            }
            GitOutboxItem::StatusBatch { repo_root, results } => {
                jobs.complete(&repo_root);
                for (webview, emits) in results {
                    emit_events(&mut commands, &mut pages, webview, emits);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::GitErrorEvent;
    use crate::host::runner::test_repo;

    #[test]
    fn repository_picker_starts_at_the_nearest_existing_directory() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("projects");
        std::fs::create_dir(&existing).unwrap();
        let missing = existing.join("github.com/vmux-ai/vmux");

        assert_eq!(GitRepositoryPicker::initial_directory(&missing), existing);
    }

    #[test]
    fn drain_empties_outbox() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitOutbox>()
            .init_resource::<GitStatusJobs>()
            .add_systems(Update, drain_git_outbox);

        let webview = app.world_mut().spawn_empty().id();
        app.world()
            .resource::<GitOutbox>()
            .0
            .lock()
            .unwrap()
            .push(GitOutboxItem::Events {
                webview,
                emits: vec![Emit::Error(GitErrorEvent {
                    message: "boom".into(),
                })],
            });

        app.update();

        assert!(
            app.world()
                .resource::<GitOutbox>()
                .0
                .lock()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn git_watch_targets_cover_index_and_refs() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let (_, targets) = git_watch_targets(&file).unwrap();
        let root = canon(repo.path());
        let git_dir = canon(&repo.path().join(".git"));

        assert!(targets.contains(&GitWatchTarget {
            path: root,
            recursive: true,
            kind: GitWatchKind::Worktree,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: git_dir.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: git_dir.join("refs"),
            recursive: true,
            kind: GitWatchKind::Metadata,
        }));
    }

    #[test]
    fn linked_worktree_targets_cover_private_and_common_git_dirs() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let parent = tempfile::tempdir().unwrap();
        let worktree = parent.path().join("linked");
        test_repo::run(
            repo.path(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "linked",
                worktree.to_str().unwrap(),
            ],
        );

        let (_, targets) = git_watch_targets(&worktree.join("a.txt")).unwrap();
        let worktree_root = canon(&worktree);
        let common = canon(&repo.path().join(".git"));

        assert!(targets.contains(&GitWatchTarget {
            path: worktree_root,
            recursive: true,
            kind: GitWatchKind::Worktree,
        }));
        assert!(targets.iter().any(|target| {
            !target.recursive
                && target.path != common
                && target.path.starts_with(common.join("worktrees"))
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: common.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        }));
        assert!(targets.contains(&GitWatchTarget {
            path: common.join("refs"),
            recursive: true,
            kind: GitWatchKind::Metadata,
        }));
    }

    #[test]
    fn replacing_subscription_releases_only_unshared_watch_targets() {
        let first = test_repo::init();
        let second = test_repo::init();
        let first_file = test_repo::write(first.path(), "a.txt", "one\n");
        let second_file = test_repo::write(second.path(), "b.txt", "two\n");
        let first_info = crate::host::worktree::repo_info(first.path()).unwrap();
        let first_targets = git_watch_targets(&first_file).unwrap().1;
        let second_targets = git_watch_targets(&second_file).unwrap().1;
        let entity = Entity::from_bits(1);
        let mut watch = GitWatch::test();

        watch.subscribe(entity, &first_file).unwrap();
        assert!(watch.subscribe_repo_info(first.path(), Some(&first_info)));
        assert!(first_targets.iter().all(|target| {
            watch
                .watch_references
                .get(target)
                .is_some_and(|references| *references == 2)
        }));

        watch.subscribe(entity, &second_file).unwrap();

        assert!(first_targets.iter().all(|target| {
            watch
                .watch_references
                .get(target)
                .is_some_and(|references| *references == 1)
        }));
        assert!(second_targets.iter().all(|target| {
            watch
                .watch_references
                .get(target)
                .is_some_and(|references| *references == 1)
        }));
    }

    #[test]
    fn inactive_repo_info_entries_release_their_watch_targets() {
        let active_repo = test_repo::init();
        let stale_repo = test_repo::init();
        let active_path = canon(active_repo.path());
        let stale_path = canon(stale_repo.path());
        let active_info = crate::host::worktree::repo_info(&active_path).unwrap();
        let stale_info = crate::host::worktree::repo_info(&stale_path).unwrap();
        let stale_targets = repo_info_watch_targets(&stale_path, Some(&stale_info));
        let mut cache = RepoInfoCache {
            entries: HashMap::from([
                (
                    active_path.clone(),
                    RepoInfoCacheEntry {
                        info: Some(active_info.clone()),
                        loaded: true,
                        dirty: false,
                        watched: true,
                        idle_syncs: 0,
                        pending: None,
                        ignore_events_until: None,
                    },
                ),
                (
                    stale_path.clone(),
                    RepoInfoCacheEntry {
                        info: Some(stale_info.clone()),
                        loaded: true,
                        dirty: false,
                        watched: true,
                        idle_syncs: 0,
                        pending: None,
                        ignore_events_until: None,
                    },
                ),
            ]),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            wake: None,
        };
        let mut watch = GitWatch::test();
        assert!(watch.subscribe_repo_info(&active_path, Some(&active_info)));
        assert!(watch.subscribe_repo_info(&stale_path, Some(&stale_info)));

        assert!(cache.get(&active_path).is_some());
        watch.evict_inactive_repo_info(&mut cache);
        assert!(cache.get(&active_path).is_some());
        watch.evict_inactive_repo_info(&mut cache);

        assert!(cache.entries.contains_key(&active_path));
        assert!(!cache.entries.contains_key(&stale_path));
        assert!(!watch.repo_info_subscriptions.contains_key(&stale_path));
        assert!(
            stale_targets
                .iter()
                .all(|target| !watch.watch_references.contains_key(target))
        );
    }

    #[test]
    fn watch_target_matching_respects_recursion() {
        let root = canon(Path::new("/tmp/vmux-git-watch"));
        let direct = GitWatchTarget {
            path: root.clone(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        };
        let recursive = GitWatchTarget {
            path: root.clone(),
            recursive: true,
            kind: GitWatchKind::Metadata,
        };

        assert!(direct.matches(&root.join("index")));
        assert!(!direct.matches(&root.join("index.lock")));
        assert!(!direct.matches(&root.join("refs/heads/main")));
        assert!(recursive.matches(&root.join("refs/heads/main")));
        assert!(!recursive.matches(&root.join("refs/heads/main.lock")));

        let worktree = GitWatchTarget {
            path: root.clone(),
            recursive: true,
            kind: GitWatchKind::Worktree,
        };
        assert!(worktree.matches(&root.join("Cargo.lock")));
        assert!(!worktree.matches(&root.join(".git/index")));
    }

    #[test]
    fn repo_info_targets_cover_the_checkout_and_git_metadata() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let info = crate::host::worktree::repo_info(repo.path()).unwrap();
        let targets = repo_info_watch_targets(repo.path(), Some(&info));

        let file = canon(&file);
        let head = canon(&info.git_dir.join("HEAD"));
        let lock = canon(&info.git_dir.join("index.lock"));

        assert!(targets.iter().any(|target| target.matches(&file)));
        assert!(targets.iter().any(|target| target.matches(&head)));
        assert!(targets.iter().all(|target| !target.matches(&lock)));
    }

    #[test]
    fn a_path_that_will_not_resolve_is_guessed_once_rather_than_every_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real");
        std::fs::create_dir_all(&real).expect("create real");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        let absent = link.join("not-yet").join("file.txt");
        let mut cache = RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            wake: None,
        };

        let guessed = cache.canonical_path(&absent);
        std::fs::create_dir_all(absent.parent().expect("parent")).expect("create");
        std::fs::write(&absent, "").expect("write");

        assert_eq!(
            cache.canonical_path(&absent),
            guessed,
            "the guess is reused inside the retry window instead of hitting the filesystem again"
        );
        assert_ne!(
            guessed,
            absent.canonicalize().expect("now resolvable"),
            "the guess must differ from a fresh resolution, or this test cannot tell them apart"
        );
    }

    #[test]
    fn repo_info_cache_refreshes_only_after_invalidation() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = canon(repo.path());
        let mut cache = RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            wake: None,
        };
        let wait_for = |cache: &mut RepoInfoCache, expected| {
            for _ in 0..500 {
                if let Some(info) = cache.get(&path)
                    && info.uncommitted == expected
                {
                    return info;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("repo info did not reach uncommitted={expected}");
        };

        assert_eq!(wait_for(&mut cache, 0).uncommitted, 0);
        test_repo::write(repo.path(), "a.txt", "two\n");
        assert_eq!(cache.get(&path).unwrap().uncommitted, 0);
        cache.invalidate(&path);
        assert_eq!(wait_for(&mut cache, 1).uncommitted, 1);
    }

    #[test]
    fn repo_info_cache_keeps_changes_that_arrive_during_refresh() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = canon(repo.path());
        let stale = crate::host::worktree::repo_info(&path);
        test_repo::write(repo.path(), "a.txt", "two\n");
        let mut cache = RepoInfoCache {
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            entries: HashMap::from([(
                path.clone(),
                RepoInfoCacheEntry {
                    info: None,
                    loaded: false,
                    dirty: false,
                    watched: false,
                    idle_syncs: 0,
                    pending: Some(IoTaskPool::get().spawn(async move { stale })),
                    ignore_events_until: None,
                },
            )]),
            wake: None,
        };

        cache.invalidate(&path);
        for _ in 0..500 {
            if cache.get(&path).is_some_and(|info| info.uncommitted == 1) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("repo info stayed stale after an in-flight invalidation");
    }

    #[test]
    fn status_jobs_batch_by_repo_and_keep_one_batch_in_flight() {
        let root = PathBuf::from("/repo");
        let other_root = PathBuf::from("/other");
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut jobs = GitStatusJobs::default();

        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: first,
                path: root.join("a.txt"),
                dirty: false,
            },
        );
        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: first,
                path: root.join("a.txt"),
                dirty: true,
            },
        );
        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: second,
                path: root.join("b.txt"),
                dirty: false,
            },
        );
        jobs.queue(
            other_root.clone(),
            PendingStatusRequest {
                webview: first,
                path: other_root.join("c.txt"),
                dirty: false,
            },
        );

        let batches = jobs.take_ready();
        assert_eq!(batches.len(), 2);
        let (_, requests) = batches
            .iter()
            .find(|(batch_root, _)| batch_root == &root)
            .unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .any(|request| request.webview == first && request.dirty)
        );

        jobs.queue(
            root.clone(),
            PendingStatusRequest {
                webview: first,
                path: root.join("a.txt"),
                dirty: false,
            },
        );
        assert!(jobs.take_ready().is_empty());

        jobs.complete(&root);
        let batches = jobs.take_ready();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].0, root);
        assert_eq!(batches[0].1.len(), 1);
    }
}
