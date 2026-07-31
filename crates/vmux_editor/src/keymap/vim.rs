use crate::edit::command::{EditCommand, EditMode, Motion, Operator, ScrollPlacement, Target};
use crate::edit::text_object::{TextObject, TextObjectKind};
use crate::keymap::{KeyInput, Keymap};

/// Whether a pending `i`/`a` prefix selects the object's interior or includes its delimiters.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectScope {
    Inner,
    Around,
}

/// What the key after `m`, `` ` ``, or `'` names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkAction {
    Set,
    GotoExact,
    GotoLine,
}

/// One unit of replayable input.
///
/// Dot-repeat and macros both work by replaying what the user typed, and insert-mode characters
/// bypass the keymap over the IME path, so the stream has to carry both kinds of event.
#[derive(Clone)]
enum Recorded {
    Key(KeyInput),
    Text(String),
}

/// Whether a command changes buffer text, which is what `.` repeats. Yanks and motions do not.
fn is_change(cmd: &EditCommand) -> bool {
    match cmd {
        EditCommand::Op { operator, .. } => *operator != Operator::Yank,
        EditCommand::Put { .. }
        | EditCommand::ReplaceChar { .. }
        | EditCommand::JoinLines { .. }
        | EditCommand::OpenLine { .. }
        | EditCommand::InsertText(_)
        | EditCommand::InsertNewline
        | EditCommand::InsertTab
        | EditCommand::DeleteBack
        | EditCommand::DeleteForward
        | EditCommand::DeleteWordBack => true,
        _ => false,
    }
}

#[derive(Default)]
pub struct VimKeymap {
    mode: EditMode,
    count: Option<usize>,
    op_count: Option<usize>,
    pending_op: Option<Operator>,
    pending_op_key: Option<char>,
    pending_object: Option<ObjectScope>,
    pending_find: Option<(bool, bool)>,
    last_find: Option<(char, bool, bool)>,
    register: Option<char>,
    register_pending: bool,
    replace_pending: bool,
    g_pending: bool,
    z_pending: bool,
    ex: Option<(char, String)>,
    mode_before_ex: EditMode,
    pending_change: Vec<Recorded>,
    last_change: Vec<Recorded>,
    in_change: bool,
    replaying: bool,
    macro_pending: Option<bool>,
    macro_record: Option<(char, Vec<Recorded>)>,
    macros: std::collections::HashMap<char, Vec<Recorded>>,
    last_macro: Option<char>,
    insert_after_next: bool,
    mark_pending: Option<MarkAction>,
}

fn motion_for(key: &str) -> Option<Motion> {
    Some(match key {
        "h" | "ArrowLeft" | "Backspace" => Motion::LeftBounded,
        "l" | "ArrowRight" | " " => Motion::RightBounded,
        "j" | "ArrowDown" => Motion::Down,
        "k" | "ArrowUp" => Motion::Up,
        "w" => Motion::WordNext,
        "b" => Motion::WordPrev,
        "e" => Motion::WordEnd,
        "W" => Motion::BigWordNext,
        "B" => Motion::BigWordPrev,
        "E" => Motion::BigWordEnd,
        "0" => Motion::LineStart,
        "^" => Motion::FirstNonBlank,
        "$" => Motion::LineEnd,
        "{" => Motion::ParagraphPrev,
        "}" => Motion::ParagraphNext,
        "%" => Motion::MatchPair,
        "n" => Motion::SearchNext { reverse: false },
        "N" => Motion::SearchNext { reverse: true },
        "H" => Motion::ScreenTop,
        "M" => Motion::ScreenMiddle,
        "L" => Motion::ScreenBottom,
        "+" | "Enter" => Motion::NextLineStart,
        "-" => Motion::PrevLineStart,
        "_" => Motion::FirstNonBlank,
        _ => return None,
    })
}

