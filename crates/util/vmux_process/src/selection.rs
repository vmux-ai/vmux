use alacritty_terminal::term::TermMode;
use bevy::prelude::*;
use vmux_api::protocol::CopyModeKey;
use vmux_ecs::event::{TermLine, TermSelectionRange};

use crate::emulator::TerminalEmulator;
use crate::pty::PtyInputWriter;
use crate::viewport::ProcessViewport;

#[derive(Component, Default, Clone, Copy)]
pub(crate) struct ProcessSelection(pub(crate) Option<TermSelectionRange>);

impl ProcessSelection {
    pub(crate) fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '_' | '.' | '/' | '-')
    }

    pub(crate) fn set(&mut self, range: Option<TermSelectionRange>, cols: u16, rows: u16) {
        self.0 = range.map(|value| Self::clamp_range(value, cols, rows));
    }

    pub(crate) fn extend_to(&mut self, col: u16, row: u16, cols: u16, rows: u16) {
        let (col, row) = Self::clamp_point(col, row, cols, rows);
        let range = match self.0.take() {
            Some(mut range) => {
                range.end_col = col;
                range.end_row = row;
                range
            }
            None => TermSelectionRange {
                start_col: col,
                start_row: row,
                end_col: col,
                end_row: row,
                is_block: false,
            },
        };
        self.0 = Some(range);
    }

    pub(crate) fn select_word_at(
        &mut self,
        emulator: &TerminalEmulator,
        col: u16,
        row: u16,
    ) -> bool {
        let Some((start_col, end_col)) = emulator.word_bounds_at(col, row, Self::is_word_char)
        else {
            return false;
        };
        self.0 = Some(TermSelectionRange {
            start_col,
            start_row: row,
            end_col,
            end_row: row,
            is_block: false,
        });
        true
    }

    pub(crate) fn select_line_at(&mut self, emulator: &TerminalEmulator, row: u16) -> bool {
        let Some(end_col) = emulator.line_end_col(row) else {
            return false;
        };
        self.0 = Some(TermSelectionRange {
            start_col: 0,
            start_row: row,
            end_col,
            end_row: row,
            is_block: false,
        });
        true
    }

    pub(crate) fn text(&self, emulator: &TerminalEmulator) -> Option<String> {
        let selection = self.0?;
        emulator.extract_selection_text(selection)
    }

    pub(crate) fn clear_on_output(&mut self, copy_mode_active: bool) {
        if !copy_mode_active && self.0.is_some() {
            self.0 = None;
        }
    }

    pub(crate) fn clear_on_changed_rows(
        &mut self,
        copy_mode_active: bool,
        full: bool,
        changed_lines: &[(u32, TermLine)],
    ) {
        if copy_mode_active || full {
            return;
        }
        let Some(selection) = self.0 else {
            return;
        };
        let lo = selection.start_row.min(selection.end_row) as u32;
        let hi = selection.start_row.max(selection.end_row) as u32;
        if changed_lines
            .iter()
            .any(|(row, _)| *row >= lo && *row <= hi)
        {
            self.0 = None;
        }
    }

    pub(crate) fn clamp_on_resize(&mut self, cols: u16, rows: u16) {
        if let Some(range) = self.0.take() {
            self.0 = Some(Self::clamp_range(range, cols, rows));
        }
    }

    fn clamp_point(col: u16, row: u16, cols: u16, rows: u16) -> (u16, u16) {
        (
            col.min(cols.saturating_sub(1)),
            row.min(rows.saturating_sub(1)),
        )
    }

    fn clamp_range(mut range: TermSelectionRange, cols: u16, rows: u16) -> TermSelectionRange {
        let max_col = cols.saturating_sub(1);
        let max_row = rows.saturating_sub(1);
        range.start_col = range.start_col.min(max_col);
        range.end_col = range.end_col.min(max_col);
        range.start_row = range.start_row.min(max_row);
        range.end_row = range.end_row.min(max_row);
        range
    }
}

#[derive(Component, Default)]
pub(crate) struct ProcessCopyMode {
    pub(crate) state: Option<CopyModeState>,
}

pub(crate) struct CopyModeState {
    pub(crate) cursor: (u16, u16),
    anchor: (u16, u16),
    visual: Option<CopyModeVisualMode>,
    last_find: Option<CopyModeFind>,
}

#[derive(Clone, Copy)]
pub(crate) enum CopyModeVisualMode {
    Character,
    Line,
}

