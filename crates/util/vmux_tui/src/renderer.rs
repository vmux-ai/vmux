use std::collections::BTreeMap;
use std::fmt::Write as _;

use bevy::prelude::Entity;
use vmux_api::terminal::{
    FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_ITALIC, FLAG_STRIKETHROUGH, FLAG_UNDERLINE, TermColor,
    TermLine,
};
use vmux_ecs::event::TerminalViewportState;
use vmux_layout::state::LayoutUiState;

use crate::theme::{BarPosition, Palette, Rgb, Theme, ThemePreset};
use crate::world::{Divider, PaneView, Rect, SplitAxis};

#[derive(Clone, Debug, Default)]
pub struct HitMap {
    pub tab_row: Option<u16>,
    pub tab_ranges: Vec<(u16, u16, String)>,
    pub new_tab_range: Option<(u16, u16)>,
    pub sidebar_space_header_row: Option<u16>,
    pub sidebar_project_rows: Vec<(u16, String)>,
    pub sidebar_add_project_row: Option<u16>,
    pub sidebar_pane_rows: Vec<(u16, Entity)>,
    pub sidebar_x_end: u16,
}

pub struct FrameSpec<'a> {
    pub ui: &'a LayoutUiState,
    pub panes: &'a [PaneView],
    pub dividers: &'a [Divider],
    pub bar_position: BarPosition,
    pub theme: ThemePreset,
    pub palette: Option<Palette>,
    pub pane_gap: bool,
    pub zoomed: bool,
    pub color: bool,
    pub cols: u16,
    pub rows: u16,
    pub leader: bool,
    pub space_picker: Option<usize>,
}

pub struct TuiRenderer;

impl TuiRenderer {
    pub fn layout_rects(
        cols: u16,
        rows: u16,
        sidebar_open: bool,
        header_open: bool,
        bar_position: BarPosition,
    ) -> (Option<Rect>, Option<Rect>, Rect) {
        let width = cols.max(56);
        let height = rows.max(14);
        let show_sidebar = sidebar_open && width >= 78;
        let side_w = if show_sidebar {
            ((u32::from(width) * 20) / 100).clamp(20, 28) as u16
        } else {
            0
        };
        let sidebar = show_sidebar.then_some(Rect {
            x: 1,
            y: 1,
            w: side_w,
            h: height,
        });
        let main_x = if show_sidebar { side_w + 1 } else { 1 };
        let main_w = width.saturating_sub(main_x.saturating_sub(1)).max(20);
        if !header_open {
            let workspace = Rect {
                x: main_x,
                y: 1,
                w: main_w,
                h: height.max(4),
            };
            return (sidebar, None, workspace);
        }
        let (tab_y, ws_y) = match bar_position {
            BarPosition::Top => (1, 2),
            BarPosition::Bottom => (height.saturating_sub(1).max(2), 1),
        };
        let bar = Rect {
            x: main_x,
            y: tab_y,
            w: main_w,
            h: 1,
        };
        let workspace = Rect {
            x: main_x,
            y: ws_y,
            w: main_w,
            h: height.saturating_sub(2).max(4),
        };
        (sidebar, Some(bar), workspace)
    }

