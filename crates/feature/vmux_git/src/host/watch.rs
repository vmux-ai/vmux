use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use super::GitUpdateSet;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vmux_ecs::PagePresented;
use vmux_path::PathIdentity;

use super::runtime::{GitRuntime, GitTask};

pub(super) struct WatchPlugin;

impl Plugin for WatchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GitRuntime>()
            .init_resource::<GitWatchDeadline>()
            .add_systems(Startup, initialize)
            .add_systems(Startup, spawn_repo_info_cache)
            .add_systems(
                Update,
                (
                    sync_page_subscriptions.run_if(page_subscriptions_changed),
                    drain,
                    start_relevance_tasks,
                    poll_relevance_tasks,
                    start_repo_info_loads,
                    poll_info,
                )
                    .chain()
                    .in_set(GitUpdateSet::Watch),
            )
            .add_systems(PostUpdate, info.run_if(repo_info_maintenance_needed));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GitWatchDeadlineOwner {
    Subscriptions,
    RepoInfo,
}

#[derive(Resource, Clone, Default)]
struct GitWatchDeadline {
    state: Arc<(Mutex<GitWatchDeadlineState>, Condvar)>,
}

#[derive(Default)]
struct GitWatchDeadlineState {
    deadlines: HashMap<GitWatchDeadlineOwner, Instant>,
    proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    worker_started: bool,
}

impl GitWatchDeadline {
    fn schedule(
        &self,
        owner: GitWatchDeadlineOwner,
        deadline: Instant,
        proxy: Option<&EventLoopProxyWrapper>,
    ) {
        self.schedule_proxy(owner, deadline, proxy.map(|proxy| &**proxy));
    }

    fn schedule_proxy(
        &self,
        owner: GitWatchDeadlineOwner,
        deadline: Instant,
        proxy: Option<&bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) {
        let Some(proxy) = proxy else {
            return;
        };
        let start_worker = {
            let (state, wake) = &*self.state;
            let Ok(mut state) = state.lock() else {
                return;
            };
            if state
                .deadlines
                .get(&owner)
                .is_some_and(|current| *current <= deadline)
            {
                return;
            }
            state.deadlines.insert(owner, deadline);
            state.proxy = Some(proxy.clone());
            let start_worker = !state.worker_started;
            state.worker_started = true;
            wake.notify_one();
            start_worker
        };
        if start_worker {
            let state = Arc::clone(&self.state);
            let _ = std::thread::Builder::new()
                .name("vmux-git-watch-deadline".to_string())
                .spawn(move || Self::run(state));
        }
    }

    fn cancel(&self, owner: GitWatchDeadlineOwner) {
        let (state, wake) = &*self.state;
        let Ok(mut state) = state.lock() else {
            return;
        };
        if state.deadlines.remove(&owner).is_some() {
            wake.notify_one();
        }
    }

    fn run(state: Arc<(Mutex<GitWatchDeadlineState>, Condvar)>) {
        let (lock, wake) = &*state;
        let Ok(mut current) = lock.lock() else {
            return;
        };
        loop {
            let Some(deadline) = current.deadlines.values().copied().min() else {
                let Ok(next) = wake.wait(current) else {
                    return;
                };
                current = next;
                continue;
            };
            let delay = deadline.saturating_duration_since(Instant::now());
            if !delay.is_zero() {
                let Ok((next, timeout)) = wake.wait_timeout(current, delay) else {
                    return;
                };
                current = next;
                if !timeout.timed_out()
                    || current.deadlines.values().copied().min() != Some(deadline)
                {
                    continue;
                }
            }
            let now = Instant::now();
            current.deadlines.retain(|_, deadline| *deadline > now);
            let proxy = current.proxy.clone();
            drop(current);
            if let Some(proxy) = proxy {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
            }
            let Ok(next) = lock.lock() else {
                return;
            };
            current = next;
        }
    }
}

fn page_subscriptions_changed(
    presented: Query<(), Added<PagePresented>>,
    transitions: Query<(), Added<vmux_ecs::PageTransition>>,
    mut removed_presented: RemovedComponents<PagePresented>,
    mut removed_transitions: RemovedComponents<vmux_ecs::PageTransition>,
    mut removed_files: RemovedComponents<super::status::FileGit>,
    watch: Option<NonSend<GitWatch>>,
) -> bool {
    !presented.is_empty()
        || !transitions.is_empty()
        || removed_presented.read().next().is_some()
        || removed_transitions.read().next().is_some()
        || removed_files.read().next().is_some()
        || watch.is_some_and(|watch| watch.retry_due(Instant::now()))
}

fn repo_info_maintenance_needed(repo_info: Single<&RepoInfoCache>) -> bool {
    repo_info.maintenance_needed(Instant::now())
}

fn initialize(world: &mut World) {
    let proxy = world
        .get_resource::<EventLoopProxyWrapper>()
        .map(|wrapper| (**wrapper).clone());
    let retry_wake = proxy
        .clone()
        .map(|proxy| (world.resource::<GitWatchDeadline>().clone(), proxy));
    let events = GitWatchEvents::start(proxy);
    let tx = events.sender;
    match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if !GitWatch::should_forward(&result) {
            return;
        }
        let Ok(event) = result else {
            return;
        };
        tx.send(event.paths);
    }) {
        Ok(watcher) => {
            world.insert_non_send(GitWatch {
                watcher: GitFileWatcher::Recommended(watcher),
                rx: events.receiver,
                watch_references: HashMap::new(),
                subscriptions: HashMap::new(),
                subscription_owners: HashMap::new(),
                repo_info_subscriptions: HashMap::new(),
                repo_info_owners: HashMap::new(),
                retry_wake,
            });
        }
        Err(error) => bevy::log::warn!("git watcher init failed: {error}"),
    }
}

