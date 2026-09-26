pub use vmux_api::git::{
    DiffKind, DiffLine, FileGitState, FileStatus, GitDiffViewport, GitFileStatus, GitLineMarker,
    GitLineStatus, GitOperationError, GitOperationResult, StyledSpan,
};
use vmux_core::input::KeyModifiers;

use crate::state::{GitBranchCollection, GitPanel};

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitKeyRequest {
    pub key: String,
    pub code: String,
    pub modifiers: KeyModifiers,
    pub repeat: bool,
}

impl GitKeyRequest {
    pub fn captures_browser_default(&self, repository_loaded: bool) -> bool {
        if self.navigation_key() {
            return repository_loaded;
        }
        if self.repeat || self.modifiers.ctrl || self.modifiers.alt || self.modifiers.super_key {
            return false;
        }
        if self.key == "?" || self.key == "Tab" || GitPanel::from_key(&self.key).is_some() {
            return true;
        }
        if !repository_loaded {
            return false;
        }
        matches!(
            self.key.as_str(),
            "e" | "u"
                | "Enter"
                | "a"
                | "s"
                | "A"
                | " "
                | "Space"
                | "x"
                | "c"
                | "r"
                | "M"
                | "f"
                | "n"
                | "d"
                | "C"
                | "V"
                | "t"
                | "g"
        )
    }

    fn navigation_key(&self) -> bool {
        if self.modifiers.alt || self.modifiers.super_key || self.modifiers.shift {
            return false;
        }
        if self.modifiers.ctrl {
            return matches!(self.code.as_str(), "KeyN" | "KeyJ" | "KeyP" | "KeyK");
        }
        matches!(self.code.as_str(), "ArrowDown" | "ArrowUp")
            || matches!(self.key.as_str(), "j" | "k")
    }
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitPanelSelectRequest {
    pub panel: GitPanel,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitFileSelectRequest {
    pub path_bytes: Vec<u8>,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitBranchCollectionSelectRequest {
    pub collection: GitBranchCollection,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitBranchSelectRequest {
    pub reference: String,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitCommitSelectRequest {
    pub commit: String,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitStashSelectRequest {
    pub reference: String,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDiscardFileRequest {
    pub path_bytes: Vec<u8>,
}

#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitRepositoryRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitRepositoryPickerRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitConfigEditRequest {
    pub repo_root: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitUpdateCheckRequest;
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitBranchLogRequest {
    pub repo_root: String,
    pub branch: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDirectoryOpenRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDirectorySelectRequest {
    pub index: u32,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDirectoryAscendRequest {
    pub target: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDirectoryDescendRequest {
    pub target: String,
}
#[vmux_api::ui_event(Copy, Eq, Default, url = "git://")]
pub struct GitDirectoryToggleHiddenRequest;
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDiffRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
    pub reference: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitStageRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitUnstageRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitDiscardRequest {
    pub repo_root: String,
    pub path: String,
    pub path_bytes: Vec<u8>,
}
#[vmux_api::ui_event(Eq, urls = ["git://", "file://"])]
pub struct GitCommitRequest {
    pub path: String,
    pub message: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitFetchRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitPullRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, urls = ["git://", "file://"])]
pub struct GitPushRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, url = "git://")]
pub struct GitStageAllRequest {
    pub path: String,
}
#[vmux_api::ui_event(Eq, urls = ["git://", "file://"])]
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

impl GitFileEntry {
    pub fn can_discard(&self) -> bool {
        self.unstaged && self.status != FileStatus::Untracked
    }
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

#[vmux_api::contract(Eq)]
pub struct GitRepositorySnapshot {
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

#[vmux_api::contract(Eq)]
pub struct GitBranchLog {
    pub repo_root: String,
    pub branch: String,
    pub commits: Vec<GitCommitEntry>,
}

#[vmux_api::ui_event_variants(Eq, url = "git://", shared(repo_root: String))]
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
