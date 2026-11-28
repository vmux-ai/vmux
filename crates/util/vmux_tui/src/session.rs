use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};

use crate::input::{DecodedKey, MouseEvent, RawGuard, TerminalIo};
use crate::io::TuiFs;
use crate::renderer::{FrameSpec, HitMap, TuiRenderer};
use crate::theme::{BarPosition, Theme, ThemePreset, ThemeStore};
use crate::world::{DesktopConfig, KeyOutcome, PaneView, TuiWorld};

pub struct TuiPlugin;

impl Plugin for TuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle);
    }
}

fn handle(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        if !invocation.is("dx.tui") {
            continue;
        }
        let result = TuiSession::execute(invocation);
        commands.entity(entity).insert(result);
    }
}

struct TuiSession {
    world: TuiWorld,
    store_dir: PathBuf,
    hits: HitMap,
    panes: Vec<PaneView>,
    space_picker: Option<usize>,
    bar_position: BarPosition,
    theme: ThemePreset,
    explicit_theme: bool,
    focus_follows_mouse: bool,
    color: bool,
    cols: u16,
    rows: u16,
    full_clear: bool,
    last_frame: String,
    seen_pane: bool,
    pending_input: Vec<u8>,
}

impl TuiSession {
    fn execute(invocation: &CliInvocation) -> CliResult {
        let cwd = match invocation.value_os("root") {
            Some(path) => PathBuf::from(path),
            None => match env::current_dir() {
                Ok(path) => path,
                Err(error) => return CliResult(Err(error.to_string())),
            },
        };
        let config = DesktopConfig::load(&cwd);
        let saved_theme = ThemeStore::load_explicit();
        let mut explicit_theme = saved_theme.is_some();
        let mut theme = saved_theme.unwrap_or_default();
        let color = io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none();
        if invocation.flag("list-themes") {
            let output = Theme::new(theme, color).theme_list(theme);
            let mut stdout = io::stdout().lock();
            let _ = stdout.write_all(output.as_bytes());
            let _ = stdout.flush();
            return CliResult::default();
        }
        if let Some(name) = invocation.value("theme") {
            let Some(preset) = ThemePreset::parse(name) else {
                return CliResult(Err(format!("Unknown TUI theme: {name}")));
            };
            if let Err(error) = ThemeStore::save(preset) {
                return CliResult(Err(format!("Failed to save TUI theme: {error}")));
            }
            theme = preset;
            explicit_theme = true;
            if !Self::is_tty() {
                let mut stdout = io::stdout().lock();
                let _ = writeln!(stdout, "CLI TUI theme set to {}", preset.id());
                return CliResult::default();
            }
        }
        if !Self::is_tty() {
            return CliResult(Err(
                "Interactive TUI requires a terminal (TTY); run explicit subcommands like vmux desktop, vmux build, or vmux cache status".into(),
            ));
        }
        match Self::start(theme, explicit_theme, color, config).and_then(Self::run) {
            Ok(()) => CliResult::default(),
            Err(error) => CliResult(Err(error)),
        }
    }

    fn is_tty() -> bool {
        let ci = env::var_os("CI").is_some_and(|value| !value.is_empty());
        let dumb = env::var_os("TERM").is_some_and(|value| value == "dumb");
        let disabled = env::var_os("VMUX_NO_TUI")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        io::stdin().is_terminal()
            && io::stdout().is_terminal()
            && io::stderr().is_terminal()
            && !ci
            && !dumb
            && !disabled
    }

    fn start(
        mut theme: ThemePreset,
        explicit_theme: bool,
        color: bool,
        config: DesktopConfig,
    ) -> Result<Self, String> {
        let (cols, rows) = TerminalIo::size();
        let store_dir = env::temp_dir().join(format!("vmux-tui-{}", std::process::id()));
        TuiFs::ensure_dir(&store_dir);
        let bar_position = ThemeStore::load_bar_position();
        let mut world = TuiWorld::boot(cols, rows, store_dir.clone(), config, true);
        let focus_follows_mouse = world.focus_follows_mouse();
        if !explicit_theme {
            let base = world.configured_theme().unwrap_or(theme);
            theme = base.for_scheme(world.is_light_mode());
        }
        Ok(Self {
            world,
            store_dir,
            hits: HitMap::default(),
            panes: Vec::new(),
            space_picker: None,
            bar_position,
            theme,
            explicit_theme,
            focus_follows_mouse,
            color,
            cols,
            rows,
            full_clear: true,
            last_frame: String::new(),
            seen_pane: false,
            pending_input: Vec::new(),
        })
    }

