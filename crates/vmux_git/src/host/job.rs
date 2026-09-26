use std::path::PathBuf;

use bevy::prelude::Component;

use crate::event::*;
use crate::host::{parse, runner};

#[derive(Debug, Clone)]
pub(super) enum Emit {
    Repository(GitRepositorySnapshot),
    BranchLog(GitBranchLog),
    Status(GitFileStatus),
    DiffViewport(GitDiffViewport),
    Result(GitOperationResult),
    Error(GitOperationError),
}

pub(super) trait GitTask: Component + Clone {
    fn run(self) -> Vec<Emit>;
}

#[derive(Clone, Component)]
pub(super) struct RepositoryJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct BranchLogJob {
    pub(super) repo_root: PathBuf,
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct DiffJob {
    pub(super) repo_root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) reference: String,
    pub(super) generation: u64,
    pub(super) top_line: u32,
    pub(super) rows: u32,
    pub(super) content: Option<String>,
}

#[derive(Clone, Component)]
pub(super) struct StageJob {
    pub(super) repo_root: PathBuf,
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct UnstageJob {
    pub(super) repo_root: PathBuf,
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct DiscardJob {
    pub(super) repo_root: PathBuf,
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct CommitJob {
    pub(super) path: PathBuf,
    pub(super) message: String,
}

#[derive(Clone, Component)]
pub(super) struct FetchJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct PullJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct PushJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct StageAllJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct HunkJob {
    pub(super) repo_root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) hunk: u32,
    pub(super) accept: bool,
}

#[derive(Clone, Component)]
pub(super) struct AmendJob {
    pub(super) repo_root: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct CheckoutCommitJob {
    pub(super) repo_root: PathBuf,
    pub(super) commit: String,
}

#[derive(Clone, Component)]
pub(super) struct CherryPickJob {
    pub(super) repo_root: PathBuf,
    pub(super) commit: String,
}

#[derive(Clone, Component)]
pub(super) struct CreateBranchJob {
    pub(super) repo_root: PathBuf,
    pub(super) branch: String,
    pub(super) start_point: String,
}

#[derive(Clone, Component)]
pub(super) struct DeleteBranchJob {
    pub(super) repo_root: PathBuf,
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct FastForwardJob {
    pub(super) repo_root: PathBuf,
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct MergeJob {
    pub(super) repo_root: PathBuf,
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct RebaseJob {
    pub(super) repo_root: PathBuf,
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct RevertJob {
    pub(super) repo_root: PathBuf,
    pub(super) commit: String,
}

#[derive(Clone, Component)]
pub(super) struct StashDropJob {
    pub(super) repo_root: PathBuf,
    pub(super) reference: String,
}

#[derive(Clone, Component)]
pub(super) struct StashPopJob {
    pub(super) repo_root: PathBuf,
    pub(super) reference: String,
}

#[derive(Clone, Component)]
pub(super) struct StashPushJob {
    pub(super) repo_root: PathBuf,
}

fn result_then_status(
    repo_root: &std::path::Path,
    path: &std::path::Path,
    operation: &str,
    message: &str,
) -> Vec<Emit> {
    let result = Emit::Result(GitOperationResult {
        operation: operation.to_string(),
        ok: true,
        message: message.to_string(),
    });
    match runner::status_at(repo_root, path) {
        Ok(event) => vec![result, Emit::Status(event)],
        Err(error) => vec![result, Emit::Error(GitOperationError { message: error.0 })],
    }
}

fn mutate(
    repo_root: &std::path::Path,
    path: &std::path::Path,
    operation: &str,
    run: fn(&std::path::Path, &std::path::Path) -> Result<(), runner::GitError>,
) -> Vec<Emit> {
    match run(repo_root, path) {
        Ok(()) => result_then_status(repo_root, path, operation, "ok"),
        Err(error) => vec![Emit::Result(GitOperationResult {
            operation: operation.to_string(),
            ok: false,
            message: error.0,
        })],
    }
}

fn operation(operation: &str, result: Result<String, runner::GitError>) -> Vec<Emit> {
    match result {
        Ok(message) => vec![Emit::Result(GitOperationResult {
            operation: operation.to_string(),
            ok: true,
            message,
        })],
        Err(error) => vec![Emit::Result(GitOperationResult {
            operation: operation.to_string(),
            ok: false,
            message: error.0,
        })],
    }
}

impl GitTask for RepositoryJob {
    fn run(self) -> Vec<Emit> {
        match GitRepositorySnapshot::load(&self.path) {
            Ok(event) => vec![Emit::Repository(event)],
            Err(error) => vec![Emit::Error(GitOperationError { message: error.0 })],
        }
    }
}

impl GitTask for BranchLogJob {
    fn run(self) -> Vec<Emit> {
        match GitCommitEntry::for_reference(&self.repo_root, &self.branch) {
            Ok(commits) => vec![Emit::BranchLog(GitBranchLog {
                repo_root: self.repo_root.to_string_lossy().into_owned(),
                branch: self.branch,
                commits,
            })],
            Err(error) => vec![Emit::Error(GitOperationError { message: error.0 })],
        }
    }
}

impl GitTask for DiffJob {
    fn run(self) -> Vec<Emit> {
        if !runner::has_repository(&self.repo_root) {
            return vec![Emit::DiffViewport(GitDiffViewport {
                generation: self.generation,
                first_line: self.top_line,
                total_lines: 0,
                lines: Vec::new(),
                markers: Vec::new(),
                error: String::new(),
            })];
        }
        let result = if self.reference.is_empty() {
            match self.content.as_deref() {
                Some(content) => {
                    runner::diff_lines_with_content(&self.repo_root, &self.path, content)
                }
                None => runner::diff_lines(&self.repo_root, &self.path),
            }
        } else {
            runner::commit_diff_lines(&self.repo_root, &self.reference)
        };
        match result {
            Ok(all_lines) => {
                let markers = super::diff::GitDiffMarkers::from_lines(&all_lines).into_inner();
                let (total_lines, lines) = parse::window(&all_lines, self.top_line, self.rows);
                vec![Emit::DiffViewport(GitDiffViewport {
                    generation: self.generation,
                    first_line: self.top_line.min(total_lines),
                    total_lines,
                    lines,
                    markers,
                    error: String::new(),
                })]
            }
            Err(error) => vec![Emit::DiffViewport(GitDiffViewport {
                generation: self.generation,
                first_line: self.top_line,
                total_lines: 0,
                lines: Vec::new(),
                markers: Vec::new(),
                error: error.0,
            })],
        }
    }
}

impl GitTask for StageJob {
    fn run(self) -> Vec<Emit> {
        mutate(&self.repo_root, &self.path, "stage", runner::stage)
    }
}

impl GitTask for UnstageJob {
    fn run(self) -> Vec<Emit> {
        mutate(&self.repo_root, &self.path, "unstage", runner::unstage)
    }
}

impl GitTask for DiscardJob {
    fn run(self) -> Vec<Emit> {
        mutate(&self.repo_root, &self.path, "discard", runner::discard)
    }
}

impl GitTask for CommitJob {
    fn run(self) -> Vec<Emit> {
        match runner::commit(&self.path, &self.message) {
            Ok(()) => result_then_status(&self.path, &self.path, "commit", "committed"),
            Err(error) => vec![Emit::Result(GitOperationResult {
                operation: "commit".into(),
                ok: false,
                message: error.0,
            })],
        }
    }
}

impl GitTask for FetchJob {
    fn run(self) -> Vec<Emit> {
        match runner::fetch(&self.path) {
            Ok(()) => result_then_status(&self.path, &self.path, "fetch", "fetched"),
            Err(error) => vec![Emit::Result(GitOperationResult {
                operation: "fetch".into(),
                ok: false,
                message: error.0,
            })],
        }
    }
}

impl GitTask for PullJob {
    fn run(self) -> Vec<Emit> {
        match runner::pull(&self.path) {
            Ok(()) => result_then_status(&self.path, &self.path, "pull", "pulled"),
            Err(error) => vec![Emit::Result(GitOperationResult {
                operation: "pull".into(),
                ok: false,
                message: error.0,
            })],
        }
    }
}

impl GitTask for PushJob {
    fn run(self) -> Vec<Emit> {
        match runner::push(&self.path) {
            Ok(()) => result_then_status(&self.path, &self.path, "push", "pushed"),
            Err(error) => vec![Emit::Result(GitOperationResult {
                operation: "push".into(),
                ok: false,
                message: error.0,
            })],
        }
    }
}

impl GitTask for StageAllJob {
    fn run(self) -> Vec<Emit> {
        match runner::stage_all(&self.path) {
            Ok(()) => result_then_status(&self.path, &self.path, "stage all", "staged"),
            Err(error) => vec![Emit::Result(GitOperationResult {
                operation: "stage all".into(),
                ok: false,
                message: error.0,
            })],
        }
    }
}

impl GitTask for HunkJob {
    fn run(self) -> Vec<Emit> {
        match runner::apply_hunk(&self.repo_root, &self.path, self.hunk, self.accept) {
            Ok(()) => result_then_status(
                &self.repo_root,
                &self.path,
                if self.accept { "accept" } else { "reject" },
                "ok",
            ),
            Err(error) => vec![Emit::Result(GitOperationResult {
                operation: "hunk".into(),
                ok: false,
                message: error.0,
            })],
        }
    }
}

impl GitTask for AmendJob {
    fn run(self) -> Vec<Emit> {
        operation("amend", runner::amend(&self.repo_root))
    }
}

impl GitTask for CheckoutCommitJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "checkout commit",
            runner::checkout_commit(&self.repo_root, &self.commit),
        )
    }
}

impl GitTask for CherryPickJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "cherry-pick",
            runner::cherry_pick(&self.repo_root, &self.commit),
        )
    }
}

impl GitTask for CreateBranchJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "new branch",
            runner::create_branch(&self.repo_root, &self.branch, &self.start_point),
        )
    }
}