#[derive(Clone, Copy)]
pub(crate) struct CopyModeFind {
    target: char,
    direction: CopyModeFindDirection,
    till: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyModeFindDirection {
    Forward,
    Backward,
}

impl CopyModeFind {
    fn reversed(self) -> Self {
        Self {
            direction: match self.direction {
                CopyModeFindDirection::Forward => CopyModeFindDirection::Backward,
                CopyModeFindDirection::Backward => CopyModeFindDirection::Forward,
            },
            ..self
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyModeCharClass {
    Space,
    Word,
    Punct,
}

impl ProcessCopyMode {
    pub(crate) fn is_active(&self) -> bool {
        self.state.is_some()
    }

    pub(crate) fn cursor(&self) -> Option<(u16, u16)> {
        self.state.as_ref().map(|state| state.cursor)
    }

    pub(crate) fn enter(
        &mut self,
        emulator: &TerminalEmulator,
        cols: u16,
        rows: u16,
        selection: &mut ProcessSelection,
    ) {
        let (col, row) = emulator.cursor_point();
        let cursor = (
            col.min(cols.saturating_sub(1)),
            row.min(rows.saturating_sub(1)),
        );
        self.state = Some(CopyModeState {
            cursor,
            anchor: cursor,
            visual: None,
            last_find: None,
        });
        selection.0 = None;
    }

    pub(crate) fn exit(&mut self) {
        self.state = None;
    }

    pub(crate) fn cancel(&mut self, selection: &mut ProcessSelection) {
        self.state = None;
        selection.0 = None;
    }

    pub(crate) fn clamp_on_resize(&mut self, cols: u16, rows: u16) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let max_col = cols.saturating_sub(1);
        let max_row = rows.saturating_sub(1);
        state.cursor.0 = state.cursor.0.min(max_col);
        state.cursor.1 = state.cursor.1.min(max_row);
        state.anchor.0 = state.anchor.0.min(max_col);
        state.anchor.1 = state.anchor.1.min(max_row);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_key(
        &mut self,
        key: CopyModeKey,
        emulator: &mut TerminalEmulator,
        pty_writer: &PtyInputWriter,
        cols: u16,
        rows: u16,
        selection: &mut ProcessSelection,
        viewport: &mut ProcessViewport,
    ) -> (bool, Option<String>) {
        let Some(state) = self.state.as_ref() else {
            return (false, None);
        };
        let (cur_col, cur_row, last_find) = (state.cursor.0, state.cursor.1, state.last_find);
        match key {
            CopyModeKey::Exit => {
                self.cancel(selection);
                return (true, None);
            }
            CopyModeKey::Copy => {
                let text = selection.text(emulator);
                self.cancel(selection);
                return (true, text);
            }
            CopyModeKey::StartSelection => {
                if let Some(state) = self.state.as_mut() {
                    state.anchor = state.cursor;
                    state.visual = Some(CopyModeVisualMode::Character);
                }
                self.sync_selection(cols, selection);
                return (false, None);
            }
            CopyModeKey::StartLineSelection => {
                if let Some(state) = self.state.as_mut() {
                    state.anchor = state.cursor;
                    state.visual = Some(CopyModeVisualMode::Line);
                }
                self.sync_selection(cols, selection);
                return (false, None);
            }
            CopyModeKey::SwapSelectionEnds => {
                if let Some(state) = self.state.as_mut()
                    && state.visual.is_some()
                {
                    std::mem::swap(&mut state.cursor, &mut state.anchor);
                }
                self.sync_selection(cols, selection);
                return (false, None);
            }
            _ => {}
        }

        let (new_pos, last_find_update) = self.resolve_motion(
            key, cur_col, cur_row, last_find, emulator, pty_writer, cols, rows, viewport,
        );
        if let Some(state) = self.state.as_mut() {
            state.cursor = new_pos;
            if let Some(find) = last_find_update {
                state.last_find = Some(find);
            }
        }
        self.sync_selection(cols, selection);
        (false, None)
    }

    pub(crate) fn scroll_viewport(
        &mut self,
        delta: i32,
        emulator: &mut TerminalEmulator,
        pty_writer: &PtyInputWriter,
        cols: u16,
        rows: u16,
        viewport: &mut ProcessViewport,
    ) -> i32 {
        let mut actual = emulator.scroll_display(delta);
        if actual == 0
            && let Some((bytes, scroll_rows)) =
                Self::alt_scroll_input(emulator.mode(), delta, cols, rows)
        {
            pty_writer.write_bytes(&bytes);
            actual = scroll_rows;
        }
        if actual != 0 {
            viewport.invalidate_screen_hashes();
            if let Some(state) = self.state.as_mut() {
                state.cursor.1 = Self::offset_row(state.cursor.1, actual);
                state.anchor.1 = Self::offset_row(state.anchor.1, actual);
            }
        }
        actual
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_motion(
        &mut self,
        key: CopyModeKey,
        cur_col: u16,
        cur_row: u16,
        last_find: Option<CopyModeFind>,
        emulator: &mut TerminalEmulator,
        pty_writer: &PtyInputWriter,
        cols: u16,
        rows: u16,
        viewport: &mut ProcessViewport,
    ) -> ((u16, u16), Option<CopyModeFind>) {
        if let Some(result) =
            Self::resolve_find_key(key, cur_col, cur_row, last_find, emulator, cols)
        {
            return result;
        }
        let pos = match key {
            CopyModeKey::Left => (cur_col.saturating_sub(1), cur_row),
            CopyModeKey::Right => ((cur_col + 1).min(cols.saturating_sub(1)), cur_row),
            CopyModeKey::Up => self.move_vertically(
                cur_col, cur_row, -1, emulator, pty_writer, cols, rows, viewport,
            ),
            CopyModeKey::Down => self.move_vertically(
                cur_col, cur_row, 1, emulator, pty_writer, cols, rows, viewport,
            ),
            CopyModeKey::PageUp => self.move_vertically(
                cur_col,
                cur_row,
                -i32::from(rows / 2),
                emulator,
                pty_writer,
                cols,
                rows,
                viewport,
            ),
            CopyModeKey::PageDown => self.move_vertically(
                cur_col,
                cur_row,
                i32::from(rows / 2),
                emulator,
                pty_writer,
                cols,
                rows,
                viewport,
            ),
            CopyModeKey::LineStart => (0, cur_row),
            CopyModeKey::LineEnd => (cols.saturating_sub(1), cur_row),
            CopyModeKey::LastNonBlank => (emulator.last_non_blank_col(cur_row), cur_row),
            CopyModeKey::FirstNonBlank => (emulator.first_non_blank_col(cur_row), cur_row),
            CopyModeKey::WordForward => {
                Self::word_forward(emulator, cols, rows, cur_col, cur_row, false)
            }
            CopyModeKey::BigWordForward => {
                Self::word_forward(emulator, cols, rows, cur_col, cur_row, true)
            }
            CopyModeKey::WordBackward => {
                Self::word_backward(emulator, cols, rows, cur_col, cur_row, false)
            }
            CopyModeKey::BigWordBackward => {
                Self::word_backward(emulator, cols, rows, cur_col, cur_row, true)
            }
            CopyModeKey::WordEndForward => {
                Self::word_end_forward(emulator, cols, rows, cur_col, cur_row, false)
            }
            CopyModeKey::BigWordEndForward => {
                Self::word_end_forward(emulator, cols, rows, cur_col, cur_row, true)
            }
            CopyModeKey::WordEndBackward => {
                Self::word_end_backward(emulator, cols, rows, cur_col, cur_row, false)
            }
            CopyModeKey::BigWordEndBackward => {
                Self::word_end_backward(emulator, cols, rows, cur_col, cur_row, true)
            }
            CopyModeKey::Top | CopyModeKey::ScreenTop => (emulator.first_non_blank_col(0), 0),
            CopyModeKey::Bottom | CopyModeKey::ScreenBottom => {
                let row = rows.saturating_sub(1);
                (emulator.first_non_blank_col(row), row)
            }
            CopyModeKey::ScreenMiddle => {
                let row = rows / 2;
                (emulator.first_non_blank_col(row), row)
            }
            CopyModeKey::PrevParagraph => Self::prev_paragraph(emulator, cols, cur_col, cur_row),
            CopyModeKey::NextParagraph => {
                Self::next_paragraph(emulator, cols, rows, cur_col, cur_row)
            }
            _ => (cur_col, cur_row),
        };
        (pos, None)
    }

    fn resolve_find_key(
        key: CopyModeKey,
        cur_col: u16,
        cur_row: u16,
        last_find: Option<CopyModeFind>,
        emulator: &TerminalEmulator,
        cols: u16,
    ) -> Option<((u16, u16), Option<CopyModeFind>)> {
        match key {
            CopyModeKey::FindForward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Forward,
                    till: false,
                };
                let pos = Self::find_on_line(emulator, cols, cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row));
                Some((pos, Some(find)))
            }
            CopyModeKey::FindBackward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Backward,
                    till: false,
                };
                let pos = Self::find_on_line(emulator, cols, cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row));
                Some((pos, Some(find)))
            }
            CopyModeKey::TillForward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Forward,
                    till: true,
                };
                let pos = Self::find_on_line(emulator, cols, cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row));
                Some((pos, Some(find)))
            }
            CopyModeKey::TillBackward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Backward,
                    till: true,
                };
                let pos = Self::find_on_line(emulator, cols, cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row));
                Some((pos, Some(find)))
            }
            CopyModeKey::RepeatFind => {
                let pos = last_find
                    .and_then(|find| Self::find_on_line(emulator, cols, cur_col, cur_row, find))
                    .unwrap_or((cur_col, cur_row));
                Some((pos, None))
            }
            CopyModeKey::RepeatFindReverse => {
                let pos = last_find
                    .and_then(|find| {
                        Self::find_on_line(emulator, cols, cur_col, cur_row, find.reversed())
                    })
                    .unwrap_or((cur_col, cur_row));
                Some((pos, None))
            }
            _ => None,
        }
    }

    fn sync_selection(&self, cols: u16, selection: &mut ProcessSelection) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let (ac, ar) = state.anchor;
        let (new_col, new_row) = state.cursor;
        selection.0 = match state.visual {
            Some(CopyModeVisualMode::Character) => Some(TermSelectionRange {
                start_col: ac,
                start_row: ar,
                end_col: new_col,
                end_row: new_row,
                is_block: false,
            }),
            Some(CopyModeVisualMode::Line) => Some(TermSelectionRange {
                start_col: 0,
                start_row: ar.min(new_row),
                end_col: cols.saturating_sub(1),
                end_row: ar.max(new_row),
                is_block: false,
            }),
            None => None,
        };
    }

    #[allow(clippy::too_many_arguments)]
    fn move_vertically(
        &mut self,
        col: u16,
        row: u16,
        delta: i32,
        emulator: &mut TerminalEmulator,
        pty_writer: &PtyInputWriter,
        cols: u16,
        rows: u16,
        viewport: &mut ProcessViewport,
    ) -> (u16, u16) {
        match delta.cmp(&0) {
            std::cmp::Ordering::Less => {
                let distance = (-delta) as u16;
                if row >= distance {
                    return (col, row - distance);
                }
                let missing = distance - row;
                let scrolled = self.scroll_viewport(
                    i32::from(missing),
                    emulator,
                    pty_writer,
                    cols,
                    rows,
                    viewport,
                );
                let row = Self::offset_row(row, scrolled);
                (
                    col,
                    row.saturating_sub(distance).min(rows.saturating_sub(1)),
                )
            }
            std::cmp::Ordering::Greater => {
                let distance = delta as u16;
                let max_row = rows.saturating_sub(1);
                if row.saturating_add(distance) <= max_row {
                    return (col, row + distance);
                }
                let missing = row.saturating_add(distance).saturating_sub(max_row);
                let scrolled = self.scroll_viewport(
                    -i32::from(missing),
                    emulator,
                    pty_writer,
                    cols,
                    rows,
                    viewport,
                );
                let row = Self::offset_row(row, scrolled);
                (col, row.saturating_add(distance).min(max_row))
            }
            std::cmp::Ordering::Equal => (col, row),
        }
    }

    fn offset_row(row: u16, delta: i32) -> u16 {
        if delta >= 0 {
            row.saturating_add(delta as u16)
        } else {
            row.saturating_sub((-delta) as u16)
        }
    }

    fn alt_scroll_input(
        mode: TermMode,
        delta: i32,
        cols: u16,
        rows: u16,
    ) -> Option<(Vec<u8>, i32)> {
        if !mode.contains(TermMode::ALT_SCREEN) {
            return None;
        }
        let button = match delta.cmp(&0) {
            std::cmp::Ordering::Greater => 64,
            std::cmp::Ordering::Less => 65,
            std::cmp::Ordering::Equal => return None,
        };
        let x = cols.saturating_div(2).saturating_add(1).max(1);
        let y = rows.saturating_div(2).saturating_add(1).max(1);
        let scroll_rows = match delta.cmp(&0) {
            std::cmp::Ordering::Greater => 2,
            std::cmp::Ordering::Less => -2,
            std::cmp::Ordering::Equal => 0,
        };
        Some((format!("\x1b[<{button};{x};{y}M").into_bytes(), scroll_rows))
    }

    fn char_class(
        emulator: &TerminalEmulator,
        col: u16,
        row: u16,
        big_word: bool,
    ) -> CopyModeCharClass {
        let Some(ch) = emulator.cell_char(col, row) else {
            return CopyModeCharClass::Space;
        };
        if ch.is_whitespace() {
            CopyModeCharClass::Space
        } else if big_word || ProcessSelection::is_word_char(ch) {
            CopyModeCharClass::Word
        } else {
            CopyModeCharClass::Punct
        }
    }

    fn pos_to_index(cols: u16, col: u16, row: u16) -> usize {
        row as usize * cols as usize + col as usize
    }

    fn index_to_pos(cols: u16, rows: u16, idx: usize) -> (u16, u16) {
        let cols = cols.max(1) as usize;
        let max_idx = (rows.max(1) as usize * cols).saturating_sub(1);
        let idx = idx.min(max_idx);
        ((idx % cols) as u16, (idx / cols) as u16)
    }

    fn word_forward(
        emulator: &TerminalEmulator,
        cols: u16,
        rows: u16,
        col: u16,
        row: u16,
        big_word: bool,
    ) -> (u16, u16) {
        let max_idx = (rows.max(1) as usize * cols.max(1) as usize).saturating_sub(1);
        let mut idx = Self::pos_to_index(cols, col, row).min(max_idx);
        if idx >= max_idx {
            return Self::index_to_pos(cols, rows, idx);
        }
        let (c, r) = Self::index_to_pos(cols, rows, idx);
        let cls = Self::char_class(emulator, c, r, big_word);
        if cls != CopyModeCharClass::Space {
            while idx < max_idx {
                let (nc, nr) = Self::index_to_pos(cols, rows, idx + 1);
                if Self::char_class(emulator, nc, nr, big_word) != cls {
                    break;
                }
                idx += 1;
            }
        }
        while idx < max_idx {
            let (nc, nr) = Self::index_to_pos(cols, rows, idx + 1);
            if Self::char_class(emulator, nc, nr, big_word) != CopyModeCharClass::Space {
                idx += 1;
                break;
            }
            idx += 1;
        }
        Self::index_to_pos(cols, rows, idx)
    }

    fn word_backward(
        emulator: &TerminalEmulator,
        cols: u16,
        rows: u16,
        col: u16,
        row: u16,
        big_word: bool,
    ) -> (u16, u16) {
        let mut idx = Self::pos_to_index(cols, col, row);
        if idx == 0 {
            return Self::index_to_pos(cols, rows, 0);
        }
        idx -= 1;
        while idx > 0 {
            let (c, r) = Self::index_to_pos(cols, rows, idx);
            if Self::char_class(emulator, c, r, big_word) != CopyModeCharClass::Space {
                break;
            }
            idx -= 1;
        }
        let (c, r) = Self::index_to_pos(cols, rows, idx);
        let cls = Self::char_class(emulator, c, r, big_word);
        while idx > 0 {
            let (pc, pr) = Self::index_to_pos(cols, rows, idx - 1);
            if Self::char_class(emulator, pc, pr, big_word) != cls {
                break;
            }
            idx -= 1;
        }
        Self::index_to_pos(cols, rows, idx)
    }

    fn word_end_forward(
        emulator: &TerminalEmulator,
        cols: u16,
        rows: u16,
        col: u16,
        row: u16,
        big_word: bool,
    ) -> (u16, u16) {
        let max_idx = (rows.max(1) as usize * cols.max(1) as usize).saturating_sub(1);
        let mut idx = Self::pos_to_index(cols, col, row).min(max_idx);
        if idx >= max_idx {
            return Self::index_to_pos(cols, rows, idx);
        }
        let (c, r) = Self::index_to_pos(cols, rows, idx);
        let cls = Self::char_class(emulator, c, r, big_word);
        if cls != CopyModeCharClass::Space {
            let (nc, nr) = Self::index_to_pos(cols, rows, idx + 1);
            if Self::char_class(emulator, nc, nr, big_word) == cls {
                while idx < max_idx {
                    let (nc, nr) = Self::index_to_pos(cols, rows, idx + 1);
                    if Self::char_class(emulator, nc, nr, big_word) != cls {
                        break;
                    }
                    idx += 1;
                }
                return Self::index_to_pos(cols, rows, idx);
            }
        }
        while idx < max_idx {
            let (nc, nr) = Self::index_to_pos(cols, rows, idx + 1);
            if Self::char_class(emulator, nc, nr, big_word) != CopyModeCharClass::Space {
                idx += 1;
                break;
            }
            idx += 1;
        }
        let (c, r) = Self::index_to_pos(cols, rows, idx);
        let cls = Self::char_class(emulator, c, r, big_word);
        while idx < max_idx {
            let (nc, nr) = Self::index_to_pos(cols, rows, idx + 1);
            if Self::char_class(emulator, nc, nr, big_word) != cls {
                break;
            }
            idx += 1;
        }
        Self::index_to_pos(cols, rows, idx)
    }

    fn word_end_backward(
        emulator: &TerminalEmulator,
        cols: u16,
        rows: u16,
        col: u16,
        row: u16,
        big_word: bool,
    ) -> (u16, u16) {
        let mut idx = Self::pos_to_index(cols, col, row);
        if idx == 0 {
            return Self::index_to_pos(cols, rows, 0);
        }
        let (c, r) = Self::index_to_pos(cols, rows, idx);
        let cur_cls = Self::char_class(emulator, c, r, big_word);
        if cur_cls != CopyModeCharClass::Space {
            while idx > 0 {
                let (pc, pr) = Self::index_to_pos(cols, rows, idx - 1);
                if Self::char_class(emulator, pc, pr, big_word) != cur_cls {
                    break;
                }
                idx -= 1;
            }
        }
        if idx == 0 {
            return Self::index_to_pos(cols, rows, 0);
        }
        idx -= 1;
        while idx > 0 {
            let (c, r) = Self::index_to_pos(cols, rows, idx);
            if Self::char_class(emulator, c, r, big_word) != CopyModeCharClass::Space {
                break;
            }
            idx -= 1;
        }
        Self::index_to_pos(cols, rows, idx)
    }

    fn prev_paragraph(emulator: &TerminalEmulator, cols: u16, col: u16, row: u16) -> (u16, u16) {
        if row == 0 {
            return (col.min(cols.saturating_sub(1)), 0);
        }
        let mut target = row.saturating_sub(1);
        while target > 0 && emulator.row_is_blank(target) {
            target -= 1;
        }
        while target > 0 && !emulator.row_is_blank(target.saturating_sub(1)) {
            target -= 1;
        }
        (emulator.first_non_blank_col(target), target)
    }

    fn next_paragraph(
        emulator: &TerminalEmulator,
        cols: u16,
        rows: u16,
        col: u16,
        row: u16,
    ) -> (u16, u16) {
        let max_row = rows.saturating_sub(1);
        if row >= max_row {
            return (col.min(cols.saturating_sub(1)), max_row);
        }
        let mut target = row + 1;
        while target < max_row && emulator.row_is_blank(target) {
            target += 1;
        }
        while target < max_row && !emulator.row_is_blank(target) {
            target += 1;
        }
        while target < max_row && emulator.row_is_blank(target) {
            target += 1;
        }
        (emulator.first_non_blank_col(target), target)
    }

    fn find_on_line(
        emulator: &TerminalEmulator,
        cols: u16,
        col: u16,
        row: u16,
        find: CopyModeFind,
    ) -> Option<(u16, u16)> {
        let max_col = cols.saturating_sub(1);
        match find.direction {
            CopyModeFindDirection::Forward => {
                for target_col in col.saturating_add(1)..=max_col {
                    if emulator.cell_char(target_col, row) == Some(find.target) {
                        let col = if find.till {
                            target_col.saturating_sub(1)
                        } else {
                            target_col
                        };
                        return Some((col, row));
                    }
                }
            }
            CopyModeFindDirection::Backward => {
                for target_col in (0..col).rev() {
                    if emulator.cell_char(target_col, row) == Some(find.target) {
                        let col = if find.till {
                            (target_col + 1).min(max_col)
                        } else {
                            target_col
                        };
                        return Some((col, row));
                    }
                }
            }
        }
        None
    }
}
