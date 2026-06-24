pub use vmux_core::{CursorPos, EditMode, SelSpan};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordNext,
    WordPrev,
    WordEnd,
    LineStart,
    FirstNonBlank,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
    GotoLine(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Move(Motion),
    Select(Motion),
    InsertText(String),
    InsertNewline,
    InsertTab,
    DeleteBack,
    DeleteForward,
    DeleteWordBack,
    DeleteToLineEnd,
    DeleteRange(Motion),
    YankRange(Motion),
    DeleteSelection,
    DeleteLine,
    Yank,
    Cut,
    Paste,
    PasteBefore,
    SetMode(EditMode),
    Undo,
    Redo,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self { anchor: at, head: at }
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
    pub fn range(&self) -> std::ops::Range<usize> {
        if self.anchor <= self.head {
            self.anchor..self.head
        } else {
            self.head..self.anchor
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_range_normalizes_direction() {
        assert_eq!(Selection { anchor: 2, head: 5 }.range(), 2..5);
        assert_eq!(Selection { anchor: 5, head: 2 }.range(), 2..5);
    }

    #[test]
    fn caret_is_empty() {
        assert!(Selection::caret(3).is_empty());
        assert!(!Selection { anchor: 1, head: 2 }.is_empty());
    }

    #[test]
    fn mode_labels() {
        assert_eq!(EditMode::Normal.label(), "NORMAL");
        assert!(EditMode::VisualLine.is_visual());
        assert!(!EditMode::Insert.is_visual());
    }
}
