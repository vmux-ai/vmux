use crate::protocol::{ProcessId, ProcessInfo, ServiceMessage};
use alacritty_terminal::{
    event::{Event as TermEvent, EventListener as TermEventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Line},
    term::{Config as TermConfig, Term, cell::Flags as CellFlags},
    vte::ansi::{Color, NamedColor, Processor},
};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[cfg(unix)]
use std::{os::unix::ffi::OsStrExt, path::Component};
use tokio::sync::{broadcast, mpsc};
use vmux_core::event::*;

const MAX_PTY_CHUNKS_PER_POLL: usize = 64;
const _: () = assert!(MAX_PTY_CHUNKS_PER_POLL <= 256);
const HEAVY_OUTPUT_FRAME_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub struct PtyInputWriter {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    input_pending: Arc<AtomicBool>,
}

impl PtyInputWriter {
    fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
            input_pending: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    if path.is_dir() {
        return false;
    }
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
}

#[cfg(unix)]
fn resolve_executable(
    command: &str,
    cwd: &str,
    env: &[(String, String)],
) -> Result<std::path::PathBuf, String> {
    let command_path = std::path::Path::new(command);
    let explicit_cwd = matches!(
        command_path.components().next(),
        Some(Component::CurDir | Component::ParentDir)
    );
    if command_path.is_absolute() || explicit_cwd {
        let candidate = if command_path.is_absolute() {
            command_path.to_path_buf()
        } else {
            std::path::Path::new(cwd).join(command_path)
        };
        return is_executable(&candidate)
            .then_some(candidate)
            .ok_or_else(|| format!("unable to execute command: {command}"));
    }

    let path = env
        .iter()
        .rev()
        .find(|(name, _)| name == "PATH")
        .map(|(_, value)| std::ffi::OsString::from(value))
        .or_else(|| std::env::var_os("PATH"))
        .ok_or_else(|| format!("unable to resolve command without PATH: {command}"))?;
    for path_entry in std::env::split_paths(&path) {
        let candidate = std::path::Path::new(cwd)
            .join(path_entry)
            .join(command_path);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err(format!("unable to resolve executable: {command}"))
}

#[cfg(unix)]
fn run_pty_reader(
    mut reader: Box<dyn Read + Send>,
    reader_fd: std::os::fd::RawFd,
    pty_tx: mpsc::UnboundedSender<Vec<u8>>,
    wake_tx: mpsc::UnboundedSender<ProcessId>,
    process_id: ProcessId,
    in_flight: Arc<AtomicBool>,
    send_delay: Duration,
) {
    let mut buf = [0u8; 4096];
    loop {
        let mut poll_fd = libc::pollfd {
            fd: reader_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll_fd, 1, -1) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        in_flight.store(true, Ordering::Release);
        match reader.read(&mut buf) {
            Ok(0) => {
                in_flight.store(false, Ordering::Release);
                break;
            }
            Ok(n) => {
                if !send_delay.is_zero() {
                    std::thread::sleep(send_delay);
                }
                let sent = pty_tx.send(buf[..n].to_vec()).is_ok();
                in_flight.store(false, Ordering::Release);
                if !sent {
                    break;
                }
                let _ = wake_tx.send(process_id);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                in_flight.store(false, Ordering::Release);
            }
            Err(_) => {
                in_flight.store(false, Ordering::Release);
                break;
            }
        }
    }
}

#[derive(Clone)]
struct ServiceEventProxy {
    process_id: ProcessId,
    pty_writer: PtyInputWriter,
    patch_tx: broadcast::Sender<ServiceMessage>,
}

impl TermEventListener for ServiceEventProxy {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::PtyWrite(text) => {
                if let Ok(mut writer) = self.pty_writer.writer.lock() {
                    let _ = writer.write_all(text.as_bytes());
                }
            }
            TermEvent::Title(title) => {
                let _ = self.patch_tx.send(ServiceMessage::ProcessTitle {
                    process_id: self.process_id,
                    title,
                });
            }
            TermEvent::Bell => {
                let _ = self.patch_tx.send(ServiceMessage::Bell {
                    process_id: self.process_id,
                });
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
    osc133: crate::osc133::Osc133Scanner,
    command_ended_seq: u64,
    last_command_exit: Option<i32>,
    run_marker: crate::run_marker::RunMarkerScanner,
    last_run_completion: Option<(String, i32)>,
    pty_writer: PtyInputWriter,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pty_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    #[cfg(unix)]
    pty_reader_in_flight: Arc<AtomicBool>,
    /// Broadcasts viewport patches to all attached GUI clients.
    patch_tx: broadcast::Sender<ServiceMessage>,
    line_hashes: Vec<u64>,
    /// Document-row-keyed hash cache for the native-scroll window path.
    win_hashes: HashMap<u32, u64>,
    /// Frontend window top (document row); ignored while `following`.
    view_top: u32,
    /// Frontend pinned to the bottom → stream the bottom window autonomously.
    following: bool,
    /// Last emitted (first_row, total_rows) for the window path (broadcast dedup).
    last_win: Option<(u32, u32)>,
    /// Whether the previous sync used the passthrough (alt/copy) path.
    last_passthrough: bool,
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
    output_viewport_dirty: bool,
    last_output_viewport_at: Option<Instant>,
    last_output_viewport_heavy: bool,
    /// Last broadcast (mouse_capture, copy_mode, alt_screen) flags.
    last_terminal_mode: Option<(bool, bool, bool, bool)>,
    /// Active copy-mode state (cursor, anchor, visual state). None when not in copy mode.
    copy_mode: Option<CopyModeState>,
    /// Keep the process (and its final grid) in the manager after the child exits instead of
    /// being reaped by the poll loop. Set for ACP-native terminals so `terminal/output` can read
    /// the completed command's scrollback until the agent calls `terminal/release`.
    keep_after_exit: bool,
    /// Set once the child has exited and all PTY output has been drained.
    exited: bool,
    /// Set once `ProcessExited` has been broadcast for the child.
    exit_reported: bool,
    /// The child's exit code, recorded when it exits. Surfaced to ACP `terminal/output`.
    exit_code: Option<i32>,
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

fn offset_row(row: u16, delta: i32) -> u16 {
    if delta >= 0 {
        row.saturating_add(delta as u16)
    } else {
        row.saturating_sub((-delta) as u16)
    }
}

fn copy_mode_scroll_input(
    mode: &alacritty_terminal::term::TermMode,
    delta: i32,
    cols: u16,
    rows: u16,
) -> Option<(Vec<u8>, i32)> {
    if !mode.contains(alacritty_terminal::term::TermMode::ALT_SCREEN) {
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

fn sgr_mouse_wheel_bytes(up: bool, col: u16, row: u16, modifiers: u8) -> Vec<u8> {
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

fn alternate_scroll_bytes(up: bool, app_cursor: bool) -> &'static [u8] {
    match (up, app_cursor) {
        (true, false) => b"\x1b[A",
        (false, false) => b"\x1b[B",
        (true, true) => b"\x1bOA",
        (false, true) => b"\x1bOB",
    }
}

impl Process {
    pub fn new(
        id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        let (wake_tx, _) = mpsc::unbounded_channel();
        Self::new_with_wake(id, command, args, cwd, env, cols, rows, wake_tx)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_wake(
        id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
        wake_tx: mpsc::UnboundedSender<ProcessId>,
    ) -> Result<Self, String> {
        Self::new_with_wake_and_reader_delay(
            id,
            command,
            args,
            cwd,
            env,
            cols,
            rows,
            wake_tx,
            Duration::ZERO,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_wake_and_reader_delay(
        id: ProcessId,
        command: String,
        mut args: Vec<String>,
        cwd: String,
        mut env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
        wake_tx: mpsc::UnboundedSender<ProcessId>,
        reader_send_delay: Duration,
    ) -> Result<Self, String> {
        crate::shell_integration::inject(
            &command,
            &mut args,
            &mut env,
            &crate::paths::shell_integration_dir(),
        );
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open PTY: {e}"))?;

        #[cfg(unix)]
        let mut cmd = if cwd.is_empty() {
            let mut cmd = CommandBuilder::new(&command);
            cmd.args(&args);
            cmd
        } else {
            let executable = resolve_executable(&command, &cwd, &env)?;
            let original_home = env
                .iter()
                .rev()
                .find(|(name, _)| name == "HOME")
                .map(|(_, value)| value.clone())
                .or_else(|| std::env::var("HOME").ok());
            let mut cmd = CommandBuilder::new("/bin/sh");
            cmd.arg("-c");
            cmd.arg(
                "if [ \"$1\" = 1 ]; then export HOME=\"$2\"; else unset HOME; fi; shift 2; exec \"$@\"",
            );
            cmd.arg("vmux-cwd");
            cmd.arg(if original_home.is_some() { "1" } else { "0" });
            cmd.arg(original_home.as_deref().unwrap_or_default());
            cmd.arg(executable);
            cmd.args(&args);
            cmd.cwd(&cwd);
            cmd
        };
        #[cfg(not(unix))]
        let mut cmd = {
            let mut cmd = CommandBuilder::new(&command);
            cmd.args(&args);
            if !cwd.is_empty() {
                let cwd_path = std::path::Path::new(&cwd);
                if !cwd_path.is_dir() {
                    return Err(format!("cwd is not a directory: {cwd}"));
                }
                cmd.cwd(cwd_path);
            }
            cmd
        };
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_CTYPE", "UTF-8");
        for (k, v) in &env {
            cmd.env(k, v);
        }
        #[cfg(unix)]
        if !cwd.is_empty() {
            cmd.env("HOME", &cwd);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn shell: {e}"))?;
        let pid = child
            .process_id()
            .ok_or_else(|| "spawned PTY child has no PID".to_string())?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone PTY reader: {e}"))?;
        #[cfg(unix)]
        let reader_fd = pair
            .master
            .as_raw_fd()
            .ok_or_else(|| "PTY master has no file descriptor".to_string())?;
        let writer = PtyInputWriter::new(Box::new(
            pair.master
                .take_writer()
                .map_err(|e| format!("failed to take PTY writer: {e}"))?,
        ));
        drop(pair.slave);

        let (pty_tx, pty_rx) = mpsc::unbounded_channel();
        let wake_process_id = id;
        #[cfg(unix)]
        let pty_reader_in_flight = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        let thread_reader_in_flight = Arc::clone(&pty_reader_in_flight);
        #[cfg(unix)]
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", &id.to_string()[..8]))
            .spawn(move || {
                run_pty_reader(
                    reader,
                    reader_fd,
                    pty_tx,
                    wake_tx,
                    wake_process_id,
                    thread_reader_in_flight,
                    reader_send_delay,
                );
            })
            .map_err(|e| format!("failed to spawn PTY reader: {e}"))?;
        #[cfg(not(unix))]
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", &id.to_string()[..8]))
            .spawn(move || {
                let mut buf = [0u8; 4096];
                let mut reader = reader;
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if !reader_send_delay.is_zero() {
                                std::thread::sleep(reader_send_delay);
                            }
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

        let (patch_tx, _) = broadcast::channel(256);
        let event_proxy = ServiceEventProxy {
            process_id: id,
            pty_writer: writer.clone(),
            patch_tx: patch_tx.clone(),
        };
        let dims = PtyDimensions { cols, rows };
        let term = Term::new(TermConfig::default(), &dims, event_proxy);

        Ok(Self {
            id,
            shell: command,
            cwd,
            cols,
            rows,
            pid,
            created_at: Instant::now(),
            term,
            processor: Processor::new(),
            osc133: crate::osc133::Osc133Scanner::new(),
            command_ended_seq: 0,
            last_command_exit: None,
            run_marker: crate::run_marker::RunMarkerScanner::new(),
            last_run_completion: None,
            pty_writer: writer,
            master: pair.master,
            child,
            pty_rx,
            #[cfg(unix)]
            pty_reader_in_flight,
            patch_tx,
            line_hashes: Vec::new(),
            win_hashes: HashMap::new(),
            view_top: 0,
            following: true,
            last_win: None,
            last_passthrough: false,
            last_cursor: None,
            selection: None,
            last_selection: None,
            last_viewport_copy_mode: None,
            output_viewport_dirty: false,
            last_output_viewport_at: None,
            last_output_viewport_heavy: false,
            last_terminal_mode: None,
            copy_mode: None,
            keep_after_exit: false,
            exited: bool::default(),
            exit_reported: false,
            exit_code: None,
        })
    }

    /// Mark this process to survive child exit (ACP-native terminal). The poll loop must not reap
    /// it; `terminal/release` removes it explicitly.
    pub fn set_keep_after_exit(&mut self) {
        self.keep_after_exit = true;
    }

    /// Whether this process should survive child exit instead of being reaped.
    pub fn keep_after_exit(&self) -> bool {
        self.keep_after_exit
    }

    /// The child's exit code once it has exited, else `None`.
    pub fn process_exit(&self) -> Option<i32> {
        self.exit_code
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServiceMessage> {
        self.patch_tx.subscribe()
    }

    pub fn command_status(&self) -> (u64, Option<i32>) {
        (self.command_ended_seq, self.last_command_exit)
    }

    /// Last agent `run` completion `(token, exit)` parsed from a
    /// [`crate::run_marker::VMUX_RUN_OSC`] escape. The token lets a blocking
    /// `run` correlate the exit code to its own command.
    pub fn run_completion(&self) -> Option<(String, i32)> {
        self.last_run_completion.clone()
    }

    pub fn write_input(&self, data: &[u8]) {
        Self::write_input_to_writer(&self.pty_writer, data);
    }

    pub fn input_writer(&self) -> PtyInputWriter {
        self.pty_writer.clone()
    }

    pub fn write_input_to_writer(writer: &PtyInputWriter, data: &[u8]) {
        writer.input_pending.store(true, Ordering::Release);
        if let Ok(mut w) = writer.writer.lock() {
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
        let offset = grid.display_offset() as i32;

        // Normalize so (start_row, start_col) <= (end_row, end_col) row-major.
        let (sr, sc, er, ec) = if (sel.start_row, sel.start_col) <= (sel.end_row, sel.end_col) {
            (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
        } else {
            (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
        };

        let max_col = num_cols.saturating_sub(1);
        let top_line = grid.topmost_line().0;
        let bottom_line = grid.bottommost_line().0;
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

    /// Broadcast TerminalMode whenever mouse-capture, copy-mode, alt-screen, or
    /// focus-reporting changes.
    fn maybe_broadcast_mode(&mut self) {
        use alacritty_terminal::term::TermMode;
        let mouse_capture = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let copy_mode = self.copy_mode.is_some();
        let alt_screen = self.term.mode().contains(TermMode::ALT_SCREEN);
        let focus_reporting = self.term.mode().contains(TermMode::FOCUS_IN_OUT);
        let cur = (mouse_capture, copy_mode, alt_screen, focus_reporting);
        if self.last_terminal_mode != Some(cur) {
            self.last_terminal_mode = Some(cur);
            let _ = self.patch_tx.send(ServiceMessage::TerminalMode {
                process_id: self.id,
                mouse_capture,
                copy_mode,
                alt_screen,
                focus_reporting,
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
            K::Up => self.move_copy_mode_cursor_vertically(cur_col, cur_row, -1),
            K::Down => self.move_copy_mode_cursor_vertically(cur_col, cur_row, 1),
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
            K::PageUp => {
                self.move_copy_mode_cursor_vertically(cur_col, cur_row, -i32::from(rows / 2))
            }
            K::PageDown => {
                self.move_copy_mode_cursor_vertically(cur_col, cur_row, i32::from(rows / 2))
            }
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

    fn move_copy_mode_cursor_vertically(&mut self, col: u16, row: u16, delta: i32) -> (u16, u16) {
        match delta.cmp(&0) {
            std::cmp::Ordering::Less => {
                let distance = (-delta) as u16;
                if row >= distance {
                    return (col, row - distance);
                }
                let missing = distance - row;
                let scrolled = self.scroll_copy_mode_viewport(i32::from(missing));
                let row = offset_row(row, scrolled);
                (
                    col,
                    row.saturating_sub(distance)
                        .min(self.rows.saturating_sub(1)),
                )
            }
            std::cmp::Ordering::Greater => {
                let distance = delta as u16;
                let max_row = self.rows.saturating_sub(1);
                if row.saturating_add(distance) <= max_row {
                    return (col, row + distance);
                }
                let missing = row.saturating_add(distance).saturating_sub(max_row);
                let scrolled = self.scroll_copy_mode_viewport(-i32::from(missing));
                let row = offset_row(row, scrolled);
                (col, row.saturating_add(distance).min(max_row))
            }
            std::cmp::Ordering::Equal => (col, row),
        }
    }

    fn scroll_copy_mode_viewport(&mut self, delta: i32) -> i32 {
        let old_offset = self.term.grid().display_offset() as i32;
        self.term.scroll_display(Scroll::Delta(delta));
        let new_offset = self.term.grid().display_offset() as i32;
        let mut actual = new_offset - old_offset;
        if actual == 0
            && let Some((bytes, rows)) =
                copy_mode_scroll_input(self.term.mode(), delta, self.cols, self.rows)
        {
            Self::write_input_to_writer(&self.pty_writer, &bytes);
            actual = rows;
        }
        if actual != 0 {
            self.line_hashes.clear();
            if let Some(cm) = self.copy_mode.as_mut() {
                cm.cursor.1 = offset_row(cm.cursor.1, actual);
                cm.anchor.1 = offset_row(cm.anchor.1, actual);
            }
        }
        actual
    }

    pub fn handle_mouse_wheel(&mut self, up: bool, col: u16, row: u16, modifiers: u8) {
        use alacritty_terminal::term::TermMode;
        let delta = if up { 1 } else { -1 };
        if self.copy_mode.is_some() {
            if self.scroll_copy_mode_viewport(delta) != 0 {
                self.sync_viewport();
            }
            return;
        }
        let mode = self.term.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = sgr_mouse_wheel_bytes(up, col, row, modifiers);
            Self::write_input_to_writer(&self.pty_writer, &bytes);
        } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let bytes = alternate_scroll_bytes(up, mode.contains(TermMode::APP_CURSOR));
            Self::write_input_to_writer(&self.pty_writer, bytes);
        }
    }

    /// Native-scroll intent from the frontend: set the window top / follow state
    /// and re-serve the window. `display_offset` is untouched (window reads use
    /// direct `Line` indexing).
    pub fn handle_scroll_window(&mut self, top_row: u32, follow: bool) {
        self.following = follow;
        self.view_top = top_row;
        self.win_hashes.clear();
        self.last_win = None;
        self.sync_viewport();
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
        self.win_hashes.clear();
        self.last_win = None;
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

    fn pty_reader_drained(&self) -> bool {
        #[cfg(unix)]
        {
            if self.pty_reader_in_flight.load(Ordering::Acquire) {
                return false;
            }
            let Some(fd) = self.master.as_raw_fd() else {
                return false;
            };
            let mut available: libc::c_int = 0;
            if unsafe { libc::ioctl(fd, libc::FIONREAD as _, &mut available) } < 0 {
                return false;
            }
            available == 0 && !self.pty_reader_in_flight.load(Ordering::Acquire)
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    /// Drain PTY output, process through VTE, broadcast viewport patches.
    /// Returns true if the child process has exited.
    pub fn poll(&mut self) -> bool {
        // A kept-after-exit process (ACP terminal) stays in the manager with its final grid; skip
        // re-processing and re-broadcasting `ProcessExited` on every subsequent tick.
        if self.exited {
            return false;
        }
        let mut got_data = false;
        let mut pty_closed = false;
        let mut remaining = MAX_PTY_CHUNKS_PER_POLL;
        let mut drained_after_exit = false;
        loop {
            if remaining == 0 {
                if self.exit_code.is_none()
                    && let Ok(Some(status)) = self.child.try_wait()
                {
                    self.exit_code = Some(status.exit_code() as i32);
                }
                if self.exit_code.is_some() && !drained_after_exit {
                    remaining = self.pty_rx.len();
                    drained_after_exit = true;
                    if remaining == 0 {
                        break;
                    }
                } else {
                    break;
                }
            }
            let data = match self.pty_rx.try_recv() {
                Ok(data) => data,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    pty_closed = true;
                    break;
                }
            };
            remaining -= 1;
            self.processor.advance(&mut self.term, &data);
            for event in self.osc133.feed(&data) {
                let kind = match event {
                    crate::osc133::Osc133Event::CommandStart => {
                        crate::protocol::CommandLifecycleKind::Started
                    }
                    crate::osc133::Osc133Event::CommandEnd(exit_code) => {
                        self.command_ended_seq = self.command_ended_seq.wrapping_add(1);
                        self.last_command_exit = exit_code;
                        crate::protocol::CommandLifecycleKind::Ended { exit_code }
                    }
                };
                let _ = self.patch_tx.send(ServiceMessage::CommandLifecycle {
                    process_id: self.id,
                    kind,
                });
            }
            for marker in self.run_marker.feed(&data) {
                self.last_run_completion = Some((marker.token, marker.exit));
            }
            got_data = true;
        }
        if got_data && self.copy_mode.is_none() && self.selection.is_some() {
            self.selection = None;
        }
        if got_data {
            self.output_viewport_dirty = true;
        }
        if self.exit_code.is_none()
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.exit_code = Some(status.exit_code() as i32);
        }
        if self.output_viewport_dirty {
            self.maybe_sync_output_viewport(pty_closed || self.exit_code.is_some(), got_data);
        }
        self.maybe_broadcast_mode();
        if !self.exit_reported
            && let Some(code) = self.exit_code
            && self.pty_reader_drained()
            && self.pty_rx.is_empty()
        {
            self.exit_reported = true;
            let _ = self.patch_tx.send(ServiceMessage::ProcessExited {
                process_id: self.id,
                exit_code: Some(code),
            });
        }
        if pty_closed && self.exit_code.is_some() {
            self.exited = true;
            return true;
        }
        false
    }

    fn maybe_sync_output_viewport(&mut self, force: bool, got_data: bool) {
        let now = Instant::now();
        let elapsed = self
            .last_output_viewport_at
            .map(|last| now.saturating_duration_since(last));
        let input_pending = take_input_priority(&self.pty_writer.input_pending, got_data);
        if !output_viewport_due(
            self.last_output_viewport_heavy,
            force || input_pending,
            elapsed,
        ) {
            return;
        }
        let changed_rows = self.sync_viewport();
        self.output_viewport_dirty = false;
        self.last_output_viewport_at = Some(now);
        self.last_output_viewport_heavy = changed_rows.saturating_mul(2) >= self.rows as usize;
    }

    fn sync_viewport(&mut self) -> usize {
        let mode = self.term.mode();
        let passthrough = self.copy_mode.is_some()
            || mode.contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
            || mode.intersects(alacritty_terminal::term::TermMode::MOUSE_MODE);
        if passthrough != self.last_passthrough {
            self.line_hashes.clear();
            self.win_hashes.clear();
            self.last_win = None;
            self.last_passthrough = passthrough;
        }
        if passthrough {
            self.sync_screen_relative()
        } else {
            self.sync_document_window()
        }
    }

    /// Passthrough path (alt-screen / copy-mode): render the visible screen at
    /// document rows `0..screen_lines` with `first_row = 0` and no native scroll.
    /// Preserves the pre-native-scroll behavior for TUIs and copy-mode.
    fn sync_screen_relative(&mut self) -> usize {
        let grid = self.term.grid();
        let num_lines = grid.screen_lines();
        let num_cols = grid.columns();
        let offset = grid.display_offset() as i32;
        let mode = self.term.mode();
        let alt = mode.contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
        let mouse = mode.intersects(alacritty_terminal::term::TermMode::MOUSE_MODE);

        let full = self.line_hashes.len() != num_lines;
        if self.line_hashes.len() != num_lines {
            self.line_hashes.resize(num_lines, 0);
        }

        let mut changed_lines = Vec::new();
        for row_idx in 0..num_lines {
            let hash = hash_grid_row(&self.term, row_idx, offset);
            if full || hash != self.line_hashes[row_idx] {
                self.line_hashes[row_idx] = hash;
                changed_lines.push((row_idx as u32, build_line(&self.term, row_idx, offset)));
            }
        }

        // If the buffer mutated under an active selection, the selection no
        // longer points at the same characters. Clear it (browser-style).
        if self.copy_mode.is_none()
            && !full
            && let Some(sel) = self.selection
            && changed_lines.iter().any(|(row, _)| {
                let r = *row;
                let lo = sel.start_row.min(sel.end_row) as u32;
                let hi = sel.start_row.max(sel.end_row) as u32;
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
            return 0;
        }
        let changed_rows = changed_lines.len();
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
                row: cursor_pos.1 as u32,
                shape: CursorShape::Block,
                visible: copy_mode_cursor.is_some() || !scrolled_back,
                ch: cursor_char,
            },
            cols: num_cols as u16,
            rows: num_lines as u16,
            selection: self.selection,
            copy_mode,
            full,
            first_row: 0,
            total_rows: num_lines as u32,
            alt,
            mouse,
            evicted_total: 0,
        };
        let _ = self.patch_tx.send(patch);
        changed_rows
    }

    /// Native-scroll path (primary screen, no copy-mode): serve a document-row
    /// window around `view_top` (or the bottom when `following`) by direct grid
    /// `Line` indexing. `display_offset` stays 0.
    fn sync_document_window(&mut self) -> usize {
        let grid = self.term.grid();
        let screen = grid.screen_lines();
        let num_cols = grid.columns();
        let total_rows = grid.total_lines() as u32;
        let history = total_rows.saturating_sub(screen as u32);
        let visible = screen as u16;

        let overscan = if self.following {
            0
        } else {
            vmux_core::scroll::overscan_for(
                visible,
                vmux_core::scroll::TERMINAL_OVERSCAN_K,
                vmux_core::scroll::OVERSCAN_FLOOR,
                vmux_core::scroll::OVERSCAN_CAP,
            )
        };
        let view_top = if self.following {
            history
        } else {
            vmux_core::scroll::clamp_top_line(self.view_top, total_rows, visible)
        };
        let first_row = view_top.saturating_sub(overscan);
        let end_row = (view_top + visible as u32 + overscan).min(total_rows);

        let offset = history as i32;

        let mut changed_lines = Vec::new();
        let mut live: HashMap<u32, u64> = HashMap::new();
        for doc_row in first_row..end_row {
            let hash = hash_grid_row(&self.term, doc_row as usize, offset);
            live.insert(doc_row, hash);
            if self.win_hashes.get(&doc_row) != Some(&hash) {
                changed_lines.push((doc_row, build_line(&self.term, doc_row as usize, offset)));
            }
        }
        let full = self.win_hashes.is_empty();
        self.win_hashes = live;

        let cursor_point = grid.cursor.point;
        let cursor_doc_row = history + cursor_point.line.0 as u32;
        let cursor_col = cursor_point.column.0 as u16;
        let cursor_in_window = cursor_doc_row >= first_row && cursor_doc_row < end_row;
        let cursor_char = {
            let cell = &grid[cursor_point.line][cursor_point.column];
            cell.c.to_string()
        };

        let cursor_key = (cursor_col, cursor_doc_row as u16);
        let cursor_moved = self.last_cursor != Some(cursor_key);
        let selection_changed = self.selection != self.last_selection;
        let win = (first_row, total_rows);
        let win_changed = self.last_win != Some(win);

        if changed_lines.is_empty() && !full && !cursor_moved && !selection_changed && !win_changed
        {
            return 0;
        }
        let changed_rows = changed_lines.len();
        self.last_cursor = Some(cursor_key);
        self.last_selection = self.selection;
        self.last_win = Some(win);
        self.last_viewport_copy_mode = Some(false);

        let patch = ServiceMessage::ViewportPatch {
            process_id: self.id,
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
            selection: self.selection,
            copy_mode: false,
            full,
            first_row,
            total_rows,
            alt: false,
            mouse: false,
            evicted_total: 0,
        };
        let _ = self.patch_tx.send(patch);
        changed_rows
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
                row: cursor_point.line.0 as u32,
                shape: CursorShape::Block,
                visible: !scrolled_back,
                ch: cursor_char,
            },
            cols: grid.columns() as u16,
            rows: num_lines as u16,
        }
    }

    /// Full terminal text: scrollback history plus the visible screen, joined by
    /// `\n` with trailing spaces stripped per line and trailing blank lines
    /// removed. Reads the rendered alacritty grid, so the text carries no ANSI.
    pub fn full_text(&self) -> String {
        let grid = self.term.grid();
        let num_cols = grid.columns();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;

        let mut lines: Vec<String> = Vec::new();
        for line_idx in top..=bottom {
            let row = &grid[Line(line_idx)];
            let mut text = String::with_capacity(num_cols);
            for col in 0..num_cols {
                text.push(row[Column(col)].c);
            }
            lines.push(text.trim_end().to_string());
        }
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
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
        if self.exit_code.is_some() {
            return;
        }
        let _ = self.child.kill();
        if let Ok(status) = self.child.wait() {
            self.exit_code = Some(status.exit_code() as i32);
        }
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

    #[allow(clippy::too_many_arguments)]
    pub fn create_process(
        &mut self,
        id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<(ProcessId, u32), String> {
        let process = Process::new_with_wake(
            id,
            command,
            args,
            cwd,
            env,
            cols,
            rows,
            self.wake_tx.clone(),
        )?;
        let pid = process.pid;
        self.processes.insert(id, process);
        Ok((id, pid))
    }

    /// Like [`create_process`](Self::create_process) but marks the process to survive child exit
    /// (ACP-native terminal): the poll loop must not reap it, so `terminal/output` can read the
    /// completed command's scrollback until `terminal/release`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_process_keep_alive(
        &mut self,
        id: ProcessId,
        command: String,
        args: Vec<String>,
        cwd: String,
        env: Vec<(String, String)>,
        cols: u16,
        rows: u16,
    ) -> Result<(ProcessId, u32), String> {
        let created = self.create_process(id, command, args, cwd, env, cols, rows)?;
        if let Some(process) = self.processes.get_mut(&id) {
            process.set_keep_after_exit();
        }
        Ok(created)
    }

    /// Kill a process's child without removing it from the manager (keeps its final grid for ACP
    /// `terminal/output`). Distinct from [`remove_process`](Self::remove_process), which also drops
    /// it.
    pub fn kill_process(&mut self, id: &ProcessId) {
        if let Some(process) = self.processes.get_mut(id) {
            process.kill();
        }
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

fn output_viewport_due(heavy: bool, force: bool, elapsed: Option<Duration>) -> bool {
    force || !heavy || elapsed.is_none_or(|elapsed| elapsed >= HEAVY_OUTPUT_FRAME_INTERVAL)
}

fn take_input_priority(input_pending: &AtomicBool, got_data: bool) -> bool {
    got_data && input_pending.swap(false, Ordering::AcqRel)
}

fn mix_row_hash(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x100000001b3);
}

fn mix_color_hash(hash: &mut u64, color: &Color) {
    match color {
        Color::Named(color) => {
            mix_row_hash(hash, 0);
            mix_row_hash(hash, *color as u8 as u64);
        }
        Color::Spec(rgb) => {
            mix_row_hash(hash, 1);
            mix_row_hash(hash, rgb.r as u64);
            mix_row_hash(hash, rgb.g as u64);
            mix_row_hash(hash, rgb.b as u64);
        }
        Color::Indexed(index) => {
            mix_row_hash(hash, 2);
            mix_row_hash(hash, *index as u64);
        }
    }
}

fn hash_grid_row<T: TermEventListener>(term: &Term<T>, row_idx: usize, offset: i32) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    let grid = term.grid();
    let num_cols = grid.columns();
    let row = &grid[Line(row_idx as i32 - offset)];
    for col_idx in 0..num_cols {
        let cell = &row[Column(col_idx)];
        mix_row_hash(&mut hash, cell.c as u32 as u64);
        mix_color_hash(&mut hash, &cell.fg);
        mix_color_hash(&mut hash, &cell.bg);
        mix_row_hash(&mut hash, cell.flags.bits() as u64);
    }
    hash
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
    TermLine {
        spans,
        links: Vec::new(),
    }
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
    fn heavy_output_waits_for_frame_interval_but_sparse_and_final_output_do_not() {
        assert!(!output_viewport_due(
            true,
            false,
            Some(HEAVY_OUTPUT_FRAME_INTERVAL - Duration::from_millis(1)),
        ));
        assert!(output_viewport_due(
            true,
            false,
            Some(HEAVY_OUTPUT_FRAME_INTERVAL),
        ));
        assert!(output_viewport_due(true, true, Some(Duration::ZERO)));
        assert!(output_viewport_due(false, false, Some(Duration::ZERO)));
    }

    #[test]
    fn input_priority_waits_for_fresh_pty_output() {
        let input_pending = AtomicBool::new(true);

        assert!(!take_input_priority(&input_pending, false));
        assert!(input_pending.load(Ordering::Acquire));
        assert!(take_input_priority(&input_pending, true));
        assert!(!input_pending.load(Ordering::Acquire));
    }

    #[test]
    fn pty_reader_notifies_when_output_arrives() {
        let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "sh".to_string(),
            vec![],
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
    fn process_starts_in_requested_cwd_without_typing_cd() {
        let temp = std::env::temp_dir().join(format!("vmux-process-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let cwd = temp.to_string_lossy().into_owned();
        let home = temp.join("home-marker").to_string_lossy().into_owned();
        let (wake_tx, _) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![],
            cwd.clone(),
            vec![("HOME".to_string(), home.clone())],
            120,
            24,
            wake_tx,
        )
        .expect("process should spawn");

        drain_process_output(&mut process, Duration::from_millis(300));
        process.write_input(b"printf 'HOME=%s\\n' \"$HOME\"; pwd\r");
        let text = wait_for_snapshot_text(&mut process, &cwd);

        process.kill();
        let _ = std::fs::remove_dir_all(&temp);

        assert!(text.contains(&cwd));
        assert!(text.contains(&format!("HOME={home}")));
        assert!(!text.contains(&format!("cd {cwd}")));
    }

    #[test]
    fn process_rejects_invalid_cwd_at_spawn() {
        let mut mgr = ProcessManager::default();
        let cwd = std::env::temp_dir().join(format!(
            "vmux-process-missing-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));

        let result = mgr.create_process(
            ProcessId::new(),
            "/bin/sh".into(),
            vec!["-c".into(), "exit 0".into()],
            cwd.to_string_lossy().into_owned(),
            Vec::new(),
            80,
            24,
        );

        assert!(result.is_err());
        assert!(mgr.processes.is_empty());

        let file = std::env::temp_dir().join(format!(
            "vmux-process-file-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));
        std::fs::write(&file, b"not a directory").expect("write cwd file");
        let result = mgr.create_process(
            ProcessId::new(),
            "/bin/sh".into(),
            vec!["-c".into(), "exit 0".into()],
            file.to_string_lossy().into_owned(),
            Vec::new(),
            80,
            24,
        );
        let _ = std::fs::remove_file(file);

        assert!(result.is_err());
        assert!(mgr.processes.is_empty());
    }

    #[test]
    fn process_with_cwd_rejects_missing_executable_at_spawn() {
        let mut mgr = ProcessManager::default();
        let cwd = std::env::temp_dir().join(format!(
            "vmux-process-command-cwd-{}-{}",
            std::process::id(),
            ProcessId::new()
        ));
        std::fs::create_dir_all(&cwd).expect("create cwd");

        let result = mgr.create_process(
            ProcessId::new(),
            "/definitely/missing/vmux-command".into(),
            Vec::new(),
            cwd.to_string_lossy().into_owned(),
            Vec::new(),
            80,
            24,
        );
        assert!(result.is_err());
        assert!(mgr.processes.is_empty());

        let command = cwd.join("not-executable");
        std::fs::write(&command, b"#!/bin/sh\nexit 0\n").expect("write command");
        let result = mgr.create_process(
            ProcessId::new(),
            command.to_string_lossy().into_owned(),
            Vec::new(),
            cwd.to_string_lossy().into_owned(),
            Vec::new(),
            80,
            24,
        );
        let _ = std::fs::remove_dir_all(cwd);

        assert!(result.is_err());
        assert!(mgr.processes.is_empty());
    }

    #[test]
    fn full_text_includes_scrolled_off_history() {
        let (wake_tx, _) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![],
            String::new(),
            Vec::new(),
            80,
            24,
            wake_tx,
        )
        .expect("process should spawn");

        drain_process_output(&mut process, Duration::from_millis(300));
        // Screen is 24 rows; printing ~60 lines scrolls FIRSTLINE into history.
        process.write_input(
            b"echo FIRSTLINE; for i in $(seq 1 60); do echo pad_$i; done; echo LASTLINE\r",
        );
        let _ = wait_for_snapshot_text(&mut process, "LASTLINE");
        drain_process_output(&mut process, Duration::from_millis(200));

        let visible = snapshot_text(process.snapshot());
        let full = process.full_text();
        process.kill();

        assert!(
            full.contains("LASTLINE"),
            "full_text should include last line"
        );
        assert!(
            full.contains("FIRSTLINE"),
            "full_text should include scrolled-off first line; full=\n{full}"
        );
        assert!(
            !visible.contains("FIRSTLINE"),
            "visible snapshot should not include scrolled-off line; visible=\n{visible}"
        );
    }

    fn drain_process_output(process: &mut Process, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            process.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn poll_broadcasts_command_lifecycle_from_osc133() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec!["-c".to_string(), "printf '\\033]133;D;0\\007'".to_string()],
            String::new(),
            vec![],
            80,
            24,
            wake_tx,
        )
        .expect("spawn");
        let mut rx = process.subscribe();

        drain_process_output(&mut process, Duration::from_secs(2));

        let mut saw_end = false;
        while let Ok(msg) = rx.try_recv() {
            if let ServiceMessage::CommandLifecycle {
                kind: crate::protocol::CommandLifecycleKind::Ended { exit_code },
                ..
            } = msg
            {
                assert_eq!(exit_code, Some(0));
                saw_end = true;
            }
        }
        assert!(
            saw_end,
            "expected a CommandLifecycle Ended broadcast from OSC 133;D;0"
        );
        assert_eq!(
            process.command_status(),
            (1, Some(0)),
            "command_status must record one completed command with its exit code"
        );
    }

    fn wait_for_snapshot_text(process: &mut Process, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            process.poll();
            let text = snapshot_text(process.snapshot());
            if text.contains(needle) || Instant::now() >= deadline {
                return text;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn snapshot_text(snapshot: ServiceMessage) -> String {
        let ServiceMessage::Snapshot { lines, .. } = snapshot else {
            unreachable!();
        };
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
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
        let writer = PtyInputWriter::new(Box::new(CapturingWriter(captured.clone())));

        Process::write_input_to_writer(&writer, b"abc");

        assert_eq!(*captured.lock().unwrap(), b"abc".to_vec());
        assert!(writer.input_pending.load(Ordering::Acquire));
    }

    #[test]
    fn copy_mode_up_at_alt_screen_top_uses_mouse_wheel_scroll() {
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

        let (wake_tx, _) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![],
            String::new(),
            Vec::new(),
            12,
            8,
            wake_tx,
        )
        .expect("process should spawn");
        let captured = Arc::new(Mutex::new(Vec::new()));
        process.pty_writer = PtyInputWriter::new(Box::new(CapturingWriter(captured.clone())));

        process.process_output_for_test(b"\x1b[?1049h\x1b[Hone\r\ntwo\r\nthree\x1b[H");
        process.enter_copy_mode();
        process.copy_mode_key(crate::protocol::CopyModeKey::StartLineSelection);
        process.copy_mode_key(crate::protocol::CopyModeKey::Up);

        assert_eq!(process.copy_mode.as_ref().unwrap().cursor.1, 1);
        process.copy_mode_key(crate::protocol::CopyModeKey::Up);
        process.kill();

        assert_eq!(*captured.lock().unwrap(), b"\x1b[<64;7;5M".to_vec());
    }

    fn capturing_process(cols: u16, rows: u16) -> (Process, Arc<Mutex<Vec<u8>>>) {
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

        let (wake_tx, _) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![],
            String::new(),
            Vec::new(),
            cols,
            rows,
            wake_tx,
        )
        .expect("process should spawn");
        let captured = Arc::new(Mutex::new(Vec::new()));
        process.pty_writer = PtyInputWriter::new(Box::new(CapturingWriter(captured.clone())));
        (process, captured)
    }

    #[test]
    fn mouse_wheel_in_mouse_mode_forwards_sgr() {
        let (mut process, captured) = capturing_process(12, 8);
        process.process_output_for_test(b"\x1b[?1000h");
        process.handle_mouse_wheel(true, 6, 4, 0);
        process.handle_mouse_wheel(false, 0, 0, 0);
        process.kill();
        assert_eq!(
            *captured.lock().unwrap(),
            b"\x1b[<64;7;5M\x1b[<65;1;1M".to_vec()
        );
    }

    #[test]
    fn mouse_wheel_in_alt_screen_sends_arrow_keys() {
        let (mut process, captured) = capturing_process(12, 8);
        process.process_output_for_test(b"\x1b[?1049h");
        process.handle_mouse_wheel(true, 0, 0, 0);
        process.handle_mouse_wheel(false, 0, 0, 0);
        process.kill();
        assert_eq!(*captured.lock().unwrap(), b"\x1b[A\x1b[B".to_vec());
    }

    #[test]
    fn mouse_wheel_in_alt_screen_app_cursor_sends_ss3_arrows() {
        let (mut process, captured) = capturing_process(12, 8);
        process.process_output_for_test(b"\x1b[?1049h\x1b[?1h");
        process.handle_mouse_wheel(true, 0, 0, 0);
        process.kill();
        assert_eq!(*captured.lock().unwrap(), b"\x1bOA".to_vec());
    }

    #[test]
    fn mouse_wheel_in_alt_screen_without_alternate_scroll_is_inert() {
        let (mut process, captured) = capturing_process(12, 8);
        process.process_output_for_test(b"\x1b[?1049h\x1b[?1007l");
        process.handle_mouse_wheel(true, 0, 0, 0);
        process.kill();
        assert!(captured.lock().unwrap().is_empty());
    }

    #[test]
    fn scroll_window_serves_document_row_window() {
        let (mut process, captured) = capturing_process(12, 4);
        let mut feed = Vec::new();
        for i in 0..40 {
            feed.extend_from_slice(format!("line{i}\r\n").as_bytes());
        }
        process.process_output_for_test(&feed);

        let total = process.term.grid().total_lines() as u32;
        assert!(
            total >= 40,
            "expected scrollback to accumulate, got {total}"
        );

        let mut patches = process.subscribe();
        // Scroll to the very top (document row 0), not following.
        process.handle_scroll_window(0, false);

        // Native scroll must not move display_offset or write to the pty.
        assert_eq!(process.term.grid().display_offset(), 0);
        assert!(
            captured.lock().unwrap().is_empty(),
            "native scroll must not write to the pty"
        );

        let (changed_lines, first_row, total_rows) = std::iter::from_fn(|| patches.try_recv().ok())
            .find_map(|msg| match msg {
                ServiceMessage::ViewportPatch {
                    changed_lines,
                    first_row,
                    total_rows,
                    ..
                } => Some((changed_lines, first_row, total_rows)),
                _ => None,
            })
            .expect("scroll must broadcast a viewport patch");
        assert_eq!(
            first_row, 0,
            "top scroll serves the window from document row 0"
        );
        assert_eq!(total_rows, total, "patch carries the full document height");
        assert!(
            changed_lines.iter().any(|(r, _)| *r == 0),
            "top window must include the oldest document row"
        );

        process.kill();
    }

    #[test]
    fn following_patch_contains_only_visible_rows() {
        let (mut process, _) = capturing_process(12, 4);
        let mut patches = process.subscribe();
        let mut feed = Vec::new();
        for i in 0..40 {
            feed.extend_from_slice(format!("line{i}\r\n").as_bytes());
        }

        process.process_output_for_test(&feed);

        let changed_lines = std::iter::from_fn(|| patches.try_recv().ok())
            .find_map(|message| match message {
                ServiceMessage::ViewportPatch { changed_lines, .. } => Some(changed_lines),
                _ => None,
            })
            .expect("output must broadcast a viewport patch");
        assert!(changed_lines.len() <= process.rows as usize);
        process.kill();
    }

    #[test]
    fn terminal_mode_broadcasts_alt_screen_toggle() {
        let (wake_tx, _) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake(
            ProcessId::new(),
            "/bin/sh".to_string(),
            vec![],
            String::new(),
            Vec::new(),
            12,
            8,
            wake_tx,
        )
        .expect("process should spawn");

        let mut rx = process.subscribe();

        process.process_output_for_test(b"\x1b[?1049h");
        process.maybe_broadcast_mode();

        let mut alt_on = None;
        while let Ok(msg) = rx.try_recv() {
            if let ServiceMessage::TerminalMode { alt_screen, .. } = msg {
                alt_on = Some(alt_screen);
            }
        }
        assert_eq!(
            alt_on,
            Some(true),
            "entering alt screen broadcasts alt_screen=true"
        );

        process.process_output_for_test(b"\x1b[?1049l");
        process.maybe_broadcast_mode();

        let mut alt_off = None;
        while let Ok(msg) = rx.try_recv() {
            if let ServiceMessage::TerminalMode { alt_screen, .. } = msg {
                alt_off = Some(alt_screen);
            }
        }
        assert_eq!(
            alt_off,
            Some(false),
            "leaving alt screen broadcasts alt_screen=false"
        );

        process.kill();
    }

    #[test]
    fn create_process_returns_real_pid() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut mgr = ProcessManager::new(wake_tx);
        let process_id = ProcessId::new();
        let (id, pid) = mgr
            .create_process(
                process_id,
                "/bin/sh".into(),
                vec![],
                String::new(),
                Vec::new(),
                80,
                24,
            )
            .expect("spawn");
        assert!(pid > 0, "expected real pid, got {pid}");
        assert!(mgr.processes.contains_key(&id));
    }

    #[test]
    fn proxy_broadcasts_process_title_on_term_title_event() {
        use std::io;

        let (tx, mut rx) = broadcast::channel::<ServiceMessage>(8);
        let writer = PtyInputWriter::new(Box::new(io::sink()));
        let process_id = ProcessId::new();
        let proxy = ServiceEventProxy {
            process_id,
            pty_writer: writer,
            patch_tx: tx,
        };

        proxy.send_event(TermEvent::Title("hello-osc".into()));

        let msg = rx.try_recv().expect("ProcessTitle should be broadcast");
        match msg {
            ServiceMessage::ProcessTitle {
                process_id: got_id,
                title,
            } => {
                assert_eq!(got_id, process_id);
                assert_eq!(title, "hello-osc");
            }
            other => panic!("expected ProcessTitle, got {other:?}"),
        }
    }

    #[test]
    fn proxy_broadcasts_bell_on_term_bell_event() {
        use std::io;

        let (tx, mut rx) = broadcast::channel::<ServiceMessage>(8);
        let writer = PtyInputWriter::new(Box::new(io::sink()));
        let process_id = ProcessId::new();
        let proxy = ServiceEventProxy {
            process_id,
            pty_writer: writer,
            patch_tx: tx,
        };

        proxy.send_event(TermEvent::Bell);

        let msg = rx.try_recv().expect("Bell should be broadcast");
        match msg {
            ServiceMessage::Bell { process_id: got_id } => assert_eq!(got_id, process_id),
            other => panic!("expected Bell, got {other:?}"),
        }
    }

    #[test]
    fn keep_after_exit_retains_process_and_exit_code() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut mgr = ProcessManager::new(wake_tx);
        let id = ProcessId::new();
        mgr.create_process_keep_alive(
            id,
            "/bin/sh".into(),
            vec!["-c".into(), "exit 5".into()],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
        let mut rx = mgr.processes.get(&id).unwrap().subscribe();

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_exit = false;
        while Instant::now() < deadline {
            if mgr.poll_all().contains(&id) {
                saw_exit = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_exit, "process should have exited");

        // Kept in the manager (not reaped by poll) with its exit code recorded.
        let process = mgr.processes.get(&id).expect("kept after exit");
        assert_eq!(process.process_exit(), Some(5));

        // Further polls neither report the exit again nor drop the process.
        assert!(mgr.poll_all().is_empty());
        assert!(mgr.processes.contains_key(&id));

        // Exactly one ProcessExited was broadcast.
        let mut exits = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, ServiceMessage::ProcessExited { .. }) {
                exits += 1;
            }
        }
        assert_eq!(exits, 1);

        mgr.remove_process(&id);
    }

    #[test]
    fn process_exit_drains_all_queued_pty_output() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut mgr = ProcessManager::new(wake_tx);
        let id = ProcessId::new();
        mgr.create_process_keep_alive(
            id,
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "awk 'BEGIN { for (i = 0; i < 70000; i++) print \"abcdefgh\"; print \"TAIL-SENTINEL\" }'"
                    .into(),
            ],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
        std::thread::sleep(Duration::from_millis(500));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_exit = false;
        while Instant::now() < deadline {
            if mgr.poll_all().contains(&id) {
                saw_exit = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        assert!(saw_exit, "process should exit");
        assert!(
            mgr.processes
                .get(&id)
                .expect("process retained")
                .full_text()
                .contains("TAIL-SENTINEL"),
            "exit must not discard queued PTY output"
        );
        mgr.remove_process(&id);
    }

    #[test]
    fn process_exit_is_reported_after_queued_pty_output_is_drained() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut mgr = ProcessManager::new(wake_tx);
        let id = ProcessId::new();
        mgr.create_process_keep_alive(
            id,
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "awk 'BEGIN { for (i = 0; i < 70000; i++) print \"abcdefgh\"; print \"TAIL-SENTINEL\" }'"
                    .into(),
            ],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
        let mut rx = mgr.processes.get(&id).expect("process").subscribe();
        std::thread::sleep(Duration::from_millis(500));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut reported = false;
        while Instant::now() < deadline {
            mgr.poll_all();
            while let Ok(message) = rx.try_recv() {
                if matches!(message, ServiceMessage::ProcessExited { .. }) {
                    reported = true;
                }
            }
            if reported {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        assert!(reported, "process exit should be reported");
        assert!(
            mgr.processes
                .get(&id)
                .expect("process retained")
                .full_text()
                .contains("TAIL-SENTINEL"),
            "exit must not be reported while queued PTY output remains"
        );
        mgr.remove_process(&id);
    }

    #[test]
    fn process_exit_waits_for_pty_reader_catch_up() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut process = Process::new_with_wake_and_reader_delay(
            ProcessId::new(),
            "/bin/sh".into(),
            vec!["-c".into(), "printf READER-TAIL".into()],
            String::new(),
            Vec::new(),
            80,
            24,
            wake_tx,
            Duration::from_millis(100),
        )
        .expect("spawn");
        let mut rx = process.subscribe();

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut reported = false;
        while Instant::now() < deadline {
            process.poll();
            while let Ok(message) = rx.try_recv() {
                if matches!(message, ServiceMessage::ProcessExited { .. }) {
                    reported = true;
                }
            }
            if reported {
                break;
            }
            std::thread::yield_now();
        }

        assert!(reported, "process exit should be reported");
        assert!(
            process.full_text().contains("READER-TAIL"),
            "exit raced with the PTY reader"
        );
        process.kill();
    }

    #[test]
    fn process_exit_is_reported_before_background_descendant_closes_pty() {
        let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
        let mut mgr = ProcessManager::new(wake_tx);
        let id = ProcessId::new();
        mgr.create_process_keep_alive(
            id,
            "/bin/sh".into(),
            vec!["-c".into(), "exit 0".into()],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
        std::thread::sleep(Duration::from_millis(50));
        let (pty_tx, pty_rx) = mpsc::unbounded_channel();
        let process = mgr.processes.get_mut(&id).expect("process");
        process.pty_rx = pty_rx;
        let mut rx = process.subscribe();

        let deadline = Instant::now() + Duration::from_millis(500);
        let mut reported = false;
        while Instant::now() < deadline {
            assert!(
                !mgr.poll_all().contains(&id),
                "process must remain until PTY output is drained"
            );
            while let Ok(message) = rx.try_recv() {
                if matches!(message, ServiceMessage::ProcessExited { .. }) {
                    reported = true;
                }
            }
            if reported {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        drop(pty_tx);
        mgr.remove_process(&id);
        assert!(reported, "child exit must not wait for PTY closure");
    }
}
