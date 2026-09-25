use super::{
    ExplorerFocusEvent, ExplorerFsResult, ExplorerPanelEvent, ExplorerSearchEvent,
    ExplorerTreeEvent, FileCodeActions, FileCursorEvent, FileDiagnostics, FileDirEvent,
    FileDirtyEvent, FileEditFailure, FileEncodingEvent, FileErrorEvent, FileHover, FileKey,
    FileKeymapEvent, FileLspStatus, FileMediaEvent, FileMetaEvent, FileNoteEvent, FilePanelState,
    FilePreviewEvent, FileRenamePrompt, FileScrollByEvent, FileShapeEvent, FileThemeEvent,
    FileTidyPromptEvent, FileViewModeEvent, FileViewportPatch, LspInstallProgress,
    LspPackageStatus, OpenEditorsEvent, OutlineEvent,
};
use vmux_api::git::FileGitState;

#[vmux_api::ui_state_patch]
pub enum FileUiStatePatch {
    Meta(FileMetaEvent),
    Viewport(FileViewportPatch),
    Note(FileNoteEvent),
    Error(FileErrorEvent),
    ScrollBy(FileScrollByEvent),
    Directory(FileDirEvent),
    Theme(FileThemeEvent),
    Preview(FilePreviewEvent),
    Media(FileMediaEvent),
    Cursor(FileCursorEvent),
    Dirty(FileDirtyEvent),
    ViewMode(FileViewModeEvent),
    Keymap(FileKeymapEvent),
    Shape(FileShapeEvent),
    Encoding(FileEncodingEvent),
    TidyPrompt(FileTidyPromptEvent),
    ExplorerTree(ExplorerTreeEvent),
    ExplorerFocus(ExplorerFocusEvent),
    ExplorerFsResult(ExplorerFsResult),
    OpenEditors(OpenEditorsEvent),
    Outline(OutlineEvent),
    ExplorerPanel(ExplorerPanelEvent),
    ExplorerSearch(ExplorerSearchEvent),
    Diagnostics(FileDiagnostics),
    LspStatus(FileLspStatus),
    LspInstallProgress(LspInstallProgress),
    LspPackageStatus(LspPackageStatus),
    Hover(FileHover),
    CodeActions(FileCodeActions),
    EditFailed(FileEditFailure),
    RenameBegin(FileRenamePrompt),
    Panel(FilePanelState),
    GitState(FileGitState),
    Key(FileKey),
}

#[vmux_api::ui_state(Default, target = "files")]
pub struct FileUiState {
    pub sequence: u64,
    pub patches: Vec<FileUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::FileDocumentKind;

    #[test]
    fn batches_preserve_patch_order() {
        let event = FileUiState {
            sequence: 1,
            patches: vec![
                FileMetaEvent {
                    revision: 1,
                    path: "src/lib.rs".into(),
                    abs_path: "/repo/src/lib.rs".into(),
                    kind: FileDocumentKind::Text,
                    language: "Rust".into(),
                    total_lines: 12,
                    indent: Default::default(),
                    line_ending: Default::default(),
                    encoding: Default::default(),
                }
                .into(),
                FileDirtyEvent { dirty: true }.into(),
            ],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<FileUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(matches!(decoded.patches[0], FileUiStatePatch::Meta(_)));
        assert!(matches!(decoded.patches[1], FileUiStatePatch::Dirty(_)));
        assert_eq!(decoded.sequence, 1);
    }
}
