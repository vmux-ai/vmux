use std::rc::Rc;

use dioxus::html::geometry::ElementPoint;
use dioxus::prelude::*;
use vmux_core::event::{FileLine, FileLineLayout, FileResizeEvent, FileViewMode};
use vmux_ui::hooks::send;
use vmux_ui::scroll::ScrollIntoView;

use super::document::is_markdown_file;
use super::{INPUT_ID, SCROLL_ID};
use crate::page_model::{CellMetrics, ColumnRuler, centered_scroll_top, gutter_width};

pub(super) struct ScrolledLineHeight;

impl ScrolledLineHeight {
    const NOTE: f64 = 28.0;

    pub(super) fn resolve(mode: FileViewMode, path: &str, cell_height: f64) -> Option<f64> {
        let height = match mode == FileViewMode::Note && is_markdown_file(path) {
            true => Self::NOTE,
            false => cell_height,
        };
        (height > 0.0).then_some(height)
    }
}

pub(super) fn column_in_line(
    at: ElementPoint,
    gutter: f64,
    cell: CellMetrics,
    text: &str,
    wrap_columns: u16,
    snap: bool,
) -> (f64, u32) {
    let x = at.x - gutter;
    if !cell.measured() {
        return (x, 0);
    }
    if wrap_columns == 0 {
        return (x, ColumnRuler::new(text, cell).col_at(x, snap));
    }
    let segment = (at.y.max(0.0) / cell.height).floor() as u32;
    let local = ColumnRuler::wrapped_row(text, cell, wrap_columns, segment).col_at(x, snap);

    (
        x,
        segment * u32::from(wrap_columns) + local.min(u32::from(wrap_columns)),
    )
}

#[derive(Clone, Copy, Default, PartialEq)]
struct ScrollBox {
    size: (f64, f64),
}

impl ScrollBox {
    fn announce(self, cell: CellMetrics, total_lines: u32, mut last: Signal<FileResizeEvent>) {
        let (cw, ch) = (cell.narrow, cell.height);
        if !cell.measured() || self.size.0 <= 0.0 {
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

pub(super) fn gutter_px(total_lines: u32, char_width: f64) -> f64 {
    gutter_width(total_lines) as f64 * char_width + 48.0
}

pub(super) struct RowRuler<'a> {
    lines: &'a [FileLine],
    layouts: &'a [FileLineLayout],
    metrics: CellMetrics,
    wrap_columns: u16,
}

impl<'a> RowRuler<'a> {
    pub(super) fn new(
        lines: &'a [FileLine],
        layouts: &'a [FileLineLayout],
        metrics: CellMetrics,
        wrap_columns: u16,
    ) -> Self {
        Self {
            lines,
            layouts,
            metrics,
            wrap_columns,
        }
    }

    fn segment_of(&self, row: u32) -> Option<(String, u32)> {
        let mut owner = None;
        for layout in self.layouts {
            if row >= layout.row && row < layout.row + u32::from(layout.rows) {
                owner = Some(layout);
                break;
            }
        }
        let layout = owner?;
        for line in self.lines {
            if line.line_no != layout.line_no {
                continue;
            }
            let mut text = String::new();
            for span in &line.spans {
                text.push_str(&span.text);
            }
            return Some((text, row - layout.row));
        }
        None
    }

    pub(super) fn x_of(&self, row: u32, col: u32) -> f64 {
        let Some((text, segment)) = self.segment_of(row) else {
            return f64::from(col) * self.metrics.narrow;
        };
        ColumnRuler::wrapped_row(&text, self.metrics, self.wrap_columns, segment).x_of(col)
    }

    pub(super) fn width_between(&self, row: u32, start: u32, end: u32) -> f64 {
        let Some((text, segment)) = self.segment_of(row) else {
            return f64::from(end.saturating_sub(start)) * self.metrics.narrow;
        };
        ColumnRuler::wrapped_row(&text, self.metrics, self.wrap_columns, segment)
            .width_between(start, end)
    }

    pub(super) fn advance_at(&self, row: u32, col: u32) -> f64 {
        let Some((text, segment)) = self.segment_of(row) else {
            return self.metrics.narrow;
        };
        ColumnRuler::wrapped_row(&text, self.metrics, self.wrap_columns, segment).advance_at(col)
    }

    pub(super) fn x_of_char(&self, line_no: u32, char_col: u32) -> f64 {
        for line in self.lines {
            if line.line_no != line_no {
                continue;
            }
            let mut text = String::new();
            for span in &line.spans {
                text.push_str(&span.text);
            }
            return ColumnRuler::new(&text, self.metrics).x_of_char(char_col);
        }
        f64::from(char_col) * self.metrics.narrow
    }
}

#[derive(Clone, Copy)]
pub(super) struct FileViewport {
    element: Signal<Option<Rc<MountedData>>>,
    field: Signal<Option<Rc<MountedData>>>,
    geometry: Signal<ScrollBox>,
    offset: Signal<(f64, f64)>,
}

impl FileViewport {
    pub(super) fn new() -> Self {
        Self {
            element: use_signal(|| None),
            field: use_signal(|| None),
            geometry: use_signal(ScrollBox::default),
            offset: use_signal(|| (0.0, 0.0)),
        }
    }

    pub(super) fn announce(
        self,
        cell: CellMetrics,
        total_lines: u32,
        last: Signal<FileResizeEvent>,
    ) {
        self.geometry.read().announce(cell, total_lines, last);
    }

    pub(super) fn scrolled_to(self, offset: (f64, f64)) {
        let mut current = self.offset;
        current.set(offset);
    }

    pub(super) fn resized(self, size: (f64, f64)) {
        let mut geometry = self.geometry;
        if geometry.peek().size == size {
            return;
        }
        geometry.write().size = size;
    }

    pub(super) fn mounted(self, element: Rc<MountedData>) {
        let mut current = self.element;
        current.set(Some(element));
        self.measure();
    }

    pub(super) fn field_mounted(self, element: Rc<MountedData>) {
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

    pub(super) fn scroll_by(self, lines: i32, line_height: f64) {
        let from = self.offset.peek().1;
        self.scroll_to(from + lines as f64 * line_height);
    }

    pub(super) fn reset(self) {
        self.scroll_to(0.0);
    }

    pub(super) fn reveal_caret(self) {
        ScrollIntoView::nearest(INPUT_ID);
    }

    pub(super) fn center_row(self, row: u32, char_height: f64) {
        let geometry = *self.geometry.peek();
        if char_height <= 0.0 || geometry.size.1 <= 0.0 {
            return;
        }
        self.scroll_to(centered_scroll_top(
            row as f64 * char_height + char_height * 0.5,
            geometry.size.1,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_file_scrolls_by_the_cell_height_its_rows_are_drawn_at() {
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Note, "/w/src/main.rs", 18.0),
            Some(18.0)
        );
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Editor, "/w/notes/a.md", 18.0),
            Some(18.0)
        );
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Note, "/w/notes/a.md", 18.0),
            Some(ScrolledLineHeight::NOTE)
        );
    }

    #[test]
    fn an_unmeasured_cell_refuses_the_scroll_rather_than_landing_at_zero() {
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Note, "/w/src/main.rs", 0.0),
            None
        );
    }
}