    pub fn render(spec: &FrameSpec<'_>) -> (String, HitMap) {
        let theme = match spec.palette {
            Some(palette) => Theme::with_palette(spec.theme, palette, spec.color),
            None => Theme::new(spec.theme, spec.color),
        };
        let sidebar_open = spec
            .ui
            .layout
            .map(|layout| layout.side_sheet_open)
            .unwrap_or(true);
        let header_open = spec
            .ui
            .layout
            .map(|layout| layout.header_open)
            .unwrap_or(true);
        let (sidebar_rect, tab_bar_rect, workspace_rect) = Self::layout_rects(
            spec.cols,
            spec.rows,
            sidebar_open,
            header_open,
            spec.bar_position,
        );
        let mut out = String::with_capacity(usize::from(spec.cols) * usize::from(spec.rows) * 3);
        let mut hits = HitMap::default();
        out.push_str("\x1b[?25l\x1b[0m");
        if let Some(side) = sidebar_rect {
            Self::draw_sidebar(&mut out, side, spec.ui, theme, &mut hits);
        }
        if let Some(bar) = tab_bar_rect {
            Self::draw_tab_bar(&mut out, bar, spec, theme, &mut hits);
            let status_rect = Rect {
                x: bar.x,
                y: spec.rows,
                w: bar.w,
                h: 1,
            };
            Self::draw_status_bar(&mut out, status_rect, spec, theme);
        }
        let focused_rect = spec.panes.iter().find(|p| p.focused).map(|p| p.rect);
        let mut cursor_pos = None;
        for pane in spec.panes {
            if spec.pane_gap {
                Self::draw_boxed_pane(&mut out, pane, theme);
            } else {
                Self::draw_split_pane(&mut out, pane, theme);
            }
            if pane.focused
                && spec.space_picker.is_none()
                && let Some(viewport) = &pane.viewport
            {
                cursor_pos = Self::cursor_coords(pane.rect, viewport, spec.pane_gap);
            }
        }
        if !spec.pane_gap {
            Self::draw_dividers(&mut out, workspace_rect, spec.dividers, focused_rect, theme);
        }
        if let Some(selected) = spec.space_picker {
            Self::draw_space_switcher(&mut out, workspace_rect, spec.ui, selected, theme);
        }
        if tab_bar_rect.is_none() && spec.leader {
            Self::draw_leader_overlay(&mut out, spec.cols, spec.rows, theme);
        }
        if let Some((cy, cx)) = cursor_pos {
            let _ = write!(out, "\x1b[{cy};{cx}H\x1b[?25h");
        }
        (out, hits)
    }

    fn draw_sidebar(
        out: &mut String,
        rect: Rect,
        ui: &LayoutUiState,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        hits.sidebar_x_end = rect.x + rect.w.saturating_sub(1);
        let inner = usize::from(rect.w);
        let active_space = ui
            .side_sheet
            .as_ref()
            .and_then(|sheet| sheet.active_space.as_ref())
            .or_else(|| ui.spaces.spaces.iter().find(|space| space.is_active))
            .map(|space| space.name.as_str())
            .unwrap_or("vmux");
        let mut rows = Vec::new();
        hits.sidebar_space_header_row = Some(rect.y);
        rows.push(theme.surface_row(
            &format!(
                " {} {} {}",
                theme.primary("●"),
                theme.bold(active_space),
                theme.muted("▾")
            ),
            inner,
        ));
        Self::push_active_session(&mut rows, rect.y, ui, inner, theme, hits);
        Self::push_sidebar_panes(&mut rows, rect.y, ui, inner, theme, hits);
        for row_idx in 0..rect.h {
            let y = rect.y + row_idx;
            let raw = rows
                .get(usize::from(row_idx))
                .map(String::as_str)
                .unwrap_or("");
            let line = theme.panel_row(raw, inner);
            let _ = write!(out, "\x1b[{y};{}H{line}", rect.x);
        }
    }

    fn push_active_session(
        rows: &mut Vec<String>,
        base_y: u16,
        ui: &LayoutUiState,
        inner: usize,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        if let Some(session) = &ui.active_session {
            let title = if session
                .task
                .as_ref()
                .is_some_and(|task| !task.name.trim().is_empty())
            {
                session.task.as_ref().unwrap().name.as_str()
            } else if !session.page.title.trim().is_empty() {
                session.page.title.as_str()
            } else {
                session.page.url.as_str()
            };
            let fitted_title = Theme::fit_title(title, inner.saturating_sub(3));
            rows.push(format!("  {}", theme.bold(&fitted_title)));
            if !session.page.url.is_empty() {
                let fitted_url = Theme::fit_title(&session.page.url, inner.saturating_sub(3));
                rows.push(format!("  {}", theme.muted(&fitted_url)));
            }
        }
        Self::push_sidebar_projects(rows, base_y, ui, inner, theme, hits);
    }

