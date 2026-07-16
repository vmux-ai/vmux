use serde::{Deserialize, Serialize};

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
    /// Clickable URL/path ranges in this row, in column coordinates.
    /// Computed by the host; the service always leaves this empty.
    #[serde(default)]
    pub links: Vec<LinkRange>,
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
pub struct LinkRange {
    /// First column of the link (0-based, inclusive).
    pub start_col: u16,
    /// Last column of the link (0-based, inclusive).
    pub end_col: u16,
    /// Ready-to-open target: `http(s)://…`, `data:…`, or `file://…`.
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
pub struct TermSpan {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    pub flags: u16,
    /// Starting column index of this span in the row (0-based).
    #[serde(default)]
    pub col: u16,
    /// Number of grid columns this span covers (accounts for wide characters
    /// taking 2 columns). When 0 (legacy), falls back to `text.chars().count()`.
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
    pub row: u32,
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
