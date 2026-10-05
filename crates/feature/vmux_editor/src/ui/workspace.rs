#![allow(non_snake_case)]

use super::breadcrumb::EditorBreadcrumbs;
use super::diagnostic::DiagnosticPresentation;
use super::directory::{PreviewDom, PreviewPane};
use super::dom::{EditorDom, ScrolledLineHeight};
use super::editor::{EditorLines, StickyScope};
use super::explorer::SidebarView;
use super::input::{EditorFocus, EditorInput, PreeditField};
use super::key::use_file_keys;
use super::menu::{CodeActionMenu, EditorContextMenu, ReferencesPanel, RenameInput};
use super::note::{NoteBlankLine, NoteBlockView, NoteBlocks, NoteCursor, NoteProperties};
use super::sidebar::{ExplorerPane, ExplorerSidebar, ExplorerToggleButton, PaneWidth};
use super::state::FileUi;
use super::status::{EncodingRecovery, FileStatusInfo, FileStatusScope};
use super::text_geometry::{CellMetrics, ColumnRuler, GutterWidth, RowRuler};
use super::text_style::StyledSpanStyle;
use super::toolbar::{EditorTabStrip, FindBar, VimStatus};
use std::collections::HashMap;

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_api::editor::KeymapKind;
use vmux_api::editor::{CursorPos, EditMode};
use vmux_api::media::MediaKind;
use vmux_ecs::event::*;
use vmux_ecs::scroll::{EDGE_TRIGGER_K, ScrollWindow};
use vmux_git::ui::{DiffView, GitFooter};
use vmux_setting::EXPLORER_DEFAULT_WIDTH;
use vmux_ui::directory::DirectoryNavigator;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{PressedKey, send, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::ime::use_ime_guard;
use vmux_ui::platform::Platform;
use vmux_ui::scroll::ScrollIntoView;

#[component]
pub fn Page() -> Element {
    use_theme();
    let ui = FileUi::use_root();
    let mut handled_document_revision = use_signal(|| 0u64);
    let meta_state = ui.use_value(|state| state.document.meta.as_ref());
    let directory_state = ui.use_value(|state| state.document.directory.as_ref());
    let media_state = ui.use_value(|state| state.document.media.as_ref());
    let note_state = ui.use_value(|state| state.document.note.as_ref());
    let error_state = ui.use_value(|state| state.document.error.as_ref());
    let path = use_memo(move || {
        directory_state
            .read()
            .as_ref()
            .map(|directory| directory.path.clone())
            .or_else(|| meta_state.read().as_ref().map(|meta| meta.path.clone()))
            .unwrap_or_default()
    });
    let git_path = use_memo(move || {
        directory_state
            .read()
            .as_ref()
            .map(|directory| directory.abs_path.clone())
            .or_else(|| meta_state.read().as_ref().map(|meta| meta.abs_path.clone()))
            .unwrap_or_default()
    });
    let mode = use_memo(move || {
        if let Some(media) = media_state.read().as_ref() {
            Mode::Media(media.kind)
        } else if directory_state.read().is_some() {
            Mode::Dir
        } else {
            Mode::Text
        }
    });
    let media = use_memo(move || media_state.read().as_ref().cloned());
    let dir_entries = use_memo(move || {
        directory_state
            .read()
            .as_ref()
            .map(|directory| directory.entries.clone())
            .unwrap_or_default()
    });
    let parent_entries = use_memo(move || {
        directory_state
            .read()
            .as_ref()
            .map(|directory| directory.parent_entries.clone())
            .unwrap_or_default()
    });
    let selected = use_memo(move || {
        directory_state
            .read()
            .as_ref()
            .map(|directory| usize::try_from(directory.selected).unwrap_or_default())
            .unwrap_or_default()
    });
    let error = use_memo(move || {
        error_state
            .read()
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_default()
    });
    let error_undecodable = use_memo(move || {
        error_state
            .read()
            .as_ref()
            .is_some_and(|error| error.undecodable)
    });
    let doc_title = use_memo(move || {
        if let Some(directory) = directory_state.read().as_ref() {
            return directory
                .path
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or(&directory.path)
                .to_string();
        }
        if let Some(note) = note_state.read().as_ref()
            && !note.title.is_empty()
        {
            return note.title.clone();
        }
        let path = path();
        path.rsplit('/').next().unwrap_or(&path).to_string()
    });
    let shape_state = ui.use_value(|state| state.document.shape.as_ref());
    let encoding_state = ui.use_value(|state| state.document.encoding.as_ref());
    let document_kind = use_memo(move || {
        meta_state
            .read()
            .as_ref()
            .map(|meta| meta.kind)
            .unwrap_or_default()
    });
    let language = use_memo(move || {
        meta_state
            .read()
            .as_ref()
            .map(|meta| meta.language.clone())
            .unwrap_or_default()
    });
    let indent = use_memo(move || {
        shape_state
            .read()
            .as_ref()
            .map(|shape| shape.indent)
            .or_else(|| meta_state.read().as_ref().map(|meta| meta.indent))
            .unwrap_or_default()
    });
    let line_ending = use_memo(move || {
        shape_state
            .read()
            .as_ref()
            .map(|shape| shape.line_ending)
            .or_else(|| meta_state.read().as_ref().map(|meta| meta.line_ending))
            .unwrap_or_default()
    });
    let encoding = use_memo(move || {
        encoding_state
            .read()
            .as_ref()
            .map(|state| state.encoding)
            .or_else(|| meta_state.read().as_ref().map(|meta| meta.encoding))
            .unwrap_or_default()
    });
    let viewport_state = ui.use_value(|state| state.viewport.content.as_ref());
    let total_lines = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.total_lines)
            .or_else(|| meta_state.read().as_ref().map(|meta| meta.total_lines))
            .unwrap_or_default()
    });
    let total_rows = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.total_rows)
            .unwrap_or_default()
    });
    let first_row = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.first_row)
            .unwrap_or_default()
    });
    let mut gutter_hover = use_signal(|| false);
    let lines = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.lines.clone())
            .unwrap_or_default()
    });
    let sticky_lines = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.sticky.clone())
            .unwrap_or_default()
    });
    let outline = ui.use_value(|state| state.explorer.outline.as_ref());
    let line_layouts = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.layouts.clone())
            .unwrap_or_default()
    });
    let wrap_columns = use_memo(move || {
        viewport_state
            .read()
            .as_ref()
            .map(|viewport| viewport.wrap_columns)
            .unwrap_or_default()
    });
    let mut hover_diag = use_signal(|| Option::<FileDiagnostic>::None);
    let lsp_install_notice = ui.use_value(|state| Some(&state.language.lsp_install_notice));
    let code_actions = ui.use_value(|state| state.language.code_actions.as_ref());
    let rename = ui.use_value(|state| Some(&state.language.rename));
    let edit_notice = ui.use_value(|state| Some(&state.document.edit_notice));
    let preview_state = ui.use_value(|state| state.document.preview.as_ref());
    let preview = use_memo(move || {
        preview_state
            .read()
            .as_ref()
            .and_then(|state| state.selected.clone())
    });
    let thumbs = use_memo(move || {
        preview_state
            .read()
            .as_ref()
            .map(|state| {
                state
                    .thumbnails
                    .iter()
                    .map(|thumbnail| (thumbnail.path.clone(), thumbnail.url.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default()
    });
    let theme = ui.use_value(|state| state.document.theme.as_ref());
    let theme_style = use_memo(move || {
        let Some(theme) = theme.read().as_ref().cloned() else {
            return String::new();
        };
        let mut style = String::new();
        if !theme.font_family.is_empty() {
            style.push_str(&format!(
                "font-family:\"{}\",var(--font-mono);",
                theme.font_family
            ));
        }
        if theme.font_size > 0.0 {
            style.push_str(&format!("font-size:{}px;", theme.font_size));
        }
        if theme.line_height > 0.0 {
            style.push_str(&format!("line-height:{};", theme.line_height));
        }
        style
    });
    let mut cell_dims = use_signal(CellMetrics::default);
    let dom = EditorDom::new();
    let last_resize = use_signal(FileResizeEvent::default);
    let diagnostics_state = ui.use_value(|state| state.language.diagnostics.as_ref());
    let diagnostics = use_memo(move || {
        let Some(state) = diagnostics_state.read().as_ref().cloned() else {
            return Vec::new();
        };
        if state.path == git_path() {
            state.diagnostics
        } else {
            Vec::new()
        }
    });
    let lsp_status_state = ui.use_value(|state| state.language.lsp_status.as_ref());
    let lsp_status = use_memo(move || {
        let state = lsp_status_state.read().as_ref().cloned()?;
        (state.path == git_path()).then_some(state)
    });
    let lsp_capabilities = use_memo(move || {
        lsp_status()
            .map(|state| state.capabilities)
            .unwrap_or_default()
    });
    let git_state_event = ui.use_value(|state| state.git.git_state.as_ref());
    let git_state = use_memo(move || git_state_event.read().clone().unwrap_or_default());
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
    let view_mode = ui.use_value(|state| state.document.view_mode.as_ref());
    let file_view_mode = use_memo(move || {
        view_mode
            .read()
            .as_ref()
            .map(|event| event.mode)
            .unwrap_or(FileViewMode::Note)
    });
    let mut view_mode_revision = use_signal(|| 0u64);
    let note_blocks = use_memo(move || {
        note_state
            .read()
            .as_ref()
            .map(|note| note.blocks.clone())
            .unwrap_or_default()
    });
    let note_properties = use_memo(move || {
        note_state
            .read()
            .as_ref()
            .map(|note| note.properties.clone())
            .unwrap_or_default()
    });
    let note_properties_open = use_memo(move || {
        note_state
            .read()
            .as_ref()
            .is_some_and(|note| note.properties_open)
    });
    let note_property_drafts = use_memo(move || {
        note_state
            .read()
            .as_ref()
            .map(|note| note.property_drafts.clone())
            .unwrap_or_default()
    });
    let note_references = use_memo(move || {
        note_state
            .read()
            .as_ref()
            .map(|note| note.references.clone())
            .unwrap_or_default()
    });
    let note_active = use_memo(move || note_state.read().as_ref().and_then(|note| note.active));
    let note_editing =
        use_memo(move || note_state.read().as_ref().is_some_and(|note| note.editing));
    let note_edit_line =
        use_memo(move || note_state.read().as_ref().and_then(|note| note.edit_line));
    let note_cursor = NoteCursor::new(note_active, note_editing, note_edit_line);
    let mut note_dragging = use_signal(|| false);
    let mut editor_dragging = use_signal(|| false);
    let mut editor_drag_origin = use_signal(|| Option::<(i32, i32)>::None);
    let find_state = ui.use_value(|state| state.viewport.find.as_ref());
    let find_open = use_memo(move || find_state.read().as_ref().is_some_and(|event| event.open));
    let find_forward =
        use_memo(move || find_state.read().as_ref().is_none_or(|event| event.forward));
    let find_query = use_memo(move || {
        find_state
            .read()
            .as_ref()
            .map(|event| event.query.clone())
            .unwrap_or_default()
    });
    let find_regex = use_memo(move || find_state.read().as_ref().is_some_and(|event| event.regex));
    let mut find_revision = use_signal(|| 0u64);
    let explorer_panel = ui.use_value(|state| state.explorer.explorer_panel.as_ref());
    let explorer_panel_effect = ui.use_field(|state| state.explorer.explorer_panel.as_ref());
    let explorer_visible = use_memo(move || {
        explorer_panel
            .read()
            .as_ref()
            .is_some_and(|panel| panel.visible)
    });
    let explorer_width = use_memo(move || {
        explorer_panel
            .read()
            .as_ref()
            .map(|panel| panel.width)
            .unwrap_or(EXPLORER_DEFAULT_WIDTH)
    });
    let sidebar_view = use_memo(move || {
        if explorer_panel
            .read()
            .as_ref()
            .is_some_and(|panel| panel.search)
        {
            SidebarView::Search
        } else {
            SidebarView::Explorer
        }
    });
    let mut explorer_search_focus_revision = use_signal(|| 0u64);
    let keymap_state = ui.use_value(|state| state.viewport.keymap.as_ref());
    let keymap = use_memo(move || {
        keymap_state
            .read()
            .as_ref()
            .map(|event| event.keymap)
            .unwrap_or_default()
    });
    let cursor_state = ui.use_value(|state| state.viewport.cursor.as_ref());
    let cursor = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.primary)
            .unwrap_or_default()
    });
    let carets = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.carets.clone())
            .unwrap_or_default()
    });
    let sel = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.selections.clone())
            .unwrap_or_default()
    });
    let source_cursor = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.source_primary)
            .unwrap_or_default()
    });
    let source_sel = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.source_selections.clone())
            .unwrap_or_default()
    });
    let ed_mode = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.mode)
            .unwrap_or(EditMode::Insert)
    });
    let ed_label = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.mode_label.clone())
            .unwrap_or_default()
    });
    let search_spans = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.search.clone())
            .unwrap_or_default()
    });
    let word_spans = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.word_highlights.clone())
            .unwrap_or_default()
    });
    let find_total = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.search_total)
            .unwrap_or_default()
    });
    let find_index = use_memo(move || {
        cursor_state
            .read()
            .as_ref()
            .map(|state| state.search_index)
            .unwrap_or_default()
    });
    let mut effect_cursor = use_signal(CursorPos::default);
    let open_editors = ui.use_value(|state| state.explorer.open_editors.as_ref());
    let ime = use_ime_guard();
    let typed = use_signal(String::new);
    let lsp_hover = ui.use_value(|state| Some(&state.language.hover));
    let mut hover_pos = use_signal(|| Option::<(u32, u32)>::None);
    let ctx_menu = use_signal(|| Option::<(f64, f64, u32, u32)>::None);
    let panel_state = ui.use_value(|state| state.panel.panel.as_ref());
    let panel = use_memo(move || panel_state.read().as_ref().cloned().unwrap_or_default());
    let mut panel_focus_revision = use_signal(|| 0u64);
    let mut last_scroll_req = use_signal(|| 0u32);
    let explorer = ExplorerPane::new(explorer_visible, explorer_width);
    let tidy = ui.use_value(|state| Some(&state.explorer.tidy));
    let is_markdown = use_memo(move || document_kind() == FileDocumentKind::Markdown);

    let keys = use_file_keys(panel.into());
    use_context_provider(|| keys);

    use_effect(move || {
        explorer_panel_effect.for_each(|event| {
            if event.search && event.search_focus_revision > explorer_search_focus_revision() {
                explorer_search_focus_revision.set(event.search_focus_revision);
                spawn(async move {
                    Platform::sleep(0).await;
                    FocusClaim::new(super::explorer::SEARCH_INPUT_ID).request();
                });
            }
        })
    });

    let find_event = FileUi::current().use_field(|state| state.viewport.find.as_ref());
    use_effect(move || {
        find_event.for_each(|event| {
            if event.revision <= find_revision() {
                return;
            }
            find_revision.set(event.revision);
            if event.open {
                spawn(async move {
                    Platform::sleep(0).await;
                    EditorFocus::find();
                });
            }
        })
    });

    let file_meta = FileUi::current().use_field(|state| state.document.meta.as_ref());
    use_effect(move || {
        file_meta.for_each(|m| {
            let reset_view = *handled_document_revision.peek() != m.revision;
            handled_document_revision.set(m.revision);
            if !reset_view {
                return;
            }
            dom.reset();
            last_scroll_req.set(0);
            let _ = send(&FileScrollEvent {
                top_row: 0,
                needs_rows: true,
            });
            hover_diag.set(None);
            note_cursor.reset();
            note_dragging.set(false);
            editor_dragging.set(false);
            editor_drag_origin.set(None);
        })
    });

    let file_viewport = FileUi::current().use_field(|state| state.viewport.content.as_ref());
    use_effect(move || {
        file_viewport.for_each(|_| {
            let _ = send(&FileHoverDismissRequest);
        })
    });

    let file_cursor = FileUi::current().use_field(|state| state.viewport.cursor.as_ref());
    use_effect(move || {
        file_cursor.for_each(|c| {
            let moved = effect_cursor().ne(&c.primary);
            if moved {
                effect_cursor.set(c.primary);
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
                if moved && let Some(index) = active {
                    note_cursor.reveal(index, c.source_primary.line);
                }
            }
            if moved && !note_mode {
                dom.reveal_caret();
            }
        })
    });

    let scroll_by = FileUi::current().use_field(|state| state.viewport.scroll_by.as_ref());
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

    let view_mode_event = FileUi::current().use_field(|state| state.document.view_mode.as_ref());
    use_effect(move || {
        view_mode_event.for_each(|event| {
            if event.revision <= *view_mode_revision.peek() {
                return;
            }
            view_mode_revision.set(event.revision);
            if event.mode != FileViewMode::Note {
                note_cursor.set_editing(false);
            }
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

    let keymap_event = FileUi::current().use_field(|state| state.viewport.keymap.as_ref());
    use_effect(move || {
        keymap_event.for_each(|event| {
            if event.keymap == KeymapKind::Vim
                && file_view_mode() == FileViewMode::Note
                && is_markdown()
            {
                let line = source_cursor().line;
                if let Some(index) = note_blocks.read().as_slice().block_index_for_line(line) {
                    note_cursor.activate_centered(index, line);
                }
            }
            if file_view_mode() == FileViewMode::Note && is_markdown() && !note_cursor.editing() {
                EditorFocus::container();
            } else {
                EditorFocus::file();
            }
        })
    });

    let note_event = FileUi::current().use_field(|state| state.document.note.as_ref());
    use_effect(move || {
        note_event.for_each(|event| {
            let FileNoteEvent {
                title: _,
                properties: _,
                properties_open: _,
                property_drafts: _,
                blocks,
                active: _,
                editing: _,
                edit_line: _,
                references: _,
                reveal_line,
            } = event;
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
            if let Some((activation, index, line)) = activation {
                match activation {
                    NoteCursorActivation::Center(_) => note_cursor.activate_centered(index, line),
                    NoteCursorActivation::PreserveViewport(_) => note_cursor.activate(index, line),
                }
            }
        })
    });

    let panel_event = FileUi::current().use_field(|state| state.panel.panel.as_ref());
    use_effect(move || {
        panel_event.for_each(|state| {
            let focus = state.focus;
            if focus.revision <= panel_focus_revision() {
                return;
            }
            panel_focus_revision.set(focus.revision);
            spawn(async move {
                Platform::sleep(0).await;
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

    let directory_event = FileUi::current().use_field(|state| state.document.directory.as_ref());
    use_effect(move || {
        directory_event.for_each(|d| {
            hover_diag.set(None);
            let index = usize::try_from(d.selected).unwrap_or_default();
            ScrollIntoView::nearest(&format!("dir-row-{index}"));
        })
    });

    let media_event = FileUi::current().use_field(|state| state.document.media.as_ref());
    use_effect(move || {
        media_event.for_each(|_| {
            hover_diag.set(None);
        })
    });

    use_effect(move || {
        dom.announce(cell_dims(), total_lines(), last_resize);
    });

    let gw = GutterWidth::for_lines(total_lines());
    let editor_tabs = open_editors
        .read()
        .as_ref()
        .map(|event| event.items.clone())
        .unwrap_or_default();
    let breadcrumb_path = match error().is_empty() {
        true => git_path(),
        false => String::new(),
    };
    let breadcrumb_outline = match mode() {
        Mode::Text => outline
            .read()
            .as_ref()
            .map(|event| event.items.clone())
            .unwrap_or_default(),
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

            PaneWidth {}

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
                            PreviewDom::toggle_video();
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
                        query: find_query(),
                        forward: find_forward(),
                        regex: find_regex(),
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
                                        let _ = send(&FileViewModeSet { mode: FileViewMode::Note });
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
                                    let _ = send(&FileViewModeSet { mode: FileViewMode::Editor });
                                },
                                {translate("editor-editor")}
                            }
                            if git_has_diff() {
                                button {
                                    class: file_mode_class(file_view_mode() == FileViewMode::Diff),
                                    title: translate("editor-git-diff"),
                                    onclick: move |_| {
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
                                let _ = send(&FileKeymapSet { keymap: next });
                            },
                            {translate("editor-keymap-standard")}
                        }
                        button {
                            class: file_mode_class(keymap() == KeymapKind::Vim),
                            onclick: move |_| {
                                let next = KeymapKind::Vim;
                                let _ = send(&FileKeymapSet { keymap: next });
                            },
                            {translate("editor-keymap-vim")}
                        }
                    }
                }
                {
                    tidy().and_then(|state| state.count).map(|count| {
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
                                    },
                                    {translate("editor-tidy")}
                                }
                                button {
                                    class: "rounded-full px-2 py-0.5 text-foreground/60 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyRequest { choice: TidyChoice::Always });
                                    },
                                    {translate("editor-always")}
                                }
                                button {
                                    class: "rounded-full px-1.5 py-0.5 text-foreground/40 hover:bg-foreground/10",
                                    onclick: move |_| {
                                        let _ = send(&FileTidyRequest { choice: TidyChoice::Dismiss });
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
                        children: match preview().map(|preview| preview.kind) {
                            Some(PreviewKind::Dir(entries)) => Some(entries),
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
                                        NoteProperties {
                                            properties: note_properties(),
                                            open: note_properties_open(),
                                            drafts: note_property_drafts(),
                                        }
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
                                        let _ = send(&FileHoverDismissRequest);
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
                                            lsp_hover().and_then(|state| state.value).map(|h| {
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
                                            titles: code_actions()
                                                .map(|actions| actions.titles)
                                                .unwrap_or_default(),
                                            selected: code_actions()
                                                .map(|actions| actions.selected)
                                                .unwrap_or_default(),
                                            top: cursor().row as f64 * ch + ch,
                                            left: gutter + ruler.x_of(cursor().row, cursor().col),
                                        }

                                        if let Some(rename) = rename().filter(|rename| rename.open) {
                                            RenameInput {
                                                top: rename.line as f64 * ch + ch,
                                                left: gutter + ruler.x_of_char(rename.line, rename.col),
                                                rename,
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
                lsp_install_notice()
                    .and_then(|notice| notice.progress.map(|progress| (progress, notice.installed)))
                    .map(|(progress, installed)| {
                    let (icon_class, icon, spinning) = match progress.phase {
                        InstallPhase::Done => ("text-ansi-2", "✓", false),
                        InstallPhase::Failed => ("text-ansi-1", "×", false),
                        _ => ("text-primary", "", true),
                    };
                    let message = if installed {
                        translate("lsp-status-installed")
                    } else {
                        progress.message.clone()
                    };
                    let detail = progress
                        .pct
                        .map_or_else(|| message.clone(), |percent| format!("{message} {percent}%"));
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
                edit_notice()
                    .and_then(|notice| notice.reason)
                    .map(|reason| {
                    rsx! {
                        div {
                            class: "pointer-events-none fixed right-4 bottom-14 z-[60] flex min-w-64 max-w-sm items-center gap-3 rounded-xl bg-background/95 px-3 py-2.5 text-xs text-foreground shadow-[0_12px_40px_rgba(0,0,0,0.28)] ring-1 ring-inset ring-foreground/10 backdrop-blur-xl",
                            span { class: "grid h-4 w-4 shrink-0 place-items-center text-base font-semibold text-ansi-1", "×" }
                            div { class: "min-w-0",
                                div { class: "truncate font-medium", {translate("editor-edit-failed")} }
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
pub(crate) const HOVER_DELAY_MS: u32 = 300;
pub(crate) const SCROLL_ID: &str = "file-scroll";

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
