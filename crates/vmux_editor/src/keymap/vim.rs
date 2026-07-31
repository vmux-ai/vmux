use crate::edit::command::{EditCommand, EditMode, Motion, Operator, Target};
use crate::edit::text_object::{TextObject, TextObjectKind};
use crate::keymap::{KeyInput, Keymap};

/// Whether a pending `i`/`a` prefix selects the object's interior or includes its delimiters.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectScope {
    Inner,
    Around,
}

#[derive(Default)]
pub struct VimKeymap {
    mode: EditMode,
    count: Option<usize>,
    op_count: Option<usize>,
    pending_op: Option<Operator>,
    pending_op_key: Option<char>,
    pending_object: Option<ObjectScope>,
    register: Option<char>,
    register_pending: bool,
    replace_pending: bool,
    g_pending: bool,
    z_pending: bool,
    ex: Option<String>,
}

fn motion_for(key: &str) -> Option<Motion> {
    Some(match key {
        "h" | "ArrowLeft" => Motion::LeftBounded,
        "l" | "ArrowRight" => Motion::RightBounded,
        "j" | "ArrowDown" => Motion::Down,
        "k" | "ArrowUp" => Motion::Up,
        "w" => Motion::WordNext,
        "b" => Motion::WordPrev,
        "e" => Motion::WordEnd,
        "0" => Motion::LineStart,
        "^" => Motion::FirstNonBlank,
        "$" => Motion::LineEnd,
        "{" => Motion::ParagraphPrev,
        "}" => Motion::ParagraphNext,
        _ => return None,
    })
}

/// The operator a `g`-prefixed key introduces, and the key that doubles it (`guu`).
fn g_operator(key: &str) -> Option<(Operator, char)> {
    Some(match key {
        "u" => (Operator::Lower, 'u'),
        "U" => (Operator::Upper, 'U'),
        "~" => (Operator::ToggleCase, '~'),
        _ => return None,
    })
}

fn operator_for(key: &str) -> Option<(Operator, char)> {
    Some(match key {
        "d" => (Operator::Delete, 'd'),
        "c" => (Operator::Change, 'c'),
        "y" => (Operator::Yank, 'y'),
        ">" => (Operator::Indent, '>'),
        "<" => (Operator::Outdent, '<'),
        "=" => (Operator::Indent, '='),
        _ => return None,
    })
}

fn text_object_kind(key: &str) -> Option<TextObjectKind> {
    Some(match key {
        "w" => TextObjectKind::Word,
        "W" => TextObjectKind::BigWord,
        "s" => TextObjectKind::Sentence,
        "p" => TextObjectKind::Paragraph,
        "(" | ")" | "b" => TextObjectKind::Paren,
        "[" | "]" => TextObjectKind::Bracket,
        "{" | "}" | "B" => TextObjectKind::Brace,
        "<" | ">" => TextObjectKind::Angle,
        "\"" => TextObjectKind::DoubleQuote,
        "'" => TextObjectKind::SingleQuote,
        "`" => TextObjectKind::BackQuote,
        "t" => TextObjectKind::Tag,
        _ => return None,
    })
}

