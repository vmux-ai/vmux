//! Per-tab worktree helpers: create a git worktree bound to a [`Tab`] (set `Tab.startup_dir` +
//! attach [`TabWorktree`]) and reconcile away a worktree whose checkout has vanished. Creation is
//! synchronous — the agent-facing `create_worktree` MCP command needs the path back in one call.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use bevy::prelude::*;

use crate::tab::{Tab, TabWorktree};
use vmux_git::worktree::{self, CheckoutInfo, WorktreeInfo};

pub struct WorktreePlugin;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabDirectoryRebindSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDirectoryObservationKind {
    Read,
    Edit,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct TabDirectoryObserved {
    pub tab: Entity,
    pub path: PathBuf,
    pub kind: TabDirectoryObservationKind,
}

impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TabDirectoryObserved>()
            .add_systems(Update, reconcile_tab_worktrees)
            .add_systems(Update, rebind_tab_directories.in_set(TabDirectoryRebindSet));
    }
}

/// Sanitize a tab name into a filesystem/branch-safe slug (lowercase alnum, `-` separators).
pub fn sanitize_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    }
}

/// Pick a `.worktrees/<slug>` path + `vmux/<slug>` branch that collide with neither an existing
/// worktree path nor an existing local branch (a leftover branch would fail `git worktree add`).
fn plan_worktree(repo_root: &Path, slug_hint: &str) -> (PathBuf, String) {
    let base = sanitize_slug(slug_hint);
    let existing = worktree::worktree_list(repo_root).unwrap_or_default();
    let branches = worktree::local_branches(repo_root).unwrap_or_default();
    let taken = |slug: &str| -> bool {
        let path = repo_root.join(".worktrees").join(slug);
        let branch = format!("vmux/{slug}");
        existing.iter().any(|p| p == &path)
            || path.exists()
            || branches.iter().any(|b| b == &branch)
    };
    let mut slug = base.clone();
    let mut n = 2;
    while taken(&slug) {
        slug = format!("{base}-{n}");
        n += 1;
    }
    let path = repo_root.join(".worktrees").join(&slug);
    let branch = format!("vmux/{slug}");
    (path, branch)
}

/// Create a worktree under `base_dir`'s repo, synchronously, and return its info. Backs the
/// agent-facing `create_worktree` MCP command (which needs the path back in one call).
pub fn create_worktree_blocking(base_dir: &Path, slug_hint: &str) -> Result<WorktreeInfo, String> {
    let repo_root = worktree::repo_root_of(base_dir).map_err(|e| e.0)?;
    let base_ref = worktree::head_ref(&repo_root).map_err(|e| e.0)?;
    let (path, branch) = plan_worktree(&repo_root, slug_hint);
    ensure_worktrees_ignored(&repo_root);
    worktree::worktree_add(&repo_root, &path, &branch, &base_ref).map_err(|e| e.0)
}

/// Add `.worktrees/` to the repo's local `info/exclude` (never the tracked `.gitignore`). The
/// exclude path is resolved via git so it lands in the shared common dir for linked worktrees too.
fn ensure_worktrees_ignored(repo_root: &Path) {
    let Some(exclude) = worktree::info_exclude_path(repo_root) else {
        return;
    };
    let body = std::fs::read_to_string(&exclude).unwrap_or_default();
    if body.lines().any(|l| l.trim() == ".worktrees/") {
        return;
    }
    let mut next = body;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(".worktrees/\n");
    if let Some(parent) = exclude.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&exclude, next);
}

/// After load (or create), drop a [`TabWorktree`] whose checkout directory no longer exists,
/// so the tab's dir cascades back through the resolver instead of pointing at a dead worktree.
fn reconcile_tab_worktrees(q: Query<(Entity, &Tab), Added<TabWorktree>>, mut commands: Commands) {
    for (entity, tab) in &q {
        let missing = tab
            .startup_dir
            .as_deref()
            .map(|d| !Path::new(d).is_dir())
            .unwrap_or(true);
        if missing {
            commands.entity(entity).remove::<TabWorktree>();
        }
    }
}

