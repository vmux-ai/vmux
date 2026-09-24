use serde::{Deserialize, Serialize};

use super::{AnsiPalette, RgbColor, TermCursor, TermLine, TermSelectionRange};

pub const TERMINAL_PAGE_URL: &str = "vmux://terminal/";

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(target = "terminal")]
pub struct ServiceUnavailableEvent {
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(target = "terminal")]
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(target = "terminal")]
pub struct TermLoadingEvent {
    pub loading: bool,
    pub label: String,
    pub segment: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(target = "terminal")]
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(target = "terminal")]
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(target = "terminal")]
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

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(target = "terminal")]
pub struct TermMouseEvent {
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    pub pressed: bool,
    #[serde(default)]
    pub moving: bool,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(target = "terminal")]
pub struct TermLinkOpenRequest {
    pub url: String,
}

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::ui_event(target = "terminal")]
pub struct TermResizeEvent {
    pub char_width: f32,
    pub char_height: f32,
    #[serde(default)]
    pub viewport_width: f32,
    #[serde(default)]
    pub viewport_height: f32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[vmux_api::host_event(target = "terminal")]
pub struct TermTitleEvent {
    pub title: String,
}

#[vmux_api::payload]
#[derive(vmux_api::UiStatePatch)]
pub enum TerminalUiStatePatch {
    ServiceUnavailable(ServiceUnavailableEvent),
    Viewport(TermViewportPatch),
    Theme(TermThemeEvent),
    Title(TermTitleEvent),
    Loading(TermLoadingEvent),
    PromptDraft(AgentPromptDraftEvent),
}

#[vmux_api::payload(Default)]
#[vmux_api::host_event(target = "terminal")]
#[derive(vmux_api::UiState)]
pub struct TerminalUiStateEvent {
    pub sequence: u64,
    pub patches: Vec<TerminalUiStatePatch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_ui_state_preserves_patch_order() {
        let event = TerminalUiStateEvent {
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
        let decoded =
            rkyv::from_bytes::<TerminalUiStateEvent, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 5);
        assert!(matches!(
            decoded.patches.as_slice(),
            [
                TerminalUiStatePatch::Title(_),
                TerminalUiStatePatch::Loading(_)
            ]
        ));
    }
}
