use std::path::PathBuf;

use bevy::prelude::Component;

use crate::event::*;

#[derive(Debug, Clone)]
pub(super) enum GitJobEmit {
    Repository(GitRepositorySnapshot),
    BranchLog(GitBranchLog),
    Status(GitFileStatus),
    DiffViewport(GitDiffViewport),
    Result(GitOperationResult),
    Error(GitOperationError),
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
