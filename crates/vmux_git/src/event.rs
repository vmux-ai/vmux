pub use vmux_api::git::{
    DiffKind, DiffLine, FileStatus, GitChangedEvent, GitDiffMetaEvent, GitDiffViewportEvent,
    GitErrorEvent, GitResultEvent, GitStatusEvent, StyledSpan,
};

#[vmux_api::ui_event(Eq, targets = ["git", "files"])]
pub struct GitStatusRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitRepositoryRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitRepositoryPickerRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitAppRequest {
    pub repo_root: String,
    pub action: GitAppAction,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitBranchLogRequest {
    pub repo_root: String,
    pub branch: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitDirectoryRequest {
    pub path: String,
    pub preview: bool,
}
#[vmux_api::ui_event(Eq, targets = ["git", "files"])]
pub struct GitDiffRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
    pub reference: String,
    pub generation: u64,
    pub top_line: u32,
    pub rows: u32,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitStageRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitUnstageRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitDiscardRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
}
#[vmux_api::ui_event(Eq, targets = ["git", "files"])]
pub struct GitCommitRequest {
    pub path: String,
    pub message: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitFetchRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitPullRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, targets = ["git", "files"])]
pub struct GitPushRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitStageAllRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, target = "git")]
pub struct GitOperationRequest {
    pub repo_root: String,
    pub operation: GitOperation,
}
#[vmux_api::ui_event(Eq, targets = ["git", "files"])]
pub struct GitHunkRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
    pub hunk: u32,
    pub accept: bool,
}

#[vmux_api::contract(Eq)]
pub struct GitFileEntry {
    pub path: String,
    pub path_bytes: Vec<u8>,
    pub previous_path: Option<String>,
    pub previous_path_bytes: Option<Vec<u8>>,
    pub status: FileStatus,
    pub staged: bool,
    pub unstaged: bool,
}

#[vmux_api::contract(Eq)]
pub struct GitCommitEntry {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub date: String,
    pub summary: String,
    pub body: String,
    pub references: String,
}

#[vmux_api::contract(Eq)]
pub struct GitBranchEntry {
    pub name: String,
    pub current: bool,
    pub upstream: String,
    pub checkout: String,
    pub short_sha: String,
    pub ahead: u32,
    pub behind: u32,
}

#[vmux_api::contract(Eq)]
pub struct GitTagEntry {
    pub name: String,
    pub short_sha: String,
    pub date: String,
    pub message: String,
}

#[vmux_api::contract(Eq)]
pub struct GitStashEntry {
    pub index: u32,
    pub reference: String,
    pub message: String,
}

#[vmux_api::host_event(Eq, target = "git")]
pub struct GitRepositoryEvent {
    pub path: String,
    pub repo_root: String,
    pub repo_name: String,
    pub branch: String,
    pub upstream: String,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<GitFileEntry>,
    pub commits: Vec<GitCommitEntry>,
    pub branches: Vec<GitBranchEntry>,
    pub remote_branches: Vec<GitBranchEntry>,
    pub tags: Vec<GitTagEntry>,
    pub stashes: Vec<GitStashEntry>,
}

#[vmux_api::host_event(Eq, target = "git")]
pub struct GitBranchLogEvent {
    pub repo_root: String,
    pub branch: String,
    pub commits: Vec<GitCommitEntry>,
}

#[vmux_api::host_event(Eq, target = "git")]
pub struct GitDirectoryEvent {
    pub path: String,
    pub parent_path: String,
    pub entries: Vec<vmux_core::event::FileDirEntry>,
    pub parent_entries: Vec<vmux_core::event::FileDirEntry>,
    pub repo_root: String,
    pub preview: bool,
}
#[vmux_api::contract(Copy, Eq)]
pub enum GitAppAction {
    EditConfig,
    CheckForUpdates,
}

#[vmux_api::contract(Eq)]
pub enum GitOperation {
    Amend,
    CheckoutCommit { commit: String },
    CherryPick { commit: String },
    CreateBranch { branch: String, start_point: String },
    DeleteBranch { branch: String },
    FastForward { branch: String },
    Merge { branch: String },
    Rebase { branch: String },
    Revert { commit: String },
    StashDrop { reference: String },
    StashPop { reference: String },
    StashPush,
}
