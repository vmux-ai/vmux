use crate::{AppCommand, BrowserCommand, OpenCommand, PaneDirection, PaneOpenMode, PaneTarget};
use bevy::ecs::resource::Resource;
use bevy::input::keyboard::KeyCode;
use std::time::Instant;

/// Every binding in force, and how long a chord may stay half-typed.
///
/// Built once and held as a resource. Two surfaces consult it — the native keyboard and a webview
/// page — and they used to carry a copy of these lookups each, which is how they drifted: the web
/// copy rebuilt the whole binding list on every keystroke, and the pair of legacy command aliases
/// below had to be fixed in both places or in neither.
#[derive(Resource, Debug, Clone)]
pub struct Keymap {
    pub bindings: Vec<(Shortcut, String)>,
    pub chord_timeout_ms: u64,
}

/// A chord that has not been answered within this long is abandoned, so a half-typed prefix cannot
/// silently swallow the next real keystroke.
const DEFAULT_CHORD_TIMEOUT_MS: u64 = 1000;

impl Keymap {
    /// The bindings compiled into the command tree, before settings have their say.
    pub fn defaults() -> Self {
        Self {
            bindings: AppCommand::default_shortcuts(),
            chord_timeout_ms: DEFAULT_CHORD_TIMEOUT_MS,
        }
    }

    /// The command bound to this key on its own.
    pub fn direct(&self, pressed: &KeyCombo) -> Option<AppCommand> {
        self.bindings
            .iter()
            .find_map(|(binding, id)| match binding {
                Shortcut::Direct(combo) if combo == pressed => AppCommand::from_shortcut_id(id),
                _ => None,
            })
    }

    /// Whether this key opens a chord, and so should be held rather than acted on.
    pub fn has_chord_prefix(&self, pressed: &KeyCombo) -> bool {
        self.bindings
            .iter()
            .any(|(binding, _)| matches!(binding, Shortcut::Chord(prefix, _) if prefix == pressed))
    }

    /// The command bound to this key as the second half of a chord.
    pub fn chord(&self, prefix: &KeyCombo, pressed: &KeyCombo) -> Option<AppCommand> {
        let second = pressed.chord_second_after(prefix);
        self.bindings
            .iter()
            .find_map(|(binding, id)| match binding {
                Shortcut::Chord(bound_prefix, bound_second)
                    if bound_prefix == prefix && bound_second == &second =>
                {
                    AppCommand::from_shortcut_id(id)
                }
                _ => None,
            })
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyCombo {
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
