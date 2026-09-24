use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use vmux_core::host::FileUiStateUpdates;

use crate::event::GitChangedEvent;

use super::GitUpdateSet;

pub(super) struct WatchPlugin;

impl Plugin for WatchPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = mpsc::channel();
        let proxy = app
            .world()
            .get_resource::<EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if !should_forward_git_watch_result(&result) {
                return;
            }
            let _ = tx.send(result);
            if let Some(proxy) = proxy.as_ref() {
                let _ = proxy.send_event(WinitUserEvent::WakeUp);
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
        let wake = app
            .world()
            .get_resource::<EventLoopProxyWrapper>()
            .map(|wrapper| (**wrapper).clone());
        app.insert_resource(RepoInfoCache {
            entries: HashMap::new(),
            canonical: HashMap::new(),
            guessed: HashMap::new(),
            wake,
        })
        .add_systems(
            Update,
            (
                drain_git_watch,
                poll_repo_info_cache,
                sync_repo_info_watches,
            )
                .chain()
                .in_set(GitUpdateSet::Watch),
        );
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

pub(super) struct GitWatch {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    watch_references: HashMap<GitWatchTarget, usize>,
    subscriptions: HashMap<Entity, GitSubscription>,
    repo_info_subscriptions: HashMap<PathBuf, Vec<GitWatchTarget>>,
}

struct RepoInfoCacheEntry {
    info: Option<super::worktree::RepoInfo>,
    loaded: bool,
    dirty: bool,
    watched: bool,
    idle_syncs: u8,
    pending: Option<Task<Option<super::worktree::RepoInfo>>>,
    ignore_events_until: Option<Instant>,
}

const UNRESOLVED_RETRY: Duration = Duration::from_secs(1);

struct GuessedPath {
    guess: PathBuf,
    made_at: Instant,
}

impl From<&Path> for GuessedPath {
    fn from(path: &Path) -> Self {
        Self {
            guess: canonical(path),
            made_at: Instant::now(),
        }
    }
}

impl GuessedPath {
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

impl RepoInfoCache {
    pub fn get(&mut self, path: &Path) -> Option<super::worktree::RepoInfo> {
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

    fn poll_and_refresh(
        path: &Path,
        entry: &mut RepoInfoCacheEntry,
        wake: Option<bevy::winit::EventLoopProxy<WinitUserEvent>>,
    ) -> bool {
        let mut changed = false;
        if let Some(task) = entry.pending.as_mut()
            && let Some(info) = future::block_on(future::poll_once(task))
        {
            changed = entry.info != info;
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
                let info = super::worktree::repo_info(&path);
                if let Some(wake) = wake {
                    let _ = wake.send_event(WinitUserEvent::WakeUp);
                }
                info
            }));
        }
        changed
    }

    fn poll(&mut self) -> bool {
        let wake = self.wake.clone();
        let mut changed = false;
        for (path, entry) in &mut self.entries {
            changed |= Self::poll_and_refresh(path, entry, wake.clone());
        }
        changed
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

    fn wake_soon(&self) {
        let Some(wake) = &self.wake else {
            return;
        };
        let _ = wake.send_event(WinitUserEvent::WakeUp);
    }
}

const WATCH_DRAIN_BUDGET: usize = 256;
const CANONICAL_CAP: usize = 4096;

fn canonical(path: &Path) -> PathBuf {
    vmux_path::PathIdentity::resolve(path).into_path_buf()
}

fn resolve_git_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    canonical(&path)
}

fn git_watch_targets(
    file: &Path,
) -> Result<(PathBuf, Vec<GitWatchTarget>), super::runner::GitError> {
    let root = super::runner::repo_root(file)?;
    let (stdout, stderr, ok) = super::runner::git(
        &root,
        &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
    )?;
    if !ok {
        return Err(super::runner::git_err(&stdout, &stderr));
    }
    let mut lines = stdout.lines();
    let git_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| super::runner::GitError("missing git directory".into()))?;
    let common_dir = lines
        .next()
        .map(|line| resolve_git_path(&root, line))
        .ok_or_else(|| super::runner::GitError("missing common git directory".into()))?;
    let mut targets = vec![
        GitWatchTarget {
            path: canonical(&root),
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
    info: Option<&super::worktree::RepoInfo>,
) -> Vec<GitWatchTarget> {
    let Some(info) = info else {
        return vec![GitWatchTarget {
            path: canonical(path),
            recursive: true,
            kind: GitWatchKind::Worktree,
        }];
    };
    let repo_root = canonical(&info.repo_root);
    let git_dir = canonical(&info.git_dir);
    let common_dir = canonical(&info.common_dir);
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
    pub(super) fn subscribe(
        &mut self,
        entity: Entity,
        path: &Path,
    ) -> Result<PathBuf, super::runner::GitError> {
        let path = canonical(path);
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

    fn subscribe_repo_info(
        &mut self,
        path: &Path,
        info: Option<&super::worktree::RepoInfo>,
    ) -> bool {
        let path = canonical(path);
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
}

fn drain_git_watch(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: ResMut<RepoInfoCache>,
    file_pages: Query<(), With<FileUiStateUpdates>>,
    mut views: Query<&mut super::view::GitView>,
    mut commands: Commands,
) {
    let Some(watch) = watch else {
        return;
    };
    let repo_info = repo_info.bypass_change_detection();
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
        if let Ok(mut view) = views.get_mut(entity) {
            if let Some(path) = view.mark_changed() {
                super::job_runner::GitJob::enqueue(
                    &mut commands,
                    entity,
                    super::job::JobKind::Repository { path: path.into() },
                );
            }
        } else {
            FileUiStateUpdates::deliver(&file_pages, &mut commands, entity, &GitChangedEvent {});
        }
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
    let changed = repo_info.bypass_change_detection().poll();
    if changed {
        repo_info.set_changed();
    }
}

fn sync_repo_info_watches(
    watch: Option<NonSendMut<GitWatch>>,
    mut repo_info: ResMut<RepoInfoCache>,
) {
    let repo_info = repo_info.bypass_change_detection();
    let Some(mut watch) = watch else {
        for path in repo_info.inactive_paths() {
            repo_info.remove(&path);
        }
        return;
    };
    watch.evict_inactive_repo_info(repo_info);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::runner::test_repo;

    #[test]
    fn git_watch_targets_cover_index_and_refs() {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        let (_, targets) = git_watch_targets(&file).unwrap();
        let root = canonical(repo.path());
        let git_dir = canonical(&repo.path().join(".git"));

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
        let worktree_root = canonical(&worktree);
        let common = canonical(&repo.path().join(".git"));

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
        let first_info = super::super::worktree::repo_info(first.path()).unwrap();
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
        let active_path = canonical(active_repo.path());
        let stale_path = canonical(stale_repo.path());
        let active_info = super::super::worktree::repo_info(&active_path).unwrap();
        let stale_info = super::super::worktree::repo_info(&stale_path).unwrap();
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
        let root = canonical(Path::new("/tmp/vmux-git-watch"));
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
        let info = super::super::worktree::repo_info(repo.path()).unwrap();
        let targets = repo_info_watch_targets(repo.path(), Some(&info));

        let file = canonical(&file);
        let head = canonical(&info.git_dir.join("HEAD"));
        let lock = canonical(&info.git_dir.join("index.lock"));

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
        let path = canonical(repo.path());
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
                std::thread::sleep(Duration::from_millis(10));
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
        let path = canonical(repo.path());
        let stale = super::super::worktree::repo_info(&path);
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
}
