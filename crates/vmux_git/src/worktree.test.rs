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
