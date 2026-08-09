use super::*;

#[test]
fn git_command_scrubs_all_local_env_vars() {
    use std::ffi::OsStr;
    let cmd = git_command(Path::new("."));
    let removed: HashSet<&OsStr> = cmd
        .get_envs()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k)
        .collect();
    let vars = local_env_vars();
    assert!(!vars.is_empty(), "local_env_vars must not be empty");
    for var in vars {
        assert!(
            removed.contains(OsStr::new(var.as_str())),
            "git_command must scrub {var} so ambient GIT_* cannot redirect the runner"
        );
    }
    for key in [
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_CONFIG",
    ] {
        assert!(
            vars.iter().any(|v| v == key),
            "{key} must appear in git's local-env-vars"
        );
    }
}

#[test]
fn repo_root_resolves_toplevel() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "hi");
    let root = repo_root(&file).unwrap();
    assert_eq!(
        root.canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
}

#[test]
fn repo_root_errors_outside_repo() {
    let dir = tempfile::tempdir().unwrap();
    let file = test_repo::write(dir.path(), "loose.txt", "x");
    assert!(repo_root(&file).is_err());
}

#[test]
fn detects_repository_marker_in_ancestor() {
    let repo = test_repo::init();
    let nested = repo.path().join("notes/projects");
    std::fs::create_dir_all(&nested).unwrap();
    let file = test_repo::write(&nested, "plan.md", "# Plan");
    assert!(has_repository(&file));

    let outside = tempfile::tempdir().unwrap();
    assert!(!has_repository(&outside.path().join("note.md")));
}

#[test]
fn dirty_set_lists_modified_and_untracked_not_clean() {
    let repo = test_repo::init();
    let _clean = test_repo::write(repo.path(), "clean.txt", "x\n");
    let modified = test_repo::write(repo.path(), "mod.txt", "one\n");
    test_repo::run(repo.path(), &["add", "."]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "mod.txt", "two\n");
    test_repo::write(repo.path(), "new.txt", "n\n");

    let (root, set) = dirty_set(&modified).unwrap();
    assert_eq!(
        root.canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
    assert!(set.contains("mod.txt"));
    assert!(set.contains("new.txt"));
    assert!(!set.contains("clean.txt"));
}

#[test]
fn status_reports_modified_then_staged() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "a.txt", "two\n");

    assert_eq!(status(&file).unwrap().file_status, FileStatus::Modified);
    stage(&file).unwrap();
    assert_eq!(status(&file).unwrap().file_status, FileStatus::Staged);
}

#[test]
fn status_reports_nested_untracked_file() {
    let repo = test_repo::init();
    std::fs::create_dir(repo.path().join("nested")).unwrap();
    let file = test_repo::write(repo.path(), "nested/new.txt", "new\n");

    assert_eq!(status(&file).unwrap().file_status, FileStatus::Untracked);
}

#[test]
fn status_batch_reports_each_requested_path() {
    let repo = test_repo::init();
    let modified = test_repo::write(repo.path(), "modified.txt", "one\n");
    let staged = test_repo::write(repo.path(), "staged.txt", "one\n");
    test_repo::run(repo.path(), &["add", "."]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "modified.txt", "two\n");
    test_repo::write(repo.path(), "staged.txt", "two\n");
    test_repo::run(repo.path(), &["add", "staged.txt"]);

    let events = statuses(repo.path(), &[modified, staged]).unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].file_status, FileStatus::Modified);
    assert_eq!(events[1].file_status, FileStatus::Staged);
}

#[test]
fn background_status_does_not_refresh_index() {
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};

    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    let index = repo.path().join(".git/index");
    let before = std::fs::read(&index).unwrap();
    OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(3600)))
        .unwrap();

    assert_eq!(status(&file).unwrap().file_status, FileStatus::Clean);

    assert_eq!(std::fs::read(index).unwrap(), before);
}

#[test]
fn diff_lines_show_added_and_removed() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "a.txt", "two\n");

    let lines = diff_lines(&file).unwrap();
    assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Add)));
    assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Remove)));
}

#[test]
fn diff_lines_with_content_reads_unsaved_buffer() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\ntwo\nthree\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);

    let lines = diff_lines_with_content(&file, "one\nchanged\nthree\n").unwrap();

    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.kind, DiffKind::Remove) && line.old_no == Some(2) })
    );
    assert!(
        lines
            .iter()
            .any(|line| { matches!(line.kind, DiffKind::Add) && line.new_no == Some(2) })
    );
    assert_eq!(std::fs::read_to_string(file).unwrap(), "one\ntwo\nthree\n");
}

#[test]
fn handles_path_with_spaces_and_metachars() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a b; rm.txt", "one\n");
    stage(&file).unwrap();
    assert_eq!(status(&file).unwrap().file_status, FileStatus::Staged);
}

#[test]
fn unstage_returns_to_modified() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "a.txt", "two\n");
    stage(&file).unwrap();
    unstage(&file).unwrap();
    assert_eq!(status(&file).unwrap().file_status, FileStatus::Modified);
}

