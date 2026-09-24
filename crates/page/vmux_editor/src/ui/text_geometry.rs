use dioxus::html::geometry::ElementPoint;
use vmux_core::event::{FileLine, FileLineLayout};

use crate::page_model::{CellMetrics, ColumnRuler, gutter_width};

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
