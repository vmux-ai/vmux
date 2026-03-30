//! ## Reference
//!
//! - [`cef_key_event_t`](https://cef-builds.spotifycdn.com/docs/106.1/structcef__key__event__t.html)
//! - [KeyboardCodes](https://chromium.googlesource.com/external/Webkit/+/safari-4-branch/WebCore/platform/KeyboardCodes.h)

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::{ButtonInput, KeyCode};
use cef_dll_sys::{cef_event_flags_t, cef_key_event_t, cef_key_event_type_t};

#[allow(clippy::unnecessary_cast)]
pub fn keyboard_modifiers(input: &ButtonInput<KeyCode>) -> u32 {
    let mut flags = 0u32;

    if input.pressed(KeyCode::ControlLeft) || input.pressed(KeyCode::ControlRight) {
        flags |= cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32;
    }
    if input.pressed(KeyCode::AltLeft) || input.pressed(KeyCode::AltRight) {
        flags |= cef_event_flags_t::EVENTFLAG_ALT_DOWN.0 as u32;
    }
    if input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight) {
        flags |= cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0 as u32;
    }
    if input.pressed(KeyCode::SuperLeft) || input.pressed(KeyCode::SuperRight) {
        flags |= cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0 as u32;
    }
    if input.pressed(KeyCode::CapsLock) {
        flags |= cef_event_flags_t::EVENTFLAG_CAPS_LOCK_ON.0 as u32;
    }
    if input.pressed(KeyCode::NumLock) {
        flags |= cef_event_flags_t::EVENTFLAG_NUM_LOCK_ON.0 as u32;
    }

    flags
}

#[inline]
fn control_modifier_down(modifiers: u32) -> bool {
    modifiers & (cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32) != 0
}

/// Text / UTF-16 character for [`KEYEVENT_CHAR`]. On macOS the first stroke in an `<input>` can
/// arrive with `text: None` while [`KeyboardInput::logical_key`] is still [`Key::Character`];
/// using only `text` produced CHAR with code 0 and odd latency until the next key.
#[inline]
fn utf16_input_char(key_event: &KeyboardInput) -> u16 {
    key_event
        .text
        .as_ref()
        .and_then(|t| t.chars().next())
        .or_else(|| match &key_event.logical_key {
            Key::Character(s) => s.chars().next(),
            _ => None,
        })
        .unwrap_or('\0') as u16
}

/// After long delete/backspace repeat, macOS can emit a frame where `text` is empty and
/// `logical_key` is not yet [`Key::Character`], so [`utf16_input_char`] returns 0. Without a
/// fallback we only emit `KEYEVENT_RAWKEYDOWN` and the first typed letter is lost until the next
/// keypress. Map physical [`KeyCode`] + shift/caps to a US-ASCII UTF-16 code unit when needed.
fn latin_utf16_fallback(key_event: &KeyboardInput, input: &ButtonInput<KeyCode>) -> Option<u16> {
    let shift = input.pressed(KeyCode::ShiftLeft) || input.pressed(KeyCode::ShiftRight);
    let caps = input.pressed(KeyCode::CapsLock);
    let upper = if caps { !shift } else { shift };

    let vk = keycode_to_windows_vk(key_event.key_code);
    if (0x41..=0x5A).contains(&vk) {
        let ch = vk as u8;
        let ch = if upper { ch } else { ch.to_ascii_lowercase() };
        return Some(ch as u16);
    }
    if (0x30..=0x39).contains(&vk) && !shift {
        return Some(vk as u16);
    }
    if key_event.key_code == KeyCode::Space {
        return Some(0x20);
    }
    None
}

#[inline]
fn resolved_utf16_char(key_event: &KeyboardInput, input: &ButtonInput<KeyCode>) -> u16 {
    let c = utf16_input_char(key_event);
    if c != 0 {
        return c;
    }
    latin_utf16_fallback(key_event, input).unwrap_or(0)
}

