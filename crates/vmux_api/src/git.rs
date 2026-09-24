#[vmux_api::contract(Eq)]
pub struct StyledSpan {
    pub text: String,
    pub fg: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

#[vmux_api::contract(Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub hunk: Option<u32>,
    pub spans: Vec<StyledSpan>,
}

#[vmux_api::contract(Copy, Eq, Default)]
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

#[vmux_api::contract(Copy, Eq, Default)]
pub enum DiffKind {
    #[default]
    Context,
    Add,
    Remove,
    Hunk,
    Staged,
}

#[vmux_api::contract(Eq)]
pub struct GitFileStatus {
    pub path: String,
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    pub file_status: FileStatus,
    pub staged_count: u32,
    pub repo_root: String,
}

#[vmux_api::contract(Copy, Eq)]
pub enum GitLineStatus {
    Added,
    Modified,
    Deleted,
    Staged,
}

#[vmux_api::contract(Copy, Eq)]
pub struct GitLineMarker {
    pub line: u32,
    pub status: GitLineStatus,
}

#[vmux_api::contract(Eq)]
pub struct GitDiffViewport {
    pub generation: u64,
    pub first_line: u32,
    pub total_lines: u32,
    pub lines: Vec<DiffLine>,
    pub markers: Vec<GitLineMarker>,
    pub error: String,
}

#[vmux_api::contract(Eq)]
pub struct GitOperationResult {
    pub action: String,
    pub ok: bool,
    pub message: String,
}

#[vmux_api::contract(Eq, Default)]
pub struct FileGitState {
    pub path: String,
    pub repo_root: String,
    pub has_diff: bool,
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub staged_count: u32,
    pub message: String,
    pub result: Option<GitOperationResult>,
    pub result_sequence: u64,
    pub refresh_revision: u64,
    pub diff_viewport: Option<GitDiffViewport>,
    pub diff_loading: bool,
}

#[vmux_api::contract(Eq)]
pub struct GitOperationError {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_line_round_trips() {
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
        assert_eq!(back, line);
    }
}
