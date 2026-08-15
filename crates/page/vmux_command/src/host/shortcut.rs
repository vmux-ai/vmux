use crate::{AppCommand, BrowserCommand, OpenCommand, PaneDirection, PaneOpenMode, PaneTarget};
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::input::keyboard::KeyCode;
use std::time::Instant;
use vmux_core::input::{ClaimedKey, KeyClaims, KeyModifiers};

/// Every binding in force, and how long a chord may stay half-typed.
///
/// Built once and held as a resource. Three surfaces consult it — the native keyboard, the macOS
/// event monitor and a webview page — and they used to carry a copy of these lookups each, which is
/// how they drifted.
///
/// Held in precedence order, so the first match wins and callers need no tie-break of their own.
#[derive(Resource, Debug, Clone, Default)]
pub struct Keymap {
    bindings: Vec<(Source, Binding)>,
    pub chord_timeout_ms: u64,
}

/// One binding: what to press, what it runs, and when it applies.
#[derive(Debug, Clone)]
pub struct Binding {
    pub shortcut: Shortcut,
    pub command: String,
    pub when: Option<When>,
}

/// Where a binding came from. A settings file outranks the compiled-in default it replaces —
/// otherwise rebinding a key that already has a default would silently do nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Default,
    Settings,
}

/// A chord that has not been answered within this long is abandoned, so a half-typed prefix cannot
/// silently swallow the next real keystroke.
const DEFAULT_CHORD_TIMEOUT_MS: u64 = 1000;

impl Keymap {
    /// The bindings compiled into the command tree, before any settings file has its say.
    pub fn defaults() -> Self {
        let mut keymap = Self {
            bindings: Vec::new(),
            chord_timeout_ms: DEFAULT_CHORD_TIMEOUT_MS,
        };
        keymap.extend(Source::Default, AppCommand::default_shortcuts());
        keymap
    }

    /// Add bindings from one source, keeping the whole list in precedence order.
    ///
    /// Sorted rather than appended because the lookups take the first match: a binding that arrives
    /// later but outranks what is already there has to move ahead of it. The sort is stable, so
    /// bindings that tie stay in the order they were declared.
    pub fn extend(&mut self, source: Source, bindings: impl IntoIterator<Item = Binding>) {
        self.bindings
            .extend(bindings.into_iter().map(|binding| (source, binding)));
        self.bindings.sort_by_key(|(source, binding)| {
            let specificity = binding.when.as_ref().map_or(0, When::specificity);
            (std::cmp::Reverse(*source), std::cmp::Reverse(specificity))
        });
    }

    /// Every binding in force, most specific first.
    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.bindings.iter().map(|(_, binding)| binding)
    }

    /// Repoint every chord at a different prefix.
    ///
    /// Rebinding the leader moves the whole family of chords at once. Doing it any other way would
    /// leave the defaults on the compiled-in prefix and only the overrides on the new one.
    pub fn set_leader(&mut self, leader: &KeyCombo) {
        for (_, binding) in &mut self.bindings {
            if let Shortcut::Chord(prefix, _) = &mut binding.shortcut {
                *prefix = leader.clone();
            }
        }
    }

    /// This keymap as it looks to a surface that has published these context keys.
    pub fn in_context<'a>(&'a self, context: &'a KeyContext) -> KeymapView<'a> {
        KeymapView {
            keymap: self,
            context,
        }
    }

    /// The command bound to this key on its own, ignoring anything context-scoped.
    pub fn direct(&self, pressed: &KeyCombo) -> Option<AppCommand> {
        self.in_context(KeyContext::NONE).direct(pressed)
    }

    /// Whether this key opens a chord, and so should be held rather than acted on.
    pub fn has_chord_prefix(&self, pressed: &KeyCombo) -> bool {
        self.in_context(KeyContext::NONE).has_chord_prefix(pressed)
    }

    /// The command bound to this key as the second half of a chord.
    pub fn chord(&self, prefix: &KeyCombo, pressed: &KeyCombo) -> Option<AppCommand> {
        self.in_context(KeyContext::NONE).chord(prefix, pressed)
    }
}

/// A keymap narrowed to one surface's context, which is what makes a key mean different things in
/// different places without either place knowing about the other.
pub struct KeymapView<'a> {
    keymap: &'a Keymap,
    context: &'a KeyContext,
}

