use ropey::Rope;
use unicode_width::UnicodeWidthStr;
use vmux_core::editor::{SelSpan, WordWrap};
use vmux_core::event::FileLineLayout;

use crate::fold::FoldView;

pub struct WrapView {
    columns: u16,
    lines: Vec<FileLineLayout>,
    total_rows: u32,
}

impl WrapView {
    pub fn new(
        rope: &Rope,
        folds: &FoldView,
        mode: WordWrap,
        viewport_columns: u16,
        word_wrap_column: u16,
    ) -> Self {
        let columns = resolve_columns(mode, viewport_columns, word_wrap_column);
        let mut lines = Vec::new();
        let mut row = 0u32;
        for line_no in 0..rope.len_lines() as u32 {
            if folds.is_hidden(line_no) {
                continue;
            }
            let text = rope
                .line(line_no as usize)
                .chars()
                .filter(|character| *character != '\n' && *character != '\r')
                .collect::<String>();
            let width = UnicodeWidthStr::width(text.as_str()) as u32;
            let rows = if columns == 0 {
                1
            } else {
                width.max(1).div_ceil(columns as u32)
            }
            .min(u16::MAX as u32) as u16;
            lines.push(FileLineLayout { line_no, row, rows });
            row = row.saturating_add(rows as u32);
        }
        Self {
            columns,
            lines,
            total_rows: row.max(1),
        }
    }

    pub fn columns(&self) -> u16 {
        self.columns
    }

    pub fn total_rows(&self) -> u32 {
        self.total_rows
    }

    pub fn window(&self, first: u32, end: u32) -> Vec<FileLineLayout> {
        let start = self
            .lines
            .partition_point(|line| line.row + line.rows as u32 <= first);
        let end = self.lines.partition_point(|line| line.row < end);
        self.lines[start..end].to_vec()
    }

    pub fn position(&self, line_no: u32, column: u32) -> (u32, u32) {
        let Ok(index) = self
            .lines
            .binary_search_by_key(&line_no, |line| line.line_no)
        else {
            return (0, column);
        };
        let line = &self.lines[index];
        if self.columns == 0 {
            return (line.row, column);
        }
        (
            line.row + column / self.columns as u32,
            column % self.columns as u32,
        )
    }

    pub fn selections(&self, selections: impl IntoIterator<Item = SelSpan>) -> Vec<SelSpan> {
        selections
            .into_iter()
            .flat_map(|selection| self.wrap_selection(selection))
            .collect()
    }

    fn wrap_selection(&self, selection: SelSpan) -> Vec<SelSpan> {
        let Ok(index) = self
            .lines
            .binary_search_by_key(&selection.line, |line| line.line_no)
        else {
            return Vec::new();
        };
        let line = &self.lines[index];
        if self.columns == 0 {
            return vec![SelSpan {
                row: line.row,
                ..selection
            }];
        }
        let columns = self.columns as u32;
        let first_segment = selection.start / columns;
        let last_segment = if selection.end == u32::MAX {
            line.rows.saturating_sub(1) as u32
        } else if selection.end > selection.start {
            selection.end.saturating_sub(1) / columns
        } else {
            first_segment
        };
        (first_segment..=last_segment)
            .map(|segment| SelSpan {
                line: selection.line,
                row: line.row + segment,
                start: if segment == first_segment {
                    selection.start % columns
                } else {
                    0
                },
                end: if selection.end == u32::MAX || segment < last_segment {
                    u32::MAX
                } else {
                    let end = selection.end % columns;
                    if end == 0 && selection.end > selection.start {
                        columns
                    } else {
                        end
                    }
                },
            })
            .collect()
    }
}

fn resolve_columns(mode: WordWrap, viewport: u16, column: u16) -> u16 {
    match mode {
        WordWrap::Off => 0,
        WordWrap::On => viewport,
        WordWrap::WordWrapColumn => column.max(1),
        WordWrap::Bounded if viewport == 0 => 0,
        WordWrap::Bounded => viewport.min(column.max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_wrap_modes_resolve_columns() {
        assert_eq!(resolve_columns(WordWrap::Off, 120, 80), 0);
        assert_eq!(resolve_columns(WordWrap::On, 120, 80), 120);
        assert_eq!(resolve_columns(WordWrap::WordWrapColumn, 120, 80), 80);
        assert_eq!(resolve_columns(WordWrap::Bounded, 120, 80), 80);
        assert_eq!(resolve_columns(WordWrap::Bounded, 60, 80), 60);
    }

    #[test]
    fn wrapped_rows_drive_cursor_and_selection_geometry() {
        let rope = Rope::from_str("abcdefghijkl\nshort\n");
        let folds = crate::fold::FoldState::default().view(rope.len_lines() as u32);
        let view = WrapView::new(&rope, &folds, WordWrap::On, 5, 80);

        assert_eq!(view.total_rows(), 5);
        assert_eq!(view.position(0, 7), (1, 2));
        assert_eq!(
            view.selections([SelSpan {
                line: 0,
                row: 0,
                start: 3,
                end: 11,
            }]),
            vec![
                SelSpan {
                    line: 0,
                    row: 0,
                    start: 3,
                    end: u32::MAX,
                },
                SelSpan {
                    line: 0,
                    row: 1,
                    start: 0,
                    end: u32::MAX,
                },
                SelSpan {
                    line: 0,
                    row: 2,
                    start: 0,
                    end: 1,
                },
            ]
        );
    }
}
