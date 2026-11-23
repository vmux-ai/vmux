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
    pub checkout: Option<super::FileCheckoutState>,
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

#[vmux_api::ui_state(Default, patch = FileUiStatePatch, version = 6)]
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
    pub checkout: Option<super::FileCheckoutState>,
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
    fn navigation_seed(&self) -> Option<Self> {
        let mut next = Self::default();
        next.document.theme = self.document.theme.clone();
        next.document.view_mode = self.document.view_mode;
        next.viewport.keymap = self.viewport.keymap;
        next.explorer.explorer_panel = self.explorer.explorer_panel;
        next.explorer.open_editors = self.explorer.open_editors.clone();
        Some(next)
    }

    fn navigation_ready(&self) -> bool {
        if self.document.directory.is_some()
            || self.document.media.is_some()
            || self.document.error.is_some()
        {
            return true;
        }
        let Some(meta) = self.document.meta.as_ref() else {
            return false;
        };
        let note = meta.kind == super::FileDocumentKind::Markdown
            && self
                .document
                .view_mode
                .as_ref()
                .is_none_or(|view| view.mode == super::FileViewMode::Note);
        if note {
            self.document.note.is_some()
        } else {
            self.viewport.content.is_some()
        }
    }

    fn apply(&mut self, mut patch: FileUiStatePatch) {
        if let Some(checkout) = patch.checkout.take() {
            self.document.checkout = Some(checkout);
        }
        if let Some(meta) = patch.meta.as_ref()
            && self
                .document
                .meta
                .as_ref()
                .is_none_or(|current| current.revision != meta.revision)
        {
            self.reset_document(meta);
        }
        self.apply_document(&mut patch);
        self.apply_viewport(&mut patch);
        self.apply_explorer(&mut patch);
        self.apply_language(&mut patch);
        if patch.panel.is_some() {
            self.panel.panel = patch.panel;
        }
        if patch.git_state.is_some() {
            self.git.git_state = patch.git_state;
        }
    }
}

impl FileUiState {
    fn reset_document(&mut self, meta: &FileMetaEvent) {
        if self
            .document
            .checkout
            .as_ref()
            .is_some_and(|checkout| checkout.path != meta.abs_path)
        {
            self.document.checkout = None;
        }
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

    fn apply_document(&mut self, patch: &mut FileUiStatePatch) {
        if let Some(meta) = patch.meta.take() {
            self.document.meta = Some(meta);
        }
        if let Some(note) = patch.note.take() {
            self.document.note = Some(note);
        }
        if let Some(error) = patch.error.take() {
            self.document.error = Some(error);
        }
        if let Some(directory) = patch.directory.take() {
            self.document.error = None;
            self.document.media = None;
            self.document.preview = None;
            self.document.directory = Some(directory);
        }
        if let Some(breadcrumb) = patch.breadcrumb.take() {
            self.document.breadcrumb = breadcrumb;
        }
        if let Some(theme) = patch.theme.take() {
            self.document.theme = Some(theme);
        }
        if let Some(preview) = patch.preview.take() {
            self.document.preview = Some(preview);
        }
        if let Some(media) = patch.media.take() {
            self.document.error = None;
            self.document.directory = None;
            self.document.preview = None;
            self.document.media = Some(media);
        }
        if let Some(dirty) = patch.dirty.take() {
            self.document.dirty = Some(dirty);
        }
        if let Some(view_mode) = patch.view_mode.take() {
            self.document.view_mode = Some(view_mode);
        }
        if let Some(shape) = patch.shape.take() {
            self.document.shape = Some(shape);
        }
        if let Some(encoding) = patch.encoding.take() {
            self.document.encoding = Some(encoding);
        }
        if let Some(notice) = patch.edit_notice.take() {
            self.document.edit_notice = notice;
        }
    }

    fn apply_viewport(&mut self, patch: &mut FileUiStatePatch) {
        if let Some(viewport) = patch.viewport.take() {
            self.viewport.content = Some(viewport);
        }
        if let Some(scroll_by) = patch.scroll_by.take() {
            self.viewport.scroll_by = Some(scroll_by);
        }
        if let Some(cursor) = patch.cursor.take() {
            self.viewport.cursor = Some(cursor);
        }
        if let Some(keymap) = patch.keymap.take() {
            self.viewport.keymap = Some(keymap);
        }
        if let Some(find) = patch.find.take() {
            self.viewport.find = Some(find);
        }
    }

    fn apply_explorer(&mut self, patch: &mut FileUiStatePatch) {
        if let Some(tidy) = patch.tidy.take() {
            self.explorer.tidy = tidy;
        }
        if let Some(tree) = patch.explorer_tree.take() {
            self.explorer.explorer_tree = Some(tree);
        }
        if let Some(focus) = patch.explorer_focus.take() {
            self.explorer.explorer_focus = Some(focus);
        }
        if let Some(notice) = patch.explorer_notice.take() {
            self.explorer.explorer_notice = notice;
        }
        if let Some(editors) = patch.open_editors.take() {
            self.explorer.open_editors = Some(editors);
        }
        if let Some(outline) = patch.outline.take() {
            self.explorer.outline = Some(outline);
        }
        if let Some(panel) = patch.explorer_panel.take() {
            self.explorer.explorer_panel = Some(panel);
        }
        if let Some(search) = patch.explorer_search.take() {
            self.explorer.explorer_search = search;
        }
        if let Some(prompt) = patch.explorer_prompt.take() {
            self.explorer.explorer_prompt = prompt;
        }
    }

    fn apply_language(&mut self, patch: &mut FileUiStatePatch) {
        if let Some(diagnostics) = patch.diagnostics.take() {
            self.language.diagnostics = Some(diagnostics);
        }
        if let Some(status) = patch.lsp_status.take() {
            self.language.lsp_status = Some(status);
        }
        if let Some(notice) = patch.lsp_install_notice.take() {
            self.language.lsp_install_notice = notice;
        }
        if let Some(hover) = patch.hover.take() {
            self.language.hover = hover;
        }
        if let Some(actions) = patch.code_actions.take() {
            self.language.code_actions = Some(actions);
        }
        if let Some(rename) = patch.rename.take() {
            self.language.rename = rename;
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
