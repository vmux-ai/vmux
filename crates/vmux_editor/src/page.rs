#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;

use crate::explorer::ExplorerPanel;
use crate::note::MdBlockView;
use crate::page_model::{
    NoteCaretVisibilityQueue, NoteCaretVisibilityRequest, NoteCursorActivation, NoteInlineKind,
    NoteInlineNode, centered_scroll_top, clamp_selection, dir_select_index, editor_drag_started,
    gutter_width, heading_class, image_mime, line_severity, note_cursor_activation,
    note_inline_nodes, note_list_marker_prefix_len, note_source_offset, note_source_position,
    severity_color_class, should_apply_explorer_chrome, span_style, squiggle_style,
    viewport_reveal_delta,
};
use dioxus::prelude::*;
use vmux_core::event::*;
use vmux_core::knowledge::{KnowledgeProperty, KnowledgePropertyKind, KnowledgeReference};
use vmux_core::media::MediaKind;
use vmux_git::event::{GIT_CHANGED_EVENT, GitChangedEvent};
use vmux_git::ui::{DiffView, GitBar, GitFooter};
use vmux_git::view::EditorDiffMarker;
use vmux_ui::components::icon::Icon;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut path = use_signal(String::new);
    let mut total_lines = use_signal(|| 0u32);
    let mut total_rows = use_signal(|| 0u32);
    let mut first_row = use_signal(|| 0u32);
    let mut gutter_hover = use_signal(|| false);
    let mut lines = use_signal(Vec::<FileLine>::new);
    let mut line_layouts = use_signal(Vec::<FileLineLayout>::new);
    let mut wrap_columns = use_signal(|| 0u16);
    let mut diagnostics = use_signal(Vec::<FileDiagnostic>::new);
    let mut hover_diag = use_signal(|| Option::<FileDiagnostic>::None);
    let mut lsp_status = use_signal(|| Option::<FileLspStatusEvent>::None);
    let mut lsp_install_notice = use_signal(|| Option::<LspInstallProgress>::None);
    let mut lsp_install_request = use_signal(|| Option::<(String, String)>::None);
    let mut lsp_notice_generation = use_signal(|| 0u32);
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
    let cell_dims = use_signal(|| (0.0f64, 0.0f64));
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
    let mut note_edit_rect = use_signal(|| Option::<NoteEditRect>::None);
    let mut note_dragging = use_signal(|| false);
    let mut editor_dragging = use_signal(|| false);
    let mut editor_drag_origin = use_signal(|| Option::<(i32, i32)>::None);
    let mut git_nonce = use_signal(|| 0u32);
    let git_refresh_generation = use_signal(|| 0u32);
    let git_display = use_signal(String::new);
    let git_branch = use_signal(String::new);
    let git_ahead = use_signal(|| 0u32);
    let git_behind = use_signal(|| 0u32);
    let git_staged = use_signal(|| 0u32);
    let git_message = use_signal(String::new);
    let mut ed_mode = use_signal(|| vmux_core::editor::EditMode::Insert);
    let mut ed_label = use_signal(String::new);
    let mut ed_command_line = use_signal(String::new);
    let mut search_spans = use_signal(Vec::<vmux_core::editor::SelSpan>::new);
    let mut keymap = use_signal(vmux_core::KeymapKind::default);
    let mut cursor = use_signal(vmux_core::editor::CursorPos::default);
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
    let mut tidy_prompt = use_signal(|| Option::<u32>::None);

    let _chrome =
        use_bin_event_listener::<ExplorerChromeEvent, _>(EXPLORER_CHROME_EVENT, move |c| {
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
            schedule_explorer_visibility_sync(
                explorer_visible,
                explorer_preferred_visible,
                explorer_width,
            );
        });

    let _tidy =
        use_bin_event_listener::<FileTidyPromptEvent, _>(FILE_TIDY_PROMPT_EVENT, move |e| {
            tidy_prompt.set(Some(e.count));
        });

    let _meta = use_bin_event_listener::<FileMetaEvent, _>(FILE_META_EVENT, move |m| {
        error.set(String::new());
        clear_blob_state(preview, thumbs);
        media.set(None);
        reset_file_scroll();
        last_scroll_req.set(0);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = m.path.rsplit('/').next().unwrap_or(&m.path).to_string();
            doc.set_title(&name);
        }
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
        mode.set(Mode::Text);
        lsp_install_notice.set(None);
        lsp_install_request.set(None);
        lsp_notice_generation.set(lsp_notice_generation().wrapping_add(1));
        show_explorer_if_room(
            explorer_visible,
            explorer_preferred_visible,
            explorer_width,
            explorer_client_id,
            explorer_request_id,
            mode,
        );
        note_blocks.set(Vec::new());
        note_properties.set(Vec::new());
        note_references.set(Vec::new());
        note_active.set(None);
        note_editing.set(false);
        note_edit_line.set(None);
        note_edit_rect.set(None);
        note_dragging.set(false);
        editor_dragging.set(false);
        editor_drag_origin.set(None);
        git_nonce.set(git_nonce() + 1);
    });

    let _vp = use_bin_event_listener::<FileViewportPatch, _>(FILE_VIEWPORT_EVENT, move |p| {
        first_row.set(p.first_row);
        total_rows.set(p.total_rows);
        total_lines.set(p.total_lines);
        wrap_columns.set(p.wrap_columns);
        line_layouts.set(p.layouts);
        lines.set(p.lines);
        lsp_hover.set(None);
    });

    let _cur = use_bin_event_listener::<FileCursorEvent, _>(FILE_CURSOR_EVENT, move |c| {
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
                    note_edit_rect,
                );
            }
            if *note_editing.peek() {
                let is_list = active.is_some_and(|index| {
                    matches!(note_blocks.peek()[index].block, MdBlock::List { .. })
                });
                let edit_line = is_list.then_some(c.source_primary.line);
                let rect = active
                    .filter(|_| is_list)
                    .and_then(|index| note_list_edit_rect_for_line(index, c.source_primary.line));
                if *note_edit_line.peek() != edit_line {
                    note_edit_line.set(edit_line);
                }
                if *note_edit_rect.peek() != rect {
                    note_edit_rect.set(rect);
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
            ensure_line_visible(c.primary.row, cell_dims().1);
        }
    });

    let _scroll_by =
        use_bin_event_listener::<FileScrollByEvent, _>(FILE_SCROLL_BY_EVENT, move |event| {
            let line_height = if file_view_mode() == FileViewMode::Note {
                28.0
            } else {
                cell_dims().1
            };
            if line_height <= 0.0 {
                return;
            }
            scroll_viewport_by(event.lines, line_height);
        });

    let _dirty = use_bin_event_listener::<FileDirtyEvent, _>(FILE_DIRTY_EVENT, move |d| {
        dirty.set(d.dirty);
        schedule_git_refresh(git_refresh_generation, git_nonce);
    });

    let _git_changed = use_bin_event_listener::<GitChangedEvent, _>(GIT_CHANGED_EVENT, move |_| {
        schedule_git_refresh(git_refresh_generation, git_nonce);
    });

    let _view_mode =
        use_bin_event_listener::<FileViewModeEvent, _>(FILE_VIEW_MODE_EVENT, move |event| {
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
                            note_edit_rect,
                        );
                    }
                }
                FileViewMode::Editor => {
                    schedule_line_center(cursor().row, cell_dims().1, true);
                }
                _ => {}
            }
        });

    let _keymap = use_bin_event_listener::<FileKeymapEvent, _>(FILE_KEYMAP_EVENT, move |event| {
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
                    note_edit_rect,
                );
            }
        }
    });

    let _note = use_bin_event_listener::<FileNoteEvent, _>(FILE_NOTE_EVENT, move |event| {
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
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title(&title);
        }
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
                    note_edit_rect,
                ),
                NoteCursorActivation::PreserveViewport(_) => activate_note_cursor(
                    index,
                    line,
                    note_active,
                    note_editing,
                    note_edit_line,
                    note_edit_rect,
                ),
            }
        }
    });

    let _hov = use_bin_event_listener::<FileHoverEvent, _>(FILE_HOVER_EVENT, move |h| {
        lsp_hover.set(Some(h));
    });

    let _refs = use_bin_event_listener::<FileReferencesEvent, _>(FILE_REFERENCES_EVENT, move |e| {
        refs.set(e.items);
        refs_sel.set(0);
        refs_open.set(true);
        focus_by_id("refs-panel");
    });

    let _comp = use_bin_event_listener::<FileCompletionEvent, _>(FILE_COMPLETION_EVENT, move |e| {
        comp_open.set(!e.items.is_empty());
        comps.set(e.items);
        comp_sel.set(0);
        comp_anchor.set((e.line, e.replace_from_col));
    });

    let _diag =
        use_bin_event_listener::<FileDiagnosticsEvent, _>(FILE_DIAGNOSTICS_EVENT, move |d| {
            if d.path != git_path() {
                return;
            }
            diagnostics.set(d.diagnostics);
        });

    let _lsp_status =
        use_bin_event_listener::<FileLspStatusEvent, _>(FILE_LSP_STATUS_EVENT, move |s| {
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
                    let _ = try_cef_bin_emit_rkyv(&LspInstallRequest { name: package });
                }
            }
            lsp_status.set(Some(s));
        });

    let _lsp_install_progress = use_bin_event_listener::<LspInstallProgress, _>(
        LSP_INSTALL_PROGRESS_EVENT,
        move |progress| {
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
        },
    );

    let _lsp_package_status =
        use_bin_event_listener::<LspPkgStatusEvent, _>(LSP_PKG_STATUS_EVENT, move |status| {
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

    let _err = use_bin_event_listener::<FileErrorEvent, _>(FILE_ERROR_EVENT, move |e| {
        error.set(e.message);
    });

    let _dir = use_bin_event_listener::<FileDirEvent, _>(FILE_DIR_EVENT, move |d| {
        error.set(String::new());
        clear_blob_state(preview, thumbs);
        media.set(None);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let name = d
                .path
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or(&d.path)
                .to_string();
            doc.set_title(&name);
        }
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

    let _media = use_bin_event_listener::<FileMediaEvent, _>(FILE_MEDIA_EVENT, move |e| {
        error.set(String::new());
        clear_blob_state(preview, thumbs);
        let kind = e.kind;
        media.set(Some(e));
        mode.set(Mode::Media(kind));
        diagnostics.set(Vec::new());
        hover_diag.set(None);
        lsp_status.set(None);
    });

    let _prev = use_bin_event_listener::<FilePreviewEvent, _>(FILE_PREVIEW_EVENT, move |ev| {
        if ev.thumb {
            if let PreviewKind::Image { bytes, .. } = ev.kind
                && let Some(url) = blob_url(&bytes)
            {
                let old = thumbs.write().insert(ev.path.clone(), url);
                if let Some(old) = old {
                    revoke(&old);
                }
            }
            return;
        }
        let vis = visible_entries(&dir_entries.read(), show_hidden());
        let sel_path = vis.get(selected()).map(|e| e.path.clone());
        if sel_path.as_deref() != Some(ev.path.as_str()) {
            return;
        }
        let next = match ev.kind {
            PreviewKind::Image { bytes, .. } => match blob_url(&bytes) {
                Some(u) => Preview::Image(u),
                None => Preview::Error(translate("editor-failed-decode-image")),
            },
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
        if let Preview::Image(old) = &*preview.read() {
            revoke(old);
        }
        preview.set(next);
    });

    let _theme = use_bin_event_listener::<FileThemeEvent, _>(FILE_THEME_EVENT, move |t| {
        let mut s = String::new();
        if !t.font_family.is_empty() {
            s.push_str(&format!(
                "font-family:\"{}\",\"JetBrainsMono NF\",monospace;",
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
        let _ = file_view_mode();
        setup_measurement(
            cell_dims,
            total_lines,
            last_resize,
            explorer_visible,
            explorer_preferred_visible,
            explorer_width,
        );
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
    let header_path = {
        let g = git_display();
        if g.is_empty() { path() } else { g }
    };

    let comp_filtered: Vec<CompletionItem> = if comp_open() {
        let (cline, cfrom) = comp_anchor();
        let lt: String = lines()
            .iter()
            .find(|l| l.line_no == cline)
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .unwrap_or_default();
        let chars: Vec<char> = lt.chars().collect();
        let caret = cursor().col as usize;
        let from = cfrom as usize;
        let prefix: String = if from <= caret && from <= chars.len() {
            chars[from..caret.min(chars.len())].iter().collect()
        } else {
            String::new()
        };
        let pl = prefix.to_lowercase();
        comps()
            .into_iter()
            .filter(|c| c.label.to_lowercase().starts_with(&pl))
            .collect()
    } else {
        Vec::new()
    };
    let comp_sel_clamped = comp_sel().min(comp_filtered.len().saturating_sub(1));
    let comp_keys = comp_filtered.clone();

    rsx! {
        div {
            id: PAGE_ID,
            class: "flex h-full w-full flex-row overflow-hidden bg-background",
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
                    let _ = try_cef_bin_emit_rkyv(&ExplorerPanelWidth { px: explorer_width() });
                }
            },

            ExplorerSidebar {
                visible: explorer_visible,
                preferred_visible: explorer_preferred_visible,
                width: explorer_width,
                resizing: explorer_resizing,
                client_id: explorer_client_id,
                request_id: explorer_request_id,
                mode,
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
                if handle_explorer_shortcut(
                    &e,
                    explorer_visible,
                    explorer_preferred_visible,
                    explorer_width,
                    explorer_client_id,
                    explorer_request_id,
                    mode,
                ) {
                    return;
                }
                let data = e.data();
                let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
                    return;
                };
                let key = raw.key();
                if mode() == Mode::Text
                    && file_view_mode() == FileViewMode::Note
                    && is_markdown_file(&git_path())
                    && !note_editing()
                {
                    let _ = forward_file_key(&e, raw, ed_mode());
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

            div {
                class: "flex h-9 shrink-0 items-center gap-2 border-b border-foreground/[0.07] bg-foreground/[0.06] px-4 font-sans text-xs text-muted-foreground",
                ExplorerToggleButton {
                    visible: explorer_visible,
                    preferred_visible: explorer_preferred_visible,
                    width: explorer_width,
                    client_id: explorer_client_id,
                    request_id: explorer_request_id,
                    mode,
                }
                {rsx! { TypeIcon { path: header_path.to_string(), is_dir: mode() == Mode::Dir, class: "h-4 w-4 shrink-0 text-foreground/80" } }}
                span { class: "truncate text-foreground/90", "{header_path}" }
                if dirty() {
                    span { class: "h-1.5 w-1.5 shrink-0 rounded-full bg-cyan-300", title: translate("editor-unsaved") }
                }
                div { class: "flex-1" }
                if mode() == Mode::Text {
                    if is_markdown_file(&git_path()) || git_has_diff() {
                        div { class: "flex shrink-0 items-center gap-0.5 rounded-md bg-foreground/[0.06] p-0.5 text-[10px] font-medium ring-1 ring-inset ring-foreground/10",
                            if is_markdown_file(&git_path()) {
                                button {
                                    class: file_mode_class(file_view_mode() == FileViewMode::Note),
                                    title: translate("editor-rendered-markdown"),
                                    onclick: move |_| {
                                        file_view_mode.set(FileViewMode::Note);
                                        let _ = try_cef_bin_emit_rkyv(&FileViewModeSet { mode: FileViewMode::Note });
                                        let line = source_cursor().line;
                                        if let Some(index) = note_block_index_for_line(&note_blocks.read(), line) {
                                            activate_note_cursor_centered(
                                                index,
                                                line,
                                                note_active,
                                                note_editing,
                                                note_edit_line,
                                                note_edit_rect,
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
                                    schedule_line_center(cursor().row, cell_dims().1, true);
                                    let _ = try_cef_bin_emit_rkyv(&FileViewModeSet { mode: FileViewMode::Editor });
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
                                        let _ = try_cef_bin_emit_rkyv(&FileViewModeSet { mode: FileViewMode::Diff });
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
                                let _ = try_cef_bin_emit_rkyv(&FileKeymapSet { keymap: next });
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
                                let _ = try_cef_bin_emit_rkyv(&FileKeymapSet { keymap: next });
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
                                        let _ = try_cef_bin_emit_rkyv(&FileTidyActionEvent { choice: TidyChoice::Tidy });
                                        tidy_prompt.set(None);
                                    },
                                    {translate("editor-tidy")}
                                }
                                button {
                                    class: "rounded-full px-2 py-0.5 text-foreground/60 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = try_cef_bin_emit_rkyv(&FileTidyActionEvent { choice: TidyChoice::Always });
                                        tidy_prompt.set(None);
                                    },
                                    {translate("editor-always")}
                                }
                                button {
                                    class: "rounded-full px-1.5 py-0.5 text-foreground/40 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = try_cef_bin_emit_rkyv(&FileTidyActionEvent { choice: TidyChoice::Dismiss });
                                        tidy_prompt.set(None);
                                    },
                                    "\u{2715}"
                                }
                            }
                        }
                    })
                }
            }

            GitBar {
                path: git_path,
                has_diff: git_has_diff,
                nonce: git_nonce,
                display_path: git_display,
                branch: git_branch,
                ahead: git_ahead,
                behind: git_behind,
                staged_count: git_staged,
                message: git_message,
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
                                                    let _ = try_cef_bin_emit_rkyv(&FileOpenExternalRequest { path: abs.clone() });
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
                            let note_input_comp_keys = comp_keys.clone();
                            rsx! {
                                div {
                                    id: "file-scroll",
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
                                                    note_edit_rect,
                                                );
                                            }
                                            return;
                                        }
                                        if note_editing() {
                                            note_editing.set(false);
                                            note_active.set(None);
                                            note_edit_line.set(None);
                                            note_edit_rect.set(None);
                                            focus_container();
                                        }
                                    },
                                    onpointermove: move |event: Event<PointerData>| {
                                        if !note_dragging() {
                                            return;
                                        }
                                        let data = event.data();
                                        let Some(pointer) = data.downcast::<web_sys::PointerEvent>() else {
                                            return;
                                        };
                                        if pointer.buttons() & 1 != 1 {
                                            note_dragging.set(false);
                                            set_pointer_capture(&event, "file-scroll", false);
                                            return;
                                        }
                                        event.stop_propagation();
                                        event.prevent_default();
                                        if let Some((line, col)) = note_pointer_position_at(
                                            pointer.client_x() as f64,
                                            pointer.client_y() as f64,
                                            &note_blocks.read(),
                                        ) {
                                            let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                                line,
                                                col,
                                                extend: true,
                                            });
                                        }
                                    },
                                    onpointerup: move |event: Event<PointerData>| {
                                        set_pointer_capture(&event, "file-scroll", false);
                                        note_dragging.set(false);
                                    },
                                    onpointercancel: move |event: Event<PointerData>| {
                                        set_pointer_capture(&event, "file-scroll", false);
                                        note_dragging.set(false);
                                    },
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
                                                        note_edit_rect,
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
                                            id: "file-input",
                                            class: "pointer-events-none absolute left-0 top-0 h-px w-px resize-none overflow-hidden border-0 bg-transparent p-0 opacity-0 outline-none",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| composing.set(true),
                                            oncompositionend: move |_| {
                                                composing.set(false);
                                                send_committed_text();
                                            },
                                            oninput: move |_| {
                                                if !composing() {
                                                    send_committed_text();
                                                }
                                            },
                                            onkeydown: move |event: Event<KeyboardData>| {
                                                let data = event.data();
                                                let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
                                                    return;
                                                };
                                                event.stop_propagation();
                                                if raw.is_composing() {
                                                    return;
                                                }
                                                let key = raw.key();
                                                let mods = key_mods(raw);
                                                if comp_open() && !note_input_comp_keys.is_empty() {
                                                    match key.as_str() {
                                                        "ArrowDown" => {
                                                            event.prevent_default();
                                                            comp_sel.set((comp_sel_clamped + 1).min(note_input_comp_keys.len() - 1));
                                                            return;
                                                        }
                                                        "ArrowUp" => {
                                                            event.prevent_default();
                                                            comp_sel.set(comp_sel_clamped.saturating_sub(1));
                                                            return;
                                                        }
                                                        "Enter" | "Tab" => {
                                                            event.prevent_default();
                                                            if let Some(item) = note_input_comp_keys.get(comp_sel_clamped) {
                                                                let (line, replace_from_col) = comp_anchor();
                                                                let _ = try_cef_bin_emit_rkyv(&FileCompletionCommit {
                                                                    line,
                                                                    replace_from_col,
                                                                    text: item.insert_text.clone(),
                                                                });
                                                            }
                                                            comp_open.set(false);
                                                            return;
                                                        }
                                                        "Escape" => {
                                                            event.prevent_default();
                                                            comp_open.set(false);
                                                            return;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                if key == "Escape" {
                                                    event.prevent_default();
                                                    if keymap() != vmux_core::KeymapKind::Vim {
                                                        note_editing.set(false);
                                                    }
                                                    let _ = try_cef_bin_emit_rkyv(&FileKeyEvent {
                                                        key,
                                                        code: raw.code(),
                                                        mods,
                                                        repeat: raw.repeat(),
                                                    });
                                                    if keymap() == vmux_core::KeymapKind::Vim {
                                                        focus_file_input();
                                                    } else {
                                                        focus_container();
                                                    }
                                                    return;
                                                }
                                                let _ = forward_file_key(&event, raw, ed_mode());
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
                                                                        let _ = try_cef_bin_emit_rkyv(&KnowledgeLinkOpen {
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
                                    id: "file-scroll",
                                    class: "file-mode-editor-enter relative min-h-0 flex-1 overflow-auto",
                                    onmouseleave: move |_| {
                                        lsp_hover.set(None);
                                        hover_pos.set(None);
                                        gutter_hover.set(false);
                                    },
                                    onpointermove: move |event: Event<PointerData>| {
                                        let data = event.data();
                                        let Some(pointer) = data.downcast::<web_sys::PointerEvent>() else {
                                            return;
                                        };
                                        let Some(origin) = editor_drag_origin() else {
                                            return;
                                        };
                                        if pointer.buttons() & 1 != 1 {
                                            editor_dragging.set(false);
                                            editor_drag_origin.set(None);
                                            set_pointer_capture(&event, "file-scroll", false);
                                            return;
                                        }
                                        if !editor_dragging() {
                                            if !editor_drag_started(
                                                origin,
                                                (pointer.client_x(), pointer.client_y()),
                                            ) {
                                                return;
                                            }
                                            editor_dragging.set(true);
                                        }
                                        let (cw, ch) = cell_dims();
                                        let gutter = gw as f64 * cw + 48.0;
                                        if let Some((line, col)) = editor_pointer_file_position(
                                            pointer,
                                            gutter,
                                            cw,
                                            ch,
                                            &line_layouts.read(),
                                            wrap_columns(),
                                            false,
                                        ) {
                                            event.prevent_default();
                                            let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                                line,
                                                col,
                                                extend: true,
                                            });
                                        }
                                    },
                                    onpointerup: move |event: Event<PointerData>| {
                                        set_pointer_capture(&event, "file-scroll", false);
                                        editor_dragging.set(false);
                                        editor_drag_origin.set(None);
                                    },
                                    onpointercancel: move |event: Event<PointerData>| {
                                        set_pointer_capture(&event, "file-scroll", false);
                                        editor_dragging.set(false);
                                        editor_drag_origin.set(None);
                                    },
                                    onscroll: move |_| {
                                        let (_, ch) = cell_dims();
                                        if ch <= 0.0 {
                                            return;
                                        }
                                        let Some(el) = scroll_el() else {
                                            return;
                                        };
                                        let vis_first = (el.scroll_top() as f64 / ch).floor().max(0.0) as u32;
                                        let vis_rows = (el.client_height() as f64 / ch).ceil() as u32 + 1;
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
                                            let _ = try_cef_bin_emit_rkyv(&FileScrollEvent { top_row: vis_first });
                                        }
                                    },
                                    div { class: "relative", style: "height:{spacer}px;",
                                        for (i, line) in lines().iter().enumerate() {
                                            {
                                                let ln = line.line_no;
                                                let layout = line_layouts().get(i).copied().unwrap_or(FileLineLayout {
                                                    line_no: ln,
                                                    row: first_row() + i as u32,
                                                    rows: 1,
                                                });
                                                let lt = layout.row as f64 * ch;
                                                let line_height = layout.rows as f64 * ch;
                                                let wrap_cols = wrap_columns();
                                                let text_class = if wrap_cols > 0 {
                                                    "relative whitespace-pre-wrap break-all pr-8"
                                                } else {
                                                    "relative whitespace-pre pr-8"
                                                };
                                                let text_style = if wrap_cols > 0 {
                                                    format!("box-sizing:border-box;width:calc(var(--cw) * {wrap_cols} + 2rem);")
                                                } else {
                                                    String::new()
                                                };
                                                let fold = line.fold;
                                                let diags = diagnostics();
                                                let sev = line_severity(&diags, ln);
                                                let diff_marker = git_line_markers().get(&(ln + 1)).copied();
                                                let line_diags: Vec<FileDiagnostic> = diags
                                                    .iter()
                                                    .filter(|d| d.line == ln)
                                                    .cloned()
                                                    .collect();
                                                rsx! {
                                                    div {
                                                        key: "{ln}",
                                                        class: if let Some(marker) = diff_marker { "group flex items-start {diff_marker_row_class(marker)}" } else { "group flex items-start hover:bg-foreground/[0.035]" },
                                                        style: "position:absolute;left:0;right:0;top:{lt}px;height:{line_height}px;",
                                                        onpointerdown: move |e: Event<PointerData>| {
                                                            e.prevent_default();
                                                            ctx_menu.set(None);
                                                            let (cw, ch) = cell_dims();
                                                            let g = gw as f64 * cw + 48.0;
                                                            let dd = e.data();
                                                            if let Some(raw) = dd.downcast::<web_sys::PointerEvent>()
                                                                && let Some((line, col)) = editor_pointer_file_position(
                                                                    raw,
                                                                    g,
                                                                    cw,
                                                                    ch,
                                                                    &line_layouts.read(),
                                                                    wrap_cols,
                                                                    true,
                                                                )
                                                            {
                                                                if raw.meta_key() {
                                                                    editor_dragging.set(false);
                                                                    editor_drag_origin.set(None);
                                                                    let _ = try_cef_bin_emit_rkyv(&FileDefinitionRequest {
                                                                        line,
                                                                        col,
                                                                    });
                                                                } else {
                                                                    editor_dragging.set(false);
                                                                    editor_drag_origin.set(Some((
                                                                        raw.client_x(),
                                                                        raw.client_y(),
                                                                    )));
                                                                    set_pointer_capture(&e, "file-scroll", true);
                                                                    let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                                                        line,
                                                                        col,
                                                                        extend: raw.shift_key(),
                                                                    });
                                                                }
                                                            }
                                                            focus_file_input();
                                                        },
                                                        oncontextmenu: move |e: Event<MouseData>| {
                                                            e.prevent_default();
                                                            let (cw, ch) = cell_dims();
                                                            let g = gw as f64 * cw + 48.0;
                                                            let dd = e.data();
                                                            if let Some(raw) = dd.downcast::<web_sys::MouseEvent>()
                                                                && let Some(t) = raw
                                                                    .current_target()
                                                                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                                            {
                                                                let (_, col) = editor_pointer_position(
                                                                    raw,
                                                                    &t,
                                                                    g,
                                                                    cw,
                                                                    ch,
                                                                    wrap_cols,
                                                                    true,
                                                                );
                                                                ctx_menu.set(Some((
                                                                    raw.client_x() as f64,
                                                                    raw.client_y() as f64,
                                                                    ln,
                                                                    col,
                                                                )));
                                                            }
                                                        },
                                                        onmousemove: move |e: Event<MouseData>| {
                                                            if editor_dragging() {
                                                                return;
                                                            }
                                                            let (cw, ch) = cell_dims();
                                                            let g = gw as f64 * cw + 48.0;
                                                            let dd = e.data();
                                                            if let Some(raw) = dd.downcast::<web_sys::MouseEvent>()
                                                                && let Some(t) = raw
                                                                    .current_target()
                                                                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                                            {
                                                                let (x, pointer_col) = editor_pointer_position(
                                                                    raw,
                                                                    &t,
                                                                    g,
                                                                    cw,
                                                                    ch,
                                                                    wrap_cols,
                                                                    false,
                                                                );
                                                                let in_gutter = x < 0.0;
                                                                if gutter_hover() != in_gutter {
                                                                    gutter_hover.set(in_gutter);
                                                                }
                                                                if x < 0.0 {
                                                                    return;
                                                                }
                                                                let col = pointer_col;
                                                                if hover_pos() != Some((ln, col)) {
                                                                    hover_pos.set(Some((ln, col)));
                                                                    lsp_hover.set(None);
                                                                    let _ = try_cef_bin_emit_rkyv(&FileHoverRequest {
                                                                        line: ln,
                                                                        col,
                                                                    });
                                                                }
                                                            }
                                                        },
                                                        span {
                                                            class: "sticky left-0 z-[1] relative flex shrink-0 select-none items-center justify-end bg-background pl-4 pr-5 tabular-nums",
                                                            style: "min-width:calc(var(--cw, 1ch) * {gw} + 3rem);height:{ch}px;",
                                                            if let Some(s) = sev {
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
                                                                    span {
                                                                        title: translate("editor-changed-line"),
                                                                        "{diff_marker_sign(marker)}"
                                                                    }
                                                                }
                                                            }
                                                            match fold {
                                                                FoldGutter::Open => {
                                                                    let vis = if gutter_hover() { "opacity-100" } else { "opacity-0" };
                                                                    rsx! {
                                                                        span {
                                                                            class: "absolute right-1 flex h-full cursor-pointer items-center text-base leading-none text-foreground/50 transition-opacity hover:!text-foreground {vis}",
                                                                            onmousedown: move |e: Event<MouseData>| {
                                                                                e.stop_propagation();
                                                                                e.prevent_default();
                                                                                let _ = try_cef_bin_emit_rkyv(&FileFoldToggle { line: ln });
                                                                            },
                                                                            "⌄"
                                                                        }
                                                                    }
                                                                }
                                                                FoldGutter::Collapsed => rsx! {
                                                                    span {
                                                                        class: "absolute right-1 flex h-full cursor-pointer items-center text-base leading-none text-foreground/70 hover:!text-foreground",
                                                                        onmousedown: move |e: Event<MouseData>| {
                                                                            e.stop_propagation();
                                                                            e.prevent_default();
                                                                            let _ = try_cef_bin_emit_rkyv(&FileFoldToggle { line: ln });
                                                                        },
                                                                        "›"
                                                                    }
                                                                },
                                                                FoldGutter::None => rsx! {},
                                                            }
                                                        }
                                                        span { class: "{text_class}", style: "{text_style}",
                                                            for (i, s) in line.spans.iter().enumerate() {
                                                                span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                                                            }
                                                            for (di, d) in line_diags.iter().enumerate() {
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
                                                                span {
                                                                    class: "ml-1 rounded bg-white/10 px-1 text-foreground/40",
                                                                    "⋯"
                                                                }
                                                            }
                                                        }
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
                                                        key: "sel{s.line}",
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

                                        textarea {
                                            id: "file-input",
                                            class: "absolute z-10 resize-none overflow-hidden whitespace-pre border-0 bg-transparent p-0 caret-transparent outline-none",
                                            style: "left:{cx}px;top:{cy}px;min-width:2ch;height:{ch}px;color:{txtcol};",
                                            autocomplete: "off",
                                            autocapitalize: "off",
                                            spellcheck: "false",
                                            oncompositionstart: move |_| composing.set(true),
                                            oncompositionend: move |_| {
                                                composing.set(false);
                                                send_committed_text();
                                            },
                                            oninput: move |_| {
                                                if composing() {
                                                    return;
                                                }
                                                send_committed_text();
                                            },
                                            onkeydown: move |e: Event<KeyboardData>| {
                                                let dd = e.data();
                                                let Some(raw) = dd.downcast::<web_sys::KeyboardEvent>() else {
                                                    return;
                                                };
                                                e.stop_propagation();
                                                if raw.is_composing() {
                                                    return;
                                                }
                                                let key = raw.key();
                                                if comp_open() && !comp_keys.is_empty() {
                                                    match key.as_str() {
                                                        "ArrowDown" => {
                                                            e.prevent_default();
                                                            comp_sel.set((comp_sel_clamped + 1).min(comp_keys.len() - 1));
                                                            return;
                                                        }
                                                        "ArrowUp" => {
                                                            e.prevent_default();
                                                            comp_sel.set(comp_sel_clamped.saturating_sub(1));
                                                            return;
                                                        }
                                                        "Enter" | "Tab" => {
                                                            e.prevent_default();
                                                            if let Some(it) = comp_keys.get(comp_sel_clamped) {
                                                                let (cline, cfrom) = comp_anchor();
                                                                let _ = try_cef_bin_emit_rkyv(&FileCompletionCommit {
                                                                    line: cline,
                                                                    replace_from_col: cfrom,
                                                                    text: it.insert_text.clone(),
                                                                });
                                                            }
                                                            comp_open.set(false);
                                                            return;
                                                        }
                                                        "Escape" => {
                                                            e.prevent_default();
                                                            comp_open.set(false);
                                                            return;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                let _ = forward_file_key(&e, raw, ed_mode());
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
                                                        class: "pointer-events-none absolute z-30 max-w-2xl overflow-hidden rounded-xl bg-foreground/[0.05] px-3 py-2 text-xs leading-snug text-foreground/90 ring-1 ring-inset ring-cyan-400/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
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
                        class: "fixed z-50 min-w-44 overflow-hidden rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
                        style: "left:{x}px;top:{y}px;",
                        div {
                            class: "cursor-default px-3 py-1.5 hover:bg-cyan-400/15",
                            onmousedown: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let _ = try_cef_bin_emit_rkyv(&FileDefinitionRequest { line, col });
                                ctx_menu.set(None);
                            },
                            {translate("editor-go-to-definition")}
                        }
                        div {
                            class: "cursor-default px-3 py-1.5 hover:bg-cyan-400/15",
                            onmousedown: move |e: Event<MouseData>| {
                                e.prevent_default();
                                let _ = try_cef_bin_emit_rkyv(&FileReferencesRequest { line, col });
                                ctx_menu.set(None);
                            },
                            {translate("editor-find-references")}
                        }
                    }
                })
            }

            {
                // Vim puts the command line on the last screen row; mirror that rather than
                // tucking it in the header, where it reads as a label instead of a prompt.
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
                                let key = e
                                    .data()
                                    .downcast::<web_sys::KeyboardEvent>()
                                    .map(|k| k.key())
                                    .unwrap_or_default();
                                let len = refs.read().len();
                                match key.as_str() {
                                    "ArrowDown" | "j" => {
                                        e.prevent_default();
                                        if len > 0 {
                                            refs_sel.set((refs_sel() + 1).min(len - 1));
                                        }
                                    }
                                    "ArrowUp" | "k" => {
                                        e.prevent_default();
                                        refs_sel.set(refs_sel().saturating_sub(1));
                                    }
                                    "Enter" => {
                                        e.prevent_default();
                                        if let Some(it) = refs.read().get(refs_sel()) {
                                            let _ = try_cef_bin_emit_rkyv(&FileGotoRequest {
                                                path: it.path.clone(),
                                                line: it.line,
                                                col: it.col,
                                            });
                                        }
                                        refs_open.set(false);
                                        focus_file_input();
                                    }
                                    "Escape" => {
                                        e.prevent_default();
                                        refs_open.set(false);
                                        focus_file_input();
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
                                                let _ = try_cef_bin_emit_rkyv(&FileGotoRequest {
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
            }
        }
        }
    }
}

const CONTAINER_ID: &str = "file-container";
const PAGE_ID: &str = "file-page";
const MEASURE_ID: &str = "file-measure";
const NOTE_CARET_ID: &str = "note-caret";
const VIDEO_HOST_ID: &str = "vmux-video-host";
const INPUT_ID: &str = "file-input";
const SCROLL_ID: &str = "file-scroll";
const GIT_REFRESH_DEBOUNCE_MS: i32 = 120;
const NOTE_MAX_CONTENT_WIDTH_PX: u32 = 768;
const LSP_NOTICE_DONE_MS: i32 = 2_500;
const LSP_NOTICE_FAILED_MS: i32 = 6_000;

std::thread_local! {
    static NOTE_CARET_VISIBILITY_QUEUE: RefCell<NoteCaretVisibilityQueue> = RefCell::new(NoteCaretVisibilityQueue::default());
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

fn editor_pointer_position(
    event: &web_sys::MouseEvent,
    target: &web_sys::Element,
    gutter: f64,
    char_width: f64,
    char_height: f64,
    wrap_columns: u16,
    round: bool,
) -> (f64, u32) {
    let rect = target.get_bounding_client_rect();
    let x = event.client_x() as f64 - rect.left() - gutter;
    if char_width <= 0.0 {
        return (x, 0);
    }
    let local = if round {
        (x.max(0.0) / char_width).round()
    } else {
        (x.max(0.0) / char_width).floor()
    } as u32;
    if wrap_columns == 0 || char_height <= 0.0 {
        return (x, local);
    }
    let wrapped_row =
        ((event.client_y() as f64 - rect.top()).max(0.0) / char_height).floor() as u32;
    (
        x,
        wrapped_row * wrap_columns as u32 + local.min(wrap_columns as u32),
    )
}

fn set_pointer_capture(event: &Event<PointerData>, element_id: &str, capture: bool) {
    let data = event.data();
    let Some(pointer) = data.downcast::<web_sys::PointerEvent>() else {
        return;
    };
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(element_id))
    else {
        return;
    };
    if capture {
        let _ = element.set_pointer_capture(pointer.pointer_id());
    } else {
        let _ = element.release_pointer_capture(pointer.pointer_id());
    }
}

fn editor_pointer_file_position(
    pointer: &web_sys::PointerEvent,
    gutter: f64,
    char_width: f64,
    char_height: f64,
    layouts: &[FileLineLayout],
    wrap_columns: u16,
    round: bool,
) -> Option<(u32, u32)> {
    if char_width <= 0.0 || char_height <= 0.0 {
        return None;
    }
    let scroll = scroll_el()?;
    let rect = scroll.get_bounding_client_rect();
    let content_y = pointer.client_y() as f64 - rect.top() + scroll.scroll_top() as f64;
    let row = (content_y.max(0.0) / char_height).floor() as u32;
    let layout = layouts
        .iter()
        .find(|layout| row >= layout.row && row < layout.row + layout.rows as u32)?;
    let x = pointer.client_x() as f64 - rect.left() + scroll.scroll_left() as f64 - gutter;
    let local = if round {
        (x.max(0.0) / char_width).round()
    } else {
        (x.max(0.0) / char_width).floor()
    } as u32;
    let col = if wrap_columns == 0 {
        local
    } else {
        (row - layout.row) * wrap_columns as u32 + local.min(wrap_columns as u32)
    };
    Some((layout.line_no, col))
}

#[derive(Clone, Copy, PartialEq)]
struct NoteEditRect {
    top: f64,
    left: f64,
    width: f64,
    height: f64,
}

fn note_list_item_line(event: &Event<MouseData>) -> Option<u32> {
    let data = event.data();
    let raw = data.downcast::<web_sys::MouseEvent>()?;
    raw.target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())?
        .closest("[data-note-list-line]")
        .ok()
        .flatten()?
        .get_attribute("data-note-list-line")?
        .parse()
        .ok()
}

fn note_list_edit_rect(event: &Event<MouseData>, block_index: usize) -> Option<NoteEditRect> {
    let data = event.data();
    let raw = data.downcast::<web_sys::MouseEvent>()?;
    let target = raw
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())?;
    let item = target.closest("[data-note-list-line]").ok().flatten()?;
    let line = item.get_attribute("data-note-list-line")?.parse().ok()?;
    note_list_edit_rect_for_line(block_index, line)
}

fn note_list_edit_rect_for_line(block_index: usize, line: u32) -> Option<NoteEditRect> {
    let document = web_sys::window()?.document()?;
    let block = document.get_element_by_id(&format!("note-block-{block_index}"))?;
    let item = block
        .query_selector(&format!("[data-note-list-line=\"{line}\"]"))
        .ok()
        .flatten()?;
    let content = item
        .query_selector(":scope > p")
        .ok()
        .flatten()
        .unwrap_or_else(|| item.clone());
    let block_rect = block.get_bounding_client_rect();
    let item_rect = content.get_bounding_client_rect();
    Some(NoteEditRect {
        top: item_rect.top() - block_rect.top(),
        left: item_rect.left() - block_rect.left(),
        width: item_rect.width(),
        height: item_rect.height(),
    })
}

fn activate_note_cursor(
    block_index: usize,
    line: u32,
    note_active: Signal<Option<u32>>,
    note_editing: Signal<bool>,
    note_edit_line: Signal<Option<u32>>,
    note_edit_rect: Signal<Option<NoteEditRect>>,
) {
    set_note_cursor_active(
        block_index,
        line,
        note_active,
        note_editing,
        note_edit_line,
        note_edit_rect,
        false,
    );
}

fn activate_note_cursor_centered(
    block_index: usize,
    line: u32,
    note_active: Signal<Option<u32>>,
    note_editing: Signal<bool>,
    note_edit_line: Signal<Option<u32>>,
    note_edit_rect: Signal<Option<NoteEditRect>>,
) {
    set_note_cursor_active(
        block_index,
        line,
        note_active,
        note_editing,
        note_edit_line,
        note_edit_rect,
        true,
    );
}

fn set_note_cursor_active(
    block_index: usize,
    line: u32,
    mut note_active: Signal<Option<u32>>,
    mut note_editing: Signal<bool>,
    mut note_edit_line: Signal<Option<u32>>,
    mut note_edit_rect: Signal<Option<NoteEditRect>>,
    center: bool,
) {
    note_active.set(Some(block_index as u32));
    note_editing.set(true);
    note_edit_line.set(Some(line));
    note_edit_rect.set(None);
    schedule_note_cursor_activation(block_index, line, note_edit_rect, center, true);
}

fn schedule_note_cursor_activation(
    block_index: usize,
    line: u32,
    mut note_edit_rect: Signal<Option<NoteEditRect>>,
    center: bool,
    retry: bool,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        note_edit_rect.set(note_list_edit_rect_for_line(block_index, line));
        focus_file_input();
        if center {
            center_note_caret(block_index, line);
        }
        if retry {
            schedule_note_cursor_activation(block_index, line, note_edit_rect, center, false);
        }
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn browser_has_text_selection() -> bool {
    web_sys::window()
        .and_then(|window| window.get_selection().ok().flatten())
        .is_some_and(|selection| !selection.is_collapsed())
}

fn note_pointer_line(event: &Event<MouseData>, start: u32, end: u32, block: &MdBlock) -> u32 {
    if matches!(block, MdBlock::List { .. })
        && let Some(line) = note_list_item_line(event)
    {
        return line;
    }
    let count = end.saturating_sub(start).max(1);
    let data = event.data();
    let Some(raw) = data.downcast::<web_sys::MouseEvent>() else {
        return start;
    };
    let Some(target) = raw
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    else {
        return start;
    };
    let rect = target.get_bounding_client_rect();
    if rect.height() <= 0.0 {
        return start;
    }
    let ratio = ((raw.client_y() as f64 - rect.top()) / rect.height()).clamp(0.0, 1.0);
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

fn note_pointer_col_from_pointer(event: &Event<PointerData>, text: &str) -> u32 {
    let data = event.data();
    let Some(raw) = data.downcast::<web_sys::PointerEvent>() else {
        return 0;
    };
    let Some(target) = raw
        .current_target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    else {
        return 0;
    };
    note_col_at_point(&target, raw.client_x() as f64, raw.client_y() as f64, text)
}

fn note_pointer_position_at(
    client_x: f64,
    client_y: f64,
    blocks: &[NoteBlock],
) -> Option<(u32, u32)> {
    let document = web_sys::window()?.document()?;
    let target = document.element_from_point(client_x as f32, client_y as f32)?;
    if let Some(block_element) = target.closest("[data-note-edit-block]").ok().flatten() {
        let index = block_element
            .get_attribute("data-note-edit-block")?
            .parse::<usize>()
            .ok()?;
        let block = blocks.get(index)?;
        let offset = note_col_at_point(&block_element, client_x, client_y, &block.source);
        return Some(note_source_position(
            &block.source,
            block.start_line,
            offset,
        ));
    }
    let line_element = target
        .closest("[data-note-edit-line], [data-note-list-line]")
        .ok()
        .flatten()?;
    let line = line_element
        .get_attribute("data-note-edit-line")
        .or_else(|| line_element.get_attribute("data-note-list-line"))?
        .parse::<u32>()
        .ok()?;
    let block = blocks
        .iter()
        .find(|block| block.start_line <= line && line < block.end_line)?;
    let raw = block
        .source
        .lines()
        .nth(line.saturating_sub(block.start_line) as usize)
        .unwrap_or_default();
    let prefix = if matches!(block.block, MdBlock::List { .. }) {
        note_list_marker_prefix_len(raw).map_or(0, |(_, prefix)| prefix as u32)
    } else {
        0
    };
    let displayed = raw.chars().skip(prefix as usize).collect::<String>();
    let col = prefix + note_col_at_point(&line_element, client_x, client_y, &displayed);
    Some((line, col))
}

fn note_col_at_point(target: &web_sys::Element, client_x: f64, client_y: f64, text: &str) -> u32 {
    let text_target = target
        .query_selector("[data-note-line-text]")
        .ok()
        .flatten()
        .unwrap_or_else(|| target.clone());
    let char_count = text.chars().count() as u32;
    if let Some(document) = web_sys::window().and_then(|window| window.document())
        && let Some(caret) = document.caret_position_from_point(client_x as f32, client_y as f32)
        && let Some(offset_node) = caret.offset_node()
    {
        let text_node: &web_sys::Node = text_target.as_ref();
        if text_node.contains(Some(&offset_node))
            && let Ok(range) = document.create_range()
            && range.select_node_contents(text_node).is_ok()
            && range.set_end(&offset_node, caret.offset()).is_ok()
            && let Ok(fragment) = range.clone_contents()
        {
            return fragment
                .text_content()
                .unwrap_or_default()
                .chars()
                .count()
                .min(char_count as usize) as u32;
        }
    }
    let rect = text_target.get_bounding_client_rect();
    if rect.width() <= 0.0 {
        return 0;
    }
    let x = client_x - rect.left();
    if x <= 0.0 {
        return 0;
    }
    if x >= rect.width() {
        return char_count;
    }
    let ratio = x / rect.width();
    (ratio * char_count as f64).round() as u32
}

fn place_note_caret(line: u32, text: String, client_x: f64, prefix: u32) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        let Some(target) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&format!("note-line-{line}")))
        else {
            return;
        };
        let rect = target.get_bounding_client_rect();
        let col =
            prefix + note_col_at_point(&target, client_x, rect.top() + rect.height() / 2.0, &text);
        let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
            line,
            col,
            extend: false,
        });
        focus_file_input();
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn place_note_block_caret(
    index: usize,
    start_line: u32,
    source: String,
    client_x: f64,
    client_y: f64,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        let Some(target) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&format!("note-live-block-{index}")))
        else {
            return;
        };
        let offset = note_col_at_point(&target, client_x, client_y, &source);
        let (line, col) = note_source_position(&source, start_line, offset);
        let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
            line,
            col,
            extend: false,
        });
        focus_file_input();
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

#[derive(Clone, PartialEq)]
struct NoteLineChunk {
    text: String,
    selected: bool,
    caret_before: bool,
}

fn note_line_chunks(
    text: &str,
    caret: Option<u32>,
    selection: Option<vmux_core::editor::SelSpan>,
) -> Vec<NoteLineChunk> {
    let chars = text.chars().collect::<Vec<_>>();
    let len = chars.len() as u32;
    let caret = caret.map(|column| column.min(len));
    let selection = selection.map(|span| {
        let start = span.start.min(len);
        let end = if span.end == u32::MAX {
            len
        } else {
            span.end.min(len)
        };
        (start.min(end), start.max(end))
    });
    let mut boundaries = vec![0, len];
    if let Some(caret) = caret {
        boundaries.push(caret);
    }
    if let Some((start, end)) = selection {
        boundaries.push(start);
        boundaries.push(end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut chunks = boundaries
        .windows(2)
        .map(|range| {
            let start = range[0];
            let end = range[1];
            NoteLineChunk {
                text: chars[start as usize..end as usize].iter().collect(),
                selected: selection.is_some_and(|(selection_start, selection_end)| {
                    start < selection_end && end > selection_start
                }),
                caret_before: caret == Some(start),
            }
        })
        .collect::<Vec<_>>();
    if chunks.is_empty() || caret == Some(len) {
        chunks.push(NoteLineChunk {
            text: String::new(),
            selected: false,
            caret_before: caret == Some(len),
        });
    }
    chunks
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

/// The blinking caret overlaid on the rendered note.
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
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    mut resizing: Signal<bool>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    mode: Signal<Mode>,
) -> Element {
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
                handle_explorer_shortcut(
                    &event,
                    visible,
                    preferred_visible,
                    width,
                    client_id,
                    request_id,
                    mode,
                );
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
fn ExplorerToggleButton(
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    mode: Signal<Mode>,
) -> Element {
    rsx! {
        button {
            class: "shrink-0 cursor-default rounded p-0.5 text-foreground/60 hover:bg-foreground/[0.08] hover:text-foreground",
            title: translate("editor-toggle-explorer"),
            onclick: move |_| {
                toggle_explorer(
                    visible,
                    preferred_visible,
                    width,
                    client_id,
                    request_id,
                    mode,
                )
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

/// A span of raw note source, split around the caret and any selection.
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

/// Inline note nodes, recursing so a wrapped node keeps its own source range visible.
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
    let _ = try_cef_bin_emit_rkyv(&FilePropertyEdit {
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
    mut note_edit_rect: Signal<Option<NoteEditRect>>,
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
    let edit_rect = if editing {
        *note_edit_rect.read()
    } else {
        None
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
    let edit_overlay_class = if is_list {
        "visible absolute z-10 cursor-text overflow-auto"
    } else {
        note_edit_overlay_class()
    };
    let edit_overlay_style = if is_list {
        edit_rect.map_or_else(String::new, |rect| {
            format!(
                "top:{}px;left:{}px;width:{}px;height:{}px;",
                rect.top, rect.left, rect.width, rect.height,
            )
        })
    } else {
        String::new()
    };

    rsx! {
        div {
            id: "note-block-{index}",
            "data-note-block": "{index}",
            class: "relative flow-root w-full cursor-text",
            onclick: move |event| {
                if editing && !is_list {
                    return;
                }
                event.stop_propagation();
                if browser_has_text_selection() {
                    return;
                }
                let event_data = event.data();
                let raw = event_data.downcast::<web_sys::MouseEvent>();
                let client_x = raw.map_or(0.0, |raw| raw.client_x() as f64);
                let client_y = raw.map_or(0.0, |raw| raw.client_y() as f64);
                if is_live_inline {
                    note_active.set(Some(index as u32));
                    note_editing.set(true);
                    note_edit_line.set(None);
                    note_edit_rect.set(None);
                    place_note_block_caret(
                        index,
                        start,
                        live_pointer_source.clone(),
                        client_x,
                        client_y,
                    );
                    return;
                }
                let line = note_pointer_line(&event, start, end, &pointer_block);
                let text = pointer_source
                    .lines()
                    .nth(line.saturating_sub(start) as usize)
                    .unwrap_or_default()
                    .to_string();
                let prefix = if is_list {
                    note_list_marker_prefix_len(&text).map_or(0, |(_, prefix)| prefix as u32)
                } else {
                    0
                };
                note_active.set(Some(index as u32));
                note_editing.set(true);
                note_edit_line.set(Some(line));
                note_edit_rect.set(
                    is_list
                        .then(|| note_list_edit_rect(&event, index))
                        .flatten(),
                );
                let displayed = text.chars().skip(prefix as usize).collect();
                place_note_caret(line, displayed, client_x, prefix);
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
            }
            if editing {
                div {
                    class: edit_overlay_class,
                    style: edit_overlay_style,
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
                                let extend = event
                                    .data()
                                    .downcast::<web_sys::PointerEvent>()
                                    .is_some_and(|raw| raw.shift_key());
                                let offset = note_pointer_col_from_pointer(
                                    &event,
                                    &live_down_source,
                                );
                                let (line, col) = note_source_position(
                                    &live_down_source,
                                    start,
                                    offset,
                                );
                                note_dragging.set(true);
                                set_pointer_capture(&event, "file-scroll", true);
                                let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                    line,
                                    col,
                                    extend,
                                });
                                focus_file_input();
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
                                    let pointer_raw_down = raw.clone();
                                    let line_selection = selections
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
                                    let chunks = note_line_chunks(
                                        raw,
                                        (line == current.line)
                                            .then_some(current.col.saturating_sub(prefix)),
                                        line_selection,
                                    );
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
                                                let extend = event
                                                    .data()
                                                    .downcast::<web_sys::PointerEvent>()
                                                    .is_some_and(|raw| raw.shift_key());
                                                let col = prefix
                                                    + note_pointer_col_from_pointer(
                                                        &event,
                                                        &pointer_raw_down,
                                                    );
                                                note_dragging.set(true);
                                                set_pointer_capture(&event, "file-scroll", true);
                                                let _ = try_cef_bin_emit_rkyv(&FilePointerEvent {
                                                    line,
                                                    col,
                                                    extend,
                                                });
                                                focus_file_input();
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
enum Mode {
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

fn blob_url(bytes: &[u8]) -> Option<String> {
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

fn revoke(url: &str) {
    let _ = web_sys::Url::revoke_object_url(url);
}

fn clear_blob_state(mut preview: Signal<Preview>, mut thumbs: Signal<HashMap<String, String>>) {
    if let Preview::Image(old) = &*preview.read() {
        revoke(old);
    }
    preview.set(Preview::None);
    for url in thumbs.read().values() {
        revoke(url);
    }
    thumbs.set(HashMap::new());
}

fn request_preview(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FilePreviewRequest { path, thumb: false });
}

fn request_thumb(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FilePreviewRequest { path, thumb: true });
}

fn open_path(path: String) {
    let _ = try_cef_bin_emit_rkyv(&FileOpenEvent { path });
}

fn schedule_git_refresh(mut generation: Signal<u32>, mut nonce: Signal<u32>) {
    let next = generation().wrapping_add(1);
    generation.set(next);
    let Some(window) = web_sys::window() else {
        nonce.set(nonce().wrapping_add(1));
        return;
    };
    let closure = Closure::once(move || {
        if generation() == next {
            nonce.set(nonce().wrapping_add(1));
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        GIT_REFRESH_DEBOUNCE_MS,
    );
    closure.forget();
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
    for url in thumbs.read().values() {
        revoke(url);
    }
    thumbs.set(HashMap::new());
    if let Preview::Image(old) = &*preview.read() {
        revoke(old);
    }
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

/// A directory entry's thumbnail, or its type icon when there is none.
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

/// The right-hand preview pane for the selected entry.
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
                        onmounted: move |_| report_video_rect(path.clone()),
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
    ((js_sys::Date::now() as u64) << 12) ^ (js_sys::Math::random() * 4096.0) as u64
}

fn set_explorer_visible(
    next: bool,
    mut visible: Signal<bool>,
    mut preferred_visible: Signal<bool>,
    width: Signal<u32>,
    client_id: Signal<u64>,
    mut request_id: Signal<u64>,
    mode: Signal<Mode>,
) {
    let next_request_id = request_id().wrapping_add(1);
    request_id.set(next_request_id);
    preferred_visible.set(next);
    visible.set(next && explorer_has_room(width()));
    let _ = try_cef_bin_emit_rkyv(&ExplorerPanelSetVisible {
        visible: next,
        client_id: client_id(),
        request_id: next_request_id,
    });
    if !next {
        match mode() {
            Mode::Text => focus_file_input(),
            Mode::Dir | Mode::Media(_) => focus_container(),
        }
    }
}

fn explorer_page_width() -> Option<u32> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(PAGE_ID))
        .map(|element| element.client_width().max(0) as u32)
}

fn explorer_has_room(explorer_width: u32) -> bool {
    explorer_page_width().is_some_and(|page_width| {
        NOTE_MAX_CONTENT_WIDTH_PX.saturating_add(explorer_width) <= page_width
    })
}

fn sync_explorer_visibility(
    mut visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
) {
    let next = preferred_visible() && explorer_has_room(width());
    if visible() != next {
        visible.set(next);
    }
}

fn schedule_explorer_visibility_sync(
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        sync_explorer_visibility(visible, preferred_visible, width);
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn show_explorer_if_room(
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    mode: Signal<Mode>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        if visible() {
            return;
        }
        if explorer_has_room(width()) {
            set_explorer_visible(
                true,
                visible,
                preferred_visible,
                width,
                client_id,
                request_id,
                mode,
            );
        }
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn schedule_lsp_notice_clear(
    mut notice: Signal<Option<LspInstallProgress>>,
    mut request: Signal<Option<(String, String)>>,
    mut generation: Signal<u32>,
    delay: i32,
) {
    let id = generation().wrapping_add(1);
    generation.set(id);
    let Some(window) = web_sys::window() else {
        return;
    };
    let clear = Closure::once(move || {
        if generation() == id {
            notice.set(None);
            request.set(None);
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        clear.as_ref().unchecked_ref(),
        delay,
    );
    clear.forget();
}

fn toggle_explorer(
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    mode: Signal<Mode>,
) {
    set_explorer_visible(
        !preferred_visible(),
        visible,
        preferred_visible,
        width,
        client_id,
        request_id,
        mode,
    );
}

fn reveal_current_in_explorer(
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    mode: Signal<Mode>,
) {
    if visible() {
        let _ = try_cef_bin_emit_rkyv(&ExplorerRevealCurrent);
    } else {
        set_explorer_visible(
            true,
            visible,
            preferred_visible,
            width,
            client_id,
            request_id,
            mode,
        );
    }
}

fn handle_explorer_shortcut(
    event: &Event<KeyboardData>,
    visible: Signal<bool>,
    preferred_visible: Signal<bool>,
    width: Signal<u32>,
    client_id: Signal<u64>,
    request_id: Signal<u64>,
    mode: Signal<Mode>,
) -> bool {
    let data = event.data();
    let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
        return false;
    };
    let key = raw.key();
    if (raw.meta_key() || raw.ctrl_key()) && raw.shift_key() && key.eq_ignore_ascii_case("e") {
        event.prevent_default();
        reveal_current_in_explorer(
            visible,
            preferred_visible,
            width,
            client_id,
            request_id,
            mode,
        );
        return true;
    }
    if (raw.meta_key() || raw.ctrl_key()) && key.eq_ignore_ascii_case("b") {
        event.prevent_default();
        toggle_explorer(
            visible,
            preferred_visible,
            width,
            client_id,
            request_id,
            mode,
        );
        return true;
    }
    false
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
) -> Element {
    rsx! {
        div { class: if invisible { "invisible" } else { "" },
            if let Some(line) = hidden_list_line {
                MdBlockView { block: block.clone(), block_key: index, hidden_list_line: Some(line) }
            } else {
                MdBlockView { block: block.clone(), block_key: index }
            }
        }
    }
}

fn scroll_dir_row_into_view(idx: usize) {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&format!("dir-row-{idx}")))
    else {
        return;
    };
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

fn toggle_preview_video() {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("preview-video"))
    else {
        return;
    };
    let target: &JsValue = el.as_ref();
    let paused = js_sys::Reflect::get(target, &JsValue::from_str("paused"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let method = if paused { "play" } else { "pause" };
    if let Ok(f) = js_sys::Reflect::get(target, &JsValue::from_str(method))
        && let Ok(f) = f.dyn_into::<js_sys::Function>()
    {
        let _ = f.call0(target);
    }
}

fn focus_container() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CONTAINER_ID))
        && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html.focus();
    }
}

fn focus_file_input() {
    focus_by_id(INPUT_ID);
}

fn focus_by_id(id: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        let _ = html.focus_with_options(&options);
    }
}

fn scroll_el() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SCROLL_ID))
}

fn scroll_viewport_by(lines: i32, line_height: f64) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(scroll) = document.get_element_by_id(SCROLL_ID) else {
        return;
    };
    let restore_input_focus = document
        .active_element()
        .is_some_and(|element| element.id() == INPUT_ID);
    if restore_input_focus
        && let Some(input) = document
            .get_element_by_id(INPUT_ID)
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = input.blur();
    }
    let top = scroll.scroll_top() as f64 + lines as f64 * line_height;
    scroll.set_scroll_top(top.max(0.0).round() as i32);
    if restore_input_focus {
        focus_by_id(INPUT_ID);
    }
}

fn scroll_note_caret_into_view(block_index: usize, line: u32) -> bool {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return false;
    };
    let Some(scroll) = document.get_element_by_id(SCROLL_ID) else {
        return false;
    };
    let block_selector = format!("#note-block-{block_index}");
    let caret = document
        .get_element_by_id(NOTE_CARET_ID)
        .filter(|caret| caret.closest(&block_selector).ok().flatten().is_some());
    let exact = caret.is_some();
    let Some(target) = caret
        .or_else(|| document.get_element_by_id(&format!("note-line-{line}")))
        .or_else(|| document.get_element_by_id(&format!("note-live-block-{block_index}")))
        .or_else(|| document.get_element_by_id(&format!("note-block-{block_index}")))
    else {
        return false;
    };
    let viewport = scroll.get_bounding_client_rect();
    let target = target.get_bounding_client_rect();
    let delta = viewport_reveal_delta(
        target.top(),
        target.bottom(),
        viewport.top(),
        viewport.bottom(),
    );
    if delta.abs() >= 1.0 {
        scroll.set_scroll_top((scroll.scroll_top() as f64 + delta).round() as i32);
    }
    exact
}

fn center_note_caret(block_index: usize, line: u32) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(scroll) = document.get_element_by_id(SCROLL_ID) else {
        return;
    };
    let block_selector = format!("#note-block-{block_index}");
    let caret = document
        .get_element_by_id(NOTE_CARET_ID)
        .filter(|caret| caret.closest(&block_selector).ok().flatten().is_some());
    let Some(target) = caret
        .or_else(|| document.get_element_by_id(&format!("note-line-{line}")))
        .or_else(|| document.get_element_by_id(&format!("note-live-block-{block_index}")))
        .or_else(|| document.get_element_by_id(&format!("note-block-{block_index}")))
    else {
        return;
    };
    let viewport = scroll.get_bounding_client_rect();
    let target = target.get_bounding_client_rect();
    let target_center =
        scroll.scroll_top() as f64 + target.top() - viewport.top() + target.height() * 0.5;
    let top = centered_scroll_top(target_center, viewport.height());
    scroll.set_scroll_top(top.round() as i32);
}

fn schedule_note_caret_visibility() {
    let Some(window) = web_sys::window() else {
        NOTE_CARET_VISIBILITY_QUEUE.with(|queue| {
            queue.borrow_mut().take();
        });
        return;
    };
    let callback = Closure::once_into_js(move || {
        let pending = NOTE_CARET_VISIBILITY_QUEUE.with(|queue| queue.borrow_mut().take());
        if let Some(request) = pending
            && !scroll_note_caret_into_view(request.block_index, request.line)
            && request.retry
        {
            queue_note_caret_visibility(request.block_index, request.line, false);
        }
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn queue_note_caret_visibility(block_index: usize, line: u32, retry: bool) {
    let should_schedule = NOTE_CARET_VISIBILITY_QUEUE.with(|queue| {
        queue.borrow_mut().enqueue(NoteCaretVisibilityRequest {
            block_index,
            line,
            retry,
        })
    });
    if should_schedule {
        schedule_note_caret_visibility();
    }
}

fn ensure_note_caret_visible(block_index: usize, line: u32) {
    queue_note_caret_visibility(block_index, line, true);
}

fn ensure_line_visible(line: u32, ch: f64) {
    if ch <= 0.0 {
        return;
    }
    let Some(el) = scroll_el() else {
        return;
    };
    let view_h = el.client_height() as f64;
    if view_h <= 0.0 {
        return;
    }
    let top = line as f64 * ch;
    let view_top = el.scroll_top() as f64;
    if top < view_top {
        el.set_scroll_top(top as i32);
    } else if top + ch > view_top + view_h {
        el.set_scroll_top((top + ch - view_h) as i32);
    }
}

fn center_line(line: u32, ch: f64) {
    if ch <= 0.0 {
        return;
    }
    let Some(el) = scroll_el() else {
        return;
    };
    let view_h = el.client_height() as f64;
    if view_h <= 0.0 {
        return;
    }
    let target_center = line as f64 * ch + ch * 0.5;
    el.set_scroll_top(centered_scroll_top(target_center, view_h).round() as i32);
}

fn schedule_line_center(line: u32, ch: f64, retry: bool) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        center_line(line, ch);
        if retry {
            schedule_line_center(line, ch, false);
        }
    })
    .unchecked_into::<js_sys::Function>();
    if window.request_animation_frame(&callback).is_err() {
        let _ = callback.call0(&JsValue::NULL);
    }
}

fn reset_file_scroll() {
    if let Some(el) = scroll_el() {
        el.set_scroll_top(0);
    }
}

fn send_committed_text() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(INPUT_ID))
        .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    {
        let v = el.value();
        if !v.is_empty() {
            let _ = try_cef_bin_emit_rkyv(&FileTextInput { text: v });
            el.set_value("");
        }
    }
}

fn key_mods(raw: &web_sys::KeyboardEvent) -> KeyMods {
    KeyMods {
        ctrl: raw.ctrl_key(),
        alt: raw.alt_key(),
        shift: raw.shift_key(),
        meta: raw.meta_key(),
    }
}

fn forward_file_key(
    event: &Event<KeyboardData>,
    raw: &web_sys::KeyboardEvent,
    mode: vmux_core::editor::EditMode,
) -> bool {
    if raw.is_composing() {
        return false;
    }
    let key = raw.key();
    let mods = key_mods(raw);
    let chord = mods.ctrl || mods.alt || mods.meta;
    if mode.accepts_text() && !chord && is_text_key(&key) {
        return false;
    }
    event.prevent_default();
    let _ = try_cef_bin_emit_rkyv(&FileKeyEvent {
        key,
        code: raw.code(),
        mods,
        repeat: raw.repeat(),
    });
    true
}

fn is_text_key(key: &str) -> bool {
    key.chars().count() == 1
}

fn setup_measurement(
    cell_dims: Signal<(f64, f64)>,
    total_lines: Signal<u32>,
    last_resize: Signal<FileResizeEvent>,
    explorer_visible: Signal<bool>,
    explorer_preferred_visible: Signal<bool>,
    explorer_width: Signal<u32>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };

    sync_explorer_visibility(explorer_visible, explorer_preferred_visible, explorer_width);
    if document.get_element_by_id(MEASURE_ID).is_some() {
        do_measure(cell_dims, total_lines, last_resize);
        return;
    }

    let measure: web_sys::Element = document.create_element("span").unwrap();
    measure
        .set_attribute(
            "style",
            "position:absolute;visibility:hidden;white-space:pre;font:inherit",
        )
        .unwrap();
    measure.set_attribute("id", MEASURE_ID).unwrap();
    let measure_node: &web_sys::Node = measure.as_ref();
    measure_node.set_text_content(Some(&"X".repeat(80)));
    container.append_child(&measure).unwrap();

    do_measure(cell_dims, total_lines, last_resize);

    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        sync_explorer_visibility(explorer_visible, explorer_preferred_visible, explorer_width);
        do_measure(cell_dims, total_lines, last_resize);
    }) as Box<dyn FnMut(JsValue)>);

    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        observer.observe(&container);
        observer.observe(&measure);
        std::mem::forget(observer);
    }
    callback.forget();
}