impl GitTask for DeleteBranchJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "delete branch",
            runner::delete_branch(&self.repo_root, &self.branch),
        )
    }
}

impl GitTask for FastForwardJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "fast-forward",
            runner::fast_forward(&self.repo_root, &self.branch),
        )
    }
}

impl GitTask for MergeJob {
    fn run(self) -> Vec<Emit> {
        operation("merge", runner::merge(&self.repo_root, &self.branch))
    }
}

impl GitTask for RebaseJob {
    fn run(self) -> Vec<Emit> {
        operation("rebase", runner::rebase(&self.repo_root, &self.branch))
    }
}

impl GitTask for RevertJob {
    fn run(self) -> Vec<Emit> {
        operation("revert", runner::revert(&self.repo_root, &self.commit))
    }
}

impl GitTask for StashDropJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "stash drop",
            runner::stash_drop(&self.repo_root, &self.reference),
        )
    }
}

impl GitTask for StashPopJob {
    fn run(self) -> Vec<Emit> {
        operation(
            "stash pop",
            runner::stash_pop(&self.repo_root, &self.reference),
        )
    }
}

impl GitTask for StashPushJob {
    fn run(self) -> Vec<Emit> {
        operation("stash", runner::stash_push(&self.repo_root))
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
    fn diff_job_emits_projected_viewport() {
        let (repo, file) = dirty_repo();
        let emits = DiffJob {
            repo_root: repo.path().to_path_buf(),
            path: file,
            reference: String::new(),
            generation: 7,
            top_line: 0,
            rows: 50,
            content: None,
        }
        .run();
        assert!(matches!(
            emits[0],
            Emit::DiffViewport(GitDiffViewport { generation: 7, .. })
        ));
    }

    #[test]
    fn stage_job_emits_result_then_fresh_status() {
        let (repo, file) = dirty_repo();
        let emits = StageJob {
            repo_root: repo.path().to_path_buf(),
            path: file,
        }
        .run();
        match emits.as_slice() {
            [Emit::Result(result), Emit::Status(status)] => {
                assert!(result.ok);
                assert_eq!(status.file_status, FileStatus::Staged);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn diff_on_non_repo_emits_empty_viewport() {
        let dir = tempfile::tempdir().unwrap();
        let file = test_repo::write(dir.path(), "loose.txt", "x");
        let emits = DiffJob {
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
            [Emit::DiffViewport(GitDiffViewport {
                generation: 7,
                total_lines: 0,
                lines,
                markers,
                ..
            })] if lines.is_empty() && markers.is_empty()
        ));
    }
}
