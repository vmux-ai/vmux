use crate::command::{AppCommand, WriteAppCommands};
use crate::settings::AppSettings;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use std::time::Instant;

pub struct KeyBindingPlugin;

impl Plugin for KeyBindingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_keybindings)
            .add_systems(Update, process_key_input.in_set(WriteAppCommands));
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyBinding {
    Direct(KeyCombo),
    Chord(KeyCombo, KeyCombo),
}

#[derive(Resource, Debug, Clone)]
pub struct KeyBindingMap {
    pub bindings: Vec<(KeyBinding, String)>,
    pub chord_timeout_ms: u64,
}

#[derive(Resource)]
pub struct ChordState {
    pub pending_prefix: Option<(KeyCombo, Instant)>,
}

impl Default for ChordState {
    fn default() -> Self {
        Self {
            pending_prefix: None,
        }
    }
}

fn init_keybindings(mut commands: Commands, settings: Option<Res<AppSettings>>) {
    let mut map = KeyBindingMap {
        bindings: AppCommand::default_key_bindings(),
        chord_timeout_ms: 1000,
    };

    if let Some(settings) = settings {
        map.chord_timeout_ms = settings.keybindings.chord_timeout_ms;
        for entry in &settings.keybindings.bindings {
            if let Some(binding) = entry.binding.to_key_binding() {
                if let Some(pos) = map.bindings.iter().position(|(_, id)| *id == entry.command) {
                    map.bindings[pos] = (binding, entry.command.clone());
                } else {
                    map.bindings.push((binding, entry.command.clone()));
                }
            }
        }
    }

    commands.insert_resource(map);
    commands.insert_resource(ChordState::default());
}

fn process_key_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    bindings: Res<KeyBindingMap>,
    mut chord_state: ResMut<ChordState>,
    mut writer: MessageWriter<AppCommand>,
) {
    let current_modifiers = read_current_modifiers(&keyboard);

    if let Some((_, instant)) = &chord_state.pending_prefix {
        let timeout = std::time::Duration::from_millis(bindings.chord_timeout_ms);
        if instant.elapsed() > timeout {
            chord_state.pending_prefix = None;
        }
    }

    for key in keyboard.get_just_pressed() {
        if is_modifier_key(*key) {
            continue;
        }

        let pressed = KeyCombo {
            key: *key,
            modifiers: current_modifiers,
        };

        if let Some((ref prefix, instant)) = chord_state.pending_prefix {
            let timeout = std::time::Duration::from_millis(bindings.chord_timeout_ms);
            if instant.elapsed() <= timeout {
                for (binding, cmd_id) in &bindings.bindings {
                    if let KeyBinding::Chord(b_prefix, b_second) = binding {
                        if b_prefix == prefix && b_second == &pressed {
                            if let Some(cmd) = AppCommand::from_menu_id(cmd_id.as_str()) {
                                writer.write(cmd);
                            }
                            chord_state.pending_prefix = None;
                            return;
                        }
                    }
                }
            }
            chord_state.pending_prefix = None;
        }

        for (binding, cmd_id) in &bindings.bindings {
            match binding {
                KeyBinding::Direct(combo) if *combo == pressed => {
                    if let Some(cmd) = AppCommand::from_menu_id(cmd_id.as_str()) {
                        writer.write(cmd);
                    }
                    return;
                }
                KeyBinding::Chord(prefix, _) if *prefix == pressed => {
                    chord_state.pending_prefix = Some((pressed, Instant::now()));
                    return;
                }
                _ => {}
            }
        }
    }
}

fn read_current_modifiers(keyboard: &ButtonInput<KeyCode>) -> Modifiers {
    Modifiers {
        ctrl: keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight),
        shift: keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight),
        alt: keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight),
        super_key: keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight),
    }
}

fn is_modifier_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
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