/// Converts a Bevy `KeyboardInput` into one or more CEF key events.
///
/// - **Character presses** (all platforms): `KEYEVENT_RAWKEYDOWN` then `KEYEVENT_CHAR` when the resolved
///   character is non-zero. OSR logs showed `CHAR` alone was dropped for the first letter after long
///   backspace bursts while `KEYUP` still fired; Chromium expects a keydown before text + keyup.
/// - **Character with unresolved char (0)** — Non-Windows: `KEYEVENT_RAWKEYDOWN` only (fallback path).
/// - **Control + character** (e.g. Ctrl+A): `KEYEVENT_RAWKEYDOWN` only — no `KEYEVENT_CHAR`. Sending a
///   printable CHAR with Control held confuses Chromium’s key handling and breaks shortcut listeners.
/// - **Non–text keys** — `KEYEVENT_RAWKEYDOWN` / `KEYEVENT_KEYUP`.
pub fn create_cef_key_events(
    modifiers: u32,
    input: &ButtonInput<KeyCode>,
    key_event: &KeyboardInput,
) -> Vec<cef::KeyEvent> {
    let native_key_code = to_native_key_code(&key_event.key_code) as _;
    let vk_code = keycode_to_windows_vk(key_event.key_code);

    let is_character_key_press = key_event.state == ButtonState::Pressed
        && !is_not_character_key_code(&key_event.key_code);

    if is_character_key_press {
        let character = resolved_utf16_char(key_event, input);

        let base = cef_key_event_t {
            size: core::mem::size_of::<cef_key_event_t>(),
            type_: cef_key_event_type_t::KEYEVENT_RAWKEYDOWN,
            modifiers,
            windows_key_code: vk_code,
            native_key_code,
            character: 0,
            unmodified_character: 0,
            is_system_key: false as _,
            focus_on_editable_field: false as _,
        };

        if character != 0 {
            if control_modifier_down(modifiers) {
                vec![cef::KeyEvent::from(base)]
            } else {
                let char_event = cef_key_event_t {
                    type_: cef_key_event_type_t::KEYEVENT_CHAR,
                    windows_key_code: character as i32,
                    character,
                    unmodified_character: character,
                    focus_on_editable_field: true as _,
                    ..base
                };
                vec![cef::KeyEvent::from(base), cef::KeyEvent::from(char_event)]
            }
        } else {
            vec![cef::KeyEvent::from(base)]
        }
    } else {
        let key_type = match key_event.state {
            ButtonState::Pressed => cef_key_event_type_t::KEYEVENT_RAWKEYDOWN,
            ButtonState::Released => cef_key_event_type_t::KEYEVENT_KEYUP,
        };

        vec![cef::KeyEvent::from(cef_key_event_t {
            size: core::mem::size_of::<cef_key_event_t>(),
            type_: key_type,
            modifiers,
            windows_key_code: vk_code,
            native_key_code,
            character: 0,
            unmodified_character: 0,
            is_system_key: false as _,
            focus_on_editable_field: false as _,
        })]
    }
}

fn is_not_character_key_code(keycode: &KeyCode) -> bool {
    match keycode {
        // Function keys are not character keys
        KeyCode::F1
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
        | KeyCode::F12 => true,

        // Navigation keys are not character keys
        KeyCode::ArrowLeft
        | KeyCode::ArrowUp
        | KeyCode::ArrowRight
        | KeyCode::ArrowDown
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown => true,

        // Modifier keys are not character keys
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight
        | KeyCode::ControlLeft
        | KeyCode::ControlRight
        | KeyCode::AltLeft
        | KeyCode::AltRight
        | KeyCode::SuperLeft
        | KeyCode::SuperRight => true,

        // Lock keys are not character keys
        KeyCode::CapsLock | KeyCode::NumLock | KeyCode::ScrollLock => true,

        // Special control keys are not character keys
        KeyCode::Escape
        | KeyCode::Tab
        | KeyCode::Enter
        | KeyCode::Backspace
        | KeyCode::Delete
        | KeyCode::Insert => true,

        // All other keys (letters, numbers, punctuation, space, numpad) are character keys
        _ => false,
    }
}

