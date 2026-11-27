use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Waker;
use std::thread;
use std::time::Duration;

use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};
use vmux_job::Job;
use vmux_job::blocking::Lane;

use crate::execution::Failure;
use crate::signals::Signals;
use crate::tui_render::{
    CockpitAction, CockpitInput, CockpitProfile, CockpitState, Theme, ThemePreset,
};

pub(crate) struct Terminal;

impl Terminal {
    pub(crate) fn probe() -> (bool, bool, u16, ThemePreset) {
        let stderr_tty = io::stderr().is_terminal();
        let stdin_tty = io::stdin().is_terminal();
        let ci = env::var_os("CI")
            .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1");
        let dumb = env::var_os("TERM").is_some_and(|value| value == "dumb");
        let disabled = env::var_os("VMUX_NO_TUI")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let interactive = stderr_tty && stdin_tty && !ci && !dumb && !disabled;
        let color = interactive && env::var_os("NO_COLOR").is_none();
        let width = if stderr_tty { Self::size().0 } else { 80 };
        let theme = ThemeStore::load();
        (interactive, color, width, theme)
    }

    pub(crate) fn size() -> (u16, u16) {
        let mut size = libc::winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let status = unsafe { libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, &mut size) };
        if status == 0 && size.ws_col >= 40 && size.ws_row >= 12 {
            (size.ws_col, size.ws_row)
        } else {
            (80, 24)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredPreferences {
    theme: String,
}

pub(crate) struct ThemeStore;

impl ThemeStore {
    pub(crate) fn load() -> ThemePreset {
        if let Some(name) = env::var_os("VMUX_TUI_THEME").and_then(|value| value.into_string().ok())
            && let Some(preset) = ThemePreset::parse(&name)
        {
            return preset;
        }
        let Some(path) = Self::path() else {
            return ThemePreset::default();
        };
        let Ok(bytes) = fs::read(path) else {
            return ThemePreset::default();
        };
        let Ok(stored) = serde_json::from_slice::<StoredPreferences>(&bytes) else {
            return ThemePreset::default();
        };
        ThemePreset::parse(&stored.theme).unwrap_or_default()
    }

    pub(crate) fn save(preset: ThemePreset) -> io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            if fs::symlink_metadata(parent).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(io::Error::other(
                    "TUI config directory must not be a symlink",
                ));
            }
            fs::create_dir_all(parent)?;
        }
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(io::Error::other("TUI config file must not be a symlink"));
        }
        let payload = serde_json::to_vec_pretty(&StoredPreferences {
            theme: preset.id().to_owned(),
        })
        .map_err(io::Error::other)?;
        let temp = path.with_extension(format!("{}.json.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Err(error) = file.write_all(&payload) {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        drop(file);
        if let Err(error) = fs::rename(&temp, &path) {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        Ok(())
    }

    fn path() -> Option<PathBuf> {
        if let Some(path) = env::var_os("VMUX_CLI_TUI_CONFIG")
            && !path.is_empty()
        {
            return Some(PathBuf::from(path));
        }
        let home = env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".vmux/cli/tui.json"))
    }
}

pub(crate) struct LiveWriter {
    transient_active: bool,
    ticker_stop: Option<Arc<AtomicBool>>,
}

impl LiveWriter {
    pub(crate) fn new() -> Self {
        Self {
            transient_active: false,
            ticker_stop: None,
        }
    }

    pub(crate) fn write_block(&mut self, text: &str) {
        self.clear_transient();
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(text.as_bytes());
        let _ = stderr.flush();
    }

    pub(crate) fn write_stdout(&mut self, text: &str) {
        self.clear_transient();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }

    pub(crate) fn write_stderr(&mut self, text: &str) {
        self.clear_transient();
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(text.as_bytes());
        let _ = stderr.flush();
    }

    pub(crate) fn draw_transient(&mut self, line: &str) {
        let mut stderr = io::stderr().lock();
        if !self.transient_active {
            let _ = stderr.write_all(b"\x1b[?25l");
            self.transient_active = true;
        }
        let _ = write!(stderr, "\r\x1b[2K{line}");
        let _ = stderr.flush();
    }

    pub(crate) fn clear_transient(&mut self) {
        if !self.transient_active {
            return;
        }
        let mut stderr = io::stderr().lock();
        let _ = stderr.write_all(b"\r\x1b[2K\x1b[?25h");
        let _ = stderr.flush();
        self.transient_active = false;
    }