fn spawn_repo_info_cache(proxy: Option<Res<EventLoopProxyWrapper>>, mut commands: Commands) {
    let wake = proxy.map(|wrapper| (**wrapper).clone());
    commands.spawn((Name::new("Repository info cache"), RepoInfoCache::new(wake)));
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct GitWatchTarget {
    path: PathBuf,
    recursive: bool,
    kind: GitWatchKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GitWatchKind {
    Worktree,
    Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitSubscriptionKind {
    Page,
    File,
}

struct GitSubscription {
    kind: GitSubscriptionKind,
    path: PathBuf,
    repo_root: PathBuf,
    targets: Vec<GitWatchTarget>,
    complete: bool,
    retry_at: Instant,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct GitWatchSubscription {
    repo_root: PathBuf,
    targets: Vec<GitWatchTarget>,
}

pub(super) struct GitWatch {
    watcher: GitFileWatcher,
    rx: mpsc::Receiver<GitWatchBatch>,
    watch_references: HashMap<GitWatchTarget, usize>,
    subscriptions: HashMap<Entity, GitSubscription>,
    subscription_owners: HashMap<GitWatchTarget, HashSet<Entity>>,
    repo_info_subscriptions: HashMap<PathBuf, Vec<GitWatchTarget>>,
    repo_info_owners: HashMap<GitWatchTarget, HashSet<PathBuf>>,
    retry_wake: Option<(
        GitWatchDeadline,
        bevy::winit::EventLoopProxy<WinitUserEvent>,
    )>,
}

enum GitFileWatcher {
    Recommended(RecommendedWatcher),
    #[cfg(test)]
    Test,
}

impl GitFileWatcher {
    fn watch_targets(
        &mut self,
        targets: &[GitWatchTarget],
        previous: &[GitWatchTarget],
    ) -> notify::Result<()> {
        if targets.is_empty() {
            return Ok(());
        }
        match self {
            Self::Recommended(watcher) => {
                let mut paths = watcher.paths_mut();
                let mut added: Vec<&Path> = Vec::new();
                for target in targets {
                    let mode = if target.recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    };
                    if let Err(error) = paths.add(&target.path, mode) {
                        for path in &added {
                            let _ = paths.remove(path);
                        }
                        for target in previous {
                            let mode = if target.recursive {
                                RecursiveMode::Recursive
                            } else {
                                RecursiveMode::NonRecursive
                            };
                            let _ = paths.add(&target.path, mode);
                        }
                        let _ = paths.commit();
                        return Err(error);
                    }
                    added.push(target.path.as_path());
                }
                paths.commit()
            }
            #[cfg(test)]
            Self::Test => Ok(()),
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Recommended(watcher) => watcher.unwatch(path),
            #[cfg(test)]
            Self::Test => Ok(()),
        }
    }
}

#[derive(Clone, Default)]
struct GitWatchRelevanceBatch(HashMap<PathBuf, GitWatchRelevanceRoot>);

#[derive(Clone, Default)]
struct GitWatchRelevanceRoot {
    changed: HashSet<PathBuf>,
    entities: HashSet<Entity>,
    repo_info: HashSet<PathBuf>,
}

#[derive(Clone, Default)]
struct GitWatchRelevanceOutput {
    entities: HashMap<Entity, HashSet<PathBuf>>,
    repo_info: HashSet<PathBuf>,
}

#[derive(Default)]
struct GitWatchOwners {
    entities: HashSet<Entity>,
    repo_info: HashSet<PathBuf>,
}

#[derive(Component, Clone)]
struct PendingGitWatchRelevance(GitWatchRelevanceBatch);

#[derive(Component)]
struct GitWatchRelevanceTask {
    fallback: GitWatchRelevanceOutput,
    task: GitTask<GitWatchRelevanceOutput>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct GitWatchRefreshes<'w, 's> {
    states: super::state::GitStates<'w, 's>,
    files: super::status::FileGitStates<'w, 's>,
    presented: Query<'w, 's, (), With<PagePresented>>,
    transitioning: Query<'w, 's, (), With<vmux_ecs::PageTransition>>,
    wake: Option<Res<'w, EventLoopProxyWrapper>>,
    commands: Commands<'w, 's>,
}

impl GitWatchRefreshes<'_, '_> {
    fn active(&self, entity: Entity) -> bool {
        self.presented.contains(entity) || self.transitioning.contains(entity)
    }

    fn request(&mut self, entity: Entity, refresh_catalogs: bool) {
        if self.states.contains(entity) {
            if let Some((path, revision)) = self.states.mark_changed(entity) {
                self.commands.spawn((
                    super::job_runner::GitJob::new(entity),
                    super::job::RepositoryJob {
                        path: path.into(),
                        refresh_catalogs,
                        revision,
                    },
                ));
            }
        } else if let Some(refresh) = self
            .files
            .schedule_changed(entity, self.wake.as_deref().map(|wake| (**wake).clone()))
        {
            self.commands.entity(entity).insert(refresh);
        }
    }
}

impl GitWatchRelevanceBatch {
    fn add_entity(
        &mut self,
        entity: Entity,
        targets: &[GitWatchTarget],
        changed: &HashSet<PathBuf>,
    ) {
        for target in targets {
            if target.kind != GitWatchKind::Worktree {
                continue;
            }
            let matching = changed
                .iter()
                .filter(|path| target.matches(path))
                .cloned()
                .collect::<Vec<_>>();
            if matching.is_empty() {
                continue;
            }
            let root = self.0.entry(target.path.clone()).or_default();
            root.changed.extend(matching);
            root.entities.insert(entity);
        }
    }

    fn add_repo_info(
        &mut self,
        path: &Path,
        targets: &[GitWatchTarget],
        changed: &HashSet<PathBuf>,
    ) {
        for target in targets {
            if target.kind != GitWatchKind::Worktree {
                continue;
            }
            let matching = changed
                .iter()
                .filter(|path| target.matches(path))
                .cloned()
                .collect::<Vec<_>>();
            if matching.is_empty() {
                continue;
            }
            let root = self.0.entry(target.path.clone()).or_default();
            root.changed.extend(matching);
            root.repo_info.insert(path.to_path_buf());
        }
    }

    fn read(self) -> GitWatchRelevanceOutput {
        let mut output = GitWatchRelevanceOutput::default();
        for (root, candidates) in self.0 {
            let repository = super::repository::GitRepository::at(&root);
            if !repository.has_unignored_changes(candidates.changed.iter().map(PathBuf::as_path)) {
                continue;
            }
            for entity in candidates.entities {
                output
                    .entities
                    .entry(entity)
                    .or_default()
                    .insert(root.clone());
            }
            output.repo_info.extend(candidates.repo_info);
        }
        output
    }

    fn all(&self) -> GitWatchRelevanceOutput {
        let mut output = GitWatchRelevanceOutput::default();
        for (root, candidates) in &self.0 {
            for entity in &candidates.entities {
                output
                    .entities
                    .entry(*entity)
                    .or_default()
                    .insert(root.clone());
            }
            output
                .repo_info
                .extend(candidates.repo_info.iter().cloned());
        }
        output
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct RepoInfoCacheEntry {
    info: Option<super::worktree::RepoInfo>,
    loaded: bool,
    dirty: bool,
    branches_dirty: bool,
    watch_attempted: bool,
    last_accessed: Instant,
    pending: Option<GitTask<Option<super::worktree::RepoInfo>>>,
}

#[derive(PartialEq, Eq)]
struct RepoInfoLease {
    paths: HashSet<PathBuf>,
    active_only: bool,
}

const REPO_INFO_IDLE_TTL: Duration = Duration::from_secs(300);
const WATCH_RETRY_DELAY: Duration = Duration::from_secs(1);
const REPO_INFO_OWNER_PATH_LIMIT: usize = 256;
const REPO_INFO_CACHE_CAPACITY: usize = 1024;
const REPO_INFO_LOAD_BUDGET: usize = 16;
const REPO_INFO_WATCH_BUDGET: usize = 4;

#[derive(Component)]
pub struct RepoInfoCache {
    entries: HashMap<PathBuf, RepoInfoCacheEntry>,
    dirty: HashSet<PathBuf>,
    pending: HashSet<PathBuf>,
    canonical: HashMap<PathBuf, PathBuf>,
    repositories: HashMap<PathBuf, PathBuf>,
    leases: HashMap<Entity, RepoInfoLease>,
    maintenance_dirty: bool,
    next_gc: Instant,
    wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
}

impl RepoInfoCache {
    fn new(wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>) -> Self {
        Self {
            entries: HashMap::new(),
            dirty: HashSet::new(),
            pending: HashSet::new(),
            canonical: HashMap::new(),
            repositories: HashMap::new(),
            leases: HashMap::new(),
            maintenance_dirty: true,
            next_gc: Instant::now() + REPO_INFO_IDLE_TTL,
            wake,
        }
    }

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

    pub fn cached(&mut self, path: &Path) -> Option<super::worktree::RepoInfo> {
        let path = self.entry_path(path);
        let entry = self.entries.get_mut(&path)?;
        entry.last_accessed = Instant::now();
        entry.info.clone()
    }

    pub fn cached_project_root(&self, path: &Path) -> Option<PathBuf> {
        let mut selected = None;
        let mut length = 0;
        for entry in self.entries.values() {
            let Some(info) = entry.info.as_ref() else {
                continue;
            };
            if path.starts_with(&info.repo_root) && info.repo_root.as_os_str().len() > length {
                length = info.repo_root.as_os_str().len();
                selected = Some(info.project_root().to_path_buf());
            }
        }
        selected
    }

    pub fn observe_active_paths(
        &mut self,
        owner: Entity,
        paths: impl IntoIterator<Item = PathBuf>,
    ) {
        let mut observed = HashSet::new();
        for path in paths {
            observed.insert(self.entry_path(&path));
            if observed.len() >= REPO_INFO_OWNER_PATH_LIMIT {
                break;
            }
        }
        let lease = RepoInfoLease {
            paths: observed.clone(),
            active_only: true,
        };
        if self.leases.get(&owner) != Some(&lease) {
            self.maintenance_dirty = true;
        }
        self.leases.insert(owner, lease);
        for path in observed {
            let _ = self.request(path);
        }
    }

    pub fn release(&mut self, owner: Entity) {
        if self.leases.remove(&owner).is_some() {
            self.maintenance_dirty = true;
        }
    }

    fn observe_with(
        &mut self,
        owner: Entity,
        path: &Path,
        active_only: bool,
    ) -> Option<super::worktree::RepoInfo> {
        let path = self.entry_path(path);
        let paths = HashSet::from([path.clone()]);
        let lease = RepoInfoLease { paths, active_only };
        if self.leases.get(&owner) != Some(&lease) {
            self.maintenance_dirty = true;
        }
        self.leases.insert(owner, lease);
        self.request(path)
    }

    fn request(&mut self, path: PathBuf) -> Option<super::worktree::RepoInfo> {
        if !self.entries.contains_key(&path) {
            if self.entries.len() >= REPO_INFO_CACHE_CAPACITY {
                let leased = self
                    .leases
                    .values()
                    .flat_map(|lease| lease.paths.iter())
                    .collect::<HashSet<_>>();
                let expired = self
                    .entries
                    .iter()
                    .filter(|(candidate, entry)| {
                        entry.pending.is_none() && !leased.contains(candidate)
                    })
                    .min_by_key(|(_, entry)| entry.last_accessed)
                    .map(|(candidate, _)| candidate.clone());
                if let Some(expired) = expired {
                    self.remove(&expired);
                }
            }
            if self.entries.len() >= REPO_INFO_CACHE_CAPACITY {
                return None;
            }
            self.maintenance_dirty = true;
            self.dirty.insert(path.clone());
            self.entries.insert(
                path.clone(),
                RepoInfoCacheEntry {
                    info: None,
                    loaded: false,
                    dirty: true,
                    branches_dirty: true,
                    watch_attempted: false,
                    last_accessed: Instant::now(),
                    pending: None,
                },
            );
        }
        let entry = self.entries.get_mut(&path)?;
        if entry.dirty && entry.pending.is_none() {
            self.dirty.insert(path);
        }
        entry.last_accessed = Instant::now();
        entry.info.clone()
    }

    fn entry_path(&mut self, path: &Path) -> PathBuf {
        let requested = path.to_path_buf();
        let path = PathIdentity::resolve(path).into_path_buf();
        if self.canonical.len() >= CANONICAL_CAP && !self.canonical.contains_key(&requested) {
            self.canonical.clear();
        }
        self.canonical.insert(requested, path.clone());
        self.repositories.get(&path).cloned().unwrap_or(path)
    }

    fn remember_repository(&mut self, path: PathBuf, info: &super::worktree::RepoInfo) {
        let root = info.repo_root.clone();
        let entry = self
            .repositories
            .get(&root)
            .cloned()
            .unwrap_or_else(|| path.clone());
        if entry != path {
            self.remove(&path);
        }
        self.canonical.insert(path.clone(), info.identity.clone());
        self.repositories.insert(path, entry.clone());
        self.repositories
            .insert(info.identity.clone(), entry.clone());
        self.repositories.entry(root).or_insert(entry);
    }

    fn canonical_path(&mut self, path: &Path) -> PathBuf {
        if let Some(known) = self.canonical.get(path) {
            return known.clone();
        }
        if self.canonical.len() >= CANONICAL_CAP {
            self.canonical.clear();
        }
        let resolved = PathIdentity::lexical(path).into_path_buf();
        self.canonical.insert(path.to_path_buf(), resolved.clone());
        resolved
    }

    fn invalidate(&mut self, path: &Path, branches_dirty: bool) {
        if let Some(entry) = self.entries.get_mut(path) {
            entry.dirty = true;
            entry.branches_dirty |= branches_dirty;
            if entry.pending.is_none() {
                self.dirty.insert(path.to_path_buf());
            }
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
        self.dirty.remove(path);
        self.pending.remove(path);
        self.canonical.retain(|_, canonical| canonical != path);
        self.repositories.retain(|_, entry| entry != path);
    }

    fn maintenance_needed(&self, now: Instant) -> bool {
        self.maintenance_dirty || now >= self.next_gc
    }

    fn schedule_next_gc(&mut self, active: &HashSet<PathBuf>, now: Instant) {
        self.next_gc = self
            .entries
            .iter()
            .filter(|(path, entry)| !active.contains(*path) && entry.pending.is_none())
            .map(|(_, entry)| entry.last_accessed + REPO_INFO_IDLE_TTL)
            .min()
            .unwrap_or(now + REPO_INFO_IDLE_TTL);
    }
}

const WATCH_EVENT_LIMIT: usize = 8192;
const WATCH_INPUT_CAPACITY: usize = 64;
const WATCH_OUTPUT_CAPACITY: usize = 1;
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

    fn mark_overflow(&mut self) {
        self.paths.clear();
        self.overflow = true;
    }
}

#[derive(Clone)]
struct GitWatchSender {
    sender: mpsc::SyncSender<Vec<PathBuf>>,
    overflow: Arc<AtomicBool>,
}

impl GitWatchSender {
    fn send(&self, mut paths: Vec<PathBuf>) {
        if paths.len() > WATCH_EVENT_LIMIT {
            paths.clear();
            self.overflow.store(true, Ordering::Release);
        }
        match self.sender.try_send(paths) {
            Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
            }
        }
    }
}

struct GitWatchEvents {
    sender: GitWatchSender,
    receiver: mpsc::Receiver<GitWatchBatch>,
}

impl GitWatchEvents {
    fn start(proxy: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>) -> Self {
        let (input_tx, input_rx) = mpsc::sync_channel::<Vec<PathBuf>>(WATCH_INPUT_CAPACITY);
        let (output_tx, output_rx) = mpsc::sync_channel::<GitWatchBatch>(WATCH_OUTPUT_CAPACITY);
        let overflow = Arc::new(AtomicBool::new(false));
        let worker_overflow = Arc::clone(&overflow);
        let worker = std::thread::Builder::new()
            .name("vmux-git-watch-debounce".to_string())
            .spawn(move || {
                while let Ok(paths) = input_rx.recv() {
                    let started = Instant::now();
                    let mut batch = GitWatchBatch::default();
                    batch.extend(paths);
                    if worker_overflow.swap(false, Ordering::AcqRel) {
                        batch.mark_overflow();
                    }
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
                        if worker_overflow.swap(false, Ordering::AcqRel) {
                            batch.mark_overflow();
                        }
                    }
                    if worker_overflow.swap(false, Ordering::AcqRel) {
                        batch.mark_overflow();
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
            sender: GitWatchSender {
                sender: input_tx,
                overflow,
            },
            receiver: output_rx,
        }
    }
}

impl GitWatchTarget {
    #[cfg(test)]
    fn for_checkout(file: &Path) -> Result<(PathBuf, Vec<Self>), super::repository::GitError> {
        let repository = super::repository::GitRepository::discover(file)?;
        let root = repository.path().to_path_buf();
        let subscription = GitWatchSubscription::read(&root)?;
        Ok((subscription.repo_root, subscription.targets))
    }

    fn for_repo_info(path: &Path, info: Option<&super::worktree::RepoInfo>) -> Vec<Self> {
        let Some(info) = info else {
            return vec![Self {
                path: PathIdentity::lexical(path).into_path_buf(),
                recursive: true,
                kind: GitWatchKind::Worktree,
            }];
        };
        let repo_root = info.repo_root.clone();
        let git_dir = info.git_dir.clone();
        let common_dir = info.common_dir.clone();
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
        if let Some(worktrees) = &info.worktrees_dir {
            targets.push(Self {
                path: worktrees.clone(),
                recursive: true,
                kind: GitWatchKind::Metadata,
            });
        }
        targets
    }

    fn resolve_path(root: &Path, value: &str) -> PathBuf {
        let path = PathBuf::from(value);
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

impl GitWatchSubscription {
    pub(super) fn read(root: &Path) -> Result<Self, super::repository::GitError> {
        let git_dir = Self::read_path(root, "--absolute-git-dir")?;
        let common_dir = Self::read_path(root, "--git-common-dir")?;
        let mut targets = vec![
            GitWatchTarget {
                path: PathIdentity::resolve(root).into_path_buf(),
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
        let worktrees = common_dir.join("worktrees");
        if worktrees.is_dir() {
            targets.push(GitWatchTarget {
                path: worktrees,
                recursive: true,
                kind: GitWatchKind::Metadata,
            });
        }
        Ok(Self {
            repo_root: root.to_path_buf(),
            targets,
        })
    }

    fn read_path(root: &Path, argument: &str) -> Result<PathBuf, super::repository::GitError> {
        let (stdout, stderr, ok) =
            super::repository::GitCommand::run(root, &["rev-parse", argument])?;
        if !ok {
            return Err(super::repository::GitCommand::error(&stdout, &stderr));
        }
        let value = stdout.strip_suffix('\n').unwrap_or(&stdout);
        if value.is_empty() {
            return Err(super::repository::GitError("missing git directory".into()));
        }
        Ok(GitWatchTarget::resolve_path(root, value))
    }
}

impl GitWatch {
    fn has_page_subscription(&self, entity: Entity, path: &Path) -> bool {
        let path = PathIdentity::lexical(path);
        self.subscriptions.get(&entity).is_some_and(|subscription| {
            subscription.kind == GitSubscriptionKind::Page
                && subscription.complete
                && PathIdentity::lexical(&subscription.path) == path
        })
    }

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

    fn subscribe(
        &mut self,
        entity: Entity,
        path: &Path,
        kind: GitSubscriptionKind,
        prepared: GitWatchSubscription,
    ) -> Result<PathBuf, super::repository::GitError> {
        let path = PathIdentity::lexical(path).into_path_buf();
        if let Some(subscription) = self.subscriptions.get_mut(&entity).filter(|subscription| {
            subscription.path == path
                && subscription.repo_root == prepared.repo_root
                && subscription.targets == prepared.targets
                && subscription.complete
        }) {
            subscription.kind = kind;
            return Ok(subscription.repo_root.clone());
        }
        let repo_root = prepared.repo_root;
        let targets = prepared.targets;
        let complete = self.acquire_targets(&targets);
        let retry_at = Instant::now() + WATCH_RETRY_DELAY;
        let previous = self.subscriptions.insert(
            entity,
            GitSubscription {
                kind,
                path,
                repo_root: repo_root.clone(),
                targets: targets.clone(),
                complete,
                retry_at,
            },
        );
        if let Some(previous) = previous.filter(|previous| previous.complete) {
            self.remove_subscription_owner(entity, &previous.targets);
            self.release_targets(&previous.targets);
        }
        if complete {
            self.add_subscription_owner(entity, &targets);
        }
        if !complete {
            self.schedule_retry(retry_at);
        }
        Ok(repo_root)
    }

    pub(super) fn subscribe_file(
        &mut self,
        entity: Entity,
        path: &Path,
        prepared: GitWatchSubscription,
    ) -> Result<PathBuf, super::repository::GitError> {
        self.subscribe(entity, path, GitSubscriptionKind::File, prepared)
    }

    pub(super) fn subscribe_page(
        &mut self,
        entity: Entity,
        path: &Path,
        prepared: GitWatchSubscription,
    ) -> Result<PathBuf, super::repository::GitError> {
        self.subscribe(entity, path, GitSubscriptionKind::Page, prepared)
    }

    pub(super) fn take_page_subscription(
        &mut self,
        entity: Entity,
    ) -> Option<GitWatchSubscription> {
        if self
            .subscriptions
            .get(&entity)
            .is_none_or(|subscription| subscription.kind != GitSubscriptionKind::Page)
        {
            return None;
        }
        let subscription = self.subscriptions.remove(&entity)?;
        if subscription.complete {
            self.remove_subscription_owner(entity, &subscription.targets);
            self.release_targets(&subscription.targets);
        }
        Some(GitWatchSubscription {
            repo_root: subscription.repo_root,
            targets: subscription.targets,
        })
    }

    #[cfg(test)]
    fn subscribe_file_now(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Result<PathBuf, super::repository::GitError> {
        let repository = super::repository::GitRepository::discover(path)?;
        let prepared = GitWatchSubscription::read(repository.path())?;
        self.subscribe_file(entity, path, prepared)
    }

    pub(super) fn release_subscription(&mut self, entity: Entity) {
        let Some(subscription) = self.subscriptions.remove(&entity) else {
            return;
        };
        if subscription.complete {
            self.remove_subscription_owner(entity, &subscription.targets);
            self.release_targets(&subscription.targets);
        }
    }

    fn retry_due(&self, now: Instant) -> bool {
        self.subscriptions
            .values()
            .any(|subscription| !subscription.complete && now >= subscription.retry_at)
    }

    fn schedule_retry(&self, retry_at: Instant) {
        let Some((deadline, proxy)) = self.retry_wake.as_ref() else {
            return;
        };
        deadline.schedule_proxy(GitWatchDeadlineOwner::Subscriptions, retry_at, Some(proxy));
    }

    fn retry_subscription(&mut self, entity: Entity, now: Instant) {
        let Some(subscription) = self.subscriptions.get(&entity) else {
            return;
        };
        if subscription.complete || now < subscription.retry_at {
            return;
        }
        let targets = subscription.targets.clone();
        let complete = self.acquire_targets(&targets);
        let Some(subscription) = self.subscriptions.get_mut(&entity) else {
            if complete {
                self.release_targets(&targets);
            }
            return;
        };
        subscription.complete = complete;
        subscription.retry_at = now + WATCH_RETRY_DELAY;
        let retry_at = subscription.retry_at;
        if complete {
            self.add_subscription_owner(entity, &targets);
        }
        if !complete {
            self.schedule_retry(retry_at);
        }
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
        let missing = targets
            .iter()
            .filter(|target| !self.watch_references.contains_key(*target))
            .cloned()
            .collect::<Vec<_>>();
        let previous = self.watch_references.keys().cloned().collect::<Vec<_>>();
        if self.watcher.watch_targets(&missing, &previous).is_err() {
            return false;
        }
        for target in targets {
            if let Some(references) = self.watch_references.get_mut(target) {
                *references += 1;
                continue;
            }
            self.watch_references.insert(target.clone(), 1);
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

    fn add_subscription_owner(&mut self, entity: Entity, targets: &[GitWatchTarget]) {
        for target in targets {
            self.subscription_owners
                .entry(target.clone())
                .or_default()
                .insert(entity);
        }
    }

    fn remove_subscription_owner(&mut self, entity: Entity, targets: &[GitWatchTarget]) {
        for target in targets {
            let remove = self
                .subscription_owners
                .get_mut(target)
                .is_some_and(|owners| {
                    owners.remove(&entity);
                    owners.is_empty()
                });
            if remove {
                self.subscription_owners.remove(target);
            }
        }
    }

    fn add_repo_info_owner(&mut self, path: &Path, targets: &[GitWatchTarget]) {
        for target in targets {
            self.repo_info_owners
                .entry(target.clone())
                .or_default()
                .insert(path.to_path_buf());
        }
    }

    fn remove_repo_info_owner(&mut self, path: &Path, targets: &[GitWatchTarget]) {
        for target in targets {
            let remove = self.repo_info_owners.get_mut(target).is_some_and(|owners| {
                owners.remove(path);
                owners.is_empty()
            });
            if remove {
                self.repo_info_owners.remove(target);
            }
        }
    }

    fn matching_owners(&self, changed: &HashSet<PathBuf>) -> GitWatchOwners {
        let mut owners = GitWatchOwners::default();
        for (target, entities) in &self.subscription_owners {
            if changed.iter().any(|path| target.matches(path)) {
                owners.entities.extend(entities);
            }
        }
        for (target, paths) in &self.repo_info_owners {
            if changed.iter().any(|path| target.matches(path)) {
                owners.repo_info.extend(paths.iter().cloned());
            }
        }
        owners
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
                self.remove_repo_info_owner(&path, &targets);
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
                self.remove_repo_info_owner(&path, &targets);
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
        let path = PathIdentity::lexical(path).into_path_buf();
        let targets = GitWatchTarget::for_repo_info(&path, info);
        if self.repo_info_subscriptions.get(&path) == Some(&targets) {
            return true;
        }
        if !self.acquire_targets(&targets) {
            return false;
        }
        let previous = self
            .repo_info_subscriptions
            .insert(path.clone(), targets.clone());
        if let Some(previous) = previous {
            self.remove_repo_info_owner(&path, &previous);
            self.release_targets(&previous);
        }
        self.add_repo_info_owner(&path, &targets);
        true
    }

    #[cfg(test)]
    pub(super) fn test() -> Self {
        let (_, rx) = mpsc::channel();
        Self {
            watcher: GitFileWatcher::Test,
            rx,
            watch_references: HashMap::new(),
            subscriptions: HashMap::new(),
            subscription_owners: HashMap::new(),
            repo_info_subscriptions: HashMap::new(),
            repo_info_owners: HashMap::new(),
            retry_wake: None,
        }
    }

    #[cfg(test)]
    pub(super) fn has_subscription(&self, entity: Entity) -> bool {
        self.subscriptions.contains_key(&entity)
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct WatchRuntime<'w> {
    watch: Option<NonSendMut<'w, GitWatch>>,
    deadline: Option<Res<'w, GitWatchDeadline>>,
    proxy: Option<Res<'w, EventLoopProxyWrapper>>,
}

type SubscriptionPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut super::state::GitState,
        Option<Ref<'static, PagePresented>>,
        Has<vmux_ecs::PageTransition>,
        &'static mut super::state::GitRepositoryLoadRevision,
    ),
>;

fn sync_page_subscriptions(
    mut repo_info: Option<Single<&mut RepoInfoCache>>,
    runtime: WatchRuntime,
    mut pages: SubscriptionPages,
    files: Query<(), With<super::status::FileGit>>,
    repository_jobs: Query<&super::job_runner::GitJob, With<super::job::RepositoryJob>>,
    mut removed_files: RemovedComponents<super::status::FileGit>,
    mut commands: Commands,
) {
    if let Some(repo_info) = repo_info.as_mut() {
        repo_info.bypass_change_detection().maintenance_dirty = true;
    }
    let Some(mut watch_handle) = runtime.watch else {
        return;
    };
    let mut active_pages = HashSet::new();
    for (entity, mut state, presented, transitioning, mut revision) in &mut pages {
        if transitioning {
            active_pages.insert(entity);
        }
        let Some(presented) = presented else {
            continue;
        };
        active_pages.insert(entity);
        if !presented.is_added() {
            continue;
        }
        let path = PathBuf::from(state.workspace());
        if path.as_os_str().is_empty() || watch_handle.has_page_subscription(entity, &path) {
            continue;
        }
        if repository_jobs.iter().any(|job| job.owner() == entity) {
            continue;
        }
        watch_handle.release_subscription(entity);
        state.snapshot.nonce = state.snapshot.nonce.wrapping_add(1).max(1);
        commands.spawn((
            super::job_runner::GitJob::new(entity),
            super::job::RepositoryJob {
                path,
                refresh_catalogs: true,
                revision: revision.advance(),
            },
        ));
    }
    for entity in removed_files.read() {
        if !files.contains(entity)
            && watch_handle
                .subscriptions
                .get(&entity)
                .is_some_and(|subscription| subscription.kind == GitSubscriptionKind::File)
        {
            watch_handle.release_subscription(entity);
        }
    }
    let inactive = watch_handle
        .subscriptions
        .iter()
        .filter_map(|(entity, subscription)| {
            (subscription.kind == GitSubscriptionKind::Page && !active_pages.contains(entity))
                .then_some(*entity)
        })
        .collect::<Vec<_>>();
    for entity in inactive {
        watch_handle.release_subscription(entity);
    }
    let retry = watch_handle
        .subscriptions
        .iter()
        .filter_map(|(entity, subscription)| {
            (!subscription.complete
                && match subscription.kind {
                    GitSubscriptionKind::Page => active_pages.contains(entity),
                    GitSubscriptionKind::File => files.contains(*entity),
                })
            .then_some(*entity)
        })
        .collect::<Vec<_>>();
    let now = Instant::now();
    for entity in retry {
        watch_handle.retry_subscription(entity, now);
    }
    if let Some(deadline) = runtime.deadline.as_deref() {
        let next_retry = watch_handle
            .subscriptions
            .iter()
            .filter_map(|(entity, subscription)| {
                (!subscription.complete
                    && match subscription.kind {
                        GitSubscriptionKind::Page => active_pages.contains(entity),
                        GitSubscriptionKind::File => files.contains(*entity),
                    })
                .then_some(subscription.retry_at)
            })
            .min();
        match next_retry {
            Some(next_retry) => deadline.schedule(
                GitWatchDeadlineOwner::Subscriptions,
                next_retry,
                runtime.proxy.as_deref(),
            ),
            None => deadline.cancel(GitWatchDeadlineOwner::Subscriptions),
        }
    }
}

fn drain(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: Single<&mut RepoInfoCache>,
    mut refreshes: GitWatchRefreshes,
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
    let owners = if overflow {
        GitWatchOwners {
            entities: watch.subscriptions.keys().copied().collect(),
            repo_info: watch.repo_info_subscriptions.keys().cloned().collect(),
        }
    } else {
        watch.matching_owners(&changed)
    };
    let mut relevance = GitWatchRelevanceBatch::default();
    for entity in owners.entities {
        if !refreshes.active(entity) {
            continue;
        }
        let Some(subscription) = watch.subscriptions.get(&entity) else {
            continue;
        };
        let metadata = subscription.targets.iter().any(|target| {
            target.kind == GitWatchKind::Metadata && changed.iter().any(|path| target.matches(path))
        });
        if overflow || metadata {
            let refresh_catalogs = overflow
                || subscription
                    .targets
                    .iter()
                    .any(|target| changed.iter().any(|path| target.branches_changed(path)));
            refreshes.request(entity, refresh_catalogs);
        } else {
            relevance.add_entity(entity, &subscription.targets, &changed);
        }
    }
    for path in owners.repo_info {
        let Some(targets) = watch.repo_info_subscriptions.get(&path) else {
            continue;
        };
        let metadata = targets.iter().any(|target| {
            target.kind == GitWatchKind::Metadata
                && changed.iter().any(|changed| target.matches(changed))
        });
        if overflow || metadata {
            let branches_dirty = overflow
                || targets
                    .iter()
                    .any(|target| changed.iter().any(|path| target.branches_changed(path)));
            repo_info.invalidate(&path, branches_dirty);
        } else {
            relevance.add_repo_info(&path, targets, &changed);
        }
    }
    if !relevance.is_empty() {
        refreshes
            .commands
            .spawn(PendingGitWatchRelevance(relevance));
    }
}

fn start_relevance_tasks(
    pending: Query<(Entity, &PendingGitWatchRelevance), Without<GitWatchRelevanceTask>>,
    runtime: Res<GitRuntime>,
    wake: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let wake = wake.as_deref().map(|wake| (**wake).clone());
    for (entity, pending) in &pending {
        let batch = pending.0.clone();
        let fallback = batch.all();
        let Some(task) = runtime.spawn(wake.clone(), move || batch.read()) else {
            break;
        };
        commands
            .entity(entity)
            .remove::<PendingGitWatchRelevance>()
            .insert(GitWatchRelevanceTask { fallback, task });
    }
}

fn poll_relevance_tasks(
    mut tasks: Query<(Entity, &mut GitWatchRelevanceTask)>,
    watch: Option<NonSend<GitWatch>>,
    mut repo_info: Single<&mut RepoInfoCache>,
    mut refreshes: GitWatchRefreshes,
) {
    let Some(watch) = watch else {
        return;
    };
    for (task_entity, mut task) in &mut tasks {
        let Some(output) = task.task.poll() else {
            continue;
        };
        refreshes.commands.entity(task_entity).despawn();
        let output = output.unwrap_or_else(|_| task.fallback.clone());
        for (entity, roots) in output.entities {
            let current = refreshes.active(entity)
                && watch
                    .subscriptions
                    .get(&entity)
                    .is_some_and(|subscription| {
                        subscription.targets.iter().any(|target| {
                            target.kind == GitWatchKind::Worktree && roots.contains(&target.path)
                        })
                    });
            if current {
                refreshes.request(entity, false);
            }
        }
        let repo_info = repo_info.bypass_change_detection();
        for path in output.repo_info {
            if watch.repo_info_subscriptions.contains_key(&path) {
                repo_info.invalidate(&path, false);
            }
        }
    }
}

fn start_repo_info_loads(mut repo_info: Single<&mut RepoInfoCache>, runtime: Res<GitRuntime>) {
    let repo_info = repo_info.bypass_change_detection();
    let wake = repo_info.wake.clone();
    let paths = repo_info
        .dirty
        .iter()
        .take(REPO_INFO_LOAD_BUDGET)
        .cloned()
        .collect::<Vec<_>>();
    for path in paths {
        repo_info.dirty.remove(&path);
        let Some(entry) = repo_info.entries.get_mut(&path) else {
            continue;
        };
        if entry.pending.is_some() || (!entry.dirty && entry.loaded) {
            continue;
        }
        let wake = wake.clone();
        let refresh_branches = entry.branches_dirty;
        let cached_branches = if refresh_branches {
            None
        } else {
            entry.info.as_ref().map(|info| info.branches.clone())
        };
        let task_path = path.clone();
        let Some(task) = runtime.spawn_background(wake, move || {
            super::worktree::RepoInfo::read_with_branches(&task_path, cached_branches)
        }) else {
            repo_info.dirty.insert(path);
            continue;
        };
        entry.dirty = false;
        entry.branches_dirty &= !refresh_branches;
        entry.pending = Some(task);
        repo_info.pending.insert(path);
    }
}

fn poll_info(mut repo_info: Single<&mut RepoInfoCache>) {
    let (changed, repositories, completed) = {
        let repo_info = repo_info.bypass_change_detection();
        let mut changed = false;
        let mut completed = false;
        let mut repositories = Vec::new();
        let paths = repo_info.pending.iter().cloned().collect::<Vec<_>>();
        for path in paths {
            let Some(entry) = repo_info.entries.get_mut(&path) else {
                repo_info.pending.remove(&path);
                continue;
            };
            let Some(task) = entry.pending.as_mut() else {
                repo_info.pending.remove(&path);
                continue;
            };
            let Some(output) = task.poll() else {
                continue;
            };
            completed = true;
            let info = output.unwrap_or(None);
            changed |= entry.info != info;
            entry.info = info;
            entry.loaded = true;
            entry.watch_attempted = false;
            entry.pending = None;
            repo_info.pending.remove(&path);
            if entry.dirty {
                repo_info.dirty.insert(path.clone());
            }
            if let Some(info) = entry.info.as_ref() {
                repositories.push((path, info.clone()));
            }
        }
        (changed, repositories, completed)
    };
    for (path, info) in repositories {
        repo_info.remember_repository(path, &info);
    }
    if changed {
        repo_info.set_changed();
    }
    if completed {
        repo_info.bypass_change_detection().maintenance_dirty = true;
    }
}

fn info(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: Single<&mut RepoInfoCache>,
    entities: Query<(), ()>,
    presented_owners: Query<(), With<PagePresented>>,
    deadline: Option<Res<GitWatchDeadline>>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut watch_cursor: Local<usize>,
) {
    let repo_info = repo_info.bypass_change_detection();
    let now = Instant::now();
    repo_info.leases.retain(|owner, lease| {
        entities.contains(*owner) && (!lease.active_only || presented_owners.contains(*owner))
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
        repo_info.schedule_next_gc(&active, now);
        repo_info.maintenance_dirty = false;
        schedule_repo_info_deadline(deadline.as_deref(), proxy.as_deref(), repo_info);
        return;
    };
    watch.release_inactive_repo_info(repo_info, &active);
    watch.evict_inactive_repo_info(repo_info, &active);
    let mut paths: Vec<PathBuf> = repo_info
        .entries
        .iter()
        .filter_map(|(path, entry)| {
            (active.contains(path) && entry.loaded && !entry.watch_attempted)
                .then_some(path.clone())
        })
        .collect();
    let mut retry_watch = false;
    let pending_watch = paths.len() > REPO_INFO_WATCH_BUDGET;
    if !paths.is_empty() {
        let start = *watch_cursor % paths.len();
        paths.rotate_left(start);
        *watch_cursor = start + REPO_INFO_WATCH_BUDGET;
    }
    for path in paths.into_iter().take(REPO_INFO_WATCH_BUDGET) {
        let info = repo_info
            .entries
            .get(&path)
            .and_then(|entry| entry.info.clone());
        let subscribed = watch.subscribe_repo_info(&path, info.as_ref());
        if let Some(entry) = repo_info.entries.get_mut(&path) {
            entry.watch_attempted = subscribed;
        }
        retry_watch |= !subscribed;
    }
    repo_info.schedule_next_gc(&active, now);
    if retry_watch {
        repo_info.next_gc = repo_info.next_gc.min(now + WATCH_RETRY_DELAY);
    }
    if pending_watch && !retry_watch {
        repo_info.next_gc = repo_info.next_gc.min(now + Duration::from_millis(16));
    }
    repo_info.maintenance_dirty = false;
    schedule_repo_info_deadline(deadline.as_deref(), proxy.as_deref(), repo_info);
}

fn schedule_repo_info_deadline(
    deadline: Option<&GitWatchDeadline>,
    proxy: Option<&EventLoopProxyWrapper>,
    repo_info: &RepoInfoCache,
) {
    let Some(deadline) = deadline else {
        return;
    };
    if repo_info.entries.is_empty() {
        deadline.cancel(GitWatchDeadlineOwner::RepoInfo);
        return;
    }
    deadline.schedule(GitWatchDeadlineOwner::RepoInfo, repo_info.next_gc, proxy);
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::host::repository::test_repo;

    #[test]
    fn failed_watch_batches_preserve_previously_registered_files() {
        let retained = tempfile::tempdir().unwrap();
        let added = tempfile::tempdir().unwrap();
        let (sender, receiver) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                let _ = sender.send(event.paths);
            }
        })
        .unwrap();
        let mut watcher = GitFileWatcher::Recommended(watcher);
        let previous = GitWatchTarget {
            path: retained.path().to_path_buf(),
            recursive: true,
            kind: GitWatchKind::Worktree,
        };
        watcher
            .watch_targets(std::slice::from_ref(&previous), &[])
            .unwrap();
        let targets = [
            GitWatchTarget {
                path: added.path().to_path_buf(),
                recursive: true,
                kind: GitWatchKind::Worktree,
            },
            GitWatchTarget {
                path: added.path().join("missing"),
                recursive: true,
                kind: GitWatchKind::Worktree,
            },
        ];
        assert!(watcher.watch_targets(&targets, &[previous]).is_err());
        std::fs::write(retained.path().join("retained.txt"), "changed").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let paths = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("the original file watch must survive a failed batch");
            if paths.iter().any(|path| path.ends_with("retained.txt")) {
                break;
            }
        }
    }

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
    fn watch_subscription_preserves_newlines_in_repository_paths() {
        let parent = tempfile::tempdir().unwrap();
        let repo = parent.path().join("repository\nname");
        std::fs::create_dir(&repo).unwrap();
        test_repo::run(&repo, &["init", "-q", "-b", "main"]);

        let subscription = GitWatchSubscription::read(&repo).unwrap();
        let git_dir = PathIdentity::resolve(repo.join(".git")).into_path_buf();

        assert!(subscription.targets.contains(&GitWatchTarget {
            path: git_dir,
            recursive: false,
            kind: GitWatchKind::Metadata,
        }));
    }

    #[cfg(unix)]
    #[test]
    fn repo_info_cache_re_resolves_a_retargeted_symlink() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let alias = parent.path().join("repository");
        let mut cache = RepoInfoCache::new(None);
        symlink(first.path(), &alias).unwrap();

        assert_eq!(
            cache.entry_path(&alias),
            first.path().canonicalize().unwrap()
        );

        std::fs::remove_file(&alias).unwrap();
        symlink(second.path(), &alias).unwrap();

        assert_eq!(
            cache.entry_path(&alias),
            second.path().canonicalize().unwrap()
        );
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

        watch.subscribe_file_now(entity, &first_file).unwrap();
        assert!(watch.subscribe_repo_info(first.path(), Some(&first_info)));
        assert!(first_targets.iter().all(|target| {
            watch
                .watch_references
                .get(target)
                .is_some_and(|references| *references == 2)
        }));

        watch.subscribe_file_now(entity, &second_file).unwrap();

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
    fn changed_paths_select_only_matching_subscription_owners() {
        let first = test_repo::init();
        let second = test_repo::init();
        let first_entity = Entity::from_bits(1);
        let second_entity = Entity::from_bits(2);
        let mut watch = GitWatch::test();
        watch
            .subscribe_page(
                first_entity,
                first.path(),
                GitWatchSubscription::read(first.path()).unwrap(),
            )
            .unwrap();
        watch
            .subscribe_page(
                second_entity,
                second.path(),
                GitWatchSubscription::read(second.path()).unwrap(),
            )
            .unwrap();

        let changed =
            HashSet::from([PathIdentity::resolve(first.path().join("a.txt")).into_path_buf()]);
        let owners = watch.matching_owners(&changed);

        assert_eq!(owners.entities, HashSet::from([first_entity]));
        watch.release_subscription(first_entity);
        assert!(watch.matching_owners(&changed).entities.is_empty());
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
    fn incomplete_subscription_retries_after_capacity_is_released() {
        let repository = tempfile::tempdir().unwrap();
        let mut watch = GitWatch::test();
        for index in 0..WATCH_TARGET_LIMIT {
            watch.watch_references.insert(
                GitWatchTarget {
                    path: PathBuf::from(format!("/tmp/vmux-git-watch-retry-{index}")),
                    recursive: false,
                    kind: GitWatchKind::Metadata,
                },
                1,
            );
        }
        let target = GitWatchTarget {
            path: repository.path().to_path_buf(),
            recursive: false,
            kind: GitWatchKind::Metadata,
        };
        let entity = Entity::from_bits(1);
        watch
            .subscribe_page(
                entity,
                repository.path(),
                GitWatchSubscription {
                    repo_root: repository.path().to_path_buf(),
                    targets: vec![target.clone()],
                },
            )
            .unwrap();
        assert!(!watch.subscriptions[&entity].complete);

        let released = watch.watch_references.keys().next().cloned().unwrap();
        watch.watch_references.remove(&released);
        watch.retry_subscription(entity, Instant::now() + WATCH_RETRY_DELAY);

        assert!(watch.subscriptions[&entity].complete);
        assert_eq!(watch.watch_references.get(&target), Some(&1));
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
    fn relevance_batch_ignores_ignored_worktree_paths() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), ".gitignore", "target/\n");
        std::fs::create_dir_all(repo.path().join("target")).unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        let ignored = PathIdentity::resolve(test_repo::write(
            repo.path(),
            "target/cache.bin",
            "ignored\n",
        ))
        .into_path_buf();
        let source = PathIdentity::resolve(test_repo::write(
            repo.path(),
            "src/lib.rs",
            "fn source() {}\n",
        ))
        .into_path_buf();
        let root = PathIdentity::resolve(repo.path()).into_path_buf();
        let target = GitWatchTarget {
            path: root,
            recursive: true,
            kind: GitWatchKind::Worktree,
        };
        let entity = Entity::from_bits(1);
        let mut ignored_batch = GitWatchRelevanceBatch::default();
        ignored_batch.add_entity(
            entity,
            std::slice::from_ref(&target),
            &HashSet::from([ignored]),
        );
        let mut source_batch = GitWatchRelevanceBatch::default();
        source_batch.add_entity(
            entity,
            std::slice::from_ref(&target),
            &HashSet::from([source]),
        );

        assert!(!ignored_batch.read().entities.contains_key(&entity));
        assert!(source_batch.read().entities.contains_key(&entity));
    }

    #[test]
    fn bounded_watch_sender_marks_overflow_when_full() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let overflow = Arc::new(AtomicBool::new(false));
        let watch_sender = GitWatchSender {
            sender,
            overflow: Arc::clone(&overflow),
        };

        watch_sender.send(vec![PathBuf::from("/tmp/first")]);
        watch_sender.send(vec![PathBuf::from("/tmp/second")]);

        assert_eq!(receiver.recv().unwrap(), vec![PathBuf::from("/tmp/first")]);
        assert!(overflow.load(Ordering::Acquire));
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
            ..RepoInfoCache::new(None)
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
        let owner = app.world_mut().spawn(PagePresented).id();
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
            ..RepoInfoCache::new(None)
        };
        cache.observe_active_paths(owner, [first_path.clone(), second_path.clone()]);
        app.world_mut().spawn(cache);

        app.update();
        let watch = app.world().non_send::<GitWatch>();
        assert!(watch.repo_info_subscriptions.contains_key(&first_path));
        assert!(watch.repo_info_subscriptions.contains_key(&second_path));

        app.world_mut().entity_mut(owner).remove::<PagePresented>();
        app.update();
        assert!(
            app.world()
                .non_send::<GitWatch>()
                .repo_info_subscriptions
                .is_empty()
        );
    }

    #[test]
    fn active_owner_repository_paths_are_bounded() {
        let mut cache = RepoInfoCache::new(None);
        let owner = Entity::from_bits(1);
        let paths = std::iter::repeat_n(PathBuf::from("/tmp/vmux-repository-repeated"), 64).chain(
            (0..REPO_INFO_OWNER_PATH_LIMIT + 32)
                .map(|index| PathBuf::from(format!("/tmp/vmux-repository-{index}"))),
        );

        cache.observe_active_paths(owner, paths);

        assert_eq!(cache.leases[&owner].paths.len(), REPO_INFO_OWNER_PATH_LIMIT);
        assert_eq!(cache.entries.len(), REPO_INFO_OWNER_PATH_LIMIT);
    }

    #[test]
    fn repository_info_cache_capacity_is_bounded() {
        let mut cache = RepoInfoCache::new(None);
        for index in 0..REPO_INFO_CACHE_CAPACITY + 32 {
            let path = PathBuf::from(format!("/tmp/vmux-cache-{index}"));
            let _ = cache.snapshot(&path);
        }

        assert_eq!(cache.entries.len(), REPO_INFO_CACHE_CAPACITY);
    }

    #[test]
    fn page_watch_subscription_exists_only_while_the_page_is_active() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, super::super::job_runner::JobPlugin))
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
                PagePresented,
            ))
            .id();

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            app.update();
            if app
                .world()
                .non_send::<GitWatch>()
                .subscriptions
                .contains_key(&page)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            app.world()
                .non_send::<GitWatch>()
                .subscriptions
                .contains_key(&page)
        );
        app.world_mut().entity_mut(page).remove::<PagePresented>();
        app.update();
        assert!(app.world().non_send::<GitWatch>().subscriptions.is_empty());
    }

    #[test]
    fn activating_an_already_subscribed_page_does_not_reload_repository() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_systems(Update, sync_page_subscriptions);
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
                PagePresented,
            ))
            .id();
        let prepared = GitWatchSubscription::read(repo.path()).unwrap();
        app.world_mut()
            .non_send_mut::<GitWatch>()
            .subscribe_page(page, &file, prepared)
            .unwrap();

        app.update();

        let mut jobs = app.world_mut().query::<&super::super::job::RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 0);
    }

    #[test]
    fn presenting_a_page_with_a_pending_repository_load_does_not_queue_another() {
        let repo = test_repo::init();
        let mut app = App::new();
        app.add_systems(Update, sync_page_subscriptions);
        app.insert_non_send(GitWatch::test());
        let page = app
            .world_mut()
            .spawn((
                super::super::state::GitState {
                    snapshot: crate::state::GitPageSnapshot {
                        workspace: repo.path().to_string_lossy().into_owned(),
                        ..Default::default()
                    },
                },
                PagePresented,
            ))
            .id();
        app.world_mut().spawn((
            super::super::job_runner::GitJob::new(page),
            super::super::job::RepositoryJob {
                path: repo.path().to_path_buf(),
                refresh_catalogs: true,
                revision: 1,
            },
        ));

        app.update();

        let mut jobs = app.world_mut().query::<&super::super::job::RepositoryJob>();
        assert_eq!(jobs.iter(app.world()).count(), 1);
    }

    #[test]
    fn page_cleanup_preserves_editor_file_subscriptions() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_page_subscriptions);
        app.insert_non_send(GitWatch::test());
        let editor = app
            .world_mut()
            .spawn(super::super::status::FileGit::new(&file, 1))
            .id();
        app.world_mut()
            .non_send_mut::<GitWatch>()
            .subscribe_file_now(editor, &file)
            .unwrap();

        assert!(
            app.world_mut()
                .non_send_mut::<GitWatch>()
                .take_page_subscription(editor)
                .is_none()
        );

        app.update();

        assert!(app.world().non_send::<GitWatch>().has_subscription(editor));
    }

    #[test]
    fn file_cleanup_releases_editor_subscription() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_page_subscriptions);
        app.insert_non_send(GitWatch::test());
        let editor = app
            .world_mut()
            .spawn(super::super::status::FileGit::new(&file, 1))
            .id();
        app.world_mut()
            .non_send_mut::<GitWatch>()
            .subscribe_file_now(editor, &file)
            .unwrap();

        app.update();
        app.world_mut()
            .entity_mut(editor)
            .remove::<super::super::status::FileGit>();
        app.update();

        assert!(!app.world().non_send::<GitWatch>().has_subscription(editor));
    }

    #[test]
    fn file_replacement_keeps_editor_subscription() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, sync_page_subscriptions);
        app.insert_non_send(GitWatch::test());
        let editor = app
            .world_mut()
            .spawn(super::super::status::FileGit::new(&file, 1))
            .id();
        app.world_mut()
            .non_send_mut::<GitWatch>()
            .subscribe_file_now(editor, &file)
            .unwrap();

        app.update();
        app.world_mut()
            .entity_mut(editor)
            .remove::<super::super::status::FileGit>()
            .insert(super::super::status::FileGit::new(&file, 2));
        app.update();

        assert!(app.world().non_send::<GitWatch>().has_subscription(editor));
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
    fn known_repository_coalesces_nested_paths_after_discovery() {
        let repo = test_repo::init();
        let first = repo.path().join("first");
        let second = repo.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let mut cache = RepoInfoCache::new(None);

        assert!(cache.snapshot(&first).is_none());
        let info = super::super::worktree::RepoInfo::read(&first).unwrap();
        let first_key = PathIdentity::resolve(&first).into_path_buf();
        let first_entry = cache.entries.get_mut(&first_key).unwrap();
        first_entry.info = Some(info.clone());
        first_entry.loaded = true;
        cache.remember_repository(first_key, &info);
        assert!(cache.snapshot(&second).is_none());
        assert_eq!(cache.entries.len(), 2);

        let info = super::super::worktree::RepoInfo::read(&second).unwrap();
        let second_key = PathIdentity::resolve(&second).into_path_buf();
        let second_entry = cache.entries.get_mut(&second_key).unwrap();
        second_entry.info = Some(info.clone());
        second_entry.loaded = true;
        cache.remember_repository(second_key, &info);

        assert_eq!(cache.entries.len(), 1);
        assert!(cache.snapshot(&second).is_some());
    }

    #[test]
    fn nested_repository_gets_its_own_cache_entry() {
        let parent = test_repo::init();
        let nested = parent.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        test_repo::run(&nested, &["init", "-q", "-b", "main"]);
        let mut cache = RepoInfoCache::new(None);

        assert!(cache.snapshot(parent.path()).is_none());
        let parent_info = super::super::worktree::RepoInfo::read(parent.path()).unwrap();
        cache.remember_repository(
            PathIdentity::resolve(parent.path()).into_path_buf(),
            &parent_info,
        );

        assert!(cache.snapshot(&nested).is_none());
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(
            super::super::worktree::RepoInfo::read(&nested)
                .unwrap()
                .repo_root,
            nested.canonicalize().unwrap()
        );
    }

    #[test]
    fn repo_info_cache_refreshes_only_after_invalidation() {
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = PathIdentity::resolve(repo.path()).into_path_buf();
        let cache = RepoInfoCache::new(None);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitRuntime>()
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
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = PathIdentity::resolve(repo.path()).into_path_buf();
        let cache = RepoInfoCache::new(None);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<GitRuntime>()
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
        let repo = test_repo::init();
        test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        let path = PathIdentity::resolve(repo.path()).into_path_buf();
        let stale = super::super::worktree::RepoInfo::read(&path);
        test_repo::write(repo.path(), "a.txt", "two\n");
        let runtime = GitRuntime::default();
        let pending = runtime.spawn(None, move || stale).unwrap();
        let cache = RepoInfoCache {
            pending: HashSet::from([path.clone()]),
            entries: HashMap::from([(
                path.clone(),
                RepoInfoCacheEntry {
                    info: None,
                    loaded: false,
                    dirty: false,
                    branches_dirty: false,
                    watch_attempted: false,
                    last_accessed: Instant::now(),
                    pending: Some(pending),
                },
            )]),
            ..RepoInfoCache::new(None)
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(runtime)
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
