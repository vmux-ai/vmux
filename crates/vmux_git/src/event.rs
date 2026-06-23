pub const GIT_STATUS_EVENT: &str = "git-status";
pub const GIT_DIFF_META_EVENT: &str = "git-diff-meta";
pub const GIT_DIFF_VIEWPORT_EVENT: &str = "git-diff-viewport";
pub const GIT_RESULT_EVENT: &str = "git-result";
pub const GIT_ERROR_EVENT: &str = "git-error";

macro_rules! wire {
    ($($item:item)*) => {
        $(
            #[derive(
                Clone, Debug,
                serde::Serialize, serde::Deserialize,
                rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
            )]
            $item
        )*
    };
}

wire! {
    pub struct GitStatusRequest { pub path: String }
    pub struct GitDiffRequest { pub path: String, pub top_line: u32, pub rows: u32 }
    pub struct GitStageRequest { pub path: String }
    pub struct GitUnstageRequest { pub path: String }
    pub struct GitDiscardRequest { pub path: String }
    pub struct GitCommitRequest { pub path: String, pub message: String }
    pub struct GitPushRequest { pub path: String }
    pub struct GitHunkRequest { pub path: String, pub hunk: u32, pub accept: bool }

    pub struct StyledSpan { pub text: String, pub fg: [u8; 3], pub bold: bool, pub italic: bool }
    pub struct DiffLine {
        pub kind: DiffKind,
        pub old_no: Option<u32>,
        pub new_no: Option<u32>,
        pub hunk: Option<u32>,
        pub spans: Vec<StyledSpan>,
    }
    pub struct GitStatusEvent {
        pub branch: String,
        pub ahead: u32,
        pub behind: u32,
        pub has_upstream: bool,
        pub file_status: FileStatus,
        pub staged_count: u32,
        pub repo_root: String,
    }
    pub struct GitDiffMetaEvent { pub total_lines: u32 }
    pub struct GitDiffViewportEvent { pub first_line: u32, pub total_lines: u32, pub lines: Vec<DiffLine> }
    pub struct GitResultEvent { pub action: String, pub ok: bool, pub message: String }
    pub struct GitErrorEvent { pub message: String }
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