    pub(crate) fn start_ticker(&mut self, wake: Waker) {
        if self.ticker_stop.is_some() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        self.ticker_stop = Some(stop);
        let _ = thread::Builder::new()
            .name("vmux-tui-tick".into())
            .spawn(move || {
                while !flag.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(90));
                    if flag.load(Ordering::Acquire) {
                        break;
                    }
                    wake.wake_by_ref();
                }
            });
    }

    pub(crate) fn stop_ticker(&mut self) {
        if let Some(stop) = self.ticker_stop.take() {
            stop.store(true, Ordering::Release);
        }
    }
}

impl Drop for LiveWriter {
    fn drop(&mut self) {
        self.stop_ticker();
        self.clear_transient();
    }
}

struct RawTerminalGuard {
    original: libc::termios,
    alt_screen: bool,
}

impl RawTerminalGuard {
    fn enter(alt_screen: bool) -> io::Result<Self> {
        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut raw = original;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut stderr = io::stderr().lock();
        if alt_screen {
            let _ = stderr.write_all(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[?25l");
        } else {
            let _ = stderr.write_all(b"\x1b[?25l");
        }
        let _ = stderr.flush();
        Ok(Self {
            original,
            alt_screen,
        })
    }
}

impl Drop for RawTerminalGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
        }
        let mut stderr = io::stderr().lock();
        if self.alt_screen {
            let _ = stderr.write_all(b"\x1b[?25h\x1b[?1049l");
        } else {
            let _ = stderr.write_all(b"\x1b[?25h");
        }
        let _ = stderr.flush();
    }
}

struct KeyReader;

impl KeyReader {
    fn poll(timeout_ms: i32) -> io::Result<Option<CockpitInput>> {
        let mut pollfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
        if ready <= 0 {
            return Ok(None);
        }
        let mut buffer = [0u8; 8];
        let count = io::stdin().read(&mut buffer)?;
        if count == 0 {
            return Ok(None);
        }
        Ok(Self::decode(&buffer[..count]))
    }

    fn decode(bytes: &[u8]) -> Option<CockpitInput> {
        match bytes {
            [0x02] => Some(CockpitInput::Leader),
            [0x1b, b'[' | b'O', b'A', ..] | [b'k'] | [b'K'] => Some(CockpitInput::Up),
            [0x1b, b'[' | b'O', b'B', ..] | [b'j'] | [b'J'] => Some(CockpitInput::Down),
            [0x1b, b'[' | b'O', b'C', ..] | [b'l'] | [b'L'] => Some(CockpitInput::Right),
            [0x1b, b'[' | b'O', b'D', ..] | [b'h'] | [b'H'] => Some(CockpitInput::Left),
            [b'\t'] => Some(CockpitInput::Tab),
            [b'o'] | [b'O'] => Some(CockpitInput::NextPane),
            [b'z'] | [b'Z'] => Some(CockpitInput::ZoomPane),
            [b's'] | [b'S'] => Some(CockpitInput::ToggleSidebar),
            [b'1'] => Some(CockpitInput::FocusSpaces),
            [b'2'] => Some(CockpitInput::FocusActions),
            [b'3'] => Some(CockpitInput::FocusThemes),
            [b'p'] | [b'P'] => Some(CockpitInput::Profile),
            [b't'] => Some(CockpitInput::ThemeNext),
            [b'T'] => Some(CockpitInput::ThemePrev),
            [b'\r'] | [b'\n'] => Some(CockpitInput::Enter),
            [b'y'] | [b'Y'] => Some(CockpitInput::ConfirmYes),
            [b'n'] | [b'N'] => Some(CockpitInput::ConfirmNo),
            [0x1b] => Some(CockpitInput::Escape),
            [b'q'] | [b'Q'] | [0x03] => Some(CockpitInput::Quit),
            _ => None,
        }
    }
}

