pub const GIT_STATUS_EVENT: &str = "git-status";
pub const GIT_REPOSITORY_EVENT: &str = "git-repository";
pub const GIT_BRANCH_LOG_EVENT: &str = "git-branch-log";
pub const GIT_DIFF_META_EVENT: &str = "git-diff-meta";
pub const GIT_DIFF_VIEWPORT_EVENT: &str = "git-diff-viewport";
pub const GIT_RESULT_EVENT: &str = "git-result";
pub const GIT_ERROR_EVENT: &str = "git-error";
pub const GIT_CHANGED_EVENT: &str = "git-changed";
pub const GIT_REPOSITORY_PICKED_EVENT: &str = "git-repository-picked";
pub const GIT_DIRECTORY_EVENT: &str = "git-directory";

macro_rules! wire {
    ($($item:item)*) => {
        $(
            #[derive(
                Clone, Debug, PartialEq, Eq,
                serde::Serialize, serde::Deserialize,
                rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
            )]
            $item
        )*
    };
}

wire! {
    pub struct GitStatusRequest { pub path: String }
    pub struct GitRepositoryRequest { pub path: String }
    pub struct GitRepositoryPickerRequest { pub path: String }
    pub struct GitBranchLogRequest { pub repo_root: String, pub branch: String }
    pub struct GitDirectoryRequest { pub path: String, pub preview: bool }
    pub struct GitDiffRequest { pub repo_root: String, pub path: String, pub path_bytes: Vec<u8>, pub reference: String, pub generation: u64, pub top_line: u32, pub rows: u32 }
    pub struct GitStageRequest { pub repo_root: String, pub path: String, pub path_bytes: Vec<u8> }
    pub struct GitUnstageRequest { pub repo_root: String, pub path: String, pub path_bytes: Vec<u8> }
    pub struct GitDiscardRequest { pub repo_root: String, pub path: String, pub path_bytes: Vec<u8> }
    pub struct GitCommitRequest { pub path: String, pub message: String }
    pub struct GitFetchRequest { pub path: String }
    pub struct GitPullRequest { pub path: String }
    pub struct GitPushRequest { pub path: String }
    pub struct GitStageAllRequest { pub path: String }
    pub struct GitOperationRequest { pub repo_root: String, pub operation: GitOperation }
    pub struct GitHunkRequest { pub repo_root: String, pub path: String, pub path_bytes: Vec<u8>, pub hunk: u32, pub accept: bool }

    pub struct StyledSpan { pub text: String, pub fg: [u8; 3], pub bold: bool, pub italic: bool }
    pub struct DiffLine {
        pub kind: DiffKind,
        pub old_no: Option<u32>,
        pub new_no: Option<u32>,
        pub hunk: Option<u32>,
        pub spans: Vec<StyledSpan>,
    }

    pub struct GitStatusEvent {
        pub path: String,
        pub branch: String,
        pub ahead: u32,
        pub behind: u32,
        pub has_upstream: bool,
        pub file_status: FileStatus,
        pub staged_count: u32,
        pub repo_root: String,
    }

    pub struct GitFileEntry {
        pub path: String,
        pub path_bytes: Vec<u8>,
        pub previous_path: Option<String>,
        pub previous_path_bytes: Option<Vec<u8>>,
        pub status: FileStatus,
        pub staged: bool,
        pub unstaged: bool,
    }

    pub struct GitCommitEntry {
        pub sha: String,
        pub short_sha: String,
        pub author: String,
        pub date: String,
        pub summary: String,
        pub body: String,
        pub references: String,
    }

    pub struct GitBranchEntry {
        pub name: String,
        pub current: bool,
        pub upstream: String,
        pub checkout: String,
    }

    pub struct GitTagEntry {
        pub name: String,
        pub short_sha: String,
        pub date: String,
        pub message: String,
    }

    pub struct GitStashEntry {
        pub index: u32,
        pub reference: String,
        pub message: String,
    }

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

    pub struct GitBranchLogEvent {
        pub repo_root: String,
        pub branch: String,
        pub commits: Vec<GitCommitEntry>,
    }

    pub struct GitDiffMetaEvent { pub total_lines: u32 }
    pub struct GitDiffViewportEvent { pub generation: u64, pub first_line: u32, pub total_lines: u32, pub lines: Vec<DiffLine>, pub error: String }
    pub struct GitResultEvent { pub action: String, pub ok: bool, pub message: String }
    pub struct GitErrorEvent { pub message: String }
    pub struct GitChangedEvent {}
    pub struct GitRepositoryPickedEvent { pub path: String }
    pub struct GitDirectoryEvent {
        pub path: String,
        pub parent_path: String,
        pub entries: Vec<vmux_core::event::FileDirEntry>,
        pub parent_entries: Vec<vmux_core::event::FileDirEntry>,
        pub repo_root: String,
        pub preview: bool,
    }
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
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

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum FileStatus {
    #[default]
    Clean,
    Modified,
    Staged,
    StagedModified,
    Untracked,
    Deleted,
    Conflicted,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum DiffKind {
    #[default]
    Context,
    Add,
    Remove,
    Hunk,
    Staged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffline_rkyv_roundtrips() {
        let line = DiffLine {
            kind: DiffKind::Add,
            old_no: None,
            new_no: Some(7),
            hunk: Some(2),
            spans: vec![StyledSpan {
                text: "x".into(),
                fg: [1, 2, 3],
                bold: false,
                italic: false,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&line).unwrap();
        let back: DiffLine = rkyv::from_bytes::<DiffLine, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back.new_no, Some(7));
        assert_eq!(back.hunk, Some(2));
        assert!(matches!(back.kind, DiffKind::Add));
    }
}
