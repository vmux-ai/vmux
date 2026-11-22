use crate::edit::command::{EditCommand, EditMode, Motion, Operator, ScrollPlacement, Target};
use crate::edit::text_object::{TextObject, TextObjectKind};
use crate::keymap::{KeyInput, Keymap};

use crate::keymap::mapping::MatchResult;
use EditCommand::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObjectScope {
    Inner,
    Around,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingMark {
    Set,
    GotoExact,
    GotoLine,
}

#[derive(Clone)]
enum Recorded {
    Key(KeyInput),
    Text(String),
}

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

#[derive(bevy::prelude::Component, Default, Clone)]
pub struct VimKeymap {
    pub(crate) applied_config: Option<crate::keymap::KeymapConfig>,
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
    pending_change: Vec<Recorded>,
    last_change: Vec<Recorded>,
    in_change: bool,
    replaying: bool,
    macro_pending: Option<bool>,
    macro_record: Option<(char, Vec<Recorded>)>,
    macros: std::collections::HashMap<char, Vec<Recorded>>,
    last_macro: Option<char>,
    insert_after_next: bool,
    mark_pending: Option<PendingMark>,
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

fn find_prefix(key: &str) -> Option<(bool, bool)> {
    Some(match key {
        "f" => (true, false),
        "F" => (false, false),
        "t" => (true, true),
        "T" => (false, true),
        _ => return None,
    })
}

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

pub(super) struct VimInput<'a> {
    parser: &'a mut VimKeymap,
    mode: EditMode,
}

impl VimKeymap {
    pub fn with_mappings(specs: &[vmux_api::editor::KeyMapping], leader: &str) -> Self {
        Self {
            mappings: crate::keymap::mapping::Mappings::new(specs, leader),
            ..Self::default()
        }
    }

    pub(crate) fn decode(
        &mut self,
        mode: EditMode,
        input: &KeyInput,
    ) -> (Vec<EditCommand>, EditMode) {
        let mut decoder = VimInput { parser: self, mode };
        let commands = decoder.handle(input);
        (commands, decoder.mode)
    }

    pub(crate) fn reject(&mut self, mode: EditMode) {
        VimInput { parser: self, mode }.reset();
        self.pending_change.clear();
        self.in_change = false;
    }

    pub(crate) fn record(&mut self, mode: EditMode, text: &str) {
        VimInput { parser: self, mode }.record_text(text);
    }

    pub(crate) fn pointer(&mut self, mode: EditMode, extend: bool) -> Option<EditCommand> {
        VimInput { parser: self, mode }.pointer_selection_mode(extend)
    }

    pub(crate) fn label(&self, mode: EditMode) -> String {
        mode.label().to_string()
    }
}

impl VimInput<'_> {
    fn take_count(&mut self) -> usize {
        self.parser.count.take().unwrap_or(1)
    }

    fn take_operator_count(&mut self) -> usize {
        let outer = self.parser.op_count.take().unwrap_or(1);
        let inner = self.parser.count.take().unwrap_or(1);
        outer.saturating_mul(inner).max(1)
    }

    fn take_register(&mut self) -> Option<char> {
        self.parser.register.take()
    }

    fn reset(&mut self) {
        self.parser.count = None;
        self.parser.op_count = None;
        self.parser.pending_op = None;
        self.parser.pending_op_key = None;
        self.parser.pending_object = None;
        self.parser.pending_find = None;
        self.parser.register = None;
        self.parser.register_pending = false;
        self.parser.replace_pending = false;
        self.parser.g_pending = false;
        self.parser.z_pending = false;
    }

    fn is_command_start(&self) -> bool {
        self.parser.pending_op.is_none()
            && self.parser.pending_object.is_none()
            && self.parser.pending_find.is_none()
            && self.parser.macro_pending.is_none()
            && self.parser.mark_pending.is_none()
            && !self.parser.register_pending
            && !self.parser.replace_pending
            && !self.parser.g_pending
            && !self.parser.z_pending
    }

    fn is_idle(&self) -> bool {
        self.is_command_start() && self.parser.count.is_none() && self.parser.op_count.is_none()
    }

    fn awaits_literal_key(&self) -> bool {
        self.parser.pending_object.is_some()
            || self.parser.macro_pending.is_some()
            || self.parser.mark_pending.is_some()
            || self.parser.register_pending
            || self.parser.replace_pending
            || self.parser.g_pending
            || self.parser.z_pending
    }

    fn route_through_mappings(&mut self, k: &KeyInput) -> Option<Vec<EditCommand>> {
        let mut pending = std::mem::take(&mut self.parser.map_pending);
        pending.push(k.clone());
        match self.parser.mappings.match_keys(self.mode, &pending) {
            MatchResult::Pending => {
                self.parser.map_pending = pending;
                Some(vec![])
            }
            MatchResult::Expand(rhs) => Some(self.run_mapping(&rhs)),
            MatchResult::Miss => {
                if pending.len() == 1 {
                    return None;
                }
                let mut out = Vec::new();
                for key in pending {
                    out.extend(self.handle_unmapped(&key));
                }
                Some(out)
            }
        }
    }

    fn run_mapping(&mut self, rhs: &[KeyInput]) -> Vec<EditCommand> {
        let mut out = Vec::new();
        let mut ex: Option<String> = None;
        for key in rhs {
            let Some(body) = ex.as_mut() else {
                match key.key.as_str() {
                    ":" => ex = Some(String::new()),
                    _ => out.extend(self.handle_unmapped(key)),
                }
                continue;
            };
            match key.key.as_str() {
                "Enter" => {
                    let line = std::mem::take(body);
                    ex = None;
                    out.extend(crate::edit::ex::ExLine::edits(&line));
                }
                "Escape" => ex = None,
                typed => {
                    if let Some(c) = single_char(typed) {
                        body.push(c);
                    }
                }
            }
        }
        out
    }

    fn handle_unmapped(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        let was_replaying = self.parser.replaying;
        self.parser.replaying = true;
        let cmds = self.record_and_dispatch(k);
        self.parser.replaying = was_replaying;
        cmds
    }

    fn replay(&mut self, inputs: &[Recorded], times: usize) -> Vec<EditCommand> {
        if self.parser.replaying {
            return vec![];
        }
        self.parser.replaying = true;
        let mut out = Vec::new();
        for _ in 0..times.max(1) {
            for input in inputs {
                match input {
                    Recorded::Key(k) => out.extend(self.dispatch(k)),
                    Recorded::Text(t) => out.push(EditCommand::InsertText(t.clone())),
                }
            }
        }
        self.parser.replaying = false;
        out
    }

    fn start_operator(&mut self, operator: Operator, key: char) {
        self.parser.pending_op = Some(operator);
        self.parser.pending_op_key = Some(key);
        self.parser.op_count = self.parser.count.take();
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

    fn motion_command(&mut self, m: Motion) -> Vec<EditCommand> {
        if let Some(operator) = self.parser.pending_op.take() {
            let count = self.take_operator_count();
            self.parser.pending_op_key = None;
            self.parser.pending_object = None;
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

    fn resolve_find(&mut self, forward: bool, till: bool, key: &str) -> Vec<EditCommand> {
        let Some(ch) = single_char(key) else {
            self.reset();
            return vec![];
        };
        self.parser.last_find = Some((ch, forward, till));
        self.motion_command(Motion::FindChar { ch, forward, till })
    }

    fn repeat_find(&mut self, reverse: bool) -> Vec<EditCommand> {
        let Some((ch, forward, till)) = self.parser.last_find else {
            return vec![];
        };
        let forward = if reverse { !forward } else { forward };
        self.motion_command(Motion::FindChar { ch, forward, till })
    }

    fn operator_pending(&mut self, operator: Operator, key: &str) -> Vec<EditCommand> {
        if let Some(scope) = self.parser.pending_object.take() {
            let count = self.take_operator_count();
            self.parser.pending_op_key = None;
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
            self.parser.pending_object = Some(if key == "i" {
                ObjectScope::Inner
            } else {
                ObjectScope::Around
            });
            self.parser.pending_op = Some(operator);
            return vec![];
        }

        let doubled = single_char(key)
            .zip(self.parser.pending_op_key)
            .is_some_and(|(a, b)| a == b);
        let count = self.take_operator_count();
        self.parser.pending_op_key = None;

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
        let key = k.key.as_str();
        if let Some(commands) = self.normal_literal(key) {
            return commands;
        }
        if let Some(commands) = self.normal_prefix(key) {
            return commands;
        }
        if self.parser.g_pending {
            self.parser.g_pending = false;
            return self.normal_g(key);
        }

        if self.parser.z_pending {
            self.parser.z_pending = false;
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

        self.normal_key(key)
    }

    fn normal_literal(&mut self, key: &str) -> Option<Vec<EditCommand>> {
        if let Some((forward, till)) = self.parser.pending_find.take() {
            return Some(self.resolve_find(forward, till, key));
        }

        if let Some(pending) = self.parser.mark_pending.take() {
            let Some(name) = single_char(key) else {
                return Some(vec![]);
            };
            return Some(match pending {
                PendingMark::Set => vec![SetMark(name)],
                PendingMark::GotoExact => vec![GotoMark {
                    name,
                    linewise: false,
                }],
                PendingMark::GotoLine => vec![GotoMark {
                    name,
                    linewise: true,
                }],
            });
        }

        if let Some(record) = self.parser.macro_pending.take() {
            let Some(reg) = single_char(key) else {
                return Some(vec![]);
            };
            if record {
                self.parser.macro_record = Some((reg, Vec::new()));
                return Some(vec![]);
            }
            let reg = if reg == '@' {
                match self.parser.last_macro {
                    Some(r) => r,
                    None => return Some(vec![]),
                }
            } else {
                reg
            };
            self.parser.last_macro = Some(reg);
            let Some(body) = self.parser.macros.get(&reg).cloned() else {
                return Some(vec![]);
            };
            let times = self.take_count();
            return Some(self.replay(&body, times));
        }

        None
    }

    fn normal_prefix(&mut self, key: &str) -> Option<Vec<EditCommand>> {
        if self.parser.register_pending {
            self.parser.register_pending = false;
            if let Some(c) = single_char(key) {
                self.parser.register = Some(c);
            }
            return Some(vec![]);
        }

        if self.parser.replace_pending {
            self.parser.replace_pending = false;
            let count = self.take_count();
            if let Some(ch) = single_char(key) {
                return Some(vec![ReplaceChar { ch, count }]);
            }
            return Some(vec![]);
        }

        if let Some(c) = single_char(key)
            && c.is_ascii_digit()
            && !(c == '0' && self.parser.count.is_none())
        {
            let d = c as usize - '0' as usize;
            self.parser.count = Some(self.parser.count.unwrap_or(0) * 10 + d);
            return Some(vec![]);
        }

        if !self.awaits_literal_key() {
            if let Some((forward, till)) = find_prefix(key) {
                self.parser.pending_find = Some((forward, till));
                return Some(vec![]);
            }
            if key == ";" || key == "," {
                return Some(self.repeat_find(key == ","));
            }
        }

        if let Some(operator) = self.parser.pending_op.take() {
            return Some(self.operator_pending(operator, key));
        }

        None
    }

    fn normal_g(&mut self, key: &str) -> Vec<EditCommand> {
        if let Some((operator, op_key)) = g_operator(key) {
            self.start_operator(operator, op_key);
            return vec![];
        }
        match key {
            "g" => {
                let line = self.parser.count.take();
                vec![match line {
                    Some(n) => Move(Motion::GotoLine(n.saturating_sub(1) as u32)),
                    None => Move(Motion::DocStart),
                }]
            }
            "d" => vec![GotoDefinition],
            "r" => vec![FindReferences],
            "R" => vec![BeginRename],
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
        }
    }

    fn normal_key(&mut self, key: &str) -> Vec<EditCommand> {
        if let Some(commands) = self.normal_edit_key(key) {
            return commands;
        }
        match key {
            "g" => {
                self.parser.g_pending = true;
                vec![]
            }
            "z" => {
                self.parser.z_pending = true;
                vec![]
            }
            "\"" => {
                self.parser.register_pending = true;
                vec![]
            }
            "r" => {
                self.parser.replace_pending = true;
                vec![]
            }
            "G" => {
                let cmd = match self.parser.count.take() {
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
                let body = std::mem::take(&mut self.parser.last_change);
                let out = self.replay(&body, times);
                self.parser.last_change = body;
                out
            }
            "R" => {
                self.mode = EditMode::Replace;
                vec![SetMode(EditMode::Replace)]
            }
            "*" => vec![SearchWord { forward: true }],
            "#" => vec![SearchWord { forward: false }],
            "m" => {
                self.parser.mark_pending = Some(PendingMark::Set);
                vec![]
            }
            "`" => {
                self.parser.mark_pending = Some(PendingMark::GotoExact);
                vec![]
            }
            "'" => {
                self.parser.mark_pending = Some(PendingMark::GotoLine);
                vec![]
            }
            "q" => {
                if let Some((reg, body)) = self.parser.macro_record.take() {
                    self.parser.macros.insert(reg, body);
                    self.parser.last_macro = Some(reg);
                } else {
                    self.parser.macro_pending = Some(true);
                }
                vec![]
            }
            "@" => {
                self.parser.macro_pending = Some(false);
                vec![]
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
            ":" => vec![EditCommand::OpenCommandLine],
            "/" | "?" => {
                vec![EditCommand::OpenFind {
                    forward: key == "/",
                }]
            }
            "Escape" => {
                self.reset();
                vec![]
            }
            _ => vec![],
        }
    }

    fn normal_edit_key(&mut self, key: &str) -> Option<Vec<EditCommand>> {
        match key {
            "i" => Some({
                self.enter_insert();
                vec![SetMode(EditMode::Insert)]
            }),
            "a" => Some({
                self.enter_insert();
                vec![SetMode(EditMode::Insert), Move(Motion::Right)]
            }),
            "I" => Some({
                self.enter_insert();
                vec![SetMode(EditMode::Insert), Move(Motion::FirstNonBlank)]
            }),
            "A" => Some({
                self.enter_insert();
                vec![SetMode(EditMode::Insert), Move(Motion::LineEnd)]
            }),
            "o" => Some({
                self.enter_insert();
                vec![OpenLine { above: false }]
            }),
            "O" => Some({
                self.enter_insert();
                vec![OpenLine { above: true }]
            }),
            "x" => Some({
                let count = self.take_count();
                vec![self.op(Operator::Delete, Target::Motion(Motion::Right, count))]
            }),
            "X" => Some({
                let count = self.take_count();
                vec![self.op(Operator::Delete, Target::Motion(Motion::Left, count))]
            }),
            "D" => Some({
                let _ = self.take_count();
                vec![self.op(Operator::Delete, Target::Motion(Motion::LineEnd, 1))]
            }),
            "C" => Some({
                let _ = self.take_count();
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Motion(Motion::LineEnd, 1))]
            }),
            "s" => Some({
                let count = self.take_count();
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Motion(Motion::Right, count))]
            }),
            "S" => Some({
                let count = self.take_count();
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Line(count))]
            }),
            "Y" => Some({
                let count = self.take_count();
                vec![self.op(Operator::Yank, Target::Line(count))]
            }),
            "J" => Some(vec![JoinLines {
                count: self.take_count(),
                spaces: true,
            }]),
            "~" => Some({
                let count = self.take_count();
                let mut cmds =
                    vec![self.op(Operator::ToggleCase, Target::Motion(Motion::Right, count))];
                cmds.extend(std::iter::repeat_n(Move(Motion::RightBounded), count));
                cmds
            }),
            "p" | "P" => Some({
                let count = self.take_count();
                vec![Put {
                    before: key == "P",
                    count,
                    register: self.take_register(),
                }]
            }),
            _ => None,
        }
    }

    fn visual(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if let Some(commands) = self.visual_prefix(k.key.as_str()) {
            return commands;
        }
        self.visual_key(k.key.as_str())
    }

    fn visual_prefix(&mut self, key: &str) -> Option<Vec<EditCommand>> {
        if let Some((forward, till)) = self.parser.pending_find.take() {
            return Some(self.resolve_find(forward, till, key));
        }

        if let Some(scope) = self.parser.pending_object.take() {
            let count = self.take_count();
            let Some(kind) = text_object_kind(key) else {
                return Some(vec![]);
            };
            return Some(vec![SelectTextObject(TextObject {
                kind,
                around: scope == ObjectScope::Around,
                count,
            })]);
        }

        if self.parser.register_pending {
            self.parser.register_pending = false;
            if let Some(c) = single_char(key) {
                self.parser.register = Some(c);
            }
            return Some(vec![]);
        }

        if self.parser.replace_pending {
            self.parser.replace_pending = false;
            if let Some(ch) = single_char(key) {
                self.mode = EditMode::Normal;
                return Some(vec![
                    Op {
                        operator: Operator::Delete,
                        target: Target::Selection,
                        register: Some(crate::edit::register::BLACKHOLE),
                    },
                    InsertText(ch.to_string()),
                    SetMode(EditMode::Normal),
                ]);
            }
            return Some(vec![]);
        }

        if let Some(c) = single_char(key)
            && c.is_ascii_digit()
            && !(c == '0' && self.parser.count.is_none())
        {
            let d = c as usize - '0' as usize;
            self.parser.count = Some(self.parser.count.unwrap_or(0) * 10 + d);
            return Some(vec![]);
        }

        if let Some((forward, till)) = find_prefix(key) {
            self.parser.pending_find = Some((forward, till));
            return Some(vec![]);
        }

        if key == ";" || key == "," {
            return Some(self.repeat_find(key == ","));
        }

        if self.parser.g_pending {
            self.parser.g_pending = false;
            return Some(match key {
                "u" => self.visual_op(Operator::Lower),
                "U" => self.visual_op(Operator::Upper),
                "~" => self.visual_op(Operator::ToggleCase),
                "g" => vec![Select(Motion::DocStart)],
                "e" => self.motion_command(Motion::WordEndPrev),
                "E" => self.motion_command(Motion::BigWordEndPrev),
                "_" => self.motion_command(Motion::LastNonBlank),
                _ => vec![],
            });
        }

        if let Some(m) = motion_for(key) {
            let n = self.take_count();
            return Some(std::iter::repeat_n(Select(m), n).collect());
        }

        None
    }

    fn visual_key(&mut self, key: &str) -> Vec<EditCommand> {
        if let Some(commands) = self.visual_operator(key) {
            return commands;
        }
        match key {
            "g" => {
                self.parser.g_pending = true;
                vec![]
            }
            "|" => {
                let col = self.take_count();
                vec![Select(Motion::Column(col))]
            }
            "\"" => {
                self.parser.register_pending = true;
                vec![]
            }
            "r" => {
                self.parser.replace_pending = true;
                vec![]
            }
            "i" | "a" => {
                self.parser.pending_object = Some(if key == "i" {
                    ObjectScope::Inner
                } else {
                    ObjectScope::Around
                });
                vec![]
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

    fn visual_operator(&mut self, key: &str) -> Option<Vec<EditCommand>> {
        match key {
            "d" | "x" => Some(self.visual_op(Operator::Delete)),
            "y" => Some(self.visual_op(Operator::Yank)),
            "u" => Some(self.visual_op(Operator::Lower)),
            "U" => Some(self.visual_op(Operator::Upper)),
            "~" => Some(self.visual_op(Operator::ToggleCase)),
            ">" => Some(self.visual_op(Operator::Indent)),
            "<" => Some(self.visual_op(Operator::Outdent)),
            "c" | "s" => Some({
                self.enter_insert();
                vec![self.op(Operator::Change, Target::Selection)]
            }),
            "D" | "X" => Some({
                self.mode = EditMode::VisualLine;
                let cmd = self.op(Operator::Delete, Target::Selection);
                self.mode = EditMode::Normal;
                vec![
                    SetMode(EditMode::VisualLine),
                    cmd,
                    SetMode(EditMode::Normal),
                ]
            }),
            "C" | "S" | "R" => Some({
                self.enter_insert();
                let cmd = self.op(Operator::Change, Target::Selection);
                vec![SetMode(EditMode::VisualLine), cmd]
            }),
            "Y" => Some({
                self.mode = EditMode::Normal;
                let cmd = self.op(Operator::Yank, Target::Selection);
                vec![
                    SetMode(EditMode::VisualLine),
                    cmd,
                    SetMode(EditMode::Normal),
                ]
            }),
            "J" => Some({
                self.mode = EditMode::Normal;
                vec![
                    JoinLines {
                        count: 2,
                        spaces: true,
                    },
                    SetMode(EditMode::Normal),
                ]
            }),
            "p" | "P" => Some({
                self.mode = EditMode::Normal;
                vec![Put {
                    before: key == "P",
                    count: self.take_count(),
                    register: self.take_register(),
                }]
            }),
            "I" | "A" if self.mode == EditMode::VisualBlock => Some({
                self.parser.block_insert = Some(key == "A");
                self.mode = EditMode::Insert;
                vec![BeginBlockInsert { after: key == "A" }]
            }),
            _ => None,
        }
    }

    fn visual_op(&mut self, operator: Operator) -> Vec<EditCommand> {
        self.mode = EditMode::Normal;
        let cmd = self.op(operator, Target::Selection);
        vec![cmd, EditCommand::SetMode(EditMode::Normal)]
    }

    fn insert(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if self.parser.register_pending {
            self.parser.register_pending = false;
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
                    self.parser.register_pending = true;
                    vec![]
                }
                "o" => {
                    self.mode = EditMode::Normal;
                    self.parser.insert_after_next = true;
                    vec![SetMode(EditMode::Normal)]
                }
                _ => vec![],
            };
        }

        match k.key.as_str() {
            "Escape" => {
                self.mode = EditMode::Normal;
                let mut cmds = Vec::new();
                if self.parser.block_insert.take().is_some() {
                    cmds.push(FinishBlockInsert {
                        text: std::mem::take(&mut self.parser.block_text),
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
}

impl Keymap for VimInput<'_> {
    fn mode(&self) -> EditMode {
        self.mode
    }

    fn record_text(&mut self, text: &str) {
        if self.parser.replaying {
            return;
        }
        if let Some((_, body)) = self.parser.macro_record.as_mut() {
            body.push(Recorded::Text(text.to_string()));
        }
        if self.parser.block_insert.is_some() {
            self.parser.block_text.push_str(text);
        }
        self.parser
            .pending_change
            .push(Recorded::Text(text.to_string()));
        self.parser.in_change = true;
    }

    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if self.parser.replaying {
            return self.dispatch(k);
        }
        if !self.parser.mappings.is_empty()
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

impl VimInput<'_> {
    fn record_and_dispatch(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        let stops_recording =
            self.parser.macro_record.is_some() && k.key == "q" && self.mode == EditMode::Normal;
        if let Some((_, body)) = self.parser.macro_record.as_mut()
            && !stops_recording
        {
            body.push(Recorded::Key(k.clone()));
        }

        let repeating = k.key == "." && self.mode == EditMode::Normal && self.is_command_start();
        if !repeating {
            self.parser.pending_change.push(Recorded::Key(k.clone()));
        }

        let cmds = self.dispatch(k);

        if repeating {
            self.parser.pending_change.clear();
            return cmds;
        }
        if cmds.iter().any(is_change) {
            self.parser.in_change = true;
        }
        if self.parser.in_change {
            if self.mode == EditMode::Normal {
                self.parser.last_change = std::mem::take(&mut self.parser.pending_change);
                self.parser.in_change = false;
            }
        } else if self.mode == EditMode::Normal && self.is_idle() {
            self.parser.pending_change.clear();
        }
        cmds
    }

    fn dispatch(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        let armed = self.parser.insert_after_next;
        let mut cmds = self.dispatch_inner(k);
        if armed && self.mode == EditMode::Normal && self.is_idle() && !cmds.is_empty() {
            self.parser.insert_after_next = false;
            let opens_find = cmds
                .iter()
                .any(|cmd| matches!(cmd, EditCommand::OpenFind { .. }));
            if !opens_find {
                self.mode = EditMode::Insert;
                cmds.push(EditCommand::SetMode(EditMode::Insert));
            }
        }
        cmds
    }

    #[cfg(target_os = "macos")]
    fn desktop_shortcut(&mut self, k: &KeyInput) -> Option<Vec<EditCommand>> {
        if !k.mods.meta || k.mods.ctrl || k.mods.alt {
            return None;
        }

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
            "v" => Some(EditCommand::Paste),
            "a" => Some(EditCommand::Move(Motion::DocStart)),
            "s" => Some(EditCommand::Save),
            "z" if k.mods.shift => Some(EditCommand::Redo),
            "z" => Some(EditCommand::Undo),
            "y" => Some(EditCommand::Redo),
            _ => None,
        };
        if let Some(command) = command {
            self.reset();
            return Some(if k.key.eq_ignore_ascii_case("a") {
                vec![command, EditCommand::Select(Motion::DocEnd)]
            } else {
                vec![command]
            });
        }
        None
    }

    fn control_shortcut(&mut self, k: &KeyInput) -> Option<Vec<EditCommand>> {
        if !k.mods.ctrl || k.mods.meta || k.mods.alt || self.mode.accepts_text() {
            return None;
        }

        let direction = match k.key.to_ascii_lowercase().as_str() {
            "e" => Some(1),
            "y" => Some(-1),
            _ => None,
        };
        if let Some(direction) = direction {
            let count = self.take_count().min(i32::MAX as usize) as i32;
            self.reset();
            return Some(vec![EditCommand::ScrollViewport(direction * count)]);
        }
        if k.key == "a" || k.key == "x" {
            let count = self.take_count() as i64;
            self.reset();
            let delta = if k.key == "a" { count } else { -count };
            return Some(vec![EditCommand::Increment(delta)]);
        }
        if k.key.eq_ignore_ascii_case("v") {
            self.reset();
            self.mode = if self.mode == EditMode::VisualBlock {
                EditMode::Normal
            } else {
                EditMode::VisualBlock
            };
            return Some(vec![EditCommand::SetMode(self.mode)]);
        }
        if k.key.eq_ignore_ascii_case("o") || k.key == "i" || k.key == "Tab" {
            let count = self.take_count();
            self.reset();
            return Some(vec![EditCommand::JumpList {
                back: k.key.eq_ignore_ascii_case("o"),
                count,
            }]);
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
            return Some(if self.mode.is_visual() {
                vec![EditCommand::Select(motion)]
            } else {
                vec![EditCommand::Move(motion)]
            });
        }
        None
    }

    fn dispatch_inner(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if let Some(command) = k.lsp_action() {
            self.reset();
            return vec![command];
        }
        #[cfg(target_os = "macos")]
        if let Some(commands) = self.desktop_shortcut(k) {
            return commands;
        }
        if let Some(commands) = self.control_shortcut(k) {
            return commands;
        }

        if k.mods.ctrl && k.key.eq_ignore_ascii_case("c") {
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
        match self.mode {
            EditMode::Insert | EditMode::Replace => self.insert(k),
            EditMode::Visual | EditMode::VisualLine | EditMode::VisualBlock => self.visual(k),
            EditMode::Normal | EditMode::CommandLine => self.normal(k),
        }
    }
}

#[cfg(test)]
pub(crate) struct Fixture {
    world: bevy::prelude::World,
    entity: bevy::prelude::Entity,
}

#[cfg(test)]
impl Fixture {
    pub(crate) fn with_mappings(specs: &[vmux_api::editor::KeyMapping], leader: &str) -> Self {
        let mut world = bevy::prelude::World::new();
        let entity = world
            .spawn((
                VimKeymap::with_mappings(specs, leader),
                crate::edit::document::Mode {
                    value: EditMode::Normal,
                },
            ))
            .id();
        Self { world, entity }
    }

    fn mapped(specs: &[(&str, &str, &str)]) -> Self {
        let mut mappings = Vec::with_capacity(specs.len());
        for (mode, lhs, rhs) in specs {
            mappings.push(vmux_api::editor::KeyMapping {
                mode: (*mode).into(),
                lhs: (*lhs).into(),
                rhs: (*rhs).into(),
            });
        }
        Self::with_mappings(&mappings, " ")
    }

    fn run(&mut self, sequence: &[&str]) -> Vec<EditCommand> {
        let mut output = Vec::new();
        for key in sequence {
            output.extend(self.handle(&KeyInput {
                key: (*key).into(),
                mods: crate::keymap::Mods::default(),
                repeat: false,
            }));
        }
        output
    }

    pub(crate) fn mode(&self) -> EditMode {
        self.world
            .get::<crate::edit::document::Mode>(self.entity)
            .unwrap()
            .value
    }

    pub(crate) fn handle(&mut self, key: &KeyInput) -> Vec<EditCommand> {
        let mode = self.mode();
        let (commands, next) = self
            .world
            .get_mut::<VimKeymap>(self.entity)
            .unwrap()
            .decode(mode, key);
        self.world
            .get_mut::<crate::edit::document::Mode>(self.entity)
            .unwrap()
            .value = next;
        commands
    }

    pub(crate) fn record_text(&mut self, text: &str) {
        let mode = self.mode();
        self.world
            .get_mut::<VimKeymap>(self.entity)
            .unwrap()
            .record(mode, text);
    }

    pub(crate) fn pointer_selection_mode(&mut self, extend: bool) -> Option<EditCommand> {
        let mode = self.mode();
        let command = self
            .world
            .get_mut::<VimKeymap>(self.entity)
            .unwrap()
            .pointer(mode, extend);
        if let Some(EditCommand::SetMode(next)) = command.as_ref() {
            self.world
                .get_mut::<crate::edit::document::Mode>(self.entity)
                .unwrap()
                .value = *next;
        }
        command
    }
}

#[cfg(test)]
impl Default for Fixture {
    fn default() -> Self {
        Self::with_mappings(&[], " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

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

    fn op(operator: Operator, target: Target) -> EditCommand {
        EditCommand::Op {
            operator,
            target,
            register: None,
        }
    }

    #[test]
    fn operators_pair_with_motions_and_doubled_keys() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
        );
        assert_eq!(
            km.run(&["d", "d"]),
            vec![op(Operator::Delete, Target::Line(1))]
        );
        assert_eq!(
            km.run(&["y", "y"]),
            vec![op(Operator::Yank, Target::Line(1))]
        );
        assert_eq!(
            km.run(&["c", "c"]),
            vec![op(Operator::Change, Target::Line(1))]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn counts_multiply_across_operator_and_motion() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "3", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 3))]
        );
        assert_eq!(
            km.run(&["2", "d", "3", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 6))]
        );
        assert_eq!(
            km.run(&["2", "d", "d"]),
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
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "i", "w"]),
            vec![op(Operator::Delete, object(TextObjectKind::Word, false, 1))]
        );
        assert_eq!(
            km.run(&["c", "a", "\""]),
            vec![op(
                Operator::Change,
                object(TextObjectKind::DoubleQuote, true, 1)
            )]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn text_object_counts_come_from_the_operator() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["2", "d", "i", "("]),
            vec![op(
                Operator::Delete,
                object(TextObjectKind::Paren, false, 2)
            )]
        );
    }

    #[test]
    fn visual_mode_selects_text_objects() {
        let mut km = Fixture::default();
        km.run(&["v"]);
        assert_eq!(
            km.run(&["i", "p"]),
            vec![EditCommand::SelectTextObject(TextObject {
                kind: TextObjectKind::Paragraph,
                around: false,
                count: 1,
            })]
        );
    }

    #[test]
    fn an_unknown_object_key_abandons_the_operator() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["d", "i", "z"]), vec![]);
        assert_eq!(
            km.run(&["d", "w"]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
        );
    }

    fn find(ch: char, forward: bool, till: bool) -> Motion {
        Motion::FindChar { ch, forward, till }
    }

    #[test]
    fn find_char_prefixes_consume_the_next_key() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["f", ","]),
            vec![EditCommand::Move(find(',', true, false))]
        );
        assert_eq!(
            km.run(&["T", "x"]),
            vec![EditCommand::Move(find('x', false, true))]
        );
    }

    #[test]
    fn semicolon_repeats_and_comma_reverses_the_last_find() {
        let mut km = Fixture::default();
        km.run(&["f", "x"]);
        assert_eq!(
            km.run(&[";"]),
            vec![EditCommand::Move(find('x', true, false))]
        );
        assert_eq!(
            km.run(&[","]),
            vec![EditCommand::Move(find('x', false, false))]
        );
    }

    #[test]
    fn repeat_find_without_a_previous_find_does_nothing() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&[";"]), vec![]);
    }

    #[test]
    fn operators_compose_with_find_char() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "t", ","]),
            vec![op(
                Operator::Delete,
                Target::Motion(find(',', true, true), 1)
            )]
        );
        assert_eq!(
            km.run(&["2", "d", "f", "x"]),
            vec![op(
                Operator::Delete,
                Target::Motion(find('x', true, false), 2)
            )]
        );
    }

    #[test]
    fn the_tag_object_is_not_shadowed_by_the_till_prefix() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "i", "t"]),
            vec![op(Operator::Delete, object(TextObjectKind::Tag, false, 1))]
        );
    }

    #[test]
    fn word_and_screen_motions_are_bound() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["W"]), vec![EditCommand::Move(Motion::BigWordNext)]);
        assert_eq!(
            km.run(&["g", "e"]),
            vec![EditCommand::Move(Motion::WordEndPrev)]
        );
        assert_eq!(km.run(&["%"]), vec![EditCommand::Move(Motion::MatchPair)]);
        assert_eq!(km.run(&["H"]), vec![EditCommand::Move(Motion::ScreenTop)]);
        assert_eq!(
            km.run(&["8", "|"]),
            vec![EditCommand::Move(Motion::Column(8))]
        );
    }

    #[test]
    fn visual_g_prefixed_motions_are_not_shadowed() {
        let mut km = Fixture::default();
        km.run(&["v"]);
        assert_eq!(km.mode(), EditMode::Visual);

        assert_eq!(
            km.run(&["g", "e"]),
            vec![EditCommand::Select(Motion::WordEndPrev)]
        );
        assert_eq!(
            km.run(&["g", "E"]),
            vec![EditCommand::Select(Motion::BigWordEndPrev)]
        );
        assert_eq!(
            km.run(&["g", "_"]),
            vec![EditCommand::Select(Motion::LastNonBlank)]
        );
        assert_eq!(km.run(&["e"]), vec![EditCommand::Select(Motion::WordEnd)]);
    }

    #[test]
    fn half_and_full_page_scroll_chords() {
        let mut km = Fixture::default();
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
        let mut km = Fixture::default();
        assert_eq!(km.run(&["z", "a"]), vec![EditCommand::FoldToggle]);
        assert_eq!(
            km.run(&["z", "z"]),
            vec![EditCommand::ScrollCursorTo(ScrollPlacement::Center)]
        );
        assert_eq!(
            km.run(&["z", "b"]),
            vec![EditCommand::ScrollCursorTo(ScrollPlacement::Bottom)]
        );
    }

    #[test]
    fn dot_repeats_the_last_change_not_the_last_motion() {
        let mut km = Fixture::default();
        km.run(&["d", "w"]);
        km.run(&["j", "l"]);
        assert_eq!(
            km.run(&["."]),
            vec![op(Operator::Delete, Target::Motion(Motion::WordNext, 1))]
        );
    }

    #[test]
    fn dot_takes_a_count_and_survives_reuse() {
        let mut km = Fixture::default();
        km.run(&["x"]);
        let expected = op(Operator::Delete, Target::Motion(Motion::Right, 1));
        assert_eq!(
            km.run(&["2", "."]),
            vec![expected.clone(), expected.clone()]
        );
        assert_eq!(km.run(&["."]), vec![expected]);
    }

    #[test]
    fn dot_repeats_an_insert_session_including_typed_text() {
        let mut km = Fixture::default();
        km.run(&["i"]);
        km.record_text("hi");
        km.run(&["Escape"]);
        assert_eq!(
            km.run(&["."]),
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
        let mut km = Fixture::default();
        km.run(&["x"]);
        km.run(&["y", "y"]);
        assert_eq!(
            km.run(&["."]),
            vec![op(Operator::Delete, Target::Motion(Motion::Right, 1))]
        );
    }

    #[test]
    fn macros_record_and_replay_through_a_register() {
        let mut km = Fixture::default();
        km.run(&["q", "a", "x", "j", "q"]);
        assert_eq!(
            km.run(&["@", "a"]),
            vec![
                op(Operator::Delete, Target::Motion(Motion::Right, 1)),
                EditCommand::Move(Motion::Down),
            ]
        );
        assert_eq!(
            km.run(&["@", "@"]),
            vec![
                op(Operator::Delete, Target::Motion(Motion::Right, 1)),
                EditCommand::Move(Motion::Down),
            ]
        );
    }

    #[test]
    fn a_counted_macro_replays_repeatedly() {
        let mut km = Fixture::default();
        km.run(&["q", "b", "x", "q"]);
        let once = op(Operator::Delete, Target::Motion(Motion::Right, 1));
        assert_eq!(
            km.run(&["3", "@", "b"]),
            vec![once.clone(), once.clone(), once]
        );
    }

    #[test]
    fn replaying_an_empty_register_is_inert() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["@", "z"]), vec![]);
    }

    #[test]
    fn insert_mode_editing_chords() {
        let mut km = Fixture::default();
        km.run(&["i"]);
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
        let mut km = Fixture::default();
        km.run(&["i"]);
        assert_eq!(km.handle(&chord("r", ctrl())), vec![]);
        assert_eq!(
            km.run(&["a"]),
            vec![EditCommand::Put {
                before: true,
                count: 1,
                register: Some('a'),
            }]
        );
    }

    #[test]
    fn jk_maps_to_escape_in_insert_mode() {
        let mut km = Fixture::mapped(&[("i", "jk", "<Esc>")]);
        km.run(&["i"]);
        assert_eq!(km.run(&["j"]), vec![]);
        assert_eq!(
            km.run(&["k"]),
            vec![
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal),
            ]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn an_abandoned_mapping_prefix_replays_as_typed() {
        let mut km = Fixture::mapped(&[("n", "gs", "0")]);
        assert_eq!(km.run(&["g"]), vec![]);
        assert_eq!(
            km.run(&["d"]),
            vec![EditCommand::GotoDefinition],
            "the buffered g and the new d still form gd"
        );
    }

    #[test]
    fn leader_mappings_expand_through_the_configured_leader() {
        let mut km = Fixture::mapped(&[("n", "<leader>s", ":w<CR>")]);
        assert_eq!(km.run(&[" "]), vec![]);
        assert_eq!(km.run(&["s"]), vec![EditCommand::Save]);
    }

    #[test]
    fn an_unmapped_key_is_untouched_by_the_mapping_layer() {
        let mut km = Fixture::mapped(&[("i", "jk", "<Esc>")]);
        assert_eq!(
            km.run(&["x"]),
            vec![op(Operator::Delete, Target::Motion(Motion::Right, 1))]
        );
    }

    #[test]
    fn ctrl_v_toggles_visual_block() {
        let mut km = Fixture::default();
        assert_eq!(
            km.handle(&chord("v", ctrl())),
            vec![EditCommand::SetMode(EditMode::VisualBlock)]
        );
        assert_eq!(km.mode(), EditMode::VisualBlock);
        assert_eq!(
            km.handle(&chord("v", ctrl())),
            vec![EditCommand::SetMode(EditMode::Normal)]
        );
    }

    #[test]
    fn block_insert_starts_then_replicates_on_escape() {
        let mut km = Fixture::default();
        km.handle(&chord("v", ctrl()));
        assert_eq!(
            km.run(&["I"]),
            vec![EditCommand::BeginBlockInsert { after: false }]
        );
        assert_eq!(km.mode(), EditMode::Insert);
        km.record_text("xy");
        assert_eq!(
            km.run(&["Escape"]),
            vec![
                EditCommand::FinishBlockInsert { text: "xy".into() },
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal),
            ]
        );
    }

    #[test]
    fn block_operators_route_through_the_selection() {
        let mut km = Fixture::default();
        km.handle(&chord("v", ctrl()));
        assert_eq!(
            km.run(&["d"]),
            vec![
                op(Operator::Delete, Target::Selection),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
    }

    #[test]
    fn ctrl_o_runs_one_normal_command_then_returns_to_insert() {
        let mut km = Fixture::default();
        km.run(&["i"]);
        assert_eq!(
            km.handle(&chord("o", ctrl())),
            vec![EditCommand::SetMode(EditMode::Normal)]
        );
        assert_eq!(km.mode(), EditMode::Normal);
        assert_eq!(
            km.run(&["$"]),
            vec![
                EditCommand::Move(Motion::LineEnd),
                EditCommand::SetMode(EditMode::Insert),
            ]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn ctrl_o_then_a_search_key_leaves_the_find_bar_owning_the_keys() {
        for (key, forward) in [("/", true), ("?", false)] {
            let mut km = Fixture::default();
            km.run(&["i"]);
            km.handle(&chord("o", ctrl()));
            assert_eq!(
                km.run(&[key]),
                vec![EditCommand::OpenFind { forward }],
                "{key} must not also hand the editor back to insert mode"
            );
            assert_eq!(km.mode(), EditMode::Normal);
        }
    }

    #[test]
    fn scroll_chords_do_not_fire_while_inserting() {
        let mut km = Fixture::default();
        km.run(&["i"]);
        assert_eq!(km.handle(&chord("e", ctrl())), vec![]);
    }

    #[test]
    fn search_repeat_and_word_search_bindings() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["n"]),
            vec![EditCommand::Move(Motion::SearchNext { reverse: false })]
        );
        assert_eq!(
            km.run(&["N"]),
            vec![EditCommand::Move(Motion::SearchNext { reverse: true })]
        );
        assert_eq!(
            km.run(&["*"]),
            vec![EditCommand::SearchWord { forward: true }]
        );
        assert_eq!(
            km.run(&["#"]),
            vec![EditCommand::SearchWord { forward: false }]
        );
    }

    #[test]
    fn an_operator_can_target_the_next_match() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "n"]),
            vec![op(
                Operator::Delete,
                Target::Motion(Motion::SearchNext { reverse: false }, 1)
            )]
        );
    }

    #[test]
    fn mark_bindings_set_and_recall() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["m", "a"]), vec![EditCommand::SetMark('a')]);
        assert_eq!(
            km.run(&["`", "a"]),
            vec![EditCommand::GotoMark {
                name: 'a',
                linewise: false
            }]
        );
        assert_eq!(
            km.run(&["'", "a"]),
            vec![EditCommand::GotoMark {
                name: 'a',
                linewise: true
            }]
        );
    }

    #[test]
    fn jump_and_change_list_bindings() {
        let mut km = Fixture::default();
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
            km.run(&["g", ";"]),
            vec![EditCommand::ChangeList {
                back: true,
                count: 1
            }]
        );
    }

    #[test]
    fn a_mark_key_is_not_read_as_a_quote_object() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["d", "i", "'"]),
            vec![op(
                Operator::Delete,
                object(TextObjectKind::SingleQuote, false, 1)
            )]
        );
    }

    #[test]
    fn count_repeats_motion() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["3", "j"]),
            vec![
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down)
            ]
        );
    }

    #[test]
    fn register_prefix_routes_yank_and_put() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["\"", "a", "y", "y"]),
            vec![EditCommand::Op {
                operator: Operator::Yank,
                target: Target::Line(1),
                register: Some('a'),
            }]
        );
        assert_eq!(
            km.run(&["\"", "a", "p"]),
            vec![EditCommand::Put {
                before: false,
                count: 1,
                register: Some('a'),
            }]
        );
    }

    #[test]
    fn indent_operators_use_doubled_keys() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&[">", ">"]),
            vec![op(Operator::Indent, Target::Line(1))]
        );
        assert_eq!(
            km.run(&["3", "<", "<"]),
            vec![op(Operator::Outdent, Target::Line(3))]
        );
    }

    #[test]
    fn g_prefixed_case_operators() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["g", "u", "w"]),
            vec![op(Operator::Lower, Target::Motion(Motion::WordNext, 1))]
        );
        assert_eq!(
            km.run(&["g", "U", "U"]),
            vec![op(Operator::Upper, Target::Line(1))]
        );
    }

    #[test]
    fn join_replace_and_toggle_case() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["3", "J"]),
            vec![EditCommand::JoinLines {
                count: 3,
                spaces: true
            }]
        );
        assert_eq!(
            km.run(&["g", "J"]),
            vec![EditCommand::JoinLines {
                count: 1,
                spaces: false
            }]
        );
        assert_eq!(
            km.run(&["2", "r", "x"]),
            vec![EditCommand::ReplaceChar { ch: 'x', count: 2 }]
        );
        assert_eq!(
            km.run(&["~"]),
            vec![
                op(Operator::ToggleCase, Target::Motion(Motion::Right, 1)),
                EditCommand::Move(Motion::RightBounded)
            ]
        );
    }

    #[test]
    fn shift_d_c_s_and_y_bindings() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["D"]),
            vec![op(Operator::Delete, Target::Motion(Motion::LineEnd, 1))]
        );
        assert_eq!(
            km.run(&["C"]),
            vec![op(Operator::Change, Target::Motion(Motion::LineEnd, 1))]
        );
        let mut km = Fixture::default();
        assert_eq!(km.run(&["S"]), vec![op(Operator::Change, Target::Line(1))]);
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["2", "Y"]),
            vec![op(Operator::Yank, Target::Line(2))]
        );
        assert_eq!(
            km.run(&["3", "x"]),
            vec![op(Operator::Delete, Target::Motion(Motion::Right, 3))]
        );
    }

    #[test]
    fn put_carries_a_count_and_direction() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["2", "p"]),
            vec![EditCommand::Put {
                before: false,
                count: 2,
                register: None
            }]
        );
        assert_eq!(
            km.run(&["P"]),
            vec![EditCommand::Put {
                before: true,
                count: 1,
                register: None
            }]
        );
    }

    #[test]
    fn open_line_replaces_the_newline_dance() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["o"]), vec![EditCommand::OpenLine { above: false }]);
        assert_eq!(km.mode(), EditMode::Insert);
        let mut km = Fixture::default();
        assert_eq!(km.run(&["O"]), vec![EditCommand::OpenLine { above: true }]);
    }

    #[test]
    fn visual_operators_act_on_the_selection() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["v"]), vec![EditCommand::SetMode(EditMode::Visual)]);
        assert_eq!(
            km.run(&["l"]),
            vec![EditCommand::Select(Motion::RightBounded)]
        );
        assert_eq!(
            km.run(&["y"]),
            vec![
                op(Operator::Yank, Target::Selection),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn visual_motions_take_counts() {
        let mut km = Fixture::default();
        km.run(&["v"]);
        assert_eq!(
            km.run(&["3", "j"]),
            vec![
                EditCommand::Select(Motion::Down),
                EditCommand::Select(Motion::Down),
                EditCommand::Select(Motion::Down)
            ]
        );
    }

    #[test]
    fn visual_indent_and_swap_ends() {
        let mut km = Fixture::default();
        km.run(&["v"]);
        assert_eq!(km.run(&["o"]), vec![EditCommand::SwapSelectionEnds]);
        assert_eq!(
            km.run(&[">"]),
            vec![
                op(Operator::Indent, Target::Selection),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
    }

    #[test]
    fn document_jump_bindings() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["g", "g"]),
            vec![EditCommand::Move(Motion::DocStart)]
        );
        assert_eq!(km.run(&["G"]), vec![EditCommand::Move(Motion::DocEnd)]);
        assert_eq!(
            km.run(&["5", "G"]),
            vec![EditCommand::Move(Motion::GotoLine(4))]
        );
        assert_eq!(
            km.run(&["5", "g", "g"]),
            vec![EditCommand::Move(Motion::GotoLine(4))]
        );
    }

    #[test]
    fn braces_move_by_paragraph() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["{"]),
            vec![EditCommand::Move(Motion::ParagraphPrev)]
        );
        assert_eq!(
            km.run(&["}"]),
            vec![EditCommand::Move(Motion::ParagraphNext)]
        );
    }

    #[test]
    fn fold_and_lsp_bindings() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["z", "a"]), vec![EditCommand::FoldToggle]);
        assert_eq!(km.run(&["z", "R"]), vec![EditCommand::UnfoldAll]);
        assert_eq!(km.run(&["g", "d"]), vec![EditCommand::GotoDefinition]);
        assert_eq!(km.run(&["g", "r"]), vec![EditCommand::FindReferences]);
        assert_eq!(km.run(&["K"]), vec![EditCommand::Hover]);
    }

    #[test]
    fn function_keys_reach_the_language_server_in_every_mode() {
        let mut km = Fixture::default();
        let f12 = KeyInput {
            key: "F12".into(),
            mods: Mods::default(),
            repeat: false,
        };
        assert_eq!(km.handle(&f12), vec![EditCommand::GotoDefinition]);
        km.handle(&KeyInput {
            key: "i".into(),
            mods: Mods::default(),
            repeat: false,
        });
        assert_eq!(km.handle(&f12), vec![EditCommand::GotoDefinition]);
        let shifted = KeyInput {
            key: "F12".into(),
            mods: Mods {
                shift: true,
                ..Default::default()
            },
            repeat: false,
        };
        assert_eq!(km.handle(&shifted), vec![EditCommand::FindReferences]);
    }

    #[test]
    fn insert_mode_entry_and_escape() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&["i"]), vec![EditCommand::SetMode(EditMode::Insert)]);
        assert_eq!(km.mode(), EditMode::Insert);
        assert_eq!(
            km.run(&["Escape"]),
            vec![
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn arrow_keys_navigate_in_normal_mode() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["ArrowDown"]),
            vec![EditCommand::Move(Motion::Down)]
        );
    }

    #[test]
    fn ctrl_navigation_never_falls_through_to_vim_commands() {
        let mut km = Fixture::default();
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
        let mut km = Fixture::default();
        km.run(&["3"]);
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
        let mut km = Fixture::default();
        assert_eq!(km.handle(&chord("r", ctrl())), vec![EditCommand::Redo]);
    }

    #[test]
    fn ctrl_c_escapes_insert_and_visual() {
        let mut km = Fixture::default();
        km.run(&["i"]);
        assert_eq!(
            km.handle(&chord("c", ctrl())),
            vec![
                EditCommand::Move(Motion::LeftBounded),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        km.run(&["v"]);
        assert_eq!(
            km.handle(&chord("c", ctrl())),
            vec![EditCommand::SetMode(EditMode::Normal)]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn colon_hands_the_line_to_the_command_bar() {
        let mut km = Fixture::default();
        assert_eq!(km.run(&[":"]), vec![EditCommand::OpenCommandLine]);
        assert_eq!(
            km.mode(),
            EditMode::Normal,
            "the bar owns the text, so the keymap must not swallow the keys that follow"
        );
        assert_eq!(
            km.run(&["w"]),
            vec![EditCommand::Move(Motion::WordNext)],
            "a key typed after the hand-off is an ordinary normal-mode key again"
        );
    }

    #[test]
    fn slash_and_question_hand_the_search_to_the_find_bar() {
        let mut km = Fixture::default();
        assert_eq!(
            km.run(&["/"]),
            vec![EditCommand::OpenFind { forward: true }]
        );
        assert_eq!(
            km.mode(),
            EditMode::Normal,
            "the bar owns the text, so the keymap must not swallow the keys that follow"
        );
        assert_eq!(
            km.run(&["?"]),
            vec![EditCommand::OpenFind { forward: false }]
        );
    }

    #[test]
    fn pointer_drag_enters_visual_and_click_returns_normal() {
        let mut km = Fixture::default();
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
