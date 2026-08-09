use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::event::{
    GIT_CHANGED_EVENT, GitChangedEvent, GitCommitRequest, GitDiffRequest, GitDiscardRequest,
    GitHunkRequest, GitPushRequest, GitStageRequest, GitStatusRequest, GitUnstageRequest,
};
use crate::job::{Emit, JobKind, emit_event_name, run_job};

/// Wires the git bridge: runs each git request on a background thread and drains completed
/// results back to the originating webview.
pub struct GitPlugin;

impl Plugin for GitPlugin {
    fn build(&self, app: &mut App) {
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
                    watched: HashSet::new(),
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
            .insert_resource(RepoInfoCache {
                entries: HashMap::new(),
                wake: repo_info_wake,
            })
            .add_plugins(BinEventEmitterPlugin::<(
                GitStatusRequest,
                GitDiffRequest,
                GitStageRequest,
                GitUnstageRequest,
                GitDiscardRequest,
                GitCommitRequest,
                GitPushRequest,
                GitHunkRequest,
            )>::default())
            .add_observer(on_status_request)
            .add_observer(on_diff_request)
            .add_observer(on_stage_request)
            .add_observer(on_unstage_request)
            .add_observer(on_discard_request)
            .add_observer(on_commit_request)
            .add_observer(on_push_request)
            .add_observer(on_hunk_request)
            .add_systems(
                Update,
                (
                    drain_git_watch,
                    poll_repo_info_cache,
                    sync_repo_info_watches,
                    drain_git_outbox,
                    dispatch_status_jobs,
                )
                    .chain(),
            );
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
    watched: HashSet<GitWatchTarget>,
    subscriptions: HashMap<Entity, GitSubscription>,
    repo_info_subscriptions: HashMap<PathBuf, Vec<GitWatchTarget>>,
}

struct RepoInfoCacheEntry {
    info: Option<crate::worktree::RepoInfo>,
    loaded: bool,
    dirty: bool,
    watched: bool,
    pending: Option<Task<Option<crate::worktree::RepoInfo>>>,
    ignore_events_until: Option<Instant>,
}

#[derive(Resource)]
pub struct RepoInfoCache {
    entries: HashMap<PathBuf, RepoInfoCacheEntry>,
    wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
}

impl RepoInfoCache {
    pub fn get(&mut self, path: &Path) -> Option<crate::worktree::RepoInfo> {
        let path = canon(path);
        let wake = self.wake.clone();
        let entry = self
            .entries
            .entry(path.clone())
            .or_insert_with(|| RepoInfoCacheEntry {
                info: None,
                loaded: false,
                dirty: true,
                watched: false,
                pending: None,
                ignore_events_until: None,
            });
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
                let info = crate::worktree::repo_info(&path);
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
) -> Result<(PathBuf, Vec<GitWatchTarget>), crate::runner::GitError> {
    let root = crate::runner::repo_root(file)?;
    let (stdout, stderr, ok) = crate::runner::git(
        &root,
        &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
    )?;
    if !ok {
        return Err(crate::runner::git_err(&stdout, &stderr));
    }
    let mut lines = stdout.lines();
    let git_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| crate::runner::GitError("missing git directory".into()))?;
    let common_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| crate::runner::GitError("missing common git directory".into()))?;
    let mut targets = vec![GitWatchTarget {
        path: git_dir.clone(),
        recursive: false,
        kind: GitWatchKind::Metadata,
    }];
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

fn target_matches(target: &GitWatchTarget, changed: &Path) -> bool {
    let changed = canon(changed);
    let matches = changed == target.path
        || if target.recursive {
            changed.starts_with(&target.path)
        } else {
            changed.parent() == Some(target.path.as_path())
        };
    if !matches {
        return false;
    }
    match target.kind {
        GitWatchKind::Metadata => !is_git_lock_path(&changed),
        GitWatchKind::Worktree => changed
            .strip_prefix(&target.path)
            .ok()
            .is_none_or(|relative| {
                !relative
                    .components()
                    .any(|component| component.as_os_str() == ".git")
            }),
    }
}

fn repo_info_watch_targets(
    path: &Path,
    info: Option<&crate::worktree::RepoInfo>,
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
    fn subscribe(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Result<PathBuf, crate::runner::GitError> {
        let path = canon(path);
        if let Some(subscription) = self
            .subscriptions
            .get(&entity)
            .filter(|subscription| subscription.path == path && subscription.complete)
        {
            return Ok(subscription.repo_root.clone());
        }
        let (repo_root, targets) = git_watch_targets(&path).inspect_err(|_| {
            self.subscriptions.remove(&entity);
        })?;
        let mut complete = true;
        for target in &targets {
            if self.watched.contains(target) {
                continue;
            }
            let mode = if target.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if self.watcher.watch(&target.path, mode).is_ok() {
                self.watched.insert(target.clone());
            } else {
                complete = false;
            }
        }
        self.subscriptions.insert(
            entity,
            GitSubscription {
                path,
                repo_root: repo_root.clone(),
                targets,
                complete,
            },
        );
        Ok(repo_root)
    }

    fn subscribe_repo_info(
        &mut self,
        path: &Path,
        info: Option<&crate::worktree::RepoInfo>,
    ) -> bool {
        let path = canon(path);
        let targets = repo_info_watch_targets(&path, info);
        if self.repo_info_subscriptions.get(&path) == Some(&targets) {
            return true;
        }
        let mut complete = true;
        for target in &targets {
            if self.watched.contains(target) {
                continue;
            }
            let mode = if target.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if self.watcher.watch(&target.path, mode).is_ok() {
                self.watched.insert(target.clone());
            } else {
                complete = false;
            }
        }
        if complete {
            self.repo_info_subscriptions.insert(path, targets);
        }
        complete
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

fn spawn_status_batch(
    outbox: &GitOutbox,
    repo_root: PathBuf,
    requests: Vec<PendingStatusRequest>,
) {
    let sink = outbox.0.clone();
    std::thread::spawn(move || {
        let paths: Vec<PathBuf> = requests.iter().map(|request| request.path.clone()).collect();
        let results = match crate::runner::statuses(&repo_root, &paths) {
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
    if !crate::runner::has_repository(&path) {
        outbox
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(GitOutboxItem::Events {
                webview,
                emits: vec![Emit::Status(crate::runner::non_repository_status())],
            });
        return;
    }
    let repo_root = if let Some(mut watch) = watch {
        watch.subscribe(webview, &path)
    } else {
        crate::runner::repo_root(&path)
    };
    match repo_root {
        Ok(repo_root) => jobs.queue(
            repo_root,
            PendingStatusRequest {
                webview,
                path,
                dirty: sources
                    .get(webview)
                    .is_ok_and(|source| source.dirty),
            },
        ),
        Err(error) => outbox
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(GitOutboxItem::Events {
                webview,
                emits: vec![Emit::Error(crate::event::GitErrorEvent {
                    message: error.0,
                })],
            }),
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
    while let Ok(result) = watch.rx.try_recv() {
        let Ok(event) = result else {
            continue;
        };
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        changed.extend(event.paths);
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
                .any(|target| changed.iter().any(|path| target_matches(target, path)))
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
                .any(|target| changed.iter().any(|path| target_matches(target, path)))
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
        return;
    };
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
    spawn_job(&outbox, trigger.event().webview, JobKind::Diff {
        path: p.path.clone().into(),
        top_line: p.top_line,
        rows: p.rows,
        content: sources
            .get(trigger.event().webview)
            .ok()
            .filter(|source| source.dirty)
            .map(|source| source.content.clone()),
    });
}

fn on_stage_request(trigger: On<BinReceive<GitStageRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Stage {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_unstage_request(trigger: On<BinReceive<GitUnstageRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Unstage {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_discard_request(trigger: On<BinReceive<GitDiscardRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Discard {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_commit_request(trigger: On<BinReceive<GitCommitRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Commit {
        path: p.path.clone().into(),
        message: p.message.clone(),
    });
}

fn on_push_request(trigger: On<BinReceive<GitPushRequest>>, outbox: Res<GitOutbox>) {
    spawn_job(&outbox, trigger.event().webview, JobKind::Push {
        path: trigger.event().payload.path.clone().into(),
    });
}

fn on_hunk_request(trigger: On<BinReceive<GitHunkRequest>>, outbox: Res<GitOutbox>) {
    let p = &trigger.event().payload;
    spawn_job(&outbox, trigger.event().webview, JobKind::Hunk {
        path: p.path.clone().into(),
        hunk: p.hunk,
        accept: p.accept,
    });
}

fn emit_events(commands: &mut Commands, webview: Entity, emits: Vec<Emit>) {
    for emit in emits {
        let name = emit_event_name(&emit);
        match emit {
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
    mut commands: Commands,
) {
    let drained: OutboxQueue = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for item in drained {
        match item {
            GitOutboxItem::Events { webview, emits } => {
                emit_events(&mut commands, webview, emits);
            }
            GitOutboxItem::StatusBatch { repo_root, results } => {
                jobs.complete(&repo_root);
                for (webview, emits) in results {
                    emit_events(&mut commands, webview, emits);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "plugin.test.rs"]
mod tests;
