use super::{
    ExplorerFocusEvent, ExplorerNotice, ExplorerPanelEvent, ExplorerPromptState,
    ExplorerSearchEvent, ExplorerTreeEvent, FileBreadcrumbState, FileCodeActions, FileCursorEvent,
    FileDiagnostics, FileDirectoryState, FileDirtyEvent, FileEditNotice, FileEncodingEvent,
    FileErrorEvent, FileFindEvent, FileHoverState, FileKeymapEvent, FileLspStatus, FileMediaEvent,
    FileMetaEvent, FileNoteEvent, FilePanelState, FilePreviewState, FileRenameState,
    FileScrollByEvent, FileShapeEvent, FileThemeEvent, FileTidyState, FileViewModeEvent,
    FileViewportPatch, LspInstallNotice, OpenEditorsEvent, OutlineEvent,
};
use vmux_api::git::FileGitState;

#[vmux_api::ui_state_patch(Default)]
pub struct FileUiStatePatch {
    pub meta: Option<FileMetaEvent>,
    pub viewport: Option<FileViewportPatch>,
    pub note: Option<FileNoteEvent>,
    pub error: Option<FileErrorEvent>,
    pub scroll_by: Option<FileScrollByEvent>,
    pub directory: Option<FileDirectoryState>,
    pub breadcrumb: Option<FileBreadcrumbState>,
    pub theme: Option<FileThemeEvent>,
    pub preview: Option<FilePreviewState>,
    pub media: Option<FileMediaEvent>,
    pub cursor: Option<FileCursorEvent>,
    pub dirty: Option<FileDirtyEvent>,
    pub view_mode: Option<FileViewModeEvent>,
    pub keymap: Option<FileKeymapEvent>,
    pub shape: Option<FileShapeEvent>,
    pub encoding: Option<FileEncodingEvent>,
    pub tidy: Option<FileTidyState>,
    pub explorer_tree: Option<ExplorerTreeEvent>,
    pub explorer_focus: Option<ExplorerFocusEvent>,
    pub explorer_notice: Option<ExplorerNotice>,
    pub open_editors: Option<OpenEditorsEvent>,
    pub outline: Option<OutlineEvent>,
    pub explorer_panel: Option<ExplorerPanelEvent>,
    pub explorer_search: Option<ExplorerSearchEvent>,
    pub explorer_prompt: Option<ExplorerPromptState>,
    pub diagnostics: Option<FileDiagnostics>,
    pub lsp_status: Option<FileLspStatus>,
    pub lsp_install_notice: Option<LspInstallNotice>,
    pub hover: Option<FileHoverState>,
    pub code_actions: Option<FileCodeActions>,
    pub edit_notice: Option<FileEditNotice>,
    pub rename: Option<FileRenameState>,
    pub panel: Option<FilePanelState>,
    pub git_state: Option<FileGitState>,
    pub find: Option<FileFindEvent>,
}

#[vmux_api::ui_state(Default, patch = FileUiStatePatch, version = 3)]
pub struct FileUiState {
    pub document: FileDocumentUiState,
    pub viewport: FileViewportUiState,
    pub explorer: FileExplorerUiState,
    pub language: FileLanguageUiState,
    pub panel: FilePanelUiState,
    pub git: FileGitUiState,
}

#[vmux_api::contract(Default)]
pub struct FileDocumentUiState {
    pub meta: Option<FileMetaEvent>,
    pub note: Option<FileNoteEvent>,
    pub error: Option<FileErrorEvent>,
    pub directory: Option<FileDirectoryState>,
    pub breadcrumb: FileBreadcrumbState,
    pub theme: Option<FileThemeEvent>,
    pub preview: Option<FilePreviewState>,
    pub media: Option<FileMediaEvent>,
    pub dirty: Option<FileDirtyEvent>,
    pub view_mode: Option<FileViewModeEvent>,
    pub shape: Option<FileShapeEvent>,
    pub encoding: Option<FileEncodingEvent>,
    pub edit_notice: FileEditNotice,
}

#[vmux_api::contract(Default)]
pub struct FileViewportUiState {
    pub content: Option<FileViewportPatch>,
    pub scroll_by: Option<FileScrollByEvent>,
    pub cursor: Option<FileCursorEvent>,
    pub keymap: Option<FileKeymapEvent>,
    pub find: Option<FileFindEvent>,
}

#[vmux_api::contract(Default)]
pub struct FileExplorerUiState {
    pub tidy: FileTidyState,
    pub explorer_tree: Option<ExplorerTreeEvent>,
    pub explorer_focus: Option<ExplorerFocusEvent>,
    pub explorer_notice: ExplorerNotice,
    pub open_editors: Option<OpenEditorsEvent>,
    pub outline: Option<OutlineEvent>,
    pub explorer_panel: Option<ExplorerPanelEvent>,
    pub explorer_search: ExplorerSearchEvent,
    pub explorer_prompt: ExplorerPromptState,
}

#[vmux_api::contract(Default)]
pub struct FileLanguageUiState {
    pub diagnostics: Option<FileDiagnostics>,
    pub lsp_status: Option<FileLspStatus>,
    pub lsp_install_notice: LspInstallNotice,
    pub hover: FileHoverState,
    pub code_actions: Option<FileCodeActions>,
    pub rename: FileRenameState,
}