    fn run(mut self) -> Result<(), String> {
        let _guard =
            RawGuard::enter(self.focus_follows_mouse).map_err(|error| error.to_string())?;
        let mut buffer = [0u8; 256];
        let mut last_render = Instant::now() - Duration::from_secs(1);
        loop {
            let (new_cols, new_rows) = TerminalIo::size();
            if (new_cols, new_rows) != (self.cols, self.rows) {
                self.cols = new_cols;
                self.rows = new_rows;
                self.world.resize(new_cols, new_rows);
                self.full_clear = true;
            }
            self.world.tick();
            if let Some(reason) = self.world.service_unavailable() {
                TuiFs::remove_dir(&self.store_dir);
                return Err(reason);
            }
            if self.world.take_space_picker_requested() {
                let ui = self.world.layout_ui().unwrap_or_default();
                let active = ui
                    .spaces
                    .spaces
                    .iter()
                    .position(|space| space.is_active)
                    .unwrap_or(0);
                self.space_picker = Some(active);
                self.full_clear = true;
            }
            if self.world.exit_requested() {
                break;
            }
            let panes = self.world.pane_count();
            if panes > 0 {
                self.seen_pane = true;
            } else if self.seen_pane {
                break;
            }
            if last_render.elapsed() >= Duration::from_millis(16) {
                self.render_frame();
                last_render = Instant::now();
            }
            let Some(count) = TerminalIo::poll_stdin(&mut buffer, 16) else {
                break;
            };
            if count == 0 {
                if !self.pending_input.is_empty() {
                    let buffered = std::mem::take(&mut self.pending_input);
                    if self.process_input(&buffered, true) {
                        break;
                    }
                    self.render_frame();
                    last_render = Instant::now();
                }
                continue;
            }
            let mut combined = std::mem::take(&mut self.pending_input);
            let remaining = 4096usize.saturating_sub(combined.len());
            combined.extend_from_slice(&buffer[..count.min(remaining)]);
            if self.process_input(&combined, false) {
                break;
            }
            self.render_frame();
            last_render = Instant::now();
        }
        TuiFs::remove_dir(&self.store_dir);
        Ok(())
    }

    fn render_frame(&mut self) {
        let ui = self.world.layout_ui().unwrap_or_default();
        let sidebar_open = ui
            .layout
            .map(|layout| layout.side_sheet_open)
            .unwrap_or(true);
        let header_open = ui.layout.map(|layout| layout.header_open).unwrap_or(true);
        let (_, _, workspace) = TuiRenderer::layout_rects(
            self.cols,
            self.rows,
            sidebar_open,
            header_open,
            self.bar_position,
        );
        let pane_gap = self.world.pane_gap();
        let (panes, dividers, _) = if TuiRenderer::is_too_small(self.cols, self.rows) {
            (Vec::new(), Vec::new(), None)
        } else {
            let result = self.world.pane_views(workspace, pane_gap);
            self.world.sync_terminal_sizes(&result.0, pane_gap);
            result
        };
        let palette = (!self.explicit_theme)
            .then(|| self.world.terminal_palette())
            .flatten();
        let spec = FrameSpec {
            ui: &ui,
            panes: &panes,
            dividers: &dividers,
            bar_position: self.bar_position,
            theme: self.theme,
            palette,
            pane_gap,
            zoomed: self.world.is_zoomed(),
            color: self.color,
            cols: self.cols,
            rows: self.rows,
            leader: self.world.leader_active(),
            space_picker: self.space_picker,
        };
        let (frame, hits) = TuiRenderer::render(&spec);
        self.hits = hits;
        self.panes = panes;
        if !self.full_clear && frame == self.last_frame {
            return;
        }
        let mut stdout = io::stdout().lock();
        if self.full_clear {
            let _ = stdout.write_all(b"\x1b[2J");
            self.full_clear = false;
        }
        let _ = stdout.write_all(frame.as_bytes());
        let _ = stdout.flush();
        self.last_frame = frame;
    }

    fn process_input(&mut self, bytes: &[u8], flush: bool) -> bool {
        let mut idx = 0usize;
        while idx < bytes.len() {
            if !flush && TerminalIo::incomplete_escape_tail(&bytes[idx..]).is_some() {
                self.pending_input.extend_from_slice(&bytes[idx..]);
                break;
            }
            if let Some(mouse) = MouseEvent::parse(&bytes[idx..]) {
                self.handle_mouse(&mouse);
                idx += mouse.consumed;
                continue;
            }
            if self.space_picker.is_some() {
                idx += self.handle_space_picker_key(&bytes[idx..]);
                continue;
            }
            if let Some(decoded) = DecodedKey::parse(&bytes[idx..]) {
                let slice = &bytes[idx..idx + decoded.consumed];
                match self.world.press_key(decoded) {
                    KeyOutcome::Consumed => {
                        self.full_clear = true;
                    }
                    KeyOutcome::UnmatchedLeader => {
                        if self.handle_tui_extra(slice) {
                            return true;
                        }
                    }
                    KeyOutcome::Forward => {
                        let pty = decoded.pty_bytes(slice);
                        if !pty.is_empty()
                            && let Some((terminal, pid)) = self.world.focused_terminal()
                        {
                            self.world.clear_scroll(terminal);
                            self.world.send_terminal_input(pid, pty);
                        }
                    }
                }
                idx += decoded.consumed;
            } else {
                if let Some((terminal, pid)) = self.world.focused_terminal() {
                    self.world.clear_scroll(terminal);
                    self.world
                        .send_terminal_input(pid, bytes[idx..idx + 1].to_vec());
                }
                idx += 1;
            }
        }
        false
    }

