#![allow(non_snake_case)]

mod directory;
mod dom;
mod editor;
mod input;
mod menu;
mod note;
mod sidebar;
mod status;
mod text_geometry;
mod toolbar;

use directory::{
    DirColumns, DirWindow, Preview, apply_dir, clear_preview, image_data_url, open_path, parent_of,
    request_preview, scroll_row_into_view, toggle_video, visible_entries,
};
use dom::{EditorDom, ScrolledLineHeight};
use editor::{EditorLines, StickyScope};
use input::{
    CONTAINER_ID, INPUT_ID, PreeditField, focus_container, forward_file_key, send_committed_text,
};
pub(crate) use input::{FIND_INPUT_ID, focus_file_input, focus_find_input};
use menu::{CodeActionMenu, EditorContextMenu, ReferencesPanel, RenameBox, RenameInput};
use note::{NoteBlankLine, NoteBlockView, NoteBlocks, NoteCursor, NoteProperties};
pub(crate) use sidebar::ExplorerPane;
use sidebar::{ExplorerSidebar, ExplorerToggleButton, PaneWidth};
use status::{EncodingRecovery, FileStatusInfo, FileStatusScope};
use std::collections::HashMap;
use text_geometry::RowRuler;
use toolbar::{EditorTabStrip, FindBar, VimStatus};

use crate::breadcrumb::EditorBreadcrumbs;
use crate::explorer::{EditorTabCommand, SidebarView};
use crate::page_key::{Completions, FilePage, use_file_keys};
use crate::page_model::{
    CellMetrics, ColumnRuler, EditorTabItem, NoteCursorActivation, clamp_selection,
    editor_drag_started, gutter_width, note_cursor_activation, severity_color_class, span_style,
};
use crate::state::use_file_ui;
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_core::knowledge::{KnowledgeProperty, KnowledgeReference};
use vmux_core::media::MediaKind;
use vmux_git::event::{
    GitChangedEvent, GitDiffViewportEvent, GitErrorEvent, GitResultEvent, GitStatusEvent,
};
use vmux_git::ui::EditorDiffMarker;
use vmux_git::ui::{DiffView, GitFooter, GitStatusFeed};
use vmux_ui::diff::DiffTone;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{PressedKey, send, use_theme, use_ui_state_root};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::ime::use_ime_guard;
use vmux_ui::platform::sleep_ms;