    fn push_sidebar_panes(
        rows: &mut Vec<String>,
        base_y: u16,
        ui: &LayoutUiState,
        inner: usize,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        let Some(sheet) = &ui.side_sheet else {
            return;
        };
        if sheet.panes.is_empty() {
            return;
        }
        rows.push(String::new());
        for (index, pane) in sheet.panes.iter().enumerate() {
            let header = format!(" Pane {}", index + 1);
            if pane.is_active {
                rows.push(theme.bold(&header));
            } else {
                rows.push(theme.muted(&header));
            }
            for stack in &pane.stacks {
                let row_y = base_y + rows.len() as u16;
                hits.sidebar_pane_rows
                    .push((row_y, Entity::from_bits(pane.id)));
                let active = pane.is_active && stack.is_active;
                let glyph = if active {
                    theme.primary("▸")
                } else {
                    theme.muted("·")
                };
                let fitted_title = Theme::fit_title(&stack.title, inner.saturating_sub(5));
                let label = format!("  {glyph} {fitted_title}");
                if active {
                    rows.push(theme.surface_row(&theme.bold(&label), inner));
                } else {
                    rows.push(theme.muted(&label));
                }
            }
        }
    }

    fn push_sidebar_projects(
        rows: &mut Vec<String>,
        base_y: u16,
        ui: &LayoutUiState,
        inner: usize,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        let active_project = ui
            .active_session
            .as_ref()
            .and_then(|session| session.project.as_ref());
        let mut items: Vec<(&str, &str, &str, bool)> = Vec::new();
        if let Some(project) = active_project {
            let active_path = project.root.path.as_str();
            for choice in &project.choices {
                let label = if !choice.label.is_empty() {
                    choice.label.as_str()
                } else {
                    choice.display_path.as_str()
                };
                let is_active = choice.is_active || choice.path == active_path;
                let branch = if is_active {
                    project.branch.as_str()
                } else {
                    choice.branch.as_str()
                };
                items.push((choice.path.as_str(), label, branch, is_active));
            }
            if items.is_empty() && !project.root.path.is_empty() {
                let label = if !project.root.label.is_empty() {
                    project.root.label.as_str()
                } else {
                    project.root.display_path.as_str()
                };
                items.push((
                    project.root.path.as_str(),
                    label,
                    project.branch.as_str(),
                    true,
                ));
            }
        }
        if items.is_empty() {
            for project in &ui.projects.projects {
                if project.depth != 0
                    || project.missing
                    || project.kind != vmux_api::space::ProjectRowKind::Project
                {
                    continue;
                }
                let label = if !project.label.is_empty() {
                    project.label.as_str()
                } else {
                    project.display_path.as_str()
                };
                items.push((
                    project.path.as_str(),
                    label,
                    project.branch.as_str(),
                    project.is_active,
                ));
            }
        }
        rows.push(String::new());
        for (path, label, branch, is_active) in items.into_iter().take(8) {
            let row_y = base_y + rows.len() as u16;
            hits.sidebar_project_rows.push((row_y, path.to_owned()));
            let dot = if is_active {
                theme.primary("●")
            } else {
                theme.muted("○")
            };
            let branch_text = if !branch.is_empty() {
                format!(" {}", theme.muted(branch))
            } else {
                String::new()
            };
            let line = format!(" {dot} {label}{branch_text}");
            if is_active {
                rows.push(theme.surface_row(&theme.bold(&line), inner));
            } else {
                rows.push(theme.muted(&line));
            }
        }
        let add_y = base_y + rows.len() as u16;
        hits.sidebar_add_project_row = Some(add_y);
        rows.push(theme.muted(" + Add project"));
    }

    fn draw_tab_bar(
        out: &mut String,
        rect: Rect,
        spec: &FrameSpec<'_>,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        hits.tab_row = Some(rect.y);
        let p = theme.palette();
        let width = usize::from(rect.w);
        let mut cursor_x = rect.x;
        let mut parts = Vec::new();
        if let Some(strip) = &spec.ui.tab_strip {
            for (index, row) in strip.tabs.iter().enumerate() {
                let short = Theme::fit_title(&row.display_title, 22);
                let label = format!(" {} {short} ", index + 1);
                let w = Theme::display_width(&label) as u16;
                hits.tab_ranges
                    .push((cursor_x, cursor_x + w, row.tab.id.clone()));
                if row.tab.is_active {
                    parts.push(format!(
                        "{}{}",
                        theme.fg_bg_bold(p.fg, p.bg, &label),
                        theme.fg_bg(p.bg, p.header, "\u{e0b0}")
                    ));
                    cursor_x = cursor_x.saturating_add(w + 1);
                } else {
                    parts.push(theme.fg_bg(p.muted, p.header, &label));
                    cursor_x = cursor_x.saturating_add(w);
                }
            }
        }
        hits.new_tab_range = Some((cursor_x, cursor_x.saturating_add(3)));
        let zoom = if spec.zoomed {
            format!(" {}", theme.fg_bg_bold(p.bg, p.gold, " ZOOM "))
        } else {
            String::new()
        };
        let line = theme.header_row(
            &format!("{}{}{zoom}", parts.join(""), theme.muted(" + ")),
            width,
        );
        let _ = write!(out, "\x1b[{};{}H{line}", rect.y, rect.x);
    }