#[vmux_api::contract(Default)]
pub struct FilePanelUiState {
    pub panel: Option<FilePanelState>,
}

#[vmux_api::contract(Default)]
pub struct FileGitUiState {
    pub git_state: Option<FileGitState>,
}

impl vmux_api::UiStateProjection<FileUiStatePatch> for FileUiState {
    fn apply(&mut self, patch: FileUiStatePatch) {
        let reset_document = patch.meta.as_ref().is_some_and(|meta| {
            self.document
                .meta
                .as_ref()
                .is_none_or(|current| current.revision != meta.revision)
        });
        if reset_document {
            self.viewport.content = None;
            self.document.note = None;
            self.document.error = None;
            self.viewport.scroll_by = None;
            self.document.directory = None;
            self.document.breadcrumb = FileBreadcrumbState::default();
            self.document.preview = None;
            self.document.media = None;
            self.viewport.cursor = None;
            self.document.dirty = None;
            self.document.shape = None;
            self.document.encoding = None;
            self.explorer.tidy = FileTidyState::default();
            self.explorer.outline = None;
            self.explorer.explorer_notice = ExplorerNotice::default();
            self.explorer.explorer_search = ExplorerSearchEvent::default();
            self.explorer.explorer_prompt = ExplorerPromptState::default();
            self.language.diagnostics = None;
            self.language.lsp_status = None;
            self.language.lsp_install_notice = LspInstallNotice::default();
            self.language.hover = FileHoverState::default();
            self.language.code_actions = None;
            self.document.edit_notice = FileEditNotice::default();
            self.language.rename = FileRenameState::default();
            self.panel.panel = None;
            self.git.git_state = None;
        }
        if patch.meta.is_some() {
            self.document.meta = patch.meta;
        }
        if patch.viewport.is_some() {
            self.viewport.content = patch.viewport;
        }
        if patch.note.is_some() {
            self.document.note = patch.note;
        }
        if patch.error.is_some() {
            self.document.error = patch.error;
        }
        if patch.scroll_by.is_some() {
            self.viewport.scroll_by = patch.scroll_by;
        }
        if patch.directory.is_some() {
            self.document.error = None;
            self.document.media = None;
            self.document.preview = None;
            self.document.directory = patch.directory;
        }
        if let Some(breadcrumb) = patch.breadcrumb {
            self.document.breadcrumb = breadcrumb;
        }
        if patch.theme.is_some() {
            self.document.theme = patch.theme;
        }
        if patch.preview.is_some() {
            self.document.preview = patch.preview;
        }
        if patch.media.is_some() {
            self.document.error = None;
            self.document.directory = None;
            self.document.preview = None;
            self.document.media = patch.media;
        }
        if patch.cursor.is_some() {
            self.viewport.cursor = patch.cursor;
        }
        if patch.dirty.is_some() {
            self.document.dirty = patch.dirty;
        }
        if patch.view_mode.is_some() {
            self.document.view_mode = patch.view_mode;
        }
        if patch.keymap.is_some() {
            self.viewport.keymap = patch.keymap;
        }
        if patch.shape.is_some() {
            self.document.shape = patch.shape;
        }
        if patch.encoding.is_some() {
            self.document.encoding = patch.encoding;
        }
        if let Some(tidy) = patch.tidy {
            self.explorer.tidy = tidy;
        }
        if patch.explorer_tree.is_some() {
            self.explorer.explorer_tree = patch.explorer_tree;
        }
        if patch.explorer_focus.is_some() {
            self.explorer.explorer_focus = patch.explorer_focus;
        }
        if let Some(notice) = patch.explorer_notice {
            self.explorer.explorer_notice = notice;
        }
        if patch.open_editors.is_some() {
            self.explorer.open_editors = patch.open_editors;
        }
        if patch.outline.is_some() {
            self.explorer.outline = patch.outline;
        }
        if patch.explorer_panel.is_some() {
            self.explorer.explorer_panel = patch.explorer_panel;
        }
        if let Some(search) = patch.explorer_search {
            self.explorer.explorer_search = search;
        }
        if let Some(prompt) = patch.explorer_prompt {
            self.explorer.explorer_prompt = prompt;
        }
        if patch.diagnostics.is_some() {
            self.language.diagnostics = patch.diagnostics;
        }
        if patch.lsp_status.is_some() {
            self.language.lsp_status = patch.lsp_status;
        }
        if let Some(notice) = patch.lsp_install_notice {
            self.language.lsp_install_notice = notice;
        }
        if let Some(hover) = patch.hover {
            self.language.hover = hover;
        }
        if patch.code_actions.is_some() {
            self.language.code_actions = patch.code_actions;
        }
        if let Some(notice) = patch.edit_notice {
            self.document.edit_notice = notice;
        }
        if let Some(rename) = patch.rename {
            self.language.rename = rename;
        }
        if patch.panel.is_some() {
            self.panel.panel = patch.panel;
        }
        if patch.git_state.is_some() {
            self.git.git_state = patch.git_state;
        }
        if patch.find.is_some() {
            self.viewport.find = patch.find;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::FileDocumentKind;

    #[test]
    fn patches_build_a_retained_tree() {
        let event = <FileUiState as vmux_api::UiState>::from_updates(
            None,
            vec![
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
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<FileUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(decoded.document.meta.is_some());
        assert!(decoded.document.dirty.unwrap().dirty);
    }
}
