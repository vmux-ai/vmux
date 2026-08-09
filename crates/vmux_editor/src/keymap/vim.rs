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
    block_insert: Option<bool>,
    block_text: String,
    mappings: crate::keymap::mapping::Mappings,
    map_pending: Vec<KeyInput>,
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
    /// Build a keymap with the user's configured key mappings applied.
    pub fn with_mappings(specs: &[vmux_core::editor::KeyMapping], leader: &str) -> Self {
        Self {
            mappings: crate::keymap::mapping::Mappings::new(specs, leader),
            ..Self::default()
        }
    }

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
        // An abandoned prompt would otherwise keep eating every key with no way back.
        if self.ex.take().is_some() {
            self.mode = self.mode_before_ex;
        }
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

    /// Feed a key through the user's mappings.
    ///
    /// Returns `None` when the key should be handled normally, `Some` when the mapping layer
    /// consumed it — either holding it as part of a longer sequence or expanding a match.
    fn route_through_mappings(&mut self, k: &KeyInput) -> Option<Vec<EditCommand>> {
        use crate::keymap::mapping::MatchResult;
        let mut pending = std::mem::take(&mut self.map_pending);
        pending.push(k.clone());
        match self.mappings.match_keys(self.mode, &pending) {
            MatchResult::Pending => {
                self.map_pending = pending;
                Some(vec![])
            }
            MatchResult::Expand(rhs) => {
                let mut out = Vec::new();
                for key in rhs {
                    out.extend(self.handle_unmapped(&key));
                }
                Some(out)
            }
            MatchResult::Miss => {
                if pending.len() == 1 {
                    return None;
                }
                // The buffered prefix turned out not to be a mapping; replay it as typed.
                let mut out = Vec::new();
                for key in pending {
                    out.extend(self.handle_unmapped(&key));
                }
                Some(out)
            }
        }
    }

    /// Run one key through recording and dispatch, skipping the mapping layer.
    fn handle_unmapped(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        let was_replaying = self.replaying;
        self.replaying = true;
        let cmds = self.record_and_dispatch(k);
        self.replaying = was_replaying;
        cmds
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
                "-" => vec![UndoTime {
                    forward: false,
                    count: self.take_count(),
                }],
                "+" => vec![UndoTime {
                    forward: true,
                    count: self.take_count(),
                }],
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
            "R" => {
                self.mode = EditMode::Replace;
                vec![SetMode(EditMode::Replace)]
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

        // Before `motion_for`, which maps bare `e`, `E` and `_` and would otherwise shadow the
        // `g`-prefixed arms below and leave `g_pending` set. `normal` orders it the same way.
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

        if let Some(m) = motion_for(key) {
            let n = self.take_count();
            return std::iter::repeat_n(Select(m), n).collect();
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
            "I" | "A" if self.mode == EditMode::VisualBlock => {
                self.block_insert = Some(key == "A");
                self.mode = EditMode::Insert;
                vec![BeginBlockInsert { after: key == "A" }]
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
                let mut cmds = Vec::new();
                if self.block_insert.take().is_some() {
                    cmds.push(FinishBlockInsert {
                        text: std::mem::take(&mut self.block_text),
                    });
                }
                cmds.push(Move(Motion::LeftBounded));
                cmds.push(SetMode(EditMode::Normal));
                cmds
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
            // Closing is a pane operation, and `EditCommand` has no way to ask for one — the
            // editor cannot close the surface hosting it. `:wq` therefore saves without closing
            // and `:q` does nothing. Wiring these needs a close command routed to the stack.
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
        if self.block_insert.is_some() {
            self.block_text.push_str(text);
        }
        self.pending_change.push(Recorded::Text(text.to_string()));
        self.in_change = true;
    }
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if self.replaying {
            return self.dispatch(k);
        }
        if !self.mappings.is_empty()
            && let Some(cmds) = self.route_through_mappings(k)
        {
            return cmds;
        }
        self.record_and_dispatch(k)
    }
    fn pointer_selection_mode(&mut self, extend: bool) -> Option<EditCommand> {
        match (extend, self.mode) {
            (true, EditMode::Normal) => {
                self.mode = EditMode::Visual;
                Some(EditCommand::SetMode(EditMode::Visual))
            }
            (false, mode) if mode.is_visual() => {
                self.mode = EditMode::Normal;
                Some(EditCommand::SetMode(EditMode::Normal))
            }
            _ => None,
        }
    }
}

impl VimKeymap {
    /// Dispatch one key while maintaining the dot-repeat and macro records.
    fn record_and_dispatch(&mut self, k: &KeyInput) -> Vec<EditCommand> {
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
            if k.key == "a" || k.key == "x" {
                let count = self.take_count() as i64;
                self.reset();
                let delta = if k.key == "a" { count } else { -count };
                return vec![EditCommand::Increment(delta)];
            }
            if k.key.eq_ignore_ascii_case("v") {
                self.reset();
                self.mode = if self.mode == EditMode::VisualBlock {
                    EditMode::Normal
                } else {
                    EditMode::VisualBlock
                };
                return vec![EditCommand::SetMode(self.mode)];
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
                EditMode::Insert | EditMode::Replace => {
                    self.mode = EditMode::Normal;
                    vec![
                        EditCommand::Move(Motion::LeftBounded),
                        EditCommand::SetMode(EditMode::Normal),
                    ]
                }
                EditMode::Visual | EditMode::VisualLine | EditMode::VisualBlock => {
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
            EditMode::Insert | EditMode::Replace => self.insert(k),
            EditMode::Visual | EditMode::VisualLine | EditMode::VisualBlock => self.visual(k),
            EditMode::Normal | EditMode::CommandLine => self.normal(k),
        }
    }
}

#[cfg(test)]
#[path = "vim.test.rs"]
mod tests;