impl KeymapView<'_> {
    fn applicable(&self) -> impl Iterator<Item = &Binding> {
        self.keymap
            .bindings()
            .filter(|binding| match &binding.when {
                Some(when) => when.matches(self.context),
                None => true,
            })
    }

    pub fn direct(&self, pressed: &KeyCombo) -> Option<AppCommand> {
        self.applicable()
            .find_map(|binding| match &binding.shortcut {
                Shortcut::Direct(combo) if combo == pressed => {
                    AppCommand::from_shortcut_id(&binding.command)
                }
                _ => None,
            })
    }

    /// The command this key means *because of* where it was pressed.
    ///
    /// Only context-scoped bindings are considered, and that is the whole point: a key a page hands
    /// over has already passed the native keyboard, which resolves the unconditional bindings for
    /// every surface at once. Answering those a second time here would issue the command twice.
    pub fn scoped(&self, pressed: &KeyCombo) -> Option<AppCommand> {
        self.applicable()
            .filter(|binding| binding.when.is_some())
            .find_map(|binding| match &binding.shortcut {
                Shortcut::Direct(combo) if combo == pressed => {
                    AppCommand::from_shortcut_id(&binding.command)
                }
                _ => None,
            })
    }

    pub fn has_chord_prefix(&self, pressed: &KeyCombo) -> bool {
        self.applicable().any(
            |binding| matches!(&binding.shortcut, Shortcut::Chord(prefix, _) if prefix == pressed),
        )
    }

    pub fn chord(&self, prefix: &KeyCombo, pressed: &KeyCombo) -> Option<AppCommand> {
        let second = pressed.chord_second_after(prefix);
        self.applicable()
            .find_map(|binding| match &binding.shortcut {
                Shortcut::Chord(bound_prefix, bound_second)
                    if bound_prefix == prefix && bound_second == &second =>
                {
                    AppCommand::from_shortcut_id(&binding.command)
                }
                _ => None,
            })
    }

    /// Every stroke a page in this context has to hand over rather than let the browser act on.
    ///
    /// A chord contributes its prefix, not its second key: pressing the prefix is what the page
    /// must give up, and by the time the second key is typed the core has already stopped
    /// forwarding keys to the page at all.
    ///
    /// Bindings naming a command that no longer exists are skipped, because claiming a key that
    /// goes on to do nothing costs the page the key for nothing.
    pub fn claims(&self) -> KeyClaims {
        let mut keys: Vec<ClaimedKey> = Vec::new();
        for binding in self.applicable() {
            if AppCommand::from_shortcut_id(&binding.command).is_none() {
                continue;
            }
            let combo = match &binding.shortcut {
                Shortcut::Direct(combo) => combo,
                Shortcut::Chord(prefix, _) => prefix,
            };
            let Some(claimed) = combo.claimed() else {
                continue;
            };
            if keys.contains(&claimed) {
                continue;
            }
            keys.push(claimed);
        }
        KeyClaims { keys }
    }
}

impl AppCommand {
    /// The command a binding names, accepting two ids that predate the command tree.
    ///
    /// `split_v` and `split_h` were never menu items, so they have no generated id; they are kept
    /// because they appear in settings files already written.
    pub fn from_shortcut_id(id: &str) -> Option<Self> {
        let split = |direction| {
            Some(AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction,
                    target: PaneTarget::NewSplit,
                    mode: PaneOpenMode::NewStack,
                    url: None,
                },
            )))
        };
        match id {
            "split_v" => split(PaneDirection::Right),
            "split_h" => split(PaneDirection::Bottom),
            _ => AppCommand::from_menu_id(id),
        }
    }
}

/// The condition under which a binding applies, read from a settings file's `when` field.
///
/// Deliberately not an expression language. Every term must hold, and a term is a context key
/// optionally negated — `chat.selector`, or `chat && !chat.approval`. A binding with more terms is
/// more specific and is tried first, which is what lets `Enter` mean "choose the highlighted row"
/// with a picker open and "submit" without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct When(Vec<WhenTerm>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct WhenTerm {
    key: String,
    negated: bool,
}

