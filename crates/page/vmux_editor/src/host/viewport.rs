use bevy::prelude::*;
use bevy_cef::prelude::*;
use vmux_core::event::{
    FileCursorEvent, FileFoldToggle, FileResizeEvent, FileScrollByEvent, FileScrollEvent,
    FileViewportPatch,
};
use vmux_core::scroll::{clamp_top_line, rows_from_viewport, window_range};

use crate::host::edit::Selection;
use crate::host::editor::{EditState, FileView};
use crate::host::file_lifecycle::{EditorFileLoadedSet, canon};
use crate::host::keymap::{EditorKeymap, Keymap};

const STICKY_SCROLL_DEPTH: usize = 5;

pub(crate) struct EditorViewportPlugin;

impl Plugin for EditorViewportPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(FileResizeEvent, FileScrollEvent)>::default())
            .add_systems(
                Update,
                (
                    sync_editor_wrap_settings.after(EditorFileLoadedSet),
                    rehighlight_on_color_scheme,
                    apply_lsp_folds,
                    persist_folds,
                ),
            )
            .add_observer(on_file_resize)
            .add_observer(on_file_scroll)
            .add_observer(on_file_fold_toggle);
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct FileViewport {
    pub(crate) top_row: u32,
    pub(crate) rows: u16,
    pub(crate) wrap_columns: u16,
    pub(crate) word_wrap: vmux_core::editor::WordWrap,
    pub(crate) word_wrap_column: u16,
}

impl FileViewport {
    pub(crate) fn scroll_to(
        &mut self,
        top: u32,
        entity: Entity,
        browsers: &Browsers,
        commands: &mut Commands,
    ) {
        let previous = self.top_row;
        if top == previous {
            return;
        }
        self.top_row = top;
        if !browsers.can_emit_to(&entity) {
            return;
        }
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &FileScrollByEvent {
                lines: top as i32 - previous as i32,
            },
        ));
    }

    pub(crate) fn visible_rows(&self, edit: &mut EditState) -> u32 {
        edit.wrapped_view(self).total_rows()
    }

    pub(crate) fn autoscroll(&self, edit: &mut EditState) -> Option<u32> {
        if self.rows == 0 {
            return None;
        }
        let cursor = edit.core.cursor_pos();
        let row = edit.wrapped_view(self).position(cursor.line, cursor.col).0;
        if row < self.top_row {
            return Some(row);
        }
        if row >= self.top_row + self.rows as u32 {
            return Some(row + 1 - self.rows as u32);
        }
        None
    }

    pub(crate) fn follow_scrolled_cursor(&self, edit: &mut EditState) -> bool {
        ScrolledCursor::follow(edit, self)
    }

    pub(crate) fn left_render_band(&self, previous_top: u32) -> bool {
        DriftedWindow::between(previous_top, self).left_the_band()
    }
}

#[derive(Component)]
pub(crate) struct FoldsDirty;

pub(crate) struct EditorWindow;

