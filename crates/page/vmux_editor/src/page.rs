#![allow(non_snake_case)]

use std::collections::HashMap;
use std::rc::Rc;

use crate::explorer::ExplorerPanel;
use crate::note::{ListEditLine, ListLineHit, MdBlockView, NoteLineChunk};
use crate::page_key::{Completions, FileKeys, FilePage, use_file_keys};
use crate::page_model::{
    NoteCursorActivation, NoteInlineKind, NoteInlineNode, centered_scroll_top, clamp_selection,
    dir_select_index, editor_drag_started, gutter_width, heading_class, image_mime, line_severity,
    note_cursor_activation, note_inline_nodes, note_list_marker_prefix_len, note_source_offset,
    note_source_position, severity_color_class, should_apply_explorer_chrome, span_style,
    squiggle_style,
};
use dioxus::html::geometry::{ClientPoint, ElementPoint};
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_core::knowledge::{KnowledgeProperty, KnowledgePropertyKind, KnowledgeReference};
use vmux_core::media::MediaKind;
use vmux_git::event::{GIT_CHANGED_EVENT, GitChangedEvent};
use vmux_git::ui::{DiffView, GitBar, GitFooter};
use vmux_git::view::EditorDiffMarker;
use vmux_ui::caret::{EventSelection, TextCaret};
use vmux_ui::components::icon::Icon;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{PressedKey, send, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::media::MediaElement;
use vmux_ui::platform::{now_millis, random_index, sleep_ms};
use vmux_ui::scroll::ScrollIntoView;
use vmux_ui::text_run::TextRun;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut path = use_signal(String::new);
    let mut total_lines = use_signal(|| 0u32);
    let mut total_rows = use_signal(|| 0u32);
    let mut first_row = use_signal(|| 0u32);
    let mut gutter_hover = use_signal(|| false);
    let mut language = use_signal(String::new);
    let mut indent = use_signal(vmux_core::event::FileIndent::default);
    let mut line_ending = use_signal(vmux_core::event::FileLineEnding::default);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut sticky_lines = use_signal(Vec::<FileLine>::new);
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
    let mut cell_dims = use_signal(|| (0.0f64, 0.0f64));
    let viewport = FileViewport::new();
    let mut page_width = use_signal(|| 0u32);
    let last_resize = use_signal(FileResizeEvent::default);
    let mut git_path = use_signal(String::new);
    let mut git_has_diff = use_signal(|| false);
    let mut git_line_markers = use_signal(HashMap::<u32, EditorDiffMarker>::new);
    let mut file_view_mode = use_signal(|| FileViewMode::Note);
    let mut note_blocks = use_signal(Vec::<NoteBlock>::new);
    let mut note_properties = use_signal(Vec::<KnowledgeProperty>::new);
    let mut note_references = use_signal(Vec::<KnowledgeReference>::new);
    let mut note_active = use_signal(|| Option::<u32>::None);
    let mut note_editing = use_signal(|| false);
    let mut note_edit_line = use_signal(|| Option::<u32>::None);
    let mut note_dragging = use_signal(|| false);
    let mut editor_dragging = use_signal(|| false);
    let mut editor_drag_origin = use_signal(|| Option::<(i32, i32)>::None);
    let mut git_nonce = use_signal(|| 0u32);
    let git_refresh_generation = use_signal(|| 0u32);
    let git_branch = use_signal(String::new);
    let git_ahead = use_signal(|| 0u32);
    let git_behind = use_signal(|| 0u32);
    let git_staged = use_signal(|| 0u32);
    let git_message = use_signal(String::new);
    let mut ed_mode = use_signal(|| vmux_core::editor::EditMode::Insert);
    let mut ed_label = use_signal(String::new);
    let mut ed_command_line = use_signal(String::new);
    let mut search_spans = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut word_spans = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let find_open = use_signal(|| false);
    let find_forward = use_signal(|| true);
    let find_query = use_signal(String::new);
    let mut find_total = use_signal(|| 0u32);
    let mut find_index = use_signal(|| 0u32);
    let mut keymap = use_signal(vmux_core::KeymapKind::default);
    let mut cursor = use_signal(vmux_core::editor::CursorPos::default);
    let mut carets = use_signal(Vec::<vmux_core::editor::CursorPos>::new);
    let mut sel = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut source_cursor = use_signal(vmux_core::editor::CursorPos::default);
    let mut source_sel = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut dirty = use_signal(|| false);
    let mut composing = use_signal(|| false);
    let mut lsp_hover = use_signal(|| Option::<FileHoverEvent>::None);
    let mut hover_pos = use_signal(|| Option::<(u32, u32)>::None);
    let mut ctx_menu = use_signal(|| Option::<(f64, f64, u32, u32)>::None);
    let mut refs = use_signal(Vec::<RefItem>::new);
    let mut refs_sel = use_signal(|| 0usize);
    let mut refs_open = use_signal(|| false);
    let mut comps = use_signal(Vec::<CompletionItem>::new);
    let mut comp_open = use_signal(|| false);
    let mut comp_sel = use_signal(|| 0usize);
    let mut comp_anchor = use_signal(|| (0u32, 0u32));
    let mut last_scroll_req = use_signal(|| 0u32);
    let explorer_visible = use_signal(|| false);
    let mut explorer_preferred_visible = use_signal(|| false);
    let mut explorer_width = use_signal(|| 240u32);
    let mut explorer_resizing = use_signal(|| false);
    let explorer_client_id = use_signal(explorer_client_id);
    let explorer_request_id = use_signal(|| 0u64);
    let explorer_reflowed_at = use_signal(|| Option::<ExplorerReflowKey>::None);
    let explorer_user_chose = use_signal(|| false);
    let explorer = ExplorerPane {
        visible: explorer_visible,
        preferred_visible: explorer_preferred_visible,
        width: explorer_width,
        page_width,
        client_id: explorer_client_id,
        request_id: explorer_request_id,
        reflowed_at: explorer_reflowed_at,
        user_chose: explorer_user_chose,
    };
    let mut tidy_prompt = use_signal(|| Option::<u32>::None);
    let mut doc_title = use_signal(String::new);

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
    };
    let keys = use_file_keys(file_page);
    use_context_provider(|| keys);

    let _chrome = use_listener::<ExplorerChromeEvent, _>(EXPLORER_CHROME_EVENT, move |c| {
        if should_apply_explorer_chrome(
            explorer_client_id(),
            explorer_request_id(),
            c.client_id,
            c.request_id,
        ) {
            explorer_preferred_visible.set(c.visible);
        }
        if explorer_width() != c.width {
            explorer_width.set(c.width);
        }
        explorer.sync();
    });

    let _tidy = use_listener::<FileTidyPromptEvent, _>(FILE_TIDY_PROMPT_EVENT, move |e| {
        tidy_prompt.set(Some(e.count));
    });

    let _meta = use_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        error.set(String::new());
        clear_preview(preview, thumbs);
        media.set(None);
        viewport.reset();
        last_scroll_req.set(0);
        doc_title.set(m.path.rsplit('/').next().unwrap_or(&m.path).to_string());
        path.set(m.path);
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
        if git_path() != m.abs_path {
            git_has_diff.set(false);
            git_line_markers.set(HashMap::new());
        }
        git_path.set(m.abs_path);
        total_lines.set(m.total_lines);
        language.set(m.language);
        indent.set(m.indent);
        line_ending.set(m.line_ending);
        mode.set(Mode::Text);
        lsp_install_notice.set(None);
        lsp_install_request.set(None);
        lsp_notice_generation.set(lsp_notice_generation().wrapping_add(1));
        explorer.show_if_room(mode);
        note_blocks.set(Vec::new());
        note_properties.set(Vec::new());
        note_references.set(Vec::new());
        note_active.set(None);
        note_editing.set(false);
        note_edit_line.set(None);
        note_dragging.set(false);
        editor_dragging.set(false);
        editor_drag_origin.set(None);
        git_nonce.set(git_nonce() + 1);
    });

    let _vp = use_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
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
    });

    let _cur = use_listener::<FileCursorEvent, _>(FILE_CURSOR_EVENT, move |c| {
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
        if ed_command_line.peek().ne(&c.command_line) {
            ed_command_line.set(c.command_line.clone());
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
        let note_mode = *file_view_mode.peek() == FileViewMode::Note
            && is_markdown_file(git_path.peek().as_str());
        if note_mode {
            let active = note_block_index_for_line(&note_blocks.peek(), c.source_primary.line);
            if *keymap.peek() == vmux_core::KeymapKind::Vim
                && !*note_editing.peek()
                && let Some(index) = active
            {
                activate_note_cursor(
                    index,
                    c.source_primary.line,
                    note_active,
                    note_editing,
                    note_edit_line,
                );
            }
            if *note_editing.peek() {
                let is_list = active.is_some_and(|index| {
                    matches!(note_blocks.peek()[index].block, MdBlock::List { .. })
                });
                let edit_line = is_list.then_some(c.source_primary.line);
                if *note_edit_line.peek() != edit_line {
                    note_edit_line.set(edit_line);
                }
            }
            let active = active.map(|index| index as u32);
            if *note_active.peek() != active {
                note_active.set(active);
            }
            if moved && let Some(index) = active {
                ensure_note_caret_visible(index as usize, c.source_primary.line);
            }
        }
        if moved && !note_mode {
            viewport.reveal_caret();
        }
    });

    let _scroll_by = use_listener::<FileScrollByEvent, _>(FILE_SCROLL_BY_EVENT, move |event| {
        let line_height = if file_view_mode() == FileViewMode::Note {
            28.0
        } else {
            cell_dims().1
        };
        if line_height <= 0.0 {
            return;
        }
        viewport.scroll_by(event.lines, line_height);
    });

    let _dirty = use_listener::<FileDirtyEvent, _>(FILE_DIRTY_EVENT, move |d| {
        dirty.set(d.dirty);
        schedule_git_refresh(git_refresh_generation, git_nonce);
    });

    let _git_changed = use_listener::<GitChangedEvent, _>(GIT_CHANGED_EVENT, move |_| {
        schedule_git_refresh(git_refresh_generation, git_nonce);
    });

    let _view_mode = use_listener::<FileViewModeEvent, _>(FILE_VIEW_MODE_EVENT, move |event| {
        if file_view_mode() != event.mode && event.mode != FileViewMode::Note {
            note_editing.set(false);
        }
        file_view_mode.set(event.mode);
        match event.mode {
            FileViewMode::Note if is_markdown_file(&git_path()) => {
                let line = source_cursor().line;
                if let Some(index) = note_block_index_for_line(&note_blocks.read(), line) {
                    activate_note_cursor_centered(
                        index,
                        line,
                        note_active,
                        note_editing,
                        note_edit_line,
                    );
                }
            }
            FileViewMode::Editor => {
                viewport.center_row(cursor().row, cell_dims().1);
            }
            _ => {}
        }
    });

    let _keymap = use_listener::<FileKeymapEvent, _>(FILE_KEYMAP_EVENT, move |event| {
        keymap.set(event.keymap);
        if event.keymap == vmux_core::KeymapKind::Vim
            && file_view_mode() == FileViewMode::Note
            && is_markdown_file(&git_path())
        {
            let line = source_cursor().line;
            if let Some(index) = note_block_index_for_line(&note_blocks.read(), line) {
                activate_note_cursor_centered(
                    index,
                    line,
                    note_active,
                    note_editing,
                    note_edit_line,
                );
            }
        }
    });

    let _note = use_listener::<FileNoteEvent, _>(FILE_NOTE_EVENT, move |event| {
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
            note_block_index_for_line(&blocks, line).map(|index| (activation, index, line))
        });
        note_blocks.set(blocks);
        note_properties.set(properties);
        note_references.set(references);
        note_active.set(active);
        if let Some((activation, index, line)) = activation {
            match activation {
                NoteCursorActivation::Center(_) => activate_note_cursor_centered(
                    index,
                    line,
                    note_active,
                    note_editing,
                    note_edit_line,
                ),
                NoteCursorActivation::PreserveViewport(_) => {
                    activate_note_cursor(index, line, note_active, note_editing, note_edit_line)
                }
            }
        }
    });

    let _hov = use_listener::<FileHoverEvent, _>(FILE_HOVER_EVENT, move |h| {
        lsp_hover.set(Some(h));
    });

    let _refs = use_listener::<FileReferencesEvent, _>(FILE_REFERENCES_EVENT, move |e| {
        refs.set(e.items);
        refs_sel.set(0);
        refs_open.set(true);
        FocusClaim::new("refs-panel").request();
    });

    let _comp = use_listener::<FileCompletionEvent, _>(FILE_COMPLETION_EVENT, move |e| {
        comp_open.set(!e.items.is_empty());
        comps.set(e.items);
        comp_sel.set(0);
        comp_anchor.set((e.line, e.replace_from_col));
    });

    let _diag = use_listener::<FileDiagnosticsEvent, _>(FILE_DIAGNOSTICS_EVENT, move |d| {
        if d.path != git_path() {
            return;
        }
        diagnostics.set(d.diagnostics);
    });

    let _lsp_status = use_listener::<FileLspStatusEvent, _>(FILE_LSP_STATUS_EVENT, move |s| {
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
    });

    let _lsp_install_progress =
        use_listener::<LspInstallProgress, _>(LSP_INSTALL_PROGRESS_EVENT, move |progress| {
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
        });

    let _lsp_package_status =
        use_listener::<LspPkgStatusEvent, _>(LSP_PKG_STATUS_EVENT, move |status| {
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
        });

    let _err = use_listener::<FileErrorEvent, _>(FILE_ERROR_EVENT, move |e| {
        error.set(e.message);
    });

    let _code_actions =
        use_listener::<FileCodeActionsEvent, _>(FILE_CODE_ACTIONS_EVENT, move |e| {
            code_action_sel.set(0);
            code_actions.set(e.titles);
        });

    let _rename_begin =
        use_listener::<FileRenameBeginEvent, _>(FILE_RENAME_BEGIN_EVENT, move |e| {
            rename_failed.set(String::new());
            rename_box.set(Some(RenameBox {
                line: e.line,
                col: e.col,
                original: e.current.clone(),
                draft: e.current,
            }));
        });

    let _rename_failed = use_listener::<FileEditFailedEvent, _>(FILE_EDIT_FAILED_EVENT, move |e| {
        rename_failed.set(e.reason);
        let id = rename_failed_generation().wrapping_add(1);
        rename_failed_generation.set(id);
        spawn(async move {
            sleep_ms(RENAME_NOTICE_MS).await;
            if rename_failed_generation() == id {
                rename_failed.set(String::new());
            }
        });
    });

    let _dir = use_listener::<FileDirEvent, _>(FILE_DIR_EVENT, move |d| {
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
    });

    let _media = use_listener::<FileMediaEvent, _>(FILE_MEDIA_EVENT, move |e| {
        error.set(String::new());
        clear_preview(preview, thumbs);
        let kind = e.kind;
        media.set(Some(e));
        mode.set(Mode::Media(kind));
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
    });

    let _prev = use_listener::<FilePreviewEvent, _>(FILE_PREVIEW_EVENT, move |ev| {
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
            PreviewKind::Image { bytes, .. } => Preview::Image(image_data_url(&bytes, &ev.path)),
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
    });

    let _theme = use_listener::<FileThemeEvent, _>(FILE_THEME_EVENT, move |t| {
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
    });

    use_effect(move || {
        explorer.sync();
        viewport
            .geometry
            .read()
            .announce(cell_dims(), total_lines(), last_resize);
    });

    use_effect(move || match mode() {
        Mode::Text if file_view_mode() == FileViewMode::Note && is_markdown_file(&git_path()) => {
            if note_editing() {
                focus_file_input();
            } else {
                focus_container();
            }
        }
        Mode::Text => focus_file_input(),
        Mode::Dir | Mode::Media(_) => focus_container(),
    });

    let gw = gutter_width(total_lines());
    let cur_basename = path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let measure_text = vec!["X".repeat(MEASURE_COLS); MEASURE_ROWS].join("\n");
    let comp_filtered: Vec<CompletionItem> = comp_filtered();
    let comp_sel_clamped = comp_sel().min(comp_filtered.len().saturating_sub(1));

    rsx! {
        if !doc_title().is_empty() {
        }
        div {
            id: PAGE_ID,
            class: "flex h-full w-full flex-row overflow-hidden bg-background",
            onresize: move |event: Event<ResizeData>| {
                let Ok(size) = event.get_border_box_size() else {
                    return;
                };
                page_width.set(size.width.max(0.0) as u32);
            },
            onmousemove: move |e: Event<MouseData>| {
                if explorer_resizing() {
                    let x = e.client_coordinates().x as i32;
                    explorer_width.set((x.max(0) as u32).clamp(160, 600));
                }
            },
            onmouseup: move |_| {
                note_dragging.set(false);
                editor_dragging.set(false);
                editor_drag_origin.set(None);
                if explorer_resizing() {
                    explorer_resizing.set(false);
                    let _ = send(&ExplorerPanelWidth { px: explorer_width() });
                }
            },

            ExplorerSidebar {
                visible: explorer_visible,
                width: explorer_width,
                resizing: explorer_resizing,
            }

        div {
            id: CONTAINER_ID,
            tabindex: "0",
            class: "relative flex h-full min-w-0 flex-1 flex-col overflow-hidden bg-background text-foreground font-mono text-sm leading-normal",
            style: "outline:none;background-image:radial-gradient(120% 80% at 50% -10%, rgba(34,211,238,0.05), transparent 60%);{theme_style}",

            onmousedown: move |e: Event<MouseData>| {
                match mode() {
                    Mode::Text => {
                        e.prevent_default();
                        if file_view_mode() == FileViewMode::Note
                            && is_markdown_file(&git_path())
                        {
                            if note_editing() {
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
                if keys.offer(&e) {
                    return;
                }
                let key = e.key().to_string();
                if mode() == Mode::Text
                    && file_view_mode() == FileViewMode::Note
                    && is_markdown_file(&git_path())
                    && !note_editing()
                {
                    let _ = forward_file_key(&e, ed_mode());
                    return;
                }
                match mode() {
                    Mode::Dir => {
                        let vis = visible_entries(&dir_entries.read(), show_hidden());
                        let len = vis.len();
                        let cur = selected();
                        match key.as_str() {
                            "j" | "ArrowDown" => {
                                e.prevent_default();
                                let next = if len == 0 { 0 } else { (cur + 1).min(len - 1) };
                                selected.set(next);
                                scroll_dir_row_into_view(next);
                                if let Some(p) = vis.get(next).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            "k" | "ArrowUp" => {
                                e.prevent_default();
                                let next = cur.saturating_sub(1);
                                selected.set(next);
                                scroll_dir_row_into_view(next);
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
                            "h" | "ArrowLeft" | "Escape" => {
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
                                scroll_dir_row_into_view(idx);
                                if let Some(p) = vis2.get(idx).map(|x| x.path.clone()) {
                                    request_preview(p);
                                }
                            }
                            " " => {
                                e.prevent_default();
                                toggle_preview_video();
                            }
                            _ => {}
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
                style: "position:absolute;top:0;left:0;visibility:hidden;white-space:pre;font:inherit",
                onresize: move |event: Event<ResizeData>| {
                    let Ok(size) = event.get_border_box_size() else {
                        return;
                    };
                    let cell = (
                        size.width / MEASURE_COLS as f64,
                        size.height / MEASURE_ROWS as f64,
                    );
                    let (cw, ch) = *cell_dims.peek();
                    if (cw - cell.0).abs() > 0.01 || (ch - cell.1).abs() > 0.01 {
                        cell_dims.set(cell);
                    }
                },
                {measure_text}
            }

            div {
                class: "flex h-9 shrink-0 items-center gap-2 border-b border-foreground/[0.07] bg-foreground/[0.06] px-4 font-sans text-xs text-muted-foreground",
                ExplorerToggleButton { pane: explorer, mode }
                {rsx! { TypeIcon { path: cur_basename.clone(), is_dir: mode() == Mode::Dir, class: "h-4 w-4 shrink-0 text-foreground/80" } }}
                span { class: "truncate text-foreground/90", "{cur_basename}" }
                if dirty() {
                    span { class: "h-1.5 w-1.5 shrink-0 rounded-full bg-cyan-300", title: translate("editor-unsaved") }
                }
                div { class: "flex-1" }
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
                    if is_markdown_file(&git_path()) || git_has_diff() {
                        div { class: "flex shrink-0 items-center gap-0.5 rounded-md bg-foreground/[0.06] p-0.5 text-[10px] font-medium ring-1 ring-inset ring-foreground/10",
                            if is_markdown_file(&git_path()) {
                                button {
                                    class: file_mode_class(file_view_mode() == FileViewMode::Note),
                                    title: translate("editor-rendered-markdown"),
                                    onclick: move |_| {
                                        file_view_mode.set(FileViewMode::Note);
                                        let _ = send(&FileViewModeSet { mode: FileViewMode::Note });
                                        let line = source_cursor().line;
                                        if let Some(index) = note_block_index_for_line(&note_blocks.read(), line) {
                                            activate_note_cursor_centered(
                                                index,
                                                line,
                                                note_active,
                                                note_editing,
                                                note_edit_line,
                                            );
                                        }
                                    },
                                    {translate("editor-note")}
                                }
                            }
                            button {
                                class: file_mode_class(
                                    file_view_mode() == FileViewMode::Editor
                                        || (file_view_mode() == FileViewMode::Note
                                            && !is_markdown_file(&git_path())),
                                ),
                                title: translate("editor-source-editor"),
                                onclick: move |_| {
                                    note_editing.set(false);
                                    file_view_mode.set(FileViewMode::Editor);
                                    viewport.center_row(cursor().row, cell_dims().1);
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
                                    && is_markdown_file(&git_path())
                                    && !note_editing()
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
                                    && is_markdown_file(&git_path())
                                    && !note_editing()
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
                GitBar {
                    path: git_path,
                    has_diff: git_has_diff,
                    nonce: git_nonce,
                    branch: git_branch,
                    ahead: git_ahead,
                    behind: git_behind,
                    staged_count: git_staged,
                    message: git_message,
                }
                {
                    tidy_prompt().map(|count| {
                        rsx! {
                            div {
                                class: "flex shrink-0 items-center gap-1.5 text-[11px]",
                                span {
                                    class: "select-none text-cyan-700 dark:text-cyan-200",
                                    {translate_with(
                                        "editor-unchanged-previews",
                                        &[("count", TranslationValue::Number(count as i64))],
                                    )}
                                }
                                button {
                                    class: "rounded-full bg-cyan-400/20 px-2 py-0.5 font-medium text-cyan-700 hover:bg-cyan-400/30 dark:text-cyan-100",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyActionEvent { choice: TidyChoice::Tidy });
                                        tidy_prompt.set(None);
                                    },
                                    {translate("editor-tidy")}
                                }
                                button {
                                    class: "rounded-full px-2 py-0.5 text-foreground/60 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyActionEvent { choice: TidyChoice::Always });
                                        tidy_prompt.set(None);
                                    },
                                    {translate("editor-always")}
                                }
                                button {
                                    class: "rounded-full px-1.5 py-0.5 text-foreground/40 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyActionEvent { choice: TidyChoice::Dismiss });
                                        tidy_prompt.set(None);
                                    },
                                    "\u{2715}"
                                }
                            }
                        }
                    })
                }
            }

            {
                let msg = error.read().clone();
                (!msg.is_empty()).then(|| rsx! {
                    div {
                        class: "absolute inset-0 z-50 flex items-center justify-center",
                        style: "background:rgba(0,0,0,0.6);",
                        div {
                            class: "rounded-md border border-ansi-1 bg-background px-4 py-2 text-sm text-ansi-1",
                            "{msg}"
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
                                    img { src: "{m.url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20" }
                                },
                                MediaKind::Video => rsx! {
                                    video {
                                        src: "{m.url}",
                                        controls: true,
                                        autoplay: false,
                                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20",
                                    }
                                },
                                MediaKind::Audio => rsx! {
                                    audio { src: "{m.url}", controls: true, class: "w-2/3" }
                                },
                                MediaKind::Pdf => {
                                    let display = path();
                                    let abs = m.abs_path.clone();
                                    rsx! {
                                        div { class: "flex flex-col items-center gap-3 rounded-2xl bg-white/[0.03] px-8 py-6 ring-1 ring-inset ring-cyan-400/15 backdrop-blur-2xl",
                                            span { class: "text-xs uppercase tracking-wide text-foreground/70", "PDF" }
                                            span { class: "max-w-md truncate text-sm text-foreground/90", "{display}" }
                                            button {
                                                class: "rounded-lg bg-cyan-400/15 px-3 py-1.5 text-xs font-semibold text-cyan-200 hover:bg-cyan-400/25",
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
                    div {
                        class: "grid min-h-0 flex-1 gap-3 p-3",
                        style: "grid-template-columns: minmax(8rem,14rem) minmax(10rem,1fr) minmax(12rem,1.3fr);",

                        div { class: PANE_CLASS,
                            for e in visible_entries(&parent_entries(), show_hidden()) {
                                div {
                                    key: "{e.path}",
                                    class: if e.name == cur_basename { "flex items-center gap-2 rounded-md bg-cyan-400/10 px-2 py-1 text-foreground shadow-[inset_2px_0_0_0_rgba(34,211,238,0.6)]" } else { "flex items-center gap-2 rounded-md px-2 py-1 text-foreground/45 transition-colors hover:bg-foreground/[0.04]" },
                                    EntryVisual { entry: e.clone(), thumb: None }
                                    span { class: "truncate text-xs", "{e.name}" }
                                }
                            }
                        }

                        div { class: PANE_CLASS,
                            for (i, e) in visible_entries(&dir_entries(), show_hidden()).into_iter().enumerate() {
                                {
                                    let p_sel = e.path.clone();
                                    let p_open = e.path.clone();
                                    let is_dir = e.is_dir;
                                    let thumb = thumbs().get(&e.path).cloned();
                                    rsx! {
                                        div {
                                            key: "{e.path}",
                                            id: "dir-row-{i}",
                                            class: row_class(i == selected()),
                                            title: "{e.path}",
                                            onclick: move |_| {
                                                selected.set(i);
                                                request_preview(p_sel.clone());
                                            },
                                            ondoubleclick: move |_| {
                                                if !is_dir {
                                                    back_dir.set(Some(parent_of(&p_open)));
                                                }
                                                open_path(p_open.clone());
                                            },
                                            EntryVisual { entry: e.clone(), thumb: thumb.clone() }
                                            span { class: "truncate text-xs", "{e.name}" }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "flex min-h-0 items-center justify-center overflow-auto rounded-2xl bg-foreground/[0.02] p-4 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]",
                            PreviewPane { preview: preview() }
                        }
                    }
                },
                Mode::Text => rsx! {
                    if git_has_diff() {
                        DiffView {
                            path: git_path,
                            nonce: git_nonce,
                            visible: file_view_mode() == FileViewMode::Diff,
                            markers: git_line_markers,
                        }
                    }
                    if file_view_mode() == FileViewMode::Note && is_markdown_file(&git_path()) {
                        {
                            let active = note_active();
                            let block_count = note_blocks.read().len();
                            rsx! {
                                div {
                                    id: SCROLL_ID,
                                    class: "file-mode-note-enter min-h-0 flex-1 overflow-auto px-8 py-8",
                                    onclick: move |event| {
                                        if keymap() == vmux_core::KeymapKind::Vim {
                                            event.prevent_default();
                                            let line = source_cursor().line;
                                            if let Some(index) = note_block_index_for_line(&note_blocks.read(), line) {
                                                activate_note_cursor(
                                                    index,
                                                    line,
                                                    note_active,
                                                    note_editing,
                                                    note_edit_line,
                                                );
                                            }
                                            return;
                                        }
                                        if note_editing() {
                                            note_editing.set(false);
                                            note_active.set(None);
                                            note_edit_line.set(None);
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
                                        for index in 0..block_count {
                                            {
                                                let editing =
                                                    note_editing() && Some(index as u32) == active;
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
                                                        note_active,
                                                        note_editing,
                                                        note_edit_line,
                                                        note_dragging,
                                                        comp_open: editing && comp_open(),
                                                        comp_filtered: if editing {
                                                            comp_filtered.clone()
                                                        } else {
                                                            Vec::new()
                                                        },
                                                        comp_sel_clamped,
                                                    }
                                                }
                                            }
                                        }
                                        textarea {
                                            id: INPUT_ID,
                                            onmounted: move |event: Event<MountedData>| {
                                                viewport.field_mounted(event.data());
                                            },
                                            class: "pointer-events-none absolute left-0 top-0 h-px w-px resize-none overflow-hidden border-0 bg-transparent p-0 opacity-0 outline-none",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| composing.set(true),
                                            oncompositionend: move |event: Event<CompositionData>| {
                                                composing.set(false);
                                                send_committed_text(event.data().data());
                                            },
                                            oninput: move |event: Event<FormData>| {
                                                if composing() {
                                                    return;
                                                }
                                                send_committed_text(event.value());
                                            },
                                            onkeydown: move |event: Event<KeyboardData>| {
                                                event.stop_propagation();
                                                if composing() {
                                                    return;
                                                }
                                                if keys.offer(&event) {
                                                    return;
                                                }
                                                if event.key() == Key::Escape {
                                                    event.prevent_default();
                                                    if keymap() != vmux_core::KeymapKind::Vim {
                                                        note_editing.set(false);
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
                            let (cw, ch) = cell_dims();
                            let gutter = gw as f64 * cw + 48.0;
                            let cx = gutter + cursor().col as f64 * cw;
                            let cy = cursor().row as f64 * ch;
                            let cursor_style = if ed_mode().accepts_text() {
                                format!(
                                    "left:{cx}px;top:{cy}px;height:{ch}px;width:2px;background:currentColor;"
                                )
                            } else {
                                format!(
                                    "left:{cx}px;top:{cy}px;height:{ch}px;width:{}px;background:color-mix(in srgb,currentColor 28%,transparent);outline:1px solid currentColor;",
                                    cw.max(2.0)
                                )
                            };
                            let cursor_key =
                                format!("{}:{}:{:?}", cursor().row, cursor().col, ed_mode());
                            let spacer = total_rows() as f64 * ch;
                            let txtcol = if composing() { "inherit" } else { "transparent" };
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
                                        viewport.mounted(event.data());
                                    },
                                    onresize: move |event: Event<ResizeData>| {
                                        let Ok(size) = event.get_border_box_size() else {
                                            return;
                                        };
                                        viewport.resized((size.width, size.height));
                                    },
                                    onscroll: move |event: Event<ScrollData>| {
                                        viewport.scrolled_to((
                                            event.scroll_left(),
                                            event.scroll_top(),
                                        ));
                                        let (_, ch) = cell_dims();
                                        if ch <= 0.0 {
                                            return;
                                        }
                                        let vis_first = (event.scroll_top() / ch).floor().max(0.0) as u32;
                                        let vis_rows = (event.client_height() as f64 / ch).ceil() as u32 + 1;
                                        let trigger = (vis_rows as f32 * vmux_core::scroll::EDGE_TRIGGER_K).ceil() as u32;
                                        let rfirst = first_row();
                                        let loaded_len = line_layouts
                                            .read()
                                            .last()
                                            .map_or(0, |line| line.row + line.rows as u32 - rfirst);
                                        if vmux_core::scroll::needs_refetch(vis_first, vis_rows, rfirst, loaded_len, trigger)
                                            && last_scroll_req() != vis_first
                                        {
                                            last_scroll_req.set(vis_first);
                                            let _ = send(&FileScrollEvent { top_row: vis_first });
                                        }
                                    },
                                    StickyScope {
                                        lines: sticky_lines(),
                                        cell_height: ch,
                                        gutter_chars: gw,
                                        on_pick: move |row: u32| viewport.center_row(row, ch),
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
                                                let left = gutter + s.start as f64 * cw;
                                                let w = (s.end.saturating_sub(s.start)) as f64 * cw;
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
                                                let left = gutter + s.start as f64 * cw;
                                                let w = (s.end.saturating_sub(s.start)) as f64 * cw;
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
                                                let left = gutter + s.start as f64 * cw;
                                                let style = if s.end == u32::MAX {
                                                    format!("left:{left}px;top:{top}px;height:{ch}px;right:0;")
                                                } else {
                                                    let w = (s.end.saturating_sub(s.start)) as f64 * cw;
                                                    format!("left:{left}px;top:{top}px;height:{ch}px;width:{w}px;")
                                                };
                                                rsx! {
                                                    div {
                                                        key: "sel{s.row}:{s.start}:{s.end}",
                                                        class: "pointer-events-none absolute z-0 bg-cyan-400/20",
                                                        style: "{style}",
                                                    }
                                                }
                                            }
                                        }

                                        div {
                                            key: "{cursor_key}",
                                            class: "pointer-events-none absolute z-20 rounded-[1px]",
                                            style: "{cursor_style}",
                                        }

                                        for extra in carets().iter().filter(|c| **c != cursor()) {
                                            {
                                                let ex = gutter + extra.col as f64 * cw;
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
                                            onmounted: move |event: Event<MountedData>| {
                                                viewport.field_mounted(event.data());
                                            },
                                            class: "absolute z-10 resize-none overflow-hidden whitespace-pre border-0 bg-transparent p-0 caret-transparent outline-none",
                                            style: "left:{cx}px;top:{cy}px;min-width:2ch;height:{ch}px;color:{txtcol};",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| composing.set(true),
                                            oncompositionend: move |event: Event<CompositionData>| {
                                                composing.set(false);
                                                send_committed_text(event.data().data());
                                            },
                                            oninput: move |event: Event<FormData>| {
                                                if composing() {
                                                    return;
                                                }
                                                send_committed_text(event.value());
                                            },
                                            onkeydown: move |e: Event<KeyboardData>| {
                                                e.stop_propagation();
                                                if composing() {
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
                                                let (cw, ch) = cell_dims();
                                                let Some(i) = lines().iter().position(|l| l.line_no == h.line) else {
                                                    return rsx! {};
                                                };
                                                let hrow = first_row() + i as u32;
                                                let top = hrow as f64 * ch + ch;
                                                let left = gw as f64 * cw + 48.0 + h.col as f64 * cw;
                                                rsx! {
                                                    div {
                                                        class: "absolute z-30 max-h-64 max-w-2xl overflow-auto rounded-xl bg-foreground/[0.05] px-3 py-2 text-xs leading-snug text-foreground/90 ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
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

                                        {
                                            (!code_actions().is_empty()).then(|| {
                                                let titles = code_actions();
                                                let chosen = code_action_sel().min(titles.len() - 1);
                                                let top = cursor().row as f64 * ch + ch;
                                                let left = gutter + cursor().col as f64 * cw;
                                                rsx! {
                                                    div {
                                                        id: CODE_ACTION_ID,
                                                        tabindex: 0,
                                                        autofocus: true,
                                                        class: "absolute z-50 max-h-56 min-w-64 overflow-auto rounded-lg bg-background/95 py-1 text-xs text-foreground outline-none ring-1 ring-inset ring-cyan-400/30 backdrop-blur-2xl shadow-lg",
                                                        style: "left:{left}px;top:{top}px;",
                                                        onkeydown: move |e| {
                                                            e.stop_propagation();
                                                            let len = code_actions().len();
                                                            match e.key() {
                                                                Key::ArrowDown => {
                                                                    e.prevent_default();
                                                                    code_action_sel.set((chosen + 1) % len);
                                                                }
                                                                Key::ArrowUp => {
                                                                    e.prevent_default();
                                                                    code_action_sel.set((chosen + len - 1) % len);
                                                                }
                                                                Key::Enter => {
                                                                    e.prevent_default();
                                                                    let _ = send(&FileCodeActionPick { index: chosen as u32 });
                                                                    code_actions.set(Vec::new());
                                                                    focus_file_input();
                                                                }
                                                                Key::Escape => {
                                                                    e.prevent_default();
                                                                    code_actions.set(Vec::new());
                                                                    focus_file_input();
                                                                }
                                                                _ => {}
                                                            }
                                                        },
                                                        onblur: move |_| code_actions.set(Vec::new()),
                                                        for (i, title) in titles.iter().enumerate() {
                                                            div {
                                                                key: "{i}",
                                                                class: if i == chosen { "cursor-default px-3 py-1 bg-cyan-400/15" } else { "cursor-default px-3 py-1" },
                                                                onmousedown: move |e: Event<MouseData>| {
                                                                    e.prevent_default();
                                                                    let _ = send(&FileCodeActionPick { index: i as u32 });
                                                                    code_actions.set(Vec::new());
                                                                    focus_file_input();
                                                                },
                                                                "{title}"
                                                            }
                                                        }
                                                    }
                                                }
                                            })
                                        }

                                        {
                                            rename_box().map(|box_| {
                                                let top = box_.line as f64 * ch + ch;
                                                let left = gutter + box_.col as f64 * cw;
                                                rsx! {
                                                    input {
                                                        id: RENAME_ID,
                                                        autofocus: true,
                                                        spellcheck: false,
                                                        autocomplete: "off",
                                                        class: "absolute z-50 min-w-32 rounded-md bg-background/95 px-2 py-1 text-xs text-foreground ring-1 ring-inset ring-cyan-400/40 outline-none backdrop-blur-2xl shadow-lg",
                                                        style: "left:{left}px;top:{top}px;",
                                                        value: "{box_.draft}",
                                                        oninput: move |e| {
                                                            if let Some(open) = rename_box.write().as_mut() {
                                                                open.draft = e.value();
                                                            }
                                                        },
                                                        onkeydown: move |e| {
                                                            e.stop_propagation();
                                                            match e.key() {
                                                                Key::Enter => {
                                                                    e.prevent_default();
                                                                    if let Some(open) = rename_box() {
                                                                        open.submit();
                                                                    }
                                                                    rename_box.set(None);
                                                                    focus_file_input();
                                                                }
                                                                Key::Escape => {
                                                                    e.prevent_default();
                                                                    rename_box.set(None);
                                                                    focus_file_input();
                                                                }
                                                                _ => {}
                                                            }
                                                        },
                                                        onblur: move |_| {
                                                            rename_box.set(None);
                                                        },
                                                    }
                                                }
                                            })
                                        }

                                        {
                                            (comp_open() && !comp_filtered.is_empty()).then(|| {
                                                let (cline, cfrom) = comp_anchor();
                                                let top = cline as f64 * ch + ch;
                                                let left = gutter + cfrom as f64 * cw;
                                                rsx! {
                                                    div {
                                                        class: "absolute z-40 max-h-56 min-w-48 overflow-auto rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                                                        style: "left:{left}px;top:{top}px;",
                                                        for (i, it) in comp_filtered.iter().enumerate() {
                                                            div {
                                                                key: "{i}",
                                                                class: if i == comp_sel_clamped { "flex items-center gap-2 px-3 py-1 bg-cyan-400/15" } else { "flex items-center gap-2 px-3 py-1" },
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
                        _ => ("text-cyan-400", "", true),
                    };
                    let detail = progress.pct.map_or_else(
                        || progress.message.clone(),
                        |percent| format!("{} {percent}%", progress.message),
                    );
                    rsx! {
                        div {
                            class: "pointer-events-none fixed right-4 bottom-14 z-[60] flex min-w-64 max-w-sm items-center gap-3 rounded-xl bg-background/95 px-3 py-2.5 text-xs text-foreground shadow-[0_12px_40px_rgba(0,0,0,0.28)] ring-1 ring-inset ring-foreground/10 backdrop-blur-xl",
                            if spinning {
                                span { class: "h-4 w-4 shrink-0 animate-spin rounded-full border-2 border-cyan-400/25 border-t-cyan-400" }
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
                        class: "pointer-events-none absolute right-4 bottom-12 z-50 max-w-md rounded-xl bg-foreground/[0.04] px-3 py-2 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
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

            {
                ctx_menu().map(|(x, y, line, col)| rsx! {
                    div {
                        class: "fixed inset-0 z-40",
                        onmousedown: move |_| ctx_menu.set(None),
                        oncontextmenu: move |e| {
                            e.prevent_default();
                            ctx_menu.set(None);
                        },
                    }
                    div {
                        class: "fixed z-50 min-w-56 overflow-hidden rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                        style: "left:{x}px;top:{y}px;",
                        for (i, row) in EditorMenu::offering(&lsp_actions()).rows().into_iter().enumerate() {
                            div {
                                key: "{i}",
                                class: if row.opens_group && i > 0 {
                                    "mt-1 flex cursor-default items-center gap-6 border-t border-foreground/10 px-3 pt-2 pb-1.5 hover:bg-cyan-400/15"
                                } else {
                                    "flex cursor-default items-center gap-6 px-3 py-1.5 hover:bg-cyan-400/15"
                                },
                                onmousedown: move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    row.invoke(line, col);
                                    ctx_menu.set(None);
                                },
                                span { class: "grow whitespace-nowrap", {translate(row.label)} }
                                span { class: "shrink-0 text-[10px] text-foreground/40", "{row.shortcut}" }
                            }
                        }
                    }
                })
            }

            {
                (!ed_command_line().is_empty()).then(|| rsx! {
                    div {
                        id: "vim-command-line",
                        class: "pointer-events-none absolute bottom-0 left-0 z-50 flex h-6 max-w-full items-center gap-px overflow-hidden bg-background/95 pl-2 pr-3 font-mono text-xs text-foreground",
                        span { class: "truncate", "{ed_command_line()}" }
                        span { class: "inline-block h-[1.05em] w-[0.5em] shrink-0 bg-foreground/70" }
                    }
                })
            }

            {
                refs_open().then(|| {
                    let items = refs();
                    rsx! {
                        div {
                            id: "refs-panel",
                            tabindex: "0",
                            class: "absolute bottom-8 left-4 right-4 z-40 max-h-64 overflow-auto rounded-xl bg-foreground/[0.05] p-1 text-xs text-foreground/90 outline-none ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                            onkeydown: move |e: Event<KeyboardData>| {
                                e.stop_propagation();
                                if keys.offer(&e) {
                                    return;
                                }
                                let key = e.key().to_string();
                                let len = refs.read().len();
                                match key.as_str() {
                                    "j" => {
                                        e.prevent_default();
                                        if len > 0 {
                                            refs_sel.set((refs_sel() + 1).min(len - 1));
                                        }
                                    }
                                    "k" => {
                                        e.prevent_default();
                                        refs_sel.set(refs_sel().saturating_sub(1));
                                    }
                                    _ => {}
                                }
                            },
                            div { class: "px-2 py-1 text-[10px] uppercase tracking-wide text-foreground/50",
                                {translate_with(
                                    "editor-references",
                                    &[("count", TranslationValue::Number(items.len() as i64))],
                                )}
                            }
                            for (i, it) in items.iter().enumerate() {
                                {
                                    let nav = (it.path.clone(), it.line, it.col);
                                    rsx! {
                                        div {
                                            key: "{i}",
                                            class: if i == refs_sel() { "flex gap-2 rounded px-2 py-1 bg-cyan-400/15" } else { "flex gap-2 rounded px-2 py-1 hover:bg-foreground/[0.05]" },
                                            onmousedown: move |e: Event<MouseData>| {
                                                e.prevent_default();
                                                let _ = send(&FileGotoRequest {
                                                    path: nav.0.clone(),
                                                    line: nav.1,
                                                    col: nav.2,
                                                });
                                                refs_open.set(false);
                                                focus_file_input();
                                            },
                                            span { class: "shrink-0 text-cyan-700/80 dark:text-cyan-300/80", "{it.display}" }
                                            span { class: "truncate text-foreground/60", "{it.preview}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            }

            GitFooter {
                path: git_path,
                branch: git_branch,
                ahead: git_ahead,
                behind: git_behind,
                staged_count: git_staged,
                message: git_message,
                always_visible: mode() == Mode::Text
                    && keymap() == vmux_core::KeymapKind::Vim,
                leading: rsx! {
                    {
                        let lbl = ed_label();
                        (!lbl.is_empty()
                            && mode() == Mode::Text
                            && keymap() == vmux_core::KeymapKind::Vim)
                            .then(|| rsx! {
                                span {
                                    class: "-ml-4 flex h-7 shrink-0 items-center bg-cyan-400/20 px-3 text-[10px] font-semibold tracking-wider text-cyan-700 dark:text-cyan-100",
                                    "{lbl}"
                                }
                            })
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
                FileStatusInfo {
                    line: cursor().line + 1,
                    col: cursor().col + 1,
                    indent: indent(),
                    line_ending: line_ending(),
                    language: language(),
                }
            }
        }
        }
    }
}

/// The list of rendered lines.
///
/// Every prop is a signal or a value only a resize changes, so moving the caret leaves them all
/// equal and Dioxus skips this whole subtree. Without the boundary a caret move re-diffs all
/// hundred rows to discover that none of them moved.
#[component]
fn EditorLines(
    lines: Signal<Vec<FileLine>>,
    line_layouts: Signal<Vec<FileLineLayout>>,
    first_row: Signal<u32>,
    diagnostics: Signal<Vec<FileDiagnostic>>,
    git_line_markers: Signal<HashMap<u32, EditorDiffMarker>>,
    wrap_columns: Signal<u16>,
    cell_height: f64,
    gutter_chars: usize,
    total_lines: Signal<u32>,
    cell_dims: Signal<(f64, f64)>,
    ctx_menu: Signal<Option<(f64, f64, u32, u32)>>,
    editor_dragging: Signal<bool>,
    editor_drag_origin: Signal<Option<(i32, i32)>>,
    gutter_hover: Signal<bool>,
    hover_pos: Signal<Option<(u32, u32)>>,
    lsp_hover: Signal<Option<FileHoverEvent>>,
    hover_diag: Signal<Option<FileDiagnostic>>,
) -> Element {
    let chunks = LineChunk::split(&lines(), &line_layouts(), first_row());
    let diags = diagnostics();
    let markers = git_line_markers();
    let wrap_cols = wrap_columns();
    rsx! {
        for chunk in chunks {
            EditorLineChunk {
                key: "{chunk.start}",
                rows: chunk.rows,
                diagnostics: diags.clone(),
                markers: markers.clone(),
                wrap_cols,
                cell_height,
                gutter_chars,
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
        }
    }
}

/// A fixed band of line numbers, so a chunk's identity survives scrolling.
///
/// The host replaces the whole window each time it sends one, so without this every row's props
/// are rebuilt and every row is diffed for a one-line move. Cutting on absolute line number keeps
/// the boundaries still while the window slides over them: the chunks that lost or gained a line
/// re-render, and the ones in the middle compare equal and are skipped whole.
struct LineChunk {
    start: u32,
    rows: Vec<(FileLine, FileLineLayout)>,
}

impl LineChunk {
    const LINES: u32 = 24;

    fn split(lines: &[FileLine], layouts: &[FileLineLayout], first_row: u32) -> Vec<Self> {
        let mut chunks: Vec<Self> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let layout = layouts.get(i).copied().unwrap_or(FileLineLayout {
                line_no: line.line_no,
                row: first_row + i as u32,
                rows: 1,
            });
            let start = line.line_no - (line.line_no % Self::LINES);
            if chunks.last().is_none_or(|chunk| chunk.start != start) {
                chunks.push(Self {
                    start,
                    rows: Vec::new(),
                });
            }
            if let Some(chunk) = chunks.last_mut() {
                chunk.rows.push((line.clone(), layout));
            }
        }
        chunks
    }
}

#[component]
fn EditorLineChunk(
    rows: Vec<(FileLine, FileLineLayout)>,
    diagnostics: Vec<FileDiagnostic>,
    markers: HashMap<u32, EditorDiffMarker>,
    wrap_cols: u16,
    cell_height: f64,
    gutter_chars: usize,
    total_lines: Signal<u32>,
    cell_dims: Signal<(f64, f64)>,
    ctx_menu: Signal<Option<(f64, f64, u32, u32)>>,
    editor_dragging: Signal<bool>,
    editor_drag_origin: Signal<Option<(i32, i32)>>,
    gutter_hover: Signal<bool>,
    hover_pos: Signal<Option<(u32, u32)>>,
    lsp_hover: Signal<Option<FileHoverEvent>>,
    hover_diag: Signal<Option<FileDiagnostic>>,
) -> Element {
    rsx! {
        for (line, layout) in rows.iter() {
            {
                let ln = line.line_no;
                let mut line_diags: Vec<FileDiagnostic> = Vec::new();
                for d in diagnostics.iter() {
                    if d.line == ln {
                        line_diags.push(d.clone());
                    }
                }
                rsx! {
                    EditorLineRow {
                        key: "{ln}",
                        line: line.clone(),
                        layout: *layout,
                        severity: line_severity(&diagnostics, ln),
                        diff_marker: markers.get(&(ln + 1)).copied(),
                        diagnostics: line_diags,
                        cell_height,
                        gutter_chars,
                        wrap_cols,
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
                }
            }
        }
    }
}

/// One rendered line of the file.
///
/// This is a component rather than inline rsx because scrolling re-sends the whole window: a
/// one-line move changes one row and leaves the other hundred identical. As a component each row
/// memoizes on its props, so the unchanged ones are skipped instead of rebuilding their rsx and
/// re-diffing every span.
#[component]
fn EditorLineRow(
    line: FileLine,
    layout: FileLineLayout,
    severity: Option<DiagSeverity>,
    diff_marker: Option<EditorDiffMarker>,
    diagnostics: Vec<FileDiagnostic>,
    cell_height: f64,
    gutter_chars: usize,
    wrap_cols: u16,
    total_lines: Signal<u32>,
    cell_dims: Signal<(f64, f64)>,
    ctx_menu: Signal<Option<(f64, f64, u32, u32)>>,
    editor_dragging: Signal<bool>,
    editor_drag_origin: Signal<Option<(i32, i32)>>,
    gutter_hover: Signal<bool>,
    hover_pos: Signal<Option<(u32, u32)>>,
    lsp_hover: Signal<Option<FileHoverEvent>>,
    hover_diag: Signal<Option<FileDiagnostic>>,
) -> Element {
    let mut ctx_menu = ctx_menu;
    let mut editor_dragging = editor_dragging;
    let mut editor_drag_origin = editor_drag_origin;
    let mut gutter_hover = gutter_hover;
    let mut hover_pos = hover_pos;
    let mut lsp_hover = lsp_hover;
    let mut hover_diag = hover_diag;
    let ln = line.line_no;
    let fold = line.fold;
    let ch = cell_height;
    let gw = gutter_chars;
    let lt = layout.row as f64 * ch;
    let line_height = layout.rows as f64 * ch;
    let text_class = if wrap_cols > 0 {
        "pointer-events-none relative whitespace-pre-wrap break-all pr-8"
    } else {
        "pointer-events-none relative whitespace-pre pr-8"
    };
    let text_style = if wrap_cols > 0 {
        format!("box-sizing:border-box;width:calc(var(--cw) * {wrap_cols} + 2rem);")
    } else {
        String::new()
    };
    rsx! {
        div {
            class: if let Some(marker) = diff_marker { "group flex items-start {diff_marker_row_class(marker)}" } else { "group flex items-start" },
            style: "position:absolute;left:0;right:0;top:{lt}px;height:{line_height}px;",
            onpointerdown: move |e: Event<PointerData>| {
                e.prevent_default();
                ctx_menu.set(None);
                let cell = cell_dims();
                let (_, col) = column_in_line(
                    e.element_coordinates(),
                    gutter_px(total_lines(), cell.0),
                    cell,
                    wrap_cols,
                    true,
                );
                let at = e.client_coordinates();
                editor_dragging.set(false);
                if e.modifiers().meta() {
                    editor_drag_origin.set(None);
                    let _ = send(&FileDefinitionRequest { line: ln, col });
                } else {
                    editor_drag_origin.set(Some((at.x as i32, at.y as i32)));
                    let _ = send(&FilePointerEvent {
                        line: ln,
                        col,
                        extend: e.modifiers().shift(),
                        add: e.modifiers().alt(),
                    });
                }
                focus_file_input();
            },
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                let cell = cell_dims();
                let (_, col) = column_in_line(
                    e.element_coordinates(),
                    gutter_px(total_lines(), cell.0),
                    cell,
                    wrap_cols,
                    true,
                );
                let at = e.client_coordinates();
                ctx_menu.set(Some((at.x, at.y, ln, col)));
            },
            onmousemove: move |e: Event<MouseData>| {
                let cell = cell_dims();
                let (x, col) = column_in_line(
                    e.element_coordinates(),
                    gutter_px(total_lines(), cell.0),
                    cell,
                    wrap_cols,
                    editor_dragging(),
                );
                if editor_dragging() {
                    e.prevent_default();
                    let _ = send(&FilePointerEvent {
                        line: ln,
                        col,
                        extend: true,
                        add: false,
                    });
                    return;
                }
                let in_gutter = x < 0.0;
                if gutter_hover() != in_gutter {
                    gutter_hover.set(in_gutter);
                }
                if in_gutter {
                    return;
                }
                if hover_pos() != Some((ln, col)) {
                    hover_pos.set(Some((ln, col)));
                    lsp_hover.set(None);
                    spawn(async move {
                        sleep_ms(HOVER_DELAY_MS).await;
                        if hover_pos() != Some((ln, col)) {
                            return;
                        }
                        let _ = send(&FileHoverRequest { line: ln, col });
                    });
                }
            },
            span {
                class: "sticky left-0 z-[1] relative flex shrink-0 select-none items-center justify-end bg-background pl-4 pr-5 tabular-nums",
                style: "min-width:calc(var(--cw, 1ch) * {gw} + 3rem);height:{ch}px;",
                if let Some(s) = severity {
                    span { class: "pointer-events-none absolute left-1 {severity_color_class(s)}", "●" }
                }
                span {
                    class: if let Some(marker) = diff_marker { "shrink-0 text-right opacity-90 {diff_marker_text_class(marker)}" } else { "shrink-0 text-right opacity-40 group-hover:opacity-90" },
                    style: "width:calc(var(--cw, 1ch) * {gw});",
                    "{ln + 1}"
                }
                span {
                    class: if let Some(marker) = diff_marker { "ml-1 w-[1ch] shrink-0 text-center font-semibold {diff_marker_text_class(marker)}" } else { "ml-1 w-[1ch] shrink-0" },
                    if let Some(marker) = diff_marker {
                        span { title: translate("editor-changed-line"), "{diff_marker_sign(marker)}" }
                    }
                }
                match fold {
                    FoldGutter::None => rsx! {},
                    _ => rsx! {
                        FoldMarker {
                            line: ln,
                            collapsed: fold == FoldGutter::Collapsed,
                            revealed: gutter_hover(),
                        }
                    },
                }
            }
            span { class: "{text_class}", style: "{text_style}",
                IndentGuides { levels: line.indent_levels }
                for (i, s) in line.spans.iter().enumerate() {
                    span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                }
                for (di, d) in diagnostics.iter().enumerate() {
                    {
                        let color = match d.severity {
                            DiagSeverity::Error => "rgb(239,68,68)",
                            DiagSeverity::Warning => "rgb(245,158,11)",
                            DiagSeverity::Info => "rgb(56,189,248)",
                            DiagSeverity::Hint => "rgb(34,211,238)",
                        };
                        let dc = d.clone();
                        rsx! {
                            span {
                                key: "d{di}",
                                style: squiggle_style(d.start_col, d.end_col, color),
                                onmouseenter: move |_| hover_diag.set(Some(dc.clone())),
                                onmouseleave: move |_| hover_diag.set(None),
                            }
                        }
                    }
                }
                if fold == FoldGutter::Collapsed {
                    span { class: "ml-1 rounded bg-white/10 px-1 text-foreground/40", "⋯" }
                }
            }
        }
    }
}

#[component]
fn FileStatusInfo(
    line: u32,
    col: u32,
    indent: vmux_core::event::FileIndent,
    line_ending: vmux_core::event::FileLineEnding,
    language: String,
) -> Element {
    let position = translate_with(
        "editor-status-position",
        &[
            ("line", TranslationValue::Number(i64::from(line))),
            ("col", TranslationValue::Number(i64::from(col))),
        ],
    );
    let indent_id = match indent.spaces {
        true => "editor-status-spaces",
        false => "editor-status-tabs",
    };
    let indent_label = translate_with(
        indent_id,
        &[("width", TranslationValue::Number(i64::from(indent.width)))],
    );
    let eol = match line_ending {
        vmux_core::event::FileLineEnding::Crlf => "CRLF",
        vmux_core::event::FileLineEnding::Lf => "LF",
    };
    rsx! {
        span { class: "flex shrink-0 items-center gap-3 tabular-nums", "{position}" }
        span { class: "shrink-0", "{indent_label}" }
        span { class: "shrink-0", "UTF-8" }
        span { class: "shrink-0", "{eol}" }
        if !language.is_empty() {
            span { class: "shrink-0", "{language}" }
        }
    }
}

#[component]
fn StickyScope(
    lines: Vec<FileLine>,
    cell_height: f64,
    gutter_chars: usize,
    on_pick: EventHandler<u32>,
) -> Element {
    if lines.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "sticky top-0 z-[12] h-0",
            div { class: "absolute inset-x-0 top-0 border-b border-foreground/10 bg-background/95 backdrop-blur",
                for line in lines {
                    StickyScopeRow {
                        key: "{line.line_no}",
                        line,
                        cell_height,
                        gutter_chars,
                        on_pick,
                    }
                }
            }
        }
    }
}

#[component]
fn StickyScopeRow(
    line: FileLine,
    cell_height: f64,
    gutter_chars: usize,
    on_pick: EventHandler<u32>,
) -> Element {
    let row = line.line_no;
    rsx! {
        div {
            class: "flex cursor-default whitespace-pre hover:bg-foreground/[0.05]",
            style: "height:{cell_height}px;",
            onclick: move |_| on_pick.call(row),
            span {
                class: "sticky left-0 flex shrink-0 select-none items-center justify-end bg-background pl-4 pr-5 tabular-nums text-muted-foreground/60",
                style: "min-width:calc(var(--cw, 1ch) * {gutter_chars} + 3rem);height:{cell_height}px;",
                span {
                    class: "shrink-0 text-right",
                    style: "width:calc(var(--cw, 1ch) * {gutter_chars});",
                    "{row + 1}"
                }
                span { class: "ml-1 w-[1ch] shrink-0" }
            }
            span { class: "pointer-events-none relative whitespace-pre pr-8",
                IndentGuides { levels: line.indent_levels }
                for (i, s) in line.spans.iter().enumerate() {
                    span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                }
            }
        }
    }
}

#[component]
fn IndentGuides(levels: u16) -> Element {
    rsx! {
        for level in 0..levels {
            div {
                key: "{level}",
                class: "pointer-events-none absolute inset-y-0 w-px bg-foreground/[0.09]",
                style: "left:calc(var(--cw, 1ch) * {u32::from(level) * 4});",
            }
        }
    }
}

#[component]
fn FoldMarker(line: u32, collapsed: bool, revealed: bool) -> Element {
    let tone = match collapsed {
        true => "text-foreground/60 opacity-100",
        false if revealed => "text-foreground/35 opacity-100",
        false => "text-foreground/35 opacity-0",
    };
    rsx! {
        span {
            class: "absolute right-0.5 flex h-full w-4 cursor-pointer items-center justify-center transition-opacity hover:!text-foreground {tone}",
            onmousedown: move |e: Event<MouseData>| {
                e.stop_propagation();
                e.prevent_default();
                let _ = send(&FileFoldToggle { line });
            },
            svg {
                class: if collapsed { "h-3 w-3 -rotate-90" } else { "h-3 w-3" },
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "m6 9 6 6 6-6" }
            }
        }
    }
}

const CONTAINER_ID: &str = "file-container";
const PAGE_ID: &str = "file-page";
const MEASURE_ID: &str = "file-measure";
const MEASURE_COLS: usize = 80;
const MEASURE_ROWS: usize = 8;
const NOTE_CARET_ID: &str = "note-caret";
const VIDEO_HOST_ID: &str = "vmux-video-host";
const INPUT_ID: &str = "file-input";
const RENAME_ID: &str = "file-rename";
const CODE_ACTION_ID: &str = "file-code-action";
pub(crate) const FIND_INPUT_ID: &str = "file-find-input";
const RENAME_NOTICE_MS: u32 = 2400;
const HOVER_DELAY_MS: u32 = 300;
const SCROLL_ID: &str = "file-scroll";
const GIT_REFRESH_DEBOUNCE_MS: u32 = 120;
const NOTE_MAX_CONTENT_WIDTH_PX: u32 = 768;
const LSP_NOTICE_DONE_MS: u32 = 2_500;
const LSP_NOTICE_FAILED_MS: u32 = 6_000;

std::thread_local! {}

#[derive(Clone, Copy, PartialEq)]
struct MenuRow {
    label: &'static str,
    shortcut: &'static str,
    action: Option<EditorAction>,
    opens_group: bool,
}

impl MenuRow {
    fn invoke(self, line: u32, col: u32) {
        match self.action {
            Some(action) => {
                let _ = send(&FileEditorAction { action, line, col });
            }
            None if self.label == "editor-go-to-definition" => {
                let _ = send(&FileDefinitionRequest { line, col });
            }
            None => {
                let _ = send(&FileReferencesRequest { line, col });
            }
        }
    }
}

struct EditorMenu {
    offered: Vec<EditorAction>,
}

impl EditorMenu {
    fn offering(offered: &[EditorAction]) -> Self {
        Self {
            offered: offered.to_vec(),
        }
    }

    fn rows(&self) -> Vec<MenuRow> {
        let lsp = |label, shortcut, action, opens_group| MenuRow {
            label,
            shortcut,
            action: Some(action),
            opens_group,
        };
        let mut rows = vec![
            MenuRow {
                label: "editor-go-to-definition",
                shortcut: "F12",
                action: None,
                opens_group: false,
            },
            MenuRow {
                label: "editor-find-references",
                shortcut: "⇧F12",
                action: None,
                opens_group: false,
            },
        ];
        for (action, label, shortcut) in [
            (
                EditorAction::GotoDeclaration,
                "editor-go-to-declaration",
                "",
            ),
            (
                EditorAction::GotoTypeDefinition,
                "editor-go-to-type-definition",
                "",
            ),
            (
                EditorAction::GotoImplementation,
                "editor-go-to-implementation",
                "⌘F12",
            ),
        ] {
            if self.offered.contains(&action) {
                rows.push(lsp(label, shortcut, action, false));
            }
        }

        let mut modifying = Vec::new();
        if self.offered.contains(&EditorAction::Rename) {
            modifying.push(lsp(
                "editor-rename-symbol",
                "F2",
                EditorAction::Rename,
                false,
            ));
        }
        modifying.push(lsp(
            "editor-change-all-occurrences",
            "⌘F2",
            EditorAction::ChangeAllOccurrences,
            false,
        ));
        if self.offered.contains(&EditorAction::FormatDocument) {
            modifying.push(lsp(
                "editor-format-document",
                "⇧⌥F",
                EditorAction::FormatDocument,
                false,
            ));
        }
        if self.offered.contains(&EditorAction::FormatSelection) {
            modifying.push(lsp(
                "editor-format-selection",
                "",
                EditorAction::FormatSelection,
                false,
            ));
        }
        if self.offered.contains(&EditorAction::CodeAction) {
            modifying.push(lsp(
                "editor-code-action",
                "⌃⇧R",
                EditorAction::CodeAction,
                false,
            ));
        }
        if let Some(first) = modifying.first_mut() {
            first.opens_group = true;
        }
        rows.append(&mut modifying);

        rows.push(lsp("editor-cut", "⌘X", EditorAction::Cut, true));
        rows.push(lsp("editor-copy", "⌘C", EditorAction::Copy, false));
        rows.push(lsp("editor-paste", "⌘V", EditorAction::Paste, false));
        rows.push(lsp(
            "editor-command-palette",
            "⇧⌘P",
            EditorAction::CommandPalette,
            true,
        ));
        rows
    }
}

#[derive(Clone, PartialEq)]
struct RenameBox {
    line: u32,
    col: u32,
    original: String,
    draft: String,
}

impl RenameBox {
    fn submit(&self) {
        let name = self.draft.trim();
        if name.is_empty() || name == self.original {
            return;
        }
        let _ = send(&FileRenameRequest {
            line: self.line,
            col: self.col,
            new_name: name.to_string(),
        });
    }
}

fn is_markdown_file(path: &str) -> bool {
    path.rsplit_once('.')
        .map(|(_, extension)| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        })
        .unwrap_or(false)
}

fn file_mode_class(active: bool) -> &'static str {
    if active {
        "rounded bg-primary/15 px-1.5 py-0.5 text-primary transition-[background-color,color,box-shadow] duration-200 ease-out"
    } else {
        "rounded px-1.5 py-0.5 text-foreground/45 transition-[background-color,color,box-shadow] duration-200 ease-out hover:bg-foreground/[0.06] hover:text-foreground"
    }
}

fn column_in_line(
    at: ElementPoint,
    gutter: f64,
    cell: (f64, f64),
    wrap_columns: u16,
    round: bool,
) -> (f64, u32) {
    let (cw, ch) = cell;
    let x = at.x - gutter;
    if cw <= 0.0 {
        return (x, 0);
    }
    let local = if round {
        (x.max(0.0) / cw).round()
    } else {
        (x.max(0.0) / cw).floor()
    } as u32;
    if wrap_columns == 0 || ch <= 0.0 {
        return (x, local);
    }
    let wrapped_row = (at.y.max(0.0) / ch).floor() as u32;

    (
        x,
        wrapped_row * wrap_columns as u32 + local.min(wrap_columns as u32),
    )
}

fn activate_note_cursor(
    block_index: usize,
    line: u32,
    note_active: Signal<Option<u32>>,
    note_editing: Signal<bool>,
    note_edit_line: Signal<Option<u32>>,
) {
    set_note_cursor_active(
        block_index,
        line,
        note_active,
        note_editing,
        note_edit_line,
        false,
    );
}

fn activate_note_cursor_centered(
    block_index: usize,
    line: u32,
    note_active: Signal<Option<u32>>,
    note_editing: Signal<bool>,
    note_edit_line: Signal<Option<u32>>,
) {
    set_note_cursor_active(
        block_index,
        line,
        note_active,
        note_editing,
        note_edit_line,
        true,
    );
}

fn set_note_cursor_active(
    block_index: usize,
    line: u32,
    mut note_active: Signal<Option<u32>>,
    mut note_editing: Signal<bool>,
    mut note_edit_line: Signal<Option<u32>>,
    center: bool,
) {
    note_active.set(Some(block_index as u32));
    note_editing.set(true);
    note_edit_line.set(Some(line));
    spawn(async move {
        sleep_ms(0).await;
        focus_file_input();
        if center {
            center_note_caret(block_index, line);
        }
    });
}

fn note_pointer_line(
    at: ElementPoint,
    height: f64,
    start: u32,
    end: u32,
    block: &MdBlock,
    list_hit: Option<u32>,
) -> u32 {
    if matches!(block, MdBlock::List { .. })
        && let Some(line) = list_hit
    {
        return line;
    }
    let count = end.saturating_sub(start).max(1);
    if height <= 0.0 {
        return start;
    }
    let ratio = (at.y / height).clamp(0.0, 1.0);

    start + ((ratio * count as f64).floor() as u32).min(count - 1)
}

fn note_edit_block_class(block: &MdBlock) -> &'static str {
    match block {
        MdBlock::Heading { level, .. } => heading_class(*level),
        MdBlock::Paragraph { .. } => "my-3",
        MdBlock::List { .. } => "my-3 pl-6",
        MdBlock::CodeBlock { .. } => {
            "my-4 rounded-xl bg-foreground/[0.05] p-4 font-mono text-xs ring-1 ring-inset ring-border"
        }
        MdBlock::BlockQuote { .. } => {
            "my-4 rounded-r-lg border-l-2 border-primary/50 bg-primary/[0.04] py-1 pl-4 pr-3 text-foreground/70"
        }
        MdBlock::Table { .. } => {
            "my-4 rounded-xl p-3 font-mono text-xs ring-1 ring-inset ring-border"
        }
        MdBlock::ThematicBreak => "my-6",
        MdBlock::Html { .. } => "my-3 whitespace-pre-wrap text-foreground/60",
    }
}

fn note_edit_line_class(block: &MdBlock) -> &'static str {
    if matches!(block, MdBlock::List { .. }) {
        "my-1 min-h-[1lh] w-full whitespace-pre-wrap break-words"
    } else {
        "min-h-[1lh] w-full whitespace-pre-wrap break-words"
    }
}

fn note_edit_overlay_class() -> &'static str {
    "visible absolute inset-0 z-10 cursor-text overflow-visible"
}

fn note_block_index_for_line(blocks: &[NoteBlock], line: u32) -> Option<usize> {
    blocks
        .iter()
        .position(|block| block.start_line <= line && line < block.end_line)
        .or_else(|| blocks.iter().rposition(|block| block.start_line <= line))
        .or_else(|| (!blocks.is_empty()).then_some(0))
}

fn place_note_caret(element_id: String, line: u32, prefix: u32, at: ClientPoint, extend: bool) {
    spawn(async move {
        let offset = TextRun::in_element(element_id)
            .offset_at(at.x, at.y)
            .await
            .unwrap_or_default();
        let _ = send(&FilePointerEvent {
            line,
            col: prefix + offset,
            extend,
            add: false,
        });
        focus_file_input();
    });
}

fn place_note_block_caret(index: usize, start_line: u32, source: String, at: ClientPoint) {
    spawn(async move {
        let offset = TextRun::in_element(format!("note-live-block-{index}"))
            .offset_at(at.x, at.y)
            .await
            .unwrap_or_default();
        let (line, col) = note_source_position(&source, start_line, offset);
        let _ = send(&FilePointerEvent {
            line,
            col,
            extend: false,
            add: false,
        });
        focus_file_input();
    });
}

#[derive(Clone, PartialEq)]
struct NoteSourceChunk {
    text: String,
    selected: bool,
    caret_before: bool,
}

fn note_source_chunks(
    source: &[char],
    start: u32,
    end: u32,
    caret: u32,
    selections: &[(u32, u32)],
) -> Vec<NoteSourceChunk> {
    let mut boundaries = vec![start, end];
    if start <= caret && caret < end {
        boundaries.push(caret);
    }
    for (selection_start, selection_end) in selections {
        let clipped_start = (*selection_start).clamp(start, end);
        let clipped_end = (*selection_end).clamp(start, end);
        if clipped_start < clipped_end {
            boundaries.push(clipped_start);
            boundaries.push(clipped_end);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
        .windows(2)
        .map(|range| {
            let chunk_start = range[0];
            let chunk_end = range[1];
            NoteSourceChunk {
                text: source[chunk_start as usize..chunk_end as usize]
                    .iter()
                    .map(|character| if *character == '\n' { ' ' } else { *character })
                    .collect(),
                selected: selections.iter().any(|(selection_start, selection_end)| {
                    chunk_start < *selection_end && chunk_end > *selection_start
                }),
                caret_before: caret == chunk_start,
            }
        })
        .collect()
}

fn note_inline_class(kind: NoteInlineKind) -> &'static str {
    match kind {
        NoteInlineKind::BlockMarker | NoteInlineKind::Escape => "",
        NoteInlineKind::Code => {
            "rounded bg-foreground/10 px-1 py-0.5 font-mono text-[0.85em] text-primary"
        }
        NoteInlineKind::Strong => "font-semibold text-foreground",
        NoteInlineKind::Emph => "italic",
        NoteInlineKind::Strike => "line-through opacity-70",
        NoteInlineKind::Link | NoteInlineKind::WikiLink => {
            "text-primary underline decoration-primary/40 underline-offset-2"
        }
    }
}

#[component]
fn NoteCaret(width_class: String) -> Element {
    rsx! {
        span {
            id: NOTE_CARET_ID,
            class: "relative inline-block h-[1.15em] w-0 scroll-mb-8 scroll-mt-8 align-text-bottom",
            span { class: "pointer-events-none absolute inset-y-0 left-0 {width_class} bg-current" }
        }
    }
}

#[component]
fn ExplorerSidebar(
    visible: Signal<bool>,
    width: Signal<u32>,
    mut resizing: Signal<bool>,
) -> Element {
    let keys = use_context::<FileKeys>();
    let open = visible();
    let panel_width = width();
    let wrapper_style = if open {
        format!("width:{panel_width}px;contain:layout style;")
    } else {
        "width:0px;contain:layout style;".to_string()
    };
    let panel_style = format!("width:{panel_width}px;");
    let panel_class = if open {
        "absolute inset-y-0 left-0 h-full translate-x-0 opacity-100 transition-[translate,opacity] duration-200 ease-out will-change-[translate]"
    } else {
        "pointer-events-none absolute inset-y-0 left-0 h-full -translate-x-full opacity-0 transition-[translate,opacity] duration-200 ease-out will-change-[translate]"
    };
    rsx! {
        div {
            class: "relative z-[2] h-full shrink-0",
            style: "{wrapper_style}",
            onkeydown: move |event| {
                keys.offer(&event);
            },
            div { class: "{panel_class}", style: "{panel_style}", ExplorerPanel { visible } }
        }
        div {
            class: if open {
                "relative z-[2] h-full w-1 shrink-0 cursor-col-resize bg-foreground/[0.06] opacity-100 transition-opacity duration-150 hover:bg-cyan-400/40"
            } else {
                "pointer-events-none h-full w-0 shrink-0 opacity-0"
            },
            onmousedown: move |e: Event<MouseData>| {
                e.prevent_default();
                resizing.set(true);
            },
        }
    }
}

#[component]
fn FindBar(
    query: Signal<String>,
    open: Signal<bool>,
    forward: Signal<bool>,
    vim: bool,
    total: u32,
    index: u32,
) -> Element {
    let mut query = query;
    let mut open = open;
    let mut regex = use_signal(|| vim);
    let mut close = move || {
        open.set(false);
        query.set(String::new());
        let _ = send(&FileFindRequest {
            done: true,
            ..Default::default()
        });
        focus_file_input();
    };
    let ask = move |text: String| {
        let _ = send(&FileFindRequest {
            query: text,
            step: false,
            reverse: false,
            done: false,
            regex: regex(),
            forward: forward(),
        });
    };
    let step = move |reverse: bool| {
        let _ = send(&FileFindRequest {
            query: query.peek().clone(),
            step: true,
            reverse,
            done: false,
            regex: regex(),
            forward: forward(),
        });
    };
    let confirm = move |reverse: bool| {
        step(reverse);
        if vim {
            focus_file_input();
        }
    };
    let count = match (total, index) {
        (0, _) => translate("editor-find-no-results"),
        (total, 0) => format!("{total}"),
        (total, index) => format!("{index}/{total}"),
    };

    rsx! {
        div {
            class: "flex h-6 shrink-0 items-center gap-1 rounded-md bg-foreground/[0.06] pl-2 pr-1 ring-1 ring-inset ring-foreground/10",
            input {
                id: FIND_INPUT_ID,
                r#type: "text",
                class: "w-40 bg-transparent font-sans text-[11px] text-foreground outline-none placeholder:text-muted-foreground",
                placeholder: translate("editor-find-placeholder"),
                value: "{query}",
                oninput: move |event| {
                    let text = event.value();
                    query.set(text.clone());
                    ask(text);
                },
                onkeydown: move |event: Event<KeyboardData>| {
                    event.stop_propagation();
                    match event.key() {
                        Key::Enter => {
                            event.prevent_default();
                            confirm(event.modifiers().shift());
                        }
                        Key::Escape => {
                            event.prevent_default();
                            close();
                        }
                        _ => {}
                    }
                },
            }
            button {
                r#type: "button",
                class: if regex() {
                    "shrink-0 rounded bg-foreground/15 px-1 font-mono text-[10px] text-foreground"
                } else {
                    "shrink-0 rounded px-1 font-mono text-[10px] text-foreground/50 hover:bg-foreground/10 hover:text-foreground"
                },
                title: translate("editor-find-regex"),
                onclick: move |_| {
                    regex.toggle();
                    ask(query.peek().clone());
                },
                ".*"
            }
            span {
                class: if total == 0 && !query().is_empty() {
                    "shrink-0 tabular-nums text-[10px] text-destructive"
                } else {
                    "shrink-0 tabular-nums text-[10px] text-muted-foreground"
                },
                "{count}"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-previous"),
                onclick: move |_| step(true),
                "‹"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-next"),
                onclick: move |_| step(false),
                "›"
            }
            button {
                r#type: "button",
                class: "shrink-0 rounded px-1 text-foreground/60 hover:bg-foreground/10 hover:text-foreground",
                title: translate("editor-find-close"),
                onclick: move |_| close(),
                "✕"
            }
        }
    }
}

#[component]
fn ExplorerToggleButton(pane: ExplorerPane, mode: Signal<Mode>) -> Element {
    rsx! {
        button {
            class: "shrink-0 cursor-default rounded p-0.5 text-foreground/60 hover:bg-foreground/[0.08] hover:text-foreground",
            title: translate("editor-toggle-explorer"),
            onclick: move |_| {
                pane.toggle(mode)
            },
            svg {
                class: "h-4 w-4",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                line { x1: "9", y1: "3", x2: "9", y2: "21" }
            }
        }
    }
}

#[component]
fn NoteSourceRange(
    source: Vec<char>,
    start: u32,
    end: u32,
    caret: u32,
    selections: Vec<(u32, u32)>,
    caret_width_class: String,
) -> Element {
    let source = source.as_slice();
    let selections = selections.as_slice();
    let caret_width_class = caret_width_class.as_str();
    let chunks = note_source_chunks(source, start, end, caret, selections);
    rsx! {
        for (index, chunk) in chunks.iter().enumerate() {
            if chunk.caret_before {
                NoteCaret { width_class: caret_width_class.to_string() }
            }
            if !chunk.text.is_empty() {
                span {
                    key: "source-{start}-{index}",
                    class: if chunk.selected { "bg-current/20" } else { "" },
                    "{chunk.text}"
                }
            }
        }
    }
}

#[component]
fn NoteInlineNodes(
    source: Vec<char>,
    nodes: Vec<NoteInlineNode>,
    caret: u32,
    selections: Vec<(u32, u32)>,
    caret_width_class: String,
) -> Element {
    let nodes = nodes.as_slice();
    rsx! {
            for (index, node) in nodes.iter().enumerate() {
                match node {
                    NoteInlineNode::Text { start, end } => rsx! {
                        span { key: "text-{index}",
                            NoteSourceRange {
        source: source.to_vec(),
        start: *start,
        end: *end,
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                        }
                    },
                    NoteInlineNode::Syntax {
                        kind,
                        start,
                        prefix_end,
                        suffix_start,
                        end,
                        children,
                    } => {
                        let reveal = *start <= caret && caret <= *end;
                        rsx! {
                            span { key: "syntax-{index}", class: note_inline_class(*kind),
                                span { class: if reveal { "text-foreground/55" } else { "hidden" },
                                    NoteSourceRange {
        source: source.to_vec(),
        start: *start,
        end: *prefix_end,
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                                }
                                NoteInlineNodes {
        source: source.to_vec(),
        nodes: children.to_vec(),
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                                span { class: if reveal { "text-foreground/55" } else { "hidden" },
                                    NoteSourceRange {
        source: source.to_vec(),
        start: *suffix_start,
        end: *end,
        caret,
        selections: selections.to_vec(),
        caret_width_class: caret_width_class.to_string(),
    }
                                }
                            }
                        }
                    }
                }
            }
        }
}

fn note_selection_ranges(
    source: &str,
    start_line: u32,
    selections: &[vmux_core::editor::SelSpan],
) -> Vec<(u32, u32)> {
    selections
        .iter()
        .map(|selection| {
            let start = note_source_offset(source, start_line, selection.line, selection.start);
            let end_col = if selection.end == u32::MAX {
                source
                    .split('\n')
                    .nth(selection.line.saturating_sub(start_line) as usize)
                    .map_or(0, |line| line.chars().count() as u32)
            } else {
                selection.end
            };
            let end = note_source_offset(source, start_line, selection.line, end_col);
            (start.min(end), start.max(end))
        })
        .filter(|(start, end)| start < end)
        .collect()
}

fn emit_property_edit(
    original_key: String,
    key: String,
    kind: KnowledgePropertyKind,
    values: Vec<String>,
    remove: bool,
) {
    let _ = send(&FilePropertyEdit {
        original_key,
        key,
        kind,
        values,
        remove,
    });
}

fn property_kind_label(kind: KnowledgePropertyKind) -> String {
    match kind {
        KnowledgePropertyKind::Text => translate("editor-property-kind-text"),
        KnowledgePropertyKind::Number => translate("editor-property-kind-number"),
        KnowledgePropertyKind::Checkbox => translate("editor-property-kind-checkbox"),
        KnowledgePropertyKind::Date => translate("editor-property-kind-date"),
        KnowledgePropertyKind::List => translate("editor-property-kind-list"),
        KnowledgePropertyKind::Link => translate("editor-property-kind-link"),
        KnowledgePropertyKind::Tags => translate("editor-property-kind-tags"),
    }
}

fn next_property_kind(kind: KnowledgePropertyKind) -> KnowledgePropertyKind {
    match kind {
        KnowledgePropertyKind::Text => KnowledgePropertyKind::Number,
        KnowledgePropertyKind::Number => KnowledgePropertyKind::Checkbox,
        KnowledgePropertyKind::Checkbox => KnowledgePropertyKind::Date,
        KnowledgePropertyKind::Date => KnowledgePropertyKind::List,
        KnowledgePropertyKind::List => KnowledgePropertyKind::Link,
        KnowledgePropertyKind::Link => KnowledgePropertyKind::Tags,
        KnowledgePropertyKind::Tags => KnowledgePropertyKind::Text,
    }
}

#[component]
fn NoteProperties(properties: Vec<KnowledgeProperty>) -> Element {
    let mut open = use_signal(|| !properties.is_empty());
    let has_tags = properties
        .iter()
        .any(|property| property.kind == KnowledgePropertyKind::Tags);
    let add_key = {
        let mut suffix = 1;
        loop {
            let candidate = if suffix == 1 {
                "property".to_string()
            } else {
                format!("property-{suffix}")
            };
            if !properties
                .iter()
                .any(|property| property.key.eq_ignore_ascii_case(&candidate))
            {
                break candidate;
            }
            suffix += 1;
        }
    };
    rsx! {
        div { class: "mb-5 rounded-xl bg-foreground/[0.025] ring-1 ring-inset ring-foreground/[0.07]",
            div { class: "flex h-9 items-center gap-2 px-3",
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 items-center gap-2 text-left text-xs font-medium text-foreground/65 hover:text-foreground",
                    onclick: move |_| open.toggle(),
                    Icon { class: if open() { "h-3.5 w-3.5 rotate-90 transition-transform" } else { "h-3.5 w-3.5 transition-transform" }, path { d: "m9 18 6-6-6-6" } }
                    span { {translate("editor-properties")} }
                    if !properties.is_empty() {
                        span { class: "text-[10px] text-muted-foreground", "{properties.len()}" }
                    }
                }
                button {
                    r#type: "button",
                    title: translate("editor-add-tags"),
                    disabled: has_tags,
                    class: if has_tags { "rounded-md px-1 text-xs text-muted-foreground/30" } else { "rounded-md px-1 text-xs text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground" },
                    onclick: move |_| {
                        open.set(true);
                        emit_property_edit(
                            String::new(),
                            "tags".to_string(),
                            KnowledgePropertyKind::Tags,
                            Vec::new(),
                            false,
                        );
                    },
                    "#"
                }
                button {
                    r#type: "button",
                    title: translate("editor-add-property"),
                    class: "rounded-md p-1 text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
                    onclick: move |_| {
                        open.set(true);
                        emit_property_edit(
                            String::new(),
                            add_key.clone(),
                            KnowledgePropertyKind::Text,
                            vec![String::new()],
                            false,
                        );
                    },
                    Icon { class: "h-3.5 w-3.5", path { d: "M12 5v14" } path { d: "M5 12h14" } }
                }
            }
            if open() {
                div { class: "border-t border-foreground/[0.06] px-1 py-1",
                    if properties.is_empty() {
                        div { class: "px-3 py-2 text-xs text-muted-foreground", {translate("editor-no-properties")} }
                    }
                    for property in properties {
                        NotePropertyRow {
                            key: "{property.key}:{property.kind:?}:{property.values:?}",
                            property,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NoteBlockView(
    note_blocks: Signal<Vec<NoteBlock>>,
    diff_markers: Signal<HashMap<u32, EditorDiffMarker>>,
    index: usize,
    editing: bool,
    source_cursor: Signal<vmux_core::editor::CursorPos>,
    source_selections: Signal<Vec<vmux_core::editor::SelSpan>>,
    note_diff_marker: Option<EditorDiffMarker>,
    keymap: vmux_core::KeymapKind,
    mut note_active: Signal<Option<u32>>,
    mut note_editing: Signal<bool>,
    mut note_edit_line: Signal<Option<u32>>,
    mut note_dragging: Signal<bool>,
    comp_open: bool,
    comp_filtered: Vec<CompletionItem>,
    comp_sel_clamped: usize,
) -> Element {
    let Some(note_block) = note_blocks.read().get(index).cloned() else {
        return rsx! {};
    };
    let current = if editing {
        *source_cursor.read()
    } else {
        vmux_core::editor::CursorPos::default()
    };
    let selections = if editing {
        source_selections.read().clone()
    } else {
        Vec::new()
    };
    let active_edit_line = if editing {
        note_edit_line.read().unwrap_or(current.line)
    } else {
        0
    };
    let is_list = matches!(note_block.block, MdBlock::List { .. });
    let is_live_inline = matches!(
        note_block.block,
        MdBlock::Paragraph { .. } | MdBlock::Heading { .. }
    );
    let start = note_block.start_line;
    let end = note_block.end_line;
    let note_diff_marker = note_block_diff_marker(&diff_markers.read(), start, end);
    let source = note_block.source.clone();
    let pointer_source = source.clone();
    let live_pointer_source = source.clone();
    let live_down_source = if editing {
        source.clone()
    } else {
        String::new()
    };
    let pointer_block = note_block.block.clone();
    let edit_lines = if !editing {
        Vec::new()
    } else if is_list {
        let raw = source
            .lines()
            .nth(active_edit_line.saturating_sub(start) as usize)
            .unwrap_or_default();
        let prefix = note_list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix);
        vec![(
            active_edit_line,
            raw.chars().skip(prefix).collect::<String>(),
            prefix as u32,
        )]
    } else if source.is_empty() {
        vec![(start, String::new(), 0)]
    } else {
        source
            .lines()
            .enumerate()
            .map(|(offset, raw)| (start + offset as u32, raw.to_string(), 0))
            .collect::<Vec<_>>()
    };
    let edit_class = note_edit_block_class(&note_block.block);
    let heading_level = match &note_block.block {
        MdBlock::Heading { level, .. } => Some(*level),
        _ => None,
    };
    let (live_nodes, live_source, live_caret, live_selections) = if editing && is_live_inline {
        (
            note_inline_nodes(&source, heading_level),
            source.chars().collect::<Vec<_>>(),
            note_source_offset(&source, start, current.line, current.col),
            note_selection_ranges(&source, start, &selections),
        )
    } else {
        (Vec::new(), Vec::new(), 0, Vec::new())
    };
    let caret_width_class = if keymap == vmux_core::KeymapKind::Vscode {
        "w-px"
    } else {
        "w-[2px]"
    };
    let line_chunks = |line: u32, raw: &str, prefix: u32| {
        let selection = selections
            .iter()
            .find(|selection| selection.line == line)
            .map(|selection| vmux_core::editor::SelSpan {
                line: selection.line,
                row: selection.row,
                start: selection.start.saturating_sub(prefix),
                end: if selection.end == u32::MAX {
                    u32::MAX
                } else {
                    selection.end.saturating_sub(prefix)
                },
            });
        NoteLineChunk::split(
            raw,
            (line == current.line).then_some(current.col.saturating_sub(prefix)),
            selection,
        )
    };
    let list_edit = match edit_lines.first() {
        Some((line, raw, prefix)) if is_list => Some(ListEditLine {
            line: *line,
            chunks: line_chunks(*line, raw, *prefix),
            caret_width_class: caret_width_class.to_string(),
        }),
        _ => None,
    };
    let mut block_height = use_signal(|| 0.0f64);
    let ListLineHit(list_hit) = use_context_provider(|| ListLineHit(Signal::new(None)));
    let marker_prefix = move |source: &str, line: u32| {
        let raw = source
            .lines()
            .nth(line.saturating_sub(start) as usize)
            .unwrap_or_default();
        if is_list {
            note_list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix as u32)
        } else {
            0
        }
    };
    let down_source = source.clone();
    let on_line_down = use_callback(move |(line, at, extend): (u32, ClientPoint, bool)| {
        note_dragging.set(true);
        place_note_caret(
            format!("note-line-{line}"),
            line,
            marker_prefix(&down_source, line),
            at,
            extend,
        );
    });

    rsx! {
        div {
            id: "note-block-{index}",
            "data-note-block": "{index}",
            class: "relative flow-root w-full cursor-text",
            onresize: move |event: Event<ResizeData>| {
                if let Ok(size) = event.get_border_box_size() {
                    block_height.set(size.height);
                }
            },
            onclick: move |event: Event<MouseData>| {
                if editing && !is_list {
                    return;
                }
                event.stop_propagation();
                if EventSelection::in_document() {
                    return;
                }
                let at = event.client_coordinates();
                if is_live_inline {
                    note_active.set(Some(index as u32));
                    note_editing.set(true);
                    note_edit_line.set(None);
                    place_note_block_caret(index, start, live_pointer_source.clone(), at);
                    return;
                }
                let line = note_pointer_line(
                    event.element_coordinates(),
                    block_height(),
                    start,
                    end,
                    &pointer_block,
                    list_hit(),
                );
                note_active.set(Some(index as u32));
                note_editing.set(true);
                note_edit_line.set(Some(line));
                place_note_caret(
                    format!("note-line-{line}"),
                    line,
                    marker_prefix(&pointer_source, line),
                    at,
                    false,
                );
            },
            if let Some(marker) = note_diff_marker {
                span {
                    class: "pointer-events-none absolute -left-4 bottom-1 top-1 w-[3px] rounded-full opacity-80 {note_diff_marker_class(marker)}"
                }
            }
            RenderedNoteBlock {
                block: note_block.block.clone(),
                index,
                hidden_list_line: (editing && is_list).then_some(active_edit_line),
                invisible: editing && !is_list,
                list_edit,
                on_line_down,
            }
            if editing && !is_list {
                div {
                    class: note_edit_overlay_class(),
                    if is_live_inline {
                        div {
                            id: "note-live-block-{index}",
                            "data-note-edit-block": "{index}",
                            class: edit_class,
                            onclick: move |event: Event<MouseData>| {
                                event.stop_propagation();
                                event.prevent_default();
                            },
                            onpointerdown: move |event: Event<PointerData>| {
                                event.stop_propagation();
                                event.prevent_default();
                                note_dragging.set(true);
                                place_note_block_caret(
                                    index,
                                    start,
                                    live_down_source.clone(),
                                    event.client_coordinates(),
                                );
                            },
                            onmousedown: move |event: Event<MouseData>| {
                                event.stop_propagation();
                                event.prevent_default();
                            },
                            span {
                                "data-note-line-text": "true",
                                class: "inline",
                                NoteInlineNodes {
                                    source: live_source.clone(),
                                    nodes: live_nodes.clone(),
                                    caret: live_caret,
                                    selections: live_selections.clone(),
                                    caret_width_class: caret_width_class.to_string(),
                                }
                                if live_caret == live_source.len() as u32 {
                                    NoteCaret { width_class: caret_width_class.to_string() }
                                }
                            }
                        }
                    } else {
                        div {
                            class: if is_list { "" } else { edit_class },
                            for (line, raw, prefix) in edit_lines.iter() {
                                {
                                    let line = *line;
                                    let prefix = *prefix;
                                    let chunks = line_chunks(line, raw, prefix);
                                    let line_class = if is_list {
                                        "min-h-[1lh] w-full whitespace-pre-wrap break-words"
                                    } else {
                                        note_edit_line_class(&note_block.block)
                                    };
                                    rsx! {
                                        div {
                                            key: "{line}",
                                            id: "note-line-{line}",
                                            "data-note-edit-line": "{line}",
                                            class: line_class,
                                            onclick: move |event: Event<MouseData>| {
                                                event.stop_propagation();
                                                event.prevent_default();
                                            },
                                            onpointerdown: move |event: Event<PointerData>| {
                                                event.stop_propagation();
                                                event.prevent_default();
                                                on_line_down.call((
                                                    line,
                                                    event.client_coordinates(),
                                                    event.modifiers().shift(),
                                                ));
                                            },
                                            onpointermove: move |event: Event<PointerData>| {
                                                if !note_dragging() {
                                                    return;
                                                }
                                                on_line_down.call((
                                                    line,
                                                    event.client_coordinates(),
                                                    true,
                                                ));
                                            },
                                            onmousedown: move |event: Event<MouseData>| {
                                                event.stop_propagation();
                                                event.prevent_default();
                                            },
                                            span {
                                                "data-note-line-text": "true",
                                                class: "inline-block min-w-[1ch]",
                                                for (chunk_index, chunk) in chunks.iter().enumerate() {
                                                    if chunk.caret_before {
                                                        span {
                                                            key: "caret-{chunk_index}",
                                                            id: NOTE_CARET_ID,
                                                            class: "relative inline-block h-[1.15em] w-0 scroll-mb-8 scroll-mt-8 align-text-bottom",
                                                            span { class: "pointer-events-none absolute inset-y-0 left-0 {caret_width_class} bg-current" }
                                                        }
                                                    }
                                                    if !chunk.text.is_empty() {
                                                        span {
                                                            key: "text-{chunk_index}",
                                                            class: if chunk.selected { "bg-cyan-400/20" } else { "" },
                                                            "{chunk.text}"
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
                    if comp_open && !comp_filtered.is_empty() {
                        div {
                            class: "absolute left-0 top-full z-40 mt-1 max-h-56 min-w-56 overflow-auto rounded-lg bg-background/95 py-1 text-xs text-foreground/90 ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg",
                            for (item_index, item) in comp_filtered.iter().enumerate() {
                                div {
                                    key: "note-completion-{item_index}",
                                    class: if item_index == comp_sel_clamped { "flex items-center gap-2 bg-cyan-400/15 px-3 py-1" } else { "flex items-center gap-2 px-3 py-1" },
                                    span { class: "truncate", "{item.label}" }
                                    span { class: "ml-auto truncate text-[10px] text-foreground/40", "{item.detail}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dir,
    Text,
    Media(MediaKind),
}

#[derive(Clone, PartialEq)]
enum Preview {
    None,
    Dir(Vec<FileDirEntry>),
    Text(Vec<FileLine>),
    Image(String),
    Video {
        url: String,
        path: String,
        native: bool,
    },
    Info {
        size: u64,
        modified: String,
        kind: String,
    },
    Error(String),
}

fn image_data_url(bytes: &[u8], path: &str) -> String {
    use base64::Engine;

    let mime = image_mime(path).unwrap_or("application/octet-stream");

    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn clear_preview(mut preview: Signal<Preview>, mut thumbs: Signal<HashMap<String, String>>) {
    preview.set(Preview::None);
    thumbs.set(HashMap::new());
}

fn request_preview(path: String) {
    let _ = send(&FilePreviewRequest { path, thumb: false });
}

fn request_thumb(path: String) {
    let _ = send(&FilePreviewRequest { path, thumb: true });
}

fn open_path(path: String) {
    let _ = send(&FileOpenEvent { path });
}

fn schedule_git_refresh(mut generation: Signal<u32>, mut nonce: Signal<u32>) {
    let next = generation().wrapping_add(1);
    generation.set(next);
    spawn(async move {
        sleep_ms(GIT_REFRESH_DEBOUNCE_MS).await;
        if generation() == next {
            nonce.set(nonce().wrapping_add(1));
        }
    });
}

fn parent_of(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((prefix, _)) => prefix.to_string(),
        None => path.to_string(),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

const PANE_CLASS: &str = "min-h-0 overflow-y-auto rounded-2xl bg-foreground/[0.025] p-2 ring-1 ring-inset ring-cyan-400/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.6)]";

fn row_class(selected: bool) -> String {
    let base =
        "flex items-center gap-2 rounded-md px-2 py-1 cursor-default transition-all duration-100";
    if selected {
        format!(
            "{base} bg-cyan-400/12 text-foreground shadow-[inset_2px_0_0_0_rgb(34,211,238),0_0_18px_-4px_rgba(34,211,238,0.45)]"
        )
    } else {
        format!("{base} text-foreground/75 hover:bg-foreground/[0.05]")
    }
}

fn diff_marker_sign(marker: EditorDiffMarker) -> &'static str {
    match marker {
        EditorDiffMarker::Added => "+",
        EditorDiffMarker::Modified | EditorDiffMarker::Staged => "~",
        EditorDiffMarker::Deleted => "-",
    }
}

fn diff_marker_text_class(marker: EditorDiffMarker) -> &'static str {
    match marker {
        EditorDiffMarker::Added => "text-ansi-2",
        EditorDiffMarker::Modified => "text-ansi-3",
        EditorDiffMarker::Deleted => "text-ansi-1",
        EditorDiffMarker::Staged => "text-ansi-3/80",
    }
}

fn diff_marker_row_class(marker: EditorDiffMarker) -> &'static str {
    match marker {
        EditorDiffMarker::Added => "bg-ansi-2/[0.06] hover:bg-ansi-2/[0.10]",
        EditorDiffMarker::Modified => "bg-ansi-3/[0.06] hover:bg-ansi-3/[0.10]",
        EditorDiffMarker::Deleted => "bg-ansi-1/[0.06] hover:bg-ansi-1/[0.10]",
        EditorDiffMarker::Staged => "bg-ansi-3/[0.035] hover:bg-ansi-3/[0.07]",
    }
}

fn note_diff_marker_class(marker: EditorDiffMarker) -> &'static str {
    match marker {
        EditorDiffMarker::Added => "bg-ansi-2",
        EditorDiffMarker::Modified | EditorDiffMarker::Staged => "bg-ansi-3",
        EditorDiffMarker::Deleted => "bg-ansi-1",
    }
}

fn note_block_diff_marker(
    markers: &HashMap<u32, EditorDiffMarker>,
    start_line: u32,
    end_line: u32,
) -> Option<EditorDiffMarker> {
    let priority = |marker| match marker {
        EditorDiffMarker::Staged => 0,
        EditorDiffMarker::Deleted => 1,
        EditorDiffMarker::Added => 2,
        EditorDiffMarker::Modified => 3,
    };
    (start_line..=end_line)
        .filter_map(|line| markers.get(&(line + 1)).copied())
        .max_by_key(|marker| priority(*marker))
}

fn visible_entries(all: &[FileDirEntry], show_hidden: bool) -> Vec<FileDirEntry> {
    if show_hidden {
        all.to_vec()
    } else {
        all.iter()
            .filter(|e| !e.name.starts_with('.'))
            .cloned()
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_dir(
    mut dir_entries: Signal<Vec<FileDirEntry>>,
    mut parent_entries: Signal<Vec<FileDirEntry>>,
    mut path: Signal<String>,
    mut selected: Signal<usize>,
    mut preview: Signal<Preview>,
    mut thumbs: Signal<HashMap<String, String>>,
    show_hidden: bool,
    entries: Vec<FileDirEntry>,
    parent: Vec<FileDirEntry>,
    new_path: String,
    select_path: Option<String>,
) {
    thumbs.set(HashMap::new());
    preview.set(Preview::None);
    parent_entries.set(parent);
    path.set(new_path);
    let vis = visible_entries(&entries, show_hidden);
    let sel_idx = select_path
        .as_deref()
        .map(|p| dir_select_index(&vis, p))
        .unwrap_or(0);
    selected.set(sel_idx);
    if let Some(sel) = vis.get(sel_idx) {
        request_preview(sel.path.clone());
    }
    for e in &vis {
        if !e.is_dir && image_mime(&e.path).is_some() {
            request_thumb(e.path.clone());
        }
    }
    dir_entries.set(entries);
}

#[component]
fn EntryVisual(entry: FileDirEntry, thumb: Option<String>) -> Element {
    let entry = &entry;
    let thumb = thumb.as_ref();
    if let Some(url) = thumb {
        return rsx! {
            img { src: "{url}", class: "h-5 w-5 shrink-0 rounded object-cover ring-1 ring-border" }
        };
    }
    rsx! { TypeIcon { path: entry.path.to_string(), is_dir: entry.is_dir, class: "h-5 w-5 shrink-0 opacity-80" } }
}

#[component]
fn PreviewPane(preview: Preview) -> Element {
    let preview = &preview;
    match preview {
        Preview::None => rsx! {
            div { class: "text-xs text-muted-foreground opacity-60", "" }
        },
        Preview::Image(url) => rsx! {
            img { src: "{url}", class: "max-h-full max-w-full rounded-xl object-contain shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20" }
        },
        Preview::Video { url, path, native } => {
            if *native {
                let path = path.clone();
                rsx! {
                    div {
                        key: "{path}",
                        id: VIDEO_HOST_ID,
                        class: "h-full w-full rounded-xl bg-black/40 ring-1 ring-cyan-400/20",
                    }
                }
            } else {
                rsx! {
                    video {
                        id: "preview-video",
                        src: "{url}",
                        controls: true,
                        autoplay: false,
                        class: "max-h-full max-w-full rounded-xl shadow-[0_0_30px_-8px_rgba(34,211,238,0.4)] ring-1 ring-cyan-400/20",
                    }
                }
            }
        }
        Preview::Text(lines) => rsx! {
            div { class: "h-full w-full overflow-auto font-mono text-xs leading-snug",
                for line in lines.iter() {
                    div { key: "{line.line_no}", class: "whitespace-pre",
                        for (i, s) in line.spans.iter().enumerate() {
                            span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                        }
                    }
                }
            }
        },
        Preview::Dir(entries) => rsx! {
            div { class: "h-full w-full overflow-auto",
                for e in entries.iter() {
                    div { key: "{e.path}", class: "flex items-center gap-2 rounded px-2 py-1 text-foreground/90",
                        EntryVisual { entry: e.clone(), thumb: None }
                        span { class: "truncate text-xs", "{e.name}" }
                    }
                }
            }
        },
        Preview::Info {
            size,
            modified,
            kind,
        } => rsx! {
            div { class: "space-y-1 text-center text-xs text-muted-foreground",
                div {
                    class: "uppercase tracking-wide text-foreground/80",
                    {match kind.as_str() {
                        "image (too large to preview)" => translate("editor-preview-large-image"),
                        "binary" => translate("editor-preview-binary"),
                        "file" => translate("editor-preview-file"),
                        _ => kind.clone(),
                    }}
                }
                div { "{format_size(*size)}" }
                if !modified.is_empty() {
                    div { class: "opacity-70", "{modified}" }
                }
            }
        },
        Preview::Error(m) => rsx! {
            div { class: "text-xs text-ansi-1", "{m}" }
        },
    }
}

fn explorer_client_id() -> u64 {
    ((now_millis() as u64) << 12) ^ random_index(4096) as u64
}

fn explorer_has_room(page_width: u32, explorer_width: u32) -> bool {
    page_width > 0 && NOTE_MAX_CONTENT_WIDTH_PX.saturating_add(explorer_width) <= page_width
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExplorerReflowKey {
    page_width: u32,
    preferred_visible: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct ExplorerPane {
    pub visible: Signal<bool>,
    pub preferred_visible: Signal<bool>,
    pub width: Signal<u32>,
    pub page_width: Signal<u32>,
    pub client_id: Signal<u64>,
    pub request_id: Signal<u64>,
    pub reflowed_at: Signal<Option<ExplorerReflowKey>>,
    pub user_chose: Signal<bool>,
}

impl ExplorerPane {
    fn has_room(self) -> bool {
        explorer_has_room((self.page_width)(), (self.width)())
    }

    fn reflow_key(self) -> ExplorerReflowKey {
        ExplorerReflowKey {
            page_width: (self.page_width)(),
            preferred_visible: (self.preferred_visible)(),
        }
    }

    fn sync(mut self) {
        let key = self.reflow_key();
        if key.page_width == 0 || (self.reflowed_at)() == Some(key) {
            return;
        }
        self.reflowed_at.set(Some(key));
        let next = key.preferred_visible && ((self.user_chose)() || self.has_room());
        if (self.visible)() != next {
            self.visible.set(next);
        }
    }

    fn set_visible(mut self, next: bool, mode: Signal<Mode>) {
        let request_id = (self.request_id)().wrapping_add(1);
        self.request_id.set(request_id);
        self.preferred_visible.set(next);
        self.visible.set(next);
        self.reflowed_at.set(Some(self.reflow_key()));
        let _ = send(&ExplorerPanelSetVisible {
            visible: next,
            client_id: (self.client_id)(),
            request_id,
        });
        if next {
            return;
        }
        match mode() {
            Mode::Text => focus_file_input(),
            Mode::Dir | Mode::Media(_) => focus_container(),
        }
    }

    pub(crate) fn toggle(mut self, mode: Signal<Mode>) {
        self.user_chose.set(true);
        self.set_visible(!(self.visible)(), mode);
    }

    pub(crate) fn reveal_current(mut self, mode: Signal<Mode>) {
        if (self.visible)() {
            let _ = send(&ExplorerRevealCurrent);
            return;
        }
        self.user_chose.set(true);
        self.set_visible(true, mode);
    }

    fn show_if_room(self, mode: Signal<Mode>) {
        spawn(async move {
            sleep_ms(0).await;
            if (self.visible)() || !self.has_room() {
                return;
            }
            self.set_visible(true, mode);
        });
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

#[component]
fn NotePropertyRow(property: KnowledgeProperty) -> Element {
    let original_key = property.key.clone();
    let kind = property.kind;
    let mut key = use_signal(|| property.key.clone());
    let mut scalar = use_signal(|| property.values.first().cloned().unwrap_or_default());
    let mut item = use_signal(String::new);
    let values = property.values.clone();
    let key_for_kind = original_key.clone();
    let key_for_delete = original_key.clone();
    rsx! {
        div { class: "group flex min-h-9 items-start gap-2 rounded-lg px-2 py-1.5 hover:bg-foreground/[0.035]",
            input {
                value: "{key}",
                class: "w-28 shrink-0 bg-transparent text-xs font-medium text-foreground/65 outline-none focus:text-foreground",
                oninput: move |event| key.set(event.value()),
                onblur: {
                    let original_key = original_key.clone();
                    let values = values.clone();
                    move |_| emit_property_edit(original_key.clone(), key(), kind, values.clone(), false)
                },
            }
            button {
                r#type: "button",
                title: translate("editor-change-property-type"),
                class: "shrink-0 rounded-md bg-foreground/[0.05] px-1.5 py-0.5 text-[9px] uppercase tracking-wide text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                onclick: {
                    let values = values.clone();
                    move |_| {
                        let next = next_property_kind(kind);
                        emit_property_edit(key_for_kind.clone(), key(), next, values.clone(), false);
                    }
                },
                {property_kind_label(kind)}
            }
            div { class: "min-w-0 flex-1",
                if kind == KnowledgePropertyKind::Checkbox {
                    button {
                        r#type: "button",
                        class: if scalar().eq_ignore_ascii_case("true") { "flex h-5 w-9 items-center justify-end rounded-full bg-primary px-0.5" } else { "flex h-5 w-9 items-center justify-start rounded-full bg-foreground/15 px-0.5" },
                        onclick: {
                            let original_key = original_key.clone();
                            move |_| {
                                let next = (!scalar().eq_ignore_ascii_case("true")).to_string();
                                scalar.set(next.clone());
                                emit_property_edit(original_key.clone(), key(), kind, vec![next], false);
                            }
                        },
                        span { class: "h-4 w-4 rounded-full bg-background shadow-sm" }
                    }
                } else if matches!(kind, KnowledgePropertyKind::List | KnowledgePropertyKind::Tags) {
                    div { class: "flex flex-wrap items-center gap-1",
                        for (index, value) in values.iter().enumerate() {
                            {
                                let remove_key = original_key.clone();
                                let remove_values = values.clone();
                                rsx! {
                                    button {
                                        key: "{index}:{value}",
                                        r#type: "button",
                                        title: translate("common-remove"),
                                        class: if kind == KnowledgePropertyKind::Tags { "rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary hover:bg-destructive/10 hover:text-destructive" } else { "rounded-md bg-foreground/[0.06] px-2 py-0.5 text-[11px] text-foreground/75 hover:bg-destructive/10 hover:text-destructive" },
                                        onclick: move |_| {
                                            let mut next = remove_values.clone();
                                            next.remove(index);
                                            emit_property_edit(remove_key.clone(), key(), kind, next, false);
                                        },
                                        if kind == KnowledgePropertyKind::Tags { "#" }
                                        "{value}"
                                    }
                                }
                            }
                        }
                        input {
                            value: "{item}",
                            placeholder: if kind == KnowledgePropertyKind::Tags { translate("editor-add-tag") } else { translate("editor-add-item") },
                            class: "min-w-20 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/60",
                            oninput: move |event| item.set(event.value()),
                            onkeydown: {
                                let add_key = original_key.clone();
                                let add_values = values.clone();
                                move |event: Event<KeyboardData>| {
                                    if event.key() != Key::Enter {
                                        return;
                                    }
                                    event.prevent_default();
                                    let value = item().trim().trim_start_matches('#').to_string();
                                    if value.is_empty() {
                                        return;
                                    }
                                    let mut next = add_values.clone();
                                    if !next.iter().any(|existing| existing.eq_ignore_ascii_case(&value)) {
                                        next.push(value);
                                    }
                                    item.set(String::new());
                                    emit_property_edit(add_key.clone(), key(), kind, next, false);
                                }
                            },
                        }
                    }
                } else {
                    input {
                        r#type: match kind {
                            KnowledgePropertyKind::Number => "number",
                            KnowledgePropertyKind::Date => "date",
                            _ => "text",
                        },
                        value: "{scalar}",
                        placeholder: if kind == KnowledgePropertyKind::Link { translate("editor-linked-note") } else { translate("editor-property-value") },
                        class: "w-full bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/60",
                        oninput: move |event| scalar.set(event.value()),
                        onblur: {
                            let original_key = original_key.clone();
                            move |_| emit_property_edit(original_key.clone(), key(), kind, vec![scalar()], false)
                        },
                    }
                }
            }
            button {
                r#type: "button",
                title: translate("editor-delete-property"),
                class: "invisible shrink-0 rounded p-0.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive group-hover:visible",
                onclick: move |_| emit_property_edit(key_for_delete.clone(), String::new(), kind, Vec::new(), true),
                Icon { class: "h-3.5 w-3.5", path { d: "M18 6 6 18" } path { d: "m6 6 12 12" } }
            }
        }
    }
}

#[component]
fn RenderedNoteBlock(
    block: MdBlock,
    index: usize,
    hidden_list_line: Option<u32>,
    invisible: bool,
    #[props(default)] list_edit: Option<ListEditLine>,
    #[props(default)] on_line_down: Option<EventHandler<(u32, ClientPoint, bool)>>,
) -> Element {
    rsx! {
        div { class: if invisible { "invisible" } else { "" },
            MdBlockView {
                block: block.clone(),
                block_key: index,
                hidden_list_line,
                list_edit,
                on_line_down,
            }
        }
    }
}

fn scroll_dir_row_into_view(idx: usize) {
    ScrollIntoView::nearest(&format!("dir-row-{idx}"));
}

fn toggle_preview_video() {
    MediaElement::with_id("preview-video").toggle_playback();
}

fn focus_container() {
    FocusClaim::new(CONTAINER_ID).request();
}

pub(crate) fn focus_file_input() {
    FocusClaim::new(INPUT_ID).request();
}

#[derive(Clone, Copy, Default, PartialEq)]
struct ScrollBox {
    size: (f64, f64),
}

impl ScrollBox {
    fn announce(self, cell: (f64, f64), total_lines: u32, mut last: Signal<FileResizeEvent>) {
        let (cw, ch) = cell;
        if cw <= 0.0 || ch <= 0.0 || self.size.0 <= 0.0 {
            return;
        }
        let next = FileResizeEvent {
            char_height: ch as f32,
            viewport_height: self.size.1 as f32,
            wrap_columns: ((self.size.0 - gutter_px(total_lines, cw) - 32.0).max(cw) / cw)
                .floor()
                .min(u16::MAX as f64) as u16,
        };
        let previous = last.peek().clone();
        if (previous.char_height - next.char_height).abs() <= 0.01
            && (previous.viewport_height - next.viewport_height).abs() <= 0.01
            && previous.wrap_columns == next.wrap_columns
        {
            return;
        }
        last.set(next.clone());
        let _ = send(&next);
    }
}

fn gutter_px(total_lines: u32, char_width: f64) -> f64 {
    gutter_width(total_lines) as f64 * char_width + 48.0
}

#[derive(Clone, Copy)]
struct FileViewport {
    element: Signal<Option<Rc<MountedData>>>,
    field: Signal<Option<Rc<MountedData>>>,
    geometry: Signal<ScrollBox>,
    offset: Signal<(f64, f64)>,
}

impl FileViewport {
    fn new() -> Self {
        Self {
            element: use_signal(|| None),
            field: use_signal(|| None),
            geometry: use_signal(ScrollBox::default),
            offset: use_signal(|| (0.0, 0.0)),
        }
    }

    fn scrolled_to(self, offset: (f64, f64)) {
        let mut current = self.offset;
        current.set(offset);
    }

    fn resized(self, size: (f64, f64)) {
        let mut geometry = self.geometry;
        if geometry.peek().size == size {
            return;
        }
        geometry.write().size = size;
    }

    fn mounted(self, element: Rc<MountedData>) {
        let mut current = self.element;
        current.set(Some(element));
        self.measure();
    }

    fn field_mounted(self, element: Rc<MountedData>) {
        let mut current = self.field;
        current.set(Some(element));
    }

    fn measure(self) {
        spawn(async move {
            let Some(element) = self.element.peek().clone() else {
                return;
            };
            let Ok(rect) = element.get_client_rect().await else {
                return;
            };
            let mut geometry = self.geometry;
            geometry.write().size = (rect.size.width, rect.size.height);
        });
    }

    fn scroll_to(self, top: f64) {
        ScrollIntoView::element_to(SCROLL_ID, top);
    }

    fn scroll_by(self, lines: i32, line_height: f64) {
        let from = self.offset.peek().1;
        self.scroll_to(from + lines as f64 * line_height);
    }

    fn reset(self) {
        self.scroll_to(0.0);
    }

    fn reveal_caret(self) {
        ScrollIntoView::nearest(INPUT_ID);
    }

    fn center_row(self, row: u32, ch: f64) {
        let geometry = *self.geometry.peek();
        if ch <= 0.0 || geometry.size.1 <= 0.0 {
            return;
        }
        self.scroll_to(centered_scroll_top(
            row as f64 * ch + ch * 0.5,
            geometry.size.1,
        ));
    }
}

struct NoteCaretAnchor([String; 4]);

impl NoteCaretAnchor {
    fn of(block_index: usize, line: u32) -> Self {
        Self([
            NOTE_CARET_ID.to_string(),
            format!("note-line-{line}"),
            format!("note-live-block-{block_index}"),
            format!("note-block-{block_index}"),
        ])
    }

    fn reveal(&self) {
        ScrollIntoView::first_rendered(&self.0.each_ref().map(String::as_str));
    }

    fn center(&self) {
        ScrollIntoView::first_rendered_centered(&self.0.each_ref().map(String::as_str));
    }
}

fn ensure_note_caret_visible(block_index: usize, line: u32) {
    NoteCaretAnchor::of(block_index, line).reveal();
}

fn center_note_caret(block_index: usize, line: u32) {
    NoteCaretAnchor::of(block_index, line).center();
}

fn send_committed_text(text: String) {
    if text.is_empty() {
        return;
    }
    let _ = send(&FileTextInput { text });
    TextCaret::in_field(INPUT_ID).clear();
}

fn forward_file_key(event: &Event<KeyboardData>, mode: vmux_core::editor::EditMode) -> bool {
    let Some(stroke) = PressedKey::new(&event.data()).stroke() else {
        return false;
    };
    if mode.accepts_text() && stroke.is_text_input() {
        return false;
    }
    event.prevent_default();
    let _ = send(&stroke);
    true
}

#[component]
fn NativeVideoHost(path: String) -> Element {
    let mut element = use_signal(|| None::<Rc<MountedData>>);
    let reported = path.clone();
    let report = use_callback(move |()| {
        let path = reported.clone();
        spawn(async move {
            let Some(element) = element.peek().clone() else {
                return;
            };
            let Ok(rect) = element.get_client_rect().await else {
                return;
            };
            if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
                return;
            }
            let _ = send(&FileVideoRect {
                path,
                x: rect.origin.x as f32,
                y: rect.origin.y as f32,
                w: rect.size.width as f32,
                h: rect.size.height as f32,
            });
        });
    });

    rsx! {
        div {
            key: "{path}",
            id: VIDEO_HOST_ID,
            class: "h-full w-full rounded-xl bg-black/40 ring-1 ring-cyan-400/20",
            onmounted: move |event: Event<MountedData>| {
                element.set(Some(event.data()));
                report.call(());
            },
            onresize: move |_| report.call(()),
        }
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    fn labels(offered: &[EditorAction]) -> Vec<&'static str> {
        EditorMenu::offering(offered)
            .rows()
            .into_iter()
            .map(|row| row.label)
            .collect()
    }

    #[test]
    fn a_file_without_a_server_keeps_only_the_rows_needing_none() {
        let rows = labels(&[]);
        assert!(rows.contains(&"editor-cut"));
        assert!(rows.contains(&"editor-change-all-occurrences"));
        assert!(!rows.contains(&"editor-rename-symbol"));
        assert!(!rows.contains(&"editor-format-document"));
    }

    #[test]
    fn a_row_appears_exactly_when_its_server_offers_it() {
        let rows = labels(&[EditorAction::Rename, EditorAction::GotoImplementation]);
        assert!(rows.contains(&"editor-rename-symbol"));
        assert!(rows.contains(&"editor-go-to-implementation"));
        assert!(!rows.contains(&"editor-go-to-declaration"));
        assert!(!rows.contains(&"editor-format-selection"));
    }

    #[test]
    fn each_group_opens_exactly_once() {
        let all = [
            EditorAction::GotoDeclaration,
            EditorAction::GotoTypeDefinition,
            EditorAction::GotoImplementation,
            EditorAction::Rename,
            EditorAction::FormatDocument,
            EditorAction::FormatSelection,
        ];
        let opens = EditorMenu::offering(&all)
            .rows()
            .into_iter()
            .filter(|row| row.opens_group)
            .count();
        assert_eq!(
            opens, 3,
            "modification, clipboard and the palette; navigation is first so it opens nothing"
        );
    }
}
