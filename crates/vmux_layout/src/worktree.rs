//! Per-tab worktree helpers: create a git worktree bound to a [`Tab`] (set `Tab.startup_dir` +
//! attach [`TabWorktree`]) and reconcile away a worktree whose checkout has vanished. Creation is
//! synchronous — the agent-facing `create_worktree` MCP command needs the path back in one call.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::tab::{Tab, TabWorktree};
use vmux_git::worktree::{self, WorktreeInfo};

pub struct WorktreePlugin;

impl Plugin for WorktreePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, reconcile_tab_worktrees);
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
