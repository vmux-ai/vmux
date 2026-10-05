use serde::{Deserialize, Serialize};

use super::{AnsiPalette, CursorStyle, RgbColor, TermCursor, TermLine, TermSelectionRange};

#[vmux_api::contract]
pub struct ServiceUnavailableEvent {
    pub message: String,
}

#[vmux_api::contract]
pub struct TermThemeEvent {
    pub foreground: RgbColor,
    pub background: RgbColor,
    pub cursor: RgbColor,
    pub ansi: AnsiPalette,
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub font_size: f32,
    #[serde(default)]
    pub line_height: f32,
    #[serde(default)]
    pub padding: f32,
    #[serde(default)]
    pub cursor_style: CursorStyle,
    #[serde(default)]
    pub cursor_blink: bool,
}

#[vmux_api::contract(Eq)]
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,
    pub segment: String,
}

#[vmux_api::contract(Default, Eq)]
pub struct AgentPromptDraftEvent {
    pub draft: String,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermViewportEvent {
    pub lines: Vec<TermLine>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    #[serde(default)]
    pub copy_mode: bool,
    #[serde(default)]
    pub selection: Option<TermSelectionRange>,
}

#[vmux_api::contract]
pub struct TermViewportPatch {
    pub changed_lines: Vec<(u32, TermLine)>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub selection: Option<TermSelectionRange>,
    #[serde(default)]
    pub copy_mode: bool,
    pub full: bool,
    #[serde(default)]
    pub first_row: u32,
    #[serde(default)]
    pub total_rows: u32,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub mouse: bool,
    #[serde(default)]
    pub evicted_total: u64,
}

impl TermViewportPatch {
    pub fn requires_row_rebuild(&self, current_cols: u16, current_rows: u16) -> bool {
        self.full || self.cols != current_cols || self.rows != current_rows
    }

    pub fn changed_row_indices(&self) -> impl Iterator<Item = u32> + '_ {
        self.changed_lines.iter().map(|(row_idx, _)| *row_idx)
    }
}

#[vmux_api::ui_event(Eq, Default)]
pub struct TermScrollEvent {
    pub top_row: u32,
    pub follow: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorRowUpdate {
    pub clear: Option<u32>,
    pub set: Option<u32>,
}

impl CursorRowUpdate {
    pub fn between(previous: Option<&TermCursor>, next: &TermCursor) -> Self {
        let clear = previous
            .filter(|cursor| cursor.visible && (!next.visible || cursor.row != next.row))
            .map(|cursor| cursor.row);
        let set = next.visible.then_some(next.row);

        Self { clear, set }
    }
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[vmux_api::ui_event(Default)]
pub struct TermMouseEvent {
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    pub pressed: bool,
    #[serde(default)]
    pub moving: bool,
}

#[vmux_api::ui_event(Default, Eq)]
pub struct TermLinkOpenRequest {
    pub url: String,
}

#[vmux_api::ui_event(Default)]
pub struct TermResizeEvent {
    pub char_width: f32,
    pub char_height: f32,
    #[serde(default)]
    pub viewport_width: f32,
    #[serde(default)]
    pub viewport_height: f32,
}

#[vmux_api::contract(Eq)]
pub struct TermTitleEvent {
    pub title: String,
}

#[vmux_api::ui_state_patch(Default)]
pub struct TerminalUiStatePatch {
    pub service_unavailable: Option<ServiceUnavailableEvent>,
    pub viewport: Option<TermViewportPatch>,
    pub theme: Option<TermThemeEvent>,
    pub title: Option<TermTitleEvent>,
    pub loading: Option<TermLoadingEvent>,
    pub prompt_draft: Option<AgentPromptDraftEvent>,
}

#[vmux_api::contract(Default)]
pub struct TerminalViewportState {
    pub rows: Vec<(u32, TermLine)>,
    pub cursor: Option<TermCursor>,
    pub cols: u16,
    pub selection: Option<TermSelectionRange>,
    pub copy_mode: bool,
    pub first_row: u32,
    pub total_rows: u32,
    pub alt: bool,
    pub mouse: bool,
}

#[vmux_api::ui_state(Default, patch = TerminalUiStatePatch)]
pub struct TerminalUiState {
    pub service_error: String,
    pub viewport: TerminalViewportState,
    pub theme: Option<TermThemeEvent>,
    pub title: String,
    pub loading: Option<TermLoadingEvent>,
    pub prompt_draft: AgentPromptDraftEvent,
}

impl vmux_api::UiStateProjection<TerminalUiStatePatch> for TerminalUiState {
    fn apply(&mut self, patch: TerminalUiStatePatch) {
        if let Some(error) = patch.service_unavailable {
            self.service_error = error.message;
        }
        if let Some(viewport) = patch.viewport {
            let first = viewport.first_row;
            let overscan = crate::scroll::Overscan::new(
                viewport.rows,
                crate::scroll::TERMINAL_OVERSCAN_K,
                crate::scroll::OVERSCAN_FLOOR,
                crate::scroll::OVERSCAN_CAP,
            )
            .rows();
            let keep_hi = (first + viewport.rows as u32 + overscan * 2 + 2)
                .min(viewport.total_rows.saturating_sub(1));
            if viewport.full {
                self.viewport.rows = viewport
                    .changed_lines
                    .into_iter()
                    .filter(|(row, _)| *row >= first && *row <= keep_hi)
                    .collect();
            } else {
                for (row, line) in viewport.changed_lines {
                    if let Some((_, current)) = self
                        .viewport
                        .rows
                        .iter_mut()
                        .find(|(current, _)| *current == row)
                    {
                        *current = line;
                    } else if row >= first && row <= keep_hi {
                        self.viewport.rows.push((row, line));
                    }
                }
                self.viewport
                    .rows
                    .retain(|(row, _)| *row >= first && *row <= keep_hi);
            }
            self.viewport.rows.sort_by_key(|(row, _)| *row);
            self.viewport.cursor = Some(viewport.cursor);
            self.viewport.cols = viewport.cols;
            self.viewport.selection = viewport.selection;
            self.viewport.copy_mode = viewport.copy_mode;
            self.viewport.first_row = first;
            self.viewport.total_rows = viewport.total_rows;
            self.viewport.alt = viewport.alt;
            self.viewport.mouse = viewport.mouse;
        }
        if let Some(theme) = patch.theme {
            self.theme = Some(theme);
        }
        if let Some(title) = patch.title {
            self.title = title.title;
        }
        if let Some(loading) = patch.loading {
            if loading.loading {
                self.loading = Some(loading);
            } else {
                self.loading = None;
                self.prompt_draft = AgentPromptDraftEvent::default();
            }
        }
        if let Some(prompt_draft) = patch.prompt_draft {
            self.prompt_draft = prompt_draft;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_ui_state_builds_a_retained_tree() {
        let event = <TerminalUiState as vmux_api::UiState>::from_updates(
            None,
            vec![
                TermTitleEvent {
                    title: "Terminal".into(),
                }
                .into(),
                TermLoadingEvent {
                    loading: true,
                    label: "Agent".into(),
                    segment: "agent".into(),
                }
                .into(),
            ],
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<TerminalUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.title, "Terminal");
        assert_eq!(decoded.loading.unwrap().label, "Agent");
    }
}
