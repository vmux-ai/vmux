use serde::{Deserialize, Serialize};

#[vmux_api::contract(Copy, Eq, Default)]
pub enum EditMode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
    CommandLine,
    Replace,
    VisualBlock,
}

impl EditMode {
    pub fn label(self) -> &'static str {
        match self {
            EditMode::Normal => "NORMAL",
            EditMode::Insert => "INSERT",
            EditMode::Visual => "VISUAL",
            EditMode::VisualLine => "V-LINE",
            EditMode::CommandLine => "COMMAND",
            EditMode::Replace => "REPLACE",
            EditMode::VisualBlock => "V-BLOCK",
        }
    }

    pub fn is_visual(self) -> bool {
        matches!(
            self,
            EditMode::Visual | EditMode::VisualLine | EditMode::VisualBlock
        )
    }

    pub fn accepts_text(self) -> bool {
        matches!(self, EditMode::Insert | EditMode::Replace)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeyMapping {
    #[serde(default)]
    pub mode: String,
    pub lhs: String,
    pub rhs: String,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct CursorPos {
    pub line: u32,
    pub row: u32,
    pub col: u32,
    #[serde(default)]
    pub char_col: u32,
}

#[vmux_api::contract(Copy, Eq, Default)]
pub struct SelSpan {
    pub line: u32,
    pub row: u32,
    pub start: u32,
    pub end: u32,
}

#[vmux_api::contract(Copy, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeymapKind {
    #[default]
    #[serde(rename = "standard", alias = "vscode")]
    Vscode,
    Vim,
}

#[vmux_api::contract(Copy, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum WordWrap {
    Off,
    #[default]
    On,
    WordWrapColumn,
    Bounded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_keymap_serializes_with_legacy_vscode_compatibility() {
        assert_eq!(
            serde_json::to_value(KeymapKind::Vscode).unwrap(),
            serde_json::json!("standard")
        );
        assert_eq!(
            serde_json::from_value::<KeymapKind>(serde_json::json!("vscode")).unwrap(),
            KeymapKind::Vscode
        );
    }
}