/// Emit the current on-screen rect of the native video host element so the backend
/// can position the `AVPlayer` overlay over it.
fn emit_video_rect(path: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(VIDEO_HOST_ID) else {
        return;
    };
    let rect = el.get_bounding_client_rect();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let _ = try_cef_bin_emit_rkyv(&FileVideoRect {
        path: path.to_string(),
        x: rect.left() as f32,
        y: rect.top() as f32,
        w: rect.width() as f32,
        h: rect.height() as f32,
    });
}

/// Report the video host rect now and on every subsequent resize (window/layout),
/// keeping the native overlay aligned with the page element.
fn report_video_rect(path: String) {
    emit_video_rect(&path);
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(VIDEO_HOST_ID) else {
        return;
    };
    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        emit_video_rect(&path);
    }) as Box<dyn FnMut(JsValue)>);
    if let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
        observer.observe(&el);
        std::mem::forget(observer);
    }
    callback.forget();
}

fn do_measure(
    mut cell_dims: Signal<(f64, f64)>,
    total_lines: Signal<u32>,
    mut last_resize: Signal<FileResizeEvent>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
        return;
    };
    let Some(measure) = document.get_element_by_id(MEASURE_ID) else {
        return;
    };

    let rect = measure.get_bounding_client_rect();
    let cw = rect.width() / 80.0;

    let ch = window
        .get_computed_style(&container)
        .ok()
        .flatten()
        .and_then(|cs| {
            cs.get_property_value("line-height")
                .ok()
                .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        })
        .unwrap_or(rect.height());

    if cw <= 0.0 || ch <= 0.0 {
        return;
    }

    let previous_dims = cell_dims();
    if (previous_dims.0 - cw).abs() > 0.01 || (previous_dims.1 - ch).abs() > 0.01 {
        cell_dims.set((cw, ch));
    }

    let html: &web_sys::HtmlElement = container.unchecked_ref();
    let _ = html.style().set_property("--cw", &format!("{cw}px"));
    let _ = html.style().set_property("--ch", &format!("{ch}px"));

    let scroll = document.get_element_by_id(SCROLL_ID);
    let vh = scroll
        .as_ref()
        .map(|element| element.client_height() as f64)
        .filter(|h| *h > 0.0)
        .unwrap_or_else(|| container.client_height() as f64);
    let vw = scroll
        .as_ref()
        .map(|element| element.client_width() as f64)
        .filter(|width| *width > 0.0)
        .unwrap_or_else(|| container.client_width() as f64);
    let gutter = gutter_width(total_lines()) as f64 * cw + 48.0;
    let wrap_columns = ((vw - gutter - 32.0).max(cw) / cw)
        .floor()
        .min(u16::MAX as f64) as u16;

    let next = FileResizeEvent {
        char_height: ch as f32,
        viewport_height: vh as f32,
        wrap_columns,
    };
    let previous = last_resize();
    if (previous.char_height - next.char_height).abs() <= 0.01
        && (previous.viewport_height - next.viewport_height).abs() <= 0.01
        && previous.wrap_columns == next.wrap_columns
    {
        return;
    }
    last_resize.set(next.clone());
    let _ = try_cef_bin_emit_rkyv(&next);
}