#[derive(Component, Clone)]
pub(crate) struct RunCockpit {
    pub root: PathBuf,
    pub color: bool,
    pub theme: ThemePreset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CockpitOutcome {
    pub action: CockpitAction,
    pub branch: Option<String>,
    pub profile: CockpitProfile,
    pub theme: ThemePreset,
}

impl Job for RunCockpit {
    type Output = Result<CockpitOutcome, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_job::blocking::BlockingCancellation::canceled() {
            return Err(Failure::interrupted(15));
        }
        let worktrees = Self::discover_worktrees(&self.root);
        let free_bytes = Self::free_bytes(&self.root);
        let mut state = CockpitState::new(worktrees, free_bytes, self.theme);
        let _guard = RawTerminalGuard::enter(true)
            .map_err(|error| Failure::message(format!("Failed to initialize TUI: {error}")))?;
        loop {
            if let Some(signal) = Signals::pending() {
                return Err(Failure::interrupted(signal));
            }
            if vmux_job::blocking::BlockingCancellation::canceled() {
                return Err(Failure::interrupted(15));
            }
            let (cols, rows) = Terminal::size();
            let frame = state.render(self.color, cols, rows);
            {
                let mut stderr = io::stderr().lock();
                let _ = write!(stderr, "\x1b[H");
                for (index, line) in frame.split('\n').enumerate() {
                    if index > 0 {
                        let _ = stderr.write_all(b"\r\n");
                    }
                    let _ = write!(stderr, "{line}\x1b[K");
                }
                let _ = stderr.write_all(b"\x1b[J");
                let _ = stderr.flush();
            }
            match KeyReader::poll(100) {
                Ok(Some(input)) => {
                    let previous_theme = state.theme;
                    let result = state.apply(input);
                    if state.theme != previous_theme {
                        let _ = ThemeStore::save(state.theme);
                    }
                    if let Some((action, branch, profile, theme)) = result {
                        return Ok(CockpitOutcome {
                            action,
                            branch,
                            profile,
                            theme,
                        });
                    }
                }
                Ok(None) => {
                    state.frame = state.frame.wrapping_add(1);
                }
                Err(error) => {
                    return Err(Failure::message(error.to_string()));
                }
            }
        }
    }
}

impl RunCockpit {
    fn discover_worktrees(root: &Path) -> Vec<(String, Option<String>, String, bool, bool)> {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let output = match Command::new("git")
            .arg("-C")
            .arg(&canonical)
            .args(["worktree", "list", "--porcelain", "-z"])
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return Vec::new(),
        };
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        let mut path = None::<PathBuf>;
        let mut branch = None::<String>;
        for entry in text.split('\0') {
            if let Some(value) = entry.strip_prefix("worktree ") {
                path = Some(PathBuf::from(value));
            } else if let Some(value) = entry.strip_prefix("branch refs/heads/") {
                branch = Some(value.to_owned());
            } else if entry.is_empty()
                && let Some(wt_path) = path.take()
            {
                let wt_canon = wt_path.canonicalize().unwrap_or(wt_path);
                let is_current = wt_canon == canonical;
                let folder = wt_canon
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("worktree")
                    .to_owned();
                let branch_arg = branch.take();
                let display = branch_arg
                    .clone()
                    .unwrap_or_else(|| format!("detached ({folder})"));
                let dirty = Self::is_dirty(&wt_canon);
                items.push((display, branch_arg, folder, is_current, dirty));
                if items.len() >= 32 {
                    break;
                }
            }
        }
        items
    }

    fn is_dirty(path: &Path) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["status", "--porcelain", "--untracked-files=no"])
            .output()
            .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
    }

    fn free_bytes(path: &Path) -> Option<u64> {
        let output = Command::new("df").args(["-Pk"]).arg(path).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = std::str::from_utf8(&output.stdout).ok()?;
        let line = text.lines().nth(1)?;
        let blocks = line.split_whitespace().nth(3)?.parse::<u64>().ok()?;
        blocks.checked_mul(1024)
    }
}