impl EditorWindow {
    pub(crate) fn emit(
        entity: Entity,
        edit: &mut EditState,
        viewport: &FileViewport,
        browsers: &Browsers,
        commands: &mut Commands,
    ) {
        if !browsers.can_emit_to(&entity) {
            return;
        }
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &Self::render(edit, viewport),
        ));
    }

    fn render(edit: &mut EditState, viewport: &FileViewport) -> FileViewportPatch {
        let total = edit.core.buffer.len_lines() as u32;
        let wrap = edit.wrapped_view(viewport);
        let (visible, wrap_columns) = (wrap.total_rows(), wrap.columns());
        let (visible_first, visible_end) = window_range(visible, viewport.top_row, viewport.rows);
        let overscan = vmux_core::scroll::overscan_for(
            viewport.rows,
            vmux_core::scroll::EDITOR_OVERSCAN_K,
            vmux_core::scroll::OVERSCAN_FLOOR,
            vmux_core::scroll::OVERSCAN_CAP,
        );
        let first_row = visible_first.saturating_sub(overscan);
        let end_row = (visible_end + overscan).min(visible);
        let visible_top = wrap.line_at(visible_first);
        let layouts = wrap.window(first_row, end_row);
        let first_row = layouts.first().map_or(first_row, |line| line.row);
        let mut lines = Vec::with_capacity(layouts.len());
        if let (Some(first_line), Some(last_line)) = (
            layouts.first().map(|layout| layout.line_no),
            layouts.last().map(|layout| layout.line_no),
        ) {
            let mut window = edit.hl.line_window(
                &edit.core.buffer.rope,
                first_line as usize,
                last_line as usize + 1,
            );
            let guides =
                crate::host::fold::IndentGuides::new(&edit.core.buffer.rope, edit.indent_width());
            for layout in &layouts {
                let index = (layout.line_no - first_line) as usize;
                let Some(line) = window.get_mut(index) else {
                    continue;
                };
                let mut line = std::mem::take(line);
                line.fold = edit.folds.gutter(layout.line_no);
                line.indent_levels = guides.levels(layout.line_no as usize);
                lines.push(line);
            }
        }
        let mut sticky = Vec::new();
        if let Some(top) = visible_top {
            let guides =
                crate::host::fold::IndentGuides::new(&edit.core.buffer.rope, edit.indent_width());
            for header in edit.folds.sticky(top, STICKY_SCROLL_DEPTH) {
                let at = header as usize;
                let mut window = edit.hl.line_window(&edit.core.buffer.rope, at, at + 1);
                if window.is_empty() {
                    continue;
                }
                let mut line = window.remove(0);
                line.fold = edit.folds.gutter(header);
                line.indent_levels = guides.levels(at);
                sticky.push(line);
            }
        }
        FileViewportPatch {
            first_row,
            total_rows: visible,
            total_lines: total,
            wrap_columns,
            layouts,
            lines,
            sticky,
        }
    }
}

pub(crate) struct EditorCursor;

impl EditorCursor {
    pub(crate) fn emit(
        entity: Entity,
        edit: &mut EditState,
        keymap: &dyn Keymap,
        viewport: &FileViewport,
        browsers: &Browsers,
        commands: &mut Commands,
    ) {
        if !browsers.can_emit_to(&entity) {
            return;
        }
        let total = edit.core.buffer.len_lines() as u32;
        let view = edit.folds.view(total);
        let source_primary = edit.core.cursor_pos();
        let mut primary = source_primary;
        let (span_first, span_rows) = HighlightedLines::window(edit, viewport);
        let raw_selections = edit
            .core
            .sel_spans(span_first, span_rows)
            .into_iter()
            .filter(|selection| !view.is_hidden(selection.line))
            .collect::<Vec<_>>();
        let raw_word_highlights = edit
            .core
            .word_highlight_spans(span_first, span_rows)
            .into_iter()
            .filter(|span| !view.is_hidden(span.line))
            .collect::<Vec<_>>();
        edit.core.refresh_search_matches();
        let matches = edit.core.cached_search_matches();
        let raw_search = edit
            .core
            .search_spans(matches, span_first, span_rows)
            .into_iter()
            .filter(|span| !view.is_hidden(span.line))
            .collect::<Vec<_>>();
        let caret = edit.core.primary().head;
        let search_index = matches
            .iter()
            .position(|found| found.contains(&caret) || found.start == caret)
            .map(|at| at as u32 + 1)
            .unwrap_or_default();
        let search_total = matches.len() as u32;
        let raw_carets: Vec<_> = edit
            .core
            .cursor_positions()
            .into_iter()
            .filter(|caret| !view.is_hidden(caret.line))
            .collect();
        let wrap = edit.wrapped_view(viewport);
        (primary.row, primary.col) = wrap.position(primary.line, primary.col);
        let mut carets = raw_carets;
        for caret in &mut carets {
            (caret.row, caret.col) = wrap.position(caret.line, caret.col);
        }
        let selections = wrap.selections(raw_selections.iter().copied());
        let search = wrap.selections(raw_search.iter().copied());
        let word_highlights = wrap.selections(raw_word_highlights.iter().copied());
        commands.trigger(BinHostEmitEvent::from_event(
            entity,
            &FileCursorEvent {
                search_total,
                search_index,
                mode: keymap.mode(),
                mode_label: keymap.mode_label(),
                primary,
                carets,
                selections,
                source_primary,
                source_selections: raw_selections,
                search,
                word_highlights,
            },
        ));
    }
}

struct HighlightedLines;

