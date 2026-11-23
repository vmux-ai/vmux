use alacritty_terminal::{
    event::{Event as TermEvent, EventListener as TermEventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Line},
    term::{Config as TermConfig, Term, TermMode},
    vte::ansi::Processor,
};
use bevy::platform::cell::SyncCell;
use bevy::prelude::*;
use tokio::sync::broadcast;
use vmux_ecs::event::{CursorShape, TermCursor, TermLine, TermSelectionRange};

use crate::pty::PtyInputWriter;
use crate::render::TermRow;
use crate::run_marker::{RunMarker, RunMarkerScanner};
use crate::shell_events::{ShellEvent, ShellEvents};
use crate::viewport::{ProcessSnapshot, ProcessUpdate};

#[derive(Component)]
pub(crate) struct ProcessEmulator(pub(crate) SyncCell<TerminalEmulator>);

#[derive(Clone)]
pub(crate) struct ServiceEventProxy {
    pub(crate) pty_writer: PtyInputWriter,
    pub(crate) update_tx: broadcast::Sender<ProcessUpdate>,
}

impl TermEventListener for ServiceEventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::PtyWrite(text) => self.pty_writer.write_raw(text.as_bytes()),
            TermEvent::Title(title) => {
                let _ = self.update_tx.send(ProcessUpdate::Title(title));
            }
            TermEvent::Bell => {
                let _ = self.update_tx.send(ProcessUpdate::Bell);
            }
            _ => {}
        }
    }
}

struct PtyDimensions {
    cols: u16,
    rows: u16,
}

impl Dimensions for PtyDimensions {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

pub(crate) struct TerminalEmulator {
    term: Term<ServiceEventProxy>,
    processor: Processor,
    shell_events: ShellEvents,
    run_marker: RunMarkerScanner,
}

impl TerminalEmulator {
    pub(crate) fn new(
        cols: u16,
        rows: u16,
        pty_writer: PtyInputWriter,
        update_tx: broadcast::Sender<ProcessUpdate>,
    ) -> Self {
        let proxy = ServiceEventProxy {
            pty_writer,
            update_tx,
        };
        let dims = PtyDimensions { cols, rows };
        let term = Term::new(TermConfig::default(), &dims, proxy);
        Self {
            term,
            processor: Processor::new(),
            shell_events: ShellEvents::new(),
            run_marker: RunMarkerScanner::new(),
        }
    }