fn keycode_to_windows_vk(keycode: KeyCode) -> i32 {
    match keycode {
        // Letters
        KeyCode::KeyA => 0x41,
        KeyCode::KeyB => 0x42,
        KeyCode::KeyC => 0x43,
        KeyCode::KeyD => 0x44,
        KeyCode::KeyE => 0x45,
        KeyCode::KeyF => 0x46,
        KeyCode::KeyG => 0x47,
        KeyCode::KeyH => 0x48,
        KeyCode::KeyI => 0x49,
        KeyCode::KeyJ => 0x4A,
        KeyCode::KeyK => 0x4B,
        KeyCode::KeyL => 0x4C,
        KeyCode::KeyM => 0x4D,
        KeyCode::KeyN => 0x4E,
        KeyCode::KeyO => 0x4F,
        KeyCode::KeyP => 0x50,
        KeyCode::KeyQ => 0x51,
        KeyCode::KeyR => 0x52,
        KeyCode::KeyS => 0x53,
        KeyCode::KeyT => 0x54,
        KeyCode::KeyU => 0x55,
        KeyCode::KeyV => 0x56,
        KeyCode::KeyW => 0x57,
        KeyCode::KeyX => 0x58,
        KeyCode::KeyY => 0x59,
        KeyCode::KeyZ => 0x5A,

        // Numbers
        KeyCode::Digit0 => 0x30,
        KeyCode::Digit1 => 0x31,
        KeyCode::Digit2 => 0x32,
        KeyCode::Digit3 => 0x33,
        KeyCode::Digit4 => 0x34,
        KeyCode::Digit5 => 0x35,
        KeyCode::Digit6 => 0x36,
        KeyCode::Digit7 => 0x37,
        KeyCode::Digit8 => 0x38,
        KeyCode::Digit9 => 0x39,

        // Function keys
        KeyCode::F1 => 0x70,
        KeyCode::F2 => 0x71,
        KeyCode::F3 => 0x72,
        KeyCode::F4 => 0x73,
        KeyCode::F5 => 0x74,
        KeyCode::F6 => 0x75,
        KeyCode::F7 => 0x76,
        KeyCode::F8 => 0x77,
        KeyCode::F9 => 0x78,
        KeyCode::F10 => 0x79,
        KeyCode::F11 => 0x7A,
        KeyCode::F12 => 0x7B,

        // Special keys
        KeyCode::Enter => 0x0D,
        KeyCode::Space => 0x20,
        KeyCode::Backspace => 0x08,
        KeyCode::Delete => 0x2E,
        KeyCode::Tab => 0x09,
        KeyCode::Escape => 0x1B,
        KeyCode::Insert => 0x2D,
        KeyCode::Home => 0x24,
        KeyCode::End => 0x23,
        KeyCode::PageUp => 0x21,
        KeyCode::PageDown => 0x22,

        // Arrow keys
        KeyCode::ArrowLeft => 0x25,
        KeyCode::ArrowUp => 0x26,
        KeyCode::ArrowRight => 0x27,
        KeyCode::ArrowDown => 0x28,

        // Modifier keys
        KeyCode::ShiftLeft | KeyCode::ShiftRight => 0x10,
        KeyCode::ControlLeft | KeyCode::ControlRight => 0x11,
        KeyCode::AltLeft | KeyCode::AltRight => 0x12,
        KeyCode::SuperLeft => 0x5B,  // Left Windows key
        KeyCode::SuperRight => 0x5C, // Right Windows key

        // Lock keys
        KeyCode::CapsLock => 0x14,
        KeyCode::NumLock => 0x90,
        KeyCode::ScrollLock => 0x91,

        // Punctuation
        KeyCode::Semicolon => 0xBA,
        KeyCode::Equal => 0xBB,
        KeyCode::Comma => 0xBC,
        KeyCode::Minus => 0xBD,
        KeyCode::Period => 0xBE,
        KeyCode::Slash => 0xBF,
        KeyCode::Backquote => 0xC0,
        KeyCode::BracketLeft => 0xDB,
        KeyCode::Backslash => 0xDC,
        KeyCode::BracketRight => 0xDD,
        KeyCode::Quote => 0xDE,

        // Numpad
        KeyCode::Numpad0 => 0x60,
        KeyCode::Numpad1 => 0x61,
        KeyCode::Numpad2 => 0x62,
        KeyCode::Numpad3 => 0x63,
        KeyCode::Numpad4 => 0x64,
        KeyCode::Numpad5 => 0x65,
        KeyCode::Numpad6 => 0x66,
        KeyCode::Numpad7 => 0x67,
        KeyCode::Numpad8 => 0x68,
        KeyCode::Numpad9 => 0x69,
        KeyCode::NumpadMultiply => 0x6A,
        KeyCode::NumpadAdd => 0x6B,
        KeyCode::NumpadSubtract => 0x6D,
        KeyCode::NumpadDecimal => 0x6E,
        KeyCode::NumpadDivide => 0x6F,

        // Default case for unhandled keys
        _ => 0,
    }
}