impl When {
    /// Read a `when` field. `None` when it names no terms, so an empty string cannot silently
    /// produce a condition that holds everywhere and outranks the unconditional bindings.
    pub fn parse(text: &str) -> Option<Self> {
        let mut terms = Vec::new();
        for term in text.split("&&") {
            let term = term.trim();
            let (negated, key) = match term.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, term),
            };
            if key.is_empty() {
                return None;
            }
            terms.push(WhenTerm {
                key: key.to_string(),
                negated,
            });
        }
        (!terms.is_empty()).then_some(Self(terms))
    }

    fn specificity(&self) -> usize {
        self.0.len()
    }

    fn matches(&self, context: &KeyContext) -> bool {
        self.0
            .iter()
            .all(|term| context.has(&term.key) != term.negated)
    }
}

/// The context keys a surface has published about itself.
///
/// Strings rather than a closed enum because a settings file has to name them, and the set grows
/// with the pages rather than with this crate.
///
/// A component on the webview it describes, not a resource. Two chat panes have to be able to
/// differ — one with a picker open and one without — and they can only differ if the context hangs
/// off the entity. That costs nothing, because a page's message already names its webview.
///
/// Deriving `Resource` here would look equivalent and is not: a resource-derived type also
/// implements `Component`, but Bevy keeps its singleton semantics, so the *second* entity to be
/// given one silently gets nothing.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyContext(std::collections::BTreeSet<String>);

impl KeyContext {
    /// No context at all — what a caller that has not published one sees. Context-scoped bindings
    /// never match it, so an unscoped surface cannot accidentally claim another's keys.
    pub const NONE: &'static Self = &Self(std::collections::BTreeSet::new());

    pub fn has(&self, key: &str) -> bool {
        self.0.contains(key)
    }

    /// Replace the whole set. A surface publishes what is true of it now rather than diffing,
    /// because a missed toggle would leave a key claimed by a picker that has since closed.
    pub fn set(&mut self, keys: impl IntoIterator<Item = String>) {
        self.0 = keys.into_iter().collect();
    }
}

impl FromIterator<String> for KeyContext {
    fn from_iter<I: IntoIterator<Item = String>>(keys: I) -> Self {
        Self(keys.into_iter().collect())
    }
}

