//! Per-tab worktree orchestration: create/remove a git worktree and bind it to a [`Tab`]
//! (set `Tab.startup_dir` + attach [`TabWorktree`]). Creation runs on a background thread and
//! drains through an outbox, mirroring `vmux_git`'s own thread+outbox pattern.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;

use crate::tab::{Tab, TabDirDecided, TabWorktree};
use vmux_git::worktree::{self, WorktreeInfo};

/// Create an isolated worktree for `tab`, based on `base_dir` (a directory inside a git repo).
#[derive(Message, Clone, Debug)]
pub struct CreateTabWorktreeRequest {
    pub tab: Entity,
    pub slug_hint: String,
    pub base_dir: PathBuf,
}

/// Remove `tab`'s worktree and delete its branch.
#[derive(Message, Clone, Debug)]
pub struct RemoveTabWorktreeRequest {
    pub tab: Entity,
    pub force: bool,
}

/// Emitted after a worktree is created and bound to its tab.
#[derive(Message, Clone, Debug)]
pub struct TabWorktreeReady {
    pub tab: Entity,
    pub info: WorktreeInfo,
}

/// Emitted when worktree creation or removal fails.
#[derive(Message, Clone, Debug)]
pub struct TabWorktreeError {
    pub tab: Entity,
    pub message: String,
}

type CreateOutcome = (Entity, Result<WorktreeInfo, String>);

#[derive(Resource, Default)]
struct WorktreeCreateOutbox(Arc<Mutex<Vec<CreateOutcome>>>);

pub struct WorktreePlugin;

impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CreateTabWorktreeRequest>()
            .add_message::<RemoveTabWorktreeRequest>()
            .add_message::<TabWorktreeReady>()
            .add_message::<TabWorktreeError>()
            .init_resource::<WorktreeCreateOutbox>()
            .add_systems(
                Update,
                (
                    spawn_worktree_create_jobs,
                    apply_worktree_create_outcomes,
                    handle_remove_worktree_requests,
                    reconcile_tab_worktrees,
                ),
            );
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

