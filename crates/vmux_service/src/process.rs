use crate::protocol::{ProcessId, ProcessInfo, ServiceMessage};
use alacritty_terminal::{
    event::{Event as TermEvent, EventListener as TermEventListener},
    grid::Dimensions,
    index::{Column, Line},
    term::{Config as TermConfig, Term, cell::Flags as CellFlags},
    vte::ansi::{Color, NamedColor, Processor},
};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::{broadcast, mpsc};
use vmux_terminal::event::*;

const MAX_PTY_CHUNKS_PER_POLL: usize = 64;
const _: () = assert!(MAX_PTY_CHUNKS_PER_POLL <= 256);

pub type PtyInputWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Clone)]
struct ServiceEventProxy {
    pty_writer: PtyInputWriter,
}

impl TermEventListener for ServiceEventProxy {
    fn send_event(&self, event: TermEvent) {
        if let TermEvent::PtyWrite(text) = event
            && let Ok(mut writer) = self.pty_writer.lock()
        {
            let _ = writer.write_all(text.as_bytes());
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

/// A single terminal process managed by the service.
pub struct Process {
    pub id: ProcessId,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub created_at: Instant,
    term: Term<ServiceEventProxy>,
    processor: Processor,
    pty_writer: PtyInputWriter,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pty_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Broadcasts viewport patches to all attached GUI clients.
    patch_tx: broadcast::Sender<ServiceMessage>,
    line_hashes: Vec<u64>,
    /// Last broadcast cursor position (col, row). Used to broadcast a patch
    /// when only the cursor moves (e.g. typing space over already-blank
    /// cells produces no line-content change but the screen cursor must
    /// still update).
    last_cursor: Option<(u16, u16)>,
    /// Currently selected range (in viewport coords). None when no selection.
    selection: Option<TermSelectionRange>,
    /// Last broadcast selection (used for change detection).
    last_selection: Option<TermSelectionRange>,
    /// Last copy-mode value emitted in a viewport patch.
    last_viewport_copy_mode: Option<bool>,
    /// Last broadcast (mouse_capture, copy_mode) flags.
    last_terminal_mode: Option<(bool, bool)>,
    /// Active copy-mode state (cursor, anchor, visual state). None when not in copy mode.
    copy_mode: Option<CopyModeState>,
}

/// Per-process state held while the user is in copy mode.
struct CopyModeState {
    /// Cursor position in viewport coords (col, row).
    cursor: (u16, u16),
    /// Selection anchor in viewport coords. Movement extends selection from
    /// this anchor to the copy-mode cursor.
    anchor: (u16, u16),
    /// Active visual mode. None means the cursor moves without selecting.
    visual: Option<CopyModeVisualMode>,
    /// Last f/F/t/T search for ; and ,.
    last_find: Option<CopyModeFind>,
}

#[derive(Clone, Copy)]
enum CopyModeVisualMode {
    Character,
    Line,
}

#[derive(Clone, Copy)]
struct CopyModeFind {
    target: char,
    direction: CopyModeFindDirection,
    till: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyModeFindDirection {
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
enum CopyModeCharClass {
    Space,
    Word,
    Punct,
}

/// Word-character class for double-click word selection and tmux-style
/// copy-mode word motions. A "word" is a maximal run of these characters.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-')
}

impl Process {
    pub fn new(
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        let (wake_tx, _) = mpsc::unbounded_channel();
        Self::new_with_wake(shell, cwd, env, cols, rows, wake_tx)
    }

    pub fn new_with_wake(
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
        wake_tx: mpsc::UnboundedSender<ProcessId>,
    ) -> Result<Self, String> {
        let id = ProcessId::new();
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open PTY: {e}"))?;

        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_CTYPE", "UTF-8");
        for (k, v) in &env {
            cmd.env(k, v);
        }
        let cwd_path = PathBuf::from(&cwd);
        if !cwd.is_empty() && cwd_path.exists() {
            cmd.cwd(&cwd_path);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn shell: {e}"))?;
        let pid = child.process_id().unwrap_or(0);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone PTY reader: {e}"))?;
        let writer: PtyInputWriter = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .map_err(|e| format!("failed to take PTY writer: {e}"))?,
        ));
        drop(pair.slave);

        if !cwd.is_empty()
            && cwd_path.exists()
            && let Ok(mut w) = writer.lock()
        {
            let cd_cmd = format!("cd {}\n", cwd);
            let _ = w.write_all(cd_cmd.as_bytes());
            let _ = w.flush();
        }

        let (pty_tx, pty_rx) = mpsc::unbounded_channel();
        let wake_process_id = id;
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", &id.to_string()[..8]))
            .spawn(move || {
                let mut buf = [0u8; 4096];
                let mut reader = reader;
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if pty_tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                            let _ = wake_tx.send(wake_process_id);
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("failed to spawn PTY reader: {e}"))?;

        let event_proxy = ServiceEventProxy {
            pty_writer: Arc::clone(&writer),
        };
        let dims = PtyDimensions { cols, rows };
        let term = Term::new(TermConfig::default(), &dims, event_proxy);
        let (patch_tx, _) = broadcast::channel(256);

        Ok(Self {
            id,
            shell,
            cwd,
            cols,
            rows,
            pid,
            created_at: Instant::now(),
            term,
            processor: Processor::new(),
            pty_writer: writer,
            master: pair.master,
            child,
            pty_rx,
            patch_tx,
            line_hashes: Vec::new(),
            last_cursor: None,
            selection: None,
            last_selection: None,
            last_viewport_copy_mode: None,
            last_terminal_mode: None,
            copy_mode: None,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServiceMessage> {
        self.patch_tx.subscribe()
    }

    pub fn write_input(&self, data: &[u8]) {
        Self::write_input_to_writer(&self.pty_writer, data);
    }

    pub fn input_writer(&self) -> PtyInputWriter {
        Arc::clone(&self.pty_writer)
    }

    pub fn write_input_to_writer(writer: &PtyInputWriter, data: &[u8]) {
        if let Ok(mut w) = writer.lock() {
            let _ = w.write_all(data);
        }
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn process_output_for_test(&mut self, data: &[u8]) {
        self.processor.advance(&mut self.term, data);
        if self.copy_mode.is_none() && self.selection.is_some() {
            self.selection = None;
        }
        self.sync_viewport();
    }

    /// Replace the selection. None clears it. Range is clamped to the
    /// current viewport dimensions to defend against stale or buggy clients.
    pub fn set_selection(&mut self, range: Option<TermSelectionRange>) {
        self.selection = range.map(|r| self.clamp_range(r));
        self.sync_viewport();
    }

    /// Extend the current selection's end point to (col, row). If no
    /// selection exists, anchor at (col, row). Coordinates are clamped to
    /// the current viewport.
    pub fn extend_selection_to(&mut self, col: u16, row: u16) {
        let (col, row) = self.clamp_point(col, row);
        let range = match self.selection.take() {
            Some(mut r) => {
                r.end_col = col;
                r.end_row = row;
                r
            }
            None => TermSelectionRange {
                start_col: col,
                start_row: row,
                end_col: col,
                end_row: row,
                is_block: false,
            },
        };
        self.selection = Some(range);
        self.sync_viewport();
    }

    fn clamp_point(&self, col: u16, row: u16) -> (u16, u16) {
        (
            col.min(self.cols.saturating_sub(1)),
            row.min(self.rows.saturating_sub(1)),
        )
    }

    fn clamp_range(&self, mut r: TermSelectionRange) -> TermSelectionRange {
        let max_col = self.cols.saturating_sub(1);
        let max_row = self.rows.saturating_sub(1);
        r.start_col = r.start_col.min(max_col);
        r.end_col = r.end_col.min(max_col);
        r.start_row = r.start_row.min(max_row);
        r.end_row = r.end_row.min(max_row);
        r
    }

    /// Select the word at (col, row). A "word" is a maximal run of
    /// `[A-Za-z0-9_./-]` characters.
    pub fn select_word_at(&mut self, col: u16, row: u16) {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        if (row as usize) >= num_lines || (col as usize) >= num_cols {
            return;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        if !is_word_char(line[Column(col as usize)].c) {
            self.selection = Some(TermSelectionRange {
                start_col: col,
                start_row: row,
                end_col: col,
                end_row: row,
                is_block: false,
            });
            self.sync_viewport();
            return;
        }
        let mut start = col as usize;
        let mut end = col as usize;
        while start > 0 && is_word_char(line[Column(start - 1)].c) {
            start -= 1;
        }
        while end + 1 < num_cols && is_word_char(line[Column(end + 1)].c) {
            end += 1;
        }
        self.selection = Some(TermSelectionRange {
            start_col: start as u16,
            start_row: row,
            end_col: end as u16,
            end_row: row,
            is_block: false,
        });
        self.sync_viewport();
    }

    /// Select the entire row from col 0 to the last non-blank cell.
    pub fn select_line_at(&mut self, row: u16) {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        if (row as usize) >= num_lines {
            return;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        let mut end = 0usize;
        for c in 0..num_cols {
            if !line[Column(c)].c.is_whitespace() {
                end = c;
            }
        }
        self.selection = Some(TermSelectionRange {
            start_col: 0,
            start_row: row,
            end_col: end as u16,
            end_row: row,
            is_block: false,
        });
        self.sync_viewport();
    }

    /// Extract selected text. Lines joined by `\n`, trailing spaces stripped per line.
    pub fn selection_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        let offset = grid.display_offset() as i32;

        // Normalize so (start_row, start_col) <= (end_row, end_col) row-major.
        let (sr, sc, er, ec) = if (sel.start_row, sel.start_col) <= (sel.end_row, sel.end_col) {
            (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
        } else {
            (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
        };

        let max_col = num_cols.saturating_sub(1);
        // Block selections require per-axis min/max independently of row order.
        let (block_lo, block_hi) = if sel.is_block {
            (
                sel.start_col.min(sel.end_col) as usize,
                sel.start_col.max(sel.end_col) as usize,
            )
        } else {
            (0, 0)
        };
        let mut lines: Vec<String> = Vec::new();
        for row_idx in sr..=er {
            if (row_idx as usize) >= num_lines {
                break;
            }
            let line = &grid[Line(row_idx as i32 - offset)];
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
            let mut s = String::new();
            for c in lo..=hi {
                s.push(line[Column(c)].c);
            }
            while s.ends_with(' ') {
                s.pop();
            }
            lines.push(s);
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    /// Broadcast TerminalMode whenever mouse-capture or copy-mode changes.
    fn maybe_broadcast_mode(&mut self) {
        use alacritty_terminal::term::TermMode;
        let mouse_capture = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let copy_mode = self.copy_mode.is_some();
        let cur = (mouse_capture, copy_mode);
        if self.last_terminal_mode != Some(cur) {
            self.last_terminal_mode = Some(cur);
            let _ = self.patch_tx.send(ServiceMessage::TerminalMode {
                process_id: self.id,
                mouse_capture,
                copy_mode,
            });
        }
    }

    pub fn is_copy_mode(&self) -> bool {
        self.copy_mode.is_some()
    }

    pub fn enter_copy_mode(&mut self) {
        let grid = self.term.grid();
        // Place the copy-mode cursor at the real PTY cursor, but clamped to
        // the visible viewport. If the user has scrolled back, the PTY
        // cursor sits off-screen; clamp to the bottom-most visible row so
        // the first arrow keystroke moves a visible cursor.
        let max_col = self.cols.saturating_sub(1);
        let max_row = self.rows.saturating_sub(1);
        let cursor = (
            (grid.cursor.point.column.0 as u16).min(max_col),
            (grid.cursor.point.line.0 as u16).min(max_row),
        );
        self.copy_mode = Some(CopyModeState {
            cursor,
            anchor: cursor,
            visual: None,
            last_find: None,
        });
        self.selection = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    pub fn exit_copy_mode(&mut self) {
        self.copy_mode = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    pub fn cancel_copy_mode(&mut self) {
        self.copy_mode = None;
        self.selection = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    /// Returns Some(text) if the key triggered a Copy action.
    pub fn copy_mode_key(&mut self, key: crate::protocol::CopyModeKey) -> Option<String> {
        use crate::protocol::CopyModeKey as K;
        let cols = self.cols;
        let rows = self.rows;
        let (cur_col, cur_row, last_find) = {
            let cm = self.copy_mode.as_ref()?;
            (cm.cursor.0, cm.cursor.1, cm.last_find)
        };

        match key {
            K::Exit => {
                self.cancel_copy_mode();
                return None;
            }
            K::Copy => {
                let text = self.selection_text();
                self.cancel_copy_mode();
                return text;
            }
            K::StartSelection => {
                if let Some(cm) = self.copy_mode.as_mut() {
                    cm.anchor = cm.cursor;
                    cm.visual = Some(CopyModeVisualMode::Character);
                }
                self.update_copy_mode_selection();
                self.sync_viewport();
                return None;
            }
            K::StartLineSelection => {
                if let Some(cm) = self.copy_mode.as_mut() {
                    cm.anchor = cm.cursor;
                    cm.visual = Some(CopyModeVisualMode::Line);
                }
                self.update_copy_mode_selection();
                self.sync_viewport();
                return None;
            }
            K::SwapSelectionEnds => {
                if let Some(cm) = self.copy_mode.as_mut()
                    && cm.visual.is_some()
                {
                    std::mem::swap(&mut cm.cursor, &mut cm.anchor);
                }
                self.update_copy_mode_selection();
                self.sync_viewport();
                return None;
            }
            _ => {}
        }

        let mut last_find_update = None;
        let (new_col, new_row) = match key {
            K::Left => (cur_col.saturating_sub(1), cur_row),
            K::Right => ((cur_col + 1).min(cols.saturating_sub(1)), cur_row),
            K::Up => (cur_col, cur_row.saturating_sub(1)),
            K::Down => (cur_col, (cur_row + 1).min(rows.saturating_sub(1))),
            K::LineStart => (0, cur_row),
            K::LineEnd => (cols.saturating_sub(1), cur_row),
            K::LastNonBlank => (self.last_non_blank_col(cur_row), cur_row),
            K::FirstNonBlank => (self.first_non_blank_col(cur_row), cur_row),
            K::WordForward => self.word_forward(cur_col, cur_row, false),
            K::BigWordForward => self.word_forward(cur_col, cur_row, true),
            K::WordBackward => self.word_backward(cur_col, cur_row, false),
            K::BigWordBackward => self.word_backward(cur_col, cur_row, true),
            K::WordEndForward => self.word_end_forward(cur_col, cur_row, false),
            K::BigWordEndForward => self.word_end_forward(cur_col, cur_row, true),
            K::WordEndBackward => self.word_end_backward(cur_col, cur_row, false),
            K::BigWordEndBackward => self.word_end_backward(cur_col, cur_row, true),
            K::Top | K::ScreenTop => (self.first_non_blank_col(0), 0),
            K::Bottom | K::ScreenBottom => {
                let row = rows.saturating_sub(1);
                (self.first_non_blank_col(row), row)
            }
            K::ScreenMiddle => {
                let row = rows / 2;
                (self.first_non_blank_col(row), row)
            }
            K::PrevParagraph => self.prev_paragraph(cur_col, cur_row),
            K::NextParagraph => self.next_paragraph(cur_col, cur_row),
            K::FindForward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Forward,
                    till: false,
                };
                last_find_update = Some(find);
                self.find_on_line(cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row))
            }
            K::FindBackward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Backward,
                    till: false,
                };
                last_find_update = Some(find);
                self.find_on_line(cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row))
            }
            K::TillForward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Forward,
                    till: true,
                };
                last_find_update = Some(find);
                self.find_on_line(cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row))
            }
            K::TillBackward(target) => {
                let find = CopyModeFind {
                    target,
                    direction: CopyModeFindDirection::Backward,
                    till: true,
                };
                last_find_update = Some(find);
                self.find_on_line(cur_col, cur_row, find)
                    .unwrap_or((cur_col, cur_row))
            }
            K::RepeatFind => last_find
                .and_then(|find| self.find_on_line(cur_col, cur_row, find))
                .unwrap_or((cur_col, cur_row)),
            K::RepeatFindReverse => last_find
                .and_then(|find| self.find_on_line(cur_col, cur_row, find.reversed()))
                .unwrap_or((cur_col, cur_row)),
            K::PageUp => (cur_col, cur_row.saturating_sub(rows / 2)),
            K::PageDown => (cur_col, (cur_row + rows / 2).min(rows.saturating_sub(1))),
            K::StartSelection
            | K::StartLineSelection
            | K::SwapSelectionEnds
            | K::Exit
            | K::Copy => {
                unreachable!("handled before movement")
            }
        };

        if let Some(cm) = self.copy_mode.as_mut() {
            cm.cursor = (new_col, new_row);
            if let Some(find) = last_find_update {
                cm.last_find = Some(find);
            }
        }
        self.update_copy_mode_selection();
        self.sync_viewport();
        None
    }

    fn update_copy_mode_selection(&mut self) {
        let Some(cm) = self.copy_mode.as_ref() else {
            return;
        };
        let (ac, ar) = cm.anchor;
        let (new_col, new_row) = cm.cursor;
        self.selection = match cm.visual {
            Some(CopyModeVisualMode::Character) => Some(TermSelectionRange {
                start_col: ac,
                start_row: ar,
                end_col: new_col,
                end_row: new_row,
                is_block: false,
            }),
            Some(CopyModeVisualMode::Line) => {
                let start_row = ar.min(new_row);
                let end_row = ar.max(new_row);
                Some(TermSelectionRange {
                    start_col: 0,
                    start_row,
                    end_col: self.cols.saturating_sub(1),
                    end_row,
                    is_block: false,
                })
            }
            None => None,
        };
    }

    fn last_non_blank_col(&self, row: u16) -> u16 {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        if (row as usize) >= num_lines {
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

    fn first_non_blank_col(&self, row: u16) -> u16 {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        if (row as usize) >= num_lines {
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

    fn cell_char(&self, col: u16, row: u16) -> Option<char> {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        if (row as usize) >= num_lines || (col as usize) >= num_cols {
            return None;
        }
        let offset = grid.display_offset() as i32;
        Some(grid[Line(row as i32 - offset)][Column(col as usize)].c)
    }

    fn char_class(&self, col: u16, row: u16, big_word: bool) -> CopyModeCharClass {
        let Some(c) = self.cell_char(col, row) else {
            return CopyModeCharClass::Space;
        };
        if c.is_whitespace() {
            CopyModeCharClass::Space
        } else if big_word || is_word_char(c) {
            CopyModeCharClass::Word
        } else {
            CopyModeCharClass::Punct
        }
    }

    fn pos_to_index(&self, col: u16, row: u16) -> usize {
        row as usize * self.cols as usize + col as usize
    }

    fn index_to_pos(&self, idx: usize) -> (u16, u16) {
        let cols = self.cols.max(1) as usize;
        let max_idx = (self.rows.max(1) as usize * cols).saturating_sub(1);
        let idx = idx.min(max_idx);
        ((idx % cols) as u16, (idx / cols) as u16)
    }

    fn word_forward(&self, col: u16, row: u16, big_word: bool) -> (u16, u16) {
        let max_idx = (self.rows.max(1) as usize * self.cols.max(1) as usize).saturating_sub(1);
        let mut idx = self.pos_to_index(col, row).min(max_idx);
        if idx >= max_idx {
            return self.index_to_pos(idx);
        }

        let (c, r) = self.index_to_pos(idx);
        let cls = self.char_class(c, r, big_word);
        if cls != CopyModeCharClass::Space {
            while idx < max_idx {
                let (nc, nr) = self.index_to_pos(idx + 1);
                if self.char_class(nc, nr, big_word) != cls {
                    break;
                }
                idx += 1;
            }
        }
        while idx < max_idx {
            let (nc, nr) = self.index_to_pos(idx + 1);
            if self.char_class(nc, nr, big_word) != CopyModeCharClass::Space {
                idx += 1;
                break;
            }
            idx += 1;
        }
        self.index_to_pos(idx)
    }

    fn word_backward(&self, col: u16, row: u16, big_word: bool) -> (u16, u16) {
        let mut idx = self.pos_to_index(col, row);
        if idx == 0 {
            return self.index_to_pos(0);
        }
        idx -= 1;
        while idx > 0 {
            let (c, r) = self.index_to_pos(idx);
            if self.char_class(c, r, big_word) != CopyModeCharClass::Space {
                break;
            }
            idx -= 1;
        }
        let (c, r) = self.index_to_pos(idx);
        let cls = self.char_class(c, r, big_word);
        while idx > 0 {
            let (pc, pr) = self.index_to_pos(idx - 1);
            if self.char_class(pc, pr, big_word) != cls {
                break;
            }
            idx -= 1;
        }
        self.index_to_pos(idx)
    }

    fn word_end_forward(&self, col: u16, row: u16, big_word: bool) -> (u16, u16) {
        let max_idx = (self.rows.max(1) as usize * self.cols.max(1) as usize).saturating_sub(1);
        let mut idx = self.pos_to_index(col, row).min(max_idx);
        if idx >= max_idx {
            return self.index_to_pos(idx);
        }

        let (c, r) = self.index_to_pos(idx);
        let cls = self.char_class(c, r, big_word);
        if cls != CopyModeCharClass::Space {
            let (nc, nr) = self.index_to_pos(idx + 1);
            if self.char_class(nc, nr, big_word) == cls {
                while idx < max_idx {
                    let (nc, nr) = self.index_to_pos(idx + 1);
                    if self.char_class(nc, nr, big_word) != cls {
                        break;
                    }
                    idx += 1;
                }
                return self.index_to_pos(idx);
            }
        }

        while idx < max_idx {
            let (nc, nr) = self.index_to_pos(idx + 1);
            if self.char_class(nc, nr, big_word) != CopyModeCharClass::Space {
                idx += 1;
                break;
            }
            idx += 1;
        }
        let (c, r) = self.index_to_pos(idx);
        let cls = self.char_class(c, r, big_word);
        while idx < max_idx {
            let (nc, nr) = self.index_to_pos(idx + 1);
            if self.char_class(nc, nr, big_word) != cls {
                break;
            }
            idx += 1;
        }
        self.index_to_pos(idx)
    }

    fn word_end_backward(&self, col: u16, row: u16, big_word: bool) -> (u16, u16) {
        let mut idx = self.pos_to_index(col, row);
        if idx == 0 {
            return self.index_to_pos(0);
        }

        let (c, r) = self.index_to_pos(idx);
        let cur_cls = self.char_class(c, r, big_word);
        if cur_cls != CopyModeCharClass::Space {
            while idx > 0 {
                let (pc, pr) = self.index_to_pos(idx - 1);
                if self.char_class(pc, pr, big_word) != cur_cls {
                    break;
                }
                idx -= 1;
            }
        }
        if idx == 0 {
            return self.index_to_pos(0);
        }
        idx -= 1;
        while idx > 0 {
            let (c, r) = self.index_to_pos(idx);
            if self.char_class(c, r, big_word) != CopyModeCharClass::Space {
                break;
            }
            idx -= 1;
        }
        self.index_to_pos(idx)
    }

    fn row_is_blank(&self, row: u16) -> bool {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let num_lines = grid.screen_lines();
        if (row as usize) >= num_lines {
            return true;
        }
        let offset = grid.display_offset() as i32;
        let line = &grid[Line(row as i32 - offset)];
        (0..num_cols).all(|col| line[Column(col)].c.is_whitespace())
    }

    fn prev_paragraph(&self, col: u16, row: u16) -> (u16, u16) {
        if row == 0 {
            return (col.min(self.cols.saturating_sub(1)), 0);
        }
        let mut r = row.saturating_sub(1);
        while r > 0 && self.row_is_blank(r) {
            r -= 1;
        }
        while r > 0 && !self.row_is_blank(r.saturating_sub(1)) {
            r -= 1;
        }
        (self.first_non_blank_col(r), r)
    }

    fn next_paragraph(&self, col: u16, row: u16) -> (u16, u16) {
        let max_row = self.rows.saturating_sub(1);
        if row >= max_row {
            return (col.min(self.cols.saturating_sub(1)), max_row);
        }
        let mut r = row + 1;
        while r < max_row && self.row_is_blank(r) {
            r += 1;
        }
        while r < max_row && !self.row_is_blank(r) {
            r += 1;
        }
        while r < max_row && self.row_is_blank(r) {
            r += 1;
        }
        (self.first_non_blank_col(r), r)
    }

    fn find_on_line(&self, col: u16, row: u16, find: CopyModeFind) -> Option<(u16, u16)> {
        let max_col = self.cols.saturating_sub(1);
        match find.direction {
            CopyModeFindDirection::Forward => {
                for target_col in col.saturating_add(1)..=max_col {
                    if self.cell_char(target_col, row) == Some(find.target) {
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
                    if self.cell_char(target_col, row) == Some(find.target) {
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

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        let dims = PtyDimensions { cols, rows };
        self.term.resize(dims);
        self.line_hashes.clear();
        // Clamp copy-mode cursor + anchor to the new bounds.
        if let Some(cm) = self.copy_mode.as_mut() {
            let max_col = cols.saturating_sub(1);
            let max_row = rows.saturating_sub(1);
            cm.cursor.0 = cm.cursor.0.min(max_col);
            cm.cursor.1 = cm.cursor.1.min(max_row);
            cm.anchor.0 = cm.anchor.0.min(max_col);
            cm.anchor.1 = cm.anchor.1.min(max_row);
        }
        if let Some(r) = self.selection.take() {
            self.selection = Some(self.clamp_range(r));
        }
    }

    /// Drain PTY output, process through VTE, broadcast viewport patches.
    /// Returns true if the child process has exited.
    pub fn poll(&mut self) -> bool {
        let mut got_data = false;
        for _ in 0..MAX_PTY_CHUNKS_PER_POLL {
            let Ok(data) = self.pty_rx.try_recv() else {
                break;
            };
            self.processor.advance(&mut self.term, &data);
            got_data = true;
        }
        if got_data && self.copy_mode.is_none() && self.selection.is_some() {
            self.selection = None;
        }
        if got_data {
            self.sync_viewport();
        }
        self.maybe_broadcast_mode();
        if let Ok(Some(status)) = self.child.try_wait() {
            let code = status.exit_code() as i32;
            let _ = self.patch_tx.send(ServiceMessage::ProcessExited {
                process_id: self.id,
                exit_code: Some(code),
            });
            return true;
        }
        false
    }

    fn sync_viewport(&mut self) {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let num_cols = grid.columns();
        let offset = grid.display_offset() as i32;

        let full = self.line_hashes.len() != num_lines;
        if self.line_hashes.len() != num_lines {
            self.line_hashes.resize(num_lines, 0);
        }

        let mut changed_lines = Vec::new();
        for row_idx in 0..num_lines {
            let hash = hash_grid_row(&self.term, row_idx, offset);
            if full || hash != self.line_hashes[row_idx] {
                self.line_hashes[row_idx] = hash;
                changed_lines.push((row_idx as u16, build_line(&self.term, row_idx, offset)));
            }
        }

        // If the buffer mutated under an active selection, the selection no
        // longer points at the same characters. Clear it (browser-style:
        // typing into a textarea drops the selection). Skip on `full` resyncs
        // (initial population) which mark every row as "changed".
        if !full
            && let Some(sel) = self.selection
            && changed_lines.iter().any(|(row, _)| {
                let r = *row;
                let lo = sel.start_row.min(sel.end_row);
                let hi = sel.start_row.max(sel.end_row);
                r >= lo && r <= hi
            })
        {
            self.selection = None;
        }

        let copy_mode_cursor = self.copy_mode.as_ref().map(|cm| cm.cursor);
        let cursor_point = grid.cursor.point;
        let normal_cursor_pos = (cursor_point.column.0 as u16, cursor_point.line.0 as u16);
        let cursor_pos = copy_mode_cursor.unwrap_or(normal_cursor_pos);
        let cursor_moved = self.last_cursor != Some(cursor_pos);

        let selection_changed = self.selection != self.last_selection;
        let copy_mode = self.copy_mode.is_some();
        let copy_mode_changed = self.last_viewport_copy_mode != Some(copy_mode);

        // Skip broadcast only when neither line content, cursor, mode, nor selection changed.
        if changed_lines.is_empty()
            && !full
            && !cursor_moved
            && !selection_changed
            && !copy_mode_changed
        {
            return;
        }
        self.last_cursor = Some(cursor_pos);
        self.last_selection = self.selection;
        self.last_viewport_copy_mode = Some(copy_mode);

        let scrolled_back = offset > 0;
        let cursor_char = if let Some((col, row)) = copy_mode_cursor {
            if (row as usize) < num_lines && (col as usize) < num_cols {
                let cursor_row = &grid[Line(row as i32 - offset)];
                cursor_row[Column(col as usize)].c.to_string()
            } else {
                " ".to_string()
            }
        } else {
            let cursor_row = &grid[cursor_point.line];
            let cell = &cursor_row[cursor_point.column];
            cell.c.to_string()
        };

        let patch = ServiceMessage::ViewportPatch {
            process_id: self.id,
            changed_lines,
            cursor: TermCursor {
                col: cursor_pos.0,
                row: cursor_pos.1,
                shape: CursorShape::Block,
                visible: copy_mode_cursor.is_some() || !scrolled_back,
                ch: cursor_char,
            },
            cols: num_cols as u16,
            rows: num_lines as u16,
            selection: self.selection,
            copy_mode,
            full,
        };
        let _ = self.patch_tx.send(patch);
    }

    pub fn snapshot(&self) -> ServiceMessage {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let offset = grid.display_offset() as i32;

        let mut lines = Vec::with_capacity(num_lines);
        for row_idx in 0..num_lines {
            lines.push(build_line(&self.term, row_idx, offset));
        }

        let cursor_point = grid.cursor.point;
        let scrolled_back = offset > 0;
        let cursor_char = {
            let cursor_row = &grid[cursor_point.line];
            let cell = &cursor_row[cursor_point.column];
            cell.c.to_string()
        };

        ServiceMessage::Snapshot {
            process_id: self.id,
            lines,
            cursor: TermCursor {
                col: cursor_point.column.0 as u16,
                row: cursor_point.line.0 as u16,
                shape: CursorShape::Block,
                visible: !scrolled_back,
                ch: cursor_char,
            },
            cols: grid.columns() as u16,
            rows: num_lines as u16,
        }
    }

    pub fn info(&self) -> ProcessInfo {
        ProcessInfo {
            id: self.id,
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            cols: self.cols,
            rows: self.rows,
            pid: self.pid,
            created_at_secs: self.created_at.elapsed().as_secs(),
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

pub struct ProcessManager {
    pub processes: HashMap<ProcessId, Process>,
    wake_tx: mpsc::UnboundedSender<ProcessId>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        let (wake_tx, _) = mpsc::unbounded_channel();
        Self::new(wake_tx)
    }
}

impl ProcessManager {
    pub fn new(wake_tx: mpsc::UnboundedSender<ProcessId>) -> Self {
        Self {
            processes: HashMap::new(),
            wake_tx,
        }
    }

    pub fn create_process(
        &mut self,
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<ProcessId, String> {
        let process = Process::new_with_wake(shell, cwd, env, cols, rows, self.wake_tx.clone())?;
        let id = process.id;
        self.processes.insert(id, process);
        Ok(id)
    }

    pub fn poll_all(&mut self) -> Vec<ProcessId> {
        let mut exited = Vec::new();
        for (id, process) in &mut self.processes {
            if process.poll() {
                exited.push(*id);
            }
        }
        exited
    }

    pub fn remove_process(&mut self, id: &ProcessId) {
        if let Some(mut process) = self.processes.remove(id) {
            process.kill();
        }
    }

    pub fn input_writer(&self, id: &ProcessId) -> Option<PtyInputWriter> {
        self.processes.get(id).map(Process::input_writer)
    }

    pub fn shutdown(&mut self) {
        for process in self.processes.values_mut() {
            process.kill();
        }
        self.processes.clear();
    }
}

// --- Grid helpers ---

fn hash_grid_row<T: TermEventListener>(term: &Term<T>, row_idx: usize, offset: i32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let grid = term.grid();
    let num_cols = grid.columns();
    let row = &grid[Line(row_idx as i32 - offset)];
    for col_idx in 0..num_cols {
        let cell = &row[Column(col_idx)];
        cell.c.hash(&mut hasher);
        std::mem::discriminant(&cell.fg).hash(&mut hasher);
        match &cell.fg {
            Color::Named(c) => (*c as u8).hash(&mut hasher),
            Color::Spec(rgb) => {
                rgb.r.hash(&mut hasher);
                rgb.g.hash(&mut hasher);
                rgb.b.hash(&mut hasher);
            }
            Color::Indexed(i) => i.hash(&mut hasher),
        }
        std::mem::discriminant(&cell.bg).hash(&mut hasher);
        match &cell.bg {
            Color::Named(c) => (*c as u8).hash(&mut hasher),
            Color::Spec(rgb) => {
                rgb.r.hash(&mut hasher);
                rgb.g.hash(&mut hasher);
                rgb.b.hash(&mut hasher);
            }
            Color::Indexed(i) => i.hash(&mut hasher),
        }
        cell.flags.bits().hash(&mut hasher);
    }
    hasher.finish()
}

fn build_line<T: TermEventListener>(term: &Term<T>, row_idx: usize, offset: i32) -> TermLine {
    let grid = term.grid();
    let num_cols = grid.columns();
    let row = &grid[Line(row_idx as i32 - offset)];
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut cur_fg = TermColor::Default;
    let mut cur_bg = TermColor::Default;
    let mut cur_flags: u16 = 0;
    let mut span_col_start: u16 = 0;
    let mut span_grid_cols: u16 = 0;

    for col_idx in 0..num_cols {
        let cell = &row[Column(col_idx)];
        if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
            span_grid_cols += 1;
            continue;
        }
        let fg = color_to_term_color(&cell.fg);
        let bg = color_to_term_color(&cell.bg);
        let flags = cell_flags_to_u16(cell.flags);
        if fg != cur_fg || bg != cur_bg || flags != cur_flags {
            if !text.is_empty() {
                spans.push(TermSpan {
                    text: std::mem::take(&mut text),
                    fg: cur_fg,
                    bg: cur_bg,
                    flags: cur_flags,
                    col: span_col_start,
                    grid_cols: span_grid_cols,
                });
                span_col_start = col_idx as u16;
                span_grid_cols = 0;
            }
            cur_fg = fg;
            cur_bg = bg;
            cur_flags = flags;
        }
        text.push(cell.c);
        span_grid_cols += 1;
    }
    if !text.is_empty() {
        spans.push(TermSpan {
            text,
            fg: cur_fg,
            bg: cur_bg,
            flags: cur_flags,
            col: span_col_start,
            grid_cols: span_grid_cols,
        });
    }
    TermLine { spans }
}

fn color_to_term_color(color: &Color) -> TermColor {
    match color {
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::DimForeground | NamedColor::BrightForeground => {
                TermColor::Default
            }
            NamedColor::Background | NamedColor::Cursor => TermColor::Default,
            other => TermColor::Indexed(named_to_ansi_index(other)),
        },
        Color::Indexed(idx) if *idx < 16 => TermColor::Indexed(*idx),
        Color::Indexed(idx) => {
            let [r, g, b] = ansi_256_to_rgb(*idx);
            TermColor::Rgb(r, g, b)
        }
        Color::Spec(rgb) => TermColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn named_to_ansi_index(named: &NamedColor) -> u8 {
    match named {
        NamedColor::Black | NamedColor::DimBlack => 0,
        NamedColor::Red | NamedColor::DimRed => 1,
        NamedColor::Green | NamedColor::DimGreen => 2,
        NamedColor::Yellow | NamedColor::DimYellow => 3,
        NamedColor::Blue | NamedColor::DimBlue => 4,
        NamedColor::Magenta | NamedColor::DimMagenta => 5,
        NamedColor::Cyan | NamedColor::DimCyan => 6,
        NamedColor::White | NamedColor::DimWhite => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => 7,
    }
}

fn cell_flags_to_u16(flags: CellFlags) -> u16 {
    let mut f = 0u16;
    if flags.contains(CellFlags::BOLD) {
        f |= FLAG_BOLD;
    }
    if flags.contains(CellFlags::ITALIC) {
        f |= FLAG_ITALIC;
    }
    if flags.contains(CellFlags::UNDERLINE) {
        f |= FLAG_UNDERLINE;
    }
    if flags.contains(CellFlags::STRIKEOUT) {
        f |= FLAG_STRIKETHROUGH;
    }
    if flags.contains(CellFlags::DIM) {
        f |= FLAG_DIM;
    }
    if flags.contains(CellFlags::INVERSE) {
        f |= FLAG_INVERSE;
    }
    f
}

fn ansi_256_to_rgb(idx: u8) -> [u8; 3] {
    if idx < 16 {
        return [0, 0, 0];
    }
    if idx < 232 {
        let i = idx - 16;
        let r = (i / 36) * 51;
        let g = ((i % 36) / 6) * 51;
        let b = (i % 6) * 51;
        [r, g, b]
    } else {
        let v = 8 + (idx - 232) * 10;
        [v, v, v]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn pty_reader_notifies_when_output_arrives() {
        let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            "/bin/sh".to_string(),
            String::new(),
            Vec::new(),
            80,
            24,
            wake_tx,
        )
        .expect("process should spawn");

        process.write_input(b"printf vmux-wake-test\r");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if wake_rx.try_recv().is_ok() {
                process.kill();
                return;
            }
            if Instant::now() >= deadline {
                process.kill();
                panic!("timed out waiting for PTY wake notification");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn write_input_to_writer_does_not_need_process_lock() {
        #[derive(Clone)]
        struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

        impl Write for CapturingWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let captured = Arc::new(Mutex::new(Vec::new()));
        let writer: PtyInputWriter =
            Arc::new(Mutex::new(Box::new(CapturingWriter(captured.clone()))));

        Process::write_input_to_writer(&writer, b"abc");

        assert_eq!(*captured.lock().unwrap(), b"abc".to_vec());
    }
}
