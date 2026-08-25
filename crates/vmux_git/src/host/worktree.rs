use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

use crate::host::runner::{GitError, git, git_err, git_read};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub base_ref: String,
    pub repo_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorktreeStatus {
    pub uncommitted: u32,
    pub ahead: u32,
}

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

pub fn repo_root_of(dir: &Path) -> Result<PathBuf, GitError> {
    checkout_info(dir).map(|info| info.root)
}

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

pub fn common_dir_of(dir: &Path) -> Result<PathBuf, GitError> {
    checkout_info(dir).map(|info| info.common_dir)
}

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoInfo {
    pub name: String,
    pub branch: String,
    pub is_worktree: bool,
    pub uncommitted: u32,
    pub ahead: u32,
    pub repo_root: PathBuf,
    pub(crate) git_dir: PathBuf,
    pub(crate) common_dir: PathBuf,
}

impl RepoInfo {
    fn name_from_remote(url: &str) -> Option<String> {
        let name = url
            .trim()
            .trim_end_matches('/')
            .rsplit(['/', ':'])
            .next()?
            .trim_end_matches(".git");
        (!name.is_empty()).then(|| name.to_string())
    }
}
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
    let mut name = None;
    if let Ok((remote, _, ok)) = git_read(dir, &["remote", "get-url", "origin"])
        && ok
    {
        name = RepoInfo::name_from_remote(&remote);
    }
    let name = name.unwrap_or_else(|| {
        repo_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    Some(RepoInfo {
        name,
        branch,
        is_worktree: git_dir != common_dir,
        uncommitted,
        ahead,
        repo_root,
        git_dir,
        common_dir,
    })
}

pub fn is_linked_worktree(dir: &Path) -> bool {
    let Ok(git_dir) = rev_parse_path(dir, "--git-dir", "git directory") else {
        return false;
    };
    let Ok(common_dir) = rev_parse_path(dir, "--git-common-dir", "git common directory") else {
        return false;
    };
    git_dir != common_dir
}

pub struct ValidatedLinkedWorkspace {
    pub cwd: PathBuf,
    pub workspace_cwd: PathBuf,
    pub checkout: CheckoutInfo,
}

pub fn validate_linked_workspace(
    cwd: &Path,
    workspace_cwd: &Path,
    branch: &str,
) -> Result<ValidatedLinkedWorkspace, String> {
    let cwd = cwd
        .canonicalize()
        .map_err(|error| format!("invalid worktree directory: {error}"))?;
    let workspace_cwd = workspace_cwd
        .canonicalize()
        .map_err(|error| format!("invalid project directory: {error}"))?;
    let checkout = checkout_info(&cwd).map_err(|error| error.0)?;
    let workspace = checkout_info(&workspace_cwd).map_err(|error| error.0)?;
    if checkout.common_dir != workspace.common_dir {
        return Err("worktree belongs to a different repository".to_string());
    }
    if !is_linked_worktree(&cwd) {
        return Err("worktree directory is not a linked worktree".to_string());
    }
    let actual_branch = head_ref(&checkout.root).map_err(|error| error.0)?;
    if actual_branch != branch {
        return Err(format!(
            "worktree is on branch {actual_branch}, expected {branch}"
        ));
    }
    Ok(ValidatedLinkedWorkspace {
        cwd,
        workspace_cwd,
        checkout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::runner::test_repo;

    fn commit_initial(repo: &Path) {
        test_repo::write(repo, "seed.txt", "seed\n");
        test_repo::run(repo, &["add", "seed.txt"]);
        test_repo::run(repo, &["commit", "-qm", "init"]);
    }

    #[test]
    fn a_remote_url_names_its_repository_however_it_is_spelled() {
        for url in [
            "https://github.com/vmux-ai/vmux.git",
            "https://github.com/vmux-ai/vmux",
            "git@github.com:vmux-ai/vmux.git",
            "ssh://git@github.com/vmux-ai/vmux.git",
            "/Users/me/checkouts/vmux",
            "/Users/me/checkouts/vmux/",
        ] {
            assert_eq!(
                RepoInfo::name_from_remote(url).as_deref(),
                Some("vmux"),
                "`{url}` should name `vmux`"
            );
        }
    }

    #[test]
    fn a_worktree_is_named_for_its_repository_not_its_directory() {
        let origin = tempfile::tempdir().unwrap();
        test_repo::run(origin.path(), &["init", "-q", "--bare"]);
        let repo = test_repo::init();
        commit_initial(repo.path());
        test_repo::run(
            repo.path(),
            &["remote", "add", "origin", origin.path().to_str().unwrap()],
        );

        let checkout = repo.path().join("wt");
        worktree_add(repo.path(), &checkout, "feature", "HEAD").unwrap();
        let info = repo_info(&checkout).unwrap();

        assert_eq!(info.branch, "feature");
        assert_eq!(
            info.name,
            origin.path().file_name().unwrap().to_string_lossy(),
            "the name must follow `origin`, not the `wt` directory the worktree sits in"
        );
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
    fn resolves_repository_marked_bare_with_dot_git_directory() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        test_repo::run(repo.path(), &["config", "core.bare", "true"]);

        let info = checkout_info(repo.path()).unwrap();

        assert_eq!(info.root, repo.path().canonicalize().unwrap());
        assert_eq!(
            info.common_dir,
            repo.path().join(".git").canonicalize().unwrap()
        );
    }

    #[test]
    fn bare_repository_named_dot_git_remains_its_own_root() {
        let path = Path::new("/tmp/example/.git");
        assert_eq!(bare_checkout_root(path, path), path);
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
    fn repository_init_makes_the_selected_directory_a_checkout() {
        let workspace = tempfile::tempdir().unwrap();

        let root = repository_init(workspace.path()).unwrap();

        assert_eq!(root, workspace.path().canonicalize().unwrap());
        assert!(workspace.path().join(".git").is_dir());
    }

    #[test]
    fn initial_snapshot_commits_existing_files_once() {
        let repository = tempfile::tempdir().unwrap();
        repository_init(repository.path()).unwrap();
        std::fs::write(repository.path().join("note.md"), "# Note\n").unwrap();

        ensure_initial_snapshot(repository.path(), "Initialize").unwrap();
        ensure_initial_snapshot(repository.path(), "Ignored").unwrap();

        let (count, _, ok) = git(repository.path(), &["rev-list", "--count", "HEAD"]).unwrap();
        assert!(ok);
        assert_eq!(count.trim(), "1");
        let (tracked, _, ok) = git(repository.path(), &["ls-files", "note.md"]).unwrap();
        assert!(ok);
        assert_eq!(tracked.trim(), "note.md");
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
    fn worktree_mutation_lock_is_shared_across_checkouts() {
        let repo = test_repo::init();
        commit_initial(repo.path());
        let wt = repo.path().join(".worktrees/feat");
        worktree_add(repo.path(), &wt, "vmux/feat", "main").unwrap();
        let lock = lock_repository_worktrees(repo.path()).unwrap();
        let competing = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(common_dir_of(&wt).unwrap().join("vmux-worktrees.lock"))
            .unwrap();

        assert_ne!(
            unsafe { libc::flock(competing.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        drop(lock);
        assert_eq!(
            unsafe { libc::flock(competing.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
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
