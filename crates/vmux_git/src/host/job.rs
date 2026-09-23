use std::path::PathBuf;

use crate::event::*;
use crate::host::{parse, runner};

#[derive(Debug, Clone)]
pub(super) enum JobKind {
    Repository {
        path: PathBuf,
    },
    BranchLog {
        repo_root: PathBuf,
        branch: String,
    },
    Diff {
        repo_root: PathBuf,
        path: PathBuf,
        reference: String,
        generation: u64,
        top_line: u32,
        rows: u32,
        content: Option<String>,
    },
    Stage {
        repo_root: PathBuf,
        path: PathBuf,
    },
    Unstage {
        repo_root: PathBuf,
        path: PathBuf,
    },
    Discard {
        repo_root: PathBuf,
        path: PathBuf,
    },
    Commit {
        path: PathBuf,
        message: String,
    },
    Fetch {
        path: PathBuf,
    },
    Pull {
        path: PathBuf,
    },
    Operation {
        repo_root: PathBuf,
        operation: GitOperation,
    },
    Push {
        path: PathBuf,
    },
    StageAll {
        path: PathBuf,
    },
    Hunk {
        repo_root: PathBuf,
        path: PathBuf,
        hunk: u32,
        accept: bool,
    },
}

#[derive(Debug, Clone)]
pub(super) enum Emit {
    Repository(GitRepositoryEvent),
    BranchLog(GitBranchLogEvent),
    Status(GitStatusEvent),
    DiffMeta(GitDiffMetaEvent),
    DiffViewport(GitDiffViewportEvent),
    Result(GitResultEvent),
    Error(GitErrorEvent),
}

fn result_then_status(
    repo_root: &std::path::Path,
    path: &std::path::Path,
    action: &str,
    message: &str,
) -> Vec<Emit> {
    let result = Emit::Result(GitResultEvent {
        action: action.to_string(),
        ok: true,
        message: message.to_string(),
    });
    match runner::status_at(repo_root, path) {
        Ok(ev) => vec![result, Emit::Status(ev)],
        Err(e) => vec![result, Emit::Error(GitErrorEvent { message: e.0 })],
    }
}

fn mutate(
    repo_root: &std::path::Path,
    path: &std::path::Path,
    action: &str,
    op: fn(&std::path::Path, &std::path::Path) -> Result<(), runner::GitError>,
) -> Vec<Emit> {
    match op(repo_root, path) {
        Ok(()) => result_then_status(repo_root, path, action, "ok"),
        Err(e) => vec![Emit::Result(GitResultEvent {
            action: action.to_string(),
            ok: false,
            message: e.0,
        })],
    }
}

