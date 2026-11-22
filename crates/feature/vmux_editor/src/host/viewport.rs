use crate::host::editor::{Edit, EditItem};
use bevy::prelude::*;
use bevy::prelude::{App, Commands, Plugin};
use bevy_cef::prelude::*;
use vmux_ecs::event::{
    FileCursorEvent, FileFoldToggle, FileResizeEvent, FileScrollByEvent, FileScrollEvent,
    FileViewportPatch,
};
use vmux_ecs::scroll::{
    EDITOR_OVERSCAN_K, OVERSCAN_CAP, OVERSCAN_FLOOR, Overscan, ScrollWindow, ViewportRows,
};

use crate::host::edit::Selection;
use crate::host::editor::FileView;
use crate::host::file_lifecycle::EditorFileLoadedSet;
use crate::host::fold::FoldState;
use crate::host::highlight::HighlightScheme;
use crate::host::keymap::EditorKeymap;

pub(crate) struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HighlightScheme>()
            .add_plugins(UiEventPlugin::<(FileResizeEvent, FileScrollEvent)>::default())
            .add_systems(
                Update,
                (
                    sync_wrap_settings.after(EditorFileLoadedSet),
                    rehighlight_on_color_scheme,
                    apply_lsp_folds.before(crate::host::fold_preferences::FoldSet::Project),
                ),
            )
            .add_observer(file_resize)
            .add_observer(file_scroll)
            .add_observer(file_fold_toggle)
            .add_observer(render)
            .add_observer(render_cursor);
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct FileViewport {
    pub(crate) top_row: u32,
    pub(crate) rows: u16,
    pub(crate) wrap_columns: u16,
    pub(crate) word_wrap: vmux_api::editor::WordWrap,
    pub(crate) word_wrap_column: u16,
    pub(crate) scroll_revision: u64,
}

impl FileViewport {
    const STICKY_SCROLL_DEPTH: usize = 5;

    fn highlight_window(&self, edit: &mut EditItem<'_, '_>) -> (u32, u16) {
        let wrap = edit.wrapped_view(self);
        let visible = wrap.total_rows();
        let (first_row, end_row) = ScrollWindow::new(visible, self.top_row, self.rows).range();
        let overscan =
            Overscan::new(self.rows, EDITOR_OVERSCAN_K, OVERSCAN_FLOOR, OVERSCAN_CAP).rows();
        let from = first_row.saturating_sub(overscan);
        let to = end_row.saturating_add(overscan).min(visible);
        let first_line = wrap.line_at(from).unwrap_or(0);
        let last_line = wrap.line_at(to.saturating_sub(1)).unwrap_or(first_line);
        let rows = last_line.saturating_sub(first_line).saturating_add(1);
        (first_line, rows.min(u16::MAX as u32) as u16)
    }

    pub(crate) fn set_top(&mut self, top: u32) -> Option<FileScrollByEvent> {
        let previous = self.top_row;
        if top == previous {
            return None;
        }
        self.top_row = top;
        self.scroll_revision = self.scroll_revision.wrapping_add(1).max(1);
        Some(FileScrollByEvent {
            revision: self.scroll_revision,
            lines: top as i32 - previous as i32,
        })
    }

    pub(crate) fn visible_rows(&self, edit: &mut EditItem<'_, '_>) -> u32 {
        edit.wrapped_view(self).total_rows()
    }

