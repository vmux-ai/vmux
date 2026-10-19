use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::GitUpdateSet;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vmux_ecs::KeyboardOwner;
use vmux_path::PathIdentity;

pub(super) struct WatchPlugin;

impl Plugin for WatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, initialize)
            .add_systems(Startup, spawn_repo_info_cache)
            .add_systems(
                Update,
                (
                    sync_page_subscriptions,
                    drain,
                    start_repo_info_loads,
                    poll_info,
                )
                    .chain()
                    .in_set(GitUpdateSet::Watch),
            )
            .add_systems(PostUpdate, info);
    }
}

fn initialize(world: &mut World) {
    let proxy = world
        .get_resource::<EventLoopProxyWrapper>()
        .map(|wrapper| (**wrapper).clone());
    let events = GitWatchEvents::start(proxy);
    let tx = events.sender;
    match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if !GitWatch::should_forward(&result) {
            return;
        }
        let Ok(event) = result else {
            return;
        };
        let _ = tx.send(event.paths);
    }) {
        Ok(watcher) => {
            world.insert_non_send(GitWatch {
                watcher,
                rx: events.receiver,
                watch_references: HashMap::new(),
                subscriptions: HashMap::new(),
                repo_info_subscriptions: HashMap::new(),
            });
        }
        Err(error) => bevy::log::warn!("git watcher init failed: {error}"),
    }
}

fn spawn_repo_info_cache(proxy: Option<Res<EventLoopProxyWrapper>>, mut commands: Commands) {
    let wake = proxy.map(|wrapper| (**wrapper).clone());
    commands.spawn((
        Name::new("Repository info cache"),
        RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            wake,
        },
    ));
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

pub(super) struct GitWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<GitWatchBatch>,
    watch_references: HashMap<GitWatchTarget, usize>,
    subscriptions: HashMap<Entity, GitSubscription>,
    repo_info_subscriptions: HashMap<PathBuf, Vec<GitWatchTarget>>,
}

struct RepoInfoCacheEntry {
    info: Option<super::worktree::RepoInfo>,
    loaded: bool,
    dirty: bool,
    branches_dirty: bool,
    watch_attempted: bool,
    last_accessed: Instant,
    pending: Option<Task<Option<super::worktree::RepoInfo>>>,
}

struct RepoInfoLease {
    paths: HashSet<PathBuf>,
    active_only: bool,
}

const UNRESOLVED_RETRY: Duration = Duration::from_secs(1);
const REPO_INFO_IDLE_TTL: Duration = Duration::from_secs(300);

struct GuessedPath {
    guess: PathBuf,
    made_at: Instant,
}

impl From<&Path> for GuessedPath {
    fn from(path: &Path) -> Self {
        Self {
            guess: PathIdentity::resolve(path).into_path_buf(),
            made_at: Instant::now(),
        }
    }
}

impl GuessedPath {
    fn worth_reusing(&self) -> bool {
        self.made_at.elapsed() < UNRESOLVED_RETRY
    }
}

#[derive(Component)]
pub struct RepoInfoCache {
    entries: HashMap<PathBuf, RepoInfoCacheEntry>,
    canonical: HashMap<PathBuf, PathBuf>,
    guessed: HashMap<PathBuf, GuessedPath>,
    repositories: HashMap<PathBuf, PathBuf>,
    leases: HashMap<Entity, RepoInfoLease>,
    wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
}

impl RepoInfoCache {
    pub fn observe(&mut self, owner: Entity, path: &Path) -> Option<super::worktree::RepoInfo> {
        self.observe_with(owner, path, false)
    }

    pub fn observe_active(
        &mut self,
        owner: Entity,
        path: &Path,
    ) -> Option<super::worktree::RepoInfo> {
        self.observe_with(owner, path, true)
    }

    pub fn snapshot(&mut self, path: &Path) -> Option<super::worktree::RepoInfo> {
        let path = self.entry_path(path);
        self.request(path)
    }

    pub fn observe_active_paths(
        &mut self,
        owner: Entity,
        paths: impl IntoIterator<Item = PathBuf>,
    ) {
        let mut observed = HashSet::new();
        for path in paths {
            let path = self.entry_path(&path);
            let _ = self.request(path.clone());
            observed.insert(path);
        }
        self.leases.insert(
            owner,
            RepoInfoLease {
                paths: observed,
                active_only: true,
            },
        );
    }

