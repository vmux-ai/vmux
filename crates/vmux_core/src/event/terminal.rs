use serde::{Deserialize, Serialize};

use super::{AnsiPalette, RgbColor, TermCursor, TermLine, TermSelectionRange};

pub const TERMINAL_PAGE_URL: &str = "vmux://terminal/";

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
    pub cursor_style: String,
    #[serde(default)]
    pub cursor_blink: bool,
}

#[vmux_api::contract(Eq)]
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,
    pub segment: String,
}

#[vmux_api::contract(Eq)]
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

#[vmux_api::ui_event(Eq, Default, url = "vmux://terminal/")]
pub struct TermScrollEvent {
    pub top_row: u32,
    pub follow: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorRowUpdate {
    pub clear: Option<u32>,
    pub set: Option<u32>,
}

pub fn cursor_row_update(previous: Option<&TermCursor>, next: &TermCursor) -> CursorRowUpdate {
    let clear = previous
        .filter(|cursor| cursor.visible && (!next.visible || cursor.row != next.row))
        .map(|cursor| cursor.row);
    let set = next.visible.then_some(next.row);

    CursorRowUpdate { clear, set }
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[vmux_api::ui_event(Default, url = "vmux://terminal/")]
pub struct TermMouseEvent {
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    pub pressed: bool,
    #[serde(default)]
    pub moving: bool,
}

#[vmux_api::ui_event(Default, Eq, url = "vmux://terminal/")]
pub struct TermLinkOpenRequest {
    pub url: String,
}

#[vmux_api::ui_event(Default, url = "vmux://terminal/")]
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

#[vmux_api::ui_state(Default, url = "vmux://terminal/")]
pub struct TerminalUiState {
    pub sequence: u64,
    pub patches: Vec<TerminalUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_ui_state_preserves_patch_order() {
        let event = TerminalUiState {
            sequence: 5,
            patches: vec![
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
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<TerminalUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 5);
        assert!(decoded.patches[0].title.is_some());
        assert!(decoded.patches[1].loading.is_some());
    }
}
