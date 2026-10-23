use std::path::PathBuf;

use bevy::prelude::Component;

use crate::event::GitCatalog;

#[derive(Clone, Component)]
pub(super) struct RepositoryJob {
    pub(super) path: PathBuf,
    pub(super) refresh_catalogs: bool,
}

#[derive(Clone, Component)]
pub(super) struct CatalogJob {
    pub(super) catalog: GitCatalog,
    pub(super) current_branch: String,
    pub(super) offset: usize,
    pub(super) limit: usize,
    pub(super) revision: u32,
}

#[derive(Clone, Component)]
pub(super) struct DiffJob {
    pub(super) path: PathBuf,
    pub(super) scope: DiffScope,
    pub(super) branch_paths: Vec<Vec<u8>>,
    pub(super) branch_has_more: bool,
    pub(super) generation: u64,
    pub(super) top_line: u32,
    pub(super) rows: u32,
    pub(super) content: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DiffScope {
    File,
    Branch(String),
    Commit(String),
    Stash(String),
}

#[derive(Clone, Component)]
pub(super) struct BranchFilesJob {
    pub(super) reference: String,
    pub(super) generation: u64,
}

#[derive(Clone, Component)]
pub(super) struct StageJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct UnstageJob {
    pub(super) path: PathBuf,
}

#[derive(Clone, Component)]
pub(super) struct DiscardJob {
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
    pub(super) path: PathBuf,
    pub(super) hunk: u32,
    pub(super) accept: bool,
}

#[derive(Clone, Component)]
pub(super) struct AmendJob;

#[derive(Clone, Component)]
pub(super) struct CheckoutCommitJob {
    pub(super) commit: String,
}

#[derive(Clone, Component)]
pub(super) struct CherryPickJob {
    pub(super) commit: String,
}

#[derive(Clone, Component)]
pub(super) struct CreateBranchJob {
    pub(super) branch: String,
    pub(super) start_point: String,
}

#[derive(Clone, Component)]
pub(super) struct DeleteBranchJob {
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct FastForwardJob {
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct MergeJob {
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct RebaseJob {
    pub(super) branch: String,
}

#[derive(Clone, Component)]
pub(super) struct RevertJob {
    pub(super) commit: String,
}

#[derive(Clone, Component)]
pub(super) struct StashDropJob {
    pub(super) reference: String,
}

#[derive(Clone, Component)]
pub(super) struct StashPopJob {
    pub(super) reference: String,
}

#[derive(Clone, Component)]
pub(super) struct StashPushJob;