    fn draw_status_bar(out: &mut String, rect: Rect, spec: &FrameSpec<'_>, theme: Theme) {
        let p = theme.palette();
        let width = usize::from(rect.w);
        let active_space = spec
            .ui
            .side_sheet
            .as_ref()
            .and_then(|sheet| sheet.active_space.as_ref())
            .or_else(|| spec.ui.spaces.spaces.iter().find(|space| space.is_active))
            .map(|space| space.name.as_str())
            .unwrap_or("vmux");
        let (mode_bg, mode_label) = if spec.leader {
            (p.gold, " LEADER ".to_owned())
        } else {
            (p.primary, format!(" ● {active_space} "))
        };
        let seg1 = format!(
            "{}{}",
            theme.fg_bg_bold(p.bg, mode_bg, &mode_label),
            theme.fg_bg(mode_bg, p.surface, "\u{e0b0}")
        );
        let mid_text = if spec.leader {
            " %/\" split · s space · z zoom · b zen · u bar · g gap · x/q quit ".to_owned()
        } else {
            let raw_url = spec
                .panes
                .iter()
                .find(|pane| pane.focused)
                .map(|pane| pane.url.as_str())
                .unwrap_or("vmux://terminal/");
            format!(" {raw_url} ")
        };
        let seg2 = format!(
            "{}{}",
            theme.fg_bg(p.fg, p.surface, &mid_text),
            theme.fg_bg(p.surface, p.header, "\u{e0b0}")
        );
        let pane_count = spec.panes.len().max(1);
        let pane_idx = spec
            .panes
            .iter()
            .position(|pane| pane.focused)
            .map(|i| i + 1)
            .unwrap_or(1);
        let right = format!(
            "{}{}",
            theme.fg_bg(p.surface, p.header, "\u{e0b2}"),
            theme.fg_bg(p.fg, p.surface, &format!(" {pane_idx}/{pane_count} "))
        );
        let right_w = Theme::display_width(&right);
        let left_budget = width.saturating_sub(right_w);
        let left = Theme::truncate_ellipsis(&format!("{seg1}{seg2}"), left_budget);
        let pad = left_budget.saturating_sub(Theme::display_width(&left));
        let line = theme.header_row(&format!("{left}{}{right}", " ".repeat(pad)), width);
        let _ = write!(out, "\x1b[{};{}H{line}", rect.y, rect.x);
    }

    fn draw_leader_overlay(out: &mut String, cols: u16, rows: u16, theme: Theme) {
        let text = format!(
            " {}  {}",
            theme.fg_bold(theme.palette().gold, "leader"),
            theme.text("% vsplit · \" hsplit · s spaces · z zoom · b zen · u bar · g gap · t theme · x close · d detach")
        );
        let line = theme.header_row(&text, usize::from(cols));
        let _ = write!(out, "\x1b[{rows};1H{line}");
    }