    pub fn release(&mut self, owner: Entity) {
        self.leases.remove(&owner);
    }

    fn observe_with(
        &mut self,
        owner: Entity,
        path: &Path,
        active_only: bool,
    ) -> Option<super::worktree::RepoInfo> {
        let path = self.entry_path(path);
        let paths = HashSet::from([path.clone()]);
        self.leases
            .insert(owner, RepoInfoLease { paths, active_only });
        self.request(path)
    }

    fn request(&mut self, path: PathBuf) -> Option<super::worktree::RepoInfo> {
        let entry = self
            .entries
            .entry(path.clone())
            .or_insert_with(|| RepoInfoCacheEntry {
                info: None,
                loaded: false,
                dirty: true,
                branches_dirty: true,
                watch_attempted: false,
                last_accessed: Instant::now(),
                pending: None,
            });
        entry.last_accessed = Instant::now();
        entry.info.clone()
    }

    fn entry_path(&mut self, path: &Path) -> PathBuf {
        let path = self.canonical_path(path);
        self.repositories
            .iter()
            .filter(|(root, _)| path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
            .map(|(_, entry)| entry.clone())
            .unwrap_or(path)
    }

    fn remember_repository(&mut self, path: PathBuf, info: &super::worktree::RepoInfo) {
        let root = PathIdentity::resolve(&info.repo_root).into_path_buf();
        self.repositories.entry(root).or_insert(path);
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
            let guessed = GuessedPath::from(path);
            let guess = guessed.guess.clone();
            self.guessed.insert(path.to_path_buf(), guessed);
            return guess;
        };
        self.guessed.remove(path);
        self.canonical.insert(path.to_path_buf(), resolved.clone());
        resolved
    }

    fn invalidate(&mut self, path: &Path, branches_dirty: bool) {
        if let Some(entry) = self.entries.get_mut(path) {
            entry.dirty = true;
            entry.branches_dirty |= branches_dirty;
        }
    }

    fn inactive_paths(&mut self, active: &HashSet<PathBuf>) -> Vec<PathBuf> {
        let mut inactive = Vec::new();
        for (path, entry) in &mut self.entries {
            if entry.pending.is_some() || active.contains(path) {
                continue;
            }
            if entry.last_accessed.elapsed() >= REPO_INFO_IDLE_TTL {
                inactive.push(path.clone());
            }
        }
        inactive
    }

    fn remove(&mut self, path: &Path) {
        self.entries.remove(path);
        self.canonical.retain(|_, canonical| canonical != path);
        self.guessed.remove(path);
        self.repositories.retain(|_, entry| entry != path);
    }
}

const WATCH_EVENT_LIMIT: usize = 256;
const WATCH_DEBOUNCE: Duration = Duration::from_millis(500);
const WATCH_MAX_LATENCY: Duration = Duration::from_secs(2);
const WATCH_TARGET_LIMIT: usize = 128;
const CANONICAL_CAP: usize = 4096;

#[derive(Default)]
struct GitWatchBatch {
    paths: HashSet<PathBuf>,
    overflow: bool,
}

impl GitWatchBatch {
    fn extend(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if self.overflow || GitWatchTarget::is_lock_path(&path) {
                continue;
            }
            if !self.paths.contains(&path) && self.paths.len() >= WATCH_EVENT_LIMIT {
                self.paths.clear();
                self.overflow = true;
                continue;
            }
            self.paths.insert(path);
        }
    }

    fn is_empty(&self) -> bool {
        !self.overflow && self.paths.is_empty()
    }
}

struct GitWatchEvents {
    sender: mpsc::Sender<Vec<PathBuf>>,
    receiver: mpsc::Receiver<GitWatchBatch>,
}