/// Returns a platform-specific native key code.
///
/// - **macOS**: Returns the Carbon virtual key code (used directly by CEF).
/// - **Windows**: Returns the Chromium-format scan code. Regular keys use the raw scan code
///   (e.g., 0x1E for KeyA). Extended keys use a 0xE0 prefix (e.g., 0xE053 for Delete).
///   CEF's `NativeKeycodeToDomCode()` uses this to derive `KeyboardEvent.code`.
fn to_native_key_code(keycode: &KeyCode) -> u32 {
    if cfg!(target_os = "macos") {
        to_macos_key_code(keycode)
    } else {
        to_windows_native_key_code(keycode)
    }
}

/// Returns the macOS Carbon virtual key code for the given key.
fn to_macos_key_code(keycode: &KeyCode) -> u32 {
    match keycode {
        // Letters
        KeyCode::KeyA => 0x00,
        KeyCode::KeyB => 0x0B,
        KeyCode::KeyC => 0x08,
        KeyCode::KeyD => 0x02,
        KeyCode::KeyE => 0x0E,
        KeyCode::KeyF => 0x03,
        KeyCode::KeyG => 0x05,
        KeyCode::KeyH => 0x04,
        KeyCode::KeyI => 0x22,
        KeyCode::KeyJ => 0x26,
        KeyCode::KeyK => 0x28,
        KeyCode::KeyL => 0x25,
        KeyCode::KeyM => 0x2E,
        KeyCode::KeyN => 0x2D,
        KeyCode::KeyO => 0x1F,
        KeyCode::KeyP => 0x23,
        KeyCode::KeyQ => 0x0C,
        KeyCode::KeyR => 0x0F,
        KeyCode::KeyS => 0x01,
        KeyCode::KeyT => 0x11,
        KeyCode::KeyU => 0x20,
        KeyCode::KeyV => 0x09,
        KeyCode::KeyW => 0x0D,
        KeyCode::KeyX => 0x07,
        KeyCode::KeyY => 0x10,
        KeyCode::KeyZ => 0x06,
        // Digits
        KeyCode::Digit0 => 0x1D,
        KeyCode::Digit1 => 0x12,
        KeyCode::Digit2 => 0x13,
        KeyCode::Digit3 => 0x14,
        KeyCode::Digit4 => 0x15,
        KeyCode::Digit5 => 0x17,
        KeyCode::Digit6 => 0x16,
        KeyCode::Digit7 => 0x1A,
        KeyCode::Digit8 => 0x1C,
        KeyCode::Digit9 => 0x19,
        // Function keys
        KeyCode::F1 => 0x7A,
        KeyCode::F2 => 0x78,
        KeyCode::F3 => 0x63,
        KeyCode::F4 => 0x76,
        KeyCode::F5 => 0x60,
        KeyCode::F6 => 0x61,
        KeyCode::F7 => 0x62,
        KeyCode::F8 => 0x64,
        KeyCode::F9 => 0x65,
        KeyCode::F10 => 0x6D,
        KeyCode::F11 => 0x67,
        KeyCode::F12 => 0x6F,
        // Special keys
        KeyCode::Enter => 0x24,
        KeyCode::Space => 0x31,
        KeyCode::Backspace => 0x33,
        KeyCode::Delete => 0x75,
        KeyCode::Tab => 0x30,
        KeyCode::Escape => 0x35,
        KeyCode::Insert => 0x72,
        KeyCode::Home => 0x73,
        KeyCode::End => 0x77,
        KeyCode::PageUp => 0x74,
        KeyCode::PageDown => 0x79,
        // Arrow keys
        KeyCode::ArrowLeft => 0x7B,
        KeyCode::ArrowUp => 0x7E,
        KeyCode::ArrowRight => 0x7C,
        KeyCode::ArrowDown => 0x7D,
        // Modifier keys
        KeyCode::ShiftLeft => 0x38,
        KeyCode::ShiftRight => 0x3C,
        KeyCode::ControlLeft => 0x3B,
        KeyCode::ControlRight => 0x3E,
        KeyCode::AltLeft => 0x3A,
        KeyCode::AltRight => 0x3D,
        KeyCode::SuperLeft => 0x37,
        KeyCode::SuperRight => 0x36,
        // Lock keys
        KeyCode::CapsLock => 0x39,
        KeyCode::NumLock => 0x47,
        KeyCode::ScrollLock => 0x6B,
        // Punctuation
        KeyCode::Semicolon => 0x29,
        KeyCode::Equal => 0x18,
        KeyCode::Comma => 0x2B,
        KeyCode::Minus => 0x1B,
        KeyCode::Period => 0x2F,
        KeyCode::Slash => 0x2C,
        KeyCode::Backquote => 0x32,
        KeyCode::BracketLeft => 0x21,
        KeyCode::Backslash => 0x2A,
        KeyCode::BracketRight => 0x1E,
        KeyCode::Quote => 0x27,
        // Numpad
        KeyCode::Numpad0 => 0x52,
        KeyCode::Numpad1 => 0x53,
        KeyCode::Numpad2 => 0x54,
        KeyCode::Numpad3 => 0x55,
        KeyCode::Numpad4 => 0x56,
        KeyCode::Numpad5 => 0x57,
        KeyCode::Numpad6 => 0x58,
        KeyCode::Numpad7 => 0x59,
        KeyCode::Numpad8 => 0x5B,
        KeyCode::Numpad9 => 0x5C,
        KeyCode::NumpadMultiply => 0x43,
        KeyCode::NumpadAdd => 0x45,
        KeyCode::NumpadSubtract => 0x4E,
        KeyCode::NumpadDecimal => 0x41,
        KeyCode::NumpadDivide => 0x4B,
        _ => 0,
    }
}