impl HighlightedLines {
    fn window(edit: &mut EditState, viewport: &FileViewport) -> (u32, u16) {
        let wrap = edit.wrapped_view(viewport);
        let visible = wrap.total_rows();
        let (first_row, end_row) = window_range(visible, viewport.top_row, viewport.rows);
        let overscan = vmux_core::scroll::overscan_for(
            viewport.rows,
            vmux_core::scroll::EDITOR_OVERSCAN_K,
            vmux_core::scroll::OVERSCAN_FLOOR,
            vmux_core::scroll::OVERSCAN_CAP,
        );
        let from = first_row.saturating_sub(overscan);
        let to = end_row.saturating_add(overscan).min(visible);
        let first_line = wrap.line_at(from).unwrap_or(0);
        let last_line = wrap.line_at(to.saturating_sub(1)).unwrap_or(first_line);
        let rows = last_line.saturating_sub(first_line).saturating_add(1);
        (first_line, rows.min(u16::MAX as u32) as u16)
    }
}

struct DriftedWindow {
    rows: u32,
    overscan: u32,
}

impl DriftedWindow {
    fn between(previous_top: u32, viewport: &FileViewport) -> Self {
        Self {
            rows: viewport.top_row.abs_diff(previous_top),
            overscan: vmux_core::scroll::overscan_for(
                viewport.rows,
                vmux_core::scroll::EDITOR_OVERSCAN_K,
                vmux_core::scroll::OVERSCAN_FLOOR,
                vmux_core::scroll::OVERSCAN_CAP,
            ),
        }
    }

    fn left_the_band(&self) -> bool {
        self.rows > self.overscan / 2
    }
}

struct ScrolledCursor;

impl ScrolledCursor {
    fn follow(edit: &mut EditState, viewport: &FileViewport) -> bool {
        if viewport.rows == 0 {
            return false;
        }
        let cursor = edit.core.cursor_pos();
        let top = viewport.top_row;
        let bottom = top + viewport.rows as u32 - 1;
        let row = edit
            .wrapped_view(viewport)
            .position(cursor.line, cursor.col)
            .0;
        let wanted = if row < top {
            top
        } else if row > bottom {
            bottom
        } else {
            return false;
        };
        let Some(line) = edit.wrapped_view(viewport).line_at(wanted) else {
            return false;
        };
        if line == cursor.line {
            return false;
        }
        let col = edit.core.char_at_cell(line as usize, cursor.col);
        let at = edit.core.buffer.coords_to_char(line as usize, col);
        if edit.core.mode.is_visual() {
            let anchor = edit.core.primary().anchor;
            edit.core.selections = vec![Selection { anchor, head: at }];
            return true;
        }
        edit.core.collapse_carets();
        edit.core.set_caret(at);
        true
    }
}