#[test]
fn discard_reverts_working_tree() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "a.txt", "two\n");
    discard(&file).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "one\n");
}

#[test]
fn commit_clears_staged_and_advances_head() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    stage(&file).unwrap();
    commit(&file, "add a").unwrap();
    assert_eq!(status(&file).unwrap().staged_count, 0);
    let (log, _, ok) = git(repo.path(), &["log", "--oneline"]).unwrap();
    assert!(ok && log.contains("add a"));
}

#[test]
fn commit_with_nothing_staged_errors() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    assert!(commit(&file, "noop").is_err());
}

#[test]
fn push_updates_bare_remote() {
    let remote = tempfile::tempdir().unwrap();
    test_repo::run(remote.path(), &["init", "-q", "--bare"]);
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    stage(&file).unwrap();
    commit(&file, "init").unwrap();
    test_repo::run(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    test_repo::run(repo.path(), &["push", "-u", "origin", "main"]);

    test_repo::write(repo.path(), "a.txt", "two\n");
    stage(&file).unwrap();
    commit(&file, "second").unwrap();
    push(&file).unwrap();

    let (log, _, ok) = git(remote.path(), &["log", "--oneline", "main"]).unwrap();
    assert!(ok && log.contains("second"));
}

#[test]
fn apply_hunk_accept_stages_then_reject_reverts() {
    let repo = test_repo::init();
    let file = test_repo::write(
        repo.path(),
        "a.txt",
        "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n",
    );
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(
        repo.path(),
        "a.txt",
        "L1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nL10\n",
    );

    apply_hunk(&file, 0, true).unwrap();
    assert_eq!(
        status(&file).unwrap().file_status,
        FileStatus::StagedModified
    );

    apply_hunk(&file, 0, false).unwrap();
    let content = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.first().copied(), Some("L1"));
    assert_eq!(lines.last().copied(), Some("l10"));
}

#[test]
fn diff_lines_marks_accepted_hunk_staged_unstaged_remains() {
    let repo = test_repo::init();
    let body = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\n";
    let file = test_repo::write(repo.path(), "a.txt", body);
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(
        repo.path(),
        "a.txt",
        "L1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nL12\n",
    );

    apply_hunk(&file, 0, true).unwrap();
    let lines = diff_lines(&file).unwrap();
    assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Staged)));
    assert!(
        lines
            .iter()
            .any(|l| matches!(l.kind, DiffKind::Add | DiffKind::Remove))
    );
}

#[test]
fn close_changes_are_independent_hunks() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "l1\nl2\nl3\nl4\nl5\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    // change line 1 and line 3 — only 2 lines apart (would merge under -U3)
    test_repo::write(repo.path(), "a.txt", "X1\nl2\nX3\nl4\nl5\n");

    let hunks: std::collections::HashSet<u32> = diff_lines(&file)
        .unwrap()
        .iter()
        .filter_map(|l| l.hunk)
        .collect();
    assert_eq!(hunks.len(), 2, "expected 2 separate hunks, got {hunks:?}");

    // accepting hunk 0 (line 1) must not touch line 3
    apply_hunk(&file, 0, true).unwrap();
    let removes: Vec<_> = diff_lines(&file)
        .unwrap()
        .into_iter()
        .filter(|l| matches!(l.kind, DiffKind::Remove))
        .collect();
    assert_eq!(removes.len(), 1);
    assert_eq!(removes[0].old_no, Some(3));
}

#[test]
fn deny_hunk_restores_line_and_clears_its_highlight() {
    let repo = test_repo::init();
    let filler = "    f();\n".repeat(10);
    let head = format!("fn greet() {{\n    a();\n}}\n{filler}fn main() {{\n    done();\n}}\n");
    let file = test_repo::write(repo.path(), "a.rs", &head);
    test_repo::run(repo.path(), &["add", "a.rs"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    let work = format!("fn greet() {{\n    B();\n}}\n{filler}fn main() {{\n}}\n");
    test_repo::write(repo.path(), "a.rs", &work);

    apply_hunk(&file, 1, false).unwrap();

    assert!(std::fs::read_to_string(&file).unwrap().contains("done();"));
    let after = diff_lines(&file).unwrap();
    let removes: Vec<_> = after
        .iter()
        .filter(|l| matches!(l.kind, DiffKind::Remove))
        .collect();
    assert_eq!(removes.len(), 1);
    assert_eq!(removes[0].old_no, Some(2));
}

#[test]
fn diff_lines_fully_staged_shows_code_without_signs() {
    let repo = test_repo::init();
    let body = "l1\nl2\nl3\nl4\nl5\n";
    let file = test_repo::write(repo.path(), "a.txt", body);
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "a.txt", "L1\nl2\nl3\nl4\nl5\n");
    stage(&file).unwrap();

    let lines = diff_lines(&file).unwrap();
    assert_eq!(lines.len(), 5);
    assert!(lines.iter().any(|l| matches!(l.kind, DiffKind::Staged)));
    assert!(
        !lines
            .iter()
            .any(|l| matches!(l.kind, DiffKind::Add | DiffKind::Remove))
    );
}