    fn handle_tui_extra(&mut self, bytes: &[u8]) -> bool {
        match bytes {
            [b'd'] | [b'q'] => return true,
            [b'b'] => {
                self.world.toggle_side_sheet();
                self.full_clear = true;
            }
            [b'u'] => {
                self.bar_position = self.bar_position.toggle();
                let _ = ThemeStore::save_bar_position(self.bar_position);
                self.full_clear = true;
            }
            [b'g'] => {
                self.world.toggle_pane_gap();
                self.full_clear = true;
            }
            [b't'] => {
                self.theme = self.theme.next();
                self.explicit_theme = true;
                let _ = ThemeStore::save(self.theme);
                self.full_clear = true;
            }
            _ => {}
        }
        false
    }

    fn handle_space_picker_key(&mut self, bytes: &[u8]) -> usize {
        let ui = self.world.layout_ui().unwrap_or_default();
        let spaces = &ui.spaces.spaces;
        let len = spaces.len();
        let Some(selected) = self.space_picker else {
            return 1;
        };
        match bytes {
            [0x1b, b'[' | b'O', b'A', ..] => {
                if len > 0 {
                    self.space_picker = Some((selected + len - 1) % len);
                }
                3
            }
            [0x1b, b'[' | b'O', b'B', ..] => {
                if len > 0 {
                    self.space_picker = Some((selected + 1) % len);
                }
                3
            }
            [0x1b] | [b'q'] => {
                self.space_picker = None;
                self.full_clear = true;
                1
            }
            [b'k'] | [0x10] => {
                if len > 0 {
                    self.space_picker = Some((selected + len - 1) % len);
                }
                1
            }
            [b'j'] | [0x0e] => {
                if len > 0 {
                    self.space_picker = Some((selected + 1) % len);
                }
                1
            }
            [b'\r'] | [b'\n'] => {
                if let Some(space) = spaces.get(selected) {
                    self.world.send_space_attach(space.id.clone());
                }
                self.space_picker = None;
                self.full_clear = true;
                1
            }
            _ => 1,
        }
    }

    fn handle_mouse(&mut self, mouse: &MouseEvent) {
        if mouse.button == 64 || mouse.button == 65 {
            let pane_gap = self.world.pane_gap();
            if let Some(pane) = self.pane_at(mouse.col, mouse.row)
                && let Some(terminal) = pane.terminal
            {
                let delta = if mouse.button == 64 { -3 } else { 3 };
                let pane_rows = pane.rect.pty_size(pane_gap).1;
                self.world.scroll_terminal(terminal, delta, pane_rows);
            }
            return;
        }
        let is_motion = mouse.button & 32 != 0;
        if is_motion {
            if self.focus_follows_mouse
                && self.space_picker.is_none()
                && let Some(pane) = self.pane_at(mouse.col, mouse.row)
                && !pane.focused
            {
                self.world.select_pane(pane.pane);
            }
            return;
        }
        if !mouse.pressed || mouse.button != 0 {
            return;
        }
        if Some(mouse.row) == self.hits.tab_row {
            for (start, end, id) in &self.hits.tab_ranges {
                if mouse.col >= *start && mouse.col < *end {
                    self.world.send_tab_activate(id.clone());
                    self.full_clear = true;
                    return;
                }
            }
            if let Some((start, end)) = self.hits.new_tab_range
                && mouse.col >= start
                && mouse.col < end
            {
                self.world.send_tab_create();
                self.full_clear = true;
                return;
            }
        }
        if self.hits.sidebar_x_end > 0 && mouse.col <= self.hits.sidebar_x_end {
            if Some(mouse.row) == self.hits.sidebar_space_header_row {
                let ui = self.world.layout_ui().unwrap_or_default();
                let active = ui
                    .spaces
                    .spaces
                    .iter()
                    .position(|space| space.is_active)
                    .unwrap_or(0);
                self.space_picker = Some(active);
                self.full_clear = true;
                return;
            }
            for (row, path) in &self.hits.sidebar_project_rows {
                if *row == mouse.row {
                    self.world.send_project_activate(path.clone());
                    self.full_clear = true;
                    return;
                }
            }
            if Some(mouse.row) == self.hits.sidebar_add_project_row {
                self.world.send_project_picker();
                self.full_clear = true;
                return;
            }
            for (row, entity) in &self.hits.sidebar_pane_rows {
                if *row == mouse.row {
                    self.world.select_pane(*entity);
                    return;
                }
            }
            return;
        }
        if let Some(pane) = self.pane_at(mouse.col, mouse.row)
            && !pane.focused
        {
            self.world.select_pane(pane.pane);
        }
    }

    fn pane_at(&self, col: u16, row: u16) -> Option<&PaneView> {
        self.panes.iter().find(|pane| pane.rect.contains(col, row))
    }
}
