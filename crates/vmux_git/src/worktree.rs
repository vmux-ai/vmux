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

/// Canonical checkout root and shared Git directory for a repository checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutInfo {
    pub root: PathBuf,
    pub common_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeRegistration {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub prunable: bool,
}

fn normalize_worktree_path(path: &Path) -> Result<PathBuf, GitError> {
    if path.is_dir() {
        return path
            .canonicalize()
            .map_err(|error| GitError(format!("invalid worktree path: {error}")));
    }
    let parent = path
        .parent()
        .ok_or_else(|| GitError("worktree path has no parent".to_string()))?
        .canonicalize()
        .map_err(|error| GitError(format!("invalid worktree parent: {error}")))?;
    let name = path
        .file_name()
        .ok_or_else(|| GitError("worktree path has no file name".to_string()))?;
    Ok(parent.join(name))
}

fn rev_parse_path(dir: &Path, flag: &str, label: &str) -> Result<PathBuf, GitError> {
    let (stdout, stderr, ok) = git(dir, &["rev-parse", "--path-format=absolute", flag])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    let value = stdout
        .strip_suffix("\r\n")
        .or_else(|| stdout.strip_suffix('\n'))
        .unwrap_or(&stdout);
    if value.is_empty() {
        return Err(GitError(format!("{label} is empty")));
    }
    Ok(PathBuf::from(value))
}

/// Resolve checkout root and shared Git directory.
pub fn checkout_info(dir: &Path) -> Result<CheckoutInfo, GitError> {
    let input_dir = dir
        .canonicalize()
        .map_err(|error| GitError(format!("invalid checkout directory: {error}")))?;
    if !input_dir.is_dir() {
        return Err(GitError("checkout path is not a directory".to_string()));
    }
    let root = rev_parse_path(&input_dir, "--show-toplevel", "git checkout root")?;
    let common_dir = rev_parse_path(&input_dir, "--git-common-dir", "git common dir")?;
    let root = root
        .canonicalize()
        .map_err(|error| GitError(format!("invalid checkout root: {error}")))?;
    if !root.is_dir() || !input_dir.starts_with(&root) {
        return Err(GitError(
            "git checkout root does not contain the input directory".to_string(),
        ));
    }
    let common_dir = common_dir
        .canonicalize()
        .map_err(|error| GitError(format!("invalid git common directory: {error}")))?;
    Ok(CheckoutInfo { root, common_dir })
}

/// The repo root containing `dir` (`git rev-parse --show-toplevel`). `dir` must exist.
pub fn repo_root_of(dir: &Path) -> Result<PathBuf, GitError> {
    checkout_info(dir).map(|info| info.root)
}