/// Returns the Chromium-format Windows scan code for the given key.
///
/// Regular keys return their raw scan code (e.g., 0x1E for KeyA).
/// Extended keys return the scan code with a 0xE0 prefix (e.g., 0xE053 for Delete).
///
/// These values match Chromium's `dom_code_data.inc` lookup table, which CEF's
/// `NativeKeycodeToDomCode()` uses to derive `KeyboardEvent.code`.
fn to_windows_native_key_code(keycode: &KeyCode) -> u32 {
    let (scan_code, extended) = match keycode {
        // Letters (row by row on US QWERTY)
        KeyCode::KeyA => (0x1E, false),
        KeyCode::KeyB => (0x30, false),
        KeyCode::KeyC => (0x2E, false),
        KeyCode::KeyD => (0x20, false),
        KeyCode::KeyE => (0x12, false),
        KeyCode::KeyF => (0x21, false),
        KeyCode::KeyG => (0x22, false),
        KeyCode::KeyH => (0x23, false),
        KeyCode::KeyI => (0x17, false),
        KeyCode::KeyJ => (0x24, false),
        KeyCode::KeyK => (0x25, false),
        KeyCode::KeyL => (0x26, false),
        KeyCode::KeyM => (0x32, false),
        KeyCode::KeyN => (0x31, false),
        KeyCode::KeyO => (0x18, false),
        KeyCode::KeyP => (0x19, false),
        KeyCode::KeyQ => (0x10, false),
        KeyCode::KeyR => (0x13, false),
        KeyCode::KeyS => (0x1F, false),
        KeyCode::KeyT => (0x14, false),
        KeyCode::KeyU => (0x16, false),
        KeyCode::KeyV => (0x2F, false),
        KeyCode::KeyW => (0x11, false),
        KeyCode::KeyX => (0x2D, false),
        KeyCode::KeyY => (0x15, false),
        KeyCode::KeyZ => (0x2C, false),
        // Digits
        KeyCode::Digit1 => (0x02, false),
        KeyCode::Digit2 => (0x03, false),
        KeyCode::Digit3 => (0x04, false),
        KeyCode::Digit4 => (0x05, false),
        KeyCode::Digit5 => (0x06, false),
        KeyCode::Digit6 => (0x07, false),
        KeyCode::Digit7 => (0x08, false),
        KeyCode::Digit8 => (0x09, false),
        KeyCode::Digit9 => (0x0A, false),
        KeyCode::Digit0 => (0x0B, false),
        // Function keys
        KeyCode::F1 => (0x3B, false),
        KeyCode::F2 => (0x3C, false),
        KeyCode::F3 => (0x3D, false),
        KeyCode::F4 => (0x3E, false),
        KeyCode::F5 => (0x3F, false),
        KeyCode::F6 => (0x40, false),
        KeyCode::F7 => (0x41, false),
        KeyCode::F8 => (0x42, false),
        KeyCode::F9 => (0x43, false),
        KeyCode::F10 => (0x44, false),
        KeyCode::F11 => (0x57, false),
        KeyCode::F12 => (0x58, false),
        // Special keys
        KeyCode::Escape => (0x01, false),
        KeyCode::Tab => (0x0F, false),
        KeyCode::CapsLock => (0x3A, false),
        KeyCode::Space => (0x39, false),
        KeyCode::Backspace => (0x0E, false),
        KeyCode::Enter => (0x1C, false),
        KeyCode::Insert => (0x52, true),
        KeyCode::Delete => (0x53, true),
        KeyCode::Home => (0x47, true),
        KeyCode::End => (0x4F, true),
        KeyCode::PageUp => (0x49, true),
        KeyCode::PageDown => (0x51, true),
        // Arrow keys (extended)
        KeyCode::ArrowLeft => (0x4B, true),
        KeyCode::ArrowUp => (0x48, true),
        KeyCode::ArrowRight => (0x4D, true),
        KeyCode::ArrowDown => (0x50, true),
        // Modifier keys
        KeyCode::ShiftLeft => (0x2A, false),
        KeyCode::ShiftRight => (0x36, false),
        KeyCode::ControlLeft => (0x1D, false),
        KeyCode::ControlRight => (0x1D, true),
        KeyCode::AltLeft => (0x38, false),
        KeyCode::AltRight => (0x38, true),
        KeyCode::SuperLeft => (0x5B, true),
        KeyCode::SuperRight => (0x5C, true),
        // Lock keys
        KeyCode::NumLock => (0x45, true),
        KeyCode::ScrollLock => (0x46, false),
        // Punctuation
        KeyCode::Minus => (0x0C, false),
        KeyCode::Equal => (0x0D, false),
        KeyCode::BracketLeft => (0x1A, false),
        KeyCode::BracketRight => (0x1B, false),
        KeyCode::Backslash => (0x2B, false),
        KeyCode::Semicolon => (0x27, false),
        KeyCode::Quote => (0x28, false),
        KeyCode::Backquote => (0x29, false),
        KeyCode::Comma => (0x33, false),
        KeyCode::Period => (0x34, false),
        KeyCode::Slash => (0x35, false),
        // Numpad
        KeyCode::Numpad0 => (0x52, false),
        KeyCode::Numpad1 => (0x4F, false),
        KeyCode::Numpad2 => (0x50, false),
        KeyCode::Numpad3 => (0x51, false),
        KeyCode::Numpad4 => (0x4B, false),
        KeyCode::Numpad5 => (0x4C, false),
        KeyCode::Numpad6 => (0x4D, false),
        KeyCode::Numpad7 => (0x47, false),
        KeyCode::Numpad8 => (0x48, false),
        KeyCode::Numpad9 => (0x49, false),
        KeyCode::NumpadMultiply => (0x37, false),
        KeyCode::NumpadAdd => (0x4E, false),
        KeyCode::NumpadSubtract => (0x4A, false),
        KeyCode::NumpadDecimal => (0x53, false),
        KeyCode::NumpadDivide => (0x35, true),
        KeyCode::NumpadEnter => (0x1C, true),
        _ => return 0,
    };
    let extended_prefix = if extended { 0xe000u32 } else { 0 };
    scan_code | extended_prefix
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::entity::Entity;
    use bevy::input::keyboard::NativeKey;
    use cef::KeyEventType;

    fn win() -> Entity {
        Entity::PLACEHOLDER
    }

    fn kb(
        key_code: KeyCode,
        logical_key: Key,
        state: ButtonState,
        text: Option<&str>,
        repeat: bool,
    ) -> KeyboardInput {
        KeyboardInput {
            key_code,
            logical_key,
            state,
            text: text.map(|s| s.into()),
            repeat,
            window: win(),
        }
    }

    #[test]
    fn letter_with_text_produces_expected_cef_sequence() {
        let input = ButtonInput::default();
        let ev = kb(
            KeyCode::KeyL,
            Key::Character("l".into()),
            ButtonState::Pressed,
            Some("l"),
            false,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 2, "expected RAWKEYDOWN + CHAR");
        assert_eq!(events[0].type_, KeyEventType::RAWKEYDOWN);
        assert_eq!(events[1].type_, KeyEventType::CHAR);
        assert_eq!(events[1].character, u16::from(b'l'));
        assert_eq!(events[1].windows_key_code, u16::from(b'l') as i32);
        assert_ne!(events[1].focus_on_editable_field, 0, "CHAR must mark editable for OSR");
    }

    /// Simulates macOS after delete-repeat: no text, logical not yet Character.
    #[test]
    fn letter_l_without_text_falls_back_to_physical_keycode_latin() {
        let input = ButtonInput::default();
        let ev = kb(
            KeyCode::KeyL,
            Key::Unidentified(NativeKey::Unidentified),
            ButtonState::Pressed,
            None,
            false,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].type_, KeyEventType::CHAR);
        assert_eq!(events[1].character, u16::from(b'l'));
        assert_eq!(events[1].windows_key_code, u16::from(b'l') as i32);
    }

    #[test]
    fn delete_press_is_rawkeydown_not_char() {
        let input = ButtonInput::default();
        let ev = kb(
            KeyCode::Delete,
            Key::Unidentified(NativeKey::Unidentified),
            ButtonState::Pressed,
            None,
            false,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].type_, KeyEventType::RAWKEYDOWN);
        assert_eq!(events[0].character, 0);
        assert_eq!(events[0].focus_on_editable_field, 0);
    }

    #[test]
    fn delete_release_is_keyup() {
        let input = ButtonInput::default();
        let ev = kb(
            KeyCode::Delete,
            Key::Unidentified(NativeKey::Unidentified),
            ButtonState::Released,
            None,
            false,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].type_, KeyEventType::KEYUP);
    }

    #[test]
    fn arrow_left_repeated_press_rawkeydown() {
        let input = ButtonInput::default();
        let ev = kb(
            KeyCode::ArrowLeft,
            Key::Unidentified(NativeKey::Unidentified),
            ButtonState::Pressed,
            None,
            true,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].type_, KeyEventType::RAWKEYDOWN);
    }

    #[test]
    fn prefers_unicode_text_when_present() {
        let input = ButtonInput::default();
        let ev = kb(
            KeyCode::KeyA,
            Key::Character("ä".into()),
            ButtonState::Pressed,
            Some("ä"),
            false,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 2);
        let char_ev = &events[1];
        assert_eq!(char_ev.type_, KeyEventType::CHAR);
        assert_eq!(char_ev.character, 0x00E4);
    }

    #[test]
    fn latin_fallback_shift_uppercase_l() {
        let mut input = ButtonInput::default();
        input.press(KeyCode::ShiftLeft);
        let ev = kb(
            KeyCode::KeyL,
            Key::Unidentified(NativeKey::Unidentified),
            ButtonState::Pressed,
            None,
            false,
        );
        let events = create_cef_key_events(0, &input, &ev);
        assert_eq!(events.len(), 2);
        let char_ev = &events[1];
        assert_eq!(char_ev.type_, KeyEventType::CHAR);
        assert_eq!(char_ev.character, u16::from(b'L'));
    }
}