impl JobKind {
    pub(super) fn run(self) -> Vec<Emit> {
        match self {
            JobKind::Repository { path } => match GitRepositoryEvent::load(&path) {
                Ok(event) => vec![Emit::Repository(event)],
                Err(error) => vec![Emit::Error(GitErrorEvent { message: error.0 })],
            },
            JobKind::BranchLog { repo_root, branch } => {
                match GitCommitEntry::for_reference(&repo_root, &branch) {
                    Ok(commits) => vec![Emit::BranchLog(GitBranchLogEvent {
                        repo_root: repo_root.to_string_lossy().into_owned(),
                        branch,
                        commits,
                    })],
                    Err(error) => vec![Emit::Error(GitErrorEvent { message: error.0 })],
                }
            }
            JobKind::Diff {
                repo_root,
                generation,
                top_line,
                ..
            } if !runner::has_repository(&repo_root) => vec![
                Emit::DiffMeta(GitDiffMetaEvent { total_lines: 0 }),
                Emit::DiffViewport(GitDiffViewportEvent {
                    generation,
                    first_line: top_line,
                    total_lines: 0,
                    lines: Vec::new(),
                    error: String::new(),
                }),
            ],
            JobKind::Diff {
                repo_root,
                path,
                reference,
                generation,
                top_line,
                rows,
                content,
            } => match if reference.is_empty() {
                content
                    .as_deref()
                    .map(|content| runner::diff_lines_with_content(&repo_root, &path, content))
                    .unwrap_or_else(|| runner::diff_lines(&repo_root, &path))
            } else {
                runner::commit_diff_lines(&repo_root, &reference)
            } {
                Ok(lines) => {
                    let (total, win) = parse::window(&lines, top_line, rows);
                    vec![
                        Emit::DiffMeta(GitDiffMetaEvent { total_lines: total }),
                        Emit::DiffViewport(GitDiffViewportEvent {
                            generation,
                            first_line: top_line.min(total),
                            total_lines: total,
                            lines: win,
                            error: String::new(),
                        }),
                    ]
                }
                Err(error) => vec![Emit::DiffViewport(GitDiffViewportEvent {
                    generation,
                    first_line: top_line,
                    total_lines: 0,
                    lines: Vec::new(),
                    error: error.0,
                })],
            },
            JobKind::Stage { repo_root, path } => mutate(&repo_root, &path, "stage", runner::stage),
            JobKind::Unstage { repo_root, path } => {
                mutate(&repo_root, &path, "unstage", runner::unstage)
            }
            JobKind::Discard { repo_root, path } => {
                mutate(&repo_root, &path, "discard", runner::discard)
            }
            JobKind::Commit { path, message } => match runner::commit(&path, &message) {
                Ok(()) => result_then_status(&path, &path, "commit", "committed"),
                Err(e) => vec![Emit::Result(GitResultEvent {
                    action: "commit".into(),
                    ok: false,
                    message: e.0,
                })],
            },
            JobKind::Fetch { path } => match runner::fetch(&path) {
                Ok(()) => result_then_status(&path, &path, "fetch", "fetched"),
                Err(e) => vec![Emit::Result(GitResultEvent {
                    action: "fetch".into(),
                    ok: false,
                    message: e.0,
                })],
            },
            JobKind::Pull { path } => match runner::pull(&path) {
                Ok(()) => result_then_status(&path, &path, "pull", "pulled"),
                Err(e) => vec![Emit::Result(GitResultEvent {
                    action: "pull".into(),
                    ok: false,
                    message: e.0,
                })],
            },
            JobKind::Operation {
                repo_root,
                operation,
            } => {
                let action = operation.action().to_string();
                match operation.run(&repo_root) {
                    Ok(message) => vec![Emit::Result(GitResultEvent {
                        action,
                        ok: true,
                        message,
                    })],
                    Err(error) => vec![Emit::Result(GitResultEvent {
                        action,
                        ok: false,
                        message: error.0,
                    })],
                }
            }
            JobKind::Push { path } => match runner::push(&path) {
                Ok(()) => result_then_status(&path, &path, "push", "pushed"),
                Err(e) => vec![Emit::Result(GitResultEvent {
                    action: "push".into(),
                    ok: false,
                    message: e.0,
                })],
            },
            JobKind::StageAll { path } => match runner::stage_all(&path) {
                Ok(()) => result_then_status(&path, &path, "stage all", "staged"),
                Err(e) => vec![Emit::Result(GitResultEvent {
                    action: "stage all".into(),
                    ok: false,
                    message: e.0,
                })],
            },
            JobKind::Hunk {
                repo_root,
                path,
                hunk,
                accept,
            } => match runner::apply_hunk(&repo_root, &path, hunk, accept) {
                Ok(()) => result_then_status(
                    &repo_root,
                    &path,
                    if accept { "accept" } else { "reject" },
                    "ok",
                ),
                Err(e) => vec![Emit::Result(GitResultEvent {
                    action: "hunk".into(),
                    ok: false,
                    message: e.0,
                })],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::runner::test_repo;

    fn dirty_repo() -> (tempfile::TempDir, PathBuf) {
        let repo = test_repo::init();
        let file = test_repo::write(repo.path(), "a.txt", "one\n");
        test_repo::run(repo.path(), &["add", "a.txt"]);
        test_repo::run(repo.path(), &["commit", "-qm", "init"]);
        test_repo::write(repo.path(), "a.txt", "two\n");
        (repo, file)
    }

    #[test]
    fn diff_job_emits_meta_then_viewport() {
        let (repo, file) = dirty_repo();
        let emits = JobKind::Diff {
            repo_root: repo.path().to_path_buf(),
            path: file,
            reference: String::new(),
            generation: 7,
            top_line: 0,
            rows: 50,
            content: None,
        }
        .run();
        assert!(matches!(emits[0], Emit::DiffMeta(_)));
        assert!(matches!(
            emits[1],
            Emit::DiffViewport(GitDiffViewportEvent { generation: 7, .. })
        ));
    }

    #[test]
    fn stage_job_emits_result_then_fresh_status() {
        let (repo, file) = dirty_repo();
        let emits = JobKind::Stage {
            repo_root: repo.path().to_path_buf(),
            path: file,
        }
        .run();
        match emits.as_slice() {
            [Emit::Result(r), Emit::Status(s)] => {
                assert!(r.ok);
                assert_eq!(s.file_status, FileStatus::Staged);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn diff_on_non_repo_emits_empty_viewport() {
        let dir = tempfile::tempdir().unwrap();
        let file = test_repo::write(dir.path(), "loose.txt", "x");
        let emits = JobKind::Diff {
            repo_root: dir.path().to_path_buf(),
            path: file,
            reference: String::new(),
            generation: 7,
            top_line: 0,
            rows: 50,
            content: None,
        }
        .run();
        assert!(matches!(
            emits.as_slice(),
            [
                Emit::DiffMeta(GitDiffMetaEvent { total_lines: 0 }),
                Emit::DiffViewport(GitDiffViewportEvent {
                    generation: 7,
                    total_lines: 0,
                    lines,
                    ..
                })
            ] if lines.is_empty()
        ));
    }
}