    pub(crate) fn autoscroll(&self, edit: &mut EditItem<'_, '_>) -> Option<u32> {
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

    pub(crate) fn follow_scrolled_cursor(&self, edit: &mut EditItem<'_, '_>) -> bool {
        if self.rows == 0 {
            return false;
        }
        let cursor = edit.core.cursor_pos();
        let top = self.top_row;
        let bottom = top + self.rows as u32 - 1;
        let row = edit.wrapped_view(self).position(cursor.line, cursor.col).0;
        let wanted = if row < top {
            top
        } else if row > bottom {
            bottom
        } else {
            return false;
        };
        let Some(line) = edit.wrapped_view(self).line_at(wanted) else {
            return false;
        };
        if line == cursor.line {
            return false;
        }
        let col = edit.core.char_at_cell(line as usize, cursor.col);
        let at = edit.core.buffer.coords_to_char(line as usize, col);
        if edit.core.mode.value.is_visual() {
            let anchor = edit.core.primary().anchor;
            edit.core.carets.selections = vec![Selection { anchor, head: at }];
            return true;
        }
        edit.core.collapse_carets();
        edit.core.set_caret(at);
        true
    }

    pub(crate) fn left_render_band(&self, previous_top: u32) -> bool {
        let rows = self.top_row.abs_diff(previous_top);
        let overscan =
            Overscan::new(self.rows, EDITOR_OVERSCAN_K, OVERSCAN_FLOOR, OVERSCAN_CAP).rows();
        rows > overscan / 2
    }

    pub(crate) fn patch(&self, edit: &mut EditItem<'_, '_>) -> FileViewportPatch {
        let total = edit.core.buffer.len_lines() as u32;
        let wrap = edit.wrapped_view(self);
        let (visible, wrap_columns) = (wrap.total_rows(), wrap.columns());
        let (visible_first, visible_end) =
            ScrollWindow::new(visible, self.top_row, self.rows).range();
        let overscan =
            Overscan::new(self.rows, EDITOR_OVERSCAN_K, OVERSCAN_FLOOR, OVERSCAN_CAP).rows();
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
            let indent_width = edit.indent_width();
            for layout in &layouts {
                let index = (layout.line_no - first_line) as usize;
                let Some(line) = window.get_mut(index) else {
                    continue;
                };
                let mut line = std::mem::take(line);
                line.fold = edit.folds.gutter(layout.line_no);
                line.indent_levels = FoldState::indent_levels(
                    &edit.core.buffer.rope,
                    indent_width,
                    layout.line_no as usize,
                );
                lines.push(line);
            }
        }
        let mut sticky = Vec::new();
        if let Some(top) = visible_top {
            let indent_width = edit.indent_width();
            for header in edit.folds.sticky(top, Self::STICKY_SCROLL_DEPTH) {
                let at = header as usize;
                let mut window = edit.hl.line_window(&edit.core.buffer.rope, at, at + 1);
                if window.is_empty() {
                    continue;
                }
                let mut line = window.remove(0);
                line.fold = edit.folds.gutter(header);
                line.indent_levels =
                    FoldState::indent_levels(&edit.core.buffer.rope, indent_width, at);
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

#[derive(Component)]
pub(crate) struct FoldsDirty;

#[derive(EntityEvent)]
pub(crate) struct ViewportRenderRequest {
    #[event_target]
    entity: Entity,
}

impl ViewportRenderRequest {
    pub(crate) fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

fn render(
    trigger: On<ViewportRenderRequest>,
    mut views: Query<(Edit, &FileViewport)>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    if !browsers.can_emit_to(&entity) {
        return;
    }
    let Ok((mut edit, viewport)) = views.get_mut(entity) else {
        return;
    };
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &viewport.patch(&mut edit),
    ));
}

#[derive(EntityEvent)]
pub(crate) struct CursorRenderRequest {
    #[event_target]
    entity: Entity,
}

impl CursorRenderRequest {
    pub(crate) fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

fn render_cursor(
    trigger: On<CursorRenderRequest>,
    mut views: Query<(
        Edit,
        &EditorKeymap,
        &crate::keymap::vim::VimKeymap,
        &FileViewport,
    )>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    if !browsers.can_emit_to(&entity) {
        return;
    }
    let Ok((mut edit, keymap, parser, viewport)) = views.get_mut(entity) else {
        return;
    };
    let total = edit.core.buffer.len_lines() as u32;
    let view = edit.folds.view(total);
    let source_primary = edit.core.cursor_pos();
    let mut primary = source_primary;
    let (span_first, span_rows) = viewport.highlight_window(&mut edit);
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
    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(
        entity,
        &FileCursorEvent {
            search_total,
            search_index,
            mode: edit.core.mode.value,
            mode_label: match keymap.0 {
                crate::keymap::KeymapKind::Vim => parser.label(edit.core.mode.value),
                crate::keymap::KeymapKind::Vscode => String::new(),
            },
            primary,
            carets,
            selections,
            source_primary,
            source_selections: raw_selections,
            search,
            word_highlights,
        },
    ));
    if edit.is_note() {
        commands.trigger(super::note::RenderRequest::new(entity));
    }
}

fn rehighlight_on_color_scheme(
    mut changes: MessageReader<vmux_setting::ColorSchemeChanged>,
    mut scheme: ResMut<HighlightScheme>,
    mut views: Query<(Entity, Edit), With<FileViewport>>,
    mut commands: Commands,
) {
    let Some(change) = changes.read().last().copied() else {
        return;
    };
    let dark = matches!(change.0, vmux_setting::ResolvedScheme::Dark);
    scheme.set_dark(dark);
    for (entity, mut edit) in &mut views {
        edit.hl.set_dark(dark);
        if edit.is_note() {
            edit.refresh_parsed_note();
            commands.trigger(super::note::RenderRequest::new(entity));
        }
        commands.trigger(ViewportRenderRequest::new(entity));
    }
}

fn sync_wrap_settings(
    settings: Res<vmux_setting::AppSettings>,
    mut views: Query<(Entity, &mut FileViewport, Option<Edit>, Has<EditorKeymap>)>,
    browsers: crate::host::delivery::Surface,
    mut commands: Commands,
) {
    for (entity, mut viewport, edit, has_keymap) in &mut views {
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
                if let Some(scroll) = viewport.set_top(wanted.unwrap_or(0))
                    && browsers.can_emit_to(&entity)
                {
                    commands.trigger(vmux_ecs::FileUiStateWrite::from_event(entity, &scroll));
                }
                edit.core.viewport.top_row = viewport.top_row;
            }
            commands.trigger(ViewportRenderRequest::new(entity));
            if has_keymap {
                commands.trigger(CursorRenderRequest::new(entity));
            }
        }
    }
}

fn file_resize(
    trigger: On<UiInput<FileResizeEvent>>,
    mut views: Query<(&mut FileViewport, Option<Edit>, Has<EditorKeymap>)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let event = &trigger.event().payload;
    let Ok((mut viewport, edit, has_keymap)) = views.get_mut(entity) else {
        return;
    };
    let rows = ViewportRows::from_pixels(event.char_height, event.viewport_height).get();
    if viewport.rows == rows && viewport.wrap_columns == event.wrap_columns {
        return;
    }
    viewport.rows = rows;
    viewport.wrap_columns = event.wrap_columns;
    if let Some(mut edit) = edit {
        edit.core.viewport.rows = viewport.rows;
        edit.core.viewport.top_row = viewport.top_row;
        commands.trigger(ViewportRenderRequest::new(entity));
        if has_keymap {
            commands.trigger(CursorRenderRequest::new(entity));
        }
    }
}

fn file_scroll(
    trigger: On<UiInput<FileScrollEvent>>,
    mut views: Query<(Edit, &mut FileViewport), With<EditorKeymap>>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let event = &trigger.event().payload;
    let Ok((mut edit, mut viewport)) = views.get_mut(entity) else {
        return;
    };
    let visible = viewport.visible_rows(&mut edit);
    viewport.top_row = ScrollWindow::new(visible, event.top_row, viewport.rows).top();
    edit.core.viewport.top_row = viewport.top_row;
    if !event.needs_rows {
        return;
    }
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
}

fn file_fold_toggle(
    trigger: On<UiInput<FileFoldToggle>>,
    mut views: Query<Edit, (With<EditorKeymap>, With<FileViewport>)>,
    mut commands: Commands,
) {
    let entity = trigger.event().webview;
    let line = trigger.event().payload.line;
    let Ok(mut edit) = views.get_mut(entity) else {
        return;
    };
    edit.folds.toggle_header(line);
    edit.sync_fold_view();
    commands.trigger(ViewportRenderRequest::new(entity));
    commands.trigger(CursorRenderRequest::new(entity));
    commands.entity(entity).insert(FoldsDirty);
}

type FoldViews<'w, 's> =
    Query<'w, 's, (Edit, &'static FileView), (With<EditorKeymap>, With<FileViewport>)>;

fn apply_lsp_folds(
    mut folds: MessageReader<crate::host::lsp::manager::LspFolds>,
    mut view_params: ParamSet<(crate::lsp::manager::LspManager, FoldViews)>,
    mut commands: Commands,
) {
    for fold in folds.read() {
        if fold
            .origin
            .is_some_and(|origin| !view_params.p0().origin_current(&origin))
        {
            continue;
        }
        let mut views = view_params.p1();
        let Ok((mut edit, file)) = views.get_mut(fold.entity) else {
            continue;
        };
        if vmux_path::PathIdentity::resolve(&file.path)
            != vmux_path::PathIdentity::resolve(&fold.path)
        {
            continue;
        }
        if fold.regions.is_empty() {
            let regions = FoldState::indent_regions(&edit.core.buffer.rope);
            edit.folds.set_regions(regions);
        } else {
            edit.folds.set_regions(fold.regions.clone());
        }
        edit.sync_fold_view();
        commands.trigger(ViewportRenderRequest::new(fold.entity));
        commands.trigger(CursorRenderRequest::new(fold.entity));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ropey::Rope;

    use super::*;
    use crate::host::edit::EditCommand;
    use crate::host::edit::highlight_cache::HighlightCache;
    use crate::host::editor::Editor;

    struct Fixture {
        world: World,
        entity: Entity,
    }

    impl Fixture {
        fn new(editor: crate::host::editor::EditorBundle, viewport: FileViewport) -> Self {
            let mut world = World::new();
            let entity = world.spawn((editor, viewport)).id();
            Self { world, entity }
        }

        fn at(cursor_line: usize, top_row: u32, rows: u16) -> Self {
            let text = (0..40).map(|i| format!("line {i}\n")).collect::<String>();
            let path = PathBuf::from("/tmp/scroll.rs");
            let mut core = crate::edit::Document::new(
                path.clone(),
                "Rust".into(),
                &text,
                crate::host::edit::EditMode::Normal,
            );
            core.carets
                .place(core.buffer.coords_to_char(cursor_line, 3));
            Self::new(
                crate::host::editor::EditorBundle::new(
                    core,
                    HighlightCache::new(&path),
                    crate::host::fold::FoldState::default(),
                ),
                FileViewport {
                    top_row,
                    rows,
                    wrap_columns: 0,
                    word_wrap: vmux_api::editor::WordWrap::Off,
                    word_wrap_column: 80,
                    scroll_revision: 0,
                },
            )
        }

        fn nested_file(lines: usize) -> Self {
            let mut text = String::new();
            for i in 0..lines {
                match i % 8 {
                    0 => text.push_str(&format!("fn f{i}() {{\n")),
                    7 => text.push_str("}\n"),
                    _ => text.push_str(&format!("    let v{i} = {i};\n")),
                }
            }
            let path = PathBuf::from("/tmp/window.rs");
            let core = crate::edit::Document::new(
                path.clone(),
                "Rust".into(),
                &text,
                crate::host::edit::EditMode::Normal,
            );
            let mut folds = crate::host::fold::FoldState::default();
            folds.set_indented(&core.buffer.rope);
            let mut fixture = Self::new(
                crate::host::editor::EditorBundle::new(core, HighlightCache::new(&path), folds),
                FileViewport {
                    top_row: 0,
                    rows: 40,
                    wrap_columns: 0,
                    word_wrap: vmux_api::editor::WordWrap::On,
                    word_wrap_column: 80,
                    scroll_revision: 0,
                },
            );
            fixture.edit().sync_fold_view();
            fixture
        }

        fn scrolling(top_row: u32, rows: u16) -> (App, Entity) {
            let text = (0..400).map(|i| format!("line {i}\n")).collect::<String>();
            let path = PathBuf::from("/tmp/scroll-report.rs");
            let core = crate::edit::Document::new(
                path.clone(),
                "Rust".into(),
                &text,
                crate::host::edit::EditMode::Normal,
            );
            let edit = crate::host::editor::EditorBundle::new(
                core,
                HighlightCache::new(&path),
                crate::host::fold::FoldState::default(),
            );
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ViewportPlugin))
                .insert_non_send(Browsers::default());
            let entity = app
                .world_mut()
                .spawn((
                    edit,
                    FileViewport {
                        top_row,
                        rows,
                        wrap_columns: 0,
                        word_wrap: vmux_api::editor::WordWrap::Off,
                        word_wrap_column: 80,
                        scroll_revision: 0,
                    },
                    EditorKeymap(vmux_api::editor::KeymapKind::Vscode),
                ))
                .id();
            (app, entity)
        }

        fn edit(&mut self) -> EditItem<'_, 'static> {
            Editor::get_mut(&mut self.world, self.entity).unwrap()
        }

        fn viewport(&self) -> FileViewport {
            *self.world.get::<FileViewport>(self.entity).unwrap()
        }

        fn scrolled(&mut self, top_row: u32, wrap_columns: u16) {
            let mut viewport = self.world.get_mut::<FileViewport>(self.entity).unwrap();
            viewport.top_row = top_row;
            viewport.wrap_columns = wrap_columns;
        }

        fn follow_scrolled_cursor(&mut self) -> bool {
            let viewport = self.viewport();
            let mut edit = self.edit();
            viewport.follow_scrolled_cursor(&mut edit)
        }

        fn cursor_line(&mut self) -> u32 {
            self.edit().core.cursor_pos().line
        }

        fn patch(&mut self) -> FileViewportPatch {
            let viewport = self.viewport();
            let mut edit = self.edit();
            viewport.patch(&mut edit)
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
        folds.set_indented(&rope);
        folds.close(0);
        let view = folds.view(rope.len_lines() as u32);
        let visible = view.lines_for_window(0, view.visible_count());

        assert!(visible.contains(&0));
        assert!(!visible.contains(&1) && !visible.contains(&2));
        assert!(visible.contains(&3));
    }

    #[test]
    fn scrolling_past_the_caret_drags_it_to_the_top_edge() {
        let mut fixture = Fixture::at(2, 10, 20);
        assert!(fixture.follow_scrolled_cursor());
        assert_eq!(fixture.cursor_line(), 10);
    }

    #[test]
    fn scrolling_back_past_the_caret_drags_it_to_the_bottom_edge() {
        let mut fixture = Fixture::at(30, 0, 20);
        assert!(fixture.follow_scrolled_cursor());
        assert_eq!(fixture.cursor_line(), 19);
    }

    #[test]
    fn a_caret_still_on_screen_is_left_alone() {
        let mut fixture = Fixture::at(12, 10, 20);
        assert!(!fixture.follow_scrolled_cursor());
        assert_eq!(fixture.cursor_line(), 12);
    }

    #[test]
    fn a_dragged_caret_keeps_its_column() {
        let mut fixture = Fixture::at(2, 10, 20);
        fixture.follow_scrolled_cursor();
        assert_eq!(fixture.edit().core.cursor_pos().col, 3);
    }

    #[test]
    fn a_report_that_asks_for_no_rows_still_moves_the_host_viewport() {
        let (mut app, entity) = Fixture::scrolling(0, 40);
        app.world_mut().trigger(UiInput {
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
            Editor::read(app.world(), entity)
                .unwrap()
                .core
                .viewport
                .top_row,
            120,
            "screen-relative motions read core.viewport.top_row"
        );
    }

    #[test]
    fn a_report_past_the_end_is_clamped_to_the_last_screenful() {
        let (mut app, entity) = Fixture::scrolling(0, 40);
        app.world_mut().trigger(UiInput {
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
                let mut fixture = Fixture::nested_file(400);
                if collapsed {
                    let mut edit = fixture.edit();
                    for header in (0..400).step_by(8) {
                        edit.folds.close(header as u32);
                    }
                    edit.sync_fold_view();
                }
                fixture.scrolled(0, wrap_columns);
                let rows = fixture.patch().total_rows;
                for top in (0..rows + 40).step_by(11) {
                    fixture.scrolled(top, wrap_columns);
                    let patch = fixture.patch();
                    assert!(
                        Fixture::paired(&patch),
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
        let mut fixture = Fixture::nested_file(400);
        fixture.scrolled(300, 100);
        let mut edit = fixture.edit();
        let at = edit.core.buffer.coords_to_char(316, 0);
        edit.core.set_caret(at);

        for command in [
            EditCommand::OpenLine { above: false },
            EditCommand::InsertText("let inserted = 1;".into()),
        ] {
            let mut edit = fixture.edit();
            edit.core.apply(command);
            let (line, _) = edit.core.buffer.char_to_coords(edit.core.primary().head);
            edit.hl.invalidate_from(line.saturating_sub(1));
            edit.folds.set_indented(&edit.core.buffer.rope);
            edit.sync_fold_view();
            assert!(Fixture::paired(&fixture.patch()));
        }
    }
}