#[derive(Clone)]
struct CachedCheckoutInfo {
    startup_dir: String,
    info: CheckoutInfo,
    fingerprint: CheckoutFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckoutFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

fn checkout_fingerprint(info: &CheckoutInfo) -> Option<CheckoutFingerprint> {
    let metadata = std::fs::symlink_metadata(info.root.join(".git")).ok()?;
    Some(CheckoutFingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}

fn store_cached_checkout_info(
    cache: &mut HashMap<Entity, CachedCheckoutInfo>,
    tab: Entity,
    startup_dir: String,
    info: &CheckoutInfo,
) {
    let Some(fingerprint) = checkout_fingerprint(info) else {
        cache.remove(&tab);
        return;
    };
    cache.insert(
        tab,
        CachedCheckoutInfo {
            startup_dir,
            info: info.clone(),
            fingerprint,
        },
    );
}

fn cached_checkout_info(
    cache: &mut HashMap<Entity, CachedCheckoutInfo>,
    tab: Entity,
    startup_dir: &str,
    resolve: impl FnOnce(&Path) -> Option<CheckoutInfo>,
) -> Option<CheckoutInfo> {
    if let Some(cached) = cache.get(&tab)
        && cached.startup_dir == startup_dir
        && checkout_fingerprint(&cached.info).as_ref() == Some(&cached.fingerprint)
    {
        return Some(cached.info.clone());
    }
    cache.remove(&tab);
    let info = resolve(Path::new(startup_dir))?;
    store_cached_checkout_info(cache, tab, startup_dir.to_string(), &info);
    Some(info)
}

fn observed_start_dir(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let start = if path.is_dir() { path } else { path.parent()? };
    start.canonicalize().ok()
}

fn is_within_checkout_without_nested_git_boundary(root: &Path, observed_dir: &Path) -> bool {
    observed_dir.starts_with(root)
        && !observed_dir
            .ancestors()
            .take_while(|ancestor| *ancestor != root)
            .any(|ancestor| ancestor.join(".git").exists())
}

fn rebind_tab_directories(
    mut reader: MessageReader<TabDirectoryObserved>,
    mut tabs: Query<&mut Tab>,
    managed: Query<(), With<TabWorktree>>,
    mut removed_tabs: RemovedComponents<Tab>,
    mut checkout_cache: Local<HashMap<Entity, CachedCheckoutInfo>>,
    mut commands: Commands,
) {
    for tab in removed_tabs.read() {
        checkout_cache.remove(&tab);
    }
    for observed in reader.read() {
        let Some(observed_dir) = observed_start_dir(&observed.path) else {
            continue;
        };
        let Ok(mut tab) = tabs.get_mut(observed.tab) else {
            continue;
        };
        let Some(current) = tab.startup_dir.clone() else {
            continue;
        };
        if let Ok(current_dir) = Path::new(&current).canonicalize()
            && is_within_checkout_without_nested_git_boundary(&current_dir, &observed_dir)
        {
            continue;
        }
        let Ok(observed_info) = worktree::checkout_info(&observed_dir) else {
            continue;
        };
        let current_info =
            cached_checkout_info(&mut checkout_cache, observed.tab, &current, |path| {
                worktree::checkout_info(path).ok()
            });
        if current_info.as_ref().is_some_and(|current_info| {
            is_within_checkout_without_nested_git_boundary(&current_info.root, &observed_dir)
        }) {
            continue;
        }
        let should_rebind = match current_info.as_ref() {
            Some(current_info) if current_info.root == observed_info.root => false,
            Some(current_info) if current_info.common_dir == observed_info.common_dir => true,
            Some(_) | None => observed.kind == TabDirectoryObservationKind::Edit,
        };
        if !should_rebind {
            continue;
        }
        let startup_dir = observed_info.root.to_string_lossy().into_owned();
        tab.startup_dir = Some(startup_dir.clone());
        store_cached_checkout_info(
            &mut checkout_cache,
            observed.tab,
            startup_dir,
            &observed_info,
        );
        if managed.contains(observed.tab) {
            commands.entity(observed.tab).remove::<TabWorktree>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::process::Command;

    #[derive(Resource)]
    struct ObservationInput {
        tab: Entity,
        path: PathBuf,
    }

    #[derive(Resource, Default)]
    struct CapturedStartupDir(Option<String>);

    fn emit_observation(
        input: Res<ObservationInput>,
        mut observations: MessageWriter<TabDirectoryObserved>,
    ) {
        observations.write(TabDirectoryObserved {
            tab: input.tab,
            path: input.path.clone(),
            kind: TabDirectoryObservationKind::Read,
        });
    }

    fn capture_startup_dir(
        input: Res<ObservationInput>,
        tabs: Query<&Tab>,
        mut captured: ResMut<CapturedStartupDir>,
    ) {
        captured.0 = tabs.get(input.tab).unwrap().startup_dir.clone();
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]);
        git(p, &["config", "user.email", "t@example.com"]);
        git(p, &["config", "user.name", "Test"]);
        git(p, &["config", "commit.gpgsign", "false"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "seed.txt"]);
        git(p, &["commit", "-qm", "init"]);
        dir
    }

    fn observe(app: &mut App, tab: Entity, path: &Path) {
        observe_with_kind(app, tab, path, TabDirectoryObservationKind::Read);
    }

    fn observe_edit(app: &mut App, tab: Entity, path: &Path) {
        observe_with_kind(app, tab, path, TabDirectoryObservationKind::Edit);
    }

    fn observe_with_kind(
        app: &mut App,
        tab: Entity,
        path: &Path,
        kind: TabDirectoryObservationKind,
    ) {
        app.world_mut()
            .resource_mut::<Messages<TabDirectoryObserved>>()
            .write(TabDirectoryObserved {
                tab,
                path: path.to_path_buf(),
                kind,
            });
        app.update();
    }

    #[test]
    fn sanitize_slug_normalizes() {
        assert_eq!(sanitize_slug("Auth Refactor!"), "auth-refactor");
        assert_eq!(sanitize_slug("  a//b  "), "a-b");
        assert_eq!(sanitize_slug("***"), "task");
        assert_eq!(sanitize_slug(""), "task");
    }

    #[test]
    fn create_worktree_blocking_creates_branch_and_excludes() {
        let repo = init_repo();
        let info = create_worktree_blocking(repo.path(), "Auth Refactor").unwrap();
        assert_eq!(info.branch, "vmux/auth-refactor");
        assert!(info.path.is_dir());
        assert!(
            info.path.ends_with("auth-refactor")
                && info.path.parent().unwrap().ends_with(".worktrees"),
            "path is <root>/.worktrees/auth-refactor: {:?}",
            info.path
        );
        let exclude =
            std::fs::read_to_string(repo.path().join(".git/info/exclude")).unwrap_or_default();
        assert!(exclude.lines().any(|l| l.trim() == ".worktrees/"));
    }

    #[test]
    fn plan_worktree_skips_existing_branch_name() {
        let repo = init_repo();
        git(repo.path(), &["branch", "vmux/feat"]);
        let (path, branch) = plan_worktree(repo.path(), "feat");
        assert_eq!(branch, "vmux/feat-2");
        assert!(path.ends_with(".worktrees/feat-2"), "{path:?}");
    }

    #[test]
    fn reconcile_drops_worktree_when_path_missing() {
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let good = tempfile::tempdir().unwrap();
        let kept = app
            .world_mut()
            .spawn((
                Tab {
                    name: "k".into(),
                    startup_dir: Some(good.path().to_string_lossy().into_owned()),
                },
                TabWorktree {
                    repo_root: "r".into(),
                    branch: "vmux/k".into(),
                    base_ref: "main".into(),
                },
            ))
            .id();
        let dropped = app
            .world_mut()
            .spawn((
                Tab {
                    name: "d".into(),
                    startup_dir: Some("/no/such/vmux-xyz-dir".into()),
                },
                TabWorktree {
                    repo_root: "r".into(),
                    branch: "vmux/d".into(),
                    base_ref: "main".into(),
                },
            ))
            .id();

        app.update();

        assert!(app.world().get::<TabWorktree>(kept).is_some());
        assert!(app.world().get::<TabWorktree>(dropped).is_none());
    }

    #[test]
    fn observation_rebinds_managed_tab_to_same_repo_checkout() {
        let repo = init_repo();
        let managed = create_worktree_blocking(repo.path(), "managed").unwrap();
        let touched = repo.path().join("seed.txt");
        let expected = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn((
                Tab {
                    name: "tab".into(),
                    startup_dir: Some(managed.path.to_string_lossy().into_owned()),
                },
                TabWorktree {
                    repo_root: managed.repo_root.to_string_lossy().into_owned(),
                    branch: managed.branch.clone(),
                    base_ref: managed.base_ref.clone(),
                },
            ))
            .id();

        observe_edit(&mut app, tab, &touched);

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
        assert!(app.world().get::<TabWorktree>(tab).is_none());
        assert!(managed.path.is_dir(), "old checkout is preserved");
    }

    #[test]
    fn observation_rebinds_before_same_frame_consumers() {
        let repo = init_repo();
        let managed = create_worktree_blocking(repo.path(), "managed").unwrap();
        let expected = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin)
            .init_resource::<CapturedStartupDir>();
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(managed.path.to_string_lossy().into_owned()),
            })
            .id();
        app.insert_resource(ObservationInput {
            tab,
            path: repo.path().join("seed.txt"),
        })
        .add_systems(Update, emit_observation.before(TabDirectoryRebindSet))
        .add_systems(Update, capture_startup_dir.after(TabDirectoryRebindSet));