/// `f`, `F`, `t`, and `T` all take the next key as their target character.
fn find_prefix(key: &str) -> Option<(bool, bool)> {
    Some(match key {
        "f" => (true, false),
        "F" => (false, false),
        "t" => (true, true),
        "T" => (false, true),
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
        self.pending_find = None;
        self.register = None;
        self.register_pending = false;
        self.replace_pending = false;
        self.g_pending = false;
        self.z_pending = false;
    }

    /// No operator or prefix is half-typed. A pending count still counts as a command start,
    /// because a count belongs to the command the next key names rather than to the previous one.
    fn is_command_start(&self) -> bool {
        self.pending_op.is_none()
            && self.pending_object.is_none()
            && self.pending_find.is_none()
            && self.macro_pending.is_none()
            && self.mark_pending.is_none()
            && self.ex.is_none()
            && !self.register_pending
            && !self.replace_pending
            && !self.g_pending
            && !self.z_pending
    }

    /// Nothing at all is half-typed, so no keys are owed to a command in progress.
    fn is_idle(&self) -> bool {
        self.is_command_start() && self.count.is_none() && self.op_count.is_none()
    }

    /// A prefix is waiting to consume the next key as a literal argument — a register or mark
    /// name, a text-object kind, a fold or `g` command — so it must not be read as a motion.
    fn awaits_literal_key(&self) -> bool {
        self.pending_object.is_some()
            || self.macro_pending.is_some()
            || self.mark_pending.is_some()
            || self.register_pending
            || self.replace_pending
            || self.g_pending
            || self.z_pending
    }

    fn replay(&mut self, inputs: &[Recorded], times: usize) -> Vec<EditCommand> {
        if self.replaying {
            return vec![];
        }
        self.replaying = true;
        let mut out = Vec::new();
        for _ in 0..times.max(1) {
            for input in inputs {
                match input {
                    Recorded::Key(k) => out.extend(self.dispatch(k)),
                    Recorded::Text(t) => out.push(EditCommand::InsertText(t.clone())),
                }
            }
        }
        self.replaying = false;
        out
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

    /// Route a resolved motion to the pending operator, a visual selection, or a plain move.
    fn motion_command(&mut self, m: Motion) -> Vec<EditCommand> {
        if let Some(operator) = self.pending_op.take() {
            let count = self.take_operator_count();
            self.pending_op_key = None;
            self.pending_object = None;
            if operator == Operator::Change {
                self.enter_insert();
            }
            return vec![self.op(operator, Target::Motion(m, count))];
        }
        let n = self.take_count();
        if self.mode.is_visual() {
            return std::iter::repeat_n(EditCommand::Select(m), n).collect();
        }
        std::iter::repeat_n(EditCommand::Move(m), n).collect()
    }

    /// Consume the character `f`/`F`/`t`/`T` was waiting for, or drop the whole pending command.
    fn resolve_find(&mut self, forward: bool, till: bool, key: &str) -> Vec<EditCommand> {
        let Some(ch) = single_char(key) else {
            self.reset();
            return vec![];
        };
        self.last_find = Some((ch, forward, till));
        self.motion_command(Motion::FindChar { ch, forward, till })
    }

    fn repeat_find(&mut self, reverse: bool) -> Vec<EditCommand> {
        let Some((ch, forward, till)) = self.last_find else {
            return vec![];
        };
        let forward = if reverse { !forward } else { forward };
        self.motion_command(Motion::FindChar { ch, forward, till })
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

        if let Some((forward, till)) = self.pending_find.take() {
            return self.resolve_find(forward, till, key);
        }

        if let Some(action) = self.mark_pending.take() {
            let Some(name) = single_char(key) else {
                return vec![];
            };
            return match action {
                MarkAction::Set => vec![SetMark(name)],
                MarkAction::GotoExact => vec![GotoMark {
                    name,
                    linewise: false,
                }],
                MarkAction::GotoLine => vec![GotoMark {
                    name,
                    linewise: true,
                }],
            };
        }

        if let Some(record) = self.macro_pending.take() {
            let Some(reg) = single_char(key) else {
                return vec![];
            };
            if record {
                self.macro_record = Some((reg, Vec::new()));
                return vec![];
            }
            let reg = if reg == '@' {
                match self.last_macro {
                    Some(r) => r,
                    None => return vec![],
                }
            } else {
                reg
            };
            self.last_macro = Some(reg);
            let Some(body) = self.macros.get(&reg).cloned() else {
                return vec![];
            };
            let times = self.take_count();
            return self.replay(&body, times);
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

        if !self.awaits_literal_key() {
            if let Some((forward, till)) = find_prefix(key) {
                self.pending_find = Some((forward, till));
                return vec![];
            }
            if key == ";" || key == "," {
                return self.repeat_find(key == ",");
            }
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
                "e" => self.motion_command(Motion::WordEndPrev),
                "E" => self.motion_command(Motion::BigWordEndPrev),
                "_" => self.motion_command(Motion::LastNonBlank),
                "0" => self.motion_command(Motion::LineStart),
                "$" => self.motion_command(Motion::LineEnd),
                "j" => self.motion_command(Motion::Down),
                "k" => self.motion_command(Motion::Up),
                ";" | "," => vec![ChangeList {
                    back: key == ";",
                    count: self.take_count(),
                }],
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
                "z" => vec![ScrollCursorTo(ScrollPlacement::Center)],
                "t" => vec![ScrollCursorTo(ScrollPlacement::Top)],
                "b" => vec![ScrollCursorTo(ScrollPlacement::Bottom)],
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
            "|" => {
                let col = self.take_count();
                vec![Move(Motion::Column(col))]
            }
            "." => {
                let times = self.take_count();
                let body = std::mem::take(&mut self.last_change);
                let out = self.replay(&body, times);
                self.last_change = body;
                out
            }
            "*" => vec![SearchWord { forward: true }],
            "#" => vec![SearchWord { forward: false }],
            "m" => {
                self.mark_pending = Some(MarkAction::Set);
                vec![]
            }
            "`" => {
                self.mark_pending = Some(MarkAction::GotoExact);
                vec![]
            }
            "'" => {
                self.mark_pending = Some(MarkAction::GotoLine);
                vec![]
            }
            "q" => {
                if let Some((reg, body)) = self.macro_record.take() {
                    self.macros.insert(reg, body);
                    self.last_macro = Some(reg);
                } else {
                    self.macro_pending = Some(true);
                }
                vec![]
            }
            "@" => {
                self.macro_pending = Some(false);
                vec![]
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
                self.enter_ex(':');
                vec![]
            }
            "/" | "?" => {
                self.enter_ex(key.chars().next().unwrap());
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

        if let Some((forward, till)) = self.pending_find.take() {
            return self.resolve_find(forward, till, key);
        }

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

        if let Some((forward, till)) = find_prefix(key) {
            self.pending_find = Some((forward, till));
            return vec![];
        }

        if key == ";" || key == "," {
            return self.repeat_find(key == ",");
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
                "e" => self.motion_command(Motion::WordEndPrev),
                "E" => self.motion_command(Motion::BigWordEndPrev),
                "_" => self.motion_command(Motion::LastNonBlank),
                _ => vec![],
            };
        }

        match key {
            "g" => {
                self.g_pending = true;
                vec![]
            }
            "|" => {
                let col = self.take_count();
                vec![Select(Motion::Column(col))]
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

        if self.register_pending {
            self.register_pending = false;
            let Some(reg) = single_char(&k.key) else {
                return vec![];
            };
            return vec![Put {
                before: true,
                count: 1,
                register: Some(reg),
            }];
        }

        if k.mods.ctrl && !k.mods.meta && !k.mods.alt {
            return match k.key.to_ascii_lowercase().as_str() {
                "w" => vec![DeleteWordBack],
                "h" => vec![DeleteBack],
                "u" => vec![self.op(Operator::Delete, Target::Motion(Motion::LineStart, 1))],
                "t" => vec![self.op(Operator::Indent, Target::Line(1))],
                "d" => vec![self.op(Operator::Outdent, Target::Line(1))],
                "r" => {
                    self.register_pending = true;
                    vec![]
                }
                "o" => {
                    self.mode = EditMode::Normal;
                    self.insert_after_next = true;
                    vec![SetMode(EditMode::Normal)]
                }
                _ => vec![],
            };
        }

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
                let Some((prompt, body)) = self.ex.take() else {
                    return vec![];
                };
                self.mode = self.mode_before_ex;
                if prompt == '/' || prompt == '?' {
                    if body.is_empty() {
                        return vec![];
                    }
                    return vec![EditCommand::SetSearch {
                        pattern: body,
                        forward: prompt == '/',
                    }];
                }
                self.run_ex(&body)
            }
            "Escape" => {
                self.ex = None;
                self.mode = self.mode_before_ex;
                vec![]
            }
            "Backspace" => {
                let empty = match self.ex.as_mut() {
                    Some((_, buf)) => {
                        buf.pop();
                        buf.is_empty()
                    }
                    None => false,
                };
                if empty {
                    self.ex = None;
                    self.mode = self.mode_before_ex;
                }
                vec![]
            }
            key => {
                if let Some(c) = single_char(key)
                    && let Some((_, buf)) = self.ex.as_mut()
                {
                    buf.push(c);
                }
                vec![]
            }
        }
    }

    fn enter_ex(&mut self, prompt: char) {
        self.mode_before_ex = self.mode;
        self.ex = Some((prompt, String::new()));
        self.mode = EditMode::CommandLine;
    }

    fn run_ex(&mut self, body: &str) -> Vec<EditCommand> {
        use crate::edit::ex::{ExCommand, parse};
        let Some(cmd) = parse(body) else {
            return vec![];
        };
        match cmd {
            ExCommand::Write => vec![EditCommand::Save],
            ExCommand::WriteQuit => vec![EditCommand::Save],
            ExCommand::Quit { .. } => vec![],
            ExCommand::NoHighlight => vec![EditCommand::ClearSearchHighlight],
            ExCommand::Goto(line) => vec![EditCommand::Move(Motion::GotoLine(line as u32))],
            ExCommand::Delete(range) => vec![EditCommand::ExDelete(range)],
            ExCommand::Yank(range) => vec![EditCommand::ExYank(range)],
            ExCommand::Substitute {
                range,
                pattern,
                replacement,
                all,
            } => vec![EditCommand::Substitute {
                range,
                pattern,
                replacement,
                all,
            }],
        }
    }
}

impl Keymap for VimKeymap {
    fn mode(&self) -> EditMode {
        self.mode
    }
    fn command_line(&self) -> Option<String> {
        self.ex
            .as_ref()
            .map(|(prompt, body)| format!("{prompt}{body}"))
    }
    fn record_text(&mut self, text: &str) {
        if self.replaying {
            return;
        }
        if let Some((_, body)) = self.macro_record.as_mut() {
            body.push(Recorded::Text(text.to_string()));
        }
        self.pending_change.push(Recorded::Text(text.to_string()));
        self.in_change = true;
    }
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if self.replaying {
            return self.dispatch(k);
        }

        let stops_recording =
            self.macro_record.is_some() && k.key == "q" && self.mode == EditMode::Normal;
        if let Some((_, body)) = self.macro_record.as_mut()
            && !stops_recording
        {
            body.push(Recorded::Key(k.clone()));
        }

        let repeating = k.key == "." && self.mode == EditMode::Normal && self.is_command_start();
        if !repeating {
            self.pending_change.push(Recorded::Key(k.clone()));
        }

        let cmds = self.dispatch(k);

        if repeating {
            self.pending_change.clear();
            return cmds;
        }
        if cmds.iter().any(is_change) {
            self.in_change = true;
        }
        if self.in_change {
            if self.mode == EditMode::Normal {
                self.last_change = std::mem::take(&mut self.pending_change);
                self.in_change = false;
            }
        } else if self.mode == EditMode::Normal && self.is_idle() {
            self.pending_change.clear();
        }
        cmds
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
}

impl VimKeymap {
    /// Interpret one key without any dot-repeat or macro bookkeeping.
    fn dispatch(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        let armed = self.insert_after_next;
        let mut cmds = self.dispatch_inner(k);
        if armed && self.mode == EditMode::Normal && self.is_idle() && !cmds.is_empty() {
            self.insert_after_next = false;
            self.mode = EditMode::Insert;
            cmds.push(EditCommand::SetMode(EditMode::Insert));
        }
        cmds
    }

    fn dispatch_inner(&mut self, k: &KeyInput) -> Vec<EditCommand> {
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
        if k.mods.ctrl && !k.mods.meta && !k.mods.alt && !self.mode.accepts_text() {
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
            if k.key.eq_ignore_ascii_case("o") || k.key == "i" || k.key == "Tab" {
                let count = self.take_count();
                self.reset();
                return vec![EditCommand::JumpList {
                    back: k.key.eq_ignore_ascii_case("o"),
                    count,
                }];
            }
            let motion = match k.key.to_ascii_lowercase().as_str() {
                "n" => Some(Motion::Down),
                "p" => Some(Motion::Up),
                "d" => Some(Motion::HalfPageDown),
                "u" => Some(Motion::HalfPageUp),
                "f" => Some(Motion::PageDown),
                "b" => Some(Motion::PageUp),
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
                self.mode = self.mode_before_ex;
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
                EditMode::Normal | EditMode::CommandLine => {
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
            EditMode::Normal | EditMode::CommandLine => self.normal(k),
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

    fn find(ch: char, forward: bool, till: bool) -> Motion {
        Motion::FindChar { ch, forward, till }
    }

    #[test]
    fn find_char_prefixes_consume_the_next_key() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["f", ","]),
            vec![EditCommand::Move(find(',', true, false))]
        );
        assert_eq!(
            run(&mut km, &["T", "x"]),
            vec![EditCommand::Move(find('x', false, true))]
        );
    }

    #[test]
    fn semicolon_repeats_and_comma_reverses_the_last_find() {
        let mut km = VimKeymap::default();
        run(&mut km, &["f", "x"]);
        assert_eq!(
            run(&mut km, &[";"]),
            vec![EditCommand::Move(find('x', true, false))]
        );
        assert_eq!(
            run(&mut km, &[","]),
            vec![EditCommand::Move(find('x', false, false))]
        );
    }

    #[test]
    fn repeat_find_without_a_previous_find_does_nothing() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &[";"]), vec![]);
    }

    #[test]
    fn operators_compose_with_find_char() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "t", ","]),
            vec![op(
                Operator::Delete,
                Target::Motion(find(',', true, true), 1)
            )]
        );
        assert_eq!(
            run(&mut km, &["2", "d", "f", "x"]),
            vec![op(
                Operator::Delete,
                Target::Motion(find('x', true, false), 2)
            )]
        );
    }

    #[test]
    fn the_tag_object_is_not_shadowed_by_the_till_prefix() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "i", "t"]),
            vec![op(Operator::Delete, object(TextObjectKind::Tag, false, 1))]
        );
    }

    #[test]
    fn word_and_screen_motions_are_bound() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["W"]),
            vec![EditCommand::Move(Motion::BigWordNext)]
        );
        assert_eq!(
            run(&mut km, &["g", "e"]),
            vec![EditCommand::Move(Motion::WordEndPrev)]
        );
        assert_eq!(
            run(&mut km, &["%"]),
            vec![EditCommand::Move(Motion::MatchPair)]
        );
        assert_eq!(
            run(&mut km, &["H"]),
            vec![EditCommand::Move(Motion::ScreenTop)]
        );
        assert_eq!(
            run(&mut km, &["8", "|"]),
            vec![EditCommand::Move(Motion::Column(8))]
        );
    }

    #[test]
    fn half_and_full_page_scroll_chords() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&chord("d", ctrl())),
            vec![EditCommand::Move(Motion::HalfPageDown)]
        );
        assert_eq!(
            km.handle(&chord("b", ctrl())),
            vec![EditCommand::Move(Motion::PageUp)]
        );
    }

    #[test]
    fn z_prefix_serves_both_folds_and_scroll_placement() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &["z", "a"]), vec![EditCommand::FoldToggle]);
        assert_eq!(
            run(&mut km, &["z", "z"]),
            vec![EditCommand::ScrollCursorTo(ScrollPlacement::Center)]
        );
        assert_eq!(
            run(&mut km, &["z", "b"]),
            vec![EditCommand::ScrollCursorTo(ScrollPlacement::Bottom)]
        );
    }

    #[test]
    fn dot_repeats_the_last_change_not_the_last_motion() {
        let mut km = VimKeymap::default();
        run(&mut km, &["d", "w"]);
        run(&mut km, &["j", "l"]);
        assert_eq!(
            run(&mut km, &["."]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
        );
    }

    #[test]
    fn dot_takes_a_count_and_survives_reuse() {
        let mut km = VimKeymap::default();
        run(&mut km, &["x"]);
        let expected = op(Operator::Delete, Target::Motion(Motion::Right, 1));
        assert_eq!(
            run(&mut km, &["2", "."]),
            vec![expected.clone(), expected.clone()]
        );
        assert_eq!(run(&mut km, &["."]), vec![expected]);
    }

    #[test]
    fn dot_repeats_an_insert_session_including_typed_text() {
        let mut km = VimKeymap::default();
        run(&mut km, &["i"]);
        km.record_text("hi");
        run(&mut km, &["Escape"]);
        assert_eq!(
            run(&mut km, &["."]),
            vec![
                EditCommand::SetMode(EditMode::Insert),
                EditCommand::InsertText("hi".into()),
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal),
            ]
        );
    }

    #[test]
    fn a_yank_is_not_a_change() {
        let mut km = VimKeymap::default();
        run(&mut km, &["x"]);
        run(&mut km, &["y", "y"]);
        assert_eq!(
            run(&mut km, &["."]),
            vec![op(Operator::Delete, Target::Motion(Motion::Right, 1))]
        );
    }

    #[test]
    fn macros_record_and_replay_through_a_register() {
        let mut km = VimKeymap::default();
        run(&mut km, &["q", "a", "x", "j", "q"]);
        assert_eq!(
            run(&mut km, &["@", "a"]),
            vec![
                op(Operator::Delete, Target::Motion(Motion::Right, 1)),
                EditCommand::Move(Motion::Down),
            ]
        );
        assert_eq!(
            run(&mut km, &["@", "@"]),
            vec![
                op(Operator::Delete, Target::Motion(Motion::Right, 1)),
                EditCommand::Move(Motion::Down),
            ]
        );
    }

    #[test]
    fn a_counted_macro_replays_repeatedly() {
        let mut km = VimKeymap::default();
        run(&mut km, &["q", "b", "x", "q"]);
        let once = op(Operator::Delete, Target::Motion(Motion::Right, 1));
        assert_eq!(
            run(&mut km, &["3", "@", "b"]),
            vec![once.clone(), once.clone(), once]
        );
    }

    #[test]
    fn replaying_an_empty_register_is_inert() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &["@", "z"]), vec![]);
    }

    #[test]
    fn insert_mode_editing_chords() {
        let mut km = VimKeymap::default();
        run(&mut km, &["i"]);
        assert_eq!(
            km.handle(&chord("w", ctrl())),
            vec![EditCommand::DeleteWordBack]
        );
        assert_eq!(
            km.handle(&chord("u", ctrl())),
            vec![op(Operator::Delete, Target::Motion(Motion::LineStart, 1))]
        );
        assert_eq!(
            km.handle(&chord("t", ctrl())),
            vec![op(Operator::Indent, Target::Line(1))]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn ctrl_r_pastes_a_register_while_inserting() {
        let mut km = VimKeymap::default();
        run(&mut km, &["i"]);
        assert_eq!(km.handle(&chord("r", ctrl())), vec![]);
        assert_eq!(
            run(&mut km, &["a"]),
            vec![EditCommand::Put {
                before: true,
                count: 1,
                register: Some('a'),
            }]
        );
    }

    #[test]
    fn ctrl_o_runs_one_normal_command_then_returns_to_insert() {
        let mut km = VimKeymap::default();
        run(&mut km, &["i"]);
        assert_eq!(
            km.handle(&chord("o", ctrl())),
            vec![EditCommand::SetMode(EditMode::Normal)]
        );
        assert_eq!(km.mode(), EditMode::Normal);
        assert_eq!(
            run(&mut km, &["$"]),
            vec![
                EditCommand::Move(Motion::LineEnd),
                EditCommand::SetMode(EditMode::Insert),
            ]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn scroll_chords_do_not_fire_while_inserting() {
        let mut km = VimKeymap::default();
        run(&mut km, &["i"]);
        assert_eq!(km.handle(&chord("e", ctrl())), vec![]);
    }

    #[test]
    fn search_repeat_and_word_search_bindings() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["n"]),
            vec![EditCommand::Move(Motion::SearchNext { reverse: false })]
        );
        assert_eq!(
            run(&mut km, &["N"]),
            vec![EditCommand::Move(Motion::SearchNext { reverse: true })]
        );
        assert_eq!(
            run(&mut km, &["*"]),
            vec![EditCommand::SearchWord { forward: true }]
        );
        assert_eq!(
            run(&mut km, &["#"]),
            vec![EditCommand::SearchWord { forward: false }]
        );
    }

    #[test]
    fn an_operator_can_target_the_next_match() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "n"]),
            vec![op(
                Operator::Delete,
                Target::Motion(Motion::SearchNext { reverse: false }, 1)
            )]
        );
    }

    #[test]
    fn mark_bindings_set_and_recall() {
        let mut km = VimKeymap::default();
        assert_eq!(run(&mut km, &["m", "a"]), vec![EditCommand::SetMark('a')]);
        assert_eq!(
            run(&mut km, &["`", "a"]),
            vec![EditCommand::GotoMark {
                name: 'a',
                linewise: false
            }]
        );
        assert_eq!(
            run(&mut km, &["'", "a"]),
            vec![EditCommand::GotoMark {
                name: 'a',
                linewise: true
            }]
        );
    }

    #[test]
    fn jump_and_change_list_bindings() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&chord("o", ctrl())),
            vec![EditCommand::JumpList {
                back: true,
                count: 1
            }]
        );
        assert_eq!(
            km.handle(&chord("i", ctrl())),
            vec![EditCommand::JumpList {
                back: false,
                count: 1
            }]
        );
        assert_eq!(
            run(&mut km, &["g", ";"]),
            vec![EditCommand::ChangeList {
                back: true,
                count: 1
            }]
        );
    }

    #[test]
    fn a_mark_key_is_not_read_as_a_quote_object() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["d", "i", "'"]),
            vec![op(
                Operator::Delete,
                object(TextObjectKind::SingleQuote, false, 1)
            )]
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
    fn a_prompt_reports_command_line_mode_and_its_text() {
        let mut km = VimKeymap::default();
        run(&mut km, &[":", "w", "q"]);
        assert_eq!(km.mode(), EditMode::CommandLine);
        assert_eq!(km.command_line().as_deref(), Some(":wq"));
        run(&mut km, &["Escape"]);
        assert_eq!(km.mode(), EditMode::Normal);
        assert_eq!(km.command_line(), None);
    }

    #[test]
    fn slash_starts_a_forward_search_and_question_a_backward_one() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &["/", "f", "o", "o", "Enter"]),
            vec![EditCommand::SetSearch {
                pattern: "foo".into(),
                forward: true
            }]
        );
        assert_eq!(
            run(&mut km, &["?", "b", "a", "r", "Enter"]),
            vec![EditCommand::SetSearch {
                pattern: "bar".into(),
                forward: false
            }]
        );
    }

    #[test]
    fn backspacing_an_empty_prompt_leaves_command_line_mode() {
        let mut km = VimKeymap::default();
        run(&mut km, &["/", "a", "Backspace"]);
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn ex_substitute_and_nohl_reach_the_core() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(
                &mut km,
                &[":", "%", "s", "/", "a", "/", "b", "/", "g", "Enter"]
            ),
            vec![EditCommand::Substitute {
                range: crate::edit::ex::ExRange::WholeFile,
                pattern: "a".into(),
                replacement: "b".into(),
                all: true,
            }]
        );
        assert_eq!(
            run(&mut km, &[":", "n", "o", "h", "Enter"]),
            vec![EditCommand::ClearSearchHighlight]
        );
    }

    #[test]
    fn a_bare_line_number_jumps() {
        let mut km = VimKeymap::default();
        assert_eq!(
            run(&mut km, &[":", "1", "2", "Enter"]),
            vec![EditCommand::Move(Motion::GotoLine(11))]
        );
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
