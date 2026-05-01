use serde::{Deserialize, Serialize};

pub const TERM_VIEWPORT_EVENT: &str = "term_viewport";
pub const TERM_KEY_EVENT: &str = "term_key";
pub const TERM_MOUSE_EVENT: &str = "term_mouse";
pub const TERM_RESIZE_EVENT: &str = "term_resize";

pub const TERM_THEME_EVENT: &str = "term_theme";
pub const TERMINAL_WEBVIEW_URL: &str = "vmux://terminal/";

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
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermThemeEvent {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub cursor: [u8; 3],
    pub ansi: [[u8; 3]; 16],
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

/// Range of selected cells in viewport coordinates (0-based row/col).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermSelectionRange {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
    pub is_block: bool,
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
pub struct TermLine {
    pub spans: Vec<TermSpan>,
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
pub struct TermSpan {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: u16,
    /// Starting column index of this span in the row (0-based).
    #[serde(default)]
    pub col: u16,
    /// Number of grid columns this span covers (accounts for wide characters
    /// taking 2 columns).  When 0 (legacy), falls back to `text.chars().count()`.
    #[serde(default)]
    pub grid_cols: u16,
}

pub const FLAG_BOLD: u16 = 1;
pub const FLAG_ITALIC: u16 = 2;
pub const FLAG_UNDERLINE: u16 = 4;
pub const FLAG_STRIKETHROUGH: u16 = 8;
pub const FLAG_DIM: u16 = 16;
pub const FLAG_INVERSE: u16 = 32;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct TermCursor {
    pub col: u16,
    pub row: u16,
    pub shape: CursorShape,
    pub visible: bool,
    /// The character under the cursor (for block-cursor rendering).
    #[serde(default)]
    pub ch: String,
}

impl Default for TermCursor {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            shape: CursorShape::Block,
            visible: true,
            ch: " ".into(),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

/// Incremental viewport update. Contains only changed lines plus cursor/selection.
/// When `full` is true, `changed_lines` contains ALL lines (used on resize/spawn).
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct TermViewportPatch {
    /// (row_index, line) pairs for rows that changed since last sync.
    pub changed_lines: Vec<(u16, TermLine)>,
    pub cursor: TermCursor,
    pub cols: u16,
    pub rows: u16,
    pub selection: Option<TermSelectionRange>,
    #[serde(default)]
    pub copy_mode: bool,
    /// When true, changed_lines contains every row (full viewport rebuild).
    pub full: bool,
}

impl TermViewportPatch {
    pub fn requires_row_rebuild(&self, current_cols: u16, current_rows: u16) -> bool {
        self.full || self.cols != current_cols || self.rows != current_rows
    }

    pub fn changed_row_indices(&self) -> impl Iterator<Item = u16> + '_ {
        self.changed_lines.iter().map(|(row_idx, _)| *row_idx)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorRowUpdate {
    pub clear: Option<u16>,
    pub set: Option<u16>,
}

pub fn cursor_row_update(previous: Option<&TermCursor>, next: &TermCursor) -> CursorRowUpdate {
    let clear = previous
        .filter(|cursor| cursor.visible && (!next.visible || cursor.row != next.row))
        .map(|cursor| cursor.row);
    let set = next.visible.then_some(next.row);

    CursorRowUpdate { clear, set }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermKeyEvent {
    pub key: String,
    pub modifiers: u8,
    pub text: Option<String>,
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermMouseEvent {
    /// 0=left, 1=middle, 2=right, 3=none (release/motion), 64=scroll_up, 65=scroll_down
    pub button: u8,
    pub col: u16,
    pub row: u16,
    pub modifiers: u8,
    /// true for press, false for release
    pub pressed: bool,
    /// true when this is a motion event (drag if button<3, move if button==3)
    #[serde(default)]
    pub moving: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TermResizeEvent {
    pub char_width: f32,
    pub char_height: f32,
    #[serde(default)]
    pub viewport_width: f32,
    #[serde(default)]
    pub viewport_height: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(changed_rows: Vec<u16>, cols: u16, rows: u16, full: bool) -> TermViewportPatch {
        TermViewportPatch {
            changed_lines: changed_rows
                .into_iter()
                .map(|row| (row, TermLine::default()))
                .collect(),
            cursor: TermCursor::default(),
            cols,
            rows,
            selection: None,
            copy_mode: false,
            full,
        }
    }

    #[test]
    fn viewport_patch_rebuilds_only_for_full_or_dimension_change() {
        assert!(!patch(vec![3], 80, 24, false).requires_row_rebuild(80, 24));
        assert!(patch(vec![3], 80, 24, true).requires_row_rebuild(80, 24));
        assert!(patch(vec![3], 100, 24, false).requires_row_rebuild(80, 24));
        assert!(patch(vec![3], 80, 30, false).requires_row_rebuild(80, 24));
    }

    #[test]
    fn viewport_patch_changed_rows_come_only_from_changed_lines() {
        let rows = patch(vec![1, 9], 80, 24, false)
            .changed_row_indices()
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![1, 9]);
    }

    #[test]
    fn cursor_row_update_targets_only_old_and_new_visible_rows() {
        let old = TermCursor {
            row: 2,
            visible: true,
            ..TermCursor::default()
        };
        let new = TermCursor {
            row: 5,
            visible: true,
            ..TermCursor::default()
        };

        assert_eq!(
            cursor_row_update(Some(&old), &new),
            CursorRowUpdate {
                clear: Some(2),
                set: Some(5)
            }
        );
        assert_eq!(
            cursor_row_update(Some(&new), &new),
            CursorRowUpdate {
                clear: None,
                set: Some(5)
            }
        );
        assert_eq!(
            cursor_row_update(
                Some(&old),
                &TermCursor {
                    visible: false,
                    ..new
                }
            ),
            CursorRowUpdate {
                clear: Some(2),
                set: None
            }
        );
    }
}
