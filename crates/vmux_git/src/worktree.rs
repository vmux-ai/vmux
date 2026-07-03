//! Git worktree operations for per-tab isolation: create/remove/list a worktree and report
//! its dirty/ahead status. Root/path-based (unlike [`crate::runner`], which is file-centric),
//! because a worktree is created at a path that does not exist yet.

use std::path::{Path, PathBuf};

use crate::runner::{GitError, git, git_err};

/// A vmux-managed worktree: its checkout path, branch, base ref, and owning repo root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub base_ref: String,
    pub repo_root: PathBuf,
}

/// Uncommitted (working-tree) and unpushed (ahead-of-upstream) commit counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorktreeStatus {
    pub uncommitted: u32,
    pub ahead: u32,
}

/// The repo root containing `dir` (`git rev-parse --show-toplevel`). `dir` must exist.
pub fn repo_root_of(dir: &Path) -> Result<PathBuf, GitError> {
    let (stdout, stderr, ok) = git(dir, &["rev-parse", "--show-toplevel"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(PathBuf::from(stdout.trim()))
}

/// The current branch name at `root`, falling back to a short SHA when HEAD is detached.
pub fn head_ref(root: &Path) -> Result<String, GitError> {
    if let Ok((stdout, _, true)) = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        let name = stdout.trim();
        if !name.is_empty() {
            return Ok(name.to_string());
        }
    }
    let (stdout, stderr, ok) = git(root, &["rev-parse", "--short", "HEAD"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(stdout.trim().to_string())
}

/// Create a worktree at `path` on a new `branch` based on `base` (`git worktree add`).
pub fn worktree_add(
    root: &Path,
    path: &Path,
    branch: &str,
    base: &str,
) -> Result<WorktreeInfo, GitError> {
    let path_str = path.to_string_lossy();
    let (stdout, stderr, ok) = git(
        root,
        &["worktree", "add", path_str.as_ref(), "-b", branch, base],
    )?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(WorktreeInfo {
        path: path.to_path_buf(),
        branch: branch.to_string(),
        base_ref: base.to_string(),
        repo_root: root.to_path_buf(),
    })
}

/// Remove the worktree at `path` and delete its `branch` (best-effort branch cleanup).
pub fn worktree_remove(
    root: &Path,
    path: &Path,
    branch: &str,
    force: bool,
) -> Result<(), GitError> {
    let path_str = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path_str.as_ref());
    let (stdout, stderr, ok) = git(root, &args)?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    let _ = git(root, &["branch", "-D", branch]);
    Ok(())
}

/// Working-tree dirtiness and unpushed-commit count for the worktree at `path`.
pub fn worktree_status(path: &Path) -> Result<WorktreeStatus, GitError> {
    let (stdout, stderr, ok) = git(path, &["status", "--porcelain"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    let uncommitted = stdout.lines().filter(|l| !l.trim().is_empty()).count() as u32;
    let ahead = git(path, &["rev-list", "--count", "@{upstream}..HEAD"])
        .ok()
        .filter(|(_, _, ok)| *ok)
        .and_then(|(out, _, _)| out.trim().parse::<u32>().ok())
        .unwrap_or(0);
    Ok(WorktreeStatus { uncommitted, ahead })
}

/// Registered worktree checkout paths for the repo at `root` (`git worktree list`).
pub fn worktree_list(root: &Path) -> Result<Vec<PathBuf>, GitError> {
    let (stdout, stderr, ok) = git(root, &["worktree", "list", "--porcelain"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(stdout
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect())
}

/// Live git status of a directory, for the side-sheet git-integration card.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoInfo {
    pub branch: String,
    pub is_worktree: bool,
    pub uncommitted: u32,
    pub ahead: u32,
}

/// Detect git info for `dir`: `None` if it isn't inside a git repo, else the current branch,
/// whether it's a linked worktree, and uncommitted/ahead counts. Auto-detected from git alone.
pub fn repo_info(dir: &Path) -> Option<RepoInfo> {
    repo_root_of(dir).ok()?;
    let status = worktree_status(dir).unwrap_or_default();
    Some(RepoInfo {
        branch: head_ref(dir).unwrap_or_default(),
        is_worktree: is_linked_worktree(dir),
        uncommitted: status.uncommitted,
        ahead: status.ahead,
    })
}

/// True if `dir` is a *linked* worktree (its git-dir differs from the repo's common git-dir),
/// i.e. not the repo's main working tree. False for the main worktree or a non-repo.
pub fn is_linked_worktree(dir: &Path) -> bool {
    let Ok((stdout, _, ok)) = git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
    ) else {
        return false;
    };
    if !ok {
        return false;
    }
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    match (lines.next(), lines.next()) {
        (Some(git_dir), Some(common)) => git_dir != common,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::test_repo;

    fn commit_initial(repo: &Path) {
        test_repo::write(repo, "seed.txt", "seed\n");
        test_repo::run(repo, &["add", "seed.txt"]);
        test_repo::run(repo, &["commit", "-qm", "init"]);
    }

    #[test]
    fn add_creates_worktree_on_new_branch_and_lists_it() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");

        let info = worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        assert_eq!(info.branch, "vmux/feat");
        assert!(wt.is_dir(), "worktree checkout created");

        let listed = worktree_list(repo.path()).unwrap();
        assert!(
            listed
                .iter()
                .any(|p| p.canonicalize().ok() == wt.canonicalize().ok()),
            "worktree appears in list: {listed:?}"
        );
    }

    #[test]
    fn status_reports_uncommitted_then_clean() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();

        assert_eq!(worktree_status(&wt).unwrap().uncommitted, 0);
        test_repo::write(&wt, "dirty.txt", "x\n");
        assert_eq!(worktree_status(&wt).unwrap().uncommitted, 1);
    }

    #[test]
    fn remove_deletes_worktree_and_branch() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();

        worktree_remove(repo.path(), &wt, "vmux/feat", false).unwrap();
        assert!(!wt.exists(), "worktree checkout removed");
        let listed = worktree_list(repo.path()).unwrap();
        assert!(
            !listed
                .iter()
                .any(|p| p.canonicalize().ok() == wt.canonicalize().ok())
        );
        let (_, _, branch_exists) =
            git(repo.path(), &["rev-parse", "--verify", "-q", "vmux/feat"]).unwrap();
        assert!(!branch_exists, "branch deleted");
    }

    #[test]
    fn head_ref_and_repo_root_of() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        assert_eq!(head_ref(repo.path()).unwrap(), "main");
        assert_eq!(
            repo_root_of(repo.path()).unwrap().canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn detects_linked_worktree() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        assert!(!is_linked_worktree(repo.path()), "main worktree");
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        assert!(is_linked_worktree(&wt), "linked worktree");
    }

    #[test]
    fn repo_info_reports_branch_and_dirtiness() {
        let not_repo = tempfile::tempdir().unwrap();
        assert!(repo_info(not_repo.path()).is_none(), "non-repo dir");
        let repo = test_repo::init();
        commit_initial(repo.path());
        let info = repo_info(repo.path()).expect("is a repo");
        assert_eq!(info.branch, "main");
        assert!(!info.is_worktree);
        assert_eq!(info.uncommitted, 0);
        test_repo::write(repo.path(), "dirty.txt", "x\n");
        assert_eq!(repo_info(repo.path()).unwrap().uncommitted, 1);

        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        let wt_info = repo_info(&wt).expect("worktree is a repo");
        assert!(wt_info.is_worktree);
        assert_eq!(wt_info.branch, "vmux/feat");
    }
}