#[derive(Component, Clone)]
pub(crate) struct PromptConfirm {
    pub reclaimable_count: usize,
    pub free_bytes: u64,
    pub cache_bytes: u64,
    pub color: bool,
    pub width: u16,
    pub theme: ThemePreset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConfirmOutcome(pub bool);

impl Job for PromptConfirm {
    type Output = Result<ConfirmOutcome, Failure>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        if vmux_job::blocking::BlockingCancellation::canceled() {
            return Err(Failure::interrupted(15));
        }
        let _guard = RawTerminalGuard::enter(false)
            .map_err(|error| Failure::message(format!("Failed to initialize prompt: {error}")))?;
        let mut apply_selected = true;
        let mut frame = 0usize;
        let mut painted_lines = 0usize;
        loop {
            if let Some(signal) = Signals::pending() {
                Self::erase_lines(painted_lines);
                return Err(Failure::interrupted(signal));
            }
            if vmux_job::blocking::BlockingCancellation::canceled() {
                Self::erase_lines(painted_lines);
                return Err(Failure::interrupted(15));
            }
            let width = Terminal::size().0.max(self.width.min(60));
            let theme = Theme::new(self.theme, self.color, width);
            let card = theme.clean_prompt(
                frame,
                apply_selected,
                self.reclaimable_count,
                self.free_bytes,
                self.cache_bytes,
            );
            {
                let mut stderr = io::stderr().lock();
                if painted_lines > 0 {
                    let _ = write!(stderr, "\x1b[{painted_lines}A\x1b[J");
                }
                let _ = stderr.write_all(card.as_bytes());
                let _ = stderr.flush();
            }
            painted_lines = card.lines().count();
            match KeyReader::poll(100) {
                Ok(Some(
                    CockpitInput::Left
                    | CockpitInput::Right
                    | CockpitInput::Up
                    | CockpitInput::Down
                    | CockpitInput::Tab,
                )) => {
                    apply_selected = !apply_selected;
                }
                Ok(Some(CockpitInput::ConfirmYes)) => {
                    Self::erase_lines(painted_lines);
                    return Ok(ConfirmOutcome(true));
                }
                Ok(Some(CockpitInput::ConfirmNo | CockpitInput::Quit | CockpitInput::Escape)) => {
                    Self::erase_lines(painted_lines);
                    return Ok(ConfirmOutcome(false));
                }
                Ok(Some(CockpitInput::Enter)) => {
                    Self::erase_lines(painted_lines);
                    return Ok(ConfirmOutcome(apply_selected));
                }
                Ok(Some(
                    CockpitInput::Profile
                    | CockpitInput::ThemeNext
                    | CockpitInput::ThemePrev
                    | CockpitInput::FocusSpaces
                    | CockpitInput::FocusActions
                    | CockpitInput::FocusThemes
                    | CockpitInput::Leader
                    | CockpitInput::NextPane
                    | CockpitInput::ZoomPane
                    | CockpitInput::ToggleSidebar,
                ))
                | Ok(None) => {
                    frame = frame.wrapping_add(1);
                }
                Err(error) => {
                    Self::erase_lines(painted_lines);
                    return Err(Failure::message(error.to_string()));
                }
            }
        }
    }
}

impl PromptConfirm {
    fn erase_lines(lines: usize) {
        if lines == 0 {
            return;
        }
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\x1b[{lines}A\x1b[J");
        let _ = stderr.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_reader_decodes_arrow_vim_theme_and_action_sequences() {
        assert_eq!(KeyReader::decode(b"\x1b[A"), Some(CockpitInput::Up));
        assert_eq!(KeyReader::decode(b"\x1b[B"), Some(CockpitInput::Down));
        assert_eq!(KeyReader::decode(b"\x1b[C"), Some(CockpitInput::Right));
        assert_eq!(KeyReader::decode(b"\x1b[D"), Some(CockpitInput::Left));
        assert_eq!(KeyReader::decode(b"k"), Some(CockpitInput::Up));
        assert_eq!(KeyReader::decode(b"j"), Some(CockpitInput::Down));
        assert_eq!(KeyReader::decode(b"\t"), Some(CockpitInput::Tab));
        assert_eq!(KeyReader::decode(b"p"), Some(CockpitInput::Profile));
        assert_eq!(KeyReader::decode(b"t"), Some(CockpitInput::ThemeNext));
        assert_eq!(KeyReader::decode(b"T"), Some(CockpitInput::ThemePrev));
        assert_eq!(KeyReader::decode(b"\r"), Some(CockpitInput::Enter));
        assert_eq!(KeyReader::decode(b"\x02"), Some(CockpitInput::Leader));
        assert_eq!(KeyReader::decode(b"o"), Some(CockpitInput::NextPane));
        assert_eq!(KeyReader::decode(b"z"), Some(CockpitInput::ZoomPane));
        assert_eq!(KeyReader::decode(b"\x1b"), Some(CockpitInput::Escape));
        assert_eq!(KeyReader::decode(b"q"), Some(CockpitInput::Quit));
        assert_eq!(KeyReader::decode(b"\x03"), Some(CockpitInput::Quit));
        assert_eq!(KeyReader::decode(b"y"), Some(CockpitInput::ConfirmYes));
        assert_eq!(KeyReader::decode(b"n"), Some(CockpitInput::ConfirmNo));
    }
}