    fn draw_space_switcher(
        out: &mut String,
        ws: Rect,
        ui: &LayoutUiState,
        selected: usize,
        theme: Theme,
    ) {
        let spaces = &ui.spaces.spaces;
        let box_w = ws.w.saturating_sub(4).clamp(32, 58);
        let list_len = spaces.len().clamp(1, 10) as u16;
        let box_h = (list_len + 4).min(ws.h.max(6));
        let bx = ws.x + ws.w.saturating_sub(box_w) / 2;
        let by = ws.y + ws.h.saturating_sub(box_h) / 2;
        let inner = usize::from(box_w.saturating_sub(2));
        let p = theme.palette();
        let title = " Spaces ";
        let fill = inner.saturating_sub(1 + title.len());
        let _ = write!(
            out,
            "\x1b[{by};{bx}H{}{}{}",
            theme.fg_bg(p.primary, p.header, "╭─"),
            theme.fg_bg_bold(p.fg, p.header, title),
            theme.fg_bg(p.primary, p.header, &format!("{}╮", "─".repeat(fill)))
        );
        let max_items = usize::from(box_h.saturating_sub(4)).max(1);
        let start = selected.saturating_sub(max_items / 2);
        for row_idx in 0..box_h.saturating_sub(2) {
            let y = by + 1 + row_idx;
            let content = if row_idx == box_h.saturating_sub(3) {
                theme.header_row(
                    &theme.muted(" ↑↓/Ctrl+n/p move · ↵ switch · esc close"),
                    inner,
                )
            } else if let Some(space) = spaces.get(start + usize::from(row_idx)) {
                let is_sel = start + usize::from(row_idx) == selected;
                let dot = if space.is_active {
                    theme.primary("●")
                } else {
                    theme.muted("○")
                };
                let cursor = if is_sel {
                    theme.primary("›")
                } else {
                    " ".into()
                };
                let label = if is_sel {
                    theme.bold(&space.name)
                } else {
                    theme.text(&space.name)
                };
                let raw = format!(
                    " {cursor} {dot} {label}  {}",
                    theme.muted(&space.startup_dir)
                );
                if is_sel {
                    theme.surface_row(&raw, inner)
                } else {
                    theme.header_row(&raw, inner)
                }
            } else {
                theme.header_row("", inner)
            };
            let _ = write!(
                out,
                "\x1b[{y};{bx}H{}{content}{}",
                theme.fg_bg(p.primary, p.header, "│"),
                theme.fg_bg(p.primary, p.header, "│")
            );
        }
        let bottom_y = by + box_h.saturating_sub(1);
        let _ = write!(
            out,
            "\x1b[{bottom_y};{bx}H{}",
            theme.fg_bg(p.primary, p.header, &format!("╰{}╯", "─".repeat(inner)))
        );
    }

    fn visible_line(
        viewport: &TerminalViewportState,
        screen_row: u16,
        pane_rows: u16,
    ) -> Option<&TermLine> {
        let base = viewport
            .total_rows
            .saturating_sub(u32::from(pane_rows))
            .max(viewport.first_row);
        let target = base + u32::from(screen_row);
        viewport
            .rows
            .iter()
            .find(|(row, _)| *row == target)
            .map(|(_, line)| line)
    }

    fn cursor_coords(
        rect: Rect,
        viewport: &TerminalViewportState,
        pane_gap: bool,
    ) -> Option<(u16, u16)> {
        let cursor = viewport.cursor.as_ref()?;
        if !cursor.visible {
            return None;
        }
        let inset = u16::from(pane_gap);
        let inner_w = rect.w.saturating_sub(inset * 2);
        let inner_h = rect.h.saturating_sub(inset * 2);
        if inner_w == 0 || inner_h == 0 {
            return None;
        }
        let base = viewport
            .total_rows
            .saturating_sub(u32::from(inner_h))
            .max(viewport.first_row);
        let rel_row = cursor.row.saturating_sub(base).min(u32::from(inner_h - 1)) as u16;
        let cx = rect.x + inset + cursor.col.min(inner_w - 1);
        let cy = rect.y + inset + rel_row;
        Some((cy, cx))
    }

    fn draw_split_pane(out: &mut String, pane: &PaneView, theme: Theme) {
        let p = theme.palette();
        let cols = usize::from(pane.rect.w);
        let paint_bg = theme.color;
        for row_idx in 0..pane.rect.h {
            let y = pane.rect.y + row_idx;
            let content = match pane
                .viewport
                .as_ref()
                .and_then(|vp| Self::visible_line(vp, row_idx, pane.rect.h))
            {
                Some(line) => Self::render_term_line(line, cols, p, theme.color, paint_bg),
                None => Self::blank_term_line(cols, p, paint_bg),
            };
            let _ = write!(out, "\x1b[{y};{}H{content}\x1b[0m", pane.rect.x);
        }
    }

