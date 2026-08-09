use super::*;
use crate::runner::test_repo;

fn dirty_repo() -> (tempfile::TempDir, PathBuf) {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);
    test_repo::write(repo.path(), "a.txt", "two\n");
    (repo, file)
}

#[test]
fn status_job_emits_status() {
    let (_repo, file) = dirty_repo();
    let emits = run_job(JobKind::Status {
        path: file,
        dirty: false,
    });
    assert!(matches!(emits.as_slice(), [Emit::Status(_)]));
}

#[test]
fn diff_job_emits_meta_then_viewport() {
    let (_repo, file) = dirty_repo();
    let emits = run_job(JobKind::Diff {
        path: file,
        top_line: 0,
        rows: 50,
        content: None,
    });
    assert!(matches!(emits[0], Emit::DiffMeta(_)));
    assert!(matches!(emits[1], Emit::DiffViewport(_)));
}

#[test]
fn stage_job_emits_result_then_fresh_status() {
    let (_repo, file) = dirty_repo();
    let emits = run_job(JobKind::Stage { path: file });
    match emits.as_slice() {
        [Emit::Result(r), Emit::Status(s)] => {
            assert!(r.ok);
            assert_eq!(s.file_status, FileStatus::Staged);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn job_on_non_repo_emits_empty_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = test_repo::write(dir.path(), "loose.txt", "x");
    let emits = run_job(JobKind::Status {
        path: file,
        dirty: false,
    });
    assert!(matches!(
        emits.as_slice(),
        [Emit::Status(GitStatusEvent {
            branch,
            file_status: FileStatus::Clean,
            ..
        })] if branch.is_empty()
    ));
}

#[test]
fn diff_on_non_repo_emits_empty_viewport() {
    let dir = tempfile::tempdir().unwrap();
    let file = test_repo::write(dir.path(), "loose.txt", "x");
    let emits = run_job(JobKind::Diff {
        path: file,
        top_line: 0,
        rows: 50,
        content: None,
    });
    assert!(matches!(
        emits.as_slice(),
        [
            Emit::DiffMeta(GitDiffMetaEvent { total_lines: 0 }),
            Emit::DiffViewport(GitDiffViewportEvent {
                total_lines: 0,
                lines,
                ..
            })
        ] if lines.is_empty()
    ));
}

#[test]
fn dirty_buffer_changes_clean_status_to_modified() {
    let repo = test_repo::init();
    let file = test_repo::write(repo.path(), "a.txt", "one\n");
    test_repo::run(repo.path(), &["add", "a.txt"]);
    test_repo::run(repo.path(), &["commit", "-qm", "init"]);

    let emits = run_job(JobKind::Status {
        path: file,
        dirty: true,
    });

    assert!(matches!(
        emits.as_slice(),
        [Emit::Status(GitStatusEvent {
            file_status: FileStatus::Modified,
            ..
        })]
    ));
}
