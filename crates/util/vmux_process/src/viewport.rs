use std::collections::HashMap;
use std::time::{Duration, Instant};

use alacritty_terminal::term::TermMode;
use bevy::prelude::*;
use tokio::sync::broadcast;
use vmux_api::protocol::{CommandLifecycleKind, ProcessId, ServiceMessage};
use vmux_ecs::event::{
    CursorShape, MOD_ALT, MOD_CTRL, MOD_SHIFT, TermCursor, TermLine, TermSelectionRange,
    TermViewportPatch,
};
use vmux_ecs::scroll::{OVERSCAN_CAP, OVERSCAN_FLOOR, Overscan, ScrollWindow, TERMINAL_OVERSCAN_K};

use crate::emulator::TerminalEmulator;
use crate::pty::PtyInputWriter;
use crate::selection::{ProcessCopyMode, ProcessSelection};

#[derive(Clone, Debug)]
pub enum ProcessUpdate {
    Viewport(TermViewportPatch),
    Exited { exit_code: Option<i32> },
    Title(String),
    Directory { cwd: String, ready: bool },
    Bell,
    CommandLifecycle(ProcessCommandLifecycle),
    Mode(ProcessMode),
}

impl ProcessUpdate {
    pub fn into_service_message(self, process_id: ProcessId) -> ServiceMessage {
        match self {
            Self::Viewport(patch) => ServiceMessage::ViewportPatch {
                process_id,
                changed_lines: patch.changed_lines,
                cursor: patch.cursor,
                cols: patch.cols,
                rows: patch.rows,
                selection: patch.selection,
                copy_mode: patch.copy_mode,
                full: patch.full,
                first_row: patch.first_row,
                total_rows: patch.total_rows,
                alt: patch.alt,
                mouse: patch.mouse,
                evicted_total: patch.evicted_total,
            },
            Self::Exited { exit_code } => ServiceMessage::ProcessExited {
                process_id,
                exit_code,
            },
            Self::Title(title) => ServiceMessage::ProcessTitle { process_id, title },
            Self::Directory { cwd, ready } => ServiceMessage::ProcessDirectory {
                process_id,
                cwd,
                ready,
            },
            Self::Bell => ServiceMessage::Bell { process_id },
            Self::CommandLifecycle(lifecycle) => ServiceMessage::CommandLifecycle {
                process_id,
                kind: match lifecycle {
                    ProcessCommandLifecycle::Started => CommandLifecycleKind::Started,
                    ProcessCommandLifecycle::Ended { exit_code } => {
                        CommandLifecycleKind::Ended { exit_code }
                    }
                },
            },
            Self::Mode(mode) => ServiceMessage::TerminalMode {
                process_id,
                mouse_capture: mode.mouse_capture,
                copy_mode: mode.copy_mode,
                alt_screen: mode.alt_screen,
                focus_reporting: mode.focus_reporting,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessMode {
    pub mouse_capture: bool,
    pub copy_mode: bool,
    pub alt_screen: bool,
    pub focus_reporting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessCommandLifecycle {
    Started,
    Ended { exit_code: Option<i32> },
}

#[derive(Clone, Debug)]
pub struct ProcessSnapshot {
    pub lines: Vec<TermLine>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
}

impl ProcessSnapshot {
    pub fn into_service_message(self, process_id: ProcessId) -> ServiceMessage {
        ServiceMessage::Snapshot {
            process_id,
            lines: self.lines,
            cursor: self.cursor,
            cols: self.cols,
            rows: self.rows,
        }
    }
}

#[derive(Component, Default)]
pub(crate) struct ProcessTerminalMode(Option<ProcessMode>);

impl ProcessTerminalMode {
    pub(crate) fn maybe_broadcast(
        &mut self,
        mode: TermMode,
        copy_mode: bool,
        updates: &broadcast::Sender<ProcessUpdate>,
    ) {
        let current = ProcessMode {
            mouse_capture: mode.intersects(TermMode::MOUSE_MODE),
            copy_mode,
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            focus_reporting: mode.contains(TermMode::FOCUS_IN_OUT),
        };
        if self.0 != Some(current) {
            self.0 = Some(current);
            let _ = updates.send(ProcessUpdate::Mode(current));
        }
    }
}

#[derive(Default)]
pub(crate) struct ViewportPace {
    heavy: bool,
    sent_at: Option<Instant>,
    keystroke_at: Option<Instant>,
}

impl ViewportPace {
    pub(crate) const HEAVY_OUTPUT_FRAME_INTERVAL: Duration = Duration::from_millis(100);

    pub(crate) fn admits(&mut self, now: Instant, flush: bool, keystroke: bool) -> bool {
        let escapes = keystroke && Self::interval_passed(self.keystroke_at, now);
        if keystroke {
            self.keystroke_at = Some(now);
        }
        flush || escapes || !self.heavy || Self::interval_passed(self.sent_at, now)
    }

    pub(crate) fn sent(&mut self, now: Instant, changed_rows: usize, rows: u16) {
        self.sent_at = Some(now);
        self.heavy = changed_rows.saturating_mul(2) >= rows as usize;
    }

    fn interval_passed(since: Option<Instant>, now: Instant) -> bool {
        since.is_none_or(|since| {
            now.saturating_duration_since(since) >= Self::HEAVY_OUTPUT_FRAME_INTERVAL
        })
    }
}

#[derive(Component)]
pub(crate) struct ProcessViewport {
    line_hashes: Vec<u64>,
    win_hashes: HashMap<u32, u64>,
    view_top: u32,
    following: bool,
    last_win: Option<(u32, u32)>,
    last_passthrough: bool,
    last_cursor: Option<(u16, u16)>,
    last_selection: Option<TermSelectionRange>,
    last_viewport_copy_mode: Option<bool>,
    output_dirty: bool,
    pace: ViewportPace,
}

impl Default for ProcessViewport {
    fn default() -> Self {
        Self {
            line_hashes: Vec::new(),
            win_hashes: HashMap::new(),
            view_top: 0,
            following: true,
            last_win: None,
            last_passthrough: false,
            last_cursor: None,
            last_selection: None,
            last_viewport_copy_mode: None,
            output_dirty: false,
            pace: ViewportPace::default(),
        }
    }
}

impl ProcessViewport {
    pub(crate) fn sgr_mouse_wheel_bytes(up: bool, col: u16, row: u16, modifiers: u8) -> Vec<u8> {
        let mut cb: u32 = if up { 64 } else { 65 };
        if modifiers & MOD_SHIFT != 0 {
            cb += 4;
        }
        if modifiers & MOD_ALT != 0 {
            cb += 8;
        }
        if modifiers & MOD_CTRL != 0 {
            cb += 16;
        }
        format!("\x1b[<{cb};{};{}M", col + 1, row + 1).into_bytes()
    }

    pub(crate) fn alternate_scroll_bytes(up: bool, app_cursor: bool) -> &'static [u8] {
        match (up, app_cursor) {
            (true, false) => b"\x1b[A",
            (false, false) => b"\x1b[B",
            (true, true) => b"\x1bOA",
            (false, true) => b"\x1bOB",
        }
    }

    pub(crate) fn invalidate_on_resize(&mut self) {
        self.line_hashes.clear();
        self.win_hashes.clear();
        self.last_win = None;
    }

    pub(crate) fn invalidate_screen_hashes(&mut self) {
        self.line_hashes.clear();
    }

    pub(crate) fn scroll_window(&mut self, top_row: u32, follow: bool) {
        self.following = follow;
        self.view_top = top_row;
        self.win_hashes.clear();
        self.last_win = None;
    }

    pub(crate) fn mark_output_dirty(&mut self) {
        self.output_dirty = true;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn maybe_sync_output(
        &mut self,
        rows: u16,
        flush: bool,
        got_data: bool,
        pty_writer: &PtyInputWriter,
        emulator: &TerminalEmulator,
        selection: &mut ProcessSelection,
        copy_mode: &ProcessCopyMode,
        updates: &broadcast::Sender<ProcessUpdate>,
    ) {
        if !self.output_dirty {
            return;
        }
        let now = Instant::now();
        let keystroke = pty_writer.take_keystroke(got_data);
        if !self.pace.admits(now, flush, keystroke) {
            return;
        }
        let changed_rows = self.sync(emulator, selection, copy_mode, updates);
        self.output_dirty = false;
        self.pace.sent(now, changed_rows, rows);
    }

    pub(crate) fn sync(
        &mut self,
        emulator: &TerminalEmulator,
        selection: &mut ProcessSelection,
        copy_mode: &ProcessCopyMode,
        updates: &broadcast::Sender<ProcessUpdate>,
    ) -> usize {
        let mode = emulator.mode();
        let passthrough = copy_mode.is_active()
            || mode.contains(TermMode::ALT_SCREEN)
            || mode.intersects(TermMode::MOUSE_MODE);
        if passthrough != self.last_passthrough {
            self.line_hashes.clear();
            self.win_hashes.clear();
            self.last_win = None;
            self.last_passthrough = passthrough;
        }
        if passthrough {
            self.sync_screen_relative(emulator, selection, copy_mode, updates)
        } else {
            self.sync_document_window(emulator, selection, updates)
        }
    }

    fn sync_screen_relative(
        &mut self,
        emulator: &TerminalEmulator,
        selection: &mut ProcessSelection,
        copy_mode: &ProcessCopyMode,
        updates: &broadcast::Sender<ProcessUpdate>,
    ) -> usize {
        let (num_cols, num_lines, _) = emulator.dimensions();
        let offset = emulator.display_offset();
        let mode = emulator.mode();
        let alt = mode.contains(TermMode::ALT_SCREEN);
        let mouse = mode.intersects(TermMode::MOUSE_MODE);

        let full = self.line_hashes.len() != num_lines;
        if self.line_hashes.len() != num_lines {
            self.line_hashes.resize(num_lines, 0);
        }
        let mut changed_lines = Vec::new();
        for row_idx in 0..num_lines {
            let hash = emulator.row_hash(row_idx, offset);
            if full || hash != self.line_hashes[row_idx] {
                self.line_hashes[row_idx] = hash;
                changed_lines.push((row_idx as u32, emulator.row_line(row_idx, offset)));
            }
        }
        selection.clear_on_changed_rows(copy_mode.is_active(), full, &changed_lines);

        let copy_cursor = copy_mode.cursor();
        let cursor_pos = copy_cursor.unwrap_or_else(|| emulator.cursor_point());
        let cursor_moved = self.last_cursor != Some(cursor_pos);
        let selection_changed = selection.0 != self.last_selection;
        let in_copy_mode = copy_mode.is_active();
        let copy_mode_changed = self.last_viewport_copy_mode != Some(in_copy_mode);
        if changed_lines.is_empty()
            && !full
            && !cursor_moved
            && !selection_changed
            && !copy_mode_changed
        {
            return 0;
        }

        let changed_rows = changed_lines.len();
        self.last_cursor = Some(cursor_pos);
        self.last_selection = selection.0;
        self.last_viewport_copy_mode = Some(in_copy_mode);
        let patch = TermViewportPatch {
            changed_lines,
            cursor: TermCursor {
                col: cursor_pos.0,
                row: cursor_pos.1 as u32,
                shape: CursorShape::Block,
                visible: copy_cursor.is_some() || offset == 0,
                ch: emulator.cursor_char(copy_cursor),
            },
            cols: num_cols as u16,
            rows: num_lines as u16,
            selection: selection.0,
            copy_mode: in_copy_mode,
            full,
            first_row: 0,
            total_rows: num_lines as u32,
            alt,
            mouse,
            evicted_total: 0,
        };
        let _ = updates.send(ProcessUpdate::Viewport(patch));
        changed_rows
    }

    fn sync_document_window(
        &mut self,
        emulator: &TerminalEmulator,
        selection: &ProcessSelection,
        updates: &broadcast::Sender<ProcessUpdate>,
    ) -> usize {
        let (num_cols, screen, total_rows) = emulator.dimensions();
        let history = total_rows.saturating_sub(screen as u32);
        let visible = screen as u16;
        let overscan = if self.following {
            0
        } else {
            Overscan::new(visible, TERMINAL_OVERSCAN_K, OVERSCAN_FLOOR, OVERSCAN_CAP).rows()
        };
        let view_top = if self.following {
            history
        } else {
            ScrollWindow::new(total_rows, self.view_top, visible).top()
        };
        let first_row = view_top.saturating_sub(overscan);
        let end_row = (view_top + visible as u32 + overscan).min(total_rows);
        let offset = history as i32;

        let mut changed_lines = Vec::new();
        let mut live = HashMap::new();
        for doc_row in first_row..end_row {
            let hash = emulator.row_hash(doc_row as usize, offset);
            live.insert(doc_row, hash);
            if self.win_hashes.get(&doc_row) != Some(&hash) {
                changed_lines.push((doc_row, emulator.row_line(doc_row as usize, offset)));
            }
        }
        let full = self.win_hashes.is_empty();
        self.win_hashes = live;

        let (cursor_col, cursor_doc_row, cursor_char) = emulator.document_cursor(history);
        let cursor_in_window = cursor_doc_row >= first_row && cursor_doc_row < end_row;
        let cursor_key = (cursor_col, cursor_doc_row as u16);
        let cursor_moved = self.last_cursor != Some(cursor_key);
        let selection_changed = selection.0 != self.last_selection;
        let win = (first_row, total_rows);
        let win_changed = self.last_win != Some(win);
        if changed_lines.is_empty() && !full && !cursor_moved && !selection_changed && !win_changed
        {
            return 0;
        }

        let changed_rows = changed_lines.len();
        self.last_cursor = Some(cursor_key);
        self.last_selection = selection.0;
        self.last_win = Some(win);
        self.last_viewport_copy_mode = Some(false);
        let patch = TermViewportPatch {
            changed_lines,
            cursor: TermCursor {
                col: cursor_col,
                row: cursor_doc_row,
                shape: CursorShape::Block,
                visible: cursor_in_window,
                ch: cursor_char,
            },
            cols: num_cols as u16,
            rows: visible,
            selection: selection.0,
            copy_mode: false,
            full,
            first_row,
            total_rows,
            alt: false,
            mouse: false,
            evicted_total: 0,
        };
        let _ = updates.send(ProcessUpdate::Viewport(patch));
        changed_rows
    }
}