        app.update();

        assert_eq!(
            app.world().resource::<CapturedStartupDir>().0.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_rebinds_repeatedly_within_same_repo() {
        let repo = init_repo();
        let first = create_worktree_blocking(repo.path(), "first").unwrap();
        let second_path = repo.path().join(".worktrees/second");
        worktree::worktree_add(repo.path(), &second_path, "vmux/second", "main").unwrap();
        let second_file = second_path.join("seed.txt");
        let main_file = repo.path().join("seed.txt");
        let second_expected = second_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let main_expected = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(first.path.to_string_lossy().into_owned()),
            })
            .id();

        observe(&mut app, tab, &second_file);
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(second_expected.as_str())
        );

        observe(&mut app, tab, &main_file);
        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(main_expected.as_str())
        );
    }

    #[test]
    fn observation_keeps_same_checkout_directory() {
        let repo = init_repo();
        let original = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &repo.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn observation_rebinds_from_main_checkout_to_nested_linked_worktree() {
        let repo = init_repo();
        let linked_path = repo.path().join(".worktrees/linked");
        worktree::worktree_add(repo.path(), &linked_path, "vmux/linked", "main").unwrap();
        let expected = linked_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(repo.path().to_string_lossy().into_owned()),
            })
            .id();

        observe(&mut app, tab, &linked_path.join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_ignores_unrelated_repo_nested_inside_checkout() {
        let repo = init_repo();
        let nested = repo.path().join("vendor/nested");
        std::fs::create_dir_all(&nested).unwrap();
        git(&nested, &["init", "-q", "-b", "main"]);
        git(&nested, &["config", "user.email", "t@example.com"]);
        git(&nested, &["config", "user.name", "Test"]);
        git(&nested, &["config", "commit.gpgsign", "false"]);
        std::fs::write(nested.join("nested.txt"), "nested\n").unwrap();
        git(&nested, &["add", "nested.txt"]);
        git(&nested, &["commit", "-qm", "init"]);
        let original = repo.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &nested.join("nested.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn cached_checkout_info_resolves_again_after_startup_or_git_identity_changes() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        let next_root = repo.path().join(".worktrees/next");
        std::fs::create_dir_all(next_root.join(".git")).unwrap();
        let startup_dir = repo.path().to_string_lossy().into_owned();
        let next_startup_dir = next_root.to_string_lossy().into_owned();
        let tab = Entity::from_bits(1);
        let calls = Cell::new(0);
        let first = vmux_git::worktree::CheckoutInfo {
            root: repo.path().to_path_buf(),
            common_dir: repo.path().join(".git"),
        };
        let second = vmux_git::worktree::CheckoutInfo {
            root: next_root,
            common_dir: repo.path().join(".git"),
        };
        let mut cache = HashMap::new();

        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(first.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(second.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        std::fs::rename(repo.path().join(".git"), repo.path().join(".git-old")).unwrap();
        std::fs::create_dir(repo.path().join(".git")).unwrap();
        let resolved = cached_checkout_info(&mut cache, tab, &startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(first.clone())
        })
        .unwrap();
        assert_eq!(resolved, first);
        let resolved = cached_checkout_info(&mut cache, tab, &next_startup_dir, |_| {
            calls.set(calls.get() + 1);
            Some(second.clone())
        })
        .unwrap();

        assert_eq!(resolved, second);
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn observation_ignores_unrelated_and_invalid_paths() {
        let repo = init_repo();
        let other = init_repo();
        let non_git = tempfile::tempdir().unwrap();
        let non_git_file = non_git.path().join("file.txt");
        std::fs::write(&non_git_file, "x").unwrap();
        let missing = repo.path().join("missing.txt");
        let original = repo.path().to_string_lossy().into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &other.path().join("seed.txt"));
        observe(&mut app, tab, &non_git_file);
        observe(&mut app, tab, &missing);

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }

    #[test]
    fn observation_rebinds_to_different_repo_on_edit() {
        let current = init_repo();
        let observed = init_repo();
        let expected = observed
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(current.path().to_string_lossy().into_owned()),
            })
            .id();

        observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_rebinds_from_non_git_directory_on_edit() {
        let current = tempfile::tempdir().unwrap();
        let observed = init_repo();
        let expected = observed
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(current.path().to_string_lossy().into_owned()),
            })
            .id();

        observe_edit(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn observation_keeps_non_git_directory_on_read() {
        let current = tempfile::tempdir().unwrap();
        let observed = init_repo();
        let original = current
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app
            .world_mut()
            .spawn(Tab {
                name: "tab".into(),
                startup_dir: Some(original.clone()),
            })
            .id();

        observe(&mut app, tab, &observed.path().join("seed.txt"));

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some(original.as_str())
        );
    }
}