    fn draw_boxed_pane(out: &mut String, pane: &PaneView, theme: Theme) {
        let p = theme.palette();
        let border = if pane.focused {
            p.primary
        } else {
            p.bg.lift(p.fg, 22)
        };
        let inner_w = usize::from(pane.rect.w.saturating_sub(2));
        let inner_h = pane.rect.h.saturating_sub(2);
        let paint_bg = theme.color;
        let title_plain = format!(
            " {} ",
            Theme::fit_title(&pane.title, inner_w.saturating_sub(4))
        );
        let title = Theme::truncate_ellipsis(&title_plain, inner_w.saturating_sub(2));
        let title_styled = if pane.focused {
            theme.fg_bg_bold(p.fg, p.bg, &title)
        } else {
            theme.fg_bg(p.muted, p.bg, &title)
        };
        let fill = inner_w.saturating_sub(1 + Theme::display_width(&title));
        let _ = write!(
            out,
            "\x1b[{};{}H{}{title_styled}{}",
            pane.rect.y,
            pane.rect.x,
            theme.fg_bg(border, p.bg, "╭─"),
            theme.fg_bg(border, p.bg, &format!("{}╮", "─".repeat(fill)))
        );
        for row_idx in 0..inner_h {
            let y = pane.rect.y + 1 + row_idx;
            let content = match pane
                .viewport
                .as_ref()
                .and_then(|vp| Self::visible_line(vp, row_idx, inner_h))
            {
                Some(line) => Self::render_term_line(line, inner_w, p, theme.color, paint_bg),
                None => Self::blank_term_line(inner_w, p, paint_bg),
            };
            let _ = write!(
                out,
                "\x1b[{y};{}H{}{content}\x1b[0m{}",
                pane.rect.x,
                theme.fg_bg(border, p.bg, "│"),
                theme.fg_bg(border, p.bg, "│")
            );
        }
        let bottom_y = pane.rect.y + pane.rect.h.saturating_sub(1);
        let _ = write!(
            out,
            "\x1b[{bottom_y};{}H{}",
            pane.rect.x,
            theme.fg_bg(border, p.bg, &format!("╰{}╯", "─".repeat(inner_w)))
        );
    }

    fn draw_dividers(
        out: &mut String,
        ws: Rect,
        dividers: &[Divider],
        focused: Option<Rect>,
        theme: Theme,
    ) {
        let mut cells: BTreeMap<(u16, u16), (u8, bool)> = BTreeMap::new();
        for div in dividers {
            match div.axis {
                SplitAxis::Vertical => {
                    for dy in 0..div.len {
                        let x = div.x;
                        let y = div.y + dy;
                        let entry = cells.entry((x, y)).or_insert((0, false));
                        entry.0 |= 0b0011;
                        entry.1 |= Self::touches_focused(x, y, focused);
                    }
                }
                SplitAxis::Horizontal => {
                    for dx in 0..div.len {
                        let x = div.x + dx;
                        let y = div.y;
                        let entry = cells.entry((x, y)).or_insert((0, false));
                        entry.0 |= 0b1100;
                        entry.1 |= Self::touches_focused(x, y, focused);
                    }
                }
            }
        }
        let p = theme.palette();
        for (&(x, y), &(mask, active)) in &cells {
            let up = y > ws.y && cells.contains_key(&(x, y - 1));
            let down = cells.contains_key(&(x, y + 1));
            let left = x > ws.x && cells.contains_key(&(x - 1, y));
            let right = cells.contains_key(&(x + 1, y));
            let glyph = match (mask, up, down, left, right) {
                (0b1111, true, true, true, true) => "┼",
                (0b1111, false, true, true, true) => "┬",
                (0b1111, true, false, true, true) => "┴",
                (0b1111, true, true, false, true) => "├",
                (0b1111, true, true, true, false) => "┤",
                (0b1111, ..) => "┼",
                (0b0011, _, _, true, true) => "┼",
                (0b0011, _, _, true, false) => "┤",
                (0b0011, _, _, false, true) => "├",
                (0b0011, ..) => "│",
                (0b1100, true, true, _, _) => "┼",
                (0b1100, true, false, _, _) => "┴",
                (0b1100, false, true, _, _) => "┬",
                _ => "─",
            };
            let color = if active {
                p.primary
            } else {
                p.bg.lift(p.fg, 22)
            };
            let _ = write!(out, "\x1b[{y};{x}H{}", theme.fg_bg(color, p.bg, glyph));
        }
    }

