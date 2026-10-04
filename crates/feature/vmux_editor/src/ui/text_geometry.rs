use dioxus::html::geometry::ElementPoint;
use vmux_ecs::event::{FileLine, FileLineLayout};

use crate::text::DisplayCells;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct CellMetrics {
    pub(super) narrow: f64,
    pub(super) wide: f64,
    pub(super) height: f64,
}

impl CellMetrics {
    pub(super) fn measured(self) -> bool {
        self.narrow > 0.0 && self.height > 0.0
    }

    pub(super) fn vars(self) -> String {
        if !self.measured() {
            return String::new();
        }
        format!("--cw:{}px;--ch:{}px;", self.narrow, self.height)
    }

    fn wide_advance(self) -> f64 {
        match self.wide > 0.0 {
            true => self.wide,
            false => self.narrow * 2.0,
        }
    }

    fn advance_of(self, character: char) -> f64 {
        match DisplayCells::width_of(character) {
            0 => 0.0,
            2 => self.wide_advance(),
            cells => self.narrow * f64::from(cells),
        }
    }
}

pub(super) struct ColumnRuler<'a> {
    text: &'a str,
    metrics: CellMetrics,
}

impl<'a> ColumnRuler<'a> {
    pub(super) fn new(text: &'a str, metrics: CellMetrics) -> Self {
        Self { text, metrics }
    }

    fn wrapped_row(text: &'a str, metrics: CellMetrics, wrap_columns: u16, index: u32) -> Self {
        if wrap_columns == 0
            || index == 0 && u32::from(wrap_columns) >= DisplayCells::from(text).width()
        {
            return Self::new(text, metrics);
        }
        let columns = u32::from(wrap_columns);
        let skip = index.saturating_mul(columns);
        let mut start = None;
        let mut end = text.len();
        let mut cells = 0;
        for (at, character) in text.char_indices() {
            if start.is_none() && cells >= skip {
                start = Some(at);
            }
            if cells >= skip.saturating_add(columns) {
                end = at;
                break;
            }
            cells += DisplayCells::width_of(character);
        }
        let start = start.unwrap_or(text.len());
        Self::new(&text[start..end.max(start)], metrics)
    }

    pub(super) fn at_point(
        at: ElementPoint,
        gutter: f64,
        metrics: CellMetrics,
        text: &'a str,
        wrap_columns: u16,
        snap: bool,
    ) -> (f64, u32) {
        let x = at.x - gutter;
        if !metrics.measured() {
            return (x, 0);
        }
        if wrap_columns == 0 {
            return (x, Self::new(text, metrics).col_at(x, snap));
        }
        let segment = (at.y.max(0.0) / metrics.height).floor() as u32;
        let local = Self::wrapped_row(text, metrics, wrap_columns, segment).col_at(x, snap);

        (
            x,
            segment * u32::from(wrap_columns) + local.min(u32::from(wrap_columns)),
        )
    }

    pub(super) fn x_of(&self, col: u32) -> f64 {
        let mut cells = 0;
        let mut x = 0.0;
        for character in self.text.chars() {
            if cells >= col {
                return x;
            }
            let width = DisplayCells::width_of(character);
            let advance = self.metrics.advance_of(character);
            if cells + width > col {
                return x + advance * f64::from(col - cells) / f64::from(width);
            }
            cells += width;
            x += advance;
        }
        x + f64::from(col.saturating_sub(cells)) * self.metrics.narrow
    }

    pub(super) fn width_between(&self, start: u32, end: u32) -> f64 {
        (self.x_of(end) - self.x_of(start)).max(0.0)
    }

    pub(super) fn x_of_char(&self, char_col: u32) -> f64 {
        let mut seen = 0;
        let mut x = 0.0;
        for character in self.text.chars() {
            if seen >= char_col {
                return x;
            }
            seen += 1;
            x += self.metrics.advance_of(character);
        }
        x + f64::from(char_col - seen) * self.metrics.narrow
    }

    pub(super) fn advance_at(&self, col: u32) -> f64 {
        let mut cells = 0;
        for character in self.text.chars() {
            let width = DisplayCells::width_of(character);
            if width == 0 {
                continue;
            }
            if cells >= col {
                return self.metrics.advance_of(character);
            }
            cells += width;
        }
        self.metrics.narrow
    }

    fn col_at(&self, x: f64, snap: bool) -> u32 {
        if x <= 0.0 || !self.metrics.measured() {
            return 0;
        }
        let mut cells = 0;
        let mut at = 0.0;
        for character in self.text.chars() {
            let width = DisplayCells::width_of(character);
            if width == 0 {
                continue;
            }
            let advance = self.metrics.advance_of(character);
            if x < at + advance {
                if !snap {
                    return cells;
                }
                let into = (x - at) / advance;
                return match into < 0.5 {
                    true => cells,
                    false => cells + width,
                };
            }
            cells += width;
            at += advance;
        }
        let past = (x - at) / self.metrics.narrow;
        let extra = match snap {
            true => past.round(),
            false => past.floor(),
        };
        cells + extra.max(0.0) as u32
    }
}