impl GitWatchEvents {
    fn start(proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>) -> Self {
        let (input_tx, input_rx) = mpsc::channel::<Vec<PathBuf>>();
        let (output_tx, output_rx) = mpsc::channel::<GitWatchBatch>();
        let worker = std::thread::Builder::new()
            .name("vmux-git-watch-debounce".to_string())
            .spawn(move || {
                while let Ok(paths) = input_rx.recv() {
                    let started = Instant::now();
                    let mut batch = GitWatchBatch::default();
                    batch.extend(paths);
                    let mut disconnected = false;
                    while let Some(remaining) = WATCH_MAX_LATENCY.checked_sub(started.elapsed()) {
                        let timeout = WATCH_DEBOUNCE.min(remaining);
                        match input_rx.recv_timeout(timeout) {
                            Ok(paths) => batch.extend(paths),
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    if !batch.is_empty() {
                        if output_tx.send(batch).is_err() {
                            break;
                        }
                        if let Some(proxy) = proxy.as_ref() {
                            let _ = proxy.send_event(WinitUserEvent::WakeUp);
                        }
                    }
                    if disconnected {
                        break;
                    }
                }
            });
        if let Err(error) = worker {
            bevy::log::warn!("git watch debounce init failed: {error}");
        }
        Self {
            sender: input_tx,
            receiver: output_rx,
        }
    }
}

impl GitWatchTarget {
    fn for_checkout(file: &Path) -> Result<(PathBuf, Vec<Self>), super::repository::GitError> {
        let repository = super::repository::GitRepository::discover(file)?;
        let root = repository.path().to_path_buf();
        let (stdout, stderr, ok) = super::repository::GitCommand::run(
            &root,
            &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
        )?;
        if !ok {
            return Err(super::repository::GitCommand::error(&stdout, &stderr));
        }
        let mut lines = stdout.lines();
        let git_dir = lines
            .next()
            .map(|line| Self::resolve_path(&root, line))
            .ok_or_else(|| super::repository::GitError("missing git directory".into()))?;
        let common_dir = lines
            .next()
            .map(|line| Self::resolve_path(&root, line))
            .ok_or_else(|| super::repository::GitError("missing common git directory".into()))?;
        let mut targets = vec![
            Self {
                path: PathIdentity::resolve(&root).into_path_buf(),
                recursive: true,
                kind: GitWatchKind::Worktree,
            },
            Self {
                path: git_dir.clone(),
                recursive: false,
                kind: GitWatchKind::Metadata,
            },
        ];
        if common_dir != git_dir {
            targets.push(Self {
                path: common_dir.clone(),
                recursive: false,
                kind: GitWatchKind::Metadata,
            });
        }
        targets.push(Self {
            path: common_dir.join("refs"),
            recursive: true,
            kind: GitWatchKind::Metadata,
        });
        let worktrees = common_dir.join("worktrees");
        if worktrees.is_dir() {
            targets.push(Self {
                path: worktrees,
                recursive: true,
                kind: GitWatchKind::Metadata,
            });
        }
        Ok((root, targets))
    }

    fn for_repo_info(path: &Path, info: Option<&super::worktree::RepoInfo>) -> Vec<Self> {
        let Some(info) = info else {
            return vec![Self {
                path: PathIdentity::resolve(path).into_path_buf(),
                recursive: true,
                kind: GitWatchKind::Worktree,
            }];
        };
        let repo_root = PathIdentity::resolve(&info.repo_root).into_path_buf();
        let git_dir = PathIdentity::resolve(&info.git_dir).into_path_buf();
        let common_dir = PathIdentity::resolve(&info.common_dir).into_path_buf();
        let mut targets = vec![
            Self {
                path: repo_root,
                recursive: true,
                kind: GitWatchKind::Worktree,
            },
            Self {
                path: git_dir.clone(),
                recursive: false,
                kind: GitWatchKind::Metadata,
            },
        ];
        if common_dir != git_dir {
            targets.push(Self {
                path: common_dir.clone(),
                recursive: false,
                kind: GitWatchKind::Metadata,
            });
        }
        targets.push(Self {
            path: common_dir.join("refs"),
            recursive: true,
            kind: GitWatchKind::Metadata,
        });
        let worktrees = common_dir.join("worktrees");
        if worktrees.is_dir() {
            targets.push(Self {
                path: worktrees,
                recursive: true,
                kind: GitWatchKind::Metadata,
            });
        }
        targets
    }

    fn resolve_path(root: &Path, value: &str) -> PathBuf {
        let path = PathBuf::from(value.trim());
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        PathIdentity::resolve(&path).into_path_buf()
    }

    fn is_lock_path(path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".lock"))
    }

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
            GitWatchKind::Metadata => !Self::is_lock_path(changed),
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

