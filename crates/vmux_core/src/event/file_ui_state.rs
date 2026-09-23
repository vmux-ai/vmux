use super::{
    ExplorerFocusEvent, ExplorerFsResult, ExplorerPanelEvent, ExplorerSearchEvent,
    ExplorerTreeEvent, FileCodeActionsEvent, FileCompletionEvent, FileCursorEvent,
    FileDiagnosticsEvent, FileDirEvent, FileDirtyEvent, FileEditFailedEvent, FileEncodingEvent,
    FileErrorEvent, FileHoverEvent, FileKeymapEvent, FileLspStatusEvent, FileMediaEvent,
    FileMetaEvent, FileNoteEvent, FilePreviewEvent, FileReferencesEvent, FileRenameBeginEvent,
    FileScrollByEvent, FileShapeEvent, FileThemeEvent, FileTidyPromptEvent, FileViewModeEvent,
    FileViewportPatch, LspInstallProgress, LspPkgStatusEvent, OpenEditorsEvent, OutlineEvent,
};
use vmux_api::git::{
    GitChangedEvent, GitDiffMetaEvent, GitDiffViewportEvent, GitErrorEvent, GitResultEvent,
    GitStatusEvent,
};

#[vmux_api::payload]
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
    Diagnostics(FileDiagnosticsEvent),
    LspStatus(FileLspStatusEvent),
    LspInstallProgress(LspInstallProgress),
    LspPackageStatus(LspPkgStatusEvent),
    Hover(FileHoverEvent),
    CodeActions(FileCodeActionsEvent),
    EditFailed(FileEditFailedEvent),
    RenameBegin(FileRenameBeginEvent),
    References(FileReferencesEvent),
    Completion(FileCompletionEvent),
    GitStatus(GitStatusEvent),
    GitDiffMeta(GitDiffMetaEvent),
    GitDiffViewport(GitDiffViewportEvent),
    GitResult(GitResultEvent),
    GitError(GitErrorEvent),
    GitChanged(GitChangedEvent),
}

#[vmux_api::payload(Default)]
#[vmux_api::host_event(target = "files")]
pub struct FileUiStateEvent {
    pub sequence: u64,
    pub patches: Vec<FileUiStatePatch>,
}

pub trait FileUiStatePayload: Clone + 'static {
    fn from_patch(patch: &FileUiStatePatch) -> Option<&Self>;
}

macro_rules! impl_patch_from {
    ($($variant:ident: $event:ty),* $(,)?) => {
        $(
            impl From<$event> for FileUiStatePatch {
                fn from(event: $event) -> Self {
                    Self::$variant(event)
                }
            }

            impl FileUiStatePayload for $event {
                fn from_patch(patch: &FileUiStatePatch) -> Option<&Self> {
                    let FileUiStatePatch::$variant(event) = patch else {
                        return None;
                    };
                    Some(event)
                }
            }
        )*
    };
}

impl_patch_from!(
    Meta: FileMetaEvent,
    Viewport: FileViewportPatch,
    Note: FileNoteEvent,
    Error: FileErrorEvent,
    ScrollBy: FileScrollByEvent,
    Directory: FileDirEvent,
    Theme: FileThemeEvent,
    Preview: FilePreviewEvent,
    Media: FileMediaEvent,
    Cursor: FileCursorEvent,
    Dirty: FileDirtyEvent,
    ViewMode: FileViewModeEvent,
    Keymap: FileKeymapEvent,
    Shape: FileShapeEvent,
    Encoding: FileEncodingEvent,
    TidyPrompt: FileTidyPromptEvent,
    ExplorerTree: ExplorerTreeEvent,
    ExplorerFocus: ExplorerFocusEvent,
    ExplorerFsResult: ExplorerFsResult,
    OpenEditors: OpenEditorsEvent,
    Outline: OutlineEvent,
    ExplorerPanel: ExplorerPanelEvent,
    ExplorerSearch: ExplorerSearchEvent,
    Diagnostics: FileDiagnosticsEvent,
    LspStatus: FileLspStatusEvent,
    LspInstallProgress: LspInstallProgress,
    LspPackageStatus: LspPkgStatusEvent,
    Hover: FileHoverEvent,
    CodeActions: FileCodeActionsEvent,
    EditFailed: FileEditFailedEvent,
    RenameBegin: FileRenameBeginEvent,
    References: FileReferencesEvent,
    Completion: FileCompletionEvent,
    GitStatus: GitStatusEvent,
    GitDiffMeta: GitDiffMetaEvent,
    GitDiffViewport: GitDiffViewportEvent,
    GitResult: GitResultEvent,
    GitError: GitErrorEvent,
    GitChanged: GitChangedEvent,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_preserve_patch_order() {
        let event = FileUiStateEvent {
            sequence: 1,
            patches: vec![
                FileMetaEvent {
                    path: "src/lib.rs".into(),
                    abs_path: "/repo/src/lib.rs".into(),
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
        let decoded = rkyv::from_bytes::<FileUiStateEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(matches!(decoded.patches[0], FileUiStatePatch::Meta(_)));
        assert!(matches!(decoded.patches[1], FileUiStatePatch::Dirty(_)));
        assert_eq!(decoded.sequence, 1);
    }
}