    fn touches_focused(x: u16, y: u16, focused: Option<Rect>) -> bool {
        let Some(rect) = focused else {
            return false;
        };
        let adj_x = x + 1 == rect.x || x == rect.x + rect.w;
        let in_y = y >= rect.y && y < rect.y + rect.h;
        let adj_y = y + 1 == rect.y || y == rect.y + rect.h;
        let in_x = x >= rect.x && x < rect.x + rect.w;
        (adj_x && in_y) || (adj_y && in_x)
    }

    fn blank_term_line(cols: usize, palette: Palette, paint_bg: bool) -> String {
        if paint_bg {
            let Rgb(r, g, b) = palette.bg;
            format!("\x1b[48;2;{r};{g};{b}m{}", " ".repeat(cols))
        } else {
            " ".repeat(cols)
        }
    }

    fn render_term_line(
        line: &TermLine,
        max_cols: usize,
        palette: Palette,
        color: bool,
        paint_bg: bool,
    ) -> String {
        let mut out = String::new();
        let bg_code = if paint_bg {
            let Rgb(r, g, b) = palette.bg;
            format!("\x1b[48;2;{r};{g};{b}m")
        } else {
            String::new()
        };
        if paint_bg {
            out.push_str(&bg_code);
        }
        let mut col = 0usize;
        for span in &line.spans {
            let start = usize::from(span.col);
            if start > col && col < max_cols {
                let gap = start.min(max_cols) - col;
                out.push_str(&" ".repeat(gap));
                col += gap;
            }
            if col >= max_cols {
                break;
            }
            if color {
                Self::write_span_style(&mut out, &span.fg, &span.bg, span.flags, palette, paint_bg);
            }
            for ch in span.text.chars() {
                let w = Theme::char_width(ch);
                if col + w > max_cols {
                    break;
                }
                out.push(ch);
                col += w;
            }
            if color {
                out.push_str("\x1b[0m");
                if paint_bg {
                    out.push_str(&bg_code);
                }
            }
        }
        if col < max_cols {
            out.push_str(&" ".repeat(max_cols - col));
        }
        out
    }

    fn write_span_style(
        out: &mut String,
        fg: &TermColor,
        bg: &TermColor,
        flags: u16,
        palette: Palette,
        paint_bg: bool,
    ) {
        out.push_str("\x1b[0");
        if flags & FLAG_BOLD != 0 {
            out.push_str(";1");
        }
        if flags & FLAG_DIM != 0 {
            out.push_str(";2");
        }
        if flags & FLAG_ITALIC != 0 {
            out.push_str(";3");
        }
        if flags & FLAG_UNDERLINE != 0 {
            out.push_str(";4");
        }
        if flags & FLAG_INVERSE != 0 {
            out.push_str(";7");
        }
        if flags & FLAG_STRIKETHROUGH != 0 {
            out.push_str(";9");
        }
        let resolved_fg =
            Self::resolve_color(fg, palette).or_else(|| paint_bg.then_some(palette.fg));
        if let Some(Rgb(r, g, b)) = resolved_fg {
            let _ = write!(out, ";38;2;{r};{g};{b}");
        }
        let resolved_bg =
            Self::resolve_color(bg, palette).or_else(|| paint_bg.then_some(palette.bg));
        if let Some(Rgb(r, g, b)) = resolved_bg {
            let _ = write!(out, ";48;2;{r};{g};{b}");
        }
        out.push('m');
    }

    fn resolve_color(color: &TermColor, palette: Palette) -> Option<Rgb> {
        match *color {
            TermColor::Default => None,
            TermColor::Indexed(index) => Some(palette.ansi(index)),
            TermColor::Rgb(r, g, b) => Some(Rgb(r, g, b)),
        }
    }
}