/// The absolute common Git directory shared by a repository's main and linked worktrees.
pub fn common_dir_of(dir: &Path) -> Result<PathBuf, GitError> {
    checkout_info(dir).map(|info| info.common_dir)
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

/// Recreate a worktree at `path` from an existing local `branch`.
pub fn worktree_add_existing(
    root: &Path,
    path: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<WorktreeInfo, GitError> {
    if path.symlink_metadata().is_ok() {
        return Err(GitError(format!(
            "worktree recovery path already exists: {}",
            path.display()
        )));
    }
    let normalized_path = normalize_worktree_path(path)?;
    let registrations = worktree_registrations(root)?;
    let target_registration = registrations
        .iter()
        .find(|registration| registration.path == normalized_path);
    if let Some(registration) = target_registration
        && registration.branch.as_deref() != Some(branch)
    {
        return Err(GitError(format!(
            "worktree path is registered to a different branch: {}",
            normalized_path.display()
        )));
    }
    if let Some(registration) = registrations.iter().find(|registration| {
        registration.branch.as_deref() == Some(branch) && registration.path != normalized_path
    }) {
        return Err(GitError(format!(
            "branch {branch} is registered to another worktree: {}",
            registration.path.display()
        )));
    }
    if target_registration.is_some() {
        let path_str = normalized_path.to_string_lossy();
        let (stdout, stderr, ok) =
            git(root, &["worktree", "remove", "--force", path_str.as_ref()])?;
        if !ok {
            return Err(git_err(&stdout, &stderr));
        }
        if let Some(registration) = worktree_registrations(root)?.iter().find(|registration| {
            registration.branch.as_deref() == Some(branch) && registration.path != normalized_path
        }) {
            return Err(GitError(format!(
                "branch {branch} is registered to another worktree: {}",
                registration.path.display()
            )));
        }
    }
    let path_str = normalized_path.to_string_lossy();
    let (stdout, stderr, ok) = git(root, &["worktree", "add", path_str.as_ref(), branch])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(WorktreeInfo {
        path: normalized_path,
        branch: branch.to_string(),
        base_ref: base_ref.to_string(),
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
    Ok(worktree_registrations(root)?
        .into_iter()
        .map(|registration| registration.path)
        .collect())
}

pub fn worktree_registrations(root: &Path) -> Result<Vec<WorktreeRegistration>, GitError> {
    let (stdout, stderr, ok) = git(root, &["worktree", "list", "--porcelain"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    let mut registrations = Vec::new();
    let mut path = None;
    let mut branch = None;
    let mut prunable = false;
    for line in stdout.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(path) = path.take() {
                registrations.push(WorktreeRegistration {
                    path,
                    branch: branch.take(),
                    prunable,
                });
            }
            prunable = false;
        } else if let Some(value) = line.strip_prefix("worktree ") {
            let value = PathBuf::from(value);
            path = Some(normalize_worktree_path(&value).unwrap_or(value));
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        } else if line == "prunable" || line.starts_with("prunable ") {
            prunable = true;
        }
    }
    Ok(registrations)
}

/// Local branch names (`git branch --format=%(refname:short)`).
pub fn local_branches(root: &Path) -> Result<Vec<String>, GitError> {
    let (stdout, stderr, ok) = git(root, &["branch", "--format=%(refname:short)"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Absolute path to the repo's `info/exclude` (the local, untracked ignore list). Resolved via
/// git so it works for both the main worktree and a linked worktree, where `.git` is a file
/// pointer rather than a directory and the exclude lives in the shared common dir.
pub fn info_exclude_path(dir: &Path) -> Option<PathBuf> {
    let (stdout, _, ok) = git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ],
    )
    .ok()?;
    if !ok {
        return None;
    }
    let p = stdout.trim();
    (!p.is_empty()).then(|| PathBuf::from(p))
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
    let Ok(git_dir) = rev_parse_path(dir, "--git-dir", "git directory") else {
        return false;
    };
    let Ok(common_dir) = rev_parse_path(dir, "--git-common-dir", "git common directory") else {
        return false;
    };
    git_dir != common_dir
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
    fn add_existing_recovers_only_the_same_stale_registration() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        std::fs::remove_dir_all(&wt).unwrap();

        let recovered = worktree_add_existing(repo.path(), &wt, "vmux/feat", "main").unwrap();

        assert!(recovered.path.is_dir());
        assert_eq!(head_ref(&recovered.path).unwrap(), "vmux/feat");
    }

    #[test]
    fn add_existing_rejects_branch_registered_elsewhere() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let first = repo.path().join(".worktrees/first");
        let second = repo.path().join(".worktrees/second");
        worktree_add(repo.path(), &first, "vmux/feat", "main").unwrap();
        std::fs::create_dir_all(second.parent().unwrap()).unwrap();

        let error = worktree_add_existing(repo.path(), &second, "vmux/feat", "main").unwrap_err();

        assert!(error.0.contains("registered to another worktree"));
        assert!(!second.exists());
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

    #[test]
    fn local_branches_lists_main_and_worktree_branches() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        assert!(
            local_branches(repo.path())
                .unwrap()
                .iter()
                .any(|b| b == "main")
        );
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        assert!(
            local_branches(repo.path())
                .unwrap()
                .iter()
                .any(|b| b == "vmux/feat"),
            "worktree branch is listed"
        );
    }

    #[test]
    fn info_exclude_path_shared_across_main_and_linked_worktree() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let main_excl = info_exclude_path(repo.path()).expect("main exclude");
        assert!(main_excl.ends_with("info/exclude"), "{main_excl:?}");
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        let wt_excl = info_exclude_path(&wt).expect("worktree exclude");
        assert_eq!(
            wt_excl, main_excl,
            "exclude resolves to the shared common dir"
        );
    }

    #[test]
    fn common_dir_identifies_repository_across_worktrees() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();

        let other = test_repo::init();
        commit_initial(other.path());
        let not_repo = tempfile::tempdir().unwrap();

        let main_common = common_dir_of(repo.path()).unwrap();
        assert_eq!(common_dir_of(&wt).unwrap(), main_common);
        assert_ne!(common_dir_of(other.path()).unwrap(), main_common);
        assert!(common_dir_of(not_repo.path()).is_err());
    }

    #[test]
    fn checkout_info_reports_root_and_shared_common_dir() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();

        let main = checkout_info(repo.path()).unwrap();
        let linked = checkout_info(&wt).unwrap();

        assert_eq!(main.root, repo.path().canonicalize().unwrap());
        assert_eq!(linked.root, wt.canonicalize().unwrap());
        assert_eq!(linked.common_dir, main.common_dir);
    }

    #[test]
    fn checkout_info_handles_newline_in_checkout_path() {
        let repo = tempfile::Builder::new()
            .prefix("vmux\ncheckout-")
            .tempdir()
            .unwrap();
        test_repo::run(repo.path(), &["init", "-q", "-b", "main"]);
        test_repo::run(repo.path(), &["config", "user.email", "t@example.com"]);
        test_repo::run(repo.path(), &["config", "user.name", "Test"]);
        test_repo::run(repo.path(), &["config", "commit.gpgsign", "false"]);

        let info = checkout_info(repo.path()).unwrap();

        assert_eq!(info.root, repo.path().canonicalize().unwrap());
    }

    #[test]
    fn checkout_info_rejects_root_outside_input_directory() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let outside = tempfile::tempdir().unwrap();
        let outside_path = outside.path().to_string_lossy();
        let (_, stderr, ok) = git(
            repo.path(),
            &["config", "core.worktree", outside_path.as_ref()],
        )
        .unwrap();
        assert!(ok, "git config failed: {stderr}");

        assert!(checkout_info(repo.path()).is_err());
    }
}
