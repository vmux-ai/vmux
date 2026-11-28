use std::fmt::Write as _;

use serde_json::json;
use vmux_setting::state::{
    SettingsRenderField, SettingsRenderFieldId, SettingsRenderFieldKind, SettingsRenderItem,
    SettingsRenderItemId, SettingsSelectOption, SettingsUiState,
};

use crate::renderer::HitMap;
use crate::theme::{Theme, ThemePreset};
use crate::world::Rect;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsMutation {
    Update {
        path: String,
        value: serde_json::Value,
    },
    Edit {
        path: String,
        draft: String,
    },
    Filter {
        query: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusColumn {
    Sections,
    Fields,
}

#[derive(Clone, Debug)]
enum SettingsRow {
    Heading {
        label: String,
    },
    Toggle {
        path: String,
        label: String,
        hint: Option<String>,
        value: bool,
    },
    Select {
        path: String,
        label: String,
        hint: Option<String>,
        value: String,
        options: Vec<SettingsSelectOption>,
    },
    ColorScheme {
        path: String,
        label: String,
        hint: Option<String>,
        value: String,
    },
    Number {
        path: String,
        label: String,
        hint: Option<String>,
        value: String,
        step: f64,
        integer: bool,
    },
    Text {
        path: String,
        label: String,
        hint: Option<String>,
        value: String,
        placeholder: Option<String>,
    },
    ReadOnly {
        label: String,
        value: String,
        hint: Option<String>,
    },
    Binding {
        command: String,
        chord: String,
    },
}

impl SettingsRow {
    fn is_selectable(&self) -> bool {
        !matches!(self, Self::Heading { .. })
    }

    fn hint(&self) -> Option<&str> {
        match self {
            Self::Toggle { hint, .. }
            | Self::Select { hint, .. }
            | Self::ColorScheme { hint, .. }
            | Self::Number { hint, .. }
            | Self::Text { hint, .. }
            | Self::ReadOnly { hint, .. } => hint.as_deref(),
            Self::Heading { .. } | Self::Binding { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsModal {
    section_index: usize,
    row_index: usize,
    row_scroll: usize,
    focus: FocusColumn,
    searching: bool,
    search_query: String,
    editing: Option<(String, String)>,
}

impl SettingsModal {
    pub fn open() -> Self {
        Self {
            section_index: 0,
            row_index: 0,
            row_scroll: 0,
            focus: FocusColumn::Sections,
            searching: false,
            search_query: String::new(),
            editing: None,
        }
    }

    pub fn close_requested_on_key(
        &mut self,
        bytes: &[u8],
        state: &SettingsUiState,
    ) -> (usize, bool, Option<SettingsMutation>) {
        if self.searching {
            return self.handle_search_key(bytes);
        }
        if self.editing.is_some() {
            return self.handle_edit_key(bytes);
        }
        let rows = self.current_rows(state);
        self.clamp_indices(state.sections.len(), &rows);
        match bytes {
            [0x1b, b'[' | b'O', b'A', ..] => {
                self.move_vertical(-1, state.sections.len(), &rows);
                (3, false, None)
            }
            [0x1b, b'[' | b'O', b'B', ..] => {
                self.move_vertical(1, state.sections.len(), &rows);
                (3, false, None)
            }
            [0x1b, b'[' | b'O', b'C', ..] => (3, false, self.handle_horizontal(1, &rows)),
            [0x1b, b'[' | b'O', b'D', ..] => (3, false, self.handle_horizontal(-1, &rows)),
            [0x1b] | [b'q'] => (1, true, None),
            [b'\t'] => {
                self.focus = match self.focus {
                    FocusColumn::Sections => FocusColumn::Fields,
                    FocusColumn::Fields => FocusColumn::Sections,
                };
                if self.focus == FocusColumn::Fields {
                    self.select_first_interactive(&rows);
                }
                (1, false, None)
            }
            [b'h'] => {
                let mutation = self.handle_horizontal(-1, &rows);
                (1, false, mutation)
            }
            [b'l'] => {
                let mutation = self.handle_horizontal(1, &rows);
                (1, false, mutation)
            }
            [b'k'] | [0x10] => {
                self.move_vertical(-1, state.sections.len(), &rows);
                (1, false, None)
            }
            [b'j'] | [0x0e] => {
                self.move_vertical(1, state.sections.len(), &rows);
                (1, false, None)
            }
            [b'/'] => {
                self.searching = true;
                (1, false, None)
            }
            [b'\r'] | [b'\n'] | [b' '] => (1, false, self.activate_current(&rows)),
            _ => (1, false, None),
        }
    }

    pub fn handle_scroll(&mut self, delta: i32, state: &SettingsUiState) {
        let rows = self.current_rows(state);
        self.clamp_indices(state.sections.len(), &rows);
        if self.focus == FocusColumn::Sections {
            self.move_vertical(if delta < 0 { -1 } else { 1 }, state.sections.len(), &rows);
        } else {
            let step = if delta < 0 { -3 } else { 3 };
            self.move_row(step, &rows);
        }
    }

    pub fn handle_click(
        &mut self,
        col: u16,
        row: u16,
        hits: &HitMap,
        state: &SettingsUiState,
    ) -> (bool, Option<SettingsMutation>) {
        if let Some(modal) = hits.settings_modal_rect
            && !modal.contains(col, row)
        {
            return (true, None);
        }
        for &(y, section_idx) in &hits.settings_section_rows {
            if y == row && col < hits.settings_split_x {
                self.section_index = section_idx;
                self.row_index = 0;
                self.row_scroll = 0;
                self.editing = None;
                self.focus = FocusColumn::Sections;
                let rows = self.current_rows(state);
                self.select_first_interactive(&rows);
                return (false, None);
            }
        }
        for &(y, row_idx) in &hits.settings_field_rows {
            if y == row && col >= hits.settings_split_x {
                self.focus = FocusColumn::Fields;
                self.row_index = row_idx;
                self.editing = None;
                let rows = self.current_rows(state);
                return (false, self.activate_current(&rows));
            }
        }
        (false, None)
    }

    pub fn draw(
        &self,
        out: &mut String,
        ws: Rect,
        state: &SettingsUiState,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        if ws.w < 24 || ws.h < 8 {
            return;
        }
        let box_w = ws.w.saturating_sub(4).clamp(44, 96).min(ws.w);
        let box_h = ws.h.saturating_sub(2).clamp(12, 32).min(ws.h);
        let bx = ws.x + ws.w.saturating_sub(box_w) / 2;
        let by = ws.y + ws.h.saturating_sub(box_h) / 2;
        let modal_rect = Rect {
            x: bx,
            y: by,
            w: box_w,
            h: box_h,
        };
        hits.settings_modal_rect = Some(modal_rect);
        let inner_w = usize::from(box_w.saturating_sub(2));
        let side_w = if inner_w >= 52 {
            (inner_w * 26 / 100).clamp(16, 22)
        } else {
            0
        };
        let field_w = if side_w > 0 {
            inner_w.saturating_sub(side_w + 1)
        } else {
            inner_w
        };
        hits.settings_split_x = bx + 1 + side_w as u16;
        let body_h = usize::from(box_h.saturating_sub(4)).max(1);
        let rows = self.current_rows(state);
        let active_section = self
            .section_index
            .min(state.sections.len().saturating_sub(1));
        let active_row = self.row_index.min(rows.len().saturating_sub(1));
        let scroll = self.effective_scroll(active_row, body_h, rows.len());
        self.draw_frame_header(out, modal_rect, inner_w, theme);
        self.draw_body(
            out,
            modal_rect,
            side_w,
            field_w,
            body_h,
            state,
            active_section,
            &rows,
            active_row,
            scroll,
            theme,
            hits,
        );
        self.draw_frame_footer(out, modal_rect, inner_w, &rows, active_row, theme);
    }

    fn handle_search_key(&mut self, bytes: &[u8]) -> (usize, bool, Option<SettingsMutation>) {
        match bytes {
            [0x1b] => {
                self.searching = false;
                if self.search_query.is_empty() {
                    return (1, false, None);
                }
                self.search_query.clear();
                self.section_index = 0;
                self.row_index = 0;
                self.row_scroll = 0;
                (
                    1,
                    false,
                    Some(SettingsMutation::Filter {
                        query: String::new(),
                    }),
                )
            }
            [b'\r'] | [b'\n'] => {
                self.searching = false;
                (1, false, None)
            }
            [0x7f] | [0x08] => {
                self.search_query.pop();
                self.section_index = 0;
                self.row_index = 0;
                self.row_scroll = 0;
                (
                    1,
                    false,
                    Some(SettingsMutation::Filter {
                        query: self.search_query.clone(),
                    }),
                )
            }
            [byte, ..] if (0x20..=0x7e).contains(byte) => {
                self.search_query.push(*byte as char);
                self.section_index = 0;
                self.row_index = 0;
                self.row_scroll = 0;
                (
                    1,
                    false,
                    Some(SettingsMutation::Filter {
                        query: self.search_query.clone(),
                    }),
                )
            }
            _ => (1, false, None),
        }
    }

    fn handle_edit_key(&mut self, bytes: &[u8]) -> (usize, bool, Option<SettingsMutation>) {
        match bytes {
            [0x1b] => {
                self.editing = None;
                (1, false, None)
            }
            [b'\r'] | [b'\n'] => {
                let mutation = self
                    .editing
                    .take()
                    .map(|(path, draft)| SettingsMutation::Edit { path, draft });
                (1, false, mutation)
            }
            [0x7f] | [0x08] => {
                if let Some((_, draft)) = &mut self.editing {
                    draft.pop();
                }
                (1, false, None)
            }
            [byte, ..] if (0x20..=0x7e).contains(byte) => {
                if let Some((_, draft)) = &mut self.editing {
                    draft.push(*byte as char);
                }
                (1, false, None)
            }
            _ => (1, false, None),
        }
    }

    fn move_vertical(&mut self, delta: isize, section_count: usize, rows: &[SettingsRow]) {
        match self.focus {
            FocusColumn::Sections => {
                if section_count == 0 {
                    return;
                }
                let next = (self.section_index as isize + delta).rem_euclid(section_count as isize)
                    as usize;
                self.section_index = next;
                self.row_index = 0;
                self.row_scroll = 0;
            }
            FocusColumn::Fields => self.move_row(delta, rows),
        }
    }

    fn move_row(&mut self, delta: isize, rows: &[SettingsRow]) {
        if rows.is_empty() {
            return;
        }
        let step = if delta < 0 { -1isize } else { 1isize };
        let steps = delta.unsigned_abs();
        let mut idx = self.row_index.min(rows.len() - 1) as isize;
        for _ in 0..steps {
            let mut candidate = idx + step;
            while candidate >= 0
                && (candidate as usize) < rows.len()
                && !rows[candidate as usize].is_selectable()
            {
                candidate += step;
            }
            if candidate >= 0 && (candidate as usize) < rows.len() {
                idx = candidate;
            } else {
                break;
            }
        }
        self.row_index = idx as usize;
    }

    fn select_first_interactive(&mut self, rows: &[SettingsRow]) {
        if let Some(pos) = rows.iter().position(SettingsRow::is_selectable) {
            self.row_index = pos;
        } else {
            self.row_index = 0;
        }
    }

    fn handle_horizontal(
        &mut self,
        delta: isize,
        rows: &[SettingsRow],
    ) -> Option<SettingsMutation> {
        if self.focus == FocusColumn::Sections {
            if delta > 0 {
                self.focus = FocusColumn::Fields;
                self.select_first_interactive(rows);
            }
            return None;
        }
        let row = rows.get(self.row_index)?;
        match row {
            SettingsRow::Toggle { path, value, .. } => Some(SettingsMutation::Update {
                path: path.clone(),
                value: json!(!*value),
            }),
            SettingsRow::Select {
                path,
                value,
                options,
                ..
            } => Self::step_select(path, value, options, delta),
            SettingsRow::ColorScheme { path, value, .. } => {
                Self::step_color_scheme(path, value, delta)
            }
            SettingsRow::Number {
                path,
                value,
                step,
                integer,
                ..
            } => Self::step_number(path, value, *step, *integer, delta),
            _ => {
                if delta < 0 {
                    self.focus = FocusColumn::Sections;
                }
                None
            }
        }
    }

    fn activate_current(&mut self, rows: &[SettingsRow]) -> Option<SettingsMutation> {
        if self.focus == FocusColumn::Sections {
            self.focus = FocusColumn::Fields;
            self.select_first_interactive(rows);
            return None;
        }
        let row = rows.get(self.row_index)?;
        match row {
            SettingsRow::Toggle { path, value, .. } => Some(SettingsMutation::Update {
                path: path.clone(),
                value: json!(!*value),
            }),
            SettingsRow::Select {
                path,
                value,
                options,
                ..
            } => Self::step_select(path, value, options, 1),
            SettingsRow::ColorScheme { path, value, .. } => Self::step_color_scheme(path, value, 1),
            SettingsRow::Number { path, value, .. } | SettingsRow::Text { path, value, .. } => {
                self.editing = Some((path.clone(), value.clone()));
                None
            }
            SettingsRow::Heading { .. }
            | SettingsRow::ReadOnly { .. }
            | SettingsRow::Binding { .. } => None,
        }
    }

    fn step_select(
        path: &str,
        current: &str,
        options: &[SettingsSelectOption],
        delta: isize,
    ) -> Option<SettingsMutation> {
        if options.is_empty() {
            return None;
        }
        let pos = options
            .iter()
            .position(|opt| opt.value == current)
            .unwrap_or(0) as isize;
        let next = (pos + delta).rem_euclid(options.len() as isize) as usize;
        Some(SettingsMutation::Update {
            path: path.to_owned(),
            value: json!(options[next].value),
        })
    }

    fn step_color_scheme(path: &str, current: &str, delta: isize) -> Option<SettingsMutation> {
        let all = ThemePreset::ALL;
        let pos = all
            .iter()
            .position(|preset| preset.id() == current)
            .unwrap_or(0) as isize;
        let next = (pos + delta).rem_euclid(all.len() as isize) as usize;
        Some(SettingsMutation::Update {
            path: path.to_owned(),
            value: json!(all[next].id()),
        })
    }

    fn step_number(
        path: &str,
        current: &str,
        step: f64,
        integer: bool,
        delta: isize,
    ) -> Option<SettingsMutation> {
        let parsed = current.trim().parse::<f64>().unwrap_or(0.0);
        let inc = if step > 0.0 { step } else { 1.0 };
        let next = (parsed + inc * (delta as f64)).max(0.0);
        let value = if integer {
            json!(next.round() as u64)
        } else {
            let rounded = (next * 100.0).round() / 100.0;
            json!(rounded)
        };
        Some(SettingsMutation::Update {
            path: path.to_owned(),
            value,
        })
    }

    fn clamp_indices(&mut self, section_count: usize, rows: &[SettingsRow]) {
        if section_count == 0 {
            self.section_index = 0;
        } else if self.section_index >= section_count {
            self.section_index = section_count - 1;
        }
        if rows.is_empty() {
            self.row_index = 0;
        } else if self.row_index >= rows.len() {
            self.row_index = rows.len() - 1;
        }
    }

    fn current_rows(&self, state: &SettingsUiState) -> Vec<SettingsRow> {
        let Some(section) = state
            .sections
            .get(self.section_index)
            .or_else(|| state.sections.first())
        else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for &field_id in &section.fields {
            Self::push_field_rows(state, field_id, &mut rows);
        }
        if section.id == "shortcuts" {
            Self::push_tui_shortcuts(&mut rows);
        }
        rows
    }

    fn push_field_rows(
        state: &SettingsUiState,
        field_id: SettingsRenderFieldId,
        rows: &mut Vec<SettingsRow>,
    ) {
        let Some(field) = state.fields.get(field_id.0 as usize) else {
            return;
        };
        Self::push_field(state, field, rows);
    }

    fn push_field(
        state: &SettingsUiState,
        field: &SettingsRenderField,
        rows: &mut Vec<SettingsRow>,
    ) {
        match &field.kind {
            SettingsRenderFieldKind::Toggle { value } => rows.push(SettingsRow::Toggle {
                path: field.path.clone(),
                label: field.label.clone(),
                hint: field.hint.clone(),
                value: *value,
            }),
            SettingsRenderFieldKind::Select { value, options } => rows.push(SettingsRow::Select {
                path: field.path.clone(),
                label: field.label.clone(),
                hint: field.hint.clone(),
                value: value.clone(),
                options: options.clone(),
            }),
            SettingsRenderFieldKind::Integer { value } => rows.push(SettingsRow::Number {
                path: field.path.clone(),
                label: field.label.clone(),
                hint: field.hint.clone(),
                value: value.clone(),
                step: 1.0,
                integer: true,
            }),
            SettingsRenderFieldKind::Number { value, step } => rows.push(SettingsRow::Number {
                path: field.path.clone(),
                label: field.label.clone(),
                hint: field.hint.clone(),
                value: value.clone(),
                step: *step,
                integer: false,
            }),
            SettingsRenderFieldKind::Text { value, placeholder } => {
                if field.path.ends_with(".color_scheme") {
                    rows.push(SettingsRow::ColorScheme {
                        path: field.path.clone(),
                        label: field.label.clone(),
                        hint: field.hint.clone(),
                        value: value.clone(),
                    });
                } else {
                    rows.push(SettingsRow::Text {
                        path: field.path.clone(),
                        label: field.label.clone(),
                        hint: field.hint.clone(),
                        value: value.clone(),
                        placeholder: placeholder.clone(),
                    });
                }
            }
            SettingsRenderFieldKind::Chord { text } => rows.push(SettingsRow::ReadOnly {
                label: field.label.clone(),
                value: text.clone(),
                hint: field.hint.clone(),
            }),
            SettingsRenderFieldKind::Bindings { rows: bindings } => {
                rows.push(SettingsRow::Heading {
                    label: field.label.clone(),
                });
                for binding in bindings {
                    rows.push(SettingsRow::Binding {
                        command: binding.command.clone(),
                        chord: binding.chord.clone(),
                    });
                }
            }
            SettingsRenderFieldKind::Group { fields } => {
                rows.push(SettingsRow::Heading {
                    label: field.label.clone(),
                });
                for &child in fields {
                    Self::push_field_rows(state, child, rows);
                }
            }
            SettingsRenderFieldKind::Array { items } => {
                rows.push(SettingsRow::Heading {
                    label: field.label.clone(),
                });
                Self::push_array_items(state, items, rows);
            }
            SettingsRenderFieldKind::UpdateCheck { button_label, .. } => {
                rows.push(SettingsRow::ReadOnly {
                    label: field.label.clone(),
                    value: button_label.clone(),
                    hint: field.hint.clone(),
                });
            }
        }
    }

    fn push_array_items(
        state: &SettingsUiState,
        items: &[SettingsRenderItemId],
        rows: &mut Vec<SettingsRow>,
    ) {
        for &item_id in items {
            let Some(item) = state.items.get(item_id.0 as usize) else {
                continue;
            };
            match item {
                SettingsRenderItem::Value { text } => rows.push(SettingsRow::ReadOnly {
                    label: String::new(),
                    value: text.clone(),
                    hint: None,
                }),
                SettingsRenderItem::Object { title, fields } => {
                    rows.push(SettingsRow::Heading {
                        label: format!("  {title}"),
                    });
                    for &child in fields {
                        Self::push_field_rows(state, child, rows);
                    }
                }
            }
        }
    }

    fn push_tui_shortcuts(rows: &mut Vec<SettingsRow>) {
        rows.push(SettingsRow::Heading {
            label: "TUI Controls".to_owned(),
        });
        let entries = [
            ("Open Settings", "Ctrl+b → ,"),
            ("Switch Space", "Ctrl+b → s"),
            ("Split Right", "Ctrl+b → %"),
            ("Split Down", "Ctrl+b → \""),
            ("Zoom Pane", "Ctrl+b → z"),
            ("Toggle Sidebar", "Ctrl+b → b"),
            ("Move Tab Bar (Top/Bottom)", "Ctrl+b → u"),
            ("Toggle Pane Gap", "Ctrl+b → g"),
            ("Next Color Theme", "Ctrl+b → t"),
            ("Close Pane / Exit", "Ctrl+b → x / q"),
        ];
        for (command, chord) in entries {
            rows.push(SettingsRow::Binding {
                command: command.to_owned(),
                chord: chord.to_owned(),
            });
        }
    }

    fn effective_scroll(&self, active_row: usize, body_h: usize, total: usize) -> usize {
        if total <= body_h {
            return 0;
        }
        let mut scroll = self.row_scroll.min(total.saturating_sub(body_h));
        if active_row < scroll {
            scroll = active_row;
        } else if active_row >= scroll + body_h {
            scroll = active_row + 1 - body_h;
        }
        scroll
    }

    fn draw_frame_header(&self, out: &mut String, rect: Rect, inner_w: usize, theme: Theme) {
        let p = theme.palette();
        let search_tag = if self.searching {
            format!(" /{}_ ", self.search_query)
        } else if !self.search_query.is_empty() {
            format!(" /{} ", self.search_query)
        } else {
            String::new()
        };
        let title = format!(" ⚙ Settings{search_tag}");
        let right = " Esc close ";
        let used = 1 + Theme::display_width(&title) + Theme::display_width(right);
        let fill = inner_w.saturating_sub(used);
        let _ = write!(
            out,
            "\x1b[{};{}H{}{}{}{}",
            rect.y,
            rect.x,
            theme.fg_bg(p.primary, p.header, "╭─"),
            theme.fg_bg_bold(p.fg, p.header, &title),
            theme.fg_bg(p.primary, p.header, &"─".repeat(fill)),
            theme.fg_bg(p.muted, p.header, &format!("{right}╮"))
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_body(
        &self,
        out: &mut String,
        rect: Rect,
        side_w: usize,
        field_w: usize,
        body_h: usize,
        state: &SettingsUiState,
        active_section: usize,
        rows: &[SettingsRow],
        active_row: usize,
        scroll: usize,
        theme: Theme,
        hits: &mut HitMap,
    ) {
        let p = theme.palette();
        for idx in 0..body_h {
            let y = rect.y + 1 + idx as u16;
            let left = if side_w > 0 {
                if let Some(section) = state.sections.get(idx) {
                    hits.settings_section_rows.push((y, idx));
                    let selected = idx == active_section;
                    let focused = selected && self.focus == FocusColumn::Sections;
                    let glyph = if selected {
                        theme.primary("▸")
                    } else {
                        theme.muted("·")
                    };
                    let name = Theme::fit_title(&section.title, side_w.saturating_sub(4));
                    let text = if selected {
                        format!(" {glyph} {}", theme.bold(&name))
                    } else {
                        format!(" {glyph} {}", theme.muted(&name))
                    };
                    let col = if focused {
                        theme.surface_row(&text, side_w)
                    } else {
                        theme.header_row(&text, side_w)
                    };
                    format!("{col}{}", theme.fg_bg(p.muted, p.header, "│"))
                } else {
                    format!(
                        "{}{}",
                        theme.header_row("", side_w),
                        theme.fg_bg(p.muted, p.header, "│")
                    )
                }
            } else {
                String::new()
            };
            let row_idx = scroll + idx;
            let right = if let Some(row) = rows.get(row_idx) {
                if row.is_selectable() {
                    hits.settings_field_rows.push((y, row_idx));
                }
                let selected = row_idx == active_row && self.focus == FocusColumn::Fields;
                self.format_row(row, field_w, selected, theme)
            } else {
                theme.header_row("", field_w)
            };
            let _ = write!(
                out,
                "\x1b[{y};{}H{}{left}{right}{}",
                rect.x,
                theme.fg_bg(p.primary, p.header, "│"),
                theme.fg_bg(p.primary, p.header, "│")
            );
        }
    }

    fn format_row(&self, row: &SettingsRow, width: usize, selected: bool, theme: Theme) -> String {
        let p = theme.palette();
        let raw = if let SettingsRow::Heading { label } = row {
            format!("  {}", theme.fg_bold(p.primary, label))
        } else {
            let cursor = if selected {
                theme.primary("›")
            } else {
                " ".into()
            };
            let (label, badge) = self.row_label_and_badge(row, theme);
            Self::split_line(
                &format!(" {cursor} {label}"),
                &badge,
                width,
                theme,
                selected,
            )
        };
        if selected {
            theme.surface_row(&raw, width)
        } else {
            theme.header_row(&raw, width)
        }
    }

    fn row_label_and_badge(&self, row: &SettingsRow, theme: Theme) -> (String, String) {
        let p = theme.palette();
        match row {
            SettingsRow::Heading { label } => (label.clone(), String::new()),
            SettingsRow::Toggle { label, value, .. } => {
                let badge = if *value {
                    theme.fg_bold(p.green, "[● On]")
                } else {
                    theme.muted("[○ Off]")
                };
                (label.clone(), badge)
            }
            SettingsRow::Select {
                label,
                value,
                options,
                ..
            } => {
                let display = options
                    .iter()
                    .find(|opt| opt.value == *value)
                    .map(|opt| opt.label.as_str())
                    .unwrap_or(value.as_str());
                (
                    label.clone(),
                    format!("◂ {} ▸", theme.fg_bold(p.cyan, display)),
                )
            }
            SettingsRow::ColorScheme { label, value, .. } => {
                let swatch = ThemePreset::parse(value)
                    .map(|preset| format!("{} ", theme.swatches(preset)))
                    .unwrap_or_default();
                (
                    label.clone(),
                    format!("{swatch}◂ {} ▸", theme.fg_bold(p.accent, value)),
                )
            }
            SettingsRow::Number {
                path, label, value, ..
            } => {
                let text = self.editing_value(path, value);
                (
                    label.clone(),
                    format!("◂ {} ▸", theme.fg_bold(p.gold, &text)),
                )
            }
            SettingsRow::Text {
                path,
                label,
                value,
                placeholder,
                ..
            } => {
                let shown = self.editing_value(path, value);
                let display = if shown.is_empty() {
                    placeholder.as_deref().unwrap_or("(empty)")
                } else {
                    shown.as_str()
                };
                (label.clone(), theme.fg_bold(p.fg, display))
            }
            SettingsRow::ReadOnly { label, value, .. } => (label.clone(), theme.muted(value)),
            SettingsRow::Binding { command, chord } => {
                (command.clone(), theme.fg_bold(p.gold, chord))
            }
        }
    }

    fn editing_value(&self, path: &str, fallback: &str) -> String {
        if let Some((edit_path, draft)) = &self.editing
            && edit_path == path
        {
            return format!("{draft}▎");
        }
        fallback.to_owned()
    }

    fn split_line(left: &str, right: &str, width: usize, theme: Theme, selected: bool) -> String {
        let right_w = Theme::display_width(right);
        let left_budget = width.saturating_sub(right_w + 3);
        let left_fit = Theme::truncate_ellipsis(left, left_budget);
        let left_styled = if selected {
            theme.bold(&left_fit)
        } else {
            left_fit
        };
        let gap = width.saturating_sub(Theme::display_width(&left_styled) + right_w + 1);
        format!("{left_styled}{}{right} ", " ".repeat(gap.max(1)))
    }

    fn draw_frame_footer(
        &self,
        out: &mut String,
        rect: Rect,
        inner_w: usize,
        rows: &[SettingsRow],
        active_row: usize,
        theme: Theme,
    ) {
        let p = theme.palette();
        let hint_y = rect.y + rect.h.saturating_sub(3);
        let help_y = rect.y + rect.h.saturating_sub(2);
        let bottom_y = rect.y + rect.h.saturating_sub(1);
        let hint = if self.searching {
            "Type to filter sections · Enter done · Esc clear"
        } else if self.editing.is_some() {
            "Editing value · Enter save · Esc cancel"
        } else {
            rows.get(active_row)
                .and_then(SettingsRow::hint)
                .unwrap_or("Changes apply live and sync with Vmux Desktop settings")
        };
        let hint_line = theme.header_row(&format!(" {}", theme.muted(hint)), inner_w);
        let help_line = theme.header_row(
            &theme.muted(" Tab pane · ↑↓ move · ←→/↵ change · / search · esc close"),
            inner_w,
        );
        let _ = write!(
            out,
            "\x1b[{hint_y};{}H{}{hint_line}{}",
            rect.x,
            theme.fg_bg(p.primary, p.header, "│"),
            theme.fg_bg(p.primary, p.header, "│")
        );
        let _ = write!(
            out,
            "\x1b[{help_y};{}H{}{help_line}{}",
            rect.x,
            theme.fg_bg(p.primary, p.header, "│"),
            theme.fg_bg(p.primary, p.header, "│")
        );
        let _ = write!(
            out,
            "\x1b[{bottom_y};{}H{}",
            rect.x,
            theme.fg_bg(p.primary, p.header, &format!("╰{}╯", "─".repeat(inner_w)))
        );
    }
}

#[cfg(test)]
mod tests {
    use vmux_setting::state::{SettingsRenderField, SettingsSection};

    use super::*;

    fn sample_state() -> SettingsUiState {
        SettingsUiState {
            query: String::new(),
            sections: vec![
                SettingsSection {
                    id: "terminal".into(),
                    title: "Terminal".into(),
                    description: None,
                    search_text: "terminal color scheme".into(),
                    fields: vec![SettingsRenderFieldId(0), SettingsRenderFieldId(1)],
                },
                SettingsSection {
                    id: "shortcuts".into(),
                    title: "Shortcuts".into(),
                    description: None,
                    search_text: "shortcuts".into(),
                    fields: vec![],
                },
            ],
            fields: vec![
                SettingsRenderField {
                    path: "terminal.confirm_close".into(),
                    label: "Confirm Close".into(),
                    hint: Some("Ask before closing".into()),
                    kind: SettingsRenderFieldKind::Toggle { value: true },
                },
                SettingsRenderField {
                    path: "terminal.themes[0].color_scheme".into(),
                    label: "Color Scheme".into(),
                    hint: None,
                    kind: SettingsRenderFieldKind::Text {
                        value: "catppuccin-mocha".into(),
                        placeholder: None,
                    },
                },
            ],
            items: Vec::new(),
        }
    }

    #[test]
    fn settings_modal_toggles_fields_cycles_color_scheme_and_lists_tui_shortcuts() {
        let state = sample_state();
        let mut modal = SettingsModal::open();
        let (_, close, _) = modal.close_requested_on_key(b"\t", &state);
        assert!(!close);
        let (_, _, toggle) = modal.close_requested_on_key(b"\r", &state);
        assert_eq!(
            toggle,
            Some(SettingsMutation::Update {
                path: "terminal.confirm_close".into(),
                value: json!(false),
            })
        );
        modal.close_requested_on_key(b"j", &state);
        let (_, _, scheme) = modal.close_requested_on_key(b"\x1b[C", &state);
        assert_eq!(
            scheme,
            Some(SettingsMutation::Update {
                path: "terminal.themes[0].color_scheme".into(),
                value: json!(ThemePreset::CatppuccinMocha.next().id()),
            })
        );
        modal.close_requested_on_key(b"\t", &state);
        modal.close_requested_on_key(b"j", &state);
        let mut out = String::new();
        let mut hits = HitMap::default();
        modal.draw(
            &mut out,
            Rect {
                x: 1,
                y: 1,
                w: 80,
                h: 24,
            },
            &state,
            Theme::new(ThemePreset::CatppuccinMocha, false),
            &mut hits,
        );
        assert!(out.contains("TUI Controls"));
        assert!(out.contains("Ctrl+b → ,"));
    }
}
