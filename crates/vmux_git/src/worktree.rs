//! Git worktree operations for per-tab isolation: create/remove/list a worktree and report
//! its dirty/ahead status. Root/path-based (unlike [`crate::runner`], which is file-centric),
//! because a worktree is created at a path that does not exist yet.

use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

use crate::runner::{GitError, git, git_err, git_read};

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

struct RepositoryWorktreeLock(File);

impl Drop for RepositoryWorktreeLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

fn lock_repository_worktrees(root: &Path) -> Result<RepositoryWorktreeLock, GitError> {
    let path = common_dir_of(root)?.join("vmux-worktrees.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| GitError(format!("failed to open worktree lock: {error}")))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(GitError(format!(
            "failed to acquire worktree lock: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(RepositoryWorktreeLock(file))
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

fn is_bare_repository(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-bare-repository"])
        .ok()
        .is_some_and(|(stdout, _, ok)| ok && stdout.trim() == "true")
}

fn bare_checkout_root(input_dir: &Path, common_dir: &Path) -> PathBuf {
    common_dir
        .file_name()
        .filter(|name| *name == ".git")
        .and_then(|_| common_dir.parent())
        .filter(|root| input_dir != common_dir && input_dir.starts_with(root))
        .unwrap_or(common_dir)
        .to_path_buf()
}

/// Resolve checkout root and shared Git directory.
pub fn checkout_info(dir: &Path) -> Result<CheckoutInfo, GitError> {
    let input_dir = dir
        .canonicalize()
        .map_err(|error| GitError(format!("invalid checkout directory: {error}")))?;
    if !input_dir.is_dir() {
        return Err(GitError("checkout path is not a directory".to_string()));
    }
    let common_dir = rev_parse_path(&input_dir, "--git-common-dir", "git common dir")?;
    let common_dir = common_dir
        .canonicalize()
        .map_err(|error| GitError(format!("invalid git common directory: {error}")))?;
    let root = match rev_parse_path(&input_dir, "--show-toplevel", "git checkout root") {
        Ok(root) => root,
        Err(_) if is_bare_repository(&input_dir) => bare_checkout_root(&input_dir, &common_dir),
        Err(error) => return Err(error),
    };
    let root = root
        .canonicalize()
        .map_err(|error| GitError(format!("invalid checkout root: {error}")))?;
    if !root.is_dir() || !input_dir.starts_with(&root) {
        return Err(GitError(
            "git checkout root does not contain the input directory".to_string(),
        ));
    }
    Ok(CheckoutInfo { root, common_dir })
}

/// The repo root containing `dir` (`git rev-parse --show-toplevel`). `dir` must exist.
pub fn repo_root_of(dir: &Path) -> Result<PathBuf, GitError> {
    checkout_info(dir).map(|info| info.root)
}

/// Initialize a Git repository in an existing directory.
pub fn repository_init(dir: &Path) -> Result<PathBuf, GitError> {
    let dir = dir
        .canonicalize()
        .map_err(|error| GitError(format!("invalid workspace directory: {error}")))?;
    if !dir.is_dir() {
        return Err(GitError("workspace path is not a directory".to_string()));
    }
    let (stdout, stderr, ok) = git(&dir, &["init", "--quiet"])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    checkout_info(&dir).map(|info| info.root)
}

/// Create the empty root commit required before Git can add a linked worktree.
pub fn ensure_initial_commit(root: &Path) -> Result<(), GitError> {
    let (_, _, has_head) = git(root, &["rev-parse", "--verify", "HEAD"])?;
    if has_head {
        return Ok(());
    }
    let (stdout, stderr, ok) = git(
        root,
        &[
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "--message",
            "Initial commit",
        ],
    )?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(())
}

pub fn ensure_initial_snapshot(root: &Path, message: &str) -> Result<(), GitError> {
    let (_, _, has_head) = git(root, &["rev-parse", "--verify", "HEAD"])?;
    if has_head {
        return Ok(());
    }
    let (stdout, stderr, added) = git(root, &["add", "--all"])?;
    if !added {
        return Err(git_err(&stdout, &stderr));
    }
    let (stdout, stderr, committed) = git(
        root,
        &[
            "-c",
            "user.name=vmux",
            "-c",
            "user.email=knowledge@vmux.ai",
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "--message",
            message,
        ],
    )?;
    if !committed {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(())
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
    let _lock = lock_repository_worktrees(root)?;
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
    let _lock = lock_repository_worktrees(root)?;
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
    let _lock = lock_repository_worktrees(root)?;
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

/// Validate a local branch name using Git's own ref-format rules.
pub fn validate_branch_name(root: &Path, branch: &str) -> Result<(), GitError> {
    if branch.is_empty() || branch.trim() != branch {
        return Err(GitError(
            "branch name is empty or has surrounding whitespace".to_string(),
        ));
    }
    let (stdout, stderr, ok) = git(root, &["check-ref-format", "--branch", branch])?;
    if !ok {
        return Err(git_err(&stdout, &stderr));
    }
    Ok(())
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
    pub(crate) repo_root: PathBuf,
    pub(crate) git_dir: PathBuf,
    pub(crate) common_dir: PathBuf,
}

/// Detect git info for `dir`: `None` if it isn't inside a git repo, else the current branch,
/// whether it's a linked worktree, and uncommitted/ahead counts. Auto-detected from git alone.
pub fn repo_info(dir: &Path) -> Option<RepoInfo> {
    let (status, _, ok) = git_read(dir, &["status", "--porcelain=v2", "--branch"]).ok()?;
    if !ok {
        return None;
    }
    let branch = status
        .lines()
        .find_map(|line| line.strip_prefix("# branch.head "))
        .filter(|branch| *branch != "(detached)")
        .unwrap_or_default()
        .to_string();
    let ahead = status
        .lines()
        .find_map(|line| line.strip_prefix("# branch.ab +"))
        .and_then(|value| value.split_once(' '))
        .and_then(|(ahead, _)| ahead.parse::<u32>().ok())
        .unwrap_or_default();
    let uncommitted = status
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count() as u32;
    let (dirs, _, ok) = git_read(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ],
    )
    .ok()?;
    if !ok {
        return None;
    }
    let mut dirs = dirs.lines().map(PathBuf::from);
    let repo_root = dirs.next()?;
    let git_dir = dirs.next()?;
    let common_dir = dirs.next()?;
    Some(RepoInfo {
        branch,
        is_worktree: git_dir != common_dir,
        uncommitted,
        ahead,
        repo_root,
        git_dir,
        common_dir,
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
#[path = "worktree.test.rs"]
mod tests;
