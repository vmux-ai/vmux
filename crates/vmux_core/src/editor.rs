use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum EditMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl EditMode {
    pub fn label(self) -> &'static str {
        match self {
            EditMode::Normal => "NORMAL",
            EditMode::Insert => "INSERT",
            EditMode::Visual => "VISUAL",
            EditMode::VisualLine => "V-LINE",
        }
    }
    pub fn is_visual(self) -> bool {
        matches!(self, EditMode::Visual | EditMode::VisualLine)
    }
    pub fn accepts_text(self) -> bool {
        matches!(self, EditMode::Insert)
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CursorPos {
    pub line: u32,
    pub row: u32,
    pub col: u32,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct SelSpan {
    pub line: u32,
    pub row: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum KeymapKind {
    #[default]
    Vscode,
    Vim,
}