    fn branches_changed(&self, changed: &Path) -> bool {
        if self.kind != GitWatchKind::Metadata || !self.matches(changed) {
            return false;
        }
        if matches!(
            self.path.file_name().and_then(|name| name.to_str()),
            Some("refs" | "worktrees")
        ) {
            return true;
        }
        matches!(
            changed.file_name().and_then(|name| name.to_str()),
            Some("HEAD" | "packed-refs" | "refs" | "worktrees")
        )
    }
}

impl GitWatch {
    fn should_forward(result: &notify::Result<notify::Event>) -> bool {
        match result {
            Ok(event) => {
                !matches!(event.kind, EventKind::Access(_))
                    && event
                        .paths
                        .iter()
                        .any(|path| !GitWatchTarget::is_lock_path(path))
            }
            Err(_) => true,
        }
    }

    pub(super) fn subscribe(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Result<PathBuf, super::repository::GitError> {
        let path = PathIdentity::resolve(path).into_path_buf();
        if let Some(subscription) = self
            .subscriptions
            .get(&entity)
            .filter(|subscription| subscription.path == path && subscription.complete)
        {
            return Ok(subscription.repo_root.clone());
        }
        let (repo_root, targets) = match GitWatchTarget::for_checkout(&path) {
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

    pub(super) fn release_subscription(&mut self, entity: Entity) {
        let Some(subscription) = self.subscriptions.remove(&entity) else {
            return;
        };
        self.release_targets(&subscription.targets);
    }

    fn acquire_targets(&mut self, targets: &[GitWatchTarget]) -> bool {
        let missing = targets
            .iter()
            .filter(|target| !self.watch_references.contains_key(*target))
            .count();
        if self.watch_references.len().saturating_add(missing) > WATCH_TARGET_LIMIT {
            bevy::log::warn!(
                current = self.watch_references.len(),
                requested = missing,
                limit = WATCH_TARGET_LIMIT,
                "git watch target limit reached"
            );
            return false;
        }
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

    fn release_inactive_repo_info(
        &mut self,
        repo_info: &mut RepoInfoCache,
        active: &HashSet<PathBuf>,
    ) {
        let paths = self
            .repo_info_subscriptions
            .keys()
            .filter(|path| !active.contains(*path))
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            if let Some(targets) = self.repo_info_subscriptions.remove(&path) {
                self.release_targets(&targets);
            }
            if let Some(entry) = repo_info.entries.get_mut(&path) {
                entry.watch_attempted = false;
            }
        }
    }

    fn evict_inactive_repo_info(
        &mut self,
        repo_info: &mut RepoInfoCache,
        active: &HashSet<PathBuf>,
    ) {
        for path in repo_info.inactive_paths(active) {
            if let Some(targets) = self.repo_info_subscriptions.remove(&path) {
                self.release_targets(&targets);
            }
            repo_info.remove(&path);
        }
    }

    fn subscribe_repo_info(
        &mut self,
        path: &Path,
        info: Option<&super::worktree::RepoInfo>,
    ) -> bool {
        let path = PathIdentity::resolve(path).into_path_buf();
        let targets = GitWatchTarget::for_repo_info(&path, info);
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

    #[cfg(test)]
    pub(super) fn test() -> Self {
        let (_, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(|_| {}).unwrap();
        Self {
            watcher,
            rx,
            watch_references: HashMap::new(),
            subscriptions: HashMap::new(),
            repo_info_subscriptions: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn has_subscription(&self, entity: Entity) -> bool {
        self.subscriptions.contains_key(&entity)
    }
}

fn sync_page_subscriptions(
    watch: Option<NonSendMut<GitWatch>>,
    pages: Query<(Entity, &super::state::GitState, Has<KeyboardOwner>)>,
    activated: Query<(Entity, &super::state::GitState), Added<KeyboardOwner>>,
    mut commands: Commands,
) {
    let Some(mut watch) = watch else {
        return;
    };
    let inactive = watch
        .subscriptions
        .keys()
        .filter(|entity| pages.get(**entity).map_or(true, |(_, _, active)| !active))
        .copied()
        .collect::<Vec<_>>();
    for entity in inactive {
        watch.release_subscription(entity);
    }
    for (entity, state) in &activated {
        let path = Path::new(state.workspace());
        if path.as_os_str().is_empty() || watch.subscriptions.contains_key(&entity) {
            continue;
        }
        if let Err(error) = watch.subscribe(entity, path) {
            commands.trigger(super::job_runner::GitJobFailure {
                webview: entity,
                message: error.0,
            });
            continue;
        }
        commands.spawn((
            super::job_runner::GitJob::new(entity),
            super::job::RepositoryJob {
                path: path.to_path_buf(),
            },
        ));
    }
}

fn drain(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: Single<&mut RepoInfoCache>,
    mut states: super::state::GitStates,
    mut files: super::status::FileGitStates,
    active: Query<(), With<KeyboardOwner>>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(watch) = watch else {
        return;
    };
    let repo_info = repo_info.bypass_change_detection();
    let mut changed = HashSet::new();
    let mut overflow = false;
    while let Ok(batch) = watch.rx.try_recv() {
        overflow |= batch.overflow;
        if overflow {
            changed.clear();
            continue;
        }
        for path in batch.paths {
            changed.insert(repo_info.canonical_path(&path));
        }
    }
    if changed.is_empty() && !overflow {
        return;
    }
    let affected: Vec<Entity> = watch
        .subscriptions
        .iter()
        .filter(|(_, subscription)| {
            overflow
                || subscription
                    .targets
                    .iter()
                    .any(|target| changed.iter().any(|path| target.matches(path)))
        })
        .filter_map(|(entity, _)| active.contains(*entity).then_some(*entity))
        .collect();
    for entity in affected {
        if states.contains(entity) {
            if let Some(path) = states.mark_changed(entity) {
                commands.spawn((
                    super::job_runner::GitJob::new(entity),
                    super::job::RepositoryJob { path: path.into() },
                ));
            }
        } else if let Some(refresh) =
            files.schedule_changed(entity, wake.as_deref().map(|wake| (**wake).clone()))
        {
            commands.entity(entity).insert(refresh);
        }
    }
    let affected_repo_info: Vec<(PathBuf, bool)> = watch
        .repo_info_subscriptions
        .iter()
        .filter(|(_, targets)| {
            overflow
                || targets
                    .iter()
                    .any(|target| changed.iter().any(|path| target.matches(path)))
        })
        .map(|(path, targets)| {
            let branches_dirty = overflow
                || targets
                    .iter()
                    .any(|target| changed.iter().any(|path| target.branches_changed(path)));
            (path.clone(), branches_dirty)
        })
        .collect();
    for (path, branches_dirty) in affected_repo_info {
        repo_info.invalidate(&path, branches_dirty);
    }
}

fn start_repo_info_loads(mut repo_info: Single<&mut RepoInfoCache>) {
    let repo_info = repo_info.bypass_change_detection();
    let wake = repo_info.wake.clone();
    for (path, entry) in &mut repo_info.entries {
        if entry.pending.is_some() || (!entry.dirty && entry.loaded) {
            continue;
        }
        entry.dirty = false;
        let path = path.clone();
        let wake = wake.clone();
        let cached_branches = if entry.branches_dirty {
            entry.branches_dirty = false;
            None
        } else {
            entry.info.as_ref().map(|info| info.branches.clone())
        };
        entry.pending = Some(IoTaskPool::get().spawn(async move {
            let info = super::worktree::RepoInfo::read_with_branches(&path, cached_branches);
            if let Some(wake) = wake {
                let _ = wake.send_event(WinitUserEvent::WakeUp);
            }
            info
        }));
    }
}

fn poll_info(mut repo_info: Single<&mut RepoInfoCache>) {
    let (changed, repositories) = {
        let repo_info = repo_info.bypass_change_detection();
        let mut changed = false;
        let mut repositories = Vec::new();
        for (path, entry) in &mut repo_info.entries {
            let Some(task) = entry.pending.as_mut() else {
                continue;
            };
            let Some(info) = future::block_on(future::poll_once(task)) else {
                continue;
            };
            changed |= entry.info != info;
            entry.info = info;
            entry.loaded = true;
            entry.watch_attempted = false;
            entry.pending = None;
            if let Some(info) = entry.info.as_ref() {
                repositories.push((path.clone(), info.clone()));
            }
        }
        (changed, repositories)
    };
    for (path, info) in repositories {
        repo_info.remember_repository(path, &info);
    }
    if changed {
        repo_info.set_changed();
    }
}

fn info(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: Single<&mut RepoInfoCache>,
    entities: Query<(), ()>,
    active_owners: Query<(), With<KeyboardOwner>>,
) {
    let repo_info = repo_info.bypass_change_detection();
    repo_info.leases.retain(|owner, lease| {
        entities.contains(*owner) && (!lease.active_only || active_owners.contains(*owner))
    });
    let active = repo_info
        .leases
        .values()
        .flat_map(|lease| lease.paths.iter().cloned())
        .collect::<HashSet<_>>();
    let Some(mut watch) = watch else {
        for path in repo_info.inactive_paths(&active) {
            repo_info.remove(&path);
        }
        return;
    };
    watch.release_inactive_repo_info(repo_info, &active);
    watch.evict_inactive_repo_info(repo_info, &active);
    let paths: Vec<PathBuf> = repo_info
        .entries
        .iter()
        .filter_map(|(path, entry)| {
            (active.contains(path) && entry.loaded && !entry.watch_attempted)
                .then_some(path.clone())
        })
        .collect();
    for path in paths {
        let info = repo_info
            .entries
            .get(&path)
            .and_then(|entry| entry.info.clone());
        let _ = watch.subscribe_repo_info(&path, info.as_ref());
        if let Some(entry) = repo_info.entries.get_mut(&path) {
            entry.watch_attempted = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::repository::test_repo;

    #[test]
    fn git_watch_targets_cover_index_and_refs() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let (_, targets) = GitWatchTarget::for_checkout(&file).unwrap();
        let root = PathIdentity::resolve(repo.path()).into_path_buf();
        let git_dir = PathIdentity::resolve(repo.path().join(".git")).into_path_buf();

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

        let (_, targets) = GitWatchTarget::for_checkout(&worktree.join("a.txt")).unwrap();
        let worktree_root = PathIdentity::resolve(&worktree).into_path_buf();
        let common = PathIdentity::resolve(repo.path().join(".git")).into_path_buf();

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
        assert!(targets.contains(&GitWatchTarget {
            path: common.join("worktrees"),
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
        let first_info = super::super::worktree::RepoInfo::read(first.path()).unwrap();
        let first_targets = GitWatchTarget::for_checkout(&first_file).unwrap().1;
        let second_targets = GitWatchTarget::for_checkout(&second_file).unwrap().1;
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
    fn watch_target_limit_rejects_new_targets() {
        let mut watch = GitWatch::test();
        for index in 0..WATCH_TARGET_LIMIT {
            watch.watch_references.insert(
                GitWatchTarget {
                    path: PathBuf::from(format!("/tmp/vmux-git-watch-{index}")),
                    recursive: false,
                    kind: GitWatchKind::Metadata,
                },
                1,
            );
        }
        let target = GitWatchTarget {
            path: PathBuf::from("/tmp/vmux-git-watch-overflow"),
            recursive: false,
            kind: GitWatchKind::Metadata,
        };

        assert!(!watch.acquire_targets(std::slice::from_ref(&target)));
        assert_eq!(watch.watch_references.len(), WATCH_TARGET_LIMIT);
        assert!(!watch.watch_references.contains_key(&target));
    }

    #[test]
    fn event_burst_collapses_to_one_overflow_refresh() {
        let mut batch = GitWatchBatch::default();
        batch.extend(
            (0..=WATCH_EVENT_LIMIT)
                .map(|index| PathBuf::from(format!("/tmp/vmux-git-event-{index}"))),
        );

        assert!(batch.overflow);
        assert!(batch.paths.is_empty());
    }

    #[test]
    fn inactive_repo_info_entries_release_their_watch_targets() {
        let active_repo = test_repo::init();
        let stale_repo = test_repo::init();
        let active_path = PathIdentity::resolve(active_repo.path()).into_path_buf();
        let stale_path = PathIdentity::resolve(stale_repo.path()).into_path_buf();
        let active_info = super::super::worktree::RepoInfo::read(&active_path).unwrap();
        let stale_info = super::super::worktree::RepoInfo::read(&stale_path).unwrap();
        let stale_targets = GitWatchTarget::for_repo_info(&stale_path, Some(&stale_info));
        let mut cache = RepoInfoCache {
            entries: HashMap::from([
                (
                    active_path.clone(),
                    RepoInfoCacheEntry {
                        info: Some(active_info.clone()),
                        loaded: true,
                        dirty: false,
                        branches_dirty: false,
                        watch_attempted: true,
                        last_accessed: Instant::now(),
                        pending: None,
                    },
                ),
                (
                    stale_path.clone(),
                    RepoInfoCacheEntry {
                        info: Some(stale_info.clone()),
                        loaded: true,
                        dirty: false,
                        branches_dirty: false,
                        watch_attempted: true,
                        last_accessed: Instant::now() - REPO_INFO_IDLE_TTL,
                        pending: None,
                    },
                ),
            ]),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            wake: None,
        };
        let mut watch = GitWatch::test();
        assert!(watch.subscribe_repo_info(&active_path, Some(&active_info)));
        assert!(watch.subscribe_repo_info(&stale_path, Some(&stale_info)));

        let active = HashSet::from([active_path.clone()]);
        watch.evict_inactive_repo_info(&mut cache, &active);

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
    fn active_owner_watches_multiple_repositories_and_releases_them_together() {
        let first = test_repo::init();
        let second = test_repo::init();
        let first_path = PathIdentity::resolve(first.path()).into_path_buf();
        let second_path = PathIdentity::resolve(second.path()).into_path_buf();
        let first_info = super::super::worktree::RepoInfo::read(&first_path).unwrap();
        let second_info = super::super::worktree::RepoInfo::read(&second_path).unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PostUpdate, info);
        app.insert_non_send(GitWatch::test());
        let owner = app.world_mut().spawn(KeyboardOwner).id();
        let mut cache = RepoInfoCache {
            entries: HashMap::from([
                (
                    first_path.clone(),
                    RepoInfoCacheEntry {
                        info: Some(first_info),
                        loaded: true,
                        dirty: false,
                        branches_dirty: false,
                        watch_attempted: false,
                        last_accessed: Instant::now(),
                        pending: None,
                    },
                ),
                (
                    second_path.clone(),
                    RepoInfoCacheEntry {
                        info: Some(second_info),
                        loaded: true,
                        dirty: false,
                        branches_dirty: false,
                        watch_attempted: false,
                        last_accessed: Instant::now(),
                        pending: None,
                    },
                ),
            ]),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            wake: None,
        };
        cache.observe_active_paths(owner, [first_path.clone(), second_path.clone()]);
        app.world_mut().spawn(cache);

        app.update();
        let watch = app.world().non_send::<GitWatch>();
        assert!(watch.repo_info_subscriptions.contains_key(&first_path));
        assert!(watch.repo_info_subscriptions.contains_key(&second_path));

        app.world_mut().entity_mut(owner).remove::<KeyboardOwner>();
        app.update();
        assert!(
            app.world()
                .non_send::<GitWatch>()
                .repo_info_subscriptions
                .is_empty()
        );
    }

    #[test]
    fn page_watch_subscription_exists_only_while_the_page_is_active() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_page_subscriptions);
        app.insert_non_send(GitWatch::test());
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: file.to_string_lossy().into_owned(),
                        ..Default::default()
                    },
                },
                KeyboardOwner,
            ))
            .id();

        app.update();
        assert!(
            app.world()
                .non_send::<GitWatch>()
                .subscriptions
                .contains_key(&page)
        );
        let mut jobs = app.world_mut().query::<&super::super::job::RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 1);

        app.update();
        let mut jobs = app.world_mut().query::<&super::super::job::RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 1);

        app.world_mut().entity_mut(page).remove::<KeyboardOwner>();
        app.update();
        assert!(app.world().non_send::<GitWatch>().subscriptions.is_empty());
    }

    #[test]
    fn watch_target_matching_respects_recursion() {
        let root = PathIdentity::resolve(Path::new("/tmp/vmux-git-watch")).into_path_buf();
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
        let info = super::super::worktree::RepoInfo::read(repo.path()).unwrap();
        let targets = GitWatchTarget::for_repo_info(repo.path(), Some(&info));

        let file = PathIdentity::resolve(&file).into_path_buf();
        let head = PathIdentity::resolve(info.git_dir.join("HEAD")).into_path_buf();
        let lock = PathIdentity::resolve(info.git_dir.join("index.lock")).into_path_buf();

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
            repositories: HashMap::new(),
            leases: HashMap::new(),
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
    fn known_repository_reuses_one_cache_entry_for_nested_paths() {
        let repo = test_repo::init();
        let first = repo.path().join("first");
        let second = repo.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let mut cache = RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            wake: None,
        };

        assert!(cache.snapshot(&first).is_none());
        let first = PathIdentity::resolve(&first).into_path_buf();
        let info = super::super::worktree::RepoInfo::read(&first).unwrap();
        cache.remember_repository(first, &info);
        assert!(cache.snapshot(&second).is_none());

        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn repo_info_cache_refreshes_only_after_invalidation() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = PathIdentity::resolve(repo.path()).into_path_buf();
        let cache = RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            wake: None,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (start_repo_info_loads, poll_info).chain());
        let cache = app.world_mut().spawn(cache).id();
        let wait_for = |app: &mut App, expected| {
            for _ in 0..500 {
                let info = app
                    .world_mut()
                    .get_mut::<RepoInfoCache>(cache)
                    .unwrap()
                    .snapshot(&path);
                app.update();
                if let Some(info) = info
                    && info.uncommitted == expected
                {
                    return info;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("repo info did not reach uncommitted={expected}");
        };

        assert_eq!(wait_for(&mut app, 0).uncommitted, 0);
        test_repo::write(repo.path(), "a.txt", "two\n");
        assert_eq!(
            app.world_mut()
                .get_mut::<RepoInfoCache>(cache)
                .unwrap()
                .snapshot(&path)
                .unwrap()
                .uncommitted,
            0
        );
        app.world_mut()
            .get_mut::<RepoInfoCache>(cache)
            .unwrap()
            .invalidate(&path, false);
        assert_eq!(wait_for(&mut app, 1).uncommitted, 1);
    }

    #[test]
    fn snapshot_requests_do_not_register_watch_targets() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = PathIdentity::resolve(repo.path()).into_path_buf();
        let cache = RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            wake: None,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (start_repo_info_loads, poll_info).chain())
            .add_systems(PostUpdate, info);
        app.insert_non_send(GitWatch::test());
        let cache = app.world_mut().spawn(cache).id();

        for _ in 0..500 {
            let loaded = app
                .world_mut()
                .get_mut::<RepoInfoCache>(cache)
                .unwrap()
                .snapshot(&path)
                .is_some();
            app.update();
            if loaded {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            app.world_mut()
                .get_mut::<RepoInfoCache>(cache)
                .unwrap()
                .snapshot(&path)
                .is_some()
        );
        assert!(
            app.world()
                .non_send::<GitWatch>()
                .repo_info_subscriptions
                .is_empty()
        );
    }

    #[test]
    fn repo_info_cache_keeps_changes_that_arrive_during_refresh() {
        IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = PathIdentity::resolve(repo.path()).into_path_buf();
        let stale = super::super::worktree::RepoInfo::read(&path);
        test_repo::write(repo.path(), "a.txt", "two\n");
        let cache = RepoInfoCache {
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            entries: HashMap::from([(
                path.clone(),
                RepoInfoCacheEntry {
                    info: None,
                    loaded: false,
                    dirty: false,
                    branches_dirty: false,
                    watch_attempted: false,
                    last_accessed: Instant::now(),
                    pending: Some(IoTaskPool::get().spawn(async move { stale })),
                },
            )]),
            wake: None,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, (start_repo_info_loads, poll_info).chain());
        let cache = app.world_mut().spawn(cache).id();
        app.world_mut()
            .get_mut::<RepoInfoCache>(cache)
            .unwrap()
            .invalidate(&path, false);
        for _ in 0..500 {
            app.update();
            if app
                .world_mut()
                .get_mut::<RepoInfoCache>(cache)
                .unwrap()
                .snapshot(&path)
                .is_some_and(|info| info.uncommitted == 1)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("repo info stayed stale after an in-flight invalidation");
    }
}
