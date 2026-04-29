use crate::protocol::{DaemonMessage, SessionId, SessionInfo};
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
struct DaemonEventProxy {
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl TermEventListener for DaemonEventProxy {
    fn send_event(&self, event: TermEvent) {
        if let TermEvent::PtyWrite(text) = event {
            if let Ok(mut writer) = self.pty_writer.lock() {
                let _ = writer.write_all(text.as_bytes());
            }
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

/// A single terminal session managed by the daemon.
pub struct Session {
    pub id: SessionId,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub pid: u32,
    pub created_at: Instant,
    term: Term<DaemonEventProxy>,
    processor: Processor,
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pty_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Broadcasts viewport patches to all attached GUI clients.
    patch_tx: broadcast::Sender<DaemonMessage>,
    line_hashes: Vec<u64>,
}

impl Session {
    pub fn new(
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        let id = SessionId::new();
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

        if !cwd.is_empty() && cwd_path.exists() {
            if let Ok(mut w) = writer.lock() {
                let cd_cmd = format!("cd {}\n", cwd);
                let _ = w.write_all(cd_cmd.as_bytes());
                let _ = w.flush();
            }
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

        let event_proxy = DaemonEventProxy {
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
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonMessage> {
        self.patch_tx.subscribe()
    }

    pub fn write_input(&self, data: &[u8]) {
        if let Ok(mut w) = self.pty_writer.lock() {
            let _ = w.write_all(data);
        }
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
        if let Ok(Some(status)) = self.child.try_wait() {
            let code = status.exit_code() as i32;
            let _ = self.patch_tx.send(DaemonMessage::SessionExited {
                session_id: self.id,
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
        if changed_lines.is_empty() && !full {
            return;
        }

        let cursor_point = grid.cursor.point;
        let scrolled_back = offset > 0;
        let cursor_char = {
            let cursor_row = &grid[cursor_point.line];
            let cell = &cursor_row[cursor_point.column];
            cell.c.to_string()
        };

        let patch = DaemonMessage::ViewportPatch {
            session_id: self.id,
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
            selection: None,
            full,
        };
        let _ = self.patch_tx.send(patch);
    }

    pub fn snapshot(&self) -> DaemonMessage {
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

        DaemonMessage::Snapshot {
            session_id: self.id,
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

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
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

pub struct SessionManager {
    pub sessions: HashMap<SessionId, Session>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn create_session(
        &mut self,
        shell: String,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<SessionId, String> {
        let session = Session::new(shell, cwd, env, cols, rows)?;
        let id = session.id;
        self.sessions.insert(id, session);
        Ok(id)
    }

    pub fn poll_all(&mut self) -> Vec<SessionId> {
        let mut exited = Vec::new();
        for (id, session) in &mut self.sessions {
            if session.poll() {
                exited.push(*id);
            }
        }
        exited
    }

    pub fn remove_session(&mut self, id: &SessionId) {
        if let Some(mut session) = self.sessions.remove(id) {
            session.kill();
        }
    }

    pub fn shutdown(&mut self) {
        for (_, session) in &mut self.sessions {
            session.kill();
        }
        self.sessions.clear();
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
    if flags.contains(CellFlags::BOLD) { f |= FLAG_BOLD; }
    if flags.contains(CellFlags::ITALIC) { f |= FLAG_ITALIC; }
    if flags.contains(CellFlags::UNDERLINE) { f |= FLAG_UNDERLINE; }
    if flags.contains(CellFlags::STRIKEOUT) { f |= FLAG_STRIKETHROUGH; }
    if flags.contains(CellFlags::DIM) { f |= FLAG_DIM; }
    if flags.contains(CellFlags::INVERSE) { f |= FLAG_INVERSE; }
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
