#![allow(non_snake_case)]

use super::breadcrumb::EditorBreadcrumbs;
use super::diagnostic::DiagnosticPresentation;
use super::directory::{Preview, PreviewPane, clear_preview, image_data_url, toggle_video};
use super::dom::{EditorDom, ScrolledLineHeight};
use super::editor::{EditorLines, StickyScope};
use super::explorer::SidebarView;
use super::input::{EditorFocus, EditorInput, PreeditField};
use super::key::use_file_keys;
use super::menu::{CodeActionMenu, EditorContextMenu, ReferencesPanel, RenameBox, RenameInput};
use super::note::{NoteBlankLine, NoteBlockView, NoteBlocks, NoteCursor, NoteProperties};
use super::sidebar::{ExplorerPane, ExplorerSidebar, ExplorerToggleButton, PaneWidth};
use super::state::use_file_ui;
use super::status::{EncodingRecovery, FileStatusInfo, FileStatusScope};
use super::text_geometry::{CellMetrics, ColumnRuler, GutterWidth, RowRuler};
use super::text_style::StyledSpanStyle;
use super::toolbar::{EditorTabItem, EditorTabStrip, FindBar, VimStatus};
use std::collections::HashMap;

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_api::editor::KeymapKind;
use vmux_api::editor::{CursorPos, EditMode, SelSpan};
use vmux_api::media::MediaKind;
use vmux_ecs::event::*;
use vmux_ecs::scroll::{EDGE_TRIGGER_K, ScrollWindow};
use vmux_git::event::{FileGitState, GitLineStatus};
use vmux_git::ui::{DiffView, GitFooter};
use vmux_knowledge::{KnowledgeProperty, KnowledgeReference};
use vmux_ui::diff::DiffTone;
use vmux_ui::directory::DirectoryNavigator;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{PressedKey, send, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::ime::use_ime_guard;
use vmux_ui::platform::sleep_ms;
use vmux_ui::scroll::ScrollIntoView;

#[component]
pub fn Page() -> Element {
    use_theme();
    use_ui_state::<FileUiState>();
    let mut path = use_signal(String::new);
    let mut document_revision = use_signal(|| 0u64);
    let mut document_kind = use_signal(FileDocumentKind::default);
    let mut total_lines = use_signal(|| 0u32);
    let mut total_rows = use_signal(|| 0u32);
    let mut first_row = use_signal(|| 0u32);
    let mut gutter_hover = use_signal(|| false);
    let mut language = use_signal(String::new);
    let mut indent = use_signal(FileIndent::default);
    let mut line_ending = use_signal(FileLineEnding::default);
    let mut encoding = use_signal(FileEncoding::default);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut sticky_lines = use_signal(Vec::<FileLine>::new);
    let mut outline = use_signal(Vec::<OutlineRow>::new);
    let mut line_layouts = use_signal(Vec::<FileLineLayout>::new);
    let mut wrap_columns = use_signal(|| 0u16);
    let mut diagnostics = use_signal(Vec::<FileDiagnostic>::new);
    let mut hover_diag = use_signal(|| Option::<FileDiagnostic>::None);
    let mut lsp_status = use_signal(|| Option::<FileLspStatus>::None);
    let mut lsp_capabilities = use_signal(Vec::<EditorCapability>::new);
    let mut lsp_install_notice = use_signal(|| Option::<LspInstallProgress>::None);
    let mut lsp_notice_generation = use_signal(|| 0u32);
    let mut code_actions = use_signal(Vec::<String>::new);
    let mut code_action_sel = use_signal(|| 0usize);
    let mut rename_box = use_signal(|| Option::<RenameBox>::None);
    let mut rename_failed = use_signal(String::new);
    let mut rename_failed_generation = use_signal(|| 0u32);
    let mut error = use_signal(String::new);
    let mut error_undecodable = use_signal(|| false);
    let mut dir_entries = use_signal(Vec::<FileDirEntry>::new);
    let mut parent_entries = use_signal(Vec::<FileDirEntry>::new);
    let mut selected = use_signal(|| 0usize);
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
    let mut git_state = use_signal(FileGitState::default);
    let git_repo_root = use_memo(move || {
        let state = git_state();
        if state.path == git_path() {
            state.repo_root
        } else {
            String::new()
        }
    });
    let git_has_diff = use_memo(move || {
        let state = git_state();
        state.path == git_path() && state.has_diff
    });
    let git_diff_viewport = use_memo(move || {
        let state = git_state();
        (state.path == git_path())
            .then_some(state.diff_viewport)
            .flatten()
    });
    let git_diff_rows = use_memo(move || {
        let state = git_state();
        if state.path == git_path() {
            state.diff_rows
        } else {
            Vec::new()
        }
    });
    let git_line_markers = use_memo(move || {
        git_diff_viewport()
            .map(|viewport| {
                viewport
                    .markers
                    .into_iter()
                    .map(|marker| (marker.line, marker.status))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default()
    });
    let mut file_view_mode = use_signal(|| FileViewMode::Note);
    let mut view_mode_revision = use_signal(|| 0u64);
    let mut note_blocks = use_signal(Vec::<NoteBlock>::new);
    let mut note_properties = use_signal(Vec::<KnowledgeProperty>::new);
    let mut note_references = use_signal(Vec::<KnowledgeReference>::new);
    let note_cursor = NoteCursor::new();
    let mut note_dragging = use_signal(|| false);
    let mut editor_dragging = use_signal(|| false);
    let mut editor_drag_origin = use_signal(|| Option::<(i32, i32)>::None);
    let mut ed_mode = use_signal(|| EditMode::Insert);
    let mut ed_label = use_signal(String::new);
    let mut search_spans = use_signal(Vec::<SelSpan>::new);
    let mut word_spans = use_signal(Vec::<SelSpan>::new);
    let find_open = use_signal(|| false);
    let find_forward = use_signal(|| true);
    let mut find_revision = use_signal(|| 0u64);
    let sidebar_view = use_signal(SidebarView::default);
    let mut explorer_search_focus_revision = use_signal(|| 0u64);
    let find_query = use_signal(String::new);
    let mut find_total = use_signal(|| 0u32);
    let mut find_index = use_signal(|| 0u32);
    let mut keymap = use_signal(KeymapKind::default);
    let mut cursor = use_signal(CursorPos::default);
    let mut carets = use_signal(Vec::<CursorPos>::new);
    let mut sel = use_signal(Vec::<SelSpan>::new);
    let mut source_cursor = use_signal(CursorPos::default);
    let mut source_sel = use_signal(Vec::<SelSpan>::new);
    let mut open_editors = use_signal(Vec::<OpenEditorItem>::new);
    let ime = use_ime_guard();
    let typed = use_signal(String::new);
    let mut lsp_hover = use_signal(|| Option::<FileHover>::None);
    let mut hover_pos = use_signal(|| Option::<(u32, u32)>::None);
    let ctx_menu = use_signal(|| Option::<(f64, f64, u32, u32)>::None);
    let mut panel = use_signal(FilePanelState::default);
    let mut panel_focus_revision = use_signal(|| 0u64);
    let mut last_scroll_req = use_signal(|| 0u32);
    let explorer = ExplorerPane::new(page_width);
    let mut tidy_prompt = use_signal(|| Option::<u32>::None);
    let mut doc_title = use_signal(String::new);
    let is_markdown = use_memo(move || document_kind() == FileDocumentKind::Markdown);

    let keys = use_file_keys(panel);
    use_context_provider(|| keys);

    let explorer_panel = use_file_ui::<ExplorerPanelEvent>();
    use_effect(move || {
        explorer_panel.for_each(|event| {
            let mut view = sidebar_view;
            view.set(match event.search {
                true => SidebarView::Search,
                false => SidebarView::Explorer,
            });
            if event.search && event.search_focus_revision > explorer_search_focus_revision() {
                explorer_search_focus_revision.set(event.search_focus_revision);
                spawn(async move {
                    sleep_ms(0).await;
                    FocusClaim::new(super::explorer::SEARCH_INPUT_ID).request();
                });
            }
            explorer.apply_panel(event);
        })
    });

    let find_event = use_file_ui::<FileFindEvent>();
    use_effect(move || {
        find_event.for_each(|event| {
            if event.revision <= find_revision() {
                return;
            }
            find_revision.set(event.revision);
            let mut open = find_open;
            let mut forward = find_forward;
            open.set(event.open);
            forward.set(event.forward);
            if event.open {
                spawn(async move {
                    sleep_ms(0).await;
                    EditorFocus::find();
                });
            }
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
            lsp_install_notice.set(None);
            lsp_notice_generation.set(lsp_notice_generation().wrapping_add(1));
            explorer.show_if_room(mode);
            note_blocks.set(Vec::new());
            note_properties.set(Vec::new());
            note_references.set(Vec::new());
            note_cursor.reset();
            note_dragging.set(false);
            editor_dragging.set(false);
            editor_drag_origin.set(None);
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
                if *keymap.peek() == KeymapKind::Vim
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
    let mut scroll_effect_revision = use_signal(|| 0u64);
    use_effect(move || {
        scroll_by.for_each(|event| {
            if event.revision <= *scroll_effect_revision.peek() {
                return;
            }
            let Some(line_height) =
                ScrolledLineHeight::resolve(file_view_mode(), document_kind(), cell_dims().height)
            else {
                return;
            };
            scroll_effect_revision.set(event.revision);
            dom.scroll_by(event.lines, line_height);
        })
    });

    let open_editors_event = use_file_ui::<OpenEditorsEvent>();
    use_effect(move || {
        open_editors_event.for_each(|event| {
            open_editors.set(event.items);
        })
    });

    let file_git_state = use_file_ui::<FileGitState>();
    use_effect(move || {
        file_git_state.for_each(|state| {
            git_state.set(state);
        })
    });

    let view_mode_event = use_file_ui::<FileViewModeEvent>();
    use_effect(move || {
        view_mode_event.for_each(|event| {
            if event.revision <= *view_mode_revision.peek() {
                return;
            }
            view_mode_revision.set(event.revision);
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
                    if note_cursor.editing() {
                        EditorFocus::file();
                    } else {
                        EditorFocus::container();
                    }
                }
                FileViewMode::Editor => {
                    dom.center_row(cursor().row, cell_dims().height);
                    EditorFocus::file();
                }
                FileViewMode::Note | FileViewMode::Diff => EditorFocus::container(),
            }
        })
    });

    let keymap_event = use_file_ui::<FileKeymapEvent>();
    use_effect(move || {
        keymap_event.for_each(|event| {
            keymap.set(event.keymap);
            if event.keymap == KeymapKind::Vim
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
            let activation = NoteCursorActivation::resolve(
                reveal_line,
                keymap() == KeymapKind::Vim && file_view_mode() == FileViewMode::Note,
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

    let hover_event = use_file_ui::<FileHover>();
    use_effect(move || {
        hover_event.for_each(|h| {
            lsp_hover.set(Some(h));
        })
    });

    let panel_event = use_file_ui::<FilePanelState>();
    use_effect(move || {
        panel_event.for_each(|state| {
            let focus = state.focus;
            panel.set(state);
            if focus.revision <= panel_focus_revision() {
                return;
            }
            panel_focus_revision.set(focus.revision);
            spawn(async move {
                sleep_ms(0).await;
                match focus.target {
                    FilePanelFocusTarget::None => {}
                    FilePanelFocusTarget::References => {
                        FocusClaim::new("refs-panel").request();
                    }
                    FilePanelFocusTarget::Editor => EditorFocus::file(),
                }
            });
        })
    });

    let diagnostics_event = use_file_ui::<FileDiagnostics>();
    use_effect(move || {
        diagnostics_event.for_each(|d| {
            if d.path != git_path() {
                return;
            }
            diagnostics.set(d.diagnostics);
        })
    });

    let lsp_status_event = use_file_ui::<FileLspStatus>();
    use_effect(move || {
        lsp_status_event.for_each(|s| {
            if s.path != git_path() {
                return;
            }
            lsp_capabilities.set(s.capabilities.clone());
            lsp_status.set(Some(s));
        })
    });

    let install_progress = use_file_ui::<LspInstallProgress>();
    use_effect(move || {
        install_progress.for_each(|progress| {
            let delay = match progress.phase {
                InstallPhase::Done => Some(LSP_NOTICE_DONE_MS),
                InstallPhase::Failed => Some(LSP_NOTICE_FAILED_MS),
                _ => None,
            };
            lsp_install_notice.set(Some(progress));
            if let Some(delay) = delay {
                schedule_lsp_notice_clear(lsp_install_notice, lsp_notice_generation, delay);
            }
        })
    });

    let package_status = use_file_ui::<LspPackageStatus>();
    use_effect(move || {
        package_status.for_each(|status| {
            if status.status != LspPkgStatus::Installed {
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

    let code_actions_event = use_file_ui::<FileCodeActions>();
    use_effect(move || {
        code_actions_event.for_each(|e| {
            code_action_sel.set(0);
            code_actions.set(e.titles);
        })
    });

    let rename_event = use_file_ui::<FileRenamePrompt>();
    use_effect(move || {
        rename_event.for_each(|e| {
            rename_failed.set(String::new());
            rename_box.set(Some(RenameBox::new(e.line, e.col, e.current)));
        })
    });

    let edit_failed = use_file_ui::<FileEditFailure>();
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

    let directory_event = use_file_ui::<FileDirectoryState>();
    use_effect(move || {
        directory_event.for_each(|d| {
            error.set(String::new());
            if path() != d.path {
                thumbs.set(HashMap::new());
            }
            preview.set(Preview::None);
            media.set(None);
            doc_title.set(
                d.path
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&d.path)
                    .to_string(),
            );
            git_path.set(d.abs_path);
            mode.set(Mode::Dir);
            diagnostics.set(Vec::new());
            hover_diag.set(None);
            lsp_status.set(None);
            dir_entries.set(d.entries);
            parent_entries.set(d.parent_entries);
            path.set(d.path);
            let index = usize::try_from(d.selected).unwrap_or_default();
            selected.set(index);
            ScrollIntoView::nearest(&format!("dir-row-{index}"));
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

    let gw = GutterWidth::for_lines(total_lines());
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
    let panel_state = panel();
    let panel_selection = panel_state.selected as usize;
    let (references, comp_filtered, comp_anchor) = match panel_state.content {
        Some(FilePanelContent::References { items }) => (items, Vec::new(), (0, 0)),
        Some(FilePanelContent::Completion {
            items,
            replace_from_col,
            line,
        }) => (Vec::new(), items, (line, replace_from_col)),
        None => (Vec::new(), Vec::new(), (0, 0)),
    };
    let refs_open = !references.is_empty();
    let comp_open = !comp_filtered.is_empty();
    let comp_sel_clamped = panel_selection.min(comp_filtered.len().saturating_sub(1));

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
            id: EditorFocus::CONTAINER_ID,
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
                                EditorFocus::file();
                            } else {
                                EditorFocus::container();
                            }
                        } else {
                            EditorFocus::file();
                        }
                    }
                    Mode::Dir => {
                        e.prevent_default();
                        EditorFocus::container();
                    }
                    Mode::Media(_) => EditorFocus::container(),
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
                    let _ = EditorInput::forward_key(&e, ed_mode());
                    return;
                }
                match current_mode {
                    Mode::Dir => match key.as_str() {
                        "Escape" => {
                            e.prevent_default();
                            let _ = send(&ExplorerCloseEditor { path: git_path() });
                        }
                        " " => {
                            e.prevent_default();
                            toggle_video();
                        }
                        _ => {
                            keys.offer(&e);
                        }
                    },
                    _ => {
                        if matches!(key.as_str(), "Escape" | "h") {
                            e.prevent_default();
                            let _ = send(&FileDirectoryBackRequest);
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
                        forward: find_forward,
                        vim: keymap() == KeymapKind::Vim,
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
                                    EditorFocus::file();
                                },
                                {translate("editor-editor")}
                            }
                            if git_has_diff() {
                                button {
                                    class: file_mode_class(file_view_mode() == FileViewMode::Diff),
                                    title: translate("editor-git-diff"),
                                    onclick: move |_| {
                                        file_view_mode.set(FileViewMode::Diff);
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
                            class: file_mode_class(keymap() == KeymapKind::Vscode),
                            onclick: move |_| {
                                let next = KeymapKind::Vscode;
                                keymap.set(next);
                                ed_mode.set(EditMode::Insert);
                                ed_label.set(String::new());
                                let _ = send(&FileKeymapSet { keymap: next });
                                if file_view_mode() == FileViewMode::Note
                                    && is_markdown()
                                    && !note_cursor.editing()
                                {
                                    EditorFocus::container();
                                } else {
                                    EditorFocus::file();
                                }
                            },
                            {translate("editor-keymap-standard")}
                        }
                        button {
                            class: file_mode_class(keymap() == KeymapKind::Vim),
                            onclick: move |_| {
                                let next = KeymapKind::Vim;
                                keymap.set(next);
                                let next_mode = EditMode::Normal;
                                ed_mode.set(next_mode);
                                ed_label.set(next_mode.label().to_string());
                                let _ = send(&FileKeymapSet { keymap: next });
                                if file_view_mode() == FileViewMode::Note
                                    && is_markdown()
                                    && !note_cursor.editing()
                                {
                                    EditorFocus::container();
                                } else {
                                    EditorFocus::file();
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
                    DirectoryNavigator {
                        path: path(),
                        parent_entries: parent_entries(),
                        entries: dir_entries(),
                        children: match preview() {
                            Preview::Dir(entries) => Some(entries),
                            _ => None,
                        },
                        selected: selected(),
                        thumbs: thumbs(),
                        preview: rsx! { PreviewPane { preview: preview() } },
                        on_select: move |(index, _)| {
                            let Ok(index) = u32::try_from(index) else {
                                return;
                            };
                            let _ = send(&FileDirectorySelectRequest { index });
                        },
                        on_ascend: move |target| {
                            let _ = send(&FileDirectoryAscendRequest { target });
                        },
                        on_descend: move |target| {
                            let _ = send(&FileDirectoryDescendRequest { target });
                        },
                        on_open: move |entry: FileDirEntry| {
                            let _ = send(&FileDirectoryOpenRequest { path: entry.path });
                        },
                        on_next: move |_| {
                            let _ = send(&FileDirectoryNextRequest);
                        },
                        on_previous: move |_| {
                            let _ = send(&FileDirectoryPreviousRequest);
                        },
                        on_activate: move |_| {
                            let _ = send(&FileDirectoryActivateRequest);
                        },
                        on_parent: move |_| {
                            let _ = send(&FileDirectoryParentRequest);
                        },
                        on_toggle_hidden: move |_| {
                            let _ = send(&FileDirectoryToggleHiddenRequest);
                        },
                    }
                },
                Mode::Text => rsx! {
                    if git_has_diff() {
                        DiffView {
                            repo_root: git_repo_root,
                            path: git_path,
                            viewport: git_diff_viewport,
                            display_rows: git_diff_rows,
                            loading: git_state().diff_loading,
                            visible: file_view_mode() == FileViewMode::Diff,
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
                                        if keymap() == KeymapKind::Vim {
                                            event.prevent_default();
                                            let line = source_cursor().line;
                                            if let Some(index) = note_blocks.read().as_slice().block_index_for_line(line) {
                                                note_cursor.activate(index, line);
                                            }
                                            return;
                                        }
                                        if note_cursor.editing() {
                                            note_cursor.reset();
                                            EditorFocus::container();
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
                                                        comp_open: editing && comp_open,
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
                                            id: EditorFocus::FILE_INPUT_ID,
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
                                                EditorInput::commit(typed, event.data().data());
                                            },
                                            oninput: move |event: Event<FormData>| {
                                                if ime.active() {
                                                    return;
                                                }
                                                EditorInput::commit(typed, event.value());
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
                                                    if keymap() != KeymapKind::Vim {
                                                        note_cursor.set_editing(false);
                                                    }
                                                    if let Some(stroke) = PressedKey::new(&event.data()).stroke() {
                                                        let _ = send(&stroke);
                                                    }
                                                    if keymap() == KeymapKind::Vim {
                                                        EditorFocus::file();
                                                    } else {
                                                        EditorFocus::container();
                                                    }
                                                    return;
                                                }
                                                let _ = EditorInput::forward_key(&event, ed_mode());
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
                                            && PointerDrag::started(origin, (at.x as i32, at.y as i32))
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
                                        let trigger = (vis_rows as f32 * EDGE_TRIGGER_K).ceil() as u32;
                                        let rfirst = first_row();
                                        let loaded_len = line_layouts
                                            .read()
                                            .last()
                                            .map_or(0, |line| line.row + line.rows as u32 - rfirst);
                                        let needs_rows = ScrollWindow::new(total_lines(), vis_first, vis_rows)
                                            .needs_refetch(rfirst, loaded_len, trigger);
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
                                            id: EditorFocus::FILE_INPUT_ID,
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
                                                EditorInput::commit(typed, event.data().data());
                                            },
                                            oninput: move |event: Event<FormData>| {
                                                if ime.active() {
                                                    return;
                                                }
                                                EditorInput::commit(typed, event.value());
                                            },
                                            onkeydown: move |e: Event<KeyboardData>| {
                                                e.stop_propagation();
                                                if ime.swallows(&e) {
                                                    return;
                                                }
                                                if keys.offer(&e) {
                                                    return;
                                                }
                                                let _ = EditorInput::forward_key(&e, ed_mode());
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
                                                                                span { key: "{si}", style: "{StyledSpanStyle::of(s)}", "{s.text}" }
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
                                            comp_open.then(|| {
                                                let (cline, cfrom) = comp_anchor;
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
                                                                onmousedown: move |event: Event<MouseData>| {
                                                                    event.prevent_default();
                                                                    let _ = send(&FilePanelPick { index: i as u32 });
                                                                },
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
                            span { class: "{DiagnosticPresentation::color_class(d.severity)}", "●" }
                            span { class: "whitespace-pre-wrap", "{d.message}" }
                        }
                        if let Some(src) = d.source.as_ref() {
                            div { class: "mt-1 opacity-50", "{src}" }
                        }
                    }
                })
            }

            EditorContextMenu { position: ctx_menu, offered: lsp_capabilities }
            ReferencesPanel {
                open: refs_open,
                items: references,
                selected: panel_selection,
            }
        }
        }

            GitFooter {
                git_state,
                always_visible: mode() == Mode::Text
                    && keymap() == KeymapKind::Vim,
                leading: rsx! {
                    if mode() == Mode::Text && keymap() == KeymapKind::Vim {
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
pub(crate) const HOVER_DELAY_MS: u32 = 300;
pub(crate) const SCROLL_ID: &str = "file-scroll";
const LSP_NOTICE_DONE_MS: u32 = 2_500;
const LSP_NOTICE_FAILED_MS: u32 = 6_000;

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

struct PointerDrag;

impl PointerDrag {
    fn started(origin: (i32, i32), current: (i32, i32)) -> bool {
        let dx = f64::from(current.0) - f64::from(origin.0);
        let dy = f64::from(current.1) - f64::from(origin.1);
        dx * dx + dy * dy >= 16.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoteCursorActivation {
    Center(u32),
    PreserveViewport(u32),
}

impl NoteCursorActivation {
    fn resolve(
        reveal_line: Option<u32>,
        restore_vim_cursor: bool,
        cursor_line: u32,
    ) -> Option<Self> {
        reveal_line
            .map(Self::Center)
            .or_else(|| restore_vim_cursor.then_some(Self::PreserveViewport(cursor_line)))
    }
}

pub(crate) fn diff_tone(marker: GitLineStatus) -> DiffTone {
    match marker {
        GitLineStatus::Added => DiffTone::Added,
        GitLineStatus::Modified => DiffTone::Modified,
        GitLineStatus::Deleted => DiffTone::Deleted,
        GitLineStatus::Staged => DiffTone::Staged,
    }
}

fn schedule_lsp_notice_clear(
    mut notice: Signal<Option<LspInstallProgress>>,
    mut generation: Signal<u32>,
    delay: u32,
) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    spawn(async move {
        sleep_ms(delay).await;
        if generation() == id {
            notice.set(None);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_requires_deliberate_pointer_movement() {
        assert!(!PointerDrag::started((100, 100), (103, 102)));
        assert!(PointerDrag::started((100, 100), (104, 100)));
        assert!(PointerDrag::started((100, 100), (96, 96)));
    }

    #[test]
    fn cursor_restore_preserves_viewport_until_explicit_reveal() {
        assert_eq!(
            NoteCursorActivation::resolve(Some(12), true, 8),
            Some(NoteCursorActivation::Center(12))
        );
        assert_eq!(
            NoteCursorActivation::resolve(None, true, 8),
            Some(NoteCursorActivation::PreserveViewport(8))
        );
        assert_eq!(NoteCursorActivation::resolve(None, false, 8), None);
    }
}