#[component]
pub fn Page() -> Element {
    use_theme();
    use_ui_state_root::<FileUiState>();
    let git_status = use_file_ui::<GitStatusEvent>();
    let mut path = use_signal(String::new);
    let mut document_revision = use_signal(|| 0u64);
    let mut document_kind = use_signal(FileDocumentKind::default);
    let mut total_lines = use_signal(|| 0u32);
    let mut total_rows = use_signal(|| 0u32);
    let mut first_row = use_signal(|| 0u32);
    let mut gutter_hover = use_signal(|| false);
    let mut language = use_signal(String::new);
    let mut indent = use_signal(vmux_core::event::FileIndent::default);
    let mut line_ending = use_signal(vmux_core::event::FileLineEnding::default);
    let mut encoding = use_signal(vmux_core::event::FileEncoding::default);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut sticky_lines = use_signal(Vec::<FileLine>::new);
    let mut outline = use_signal(Vec::<OutlineRow>::new);
    let mut line_layouts = use_signal(Vec::<FileLineLayout>::new);
    let mut wrap_columns = use_signal(|| 0u16);
    let mut diagnostics = use_signal(Vec::<FileDiagnostic>::new);
    let mut hover_diag = use_signal(|| Option::<FileDiagnostic>::None);
    let mut lsp_status = use_signal(|| Option::<FileLspStatusEvent>::None);
    let mut lsp_actions = use_signal(Vec::<EditorAction>::new);
    let mut lsp_install_notice = use_signal(|| Option::<LspInstallProgress>::None);
    let mut lsp_install_request = use_signal(|| Option::<(String, String)>::None);
    let mut lsp_notice_generation = use_signal(|| 0u32);
    let mut code_actions = use_signal(Vec::<String>::new);
    let mut code_action_sel = use_signal(|| 0usize);
    let mut rename_box = use_signal(|| Option::<RenameBox>::None);
    let mut rename_failed = use_signal(String::new);
    let mut rename_failed_generation = use_signal(|| 0u32);
    let mut error = use_signal(String::new);
    let mut error_undecodable = use_signal(|| false);
    let dir_entries = use_signal(Vec::<FileDirEntry>::new);
    let parent_entries = use_signal(Vec::<FileDirEntry>::new);
    let mut parent_path = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut came_from = use_signal(String::new);
    let mut back_dir = use_signal(|| Option::<String>::None);
    let mut show_hidden = use_signal(|| true);
    let mut mode = use_signal(|| Mode::Text);
    let mut media = use_signal(|| Option::<FileMediaEvent>::None);
    let mut preview = use_signal(|| Preview::None);
    let mut thumbs = use_signal(HashMap::<String, String>::new);
    let mut theme_style = use_signal(String::new);
    let mut cell_dims = use_signal(CellMetrics::default);
    let dom = EditorDom::new();
    let page_width = use_signal(|| 0u32);
    let last_resize = use_signal(FileResizeEvent::default);
    let mut git_path = use_signal(String::new);
    let mut git_has_diff = use_signal(|| false);
    let mut git_line_markers = use_signal(HashMap::<u32, EditorDiffMarker>::new);
    let mut file_view_mode = use_signal(|| FileViewMode::Note);
    let mut note_blocks = use_signal(Vec::<NoteBlock>::new);
    let mut note_properties = use_signal(Vec::<KnowledgeProperty>::new);
    let mut note_references = use_signal(Vec::<KnowledgeReference>::new);
    let note_cursor = NoteCursor::new();
    let mut note_dragging = use_signal(|| false);
    let mut editor_dragging = use_signal(|| false);
    let mut editor_drag_origin = use_signal(|| Option::<(i32, i32)>::None);
    let mut git_nonce = use_signal(|| 0u32);
    let git_repo_root = use_signal(String::new);
    let git_refresh_generation = use_signal(|| 0u32);
    let git_refresh_settled = use_signal(|| true);
    let git_branch = use_signal(String::new);
    let git_ahead = use_signal(|| 0u32);
    let git_behind = use_signal(|| 0u32);
    let git_staged = use_signal(|| 0u32);
    let git_message = use_signal(String::new);
    let mut git_result = use_signal(|| None::<GitResultEvent>);
    let mut git_diff_viewport = use_signal(|| None::<GitDiffViewportEvent>);
    let git_feed = GitStatusFeed {
        path: git_path.into(),
        nonce: git_nonce,
        repo_root: git_repo_root,
        has_diff: git_has_diff,
        branch: git_branch,
        ahead: git_ahead,
        behind: git_behind,
        staged_count: git_staged,
        message: git_message,
    };
    git_feed.request();
    let mut ed_mode = use_signal(|| vmux_core::editor::EditMode::Insert);
    let mut ed_label = use_signal(String::new);
    let mut search_spans = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut word_spans = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let find_open = use_signal(|| false);
    let find_forward = use_signal(|| true);
    let sidebar_view = use_signal(SidebarView::default);
    let find_query = use_signal(String::new);
    let mut find_total = use_signal(|| 0u32);
    let mut find_index = use_signal(|| 0u32);
    let mut keymap = use_signal(vmux_core::KeymapKind::default);
    let mut cursor = use_signal(vmux_core::editor::CursorPos::default);
    let mut carets = use_signal(Vec::<vmux_core::editor::CursorPos>::new);
    let mut sel = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut source_cursor = use_signal(vmux_core::editor::CursorPos::default);
    let mut source_sel = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut open_editors = use_signal(Vec::<OpenEditorItem>::new);
    let ime = use_ime_guard();
    let typed = use_signal(String::new);
    let mut lsp_hover = use_signal(|| Option::<FileHoverEvent>::None);
    let mut hover_pos = use_signal(|| Option::<(u32, u32)>::None);
    let ctx_menu = use_signal(|| Option::<(f64, f64, u32, u32)>::None);
    let mut refs = use_signal(Vec::<RefItem>::new);
    let mut refs_sel = use_signal(|| 0usize);
    let mut refs_open = use_signal(|| false);
    let mut comps = use_signal(Vec::<CompletionItem>::new);
    let mut comp_open = use_signal(|| false);
    let mut comp_sel = use_signal(|| 0usize);
    let mut comp_anchor = use_signal(|| (0u32, 0u32));
    let mut last_scroll_req = use_signal(|| 0u32);
    let explorer = ExplorerPane::new(page_width);
    let mut tidy_prompt = use_signal(|| Option::<u32>::None);
    let mut doc_title = use_signal(String::new);
    let is_markdown = use_memo(move || document_kind() == FileDocumentKind::Markdown);

    let completions = Completions {
        open: comp_open,
        anchor: comp_anchor,
        items: comps,
        lines,
        cursor,
    };
    let comp_filtered = use_memo(move || completions.matching());
    let file_page = FilePage {
        mode,
        explorer,
        completion_open: comp_open,
        completion_selection: comp_sel,
        completion_anchor: comp_anchor,
        completions: comp_filtered,
        references_open: refs_open,
        reference_selection: refs_sel,
        references: refs,
        find_open,
        find_forward,
        sidebar_view,
    };
    let keys = use_file_keys(file_page);
    use_context_provider(|| keys);

    let explorer_panel = use_file_ui::<ExplorerPanelEvent>();
    use_effect(move || {
        explorer_panel.for_each(|event| {
            explorer.apply_panel(event);
        })
    });

    let tidy_prompt_event = use_file_ui::<FileTidyPromptEvent>();
    use_effect(move || {
        tidy_prompt_event.for_each(|e| {
            tidy_prompt.set(Some(e.count));
        })
    });

    let file_meta = use_file_ui::<FileMetaEvent>();
    use_effect(move || {
        file_meta.for_each(|m| {
            let reset_view = *document_revision.peek() != m.revision;
            document_revision.set(m.revision);
            document_kind.set(m.kind);
            doc_title.set(m.path.rsplit('/').next().unwrap_or(&m.path).to_string());
            path.set(m.path);
            git_path.set(m.abs_path);
            total_lines.set(m.total_lines);
            language.set(m.language);
            indent.set(m.indent);
            line_ending.set(m.line_ending);
            encoding.set(m.encoding);
            mode.set(Mode::Text);
            if !reset_view {
                return;
            }
            error.set(String::new());
            clear_preview(preview, thumbs);
            media.set(None);
            dom.reset();
            last_scroll_req.set(0);
            let _ = send(&FileScrollEvent {
                top_row: 0,
                needs_rows: true,
            });
            diagnostics.set(Vec::new());
            hover_diag.set(None);
            lsp_status.set(None);
            git_has_diff.set(false);
            git_line_markers.set(HashMap::new());
            lsp_install_notice.set(None);
            lsp_install_request.set(None);
            lsp_notice_generation.set(lsp_notice_generation().wrapping_add(1));
            explorer.show_if_room(mode);
            note_blocks.set(Vec::new());
            note_properties.set(Vec::new());
            note_references.set(Vec::new());
            note_cursor.reset();
            note_dragging.set(false);
            editor_dragging.set(false);
            editor_drag_origin.set(None);
            git_nonce.set(git_nonce() + 1);
        })
    });

    let file_shape = use_file_ui::<FileShapeEvent>();
    use_effect(move || {
        file_shape.for_each(|s| {
            indent.set(s.indent);
            line_ending.set(s.line_ending);
        })
    });

    let file_encoding = use_file_ui::<FileEncodingEvent>();
    use_effect(move || {
        file_encoding.for_each(|e| {
            encoding.set(e.encoding);
        })
    });

    let file_viewport = use_file_ui::<FileViewportPatch>();
    use_effect(move || {
        file_viewport.for_each(|p| {
            first_row.set(p.first_row);
            total_rows.set(p.total_rows);
            total_lines.set(p.total_lines);
            wrap_columns.set(p.wrap_columns);
            if line_layouts.peek().as_slice() != p.layouts.as_slice() {
                line_layouts.set(p.layouts);
            }
            if lines.peek().as_slice() != p.lines.as_slice() {
                lines.set(p.lines);
            }
            if sticky_lines.peek().as_slice() != p.sticky.as_slice() {
                sticky_lines.set(p.sticky);
            }
            lsp_hover.set(None);
        })
    });

    let outline_event = use_file_ui::<OutlineEvent>();
    use_effect(move || {
        outline_event.for_each(|e| {
            outline.set(e.items);
        })
    });

    let file_cursor = use_file_ui::<FileCursorEvent>();
    use_effect(move || {
        file_cursor.for_each(|c| {
            let moved = cursor.peek().ne(&c.primary);
            if *ed_mode.peek() != c.mode {
                ed_mode.set(c.mode);
            }
            if ed_label.peek().ne(&c.mode_label) {
                ed_label.set(c.mode_label.clone());
            }
            if moved {
                cursor.set(c.primary);
            }
            if carets.peek().as_slice() != c.carets.as_slice() {
                carets.set(c.carets.clone());
            }
            if sel.peek().as_slice() != c.selections.as_slice() {
                sel.set(c.selections.clone());
            }
            if source_cursor.peek().ne(&c.source_primary) {
                source_cursor.set(c.source_primary);
            }
            if source_sel.peek().as_slice() != c.source_selections.as_slice() {
                source_sel.set(c.source_selections.clone());
            }
            if search_spans.peek().as_slice() != c.search.as_slice() {
                search_spans.set(c.search.clone());
            }
            if word_spans.peek().as_slice() != c.word_highlights.as_slice() {
                word_spans.set(c.word_highlights.clone());
            }
            if *find_total.peek() != c.search_total {
                find_total.set(c.search_total);
            }
            if *find_index.peek() != c.search_index {
                find_index.set(c.search_index);
            }
            let note_mode = *file_view_mode.peek() == FileViewMode::Note && *is_markdown.peek();
            if note_mode {
                let active = note_blocks
                    .peek()
                    .as_slice()
                    .block_index_for_line(c.source_primary.line);
                if *keymap.peek() == vmux_core::KeymapKind::Vim
                    && !note_cursor.editing()
                    && let Some(index) = active
                {
                    note_cursor.activate(index, c.source_primary.line);
                }
                if note_cursor.editing() {
                    let is_list = active.is_some_and(|index| {
                        matches!(note_blocks.peek()[index].block, MdBlock::List { .. })
                    });
                    let edit_line = is_list.then_some(c.source_primary.line);
                    if note_cursor.edit_line() != edit_line {
                        note_cursor.set_edit_line(edit_line);
                    }
                }
                let active = active.map(|index| index as u32);
                if note_cursor.active() != active {
                    note_cursor.set_active(active);
                }
                if moved && let Some(index) = active {
                    note_cursor.reveal(index as usize, c.source_primary.line);
                }
            }
            if moved && !note_mode {
                dom.reveal_caret();
            }
        })
    });

    let scroll_by = use_file_ui::<FileScrollByEvent>();
    use_effect(move || {
        scroll_by.for_each(|event| {
            let Some(line_height) =
                ScrolledLineHeight::resolve(file_view_mode(), document_kind(), cell_dims().height)
            else {
                return;
            };
            dom.scroll_by(event.lines, line_height);
        })
    });

    let open_editors_event = use_file_ui::<OpenEditorsEvent>();
    use_effect(move || {
        open_editors_event.for_each(|event| {
            open_editors.set(event.items);
        })
    });

    let file_dirty = use_file_ui::<FileDirtyEvent>();
    use_effect(move || {
        file_dirty.for_each(|_| {
            GitRefresh {
                generation: git_refresh_generation,
                nonce: git_nonce,
                settled: git_refresh_settled,
            }
            .schedule();
        })
    });

    use_effect(move || {
        git_status.for_each(|status| {
            git_feed.apply_status(status);
        })
    });

    let git_result_event = use_file_ui::<GitResultEvent>();
    use_effect(move || {
        git_result_event.for_each(|event| {
            git_feed.apply_result(event.clone());
            git_result.set(Some(event));
        })
    });

    let git_error = use_file_ui::<GitErrorEvent>();
    use_effect(move || {
        git_error.for_each(|event| {
            git_feed.apply_error(event);
        })
    });

    let git_changed = use_file_ui::<GitChangedEvent>();
    use_effect(move || {
        git_changed.for_each(|_| {
            GitRefresh {
                generation: git_refresh_generation,
                nonce: git_nonce,
                settled: git_refresh_settled,
            }
            .schedule();
        })
    });

    let git_diff_viewport_event = use_file_ui::<GitDiffViewportEvent>();
    use_effect(move || {
        git_diff_viewport_event.for_each(|event| {
            git_diff_viewport.set(Some(event));
        })
    });

    let view_mode_event = use_file_ui::<FileViewModeEvent>();
    use_effect(move || {
        view_mode_event.for_each(|event| {
            if file_view_mode() != event.mode && event.mode != FileViewMode::Note {
                note_cursor.set_editing(false);
            }
            file_view_mode.set(event.mode);
            match event.mode {
                FileViewMode::Note if is_markdown() => {
                    let line = source_cursor().line;
                    if let Some(index) = note_blocks.read().as_slice().block_index_for_line(line) {
                        note_cursor.activate_centered(index, line);
                    }
                }
                FileViewMode::Editor => {
                    dom.center_row(cursor().row, cell_dims().height);
                }
                _ => {}
            }
        })
    });

    let keymap_event = use_file_ui::<FileKeymapEvent>();
    use_effect(move || {
        keymap_event.for_each(|event| {
            keymap.set(event.keymap);
            if event.keymap == vmux_core::KeymapKind::Vim
                && file_view_mode() == FileViewMode::Note
                && is_markdown()
            {
                let line = source_cursor().line;
                if let Some(index) = note_blocks.read().as_slice().block_index_for_line(line) {
                    note_cursor.activate_centered(index, line);
                }
            }
        })
    });

    let note_event = use_file_ui::<FileNoteEvent>();
    use_effect(move || {
        note_event.for_each(|event| {
            let FileNoteEvent {
                title,
                properties,
                blocks,
                active,
                references,
                reveal_line,
            } = event;
            let title = if title.is_empty() {
                path().rsplit('/').next().unwrap_or_default().to_string()
            } else {
                title
            };
            doc_title.set(title.clone());
            let activation = note_cursor_activation(
                reveal_line,
                keymap() == vmux_core::KeymapKind::Vim && file_view_mode() == FileViewMode::Note,
                source_cursor().line,
            );
            let activation = activation.and_then(|activation| {
                let line = match activation {
                    NoteCursorActivation::Center(line)
                    | NoteCursorActivation::PreserveViewport(line) => line,
                };
                blocks
                    .as_slice()
                    .block_index_for_line(line)
                    .map(|index| (activation, index, line))
            });
            note_blocks.set(blocks);
            note_properties.set(properties);
            note_references.set(references);
            note_cursor.set_active(active);
            if let Some((activation, index, line)) = activation {
                match activation {
                    NoteCursorActivation::Center(_) => note_cursor.activate_centered(index, line),
                    NoteCursorActivation::PreserveViewport(_) => note_cursor.activate(index, line),
                }
            }
        })
    });

    let hover_event = use_file_ui::<FileHoverEvent>();
    use_effect(move || {
        hover_event.for_each(|h| {
            lsp_hover.set(Some(h));
        })
    });

    let references_event = use_file_ui::<FileReferencesEvent>();
    use_effect(move || {
        references_event.for_each(|e| {
            refs.set(e.items);
            refs_sel.set(0);
            refs_open.set(true);
            FocusClaim::new("refs-panel").request();
        })
    });

    let completion_event = use_file_ui::<FileCompletionEvent>();
    use_effect(move || {
        completion_event.for_each(|e| {
            comp_open.set(!e.items.is_empty());
            comps.set(e.items);
            comp_sel.set(0);
            comp_anchor.set((e.line, e.replace_from_col));
        })
    });

    let diagnostics_event = use_file_ui::<FileDiagnosticsEvent>();
    use_effect(move || {
        diagnostics_event.for_each(|d| {
            if d.path != git_path() {
                return;
            }
            diagnostics.set(d.diagnostics);
        })
    });

    let lsp_status_event = use_file_ui::<FileLspStatusEvent>();
    use_effect(move || {
        lsp_status_event.for_each(|s| {
            if s.path != git_path() {
                return;
            }
            if s.state == LspServerState::Missing
                && let Some(package) = s.package.clone()
            {
                let request = (s.path.clone(), package.clone());
                if lsp_install_request() != Some(request.clone()) {
                    lsp_notice_generation.set(lsp_notice_generation().wrapping_add(1));
                    lsp_install_request.set(Some(request));
                    lsp_install_notice.set(Some(LspInstallProgress {
                        name: package.clone(),
                        phase: InstallPhase::Resolving,
                        pct: None,
                        message: translate("lsp-status-installing"),
                    }));
                    let _ = send(&LspInstallRequest { name: package });
                }
            }
            lsp_actions.set(s.actions.clone());
            lsp_status.set(Some(s));
        })
    });

    let install_progress = use_file_ui::<LspInstallProgress>();
    use_effect(move || {
        install_progress.for_each(|progress| {
            let active = lsp_install_request().is_some_and(|(_, package)| package == progress.name);
            if !active {
                return;
            }
            let delay = match progress.phase {
                InstallPhase::Done => Some(LSP_NOTICE_DONE_MS),
                InstallPhase::Failed => Some(LSP_NOTICE_FAILED_MS),
                _ => None,
            };
            lsp_install_notice.set(Some(progress));
            if let Some(delay) = delay {
                schedule_lsp_notice_clear(
                    lsp_install_notice,
                    lsp_install_request,
                    lsp_notice_generation,
                    delay,
                );
            }
        })
    });

    let package_status = use_file_ui::<LspPkgStatusEvent>();
    use_effect(move || {
        package_status.for_each(|status| {
            if status.status != LspPkgStatus::Installed
                || lsp_install_request().is_none_or(|(_, package)| package != status.name)
            {
                return;
            }
            lsp_install_notice.set(Some(LspInstallProgress {
                name: status.name,
                phase: InstallPhase::Done,
                pct: Some(100),
                message: translate("lsp-status-installed"),
            }));
            schedule_lsp_notice_clear(
                lsp_install_notice,
                lsp_install_request,
                lsp_notice_generation,
                LSP_NOTICE_DONE_MS,
            );
        })
    });

    let file_error = use_file_ui::<FileErrorEvent>();
    use_effect(move || {
        file_error.for_each(|e| {
            error_undecodable.set(e.undecodable);
            error.set(e.message);
        })
    });

    let code_actions_event = use_file_ui::<FileCodeActionsEvent>();
    use_effect(move || {
        code_actions_event.for_each(|e| {
            code_action_sel.set(0);
            code_actions.set(e.titles);
        })
    });

    let rename_event = use_file_ui::<FileRenameBeginEvent>();
    use_effect(move || {
        rename_event.for_each(|e| {
            rename_failed.set(String::new());
            rename_box.set(Some(RenameBox::new(e.line, e.col, e.current)));
        })
    });

    let edit_failed = use_file_ui::<FileEditFailedEvent>();
    use_effect(move || {
        edit_failed.for_each(|e| {
            rename_failed.set(e.reason);
            let id = rename_failed_generation().wrapping_add(1);
            rename_failed_generation.set(id);
            spawn(async move {
                sleep_ms(RENAME_NOTICE_MS).await;
                if rename_failed_generation() == id {
                    rename_failed.set(String::new());
                }
            });
        })
    });

    let directory_event = use_file_ui::<FileDirEvent>();
    use_effect(move || {
        directory_event.for_each(|d| {
            error.set(String::new());
            clear_preview(preview, thumbs);
            media.set(None);
            doc_title.set(
                d.path
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&d.path)
                    .to_string(),
            );
            parent_path.set(d.parent_path);
            if git_path() != d.abs_path {
                git_has_diff.set(false);
                git_line_markers.set(HashMap::new());
            }
            git_path.set(d.abs_path);
            git_nonce.set(git_nonce() + 1);
            mode.set(Mode::Dir);
            comp_open.set(false);
            comps.set(Vec::new());
            refs_open.set(false);
            refs.set(Vec::new());
            diagnostics.set(Vec::new());
            hover_diag.set(None);
            lsp_status.set(None);
            let came = came_from();
            came_from.set(String::new());
            apply_dir(
                dir_entries,
                parent_entries,
                path,
                selected,
                preview,
                thumbs,
                show_hidden(),
                d.entries,
                d.parent_entries,
                d.path,
                (!came.is_empty()).then_some(came),
            );
        })
    });

    let media_event = use_file_ui::<FileMediaEvent>();
    use_effect(move || {
        media_event.for_each(|e| {
            error.set(String::new());
            clear_preview(preview, thumbs);
            let kind = e.kind;
            media.set(Some(e));
            mode.set(Mode::Media(kind));
            diagnostics.set(Vec::new());
            hover_diag.set(None);
            lsp_status.set(None);
        })
    });

    let preview_event = use_file_ui::<FilePreviewEvent>();
    use_effect(move || {
        preview_event.for_each(|ev| {
            if ev.thumb {
                if let PreviewKind::Image { bytes, .. } = ev.kind {
                    let url = image_data_url(&bytes, &ev.path);
                    thumbs.write().insert(ev.path.clone(), url);
                }
                return;
            }
            let vis = visible_entries(&dir_entries.read(), show_hidden());
            let sel_path = vis.get(selected()).map(|e| e.path.clone());
            if sel_path.as_deref() != Some(ev.path.as_str()) {
                return;
            }
            let next = match ev.kind {
                PreviewKind::Image { bytes, .. } => {
                    Preview::Image(image_data_url(&bytes, &ev.path))
                }
                PreviewKind::Video { url, path, native } => Preview::Video { url, path, native },
                PreviewKind::Text(l) => Preview::Text(l),
                PreviewKind::Dir(e) => Preview::Dir(e),
                PreviewKind::Info {
                    size,
                    modified,
                    kind,
                } => Preview::Info {
                    size,
                    modified,
                    kind,
                },
                PreviewKind::Error(m) => Preview::Error(m),
            };
            preview.set(next);
        })
    });

    let theme_event = use_file_ui::<FileThemeEvent>();
    use_effect(move || {
        theme_event.for_each(|t| {
            let mut s = String::new();
            if !t.font_family.is_empty() {
                s.push_str(&format!(
                    "font-family:\"{}\",var(--font-mono);",
                    t.font_family
                ));
            }
            if t.font_size > 0.0 {
                s.push_str(&format!("font-size:{}px;", t.font_size));
            }
            if t.line_height > 0.0 {
                s.push_str(&format!("line-height:{};", t.line_height));
            }
            theme_style.set(s);
        })
    });

    use_effect(move || {
        explorer.sync();
        dom.announce(cell_dims(), total_lines(), last_resize);
    });

    use_effect(move || match mode() {
        Mode::Text if file_view_mode() == FileViewMode::Note && is_markdown() => {
            if note_cursor.editing() {
                focus_file_input();
            } else {
                focus_container();
            }
        }
        Mode::Text => focus_file_input(),
        Mode::Dir | Mode::Media(_) => focus_container(),
    });

    let gw = gutter_width(total_lines());
    let editor_tabs = EditorTabItem::all(&open_editors.read());
    let breadcrumb_path = match error().is_empty() {
        true => git_path(),
        false => String::new(),
    };
    let breadcrumb_outline = match mode() {
        Mode::Text => outline(),
        Mode::Dir | Mode::Media(_) => Vec::new(),
    };
    let breadcrumb_caret_line = match file_view_mode() == FileViewMode::Note && is_markdown() {
        true => source_cursor().line,
        false => cursor().line,
    };
    let status_scope =
        FileStatusScope::new(mode(), file_view_mode(), is_markdown(), git_has_diff());
    let status_caret = match status_scope.reading_note() {
        true => source_cursor(),
        false => cursor(),
    };
    let measure_text = vec!["X".repeat(MEASURE_COLS); MEASURE_ROWS].join("\n");
    let measure_wide_text = MEASURE_WIDE_GLYPH.repeat(MEASURE_COLS);
    let comp_filtered: Vec<CompletionItem> = comp_filtered();
    let comp_sel_clamped = comp_sel().min(comp_filtered.len().saturating_sub(1));

    rsx! {
        if !doc_title().is_empty() {
        }
        div {
            id: PAGE_ID,
            class: "relative flex h-full w-full flex-col overflow-hidden bg-background",
            onmousemove: move |e: Event<MouseData>| {
                explorer.resize_to(e.client_coordinates().x);
            },
            onmouseup: move |_| {
                note_dragging.set(false);
                editor_dragging.set(false);
                editor_drag_origin.set(None);
                explorer.finish_resize();
            },

            PaneWidth { width: page_width }

        div {
            class: "flex min-h-0 flex-1 flex-row overflow-hidden",

            ExplorerSidebar {
                pane: explorer,
                caret_line: breadcrumb_caret_line,
                view: sidebar_view,
            }

        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative flex h-full min-w-[320px] flex-1 flex-col overflow-hidden bg-background bg-[radial-gradient(120%_80%_at_50%_-10%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_60%)] text-foreground font-mono text-sm leading-normal outline-none",
            style: "--iw:{indent().width};{cell_dims().vars()}{theme_style}",

            onmousedown: move |e: Event<MouseData>| {
                match mode() {
                    Mode::Text => {
                        e.prevent_default();
                        if file_view_mode() == FileViewMode::Note
                            && is_markdown()
                        {
                            if note_cursor.editing() {
                                focus_file_input();
                            } else {
                                focus_container();
                            }
                        } else {
                            focus_file_input();
                        }
                    }
                    Mode::Dir => {
                        e.prevent_default();
                        focus_container();
                    }
                    Mode::Media(_) => focus_container(),
                }
            },

            onkeydown: move |e: Event<KeyboardData>| {
                let current_mode = mode();
                if current_mode != Mode::Dir && keys.offer(&e) {
                    return;
                }
                let key = e.key().to_string();
                if current_mode == Mode::Text
                    && file_view_mode() == FileViewMode::Note
                    && is_markdown()
                    && !note_cursor.editing()
                {
                    let _ = forward_file_key(&e, ed_mode());
                    return;
                }
                match current_mode {
                    Mode::Dir => {
                        let vis = visible_entries(&dir_entries.read(), show_hidden());
                        let len = vis.len();
                        let cur = selected();
                        match key.as_str() {
                            "j" | "ArrowDown" => {
                                e.prevent_default();
                                let next = if len == 0 { 0 } else { (cur + 1).min(len - 1) };
                                selected.set(next);
                                scroll_row_into_view(next);
                                if let Some(p) = vis.get(next).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            "k" | "ArrowUp" => {
                                e.prevent_default();
                                let next = cur.saturating_sub(1);
                                selected.set(next);
                                scroll_row_into_view(next);
                                if let Some(p) = vis.get(next).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            "l" | "ArrowRight" | "Enter" => {
                                e.prevent_default();
                                let Some(ent) = vis.get(cur).cloned() else {
                                    return;
                                };
                                if ent.is_dir {
                                    let children = match &*preview.read() {
                                        Preview::Dir(c) => Some(c.clone()),
                                        _ => None,
                                    };
                                    if let Some(children) = children {
                                        let cur_entries = dir_entries.read().clone();
                                        parent_path.set(parent_of(&ent.path));
                                        apply_dir(
                                            dir_entries,
                                            parent_entries,
                                            path,
                                            selected,
                                            preview,
                                            thumbs,
                                            show_hidden(),
                                            children,
                                            cur_entries,
                                            ent.path.clone(),
                                            None,
                                        );
                                    }
                                    open_path(ent.path);
                                } else {
                                    back_dir.set(Some(parent_of(&ent.path)));
                                    open_path(ent.path);
                                }
                            }
                            "Escape" => {
                                e.prevent_default();
                                EditorTabCommand { path: git_path() }.close();
                            }
                            "h" | "ArrowLeft" => {
                                let pp = parent_path();
                                if !pp.is_empty() {
                                    e.prevent_default();
                                    let came = path();
                                    came_from.set(came.clone());
                                    let pe = parent_entries.read().clone();
                                    if !pe.is_empty() {
                                        parent_path.set(parent_of(&pp));
                                        apply_dir(
                                            dir_entries,
                                            parent_entries,
                                            path,
                                            selected,
                                            preview,
                                            thumbs,
                                            show_hidden(),
                                            pe,
                                            Vec::new(),
                                            pp.clone(),
                                            Some(came),
                                        );
                                    }
                                    open_path(pp);
                                }
                            }
                            "." => {
                                e.prevent_default();
                                let next = !show_hidden();
                                show_hidden.set(next);
                                let vis2 = visible_entries(&dir_entries.read(), next);
                                let idx = clamp_selection(cur, vis2.len());
                                selected.set(idx);
                                scroll_row_into_view(idx);
                                if let Some(p) = vis2.get(idx).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            " " => {
                                e.prevent_default();
                                toggle_video();
                            }
                            _ => {
                                keys.offer(&e);
                            }
                        }
                    }
                    _ => {
                        if matches!(key.as_str(), "Escape" | "h")
                            && let Some(d) = back_dir()
                        {
                            e.prevent_default();
                            open_path(d);
                        }
                    }
                }
            },

            span {
                id: MEASURE_ID,
                class: "invisible absolute left-0 top-0 whitespace-pre [font:inherit]",
                onresize: move |event: Event<ResizeData>| {
                    let Ok(size) = event.get_border_box_size() else {
                        return;
                    };
                    let measured = CellMetrics {
                        narrow: size.width / MEASURE_COLS as f64,
                        wide: cell_dims.peek().wide,
                        height: size.height / MEASURE_ROWS as f64,
                    };
                    if measured != *cell_dims.peek() {
                        cell_dims.set(measured);
                    }
                },
                {measure_text}
            }

            span {
                id: MEASURE_WIDE_ID,
                class: "invisible absolute left-0 top-0 whitespace-pre [font:inherit]",
                onresize: move |event: Event<ResizeData>| {
                    let Ok(size) = event.get_border_box_size() else {
                        return;
                    };
                    let measured = CellMetrics {
                        wide: size.width / MEASURE_COLS as f64,
                        ..*cell_dims.peek()
                    };
                    if measured != *cell_dims.peek() {
                        cell_dims.set(measured);
                    }
                },
                {measure_wide_text}
            }

            div {
                class: "flex h-9 shrink-0 items-center gap-2 border-b border-foreground/[0.07] bg-foreground/[0.06] px-4 font-sans text-xs text-muted-foreground",
                ExplorerToggleButton { pane: explorer, mode }
                EditorTabStrip { tabs: editor_tabs }
                if find_open() {
                    FindBar {
                        query: find_query,
                        open: find_open,
                        forward: find_forward,
                        vim: keymap() == vmux_core::KeymapKind::Vim,
                        total: find_total(),
                        index: find_index(),
                    }
                }
                if mode() == Mode::Text {
                    if is_markdown() || git_has_diff() {
                        div { class: "flex shrink-0 items-center gap-0.5 rounded-md bg-foreground/[0.06] p-0.5 text-[10px] font-medium ring-1 ring-inset ring-foreground/10",
                            if is_markdown() {
                                button {
                                    class: file_mode_class(file_view_mode() == FileViewMode::Note),
                                    title: translate("editor-rendered-markdown"),
                                    onclick: move |_| {
                                        file_view_mode.set(FileViewMode::Note);
                                        let _ = send(&FileViewModeSet { mode: FileViewMode::Note });
                                        let line = source_cursor().line;
                                        if let Some(index) = note_blocks.read().as_slice().block_index_for_line(line) {
                                            note_cursor.activate_centered(index, line);
                                        }
                                    },
                                    {translate("editor-note")}
                                }
                            }
                            button {
                                class: file_mode_class(
                                    file_view_mode() == FileViewMode::Editor
                                        || (file_view_mode() == FileViewMode::Note
                                            && !is_markdown()),
                                ),
                                title: translate("editor-source-editor"),
                                onclick: move |_| {
                                    note_cursor.set_editing(false);
                                    file_view_mode.set(FileViewMode::Editor);
                                    dom.center_row(cursor().row, cell_dims().height);
                                    let _ = send(&FileViewModeSet { mode: FileViewMode::Editor });
                                    focus_file_input();
                                },
                                {translate("editor-editor")}
                            }
                            if git_has_diff() {
                                button {
                                    class: file_mode_class(file_view_mode() == FileViewMode::Diff),
                                    title: translate("editor-git-diff"),
                                    onclick: move |_| {
                                        file_view_mode.set(FileViewMode::Diff);
                                        git_nonce.set(git_nonce().wrapping_add(1));
                                        let _ = send(&FileViewModeSet { mode: FileViewMode::Diff });
                                    },
                                    {translate("editor-diff")}
                                }
                            }
                        }
                    }
                    div {
                        class: "flex shrink-0 items-center gap-0.5 rounded-md bg-foreground/[0.06] p-0.5 text-[10px] font-medium ring-1 ring-inset ring-foreground/10",
                        title: translate("schema-keymap"),
                        button {
                            class: file_mode_class(keymap() == vmux_core::KeymapKind::Vscode),
                            onclick: move |_| {
                                let next = vmux_core::KeymapKind::Vscode;
                                keymap.set(next);
                                ed_mode.set(vmux_core::editor::EditMode::Insert);
                                ed_label.set(String::new());
                                let _ = send(&FileKeymapSet { keymap: next });
                                if file_view_mode() == FileViewMode::Note
                                    && is_markdown()
                                    && !note_cursor.editing()
                                {
                                    focus_container();
                                } else {
                                    focus_file_input();
                                }
                            },
                            {translate("editor-keymap-standard")}
                        }
                        button {
                            class: file_mode_class(keymap() == vmux_core::KeymapKind::Vim),
                            onclick: move |_| {
                                let next = vmux_core::KeymapKind::Vim;
                                keymap.set(next);
                                let next_mode = vmux_core::editor::EditMode::Normal;
                                ed_mode.set(next_mode);
                                ed_label.set(next_mode.label().to_string());
                                let _ = send(&FileKeymapSet { keymap: next });
                                if file_view_mode() == FileViewMode::Note
                                    && is_markdown()
                                    && !note_cursor.editing()
                                {
                                    focus_container();
                                } else {
                                    focus_file_input();
                                }
                            },
                            {translate("editor-keymap-vim")}
                        }
                    }
                }
                {
                    tidy_prompt().map(|count| {
                        rsx! {
                            div {
                                class: "flex shrink-0 items-center gap-1.5 text-[11px]",
                                span {
                                    class: "select-none text-primary",
                                    {translate_with(
                                        "editor-unchanged-previews",
                                        &[("count", TranslationValue::Number(count as i64))],
                                    )}
                                }
                                button {
                                    class: "rounded-full bg-primary/20 px-2 py-0.5 font-medium text-primary hover:bg-primary/30",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyRequest { choice: TidyChoice::Tidy });
                                        tidy_prompt.set(None);
                                    },
                                    {translate("editor-tidy")}
                                }
                                button {
                                    class: "rounded-full px-2 py-0.5 text-foreground/60 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyRequest { choice: TidyChoice::Always });
                                        tidy_prompt.set(None);
                                    },
                                    {translate("editor-always")}
                                }
                                button {
                                    class: "rounded-full px-1.5 py-0.5 text-foreground/40 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyRequest { choice: TidyChoice::Dismiss });
                                        tidy_prompt.set(None);
                                    },
                                    "\u{2715}"
                                }
                            }
                        }
                    })
                }
            }

            EditorBreadcrumbs {
                display_path: path(),
                abs_path: breadcrumb_path,
                leaf_is_dir: mode() == Mode::Dir,
                outline: breadcrumb_outline,
                caret_line: breadcrumb_caret_line,
            }

            {
                let msg = error.read().clone();
                (!msg.is_empty()).then(|| rsx! {
                    div {
                        class: "absolute inset-0 z-50 flex items-center justify-center bg-black/60",
                        div {
                            class: "flex max-w-xl flex-col items-center gap-3 rounded-md border border-ansi-1 bg-background px-4 py-3 text-sm text-ansi-1",
                            span { class: "break-all text-center", "{msg}" }
                            if error_undecodable() {
                                EncodingRecovery {}
                            }
                        }
                    }
                })
            }

            if !error().is_empty() {
                div {
                    class: "flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-8 text-center",
                    div { class: "text-sm font-medium text-foreground", {translate("editor-cannot-open")} }
                    div { class: "max-w-xl break-all font-mono text-xs text-muted-foreground", "{error()}" }
                }
            }

            match mode() {
                Mode::Media(kind) => rsx! {
                    div { class: "flex min-h-0 flex-1 items-center justify-center overflow-auto p-4",
                        if let Some(m) = media() {
                            match kind {
                                MediaKind::Image => rsx! {
                                    img { src: "{m.url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_color-mix(in_oklab,var(--primary)_40%,transparent)] ring-1 ring-primary/20" }
                                },
                                MediaKind::Video => rsx! {
                                    video {
                                        src: "{m.url}",
                                        controls: true,
                                        autoplay: false,
                                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_color-mix(in_oklab,var(--primary)_40%,transparent)] ring-1 ring-primary/20",
                                    }
                                },
                                MediaKind::Audio => rsx! {
                                    audio { src: "{m.url}", controls: true, class: "w-2/3" }
                                },
                                MediaKind::Pdf => {
                                    let display = path();
                                    let abs = m.abs_path.clone();
                                    rsx! {
                                        div { class: "flex flex-col items-center gap-3 rounded-2xl bg-white/[0.03] px-8 py-6 ring-1 ring-inset ring-primary/15 backdrop-blur-2xl",
                                            span { class: "text-xs uppercase tracking-wide text-foreground/70", "PDF" }
                                            span { class: "max-w-md truncate text-sm text-foreground/90", "{display}" }
                                            button {
                                                class: "rounded-lg bg-primary/15 px-3 py-1.5 text-xs font-semibold text-primary hover:bg-primary/25",
                                                onclick: move |_| {
                                                    let _ = send(&FileOpenExternalRequest { path: abs.clone() });
                                                },
                                                {translate("editor-open-externally")}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Mode::Dir => rsx! {
                    DirColumns {
                        window: DirWindow {
                            dir_entries,
                            parent_entries,
                            path,
                            parent_path,
                            selected,
                            preview,
                            thumbs,
                            came_from,
                            back_dir,
                            show_hidden,
                        },
                    }
                },
                Mode::Text => rsx! {
                    if git_has_diff() {
                        DiffView {
                            repo_root: git_repo_root,
                            path: git_path,
                            nonce: git_nonce,
                            viewport: git_diff_viewport,
                            visible: file_view_mode() == FileViewMode::Diff,
                            markers: git_line_markers,
                        }
                    }
                    if file_view_mode() == FileViewMode::Note && is_markdown() {
                        {
                            let active = note_cursor.active();
                            let source_position = source_cursor();
                            let block_count = note_blocks.read().len();
                            let blank_line_slot = note_cursor.editing()
                                .then(|| {
                                    note_blocks
                                        .read()
                                        .as_slice()
                                        .blank_line_slot(source_position.line)
                                })
                                .flatten();
                            rsx! {
                                div {
                                    id: SCROLL_ID,
                                    class: "file-mode-note-enter min-h-0 flex-1 overflow-auto px-8 py-8",
                                    onclick: move |event| {
                                        if keymap() == vmux_core::KeymapKind::Vim {
                                            event.prevent_default();
                                            let line = source_cursor().line;
                                            if let Some(index) = note_blocks.read().as_slice().block_index_for_line(line) {
                                                note_cursor.activate(index, line);
                                            }
                                            return;
                                        }
                                        if note_cursor.editing() {
                                            note_cursor.reset();
                                            focus_container();
                                        }
                                    },
                                    onpointermove: move |event: Event<PointerData>| {
                                        if !note_dragging() {
                                            return;
                                        }
                                        if !event.held_buttons().contains(MouseButton::Primary) {
                                            note_dragging.set(false);
                                        }
                                    },
                                    onpointerup: move |_| note_dragging.set(false),
                                    onpointercancel: move |_| note_dragging.set(false),
                                    div {
                                        class: "mx-auto max-w-3xl font-sans text-[15px] leading-7 text-foreground/90",
                                        NoteProperties { properties: note_properties() }
                                        if blank_line_slot == Some(0) {
                                            NoteBlankLine {
                                                key: "blank-{source_position.line}",
                                                line: source_position.line,
                                                col: source_position.col,
                                                keymap: keymap(),
                                            }
                                        }
                                        for index in 0..block_count {
                                            {
                                                let cursor_in_block = note_blocks
                                                    .read()
                                                    .get(index)
                                                    .is_some_and(|block| {
                                                        block.start_line <= source_position.line
                                                            && source_position.line < block.end_line
                                                    });
                                                let editing = note_cursor.editing()
                                                    && Some(index as u32) == active
                                                    && cursor_in_block;
                                                rsx! {
                                                    NoteBlockView {
                                                        key: "block-{index}",
                                                        note_blocks,
                                                        diff_markers: git_line_markers,
                                                        index,
                                                        editing,
                                                        source_cursor,
                                                        source_selections: source_sel,
                                                        keymap: keymap(),
                                                        note_cursor,
                                                        note_dragging,
                                                        comp_open: editing && comp_open(),
                                                        comp_filtered: if editing {
                                                            comp_filtered.clone()
                                                        } else {
                                                            Vec::new()
                                                        },
                                                        comp_sel_clamped,
                                                    }
                                                    if blank_line_slot == Some(index + 1) {
                                                        NoteBlankLine {
                                                            key: "blank-{source_position.line}",
                                                            line: source_position.line,
                                                            col: source_position.col,
                                                            keymap: keymap(),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        textarea {
                                            id: INPUT_ID,
                                            value: "{typed}",
                                            onmounted: move |event: Event<MountedData>| {
                                                dom.field_mounted(event.data());
                                            },
                                            class: "pointer-events-none absolute left-0 top-0 h-px w-px resize-none overflow-hidden border-0 bg-transparent p-0 opacity-0 outline-none",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| ime.start(),
                                            oncompositionend: move |event: Event<CompositionData>| {
                                                ime.commit();
                                                send_committed_text(typed, event.data().data());
                                            },
                                            oninput: move |event: Event<FormData>| {
                                                if ime.active() {
                                                    return;
                                                }
                                                send_committed_text(typed, event.value());
                                            },
                                            onkeydown: move |event: Event<KeyboardData>| {
                                                event.stop_propagation();
                                                if ime.swallows(&event) {
                                                    return;
                                                }
                                                if keys.offer(&event) {
                                                    return;
                                                }
                                                if event.key() == Key::Escape {
                                                    event.prevent_default();
                                                    if keymap() != vmux_core::KeymapKind::Vim {
                                                        note_cursor.set_editing(false);
                                                    }
                                                    if let Some(stroke) = PressedKey::new(&event.data()).stroke() {
                                                        let _ = send(&stroke);
                                                    }
                                                    if keymap() == vmux_core::KeymapKind::Vim {
                                                        focus_file_input();
                                                    } else {
                                                        focus_container();
                                                    }
                                                    return;
                                                }
                                                let _ = forward_file_key(&event, ed_mode());
                                            },
                                        }
                                        if !note_references().is_empty() {
                                            div { class: "mt-10 border-t border-foreground/10 pt-5",
                                                div { class: "mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground", {translate("editor-references")} }
                                                div { class: "flex flex-col gap-1",
                                                    for reference in note_references() {
                                                        {
                                                            let open_path = reference.path.clone();
                                                            let open_title = reference.title.clone();
                                                            let open_line = reference.line;
                                                            rsx! {
                                                                button {
                                                                    key: "{reference.path}:{reference.line}:{reference.unlinked}",
                                                                    r#type: "button",
                                                                    class: "rounded-lg px-3 py-2 text-left text-xs text-foreground/75 ring-1 ring-inset ring-foreground/10 transition-colors hover:bg-foreground/[0.05] hover:text-foreground",
                                                                    title: "{reference.path}",
                                                                    onclick: move |_| {
                                                                        let _ = send(&KnowledgeLinkOpen {
                                                                            path: open_path.clone(),
                                                                            title: open_title.clone(),
                                                                            line: Some(open_line),
                                                                            create: false,
                                                                        });
                                                                    },
                                                                    div { class: "flex items-center gap-2",
                                                                        span { class: "min-w-0 flex-1 truncate font-medium", "{reference.title}" }
                                                                        if reference.unlinked {
                                                                            span { class: "shrink-0 rounded-full bg-amber-400/10 px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-amber-500", "Unlinked" }
                                                                        }
                                                                    }
                                                                    if !reference.preview.is_empty() {
                                                                        div { class: "mt-0.5 line-clamp-2 text-[11px] text-muted-foreground", "{reference.preview}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if file_view_mode() != FileViewMode::Diff || !git_has_diff() {
                        {
                            let cell = cell_dims();
                            let (cw, ch) = (cell.narrow, cell.height);
                            let overlay_lines = lines();
                            let overlay_layouts = line_layouts();
                            let ruler = RowRuler::new(
                                &overlay_lines,
                                &overlay_layouts,
                                cell,
                                wrap_columns(),
                            );
                            let gutter = gw as f64 * cw + 48.0;
                            let cx = gutter + ruler.x_of(cursor().row, cursor().col);
                            let cy = cursor().row as f64 * ch;
                            let cursor_style = if ed_mode().accepts_text() {
                                format!(
                                    "left:{cx}px;top:{cy}px;height:{ch}px;width:2px;background:currentColor;"
                                )
                            } else {
                                format!(
                                    "left:{cx}px;top:{cy}px;height:{ch}px;width:{}px;background:color-mix(in srgb,currentColor 28%,transparent);outline:1px solid currentColor;",
                                    ruler.advance_at(cursor().row, cursor().col).max(2.0)
                                )
                            };
                            let cursor_key =
                                format!("{}:{}:{:?}", cursor().row, cursor().col, ed_mode());
                            let spacer = total_rows() as f64 * ch;
                            let preedit = PreeditField::from(ime);
                            let txtcol = preedit.text_color();
                            let field_caret = preedit.caret_class();
                            rsx! {
                                div {
                                    id: SCROLL_ID,
                                    class: "file-mode-editor-enter relative min-h-0 flex-1 overflow-auto",
                                    onmouseleave: move |_| {
                                        lsp_hover.set(None);
                                        hover_pos.set(None);
                                        gutter_hover.set(false);
                                    },
                                    onpointermove: move |event: Event<PointerData>| {
                                        let Some(origin) = editor_drag_origin() else {
                                            return;
                                        };
                                        let at = event.client_coordinates();
                                        if !event.held_buttons().contains(MouseButton::Primary) {
                                            editor_dragging.set(false);
                                            editor_drag_origin.set(None);
                                            return;
                                        }
                                        if !editor_dragging()
                                            && editor_drag_started(origin, (at.x as i32, at.y as i32))
                                        {
                                            editor_dragging.set(true);
                                        }
                                    },
                                    onpointerup: move |_| {
                                        editor_dragging.set(false);
                                        editor_drag_origin.set(None);
                                    },
                                    onpointercancel: move |_| {
                                        editor_dragging.set(false);
                                        editor_drag_origin.set(None);
                                    },
                                    onmounted: move |event: Event<MountedData>| {
                                        dom.mounted(event.data());
                                    },
                                    onresize: move |event: Event<ResizeData>| {
                                        let Ok(size) = event.get_border_box_size() else {
                                            return;
                                        };
                                        dom.resized((size.width, size.height));
                                    },
                                    onscroll: move |event: Event<ScrollData>| {
                                        dom.scrolled_to((
                                            event.scroll_left(),
                                            event.scroll_top(),
                                        ));
                                        let ch = cell_dims().height;
                                        if ch <= 0.0 {
                                            return;
                                        }
                                        let vis_first = (event.scroll_top() / ch).floor().max(0.0) as u32;
                                        if last_scroll_req() == vis_first {
                                            return;
                                        }
                                        last_scroll_req.set(vis_first);
                                        let vis_rows = (event.client_height() as f64 / ch).ceil() as u32 + 1;
                                        let trigger = (vis_rows as f32 * vmux_core::scroll::EDGE_TRIGGER_K).ceil() as u32;
                                        let rfirst = first_row();
                                        let loaded_len = line_layouts
                                            .read()
                                            .last()
                                            .map_or(0, |line| line.row + line.rows as u32 - rfirst);
                                        let needs_rows = vmux_core::scroll::needs_refetch(
                                            vis_first,
                                            vis_rows,
                                            rfirst,
                                            loaded_len,
                                            trigger,
                                        );
                                        let _ = send(&FileScrollEvent {
                                            top_row: vis_first,
                                            needs_rows,
                                        });
                                    },
                                    StickyScope {
                                        lines: sticky_lines(),
                                        cell_height: ch,
                                        gutter_chars: gw,
                                        on_pick: move |row: u32| dom.center_row(row, ch),
                                    }
                                    div { class: "relative", style: "height:{spacer}px;",
                                        EditorLines {
                                            lines,
                                            line_layouts,
                                            first_row,
                                            diagnostics,
                                            git_line_markers,
                                            wrap_columns,
                                            cell_height: ch,
                                            gutter_chars: gw,
                                            total_lines,
                                            cell_dims,
                                            ctx_menu,
                                            editor_dragging,
                                            editor_drag_origin,
                                            gutter_hover,
                                            hover_pos,
                                            lsp_hover,
                                            hover_diag,
                                        }
                                        if sel().is_empty() {
                                            {
                                                let mut caret_rows = carets()
                                                    .iter()
                                                    .map(|caret| caret.row)
                                                    .collect::<Vec<_>>();
                                                caret_rows.push(cursor().row);
                                                caret_rows.sort_unstable();
                                                caret_rows.dedup();
                                                rsx! {
                                                    for row in caret_rows {
                                                        {
                                                            let top = row as f64 * ch;
                                                            let style = format!(
                                                                "left:{gutter}px;right:0;top:{top}px;height:{ch}px;",
                                                            );
                                                            rsx! {
                                                                div {
                                                                    key: "curline{row}",
                                                                    class: "pointer-events-none absolute z-0 bg-foreground/[0.05]",
                                                                    style: "{style}",
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        for s in word_spans().iter() {
                                            {
                                                let top = s.row as f64 * ch;
                                                let left = gutter + ruler.x_of(s.row, s.start);
                                                let w = ruler.width_between(s.row, s.start, s.end);
                                                let style = format!("left:{left}px;top:{top}px;height:{ch}px;width:{w}px;");
                                                rsx! {
                                                    div {
                                                        key: "word{s.row}-{s.start}",
                                                        class: "pointer-events-none absolute z-0 rounded-[2px] bg-foreground/20 ring-1 ring-inset ring-foreground/30",
                                                        style: "{style}",
                                                    }
                                                }
                                            }
                                        }

                                        for s in search_spans().iter() {
                                            {
                                                let top = s.row as f64 * ch;
                                                let left = gutter + ruler.x_of(s.row, s.start);
                                                let w = ruler.width_between(s.row, s.start, s.end);
                                                let style = format!("left:{left}px;top:{top}px;height:{ch}px;width:{w}px;");
                                                rsx! {
                                                    div {
                                                        key: "search{s.row}-{s.start}",
                                                        class: "pointer-events-none absolute z-0 bg-amber-400/30",
                                                        style: "{style}",
                                                    }
                                                }
                                            }
                                        }

                                        for s in sel().iter() {
                                            {
                                                let top = s.row as f64 * ch;
                                                let left = gutter + ruler.x_of(s.row, s.start);
                                                let style = if s.end == u32::MAX {
                                                    format!("left:{left}px;top:{top}px;height:{ch}px;right:0;")
                                                } else {
                                                    let w = ruler.width_between(s.row, s.start, s.end);
                                                    format!("left:{left}px;top:{top}px;height:{ch}px;width:{w}px;")
                                                };
                                                rsx! {
                                                    div {
                                                        key: "sel{s.row}:{s.start}:{s.end}",
                                                        class: "pointer-events-none absolute z-0 bg-primary/20",
                                                        style: "{style}",
                                                    }
                                                }
                                            }
                                        }

                                        if !preedit.owns_caret() {
                                            div {
                                                key: "{cursor_key}",
                                                class: "pointer-events-none absolute z-20 rounded-[1px]",
                                                style: "{cursor_style}",
                                            }
                                        }

                                        for extra in carets().iter().filter(|c| **c != cursor()) {
                                            {
                                                let ex = gutter + ruler.x_of(extra.row, extra.col);
                                                let ey = extra.row as f64 * ch;
                                                let style = format!(
                                                    "left:{ex}px;top:{ey}px;height:{ch}px;width:2px;background-color:currentColor;"
                                                );
                                                rsx! {
                                                    div {
                                                        key: "caret{extra.row}:{extra.col}",
                                                        class: "pointer-events-none absolute z-20 rounded-[1px]",
                                                        style: "{style}",
                                                    }
                                                }
                                            }
                                        }

                                        textarea {
                                            id: INPUT_ID,
                                            value: "{typed}",
                                            onmounted: move |event: Event<MountedData>| {
                                                dom.field_mounted(event.data());
                                            },
                                            class: "absolute z-10 min-w-[2ch] resize-none overflow-hidden whitespace-pre border-0 bg-transparent p-0 outline-none {field_caret}",
                                            style: "left:{cx}px;top:{cy}px;height:{ch}px;color:{txtcol};",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| ime.start(),
                                            oncompositionend: move |event: Event<CompositionData>| {
                                                ime.commit();
                                                send_committed_text(typed, event.data().data());
                                            },
                                            oninput: move |event: Event<FormData>| {
                                                if ime.active() {
                                                    return;
                                                }
                                                send_committed_text(typed, event.value());
                                            },
                                            onkeydown: move |e: Event<KeyboardData>| {
                                                e.stop_propagation();
                                                if ime.swallows(&e) {
                                                    return;
                                                }
                                                if keys.offer(&e) {
                                                    return;
                                                }
                                                let _ = forward_file_key(&e, ed_mode());
                                            },
                                        }

                                        {
                                            lsp_hover().map(|h| {
                                                let Some(i) = lines().iter().position(|l| l.line_no == h.line) else {
                                                    return rsx! {};
                                                };
                                                let mut hovered = String::new();
                                                for span in &lines()[i].spans {
                                                    hovered.push_str(&span.text);
                                                }
                                                let hrow = first_row() + i as u32;
                                                let top = hrow as f64 * ch + ch;
                                                let left = gutter
                                                    + ColumnRuler::new(&hovered, cell).x_of_char(h.col);
                                                rsx! {
                                                    div {
                                                        class: "absolute z-30 max-h-64 max-w-2xl overflow-auto rounded-xl bg-foreground/[0.05] px-3 py-2 text-xs leading-snug text-foreground/90 ring-1 ring-inset ring-primary/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                                                        style: "left:{left}px;top:{top}px;",
                                                        for (bi, b) in h.blocks.iter().enumerate() {
                                                            if b.code {
                                                                div {
                                                                    key: "b{bi}",
                                                                    class: "my-1 max-w-full overflow-x-auto whitespace-pre font-mono",
                                                                    for line in b.lines.iter() {
                                                                        div { key: "{line.line_no}",
                                                                            for (si, s) in line.spans.iter().enumerate() {
                                                                                span { key: "{si}", style: "{span_style(s)}", "{s.text}" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                div {
                                                                    key: "b{bi}",
                                                                    class: "whitespace-pre-wrap opacity-80",
                                                                    "{b.text}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            })
                                        }

                                        CodeActionMenu {
                                            titles: code_actions,
                                            selected: code_action_sel,
                                            top: cursor().row as f64 * ch + ch,
                                            left: gutter + ruler.x_of(cursor().row, cursor().col),
                                        }

                                        if let Some(rename) = rename_box() {
                                            RenameInput {
                                                state: rename_box,
                                                top: rename.line() as f64 * ch + ch,
                                                left: gutter + ruler.x_of_char(rename.line(), rename.col()),
                                            }
                                        }

                                        {
                                            (comp_open() && !comp_filtered.is_empty()).then(|| {
                                                let (cline, cfrom) = comp_anchor();
                                                let top = cline as f64 * ch + ch;
                                                let left = gutter + ruler.x_of_char(cline, cfrom);
                                                rsx! {
                                                    div {
                                                        class: "absolute z-40 max-h-56 min-w-48 overflow-auto rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-primary/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                                                        style: "left:{left}px;top:{top}px;",
                                                        for (i, it) in comp_filtered.iter().enumerate() {
                                                            div {
                                                                key: "{i}",
                                                                class: if i == comp_sel_clamped { "flex items-center gap-2 px-3 py-1 bg-primary/15" } else { "flex items-center gap-2 px-3 py-1" },
                                                                span { class: "truncate", "{it.label}" }
                                                                if !it.detail.is_empty() {
                                                                    span { class: "ml-auto truncate text-[10px] text-foreground/40", "{it.detail}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            })
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }

            {
                lsp_install_notice().map(|progress| {
                    let (icon_class, icon, spinning) = match progress.phase {
                        InstallPhase::Done => ("text-ansi-2", "✓", false),
                        InstallPhase::Failed => ("text-ansi-1", "×", false),
                        _ => ("text-primary", "", true),
                    };
                    let detail = progress.pct.map_or_else(
                        || progress.message.clone(),
                        |percent| format!("{} {percent}%", progress.message),
                    );
                    rsx! {
                        div {
                            class: "pointer-events-none fixed right-4 bottom-14 z-[60] flex min-w-64 max-w-sm items-center gap-3 rounded-xl bg-background/95 px-3 py-2.5 text-xs text-foreground shadow-[0_12px_40px_rgba(0,0,0,0.28)] ring-1 ring-inset ring-foreground/10 backdrop-blur-xl",
                            if spinning {
                                span { class: "h-4 w-4 shrink-0 animate-spin rounded-full border-2 border-primary/25 border-t-primary" }
                            } else {
                                span { class: "grid h-4 w-4 shrink-0 place-items-center text-base font-semibold {icon_class}", "{icon}" }
                            }
                            div { class: "min-w-0",
                                div { class: "truncate font-medium", "{progress.name}" }
                                div { class: "truncate text-[10px] text-muted-foreground", "{detail}" }
                            }
                        }
                    }
                })
            }

            {
                (!rename_failed().is_empty()).then(|| {
                    let reason = rename_failed();
                    rsx! {
                        div {
                            class: "pointer-events-none fixed right-4 bottom-14 z-[60] flex min-w-64 max-w-sm items-center gap-3 rounded-xl bg-background/95 px-3 py-2.5 text-xs text-foreground shadow-[0_12px_40px_rgba(0,0,0,0.28)] ring-1 ring-inset ring-foreground/10 backdrop-blur-xl",
                            span { class: "grid h-4 w-4 shrink-0 place-items-center text-base font-semibold text-ansi-1", "×" }
                            div { class: "min-w-0",
                                div { class: "truncate font-medium", {translate("editor-rename-failed")} }
                                div { class: "truncate text-[10px] text-muted-foreground", "{reason}" }
                            }
                        }
                    }
                })
            }

            {
                hover_diag().map(|d| rsx! {
                    div {
                        class: "pointer-events-none absolute right-4 bottom-5 z-50 max-w-md rounded-xl bg-foreground/[0.04] px-3 py-2 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                        div { class: "flex items-center gap-2",
                            span { class: "{severity_color_class(d.severity)}", "●" }
                            span { class: "whitespace-pre-wrap", "{d.message}" }
                        }
                        if let Some(src) = d.source.as_ref() {
                            div { class: "mt-1 opacity-50", "{src}" }
                        }
                    }
                })
            }

            EditorContextMenu { position: ctx_menu, offered: lsp_actions }
            ReferencesPanel { open: refs_open, items: refs, selected: refs_sel }
        }
        }

            GitFooter {
                path: git_path,
                branch: git_branch,
                ahead: git_ahead,
                behind: git_behind,
                staged_count: git_staged,
                message: git_message,
                result: git_result,
                always_visible: mode() == Mode::Text
                    && keymap() == vmux_core::KeymapKind::Vim,
                leading: rsx! {
                    if mode() == Mode::Text && keymap() == vmux_core::KeymapKind::Vim {
                        VimStatus { label: ed_label() }
                    }
                },
                {
                    lsp_status().map(|s| {
                        let (dot, label) = match s.state {
                            LspServerState::Ready => ("text-ansi-2", s.server.clone()),
                            LspServerState::Starting => {
                                (
                                    "text-ansi-3",
                                    translate_with(
                                        "editor-lsp-starting",
                                        &[("server", TranslationValue::String(&s.server))],
                                    ),
                                )
                            }
                            LspServerState::Missing => {
                                (
                                    "text-ansi-1",
                                    translate_with(
                                        "editor-lsp-not-installed",
                                        &[("server", TranslationValue::String(&s.server))],
                                    ),
                                )
                            }
                        };
                        rsx! {
                            span {
                                class: "flex shrink-0 items-center gap-1.5",
                                title: "LSP",
                                span { class: "{dot}", "\u{25CF}" }
                                span { "{label}" }
                            }
                        }
                    })
                }
                if status_scope.shows_anything() {
                    FileStatusInfo {
                        scope: status_scope,
                        line: status_caret.line + 1,
                        col: status_caret.char_col + 1,
                        indent: indent(),
                        line_ending: line_ending(),
                        encoding: encoding(),
                        language: language(),
                    }
                }
            }
        }
    }
}

const PAGE_ID: &str = "file-page";
const MEASURE_ID: &str = "file-measure";
const MEASURE_WIDE_ID: &str = "file-measure-wide";
const MEASURE_COLS: usize = 80;
const MEASURE_ROWS: usize = 8;
const MEASURE_WIDE_GLYPH: &str = "\u{6f22}";
const RENAME_NOTICE_MS: u32 = 2400;
const HOVER_DELAY_MS: u32 = 300;
const SCROLL_ID: &str = "file-scroll";
const GIT_REFRESH_DEBOUNCE_MS: u32 = 120;
const LSP_NOTICE_DONE_MS: u32 = 2_500;
const LSP_NOTICE_FAILED_MS: u32 = 6_000;

std::thread_local! {}

fn file_mode_class(active: bool) -> &'static str {
    if active {
        "rounded bg-primary/15 px-1.5 py-0.5 text-primary transition-[background-color,color,box-shadow] duration-200 ease-out"
    } else {
        "rounded px-1.5 py-0.5 text-foreground/45 transition-[background-color,color,box-shadow] duration-200 ease-out hover:bg-foreground/[0.06] hover:text-foreground"
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dir,
    Text,
    Media(MediaKind),
}

#[derive(Clone, Copy)]
struct GitRefresh {
    generation: Signal<u32>,
    nonce: Signal<u32>,
    settled: Signal<bool>,
}

impl GitRefresh {
    fn schedule(mut self) {
        let next = self.generation.peek().wrapping_add(1);
        self.generation.set(next);
        if *self.settled.peek() {
            self.settled.set(false);
            self.bump();
        }
        spawn(async move {
            sleep_ms(GIT_REFRESH_DEBOUNCE_MS).await;
            if *self.generation.peek() != next {
                return;
            }
            self.settled.set(true);
            self.bump();
        });
    }

    fn bump(&mut self) {
        let next = self.nonce.peek().wrapping_add(1);
        self.nonce.set(next);
    }
}

fn diff_marker_sign(marker: EditorDiffMarker) -> &'static str {
    diff_tone(marker).sign()
}

fn diff_marker_text_class(marker: EditorDiffMarker) -> &'static str {
    diff_tone(marker).text_class()
}

fn diff_marker_row_class(marker: EditorDiffMarker) -> &'static str {
    diff_tone(marker).row_class()
}

pub(super) fn diff_tone(marker: EditorDiffMarker) -> DiffTone {
    match marker {
        EditorDiffMarker::Added => DiffTone::Added,
        EditorDiffMarker::Modified => DiffTone::Modified,
        EditorDiffMarker::Deleted => DiffTone::Deleted,
        EditorDiffMarker::Staged => DiffTone::Staged,
    }
}

fn schedule_lsp_notice_clear(
    mut notice: Signal<Option<LspInstallProgress>>,
    mut request: Signal<Option<(String, String)>>,
    mut generation: Signal<u32>,
    delay: u32,
) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    spawn(async move {
        sleep_ms(delay).await;
        if generation() == id {
            notice.set(None);
            request.set(None);
        }
    });
}