/// Pick a `.worktrees/<slug>` path + `vmux/<slug>` branch that don't collide with existing ones.
fn plan_worktree(repo_root: &Path, slug_hint: &str) -> (PathBuf, String) {
    let base = sanitize_slug(slug_hint);
    let existing = worktree::worktree_list(repo_root).unwrap_or_default();
    let taken = |slug: &str| -> bool {
        let path = repo_root.join(".worktrees").join(slug);
        existing.iter().any(|p| p == &path) || path.exists()
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

fn create_worktree_blocking(base_dir: &Path, slug_hint: &str) -> Result<WorktreeInfo, String> {
    let repo_root = worktree::repo_root_of(base_dir).map_err(|e| e.0)?;
    let base_ref = worktree::head_ref(&repo_root).map_err(|e| e.0)?;
    let (path, branch) = plan_worktree(&repo_root, slug_hint);
    ensure_worktrees_ignored(&repo_root);
    worktree::worktree_add(&repo_root, &path, &branch, &base_ref).map_err(|e| e.0)
}

/// Add `.worktrees/` to the repo's local `.git/info/exclude` (never the tracked `.gitignore`).
fn ensure_worktrees_ignored(repo_root: &Path) {
    let exclude = repo_root.join(".git").join("info").join("exclude");
    let body = std::fs::read_to_string(&exclude).unwrap_or_default();
    if body.lines().any(|l| l.trim() == ".worktrees/") {
        return;
    }
    let mut next = body;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(".worktrees/\n");
    let _ = std::fs::write(&exclude, next);
}

fn spawn_worktree_create_jobs(
    mut reader: MessageReader<CreateTabWorktreeRequest>,
    outbox: Res<WorktreeCreateOutbox>,
) {
    for req in reader.read() {
        let sink = outbox.0.clone();
        let tab = req.tab;
        let base_dir = req.base_dir.clone();
        let slug_hint = req.slug_hint.clone();
        std::thread::spawn(move || {
            let result = create_worktree_blocking(&base_dir, &slug_hint);
            sink.lock()
                .unwrap_or_else(|p| p.into_inner())
                .push((tab, result));
        });
    }
}

fn apply_worktree_create_outcomes(
    outbox: Res<WorktreeCreateOutbox>,
    mut tabs: Query<&mut Tab>,
    mut ready: MessageWriter<TabWorktreeReady>,
    mut errors: MessageWriter<TabWorktreeError>,
    mut commands: Commands,
) {
    let drained: Vec<CreateOutcome> = {
        let mut q = outbox.0.lock().unwrap_or_else(|p| p.into_inner());
        q.drain(..).collect()
    };
    for (tab, result) in drained {
        match result {
            Ok(info) => {
                if let Ok(mut t) = tabs.get_mut(tab) {
                    t.startup_dir = Some(info.path.to_string_lossy().into_owned());
                }
                commands.entity(tab).insert((
                    TabWorktree {
                        repo_root: info.repo_root.to_string_lossy().into_owned(),
                        branch: info.branch.clone(),
                        base_ref: info.base_ref.clone(),
                    },
                    TabDirDecided,
                ));
                ready.write(TabWorktreeReady { tab, info });
            }
            Err(message) => {
                errors.write(TabWorktreeError { tab, message });
            }
        }
    }
}

fn handle_remove_worktree_requests(
    mut reader: MessageReader<RemoveTabWorktreeRequest>,
    worktrees: Query<&TabWorktree>,
    mut tabs: Query<&mut Tab>,
    mut errors: MessageWriter<TabWorktreeError>,
    mut commands: Commands,
) {
    for req in reader.read() {
        let Ok(wt) = worktrees.get(req.tab) else {
            continue;
        };
        let Some(path) = tabs.get(req.tab).ok().and_then(|t| t.startup_dir.clone()) else {
            continue;
        };
        let repo_root = PathBuf::from(&wt.repo_root);
        match worktree::worktree_remove(&repo_root, Path::new(&path), &wt.branch, req.force) {
            Ok(()) => {
                if let Ok(mut t) = tabs.get_mut(req.tab) {
                    t.startup_dir = None;
                }
                commands
                    .entity(req.tab)
                    .remove::<TabWorktree>()
                    .remove::<TabDirDecided>();
            }
            Err(e) => {
                errors.write(TabWorktreeError {
                    tab: req.tab,
                    message: e.0,
                });
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use std::process::Command;

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
    fn apply_outcome_binds_worktree_to_tab() {
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app.world_mut().spawn(Tab::default()).id();

        let info = WorktreeInfo {
            path: PathBuf::from("/repo/.worktrees/feat"),
            branch: "vmux/feat".into(),
            base_ref: "main".into(),
            repo_root: PathBuf::from("/repo"),
        };
        app.world()
            .resource::<WorktreeCreateOutbox>()
            .0
            .lock()
            .unwrap()
            .push((tab, Ok(info)));

        app.update();

        assert_eq!(
            app.world().get::<Tab>(tab).unwrap().startup_dir.as_deref(),
            Some("/repo/.worktrees/feat")
        );
        let wt = app
            .world()
            .get::<TabWorktree>(tab)
            .expect("TabWorktree set");
        assert_eq!(wt.branch, "vmux/feat");
        assert!(app.world().get::<TabDirDecided>(tab).is_some());
        let ready = app
            .world_mut()
            .resource_mut::<Messages<TabWorktreeReady>>()
            .drain()
            .count();
        assert_eq!(ready, 1);
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
    fn apply_outcome_error_emits_error_no_binding() {
        let mut app = App::new();
        app.add_plugins(WorktreePlugin);
        let tab = app.world_mut().spawn(Tab::default()).id();
        app.world()
            .resource::<WorktreeCreateOutbox>()
            .0
            .lock()
            .unwrap()
            .push((tab, Err("boom".into())));

        app.update();

        assert!(app.world().get::<TabWorktree>(tab).is_none());
        let errs = app
            .world_mut()
            .resource_mut::<Messages<TabWorktreeError>>()
            .drain()
            .count();
        assert_eq!(errs, 1);
    }
}