fn single_char(key: &str) -> Option<char> {
    let mut chars = key.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

impl VimKeymap {
    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    /// Vim multiplies a count before the operator with one before the motion: `2d3w` is six words.
    fn take_operator_count(&mut self) -> usize {
        let outer = self.op_count.take().unwrap_or(1);
        let inner = self.count.take().unwrap_or(1);
        outer.saturating_mul(inner).max(1)
    }

    fn take_register(&mut self) -> Option<char> {
        self.register.take()
    }

    fn reset(&mut self) {
        self.count = None;
        self.op_count = None;
        self.pending_op = None;
        self.pending_op_key = None;
        self.pending_object = None;
        self.register = None;
        self.register_pending = false;
        self.replace_pending = false;
        self.g_pending = false;
        self.z_pending = false;
    }

    fn start_operator(&mut self, operator: Operator, key: char) {
        self.pending_op = Some(operator);
        self.pending_op_key = Some(key);
        self.op_count = self.count.take();
    }

    fn enter_insert(&mut self) {
        self.mode = EditMode::Insert;
    }

    fn op(&mut self, operator: Operator, target: Target) -> EditCommand {
        EditCommand::Op {
            operator,
            target,
            register: self.take_register(),
        }
    }

    /// Apply a pending operator to a motion, a text object, a doubled key (`dd`), or abandon it.
    fn operator_pending(&mut self, operator: Operator, key: &str) -> Vec<EditCommand> {
        if let Some(scope) = self.pending_object.take() {
            let count = self.take_operator_count();
            self.pending_op_key = None;
            let Some(kind) = text_object_kind(key) else {
                return vec![];
            };
            let target = Target::TextObject(TextObject {
                kind,
                around: scope == ObjectScope::Around,
                count,
            });
            if operator == Operator::Change {
                self.enter_insert();
            }
            return vec![self.op(operator, target)];
        }

        if key == "i" || key == "a" {
            self.pending_object = Some(if key == "i" {
                ObjectScope::Inner
            } else {
                ObjectScope::Around
            });
            self.pending_op = Some(operator);
            return vec![];
        }

        let doubled = single_char(key)
            .zip(self.pending_op_key)
            .is_some_and(|(a, b)| a == b);
        let count = self.take_operator_count();
        self.pending_op_key = None;

        let target = if doubled {
            Target::Line(count)
        } else if let Some(m) = motion_for(key) {
            Target::Motion(m, count)
        } else if key == "G" {
            Target::Motion(Motion::DocEnd, 1)
        } else {
            return vec![];
        };

        if operator == Operator::Change {
            self.enter_insert();
        }
        vec![self.op(operator, target)]
    }

    fn normal(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let key = k.key.as_str();

        if self.register_pending {
            self.register_pending = false;
            if let Some(c) = single_char(key) {
                self.register = Some(c);
            }
            return vec![];
        }

        if self.replace_pending {
            self.replace_pending = false;
            let count = self.take_count();
            if let Some(ch) = single_char(key) {
                return vec![ReplaceChar { ch, count }];
            }
            return vec![];
        }

        if let Some(c) = single_char(key)
            && c.is_ascii_digit()
            && !(c == '0' && self.count.is_none())
        {
            let d = c as usize - '0' as usize;
            self.count = Some(self.count.unwrap_or(0) * 10 + d);
            return vec![];
        }

        if let Some(operator) = self.pending_op.take() {
            return self.operator_pending(operator, key);
        }

        if self.g_pending {
            self.g_pending = false;
            if let Some((operator, op_key)) = g_operator(key) {
                self.start_operator(operator, op_key);
                return vec![];
            }
            return match key {
                "g" => {
                    let line = self.count.take();
                    vec![match line {
                        Some(n) => Move(Motion::GotoLine(n.saturating_sub(1) as u32)),
                        None => Move(Motion::DocStart),
                    }]
                }
                "d" => vec![GotoDefinition],
                "r" => vec![FindReferences],
                "J" => vec![JoinLines {
                    count: self.take_count(),
                    spaces: false,
                }],
                _ => vec![],
            };
        }

        if self.z_pending {
            self.z_pending = false;
            return match key {
                "a" => vec![FoldToggle],
                "o" => vec![FoldOpen],
                "c" => vec![FoldClose],
                "A" => vec![FoldToggleRecursive],
                "R" => vec![UnfoldAll],
                "M" => vec![FoldAll],
                _ => vec![],
            };
        }

        if key == "r" && k.mods.ctrl {
            let n = self.take_count();
            return std::iter::repeat_n(Redo, n).collect();
        }

        if let Some(m) = motion_for(key) {
            let n = self.take_count();
            return std::iter::repeat_n(Move(m), n).collect();
        }

        if let Some((operator, op_key)) = operator_for(key) {
            self.start_operator(operator, op_key);
            return vec![];
        }

        match key {
            "g" => {
                self.g_pending = true;
                vec![]
            }
            "z" => {
                self.z_pending = true;
                vec![]
            }
            "\"" => {
                self.register_pending = true;
                vec![]
            }
            "r" => {
                self.replace_pending = true;
                vec![]
            }
            "G" => {
                let cmd = match self.count.take() {
                    Some(n) => Move(Motion::GotoLine(n.saturating_sub(1) as u32)),
                    None => Move(Motion::DocEnd),
                };
                vec![cmd]
            }
            "i" => {
                self.enter_insert();
                vec![SetMode(EditMode::Insert)]
            }
            "a" => {
                self.enter_insert();
                vec![Move(Motion::Right), SetMode(EditMode::Insert)]
            }
            "I" => {
                self.enter_insert();
                vec![Move(Motion::FirstNonBlank), SetMode(EditMode::Insert)]
            }
            "A" => {
                self.enter_insert();
                vec![Move(Motion::LineEnd), SetMode(EditMode::Insert)]
            }
            "o" => {
                self.enter_insert();
                vec![OpenLine { above: false }]
            }
            "O" => {
                self.enter_insert();
                vec![OpenLine { above: true }]
            }
            "x" => {
                let count = self.take_count();
                vec![self.op(Operator::Delete, Target::Motion(Motion::Right, count))]
            }
            "X" => {
                let count = self.take_count();
                vec![self.op(Operator::Delete, Target::Motion(Motion::Left, count))]
            }
            "D" => {
                let _ = self.take_count();
                vec![self.op(Operator::Delete, Target::Motion(Motion::LineEnd, 1))]
            }
            "C" => {
                let _ = self.take_count();
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Motion(Motion::LineEnd, 1))]
            }
            "s" => {
                let count = self.take_count();
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Motion(Motion::Right, count))]
            }
            "S" => {
                let count = self.take_count();
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Line(count))]
            }
            "Y" => {
                let count = self.take_count();
                vec![self.op(Operator::Yank, Target::Line(count))]
            }
            "J" => vec![JoinLines {
                count: self.take_count(),
                spaces: true,
            }],
            "~" => {
                let count = self.take_count();
                let mut cmds =
                    vec![self.op(Operator::ToggleCase, Target::Motion(Motion::Right, count))];
                cmds.extend(std::iter::repeat_n(Move(Motion::RightBounded), count));
                cmds
            }
            "p" | "P" => {
                let count = self.take_count();
                vec![Put {
                    before: key == "P",
                    count,
                    register: self.take_register(),
                }]
            }
            "K" => vec![Hover],
            "u" => {
                let n = self.take_count();
                std::iter::repeat_n(Undo, n).collect()
            }
            "v" => {
                self.mode = EditMode::Visual;
                vec![SetMode(EditMode::Visual)]
            }
            "V" => {
                self.mode = EditMode::VisualLine;
                vec![SetMode(EditMode::VisualLine)]
            }
            ":" => {
                self.ex = Some(String::new());
                vec![]
            }
            "Escape" => {
                self.reset();
                vec![]
            }
            _ => vec![],
        }
    }

    fn visual(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let key = k.key.as_str();

        if let Some(scope) = self.pending_object.take() {
            let count = self.take_count();
            let Some(kind) = text_object_kind(key) else {
                return vec![];
            };
            return vec![SelectTextObject(TextObject {
                kind,
                around: scope == ObjectScope::Around,
                count,
            })];
        }

        if self.register_pending {
            self.register_pending = false;
            if let Some(c) = single_char(key) {
                self.register = Some(c);
            }
            return vec![];
        }

        if self.replace_pending {
            self.replace_pending = false;
            if let Some(ch) = single_char(key) {
                self.mode = EditMode::Normal;
                return vec![
                    Op {
                        operator: Operator::Delete,
                        target: Target::Selection,
                        register: Some(crate::edit::register::BLACKHOLE),
                    },
                    InsertText(ch.to_string()),
                    SetMode(EditMode::Normal),
                ];
            }
            return vec![];
        }

        if let Some(c) = single_char(key)
            && c.is_ascii_digit()
            && !(c == '0' && self.count.is_none())
        {
            let d = c as usize - '0' as usize;
            self.count = Some(self.count.unwrap_or(0) * 10 + d);
            return vec![];
        }

        if let Some(m) = motion_for(key) {
            let n = self.take_count();
            return std::iter::repeat_n(Select(m), n).collect();
        }

        if self.g_pending {
            self.g_pending = false;
            return match key {
                "u" => self.visual_op(Operator::Lower),
                "U" => self.visual_op(Operator::Upper),
                "~" => self.visual_op(Operator::ToggleCase),
                "g" => vec![Select(Motion::DocStart)],
                _ => vec![],
            };
        }

        match key {
            "g" => {
                self.g_pending = true;
                vec![]
            }
            "\"" => {
                self.register_pending = true;
                vec![]
            }
            "r" => {
                self.replace_pending = true;
                vec![]
            }
            "i" | "a" => {
                self.pending_object = Some(if key == "i" {
                    ObjectScope::Inner
                } else {
                    ObjectScope::Around
                });
                vec![]
            }
            "d" | "x" => self.visual_op(Operator::Delete),
            "y" => self.visual_op(Operator::Yank),
            "u" => self.visual_op(Operator::Lower),
            "U" => self.visual_op(Operator::Upper),
            "~" => self.visual_op(Operator::ToggleCase),
            ">" => self.visual_op(Operator::Indent),
            "<" => self.visual_op(Operator::Outdent),
            "c" | "s" => {
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Selection)]
            }
            "D" | "X" => {
                self.mode = EditMode::VisualLine;
                let cmd = self.op(Operator::Delete, Target::Selection);
                self.mode = EditMode::Normal;
                vec![
                    SetMode(EditMode::VisualLine),
                    cmd,
                    SetMode(EditMode::Normal),
                ]
            }
            "C" | "S" | "R" => {
                self.enter_insert();
                let cmd = self.op(Operator::Change, Target::Selection);
                vec![SetMode(EditMode::VisualLine), cmd]
            }
            "Y" => {
                self.mode = EditMode::Normal;
                let cmd = self.op(Operator::Yank, Target::Selection);
                vec![
                    SetMode(EditMode::VisualLine),
                    cmd,
                    SetMode(EditMode::Normal),
                ]
            }
            "J" => {
                self.mode = EditMode::Normal;
                vec![
                    JoinLines {
                        count: 2,
                        spaces: true,
                    },
                    SetMode(EditMode::Normal),
                ]
            }
            "p" | "P" => {
                self.mode = EditMode::Normal;
                vec![Put {
                    before: key == "P",
                    count: self.take_count(),
                    register: self.take_register(),
                }]
            }
            "o" => vec![SwapSelectionEnds],
            "v" => {
                self.mode = if self.mode == EditMode::Visual {
                    EditMode::Normal
                } else {
                    EditMode::Visual
                };
                vec![SetMode(self.mode)]
            }
            "V" => {
                self.mode = if self.mode == EditMode::VisualLine {
                    EditMode::Normal
                } else {
                    EditMode::VisualLine
                };
                vec![SetMode(self.mode)]
            }
            "Escape" => {
                self.reset();
                self.mode = EditMode::Normal;
                vec![SetMode(EditMode::Normal)]
            }
            _ => vec![],
        }
    }

    fn visual_op(&mut self, operator: Operator) -> Vec<EditCommand> {
        self.mode = EditMode::Normal;
        let cmd = self.op(operator, Target::Selection);
        vec![cmd, EditCommand::SetMode(EditMode::Normal)]
    }

    fn insert(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        match k.key.as_str() {
            "Escape" => {
                self.mode = EditMode::Normal;
                vec![Move(Motion::LeftBounded), SetMode(EditMode::Normal)]
            }
            "Backspace" => vec![DeleteBack],
            "Delete" => vec![DeleteForward],
            "Enter" => vec![InsertNewline],
            "Tab" => vec![InsertTab],
            "ArrowLeft" => vec![Move(Motion::Left)],
            "ArrowRight" => vec![Move(Motion::Right)],
            "ArrowUp" => vec![Move(Motion::Up)],
            "ArrowDown" => vec![Move(Motion::Down)],
            _ => vec![],
        }
    }

    fn ex_key(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        match k.key.as_str() {
            "Enter" => {
                let cmd = self.ex.take().unwrap_or_default();
                match cmd.as_str() {
                    "w" | "wq" | "x" => vec![EditCommand::Save],
                    _ => vec![],
                }
            }
            "Escape" => {
                self.ex = None;
                vec![]
            }
            "Backspace" => {
                if let Some(buf) = self.ex.as_mut() {
                    buf.pop();
                }
                vec![]
            }
            key if key.len() == 1 => {
                if let Some(buf) = self.ex.as_mut() {
                    buf.push_str(key);
                }
                vec![]
            }
            _ => vec![],
        }
    }
}

