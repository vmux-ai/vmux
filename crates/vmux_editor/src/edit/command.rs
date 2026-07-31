pub use vmux_core::{CursorPos, EditMode, SelSpan};

/// How an operator turns a motion's endpoint into a range.
///
/// Exclusive stops before the target character, inclusive covers it, and linewise expands the
/// range to whole lines. Getting this wrong is what makes `de` and `dj` behave like `dw`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKind {
    Exclusive,
    Inclusive,
    Linewise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    LeftBounded,
    RightBounded,
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
    ParagraphPrev,
    ParagraphNext,
    GotoLine(u32),
}

impl Motion {
    pub fn kind(self) -> MotionKind {
        match self {
            Motion::Up
            | Motion::Down
            | Motion::PageUp
            | Motion::PageDown
            | Motion::DocStart
            | Motion::DocEnd
            | Motion::GotoLine(_) => MotionKind::Linewise,
            Motion::WordEnd | Motion::LineEnd => MotionKind::Inclusive,
            _ => MotionKind::Exclusive,
        }
    }
}

/// A transformation applied to a range of text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    Upper,
    Lower,
    ToggleCase,
}

impl Operator {
    pub fn is_linewise_only(self) -> bool {
        matches!(self, Operator::Indent | Operator::Outdent)
    }
}

/// What an operator acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Motion(Motion, usize),
    Line(usize),
    Selection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Move(Motion),
    Select(Motion),
    InsertText(String),
    ReplaceText(String),
    InsertNewline,
    InsertTab,
    DeleteBack,
    DeleteForward,
    DeleteWordBack,
    Op {
        operator: Operator,
        target: Target,
        register: Option<char>,
    },
    Put {
        before: bool,
        count: usize,
        register: Option<char>,
    },
    ReplaceChar {
        ch: char,
        count: usize,
    },
    JoinLines {
        count: usize,
        spaces: bool,
    },
    OpenLine {
        above: bool,
    },
    SwapSelectionEnds,
    ScrollViewport(i32),
    SetMode(EditMode),
    Undo,
    Redo,
    Save,
    GotoDefinition,
    FindReferences,
    Hover,
    TriggerCompletion,
    FoldToggle,
    FoldOpen,
    FoldClose,
    FoldToggleRecursive,
    FoldAll,
    UnfoldAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
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
