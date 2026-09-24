enum Events {}

impl crate::BinEventFamily for Events {
    const TARGET: crate::BinEventTarget = crate::BinEventTarget::Host("git");
}

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

#[vmux_api::host_event(Eq)]
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

#[vmux_api::host_event(Eq)]
pub struct GitDiffMetaEvent {
    pub total_lines: u32,
}

#[vmux_api::host_event(Eq)]
pub struct GitDiffViewportEvent {
    pub generation: u64,
    pub first_line: u32,
    pub total_lines: u32,
    pub lines: Vec<DiffLine>,
    pub error: String,
}

#[vmux_api::host_event(Eq)]
pub struct GitResultEvent {
    pub action: String,
    pub ok: bool,
    pub message: String,
}

#[vmux_api::host_event(Eq)]
pub struct GitErrorEvent {
    pub message: String,
}

#[vmux_api::host_event(Eq, Default)]
pub struct GitChangedEvent {}

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