#[derive(Resource, Default)]
pub struct ChordState {
    pub pending_prefix: Option<(KeyCombo, Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub super_key: bool,
}

impl From<Modifiers> for KeyModifiers {
    fn from(modifiers: Modifiers) -> Self {
        Self {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            super_key: modifiers.super_key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyCombo {
    /// The combo a page's keystroke names, or `None` for a bare modifier or a key with no
    /// [`KeyCode`].
    ///
    /// Read from `code` rather than `key`, the same half of the event [`KeyCombo::web_code`]
    /// writes, so a layout that renames the character cannot change which binding matches.
    pub fn of(stroke: &vmux_core::input::KeyStroke) -> Option<Self> {
        if stroke.is_modifier_key() {
            return None;
        }
        Some(Self {
            key: key_code_from_str(&stroke.code)?,
            modifiers: Modifiers {
                ctrl: stroke.mods.ctrl,
                shift: stroke.mods.shift,
                alt: stroke.mods.alt,
                super_key: stroke.mods.super_key,
            },
        })
    }

    /// Escape with no modifier held.
    pub fn is_bare_escape(&self) -> bool {
        self.key == KeyCode::Escape
            && !self.modifiers.ctrl
            && !self.modifiers.alt
            && !self.modifiers.super_key
    }

    /// True for the two combos that close the command bar: bare Escape, and Ctrl-C.
    ///
    /// The bar is the only surface that answers Ctrl-C this way, so the pairing belongs here
    /// rather than in whichever key monitor happens to ask.
    pub fn dismisses_command_bar(&self) -> bool {
        self.is_bare_escape()
            || (self.key == KeyCode::KeyC
                && self.modifiers.ctrl
                && !self.modifiers.shift
                && !self.modifiers.alt
                && !self.modifiers.super_key)
    }

    /// This combo as a claim a page can test, or `None` when the page decides it without asking.
    ///
    /// The excluded case is exactly [`KeyCombo::is_text_input`]: a printable key held with nothing
    /// but Shift. A page answers that from the keystroke alone, in the same tick, so putting it in
    /// a set that is briefly stale after every context change could only lose a character.
    /// Everything else is claimable, because a page has no way to know whether it is bound.
    pub fn claimed(&self) -> Option<ClaimedKey> {
        if self.is_text_input() {
            return None;
        }
        Some(ClaimedKey {
            code: self.web_code(),
            mods: KeyModifiers::from(self.modifiers),
        })
    }

    /// True when pressing this is someone typing rather than invoking something.
    ///
    /// The counterpart of [`vmux_core::input::KeyStroke::is_text_input`] on the keymap side of the
    /// wire, and it has to agree with it: this decides what is left out of the claimed set, and
    /// that one decides what the page keeps when the set says nothing.
    fn is_text_input(&self) -> bool {
        if KeyModifiers::from(self.modifiers).has_chord() {
            return false;
        }
        !matches!(
            self.key,
            KeyCode::Backspace
                | KeyCode::CapsLock
                | KeyCode::Delete
                | KeyCode::End
                | KeyCode::Enter
                | KeyCode::Escape
                | KeyCode::Home
                | KeyCode::Insert
                | KeyCode::PageDown
                | KeyCode::PageUp
                | KeyCode::Tab
                | KeyCode::ArrowDown
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::ArrowUp
                | KeyCode::F1
                | KeyCode::F2
                | KeyCode::F3
                | KeyCode::F4
                | KeyCode::F5
                | KeyCode::F6
                | KeyCode::F7
                | KeyCode::F8
                | KeyCode::F9
                | KeyCode::F10
                | KeyCode::F11
                | KeyCode::F12
        )
    }

    /// This key's `code` attribute, as a page reports it.
    ///
    /// [`KeyCode`]'s variants are named after the W3C `code` values, so its `Debug` spelling *is*
    /// the wire name. Mirroring [`key_code_from_str`] by hand instead would be a second copy of
    /// that table with nothing keeping the two in step; `web_code_round_trips_through_resolve_key`
    /// pins the assumption against the table itself and fails loudly if Bevy ever renames one.
    fn web_code(&self) -> String {
        format!("{:?}", self.key)
    }

    /// This key read as the second half of a chord opened by `prefix`.
    ///
    /// A modifier the prefix already holds is dropped, because people keep Ctrl down through
    /// `Ctrl+g s` rather than releasing it between the halves. Shift is kept: it distinguishes the
    /// second key rather than merely surviving from the first.
    fn chord_second_after(&self, prefix: &KeyCombo) -> KeyCombo {
        let mut second = self.clone();
        second.modifiers.ctrl &= !prefix.modifiers.ctrl;
        second.modifiers.alt &= !prefix.modifiers.alt;
        second.modifiers.super_key &= !prefix.modifiers.super_key;
        second
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Shortcut {
    Direct(KeyCombo),
    Chord(KeyCombo, KeyCombo),
}

pub struct ResolvedKey {
    pub key: KeyCode,
    pub implicit_shift: bool,
}

pub fn resolve_key(s: &str) -> Option<ResolvedKey> {
    if let Some(key) = key_code_from_str(s) {
        return Some(ResolvedKey {
            key,
            implicit_shift: false,
        });
    }

    let chars: Vec<char> = s.chars().collect();
    if chars.len() == 1 {
        return resolve_char_literal(chars[0]);
    }

    None
}

fn resolve_char_literal(c: char) -> Option<ResolvedKey> {
    let (key, shifted) = match c {
        'a'..='z' => (
            key_code_from_str(&format!("Key{}", c.to_ascii_uppercase()))?,
            false,
        ),
        'A'..='Z' => (key_code_from_str(&format!("Key{}", c))?, true),
        '0'..='9' => (key_code_from_str(&format!("Digit{}", c))?, false),
        ')' => (KeyCode::Digit0, true),
        '!' => (KeyCode::Digit1, true),
        '@' => (KeyCode::Digit2, true),
        '#' => (KeyCode::Digit3, true),
        '$' => (KeyCode::Digit4, true),
        '%' => (KeyCode::Digit5, true),
        '^' => (KeyCode::Digit6, true),
        '&' => (KeyCode::Digit7, true),
        '*' => (KeyCode::Digit8, true),
        '(' => (KeyCode::Digit9, true),
        '-' => (KeyCode::Minus, false),
        '_' => (KeyCode::Minus, true),
        '=' => (KeyCode::Equal, false),
        '/' => (KeyCode::Slash, false),
        '?' => (KeyCode::Slash, true),
        '.' => (KeyCode::Period, false),
        '>' => (KeyCode::Period, true),
        ',' => (KeyCode::Comma, false),
        '<' => (KeyCode::Comma, true),
        ';' => (KeyCode::Semicolon, false),
        ':' => (KeyCode::Semicolon, true),
        '\'' => (KeyCode::Quote, false),
        '"' => (KeyCode::Quote, true),
        '[' => (KeyCode::BracketLeft, false),
        '{' => (KeyCode::BracketLeft, true),
        ']' => (KeyCode::BracketRight, false),
        '}' => (KeyCode::BracketRight, true),
        '\\' => (KeyCode::Backslash, false),
        '|' => (KeyCode::Backslash, true),
        '`' => (KeyCode::Backquote, false),
        '~' => (KeyCode::Backquote, true),
        ' ' => (KeyCode::Space, false),
        _ => return None,
    };
    Some(ResolvedKey {
        key,
        implicit_shift: shifted,
    })
}

fn key_code_from_str(s: &str) -> Option<KeyCode> {
    match s {
        "Backquote" => Some(KeyCode::Backquote),
        "Backslash" => Some(KeyCode::Backslash),
        "BracketLeft" => Some(KeyCode::BracketLeft),
        "BracketRight" => Some(KeyCode::BracketRight),
        "Comma" => Some(KeyCode::Comma),
        "Digit0" => Some(KeyCode::Digit0),
        "Digit1" => Some(KeyCode::Digit1),
        "Digit2" => Some(KeyCode::Digit2),
        "Digit3" => Some(KeyCode::Digit3),
        "Digit4" => Some(KeyCode::Digit4),
        "Digit5" => Some(KeyCode::Digit5),
        "Digit6" => Some(KeyCode::Digit6),
        "Digit7" => Some(KeyCode::Digit7),
        "Digit8" => Some(KeyCode::Digit8),
        "Digit9" => Some(KeyCode::Digit9),
        "Equal" => Some(KeyCode::Equal),
        "IntlBackslash" => Some(KeyCode::IntlBackslash),
        "IntlRo" => Some(KeyCode::IntlRo),
        "IntlYen" => Some(KeyCode::IntlYen),
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),
        "Minus" => Some(KeyCode::Minus),
        "Period" => Some(KeyCode::Period),
        "Quote" => Some(KeyCode::Quote),
        "Semicolon" => Some(KeyCode::Semicolon),
        "Slash" => Some(KeyCode::Slash),
        "Backspace" => Some(KeyCode::Backspace),
        "CapsLock" => Some(KeyCode::CapsLock),
        "Enter" => Some(KeyCode::Enter),
        "Space" => Some(KeyCode::Space),
        "Tab" => Some(KeyCode::Tab),
        "Delete" => Some(KeyCode::Delete),
        "End" => Some(KeyCode::End),
        "Home" => Some(KeyCode::Home),
        "Insert" => Some(KeyCode::Insert),
        "PageDown" => Some(KeyCode::PageDown),
        "PageUp" => Some(KeyCode::PageUp),
        "ArrowDown" => Some(KeyCode::ArrowDown),
        "ArrowLeft" => Some(KeyCode::ArrowLeft),
        "ArrowRight" => Some(KeyCode::ArrowRight),
        "ArrowUp" => Some(KeyCode::ArrowUp),
        "Escape" => Some(KeyCode::Escape),
        "F1" => Some(KeyCode::F1),
        "F2" => Some(KeyCode::F2),
        "F3" => Some(KeyCode::F3),
        "F4" => Some(KeyCode::F4),
        "F5" => Some(KeyCode::F5),
        "F6" => Some(KeyCode::F6),
        "F7" => Some(KeyCode::F7),
        "F8" => Some(KeyCode::F8),
        "F9" => Some(KeyCode::F9),
        "F10" => Some(KeyCode::F10),
        "F11" => Some(KeyCode::F11),
        "F12" => Some(KeyCode::F12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two ids that exist in the command tree and are not the same command, so a precedence test
    /// cannot pass by accident.
    const STACK_CLOSE: &str = "stack_close";
    const PANE_CLOSE: &str = "close_pane";

    fn combo(key: KeyCode) -> KeyCombo {
        KeyCombo {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn binding(command: &str, when: Option<&str>) -> Binding {
        Binding {
            shortcut: Shortcut::Direct(combo(KeyCode::KeyX)),
            command: command.to_string(),
            when: when.and_then(When::parse),
        }
    }

    fn context(keys: &[&str]) -> KeyContext {
        keys.iter().map(|key| key.to_string()).collect()
    }

    fn combo_with(key: KeyCode, modifiers: Modifiers) -> KeyCombo {
        KeyCombo { key, modifiers }
    }

    #[test]
    fn bare_escape_is_escape_with_no_modifier_but_shift() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        let super_key = Modifiers {
            super_key: true,
            ..Default::default()
        };

        assert!(combo(KeyCode::Escape).is_bare_escape());
        assert!(combo_with(KeyCode::Escape, shift).is_bare_escape());
        assert!(!combo_with(KeyCode::Escape, ctrl).is_bare_escape());
        assert!(!combo_with(KeyCode::Escape, super_key).is_bare_escape());
        assert!(!combo(KeyCode::KeyH).is_bare_escape());
    }

    #[test]
    fn command_bar_dismisses_on_escape_and_ctrl_c_only() {
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let super_key = Modifiers {
            super_key: true,
            ..Default::default()
        };

        assert!(combo(KeyCode::Escape).dismisses_command_bar());
        assert!(combo_with(KeyCode::KeyC, ctrl).dismisses_command_bar());
        assert!(!combo(KeyCode::KeyC).dismisses_command_bar());
        assert!(!combo_with(KeyCode::KeyC, super_key).dismisses_command_bar());
    }

    /// The point of putting bindings in a settings file: rebinding a key that already has a
    /// compiled-in default has to actually take effect.
    #[test]
    fn a_configured_binding_outranks_the_default_on_the_same_key() {
        let mut keymap = Keymap::default();
        keymap.extend(Source::Default, [binding(STACK_CLOSE, None)]);
        keymap.extend(Source::Settings, [binding(PANE_CLOSE, None)]);

        assert_eq!(
            keymap.direct(&combo(KeyCode::KeyX)),
            AppCommand::from_shortcut_id(PANE_CLOSE)
        );
    }

    /// Order of arrival must not decide the winner, or the same settings file would behave
    /// differently depending on how the keymap happened to be assembled.
    #[test]
    fn the_default_still_loses_when_it_arrives_last() {
        let mut keymap = Keymap::default();
        keymap.extend(Source::Settings, [binding(PANE_CLOSE, None)]);
        keymap.extend(Source::Default, [binding(STACK_CLOSE, None)]);

        assert_eq!(
            keymap.direct(&combo(KeyCode::KeyX)),
            AppCommand::from_shortcut_id(PANE_CLOSE)
        );
    }

    /// The whole reason `when` exists: one key meaning two things depending on what is open.
    #[test]
    fn a_scoped_binding_wins_only_inside_its_context() {
        let mut keymap = Keymap::default();
        keymap.extend(
            Source::Settings,
            [
                binding(STACK_CLOSE, None),
                binding(PANE_CLOSE, Some("chat.selector")),
            ],
        );

        assert_eq!(
            keymap
                .in_context(&context(&["chat.selector"]))
                .direct(&combo(KeyCode::KeyX)),
            AppCommand::from_shortcut_id(PANE_CLOSE)
        );
        assert_eq!(
            keymap
                .in_context(&context(&["chat"]))
                .direct(&combo(KeyCode::KeyX)),
            AppCommand::from_shortcut_id(STACK_CLOSE)
        );
    }

    /// A surface that publishes no context must not inherit another's scoped bindings.
    #[test]
    fn a_scoped_binding_never_matches_an_absent_context() {
        let mut keymap = Keymap::default();
        keymap.extend(
            Source::Settings,
            [binding(PANE_CLOSE, Some("chat.selector"))],
        );

        assert_eq!(keymap.direct(&combo(KeyCode::KeyX)), None);
    }

    #[test]
    fn every_term_must_hold_and_a_negated_term_inverts() {
        let when = When::parse("chat && !chat.approval").unwrap();
        assert_eq!(when.specificity(), 2);
        assert!(when.matches(&context(&["chat"])));
        assert!(!when.matches(&context(&["chat", "chat.approval"])));
        assert!(!when.matches(&context(&["chat.approval"])));
    }

    /// An empty condition would otherwise parse as a clause that holds everywhere and, being
    /// present, sort ahead of the unconditional bindings it should tie with.
    #[test]
    fn a_condition_naming_no_terms_is_not_a_condition() {
        assert_eq!(When::parse(""), None);
        assert_eq!(When::parse("   "), None);
        assert_eq!(When::parse("chat && "), None);
        assert_eq!(When::parse("!"), None);
    }

    fn modified(key: KeyCode, modifiers: Modifiers) -> KeyCombo {
        KeyCombo { key, modifiers }
    }

    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    };
    const SHIFT: Modifiers = Modifiers {
        ctrl: false,
        shift: true,
        alt: false,
        super_key: false,
    };

    /// `web_code` reads a page-facing string off `KeyCode`'s `Debug`. The `key_code_from_str` table
    /// is the independent oracle: every name it accepts has to come back out unchanged, so a Bevy
    /// rename shows up here rather than as a shortcut that silently stops firing in the browser.
    #[test]
    fn web_code_round_trips_through_resolve_key() {
        for name in [
            "KeyA",
            "KeyG",
            "KeyZ",
            "Digit0",
            "Digit9",
            "Backquote",
            "Backslash",
            "BracketLeft",
            "BracketRight",
            "Comma",
            "Equal",
            "IntlBackslash",
            "IntlRo",
            "IntlYen",
            "Minus",
            "Period",
            "Quote",
            "Semicolon",
            "Slash",
            "Backspace",
            "CapsLock",
            "Enter",
            "Space",
            "Tab",
            "Delete",
            "End",
            "Home",
            "Insert",
            "PageDown",
            "PageUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "ArrowUp",
            "Escape",
            "F1",
            "F9",
            "F12",
        ] {
            let key = resolve_key(name).expect("table key resolves").key;
            assert_eq!(combo(key).web_code(), name);
        }
    }

    /// The rule that decides the claimed set. A printable key held with nothing but Shift stays
    /// with the page, because a stale set claiming it would swallow a character; everything else is
    /// claimable, because the page cannot tell whether it is bound.
    #[test]
    fn only_strokes_a_page_cannot_decide_for_itself_are_claimable() {
        assert_eq!(combo(KeyCode::KeyX).claimed(), None);
        assert_eq!(combo(KeyCode::Space).claimed(), None);
        assert_eq!(combo(KeyCode::Digit5).claimed(), None);
        assert_eq!(modified(KeyCode::KeyX, SHIFT).claimed(), None);

        assert!(modified(KeyCode::KeyX, CTRL).claimed().is_some());
        assert!(combo(KeyCode::Escape).claimed().is_some());
        assert!(combo(KeyCode::Enter).claimed().is_some());
        assert!(combo(KeyCode::ArrowUp).claimed().is_some());
        assert!(combo(KeyCode::F5).claimed().is_some());
    }

    /// What a page is told to hand over, and what changes when its context does. A chord is
    /// represented by its prefix, since that is the press the page has to give up.
    #[test]
    fn the_claimed_set_follows_the_context() {
        let mut keymap = Keymap::default();
        keymap.extend(
            Source::Settings,
            [
                Binding {
                    shortcut: Shortcut::Direct(modified(KeyCode::KeyX, CTRL)),
                    command: STACK_CLOSE.to_string(),
                    when: None,
                },
                Binding {
                    shortcut: Shortcut::Direct(combo(KeyCode::Escape)),
                    command: PANE_CLOSE.to_string(),
                    when: When::parse("chat.selector"),
                },
                Binding {
                    shortcut: Shortcut::Chord(modified(KeyCode::KeyG, CTRL), combo(KeyCode::KeyS)),
                    command: PANE_CLOSE.to_string(),
                    when: None,
                },
            ],
        );

        let codes = |context: &KeyContext| -> Vec<String> {
            let mut codes: Vec<String> = keymap
                .in_context(context)
                .claims()
                .keys
                .into_iter()
                .map(|claimed| claimed.code)
                .collect();
            codes.sort();
            codes
        };

        assert_eq!(codes(KeyContext::NONE), vec!["KeyG", "KeyX"]);
        assert_eq!(
            codes(&context(&["chat.selector"])),
            vec!["Escape", "KeyG", "KeyX"]
        );
    }

    /// A binding naming a command that no longer exists would otherwise cost the page a key and
    /// give it nothing back.
    #[test]
    fn a_binding_on_an_unknown_command_claims_nothing() {
        let mut keymap = Keymap::default();
        keymap.extend(
            Source::Settings,
            [Binding {
                shortcut: Shortcut::Direct(modified(KeyCode::KeyX, CTRL)),
                command: "no_such_command".to_string(),
                when: None,
            }],
        );

        assert_eq!(keymap.in_context(KeyContext::NONE).claims().keys, vec![]);
    }

    /// The two sides of the wire have to agree on what a claim matches, or the page would hand over
    /// a key the core does not act on — or keep one it does.
    #[test]
    fn a_claim_matches_the_stroke_that_produced_it() {
        let claims = KeyClaims {
            keys: vec![modified(KeyCode::KeyX, CTRL).claimed().unwrap()],
        };
        let stroke = |mods: KeyModifiers| vmux_core::input::KeyStroke {
            key: "x".to_string(),
            code: "KeyX".to_string(),
            mods,
            text: None,
            repeat: false,
        };

        assert!(claims.contains(&stroke(KeyModifiers::from(CTRL))));
        assert!(!claims.contains(&stroke(KeyModifiers::default())));
        assert!(!claims.contains(&stroke(KeyModifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        })));
    }

    /// What the spaces page gets back for each chord it hands over, out of the compiled defaults.
    ///
    /// [`KeymapView::claims`] skips a binding whose command id no longer resolves, so a leaf that
    /// stopped round-tripping would cost the page the key with no error anywhere: it would simply
    /// stop navigating. `scoped` is what `PageKeyPlugin` calls on a stroke that arrived, so this is
    /// the answer the page itself would receive.
    #[test]
    fn the_spaces_page_resolves_every_chord_it_hands_over() {
        use crate::{LayoutCommand, SpaceCommand};

        let keymap = Keymap::defaults();
        let on_spaces = context(&["spaces"]);
        let table = [
            (combo(KeyCode::ArrowDown), SpaceCommand::Next),
            (modified(KeyCode::KeyN, CTRL), SpaceCommand::Next),
            (modified(KeyCode::KeyJ, CTRL), SpaceCommand::Next),
            (combo(KeyCode::ArrowUp), SpaceCommand::Previous),
            (modified(KeyCode::KeyP, CTRL), SpaceCommand::Previous),
            (modified(KeyCode::KeyK, CTRL), SpaceCommand::Previous),
            (combo(KeyCode::Enter), SpaceCommand::Attach),
            (combo(KeyCode::Delete), SpaceCommand::Delete),
            (combo(KeyCode::Backspace), SpaceCommand::Delete),
        ];

        for (pressed, expected) in table {
            assert_eq!(
                keymap.in_context(&on_spaces).scoped(&pressed),
                Some(AppCommand::Layout(LayoutCommand::Space(expected))),
                "{pressed:?}"
            );
            let claimed = pressed.claimed().expect("a bound chord is claimable");
            assert!(
                keymap
                    .in_context(&on_spaces)
                    .claims()
                    .keys
                    .contains(&claimed),
                "{pressed:?} is bound but never claimed, so the page would never hand it over"
            );
        }
    }

    /// Every spaces key is scoped, so off that page it belongs to whoever is focused. One that
    /// leaked would delete a space from a terminal.
    #[test]
    fn a_spaces_chord_means_nothing_off_the_spaces_page() {
        let keymap = Keymap::defaults();

        for pressed in [
            combo(KeyCode::Backspace),
            combo(KeyCode::Delete),
            combo(KeyCode::Enter),
            combo(KeyCode::ArrowDown),
            modified(KeyCode::KeyJ, CTRL),
        ] {
            assert_eq!(
                keymap.in_context(KeyContext::NONE).scoped(&pressed),
                None,
                "{pressed:?}"
            );
        }
    }
}
