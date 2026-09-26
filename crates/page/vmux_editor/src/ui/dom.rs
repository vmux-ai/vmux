use std::rc::Rc;

use dioxus::prelude::*;
use vmux_core::event::{FileDocumentKind, FileResizeEvent, FileViewMode};
use vmux_ui::hooks::send;
use vmux_ui::scroll::ScrollIntoView;

use super::text_geometry::gutter_px;
use super::{INPUT_ID, SCROLL_ID};
use crate::page_model::{CellMetrics, centered_scroll_top};

pub(super) struct ScrolledLineHeight;

impl ScrolledLineHeight {
    const NOTE: f64 = 28.0;

    pub(super) fn resolve(
        mode: FileViewMode,
        kind: FileDocumentKind,
        cell_height: f64,
    ) -> Option<f64> {
        let height = match mode == FileViewMode::Note && kind == FileDocumentKind::Markdown {
            true => Self::NOTE,
            false => cell_height,
        };
        (height > 0.0).then_some(height)
    }
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

#[derive(Clone, Copy)]
pub(super) struct EditorDom {
    element: Signal<Option<Rc<MountedData>>>,
    field: Signal<Option<Rc<MountedData>>>,
    geometry: Signal<ScrollBox>,
    offset: Signal<(f64, f64)>,
}

impl EditorDom {
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
            ScrolledLineHeight::resolve(FileViewMode::Note, FileDocumentKind::Text, 18.0),
            Some(18.0)
        );
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Editor, FileDocumentKind::Markdown, 18.0),
            Some(18.0)
        );
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Note, FileDocumentKind::Markdown, 18.0),
            Some(ScrolledLineHeight::NOTE)
        );
    }

    #[test]
    fn an_unmeasured_cell_refuses_the_scroll_rather_than_landing_at_zero() {
        assert_eq!(
            ScrolledLineHeight::resolve(FileViewMode::Note, FileDocumentKind::Text, 0.0),
            None
        );
    }
}
