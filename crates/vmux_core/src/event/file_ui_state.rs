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

#[vmux_api::ui_state_patch(Default)]
pub struct FileUiStatePatch {
    pub meta: Option<FileMetaEvent>,
    pub viewport: Option<FileViewportPatch>,
    pub note: Option<FileNoteEvent>,
    pub error: Option<FileErrorEvent>,
    pub scroll_by: Option<FileScrollByEvent>,
    pub directory: Option<FileDirEvent>,
    pub theme: Option<FileThemeEvent>,
    pub preview: Option<FilePreviewEvent>,
    pub media: Option<FileMediaEvent>,
    pub cursor: Option<FileCursorEvent>,
    pub dirty: Option<FileDirtyEvent>,
    pub view_mode: Option<FileViewModeEvent>,
    pub keymap: Option<FileKeymapEvent>,
    pub shape: Option<FileShapeEvent>,
    pub encoding: Option<FileEncodingEvent>,
    pub tidy_prompt: Option<FileTidyPromptEvent>,
    pub explorer_tree: Option<ExplorerTreeEvent>,
    pub explorer_focus: Option<ExplorerFocusEvent>,
    pub explorer_fs_result: Option<ExplorerFsResult>,
    pub open_editors: Option<OpenEditorsEvent>,
    pub outline: Option<OutlineEvent>,
    pub explorer_panel: Option<ExplorerPanelEvent>,
    pub explorer_search: Option<ExplorerSearchEvent>,
    pub diagnostics: Option<FileDiagnostics>,
    pub lsp_status: Option<FileLspStatus>,
    pub lsp_install_progress: Option<LspInstallProgress>,
    pub lsp_package_status: Option<LspPackageStatus>,
    pub hover: Option<FileHover>,
    pub code_actions: Option<FileCodeActions>,
    pub edit_failed: Option<FileEditFailure>,
    pub rename_begin: Option<FileRenamePrompt>,
    pub panel: Option<FilePanelState>,
    pub git_state: Option<FileGitState>,
    pub key: Option<FileKey>,
}

#[vmux_api::ui_state(Default, url = "file://")]
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
        assert!(decoded.patches[0].meta.is_some());
        assert!(decoded.patches[1].dirty.is_some());
        assert_eq!(decoded.sequence, 1);
    }
}