impl Keymap for VimKeymap {
    fn mode(&self) -> EditMode {
        self.mode
    }
    fn pointer_selection_mode(&mut self, extend: bool) -> Option<EditCommand> {
        match (extend, self.mode) {
            (true, EditMode::Normal) => {
                self.mode = EditMode::Visual;
                Some(EditCommand::SetMode(EditMode::Visual))
            }
            (false, EditMode::Visual | EditMode::VisualLine) => {
                self.mode = EditMode::Normal;
                Some(EditCommand::SetMode(EditMode::Normal))
            }
            _ => None,
        }
    }
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        #[cfg(target_os = "macos")]
        if k.mods.meta && !k.mods.ctrl && !k.mods.alt {
            let command = match k.key.to_ascii_lowercase().as_str() {
                "c" => Some(EditCommand::Op {
                    operator: Operator::Yank,
                    target: Target::Selection,
                    register: None,
                }),
                "x" => Some(EditCommand::Op {
                    operator: Operator::Delete,
                    target: Target::Selection,
                    register: None,
                }),
                "v" => Some(EditCommand::Put {
                    before: true,
                    count: 1,
                    register: None,
                }),
                "a" => Some(EditCommand::Move(Motion::DocStart)),
                "s" => Some(EditCommand::Save),
                "z" if k.mods.shift => Some(EditCommand::Redo),
                "z" => Some(EditCommand::Undo),
                "y" => Some(EditCommand::Redo),
                _ => None,
            };
            if let Some(command) = command {
                self.reset();
                return if k.key.eq_ignore_ascii_case("a") {
                    vec![command, EditCommand::Select(Motion::DocEnd)]
                } else {
                    vec![command]
                };
            }
        }
        if k.mods.ctrl && !k.mods.meta && !k.mods.alt {
            let direction = match k.key.to_ascii_lowercase().as_str() {
                "e" => Some(1),
                "y" => Some(-1),
                _ => None,
            };
            if let Some(direction) = direction {
                let count = self.take_count().min(i32::MAX as usize) as i32;
                self.reset();
                return vec![EditCommand::ScrollViewport(direction * count)];
            }
            let motion = match k.key.to_ascii_lowercase().as_str() {
                "n" => Some(Motion::Down),
                "p" => Some(Motion::Up),
                _ => None,
            };
            if let Some(motion) = motion {
                self.reset();
                return if self.mode.is_visual() {
                    vec![EditCommand::Select(motion)]
                } else {
                    vec![EditCommand::Move(motion)]
                };
            }
        }
        if k.mods.ctrl && k.key.eq_ignore_ascii_case("c") {
            if self.ex.take().is_some() {
                return vec![];
            }
            return match self.mode {
                EditMode::Insert => {
                    self.mode = EditMode::Normal;
                    vec![
                        EditCommand::Move(Motion::LeftBounded),
                        EditCommand::SetMode(EditMode::Normal),
                    ]
                }
                EditMode::Visual | EditMode::VisualLine => {
                    self.reset();
                    self.mode = EditMode::Normal;
                    vec![EditCommand::SetMode(EditMode::Normal)]
                }
                EditMode::Normal => {
                    self.reset();
                    vec![]
                }
            };
        }
        if self.ex.is_some() {
            return self.ex_key(k);
        }
        match self.mode {
            EditMode::Insert => self.insert(k),
            EditMode::Visual | EditMode::VisualLine => self.visual(k),
            EditMode::Normal => self.normal(k),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

    fn k(key: &str) -> KeyInput {
        KeyInput {
            key: key.into(),
            mods: Mods::default(),
            repeat: false,
        }
    }
    fn chord(key: &str, mods: Mods) -> KeyInput {
        KeyInput {
            key: key.into(),
            mods,
            repeat: false,
        }
    }
    fn ctrl() -> Mods {
        Mods {
            ctrl: true,
            ..Default::default()
        }
    }
    fn run(km: &mut VimKeymap, seq: &[&str]) -> Vec<EditCommand> {
        let mut out = Vec::new();
        for s in seq {
            out.extend(km.handle(&k(s)));
        }
        out
    }
    fn op(operator: Operator, target: Target) -> EditCommand {
        EditCommand::Op {
            operator,
            target,
            register: None,
        }
    }

    #[test]
    fn operators_pair_with_motions_and_doubled_keys() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
        );
        assert_eq!(
            run(&mut km, &["d", "d"]),
            vec![op(Operator::Delete, Target::Line(1))]
        );
        assert_eq!(
            run(&mut km, &["y", "y"]),
            vec![op(Operator::Yank, Target::Line(1))]
        );
        assert_eq!(
            run(&mut km, &["c", "c"]),
            vec![op(Operator::Change, Target::Line(1))]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn counts_multiply_across_operator_and_motion() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "3", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 3))]
        );
        assert_eq!(
            run(&mut km, &["2", "d", "3", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 6))]
        );
        assert_eq!(
            run(&mut km, &["2", "d", "d"]),
            vec![op(Operator::Delete, Target::Line(2))]
        );
    }

    fn object(kind: TextObjectKind, around: bool, count: usize) -> Target {
        Target::TextObject(TextObject {
            kind,
            around,
            count,
        })
    }

    #[test]
    fn operators_take_text_objects() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "i", "w"]),
            vec![op(Operator::Delete, object(TextObjectKind::Word, false, 1))]
        );
        assert_eq!(
            run(&mut km, &["c", "a", "\""]),
            vec![op(
                Operator::Change,
                object(TextObjectKind::DoubleQuote, true, 1)
            )]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn text_object_counts_come_from_the_operator() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["2", "d", "i", "("]),
            vec![op(
                Operator::Delete,
                object(TextObjectKind::Paren, false, 2)
            )]
        );
    }

    #[test]
    fn visual_mode_selects_text_objects() {
        let mut km = VimKeymap::default();
        run(&mut km, &["v"]);
        assert_eq!(
            run(&mut km, &["i", "p"]),
            vec![EditCommand::SelectTextObject(TextObject {
                kind: TextObjectKind::Paragraph,
                around: false,
                count: 1,
            })]
        );
    }

    #[test]
    fn an_unknown_object_key_abandons_the_operator() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &["d", "i", "z"]), vec![]);
        assert_eq!(
            run(&mut km, &["d", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
        );
    }

    #[test]
    fn count_repeats_motion() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["3", "j"]),
            vec![
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down)
            ]
        );
    }

    #[test]
    fn register_prefix_routes_yank_and_put() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["\"", "a", "y", "y"]),
            vec![EditCommand::Op {
                operator: Operator::Yank,
                target: Target::Line(1),
                register: Some('a'),
            }]
        );
        assert_eq!(
            run(&mut km, &["\"", "a", "p"]),
            vec![EditCommand::Put {
                before: false,
                count: 1,
                register: Some('a'),
            }]
        );
    }

    #[test]
    fn indent_operators_use_doubled_keys() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &[">", ">"]),
            vec![op(Operator::Indent, Target::Line(1))]
        );
        assert_eq!(
            run(&mut km, &["3", "<", "<"]),
            vec![op(Operator::Outdent, Target::Line(3))]
        );
    }

    #[test]
    fn g_prefixed_case_operators() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["g", "u", "w"]),
            vec![op(Operator::Lower, Target::Motion(Motion::WordNext, 1))]
        );
        assert_eq!(
            run(&mut km, &["g", "U", "U"]),
            vec![op(Operator::Upper, Target::Line(1))]
        );
    }

    #[test]
    fn join_replace_and_toggle_case() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["3", "J"]),
            vec![EditCommand::JoinLines {
                count: 3,
                spaces: true
            }]
        );
        assert_eq!(
            run(&mut km, &["g", "J"]),
            vec![EditCommand::JoinLines {
                count: 1,
                spaces: false
            }]
        );
        assert_eq!(
            run(&mut km, &["2", "r", "x"]),
            vec![EditCommand::ReplaceChar { ch: 'x', count: 2 }]
        );
        assert_eq!(
            run(&mut km, &["~"]),
            vec![
                op(Operator::ToggleCase, Target::Motion(Motion::Right, 1)),
                EditCommand::Move(Motion::RightBounded)
            ]
        );
    }

    #[test]
    fn shift_d_c_s_and_y_bindings() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["D"]),
            vec![op(Operator::Delete, Target::Motion(Motion::LineEnd, 1))]
        );
        assert_eq!(
            run(&mut km, &["C"]),
            vec![op(Operator::Change, Target::Motion(Motion::LineEnd, 1))]
        );
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["S"]),
            vec![op(Operator::Change, Target::Line(1))]
        );
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["2", "Y"]),
            vec![op(Operator::Yank, Target::Line(2))]
        );
        assert_eq!(
            run(&mut km, &["3", "x"]),
            vec![op(Operator::Delete, Target::Motion(Motion::Right, 3))]
        );
    }

    #[test]
    fn put_carries_a_count_and_direction() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["2", "p"]),
            vec![EditCommand::Put {
                before: false,
                count: 2,
                register: None
            }]
        );
        assert_eq!(
            run(&mut km, &["P"]),
            vec![EditCommand::Put {
                before: true,
                count: 1,
                register: None
            }]
        );
    }

    #[test]
    fn open_line_replaces_the_newline_dance() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["o"]),
            vec![EditCommand::OpenLine { above: false }]
        );
        assert_eq!(km.mode(), EditMode::Insert);
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["O"]),
            vec![EditCommand::OpenLine { above: true }]
        );
    }

    #[test]
    fn visual_operators_act_on_the_selection() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["v"]),
            vec![EditCommand::SetMode(EditMode::Visual)]
        );
        assert_eq!(
            run(&mut km, &["l"]),
            vec![EditCommand::Select(Motion::RightBounded)]
        );
        assert_eq!(
            run(&mut km, &["y"]),
            vec![
                op(Operator::Yank, Target::Selection),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn visual_motions_take_counts() {
        let mut km = VimKeymap::default();
        run(&mut km, &["v"]);
        assert_eq!(
            run(&mut km, &["3", "j"]),
            vec![
                EditCommand::Select(Motion::Down),
                EditCommand::Select(Motion::Down),
                EditCommand::Select(Motion::Down)
            ]
        );
    }

    #[test]
    fn visual_indent_and_swap_ends() {
        let mut km = VimKeymap::default();
        run(&mut km, &["v"]);
        assert_eq!(run(&mut km, &["o"]), vec![EditCommand::SwapSelectionEnds]);
        assert_eq!(
            run(&mut km, &[">"]),
            vec![
                op(Operator::Indent, Target::Selection),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
    }

    #[test]
    fn document_jump_bindings() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["g", "g"]),
            vec![EditCommand::Move(Motion::DocStart)]
        );
        assert_eq!(
            run(&mut km, &["G"]),
            vec![EditCommand::Move(Motion::DocEnd)]
        );
        assert_eq!(
            run(&mut km, &["5", "G"]),
            vec![EditCommand::Move(Motion::GotoLine(4))]
        );
        assert_eq!(
            run(&mut km, &["5", "g", "g"]),
            vec![EditCommand::Move(Motion::GotoLine(4))]
        );
    }

    #[test]
    fn braces_move_by_paragraph() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["{"]),
            vec![EditCommand::Move(Motion::ParagraphPrev)]
        );
        assert_eq!(
            run(&mut km, &["}"]),
            vec![EditCommand::Move(Motion::ParagraphNext)]
        );
    }

    #[test]
    fn fold_and_lsp_bindings() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &["z", "a"]), vec![EditCommand::FoldToggle]);
        assert_eq!(run(&mut km, &["z", "R"]), vec![EditCommand::UnfoldAll]);
        assert_eq!(run(&mut km, &["g", "d"]), vec![EditCommand::GotoDefinition]);
        assert_eq!(run(&mut km, &["g", "r"]), vec![EditCommand::FindReferences]);
        assert_eq!(run(&mut km, &["K"]), vec![EditCommand::Hover]);
    }

    #[test]
    fn insert_mode_entry_and_escape() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["i"]),
            vec![EditCommand::SetMode(EditMode::Insert)]
        );
        assert_eq!(km.mode(), EditMode::Insert);
        assert_eq!(
            run(&mut km, &["Escape"]),
            vec![
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn arrow_keys_navigate_in_normal_mode() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["ArrowDown"]),
            vec![EditCommand::Move(Motion::Down)]
        );
    }

    #[test]
    fn ctrl_navigation_never_falls_through_to_vim_commands() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&chord("n", ctrl())),
            vec![EditCommand::Move(Motion::Down)]
        );
        assert_eq!(
            km.handle(&chord("p", ctrl())),
            vec![EditCommand::Move(Motion::Up)]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn ctrl_e_y_scroll_the_viewport_with_counts() {
        let mut km = VimKeymap::default();
        run(&mut km, &["3"]);
        assert_eq!(
            km.handle(&chord("e", ctrl())),
            vec![EditCommand::ScrollViewport(3)]
        );
        assert_eq!(
            km.handle(&chord("y", ctrl())),
            vec![EditCommand::ScrollViewport(-1)]
        );
    }

    #[test]
    fn ctrl_r_redoes() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&chord("r", ctrl())), vec![EditCommand::Redo]);
    }

    #[test]
    fn ctrl_c_escapes_insert_and_visual() {
        let mut km = VimKeymap::default();
        run(&mut km, &["i"]);
        assert_eq!(
            km.handle(&chord("c", ctrl())),
            vec![
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        run(&mut km, &["v"]);
        assert_eq!(
            km.handle(&chord("c", ctrl())),
            vec![EditCommand::SetMode(EditMode::Normal)]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn ex_write_saves() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &[":", "w", "Enter"]), vec![EditCommand::Save]);
        assert_eq!(run(&mut km, &[":", "q", "Enter"]), vec![]);
    }

    #[test]
    fn pointer_drag_enters_visual_and_click_returns_normal() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.pointer_selection_mode(true),
            Some(EditCommand::SetMode(EditMode::Visual))
        );
        assert_eq!(km.mode(), EditMode::Visual);
        assert_eq!(
            km.pointer_selection_mode(false),
            Some(EditCommand::SetMode(EditMode::Normal))
        );
    }
}
