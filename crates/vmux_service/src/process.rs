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

#[derive(Clone)]
struct ServiceEventProxy {
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
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
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
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
    /// Last broadcast (mouse_capture, copy_mode) flags.
    last_terminal_mode: Option<(bool, bool)>,
    /// Active copy-mode state (cursor + optional anchor). None when not in copy mode.
    copy_mode: Option<CopyModeState>,
}

struct CopyModeState {
    cursor: (u16, u16),
    anchor: Option<(u16, u16)>,
}

impl Process {
    pub fn new(
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
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
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
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
            last_terminal_mode: None,
            copy_mode: None,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServiceMessage> {
        self.patch_tx.subscribe()
    }

    pub fn write_input(&self, data: &[u8]) {
        if let Ok(mut w) = self.pty_writer.lock() {
            let _ = w.write_all(data);
        }
    }

    /// Replace the selection. None clears it.
    pub fn set_selection(&mut self, range: Option<TermSelectionRange>) {
        self.selection = range;
        self.sync_viewport();
    }

    /// Extend the current selection's end point to (col, row). If no
    /// selection exists, anchor at (col, row).
    pub fn extend_selection_to(&mut self, col: u16, row: u16) {
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
        let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '-');
        if !is_word(line[Column(col as usize)].c) {
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
        while start > 0 && is_word(line[Column(start - 1)].c) {
            start -= 1;
        }
        while end + 1 < num_cols && is_word(line[Column(end + 1)].c) {
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
        let mut lines: Vec<String> = Vec::new();
        for row_idx in sr..=er {
            if (row_idx as usize) >= num_lines {
                break;
            }
            let line = &grid[Line(row_idx as i32 - offset)];
            let (lo, hi) = if sel.is_block || sr == er {
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
        let cursor = (
            grid.cursor.point.column.0 as u16,
            grid.cursor.point.line.0 as u16,
        );
        self.copy_mode = Some(CopyModeState {
            cursor,
            anchor: None,
        });
        self.selection = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    pub fn exit_copy_mode(&mut self) {
        self.copy_mode = None;
        self.selection = None;
        self.maybe_broadcast_mode();
        self.sync_viewport();
    }

    /// Returns Some(text) if the key triggered a Copy action.
    pub fn copy_mode_key(&mut self, key: crate::protocol::CopyModeKey) -> Option<String> {
        use crate::protocol::CopyModeKey as K;
        let cm = self.copy_mode.as_mut()?;
        let cols = self.cols;
        let rows = self.rows;
        let (cur_col, cur_row) = cm.cursor;
        let (new_col, new_row) = match key {
            K::Left => (cur_col.saturating_sub(1), cur_row),
            K::Right => ((cur_col + 1).min(cols.saturating_sub(1)), cur_row),
            K::Up => (cur_col, cur_row.saturating_sub(1)),
            K::Down => (cur_col, (cur_row + 1).min(rows.saturating_sub(1))),
            K::LineStart => (0, cur_row),
            K::LineEnd => (cols.saturating_sub(1), cur_row),
            K::PageUp => (cur_col, cur_row.saturating_sub(rows / 2)),
            K::PageDown => (cur_col, (cur_row + rows / 2).min(rows.saturating_sub(1))),
            K::StartSelection => {
                cm.anchor = Some((cur_col, cur_row));
                (cur_col, cur_row)
            }
            K::Exit => {
                self.exit_copy_mode();
                return None;
            }
            K::Copy => {
                let text = self.selection_text();
                self.exit_copy_mode();
                return text;
            }
        };
        cm.cursor = (new_col, new_row);
        if let Some((ac, ar)) = cm.anchor {
            self.selection = Some(TermSelectionRange {
                start_col: ac,
                start_row: ar,
                end_col: new_col,
                end_row: new_row,
                is_block: false,
            });
        }
        self.sync_viewport();
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
    }

    /// Drain PTY output, process through VTE, broadcast viewport patches.
    /// Returns true if the child process has exited.
    pub fn poll(&mut self) -> bool {
        let mut got_data = false;
        while let Ok(data) = self.pty_rx.try_recv() {
            self.processor.advance(&mut self.term, &data);
            got_data = true;
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

        let cursor_point = grid.cursor.point;
        let cursor_pos = (cursor_point.column.0 as u16, cursor_point.line.0 as u16);
        let cursor_moved = self.last_cursor != Some(cursor_pos);

        let selection_changed = self.selection != self.last_selection;

        // Skip broadcast only when neither line content, cursor, nor selection changed.
        if changed_lines.is_empty() && !full && !cursor_moved && !selection_changed {
            return;
        }
        self.last_cursor = Some(cursor_pos);
        self.last_selection = self.selection;

        let scrolled_back = offset > 0;
        let cursor_char = {
            let cursor_row = &grid[cursor_point.line];
            let cell = &cursor_row[cursor_point.column];
            cell.c.to_string()
        };

        let patch = ServiceMessage::ViewportPatch {
            process_id: self.id,
            changed_lines,
            cursor: TermCursor {
                col: cursor_point.column.0 as u16,
                row: cursor_point.line.0 as u16,
                shape: CursorShape::Block,
                visible: !scrolled_back,
                ch: cursor_char,
            },
            cols: num_cols as u16,
            rows: num_lines as u16,
            selection: self.selection,
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
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
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
        let process = Process::new(shell, cwd, env, cols, rows)?;
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