pub(super) struct GutterWidth;

impl GutterWidth {
    pub(super) fn for_lines(total_lines: u32) -> usize {
        let digits = total_lines.max(1).to_string().len();
        digits.max(3)
    }

    pub(super) fn pixels(total_lines: u32, char_width: f64) -> f64 {
        Self::for_lines(total_lines) as f64 * char_width + 48.0
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const MENLO: CellMetrics = CellMetrics {
        narrow: 8.4287109375,
        wide: 14.0,
        height: 17.0,
    };

    #[test]
    fn gutter_width_has_a_three_character_minimum() {
        assert_eq!(GutterWidth::for_lines(0), 3);
        assert_eq!(GutterWidth::for_lines(9), 3);
        assert_eq!(GutterWidth::for_lines(1000), 4);
        assert_eq!(GutterWidth::for_lines(99999), 5);
    }

    #[test]
    fn a_wide_glyph_uses_its_measured_advance() {
        let ruler = ColumnRuler::new("今日の予定は？", MENLO);

        assert_eq!(ruler.x_of(14), 7.0 * MENLO.wide);
        assert_eq!(ruler.x_of(4), 2.0 * MENLO.wide);
        assert!(ruler.x_of(14) < 14.0 * MENLO.narrow);
    }

    #[test]
    fn a_mixed_line_round_trips_every_character_boundary() {
        let text = "ab今c😀d\u{0301}e";
        let ruler = ColumnRuler::new(text, MENLO);

        assert_eq!(DisplayCells::from(text).width(), 9);

        let mut boundaries = vec![0];
        let mut cells = 0;
        for character in text.chars() {
            cells += DisplayCells::width_of(character);
            boundaries.push(cells);
        }
        boundaries.dedup();

        for col in boundaries {
            let x = ruler.x_of(col);
            assert_eq!(ruler.col_at(x, true), col);
        }
    }

    #[test]
    fn a_column_inside_a_wide_glyph_snaps_out_to_a_boundary() {
        let ruler = ColumnRuler::new("ab今c", MENLO);

        assert_eq!(ruler.col_at(ruler.x_of(3), true), 4);
        assert_eq!(ruler.col_at(ruler.x_of(3), false), 2);
    }

    #[test]
    fn x_grows_with_every_column_that_has_width() {
        let ruler = ColumnRuler::new("a今b", MENLO);
        let widths = [
            ruler.x_of(0),
            ruler.x_of(1),
            ruler.x_of(2),
            ruler.x_of(3),
            ruler.x_of(4),
        ];

        assert_eq!(widths[0], 0.0);
        assert_eq!(widths[1], MENLO.narrow);
        assert_eq!(widths[2], MENLO.narrow + MENLO.wide / 2.0);
        assert_eq!(widths[3], MENLO.narrow + MENLO.wide);
        assert_eq!(widths[4], MENLO.narrow * 2.0 + MENLO.wide);
    }

    #[test]
    fn a_click_snaps_to_the_nearer_edge_of_a_wide_glyph() {
        let ruler = ColumnRuler::new("今日", MENLO);

        assert_eq!(ruler.col_at(MENLO.wide * 0.4, true), 0);
        assert_eq!(ruler.col_at(MENLO.wide * 0.6, true), 2);
        assert_eq!(ruler.col_at(MENLO.wide * 0.6, false), 0);
        assert_eq!(ruler.col_at(MENLO.wide * 1.6, false), 2);
    }

    #[test]
    fn a_click_past_the_end_counts_narrow_cells() {
        let ruler = ColumnRuler::new("今", MENLO);

        assert_eq!(ruler.col_at(MENLO.wide + MENLO.narrow * 3.0, false), 5);
    }

    #[test]
    fn a_wrapped_row_measures_only_its_own_segment() {
        let ruler = ColumnRuler::wrapped_row("今日の予定は？", MENLO, 4, 1);

        assert_eq!(ruler.x_of(4), 2.0 * MENLO.wide);
        assert_eq!(ruler.col_at(2.0 * MENLO.wide, true), 4);
    }

    #[test]
    fn cell_and_character_columns_diverge_on_wide_text() {
        assert_eq!(DisplayCells::from("今日の予定は？").width(), 14);
        assert_eq!(DisplayCells::from("今日の予定は？").char_at(14), 7);
        assert_eq!(DisplayCells::from("今日の予定は？").char_at(4), 2);
        assert_eq!(DisplayCells::from("ab今").char_at(3), 2);
        assert_eq!(DisplayCells::from("ab今").char_at(4), 3);
    }
}