fn rehighlight_on_color_scheme(
    mut changes: MessageReader<vmux_setting::ColorSchemeChanged>,
    mut views: Query<(Entity, &mut EditState, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let Some(change) = changes.read().last().copied() else {
        return;
    };
    crate::host::highlight::set_dark_theme(matches!(change.0, vmux_setting::ResolvedScheme::Dark));
    for (entity, mut edit, viewport) in &mut views {
        EditorWindow::emit(entity, &mut edit, viewport, &browsers, &mut commands);
    }
}

fn sync_editor_wrap_settings(
    settings: Res<vmux_setting::AppSettings>,
    mut views: Query<(
        Entity,
        &mut FileViewport,
        Option<&mut EditState>,
        Option<&EditorKeymap>,
    )>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (entity, mut viewport, edit, keymap) in &mut views {
        let wanted_column = settings.editor.word_wrap_column.max(1);
        if viewport.word_wrap == settings.editor.word_wrap
            && viewport.word_wrap_column == wanted_column
        {
            continue;
        }
        let showing = viewport.top_row;
        viewport.word_wrap = settings.editor.word_wrap;
        viewport.word_wrap_column = wanted_column;
        viewport.top_row = 0;
        if let Some(mut edit) = edit {
            let wanted = viewport.autoscroll(&mut edit);
            viewport.top_row = showing;
            if viewport.rows > 0 {
                viewport.scroll_to(wanted.unwrap_or(0), entity, &browsers, &mut commands);
                edit.core.top_row = viewport.top_row;
            }
            EditorWindow::emit(entity, &mut edit, &viewport, &browsers, &mut commands);
            if let Some(keymap) = keymap {
                EditorCursor::emit(
                    entity,
                    &mut edit,
                    keymap.0.as_ref(),
                    &viewport,
                    &browsers,
                    &mut commands,
                );
            }
        }
    }
}

fn on_file_resize(
    trigger: On<BinReceive<FileResizeEvent>>,
    mut views: Query<(
        &mut FileViewport,
        Option<&mut EditState>,
        Option<&EditorKeymap>,
    )>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let event = &trigger.event().payload;
    let Ok((mut viewport, edit, keymap)) = views.get_mut(entity) else {
        return;
    };
    let rows = rows_from_viewport(event.char_height, event.viewport_height);
    if viewport.rows == rows && viewport.wrap_columns == event.wrap_columns {
        return;
    }
    viewport.rows = rows;
    viewport.wrap_columns = event.wrap_columns;
    if let Some(mut edit) = edit {
        edit.core.rows = viewport.rows;
        edit.core.top_row = viewport.top_row;
        EditorWindow::emit(entity, &mut edit, &viewport, &browsers, &mut commands);
        if let Some(keymap) = keymap {
            EditorCursor::emit(
                entity,
                &mut edit,
                keymap.0.as_ref(),
                &viewport,
                &browsers,
                &mut commands,
            );
        }
    }
}

fn on_file_scroll(
    trigger: On<BinReceive<FileScrollEvent>>,
    mut views: Query<(&mut EditState, &mut FileViewport, &EditorKeymap)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let event = &trigger.event().payload;
    let Ok((mut edit, mut viewport, keymap)) = views.get_mut(entity) else {
        return;
    };
    let visible = viewport.visible_rows(&mut edit);
    viewport.top_row = clamp_top_line(event.top_row, visible, viewport.rows);
    edit.core.top_row = viewport.top_row;
    if !event.needs_rows {
        return;
    }
    EditorWindow::emit(entity, &mut edit, &viewport, &browsers, &mut commands);
    EditorCursor::emit(
        entity,
        &mut edit,
        keymap.0.as_ref(),
        &viewport,
        &browsers,
        &mut commands,
    );
}

fn on_file_fold_toggle(
    trigger: On<BinReceive<FileFoldToggle>>,
    mut views: Query<(&mut EditState, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let line = trigger.event().payload.line;
    let Ok((mut edit, keymap, viewport)) = views.get_mut(entity) else {
        return;
    };
    edit.folds.toggle_header(line);
    edit.sync_fold_view();
    EditorWindow::emit(entity, &mut edit, viewport, &browsers, &mut commands);
    EditorCursor::emit(
        entity,
        &mut edit,
        keymap.0.as_ref(),
        viewport,
        &browsers,
        &mut commands,
    );
    commands.entity(entity).insert(FoldsDirty);
}

fn persist_folds(
    views: Query<(Entity, &FileView, &EditState), With<FoldsDirty>>,
    mut store: NonSendMut<crate::host::fold_store::FoldStore>,
    mut commands: Commands,
) {
    let mut changed = false;
    for (entity, file, edit) in &views {
        let mut collapsed: Vec<u32> = edit.folds.collapsed.iter().copied().collect();
        collapsed.sort_unstable();
        store.set(&file.path, &collapsed);
        commands.entity(entity).remove::<FoldsDirty>();
        changed = true;
    }
    if changed {
        store.save();
    }
}

fn apply_lsp_folds(
    mut folds: MessageReader<crate::host::lsp::manager::LspFolds>,
    mut views: Query<(&mut EditState, &FileView, &EditorKeymap, &FileViewport)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for fold in folds.read() {
        let Ok((mut edit, file, keymap, viewport)) = views.get_mut(fold.entity) else {
            continue;
        };
        if canon(&file.path) != canon(&fold.path) {
            continue;
        }
        let regions = if fold.regions.is_empty() {
            crate::host::fold::indent_regions(&edit.core.buffer.rope)
        } else {
            fold.regions.clone()
        };
        edit.folds.set_regions(regions);
        edit.sync_fold_view();
        EditorWindow::emit(fold.entity, &mut edit, viewport, &browsers, &mut commands);
        EditorCursor::emit(
            fold.entity,
            &mut edit,
            keymap.0.as_ref(),
            viewport,
            &browsers,
            &mut commands,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use ropey::Rope;

    use super::*;
    use crate::host::edit::highlight_cache::HighlightCache;
    use crate::host::edit::{EditCommand, EditCore};
    use crate::host::keymap::KeymapKindExt;

    impl ScrolledCursor {
        fn at(cursor_line: usize, top_row: u32, rows: u16) -> (EditState, FileViewport) {
            let text = (0..40).map(|i| format!("line {i}\n")).collect::<String>();
            let mut core = EditCore::new(
                PathBuf::from("/tmp/scroll.rs"),
                "Rust".into(),
                &text,
                crate::host::edit::EditMode::Normal,
            );
            core.set_caret(core.buffer.coords_to_char(cursor_line, 3));
            let edit = EditState::new(
                core,
                HighlightCache::new(Path::new("/tmp/scroll.rs")),
                crate::host::fold::FoldState::default(),
            );
            let viewport = FileViewport {
                top_row,
                rows,
                wrap_columns: 0,
                word_wrap: vmux_core::editor::WordWrap::Off,
                word_wrap_column: 80,
            };
            (edit, viewport)
        }

        fn cursor_line(edit: &EditState) -> u32 {
            edit.core.cursor_pos().line
        }
    }

    impl FileViewport {
        fn scrolling(top_row: u32, rows: u16) -> (App, Entity) {
            let text = (0..400).map(|i| format!("line {i}\n")).collect::<String>();
            let path = PathBuf::from("/tmp/scroll-report.rs");
            let core = EditCore::new(
                path.clone(),
                "Rust".into(),
                &text,
                crate::host::edit::EditMode::Normal,
            );
            let edit = EditState::new(
                core,
                HighlightCache::new(&path),
                crate::host::fold::FoldState::default(),
            );
            let mut app = App::new();
            app.add_plugins(MinimalPlugins).add_observer(on_file_scroll);
            app.world_mut().insert_non_send(Browsers::default());
            let entity = app
                .world_mut()
                .spawn((
                    edit,
                    FileViewport {
                        top_row,
                        rows,
                        wrap_columns: 0,
                        word_wrap: vmux_core::editor::WordWrap::Off,
                        word_wrap_column: 80,
                    },
                    EditorKeymap(vmux_core::editor::KeymapKind::Vscode.make(&[], "\\")),
                ))
                .id();
            (app, entity)
        }
    }

    impl EditorWindow {
        fn nested_file(lines: usize) -> EditState {
            let mut text = String::new();
            for i in 0..lines {
                match i % 8 {
                    0 => text.push_str(&format!("fn f{i}() {{\n")),
                    7 => text.push_str("}\n"),
                    _ => text.push_str(&format!("    let v{i} = {i};\n")),
                }
            }
            let path = PathBuf::from("/tmp/window.rs");
            let core = EditCore::new(
                path.clone(),
                "Rust".into(),
                &text,
                crate::host::edit::EditMode::Normal,
            );
            let mut folds = crate::host::fold::FoldState::default();
            folds.set_regions(crate::host::fold::indent_regions(&core.buffer.rope));
            let mut edit = EditState::new(core, HighlightCache::new(&path), folds);
            edit.sync_fold_view();
            edit
        }

        fn scrolled(top_row: u32, wrap_columns: u16) -> FileViewport {
            FileViewport {
                top_row,
                rows: 40,
                wrap_columns,
                word_wrap: vmux_core::editor::WordWrap::On,
                word_wrap_column: 80,
            }
        }

        fn paired(patch: &FileViewportPatch) -> bool {
            patch.lines.len() == patch.layouts.len()
                && std::iter::zip(&patch.lines, &patch.layouts)
                    .all(|(line, layout)| line.line_no == layout.line_no)
        }
    }

    #[test]
    fn collapsed_region_is_hidden_from_the_window() {
        let rope = Rope::from_str("fn a() {\n    x;\n    y;\n}\nz;\n");
        let mut folds = crate::host::fold::FoldState::default();
        folds.set_regions(crate::host::fold::indent_regions(&rope));
        folds.close(0);
        let view = folds.view(rope.len_lines() as u32);
        let visible = view.lines_for_window(0, view.visible_count());

        assert!(visible.contains(&0));
        assert!(!visible.contains(&1) && !visible.contains(&2));
        assert!(visible.contains(&3));
    }

    #[test]
    fn scrolling_past_the_caret_drags_it_to_the_top_edge() {
        let (mut edit, viewport) = ScrolledCursor::at(2, 10, 20);
        assert!(ScrolledCursor::follow(&mut edit, &viewport));
        assert_eq!(ScrolledCursor::cursor_line(&edit), 10);
    }

    #[test]
    fn scrolling_back_past_the_caret_drags_it_to_the_bottom_edge() {
        let (mut edit, viewport) = ScrolledCursor::at(30, 0, 20);
        assert!(ScrolledCursor::follow(&mut edit, &viewport));
        assert_eq!(ScrolledCursor::cursor_line(&edit), 19);
    }

    #[test]
    fn a_caret_still_on_screen_is_left_alone() {
        let (mut edit, viewport) = ScrolledCursor::at(12, 10, 20);
        assert!(!ScrolledCursor::follow(&mut edit, &viewport));
        assert_eq!(ScrolledCursor::cursor_line(&edit), 12);
    }

    #[test]
    fn a_dragged_caret_keeps_its_column() {
        let (mut edit, viewport) = ScrolledCursor::at(2, 10, 20);
        ScrolledCursor::follow(&mut edit, &viewport);
        assert_eq!(edit.core.cursor_pos().col, 3);
    }

    #[test]
    fn a_report_that_asks_for_no_rows_still_moves_the_host_viewport() {
        let (mut app, entity) = FileViewport::scrolling(0, 40);
        app.world_mut().trigger(BinReceive {
            webview: entity,
            payload: FileScrollEvent {
                top_row: 120,
                needs_rows: false,
            },
        });

        assert_eq!(
            app.world().get::<FileViewport>(entity).unwrap().top_row,
            120
        );
        assert_eq!(
            app.world().get::<EditState>(entity).unwrap().core.top_row,
            120,
            "screen-relative motions read core.top_row"
        );
    }

    #[test]
    fn a_report_past_the_end_is_clamped_to_the_last_screenful() {
        let (mut app, entity) = FileViewport::scrolling(0, 40);
        app.world_mut().trigger(BinReceive {
            webview: entity,
            payload: FileScrollEvent {
                top_row: 9_000,
                needs_rows: true,
            },
        });

        assert_eq!(
            app.world().get::<FileViewport>(entity).unwrap().top_row,
            361
        );
    }

    #[test]
    fn every_laid_out_row_carries_its_line_however_the_window_is_placed() {
        for wrap_columns in [0u16, 12, 100] {
            for collapsed in [false, true] {
                let mut edit = EditorWindow::nested_file(400);
                if collapsed {
                    for header in (0..400).step_by(8) {
                        edit.folds.close(header as u32);
                    }
                    edit.sync_fold_view();
                }
                let rows =
                    EditorWindow::render(&mut edit, &EditorWindow::scrolled(0, wrap_columns))
                        .total_rows;
                for top in (0..rows + 40).step_by(11) {
                    let patch =
                        EditorWindow::render(&mut edit, &EditorWindow::scrolled(top, wrap_columns));
                    assert!(
                        EditorWindow::paired(&patch),
                        "cols {wrap_columns} collapsed {collapsed} top {top}: {} layouts against {} lines",
                        patch.layouts.len(),
                        patch.lines.len()
                    );
                }
            }
        }
    }

    #[test]
    fn opening_a_line_deep_in_the_file_keeps_the_window_whole() {
        let mut edit = EditorWindow::nested_file(400);
        let viewport = EditorWindow::scrolled(300, 100);
        let at = edit.core.buffer.coords_to_char(316, 0);
        edit.core.set_caret(at);

        for command in [
            EditCommand::OpenLine { above: false },
            EditCommand::InsertText("let inserted = 1;".into()),
        ] {
            edit.core.apply(command);
            let (line, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
            edit.hl.invalidate_from(line.saturating_sub(1));
            edit.folds
                .set_regions(crate::host::fold::indent_regions(&edit.core.buffer.rope));
            edit.sync_fold_view();
            assert!(EditorWindow::paired(&EditorWindow::render(
                &mut edit, &viewport
            )));
        }
    }
}
