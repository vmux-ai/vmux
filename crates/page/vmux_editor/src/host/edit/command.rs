pub use vmux_core::{CursorPos, EditMode, SelSpan};

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
    BigWordNext,
    BigWordPrev,
    BigWordEnd,
    WordEndPrev,
    BigWordEndPrev,
    LineStart,
    FirstNonBlank,
    LineEnd,
    LastNonBlank,
    Column(usize),
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    NextLineStart,
    PrevLineStart,
    ParagraphPrev,
    ParagraphNext,
    MatchPair,
    FindChar { ch: char, forward: bool, till: bool },
    SearchNext { reverse: bool },
    GotoLine(u32),
}

impl Motion {
    pub fn kind(self) -> MotionKind {
        match self {
            Motion::Up
            | Motion::Down
            | Motion::PageUp
            | Motion::PageDown
            | Motion::HalfPageUp
            | Motion::HalfPageDown
            | Motion::ScreenTop
            | Motion::ScreenMiddle
            | Motion::ScreenBottom
            | Motion::NextLineStart
            | Motion::PrevLineStart
            | Motion::DocStart
            | Motion::DocEnd
            | Motion::GotoLine(_) => MotionKind::Linewise,
            Motion::WordEnd
            | Motion::BigWordEnd
            | Motion::WordEndPrev
            | Motion::BigWordEndPrev
            | Motion::LastNonBlank
            | Motion::MatchPair => MotionKind::Inclusive,
            Motion::LineEnd => MotionKind::Exclusive,
            Motion::FindChar { forward, .. } if forward => MotionKind::Inclusive,
            _ => MotionKind::Exclusive,
        }
    }
}

impl Motion {
    pub fn collapse_to_start(self) -> Option<bool> {
        Some(match self {
            Motion::Left
            | Motion::LeftBounded
            | Motion::Up
            | Motion::PageUp
            | Motion::HalfPageUp
            | Motion::ParagraphPrev
            | Motion::WordPrev
            | Motion::BigWordPrev
            | Motion::WordEndPrev
            | Motion::BigWordEndPrev
            | Motion::LineStart
            | Motion::PrevLineStart
            | Motion::ScreenTop
            | Motion::DocStart => true,
            Motion::Right
            | Motion::RightBounded
            | Motion::Down
            | Motion::PageDown
            | Motion::HalfPageDown
            | Motion::ParagraphNext
            | Motion::WordNext
            | Motion::BigWordNext
            | Motion::WordEnd
            | Motion::BigWordEnd
            | Motion::LineEnd
            | Motion::LastNonBlank
            | Motion::NextLineStart
            | Motion::ScreenBottom
            | Motion::DocEnd => false,
            _ => return None,
        })
    }

    pub fn is_jump(self) -> bool {
        matches!(
            self,
            Motion::DocStart
                | Motion::DocEnd
                | Motion::GotoLine(_)
                | Motion::ParagraphPrev
                | Motion::ParagraphNext
                | Motion::MatchPair
                | Motion::ScreenTop
                | Motion::ScreenMiddle
                | Motion::ScreenBottom
                | Motion::SearchNext { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollPlacement {
    Top,
    Center,
    Bottom,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Motion(Motion, usize),
    TextObject(crate::edit::text_object::TextObject),
    Line(usize),
    Selection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Move(Motion),
    Select(Motion),
    InsertText(String),
    OvertypeText(String),
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
    BeginBlockInsert {
        after: bool,
    },
    FinishBlockInsert {
        text: String,
    },
    UndoTime {
        forward: bool,
        count: usize,
    },
    Increment(i64),
    SwapSelectionEnds,
    SelectTextObject(crate::edit::text_object::TextObject),
    SetSearch {
        pattern: String,
        forward: bool,
    },
    SearchWord {
        forward: bool,
    },
    ClearSearchHighlight,
    Substitute {
        range: crate::edit::ex::ExRange,
        pattern: String,
        replacement: String,
        all: bool,
    },
    ExDelete(crate::edit::ex::ExRange),
    ExYank(crate::edit::ex::ExRange),
    SetMark(char),
    GotoMark {
        name: char,
        linewise: bool,
    },
    JumpList {
        back: bool,
        count: usize,
    },
    ChangeList {
        back: bool,
        count: usize,
    },
    ScrollViewport(i32),
    ScrollCursorTo(ScrollPlacement),
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
    /// Put back every caret the user added, keeping the one they placed last.
    CollapseCarets,
    /// Add a caret one row above or below the active one, in the same column.
    ///
    /// The keyboard's way into multi-caret editing. Alt-click was the only other, which left the
    /// feature unreachable without a mouse.
    AddCaretVertically(VerticalDirection),
}

/// Which way [`EditCommand::AddCaretVertically`] grows the caret set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalDirection {
    Up,
    Down,
}

impl EditCommand {
    /// Whether this means "do it at each caret" rather than "do it to the buffer".
    ///
    /// Deliberately a small allowlist. Multi-caret editing is reachable from the VS Code keymap
    /// only, so this covers typing, deleting and moving; everything else — undo, search, ex
    /// ranges, folds, whole-buffer replacement — runs once against the first caret, which is
    /// what a command with buffer-wide meaning should do.
    pub fn is_per_caret(&self) -> bool {
        matches!(
            self,
            Self::Move(_)
                | Self::Select(_)
                | Self::InsertText(_)
                | Self::OvertypeText(_)
                | Self::InsertNewline
                | Self::InsertTab
                | Self::DeleteBack
                | Self::DeleteForward
                | Self::DeleteWordBack
                | Self::ReplaceChar { .. }
                | Self::SelectTextObject(_)
                | Self::SwapSelectionEnds
        )
    }
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