    pub(crate) fn advance(&mut self, data: &[u8]) -> (Vec<ShellEvent>, Vec<RunMarker>) {
        self.processor.advance(&mut self.term, data);
        let events = self.shell_events.feed(data);
        let markers = self.run_marker.feed(data);
        (events, markers)
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn advance_term_only(&mut self, data: &[u8]) {
        self.processor.advance(&mut self.term, data);
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        self.term.resize(PtyDimensions { cols, rows });
    }

    pub(crate) fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    pub(crate) fn display_offset(&self) -> i32 {
        self.term.grid().display_offset() as i32
    }

    pub(crate) fn scroll_display(&mut self, delta: i32) -> i32 {
        let old_offset = self.term.grid().display_offset() as i32;
        self.term.scroll_display(Scroll::Delta(delta));
        let new_offset = self.term.grid().display_offset() as i32;
        new_offset - old_offset
    }

    pub(crate) fn dimensions(&self) -> (usize, usize, u32) {
        let grid = self.term.grid();
        (
            grid.columns(),
            grid.screen_lines(),
            grid.total_lines() as u32,
        )
    }

    pub(crate) fn cursor_point(&self) -> (u16, u16) {
        let point = self.term.grid().cursor.point;
        (point.column.0 as u16, point.line.0 as u16)
    }

    pub(crate) fn cursor_char(&self, copy_cursor: Option<(u16, u16)>) -> String {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let num_cols = grid.columns();
        let offset = grid.display_offset() as i32;
        if let Some((col, row)) = copy_cursor {
            if (row as usize) < num_lines && (col as usize) < num_cols {
                let cursor_row = &grid[Line(row as i32 - offset)];
                cursor_row[Column(col as usize)].c.to_string()
            } else {
                " ".to_string()
            }
        } else {
            let point = grid.cursor.point;
            grid[point.line][point.column].c.to_string()
        }
    }

    pub(crate) fn document_cursor(&self, history: u32) -> (u16, u32, String) {
        let grid = self.term.grid();
        let point = grid.cursor.point;
        let doc_row = history + point.line.0 as u32;
        let col = point.column.0 as u16;
        let ch = grid[point.line][point.column].c.to_string();
        (col, doc_row, ch)
    }

    pub(crate) fn row_hash(&self, row_idx: usize, offset: i32) -> u64 {
        TermRow::hash(&self.term, row_idx, offset)
    }

    pub(crate) fn row_line(&self, row_idx: usize, offset: i32) -> TermLine {
        TermRow::line(&self.term, row_idx, offset)
    }

    pub(crate) fn cell_char(&self, col: u16, row: u16) -> Option<char> {
        let grid = self.term.grid();
        if (row as usize) >= grid.screen_lines() || (col as usize) >= grid.columns() {
            return None;
        }
        let offset = grid.display_offset() as i32;
        Some(grid[Line(row as i32 - offset)][Column(col as usize)].c)
    }

    pub(crate) fn row_is_blank(&self, row: u16) -> bool {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() {
            return true;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        (0..num_cols).all(|col| line[Column(col)].c.is_whitespace())
    }

    pub(crate) fn first_non_blank_col(&self, row: u16) -> u16 {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() {
            return 0;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        for col in 0..num_cols {
            if !line[Column(col)].c.is_whitespace() {
                return col as u16;
            }
        }
        0
    }

    pub(crate) fn last_non_blank_col(&self, row: u16) -> u16 {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() {
            return 0;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        for col in (0..num_cols).rev() {
            if !line[Column(col)].c.is_whitespace() {
                return col as u16;
            }
        }
        0
    }

    pub(crate) fn word_bounds_at(
        &self,
        col: u16,
        row: u16,
        is_word: impl Fn(char) -> bool,
    ) -> Option<(u16, u16)> {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() || (col as usize) >= num_cols {
            return None;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        if !is_word(line[Column(col as usize)].c) {
            return Some((col, col));
        }
        let mut start = col as usize;
        let mut end = col as usize;
        while start > 0 && is_word(line[Column(start - 1)].c) {
            start -= 1;
        }
        while end + 1 < num_cols && is_word(line[Column(end + 1)].c) {
            end += 1;
        }
        Some((start as u16, end as u16))
    }

    pub(crate) fn line_end_col(&self, row: u16) -> Option<u16> {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        if (row as usize) >= grid.screen_lines() {
            return None;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        let mut end = 0usize;
        for col in 0..num_cols {
            if !line[Column(col)].c.is_whitespace() {
                end = col;
            }
        }
        Some(end as u16)
    }

    pub(crate) fn extract_selection_text(&self, sel: TermSelectionRange) -> Option<String> {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let offset = grid.display_offset() as i32;
        let (sr, sc, er, ec) = if (sel.start_row, sel.start_col) <= (sel.end_row, sel.end_col) {
            (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
        } else {
            (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
        };
        let max_col = num_cols.saturating_sub(1);
        let top_line = grid.topmost_line().0;
        let bottom_line = grid.bottommost_line().0;
        let (block_lo, block_hi) = if sel.is_block {
            (
                sel.start_col.min(sel.end_col) as usize,
                sel.start_col.max(sel.end_col) as usize,
            )
        } else {
            (0, 0)
        };
        let mut lines = Vec::new();
        for row_idx in sr..=er {
            let line_idx = row_idx as i32 - offset;
            if line_idx > bottom_line {
                break;
            }
            if line_idx < top_line {
                continue;
            }
            let line = &grid[Line(line_idx)];
            let (lo, hi) = if sel.is_block {
                (block_lo, block_hi)
            } else if sr == er {
                (sc as usize, ec as usize)
            } else if row_idx == sr {
                (sc as usize, max_col)
            } else if row_idx == er {
                (0, ec as usize)
            } else {
                (0, max_col)
            };
            let hi = hi.min(max_col);
            if lo > hi {
                lines.push(String::new());
                continue;
            }
            let mut text = String::new();
            for col in lo..=hi {
                text.push(line[Column(col)].c);
            }
            while text.ends_with(' ') {
                text.pop();
            }
            lines.push(text);
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    pub(crate) fn snapshot(&self) -> ProcessSnapshot {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let offset = grid.display_offset() as i32;
        let lines = self.visible_lines();
        let point = grid.cursor.point;
        let cursor_char = grid[point.line][point.column].c.to_string();
        ProcessSnapshot {
            lines,
            cursor: TermCursor {
                col: point.column.0 as u16,
                row: point.line.0 as u32,
                shape: CursorShape::Block,
                visible: offset == 0,
                ch: cursor_char,
            },
            cols: grid.columns() as u16,
            rows: num_lines as u16,
        }
    }

    pub(crate) fn visible_text(&self) -> String {
        let mut rows = Vec::new();
        for line in self.visible_lines() {
            let mut text = String::new();
            for span in line.spans {
                text.push_str(&span.text);
            }
            rows.push(text);
        }
        rows.join("\n")
    }

    pub(crate) fn full_text(&self) -> String {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        let mut lines = Vec::new();
        for line_idx in top..=bottom {
            let row = &grid[Line(line_idx)];
            let mut text = String::with_capacity(num_cols);
            for col in 0..num_cols {
                text.push(row[Column(col)].c);
            }
            lines.push(text.trim_end().to_string());
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    fn visible_lines(&self) -> Vec<TermLine> {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let offset = grid.display_offset() as i32;
        let mut lines = Vec::with_capacity(num_lines);
        for row_idx in 0..num_lines {
            lines.push(TermRow::line(&self.term, row_idx, offset));
        }
        lines
    }
}
