use std::ptr::NonNull;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
use parking_lot::Mutex;
use vmux_command::AppCommand;

use crate::shortcut::{
    KeyCombo, Modifiers, ShortcutMap, chord_command, direct_command, has_chord_prefix,
};

static SHORTCUT_MAP: LazyLock<Mutex<Option<ShortcutMap>>> = LazyLock::new(|| Mutex::new(None));
static PENDING_PREFIX: LazyLock<Mutex<Option<(KeyCombo, Instant)>>> =
    LazyLock::new(|| Mutex::new(None));
static PENDING_COMMANDS: LazyLock<Mutex<Vec<AppCommand>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static ESC_EXITS_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static EXIT_FULLSCREEN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_shortcut_map(map: ShortcutMap) {
    *SHORTCUT_MAP.lock() = Some(map);
}

#[cfg(feature = "native-glass")]
pub(crate) fn set_escape_exits_fullscreen(value: bool) {
    ESC_EXITS_FULLSCREEN.store(value, Ordering::Relaxed);
}

#[cfg(feature = "native-glass")]
pub(crate) fn take_exit_fullscreen_request() -> bool {
    EXIT_FULLSCREEN_REQUESTED.swap(false, Ordering::Relaxed)
}

fn is_bare_escape(combo: &KeyCombo) -> bool {
    combo.key == KeyCode::Escape
        && !combo.modifiers.ctrl
        && !combo.modifiers.alt
        && !combo.modifiers.super_key
}

pub(crate) enum KeyAction {
    Consume(Option<AppCommand>),
    PassThrough,
}

pub(crate) fn decide(
    map: &ShortcutMap,
    pending: &mut Option<(KeyCombo, Instant)>,
    combo: KeyCombo,
    now: Instant,
) -> KeyAction {
    if let Some((_, started)) = pending.as_ref()
        && now.duration_since(*started) > Duration::from_millis(map.chord_timeout_ms)
    {
        *pending = None;
    }

    if let Some((prefix, _)) = pending.clone() {
        if let Some(cmd) = chord_command(map, &prefix, &combo) {
            *pending = None;
            return KeyAction::Consume(Some(cmd));
        }
        *pending = None;
    }

    if let Some(cmd) = direct_command(map, &combo) {
        if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key {
            return KeyAction::Consume(Some(cmd));
        }
        return KeyAction::PassThrough;
    }

    if has_chord_prefix(map, &combo) {
        *pending = Some((combo, now));
        return KeyAction::Consume(None);
    }

    KeyAction::PassThrough
}

pub(crate) fn classify(combo: KeyCombo) -> KeyAction {
    if is_bare_escape(&combo) && ESC_EXITS_FULLSCREEN.load(Ordering::Relaxed) {
        EXIT_FULLSCREEN_REQUESTED.store(true, Ordering::Relaxed);
        return KeyAction::Consume(None);
    }
    let guard = SHORTCUT_MAP.lock();
    let Some(map) = guard.as_ref() else {
        return KeyAction::PassThrough;
    };
    let mut pending = PENDING_PREFIX.lock();
    decide(map, &mut pending, combo, Instant::now())
}

pub(crate) fn push_command(cmd: AppCommand) {
    PENDING_COMMANDS.lock().push(cmd);
}

fn translate(key_code: u16, flags: NSEventModifierFlags) -> Option<KeyCombo> {
    let key = key_code_from_vk(key_code)?;
    let modifiers = Modifiers {
        ctrl: flags.contains(NSEventModifierFlags::Control),
        shift: flags.contains(NSEventModifierFlags::Shift),
        alt: flags.contains(NSEventModifierFlags::Option),
        super_key: flags.contains(NSEventModifierFlags::Command),
    };
    Some(KeyCombo { key, modifiers })
}

pub(crate) fn key_code_from_vk(vk: u16) -> Option<KeyCode> {
    let key = match vk {
        0x00 => KeyCode::KeyA,
        0x01 => KeyCode::KeyS,
        0x02 => KeyCode::KeyD,
        0x03 => KeyCode::KeyF,
        0x04 => KeyCode::KeyH,
        0x05 => KeyCode::KeyG,
        0x06 => KeyCode::KeyZ,
        0x07 => KeyCode::KeyX,
        0x08 => KeyCode::KeyC,
        0x09 => KeyCode::KeyV,
        0x0B => KeyCode::KeyB,
        0x0C => KeyCode::KeyQ,
        0x0D => KeyCode::KeyW,
        0x0E => KeyCode::KeyE,
        0x0F => KeyCode::KeyR,
        0x10 => KeyCode::KeyY,
        0x11 => KeyCode::KeyT,
        0x12 => KeyCode::Digit1,
        0x13 => KeyCode::Digit2,
        0x14 => KeyCode::Digit3,
        0x15 => KeyCode::Digit4,
        0x16 => KeyCode::Digit6,
        0x17 => KeyCode::Digit5,
        0x18 => KeyCode::Equal,
        0x19 => KeyCode::Digit9,
        0x1A => KeyCode::Digit7,
        0x1B => KeyCode::Minus,
        0x1C => KeyCode::Digit8,
        0x1D => KeyCode::Digit0,
        0x1E => KeyCode::BracketRight,
        0x1F => KeyCode::KeyO,
        0x20 => KeyCode::KeyU,
        0x21 => KeyCode::BracketLeft,
        0x22 => KeyCode::KeyI,
        0x23 => KeyCode::KeyP,
        0x24 => KeyCode::Enter,
        0x25 => KeyCode::KeyL,
        0x26 => KeyCode::KeyJ,
        0x27 => KeyCode::Quote,
        0x28 => KeyCode::KeyK,
        0x29 => KeyCode::Semicolon,
        0x2A => KeyCode::Backslash,
        0x2B => KeyCode::Comma,
        0x2C => KeyCode::Slash,
        0x2D => KeyCode::KeyN,
        0x2E => KeyCode::KeyM,
        0x2F => KeyCode::Period,
        0x30 => KeyCode::Tab,
        0x31 => KeyCode::Space,
        0x32 => KeyCode::Backquote,
        0x33 => KeyCode::Backspace,
        0x35 => KeyCode::Escape,
        0x60 => KeyCode::F5,
        0x61 => KeyCode::F6,
        0x62 => KeyCode::F7,
        0x63 => KeyCode::F3,
        0x64 => KeyCode::F8,
        0x65 => KeyCode::F9,
        0x67 => KeyCode::F11,
        0x6D => KeyCode::F10,
        0x6F => KeyCode::F12,
        0x73 => KeyCode::Home,
        0x74 => KeyCode::PageUp,
        0x75 => KeyCode::Delete,
        0x76 => KeyCode::F4,
        0x77 => KeyCode::End,
        0x78 => KeyCode::F2,
        0x79 => KeyCode::PageDown,
        0x7A => KeyCode::F1,
        0x7B => KeyCode::ArrowLeft,
        0x7C => KeyCode::ArrowRight,
        0x7D => KeyCode::ArrowDown,
        0x7E => KeyCode::ArrowUp,
        _ => return None,
    };
    Some(key)
}

fn install(wake: impl Fn() + Send + Sync + 'static) {
    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { event.as_ref() };
        if ev.r#type() != NSEventType::KeyDown {
            return event.as_ptr();
        }
        let key_code = ev.keyCode();
        let flags = ev.modifierFlags();
        let Some(combo) = translate(key_code, flags) else {
            return event.as_ptr();
        };
        match classify(combo) {
            KeyAction::Consume(cmd) => {
                wake();
                if let Some(cmd) = cmd {
                    PENDING_COMMANDS.lock().push(cmd);
                }
                std::ptr::null_mut()
            }
            KeyAction::PassThrough => event.as_ptr(),
        }
    });
    let mask = NSEventMask::KeyDown | NSEventMask::KeyUp | NSEventMask::FlagsChanged;
    let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    if let Some(token) = token {
        std::mem::forget(token);
    }
}

pub(crate) fn install_native_key_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
    let Some(proxy) = proxy else {
        return;
    };
    let proxy = (**proxy).clone();
    install(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
}

pub(crate) fn process_monitored_keys(
    mut issuer: vmux_command::CommandIssuer,
    user: Query<Entity, With<vmux_core::team::User>>,
) {
    let drained = {
        let mut queue = PENDING_COMMANDS.lock();
        if queue.is_empty() {
            return;
        }
        std::mem::take(&mut *queue)
    };
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    for cmd in drained {
        issuer.issue(caller, cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::{AppCommand, LayoutCommand, PaneCommand};

    fn map() -> ShortcutMap {
        ShortcutMap {
            bindings: AppCommand::default_shortcuts(),
            chord_timeout_ms: 1000,
        }
    }

    fn combo(key: KeyCode, ctrl: bool) -> KeyCombo {
        KeyCombo {
            key,
            modifiers: Modifiers {
                ctrl,
                ..Default::default()
            },
        }
    }

    fn super_combo(key: KeyCode) -> KeyCombo {
        KeyCombo {
            key,
            modifiers: Modifiers {
                super_key: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn leader_then_h_consumes_and_emits_select_left() {
        let map = map();
        let mut pending = None;
        let now = Instant::now();

        let prefix = decide(&map, &mut pending, combo(KeyCode::KeyG, true), now);
        assert!(matches!(prefix, KeyAction::Consume(None)));
        assert!(pending.is_some());

        let second = decide(&map, &mut pending, combo(KeyCode::KeyH, false), now);
        match second {
            KeyAction::Consume(Some(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft,
            )))) => {}
            _ => panic!("expected SelectLeft"),
        }
        assert!(pending.is_none());
    }

    #[test]
    fn bare_escape_detected_only_without_modifiers() {
        assert!(is_bare_escape(&combo(KeyCode::Escape, false)));
        assert!(!is_bare_escape(&combo(KeyCode::Escape, true)));
        assert!(!is_bare_escape(&super_combo(KeyCode::Escape)));
        assert!(!is_bare_escape(&combo(KeyCode::KeyH, false)));
    }

    #[test]
    fn bare_key_without_pending_passes_through() {
        let map = map();
        let mut pending = None;
        let action = decide(
            &map,
            &mut pending,
            combo(KeyCode::KeyH, false),
            Instant::now(),
        );
        assert!(matches!(action, KeyAction::PassThrough));
    }

    #[test]
    fn expired_prefix_does_not_consume_second_key() {
        let map = map();
        let mut pending = Some((combo(KeyCode::KeyG, true), Instant::now()));
        let later = Instant::now() + Duration::from_millis(2000);
        let action = decide(&map, &mut pending, combo(KeyCode::KeyH, false), later);
        assert!(matches!(action, KeyAction::PassThrough));
        assert!(pending.is_none());
    }

    #[test]
    fn native_command_bar_shortcuts_are_consumed_before_cef() {
        use vmux_command::{BrowserBarCommand, BrowserCommand};

        let map = map();
        let mut pending = None;
        let now = Instant::now();
        let shortcuts = [
            (
                super_combo(KeyCode::KeyK),
                BrowserBarCommand::OpenCommandBar,
            ),
            (
                super_combo(KeyCode::KeyL),
                BrowserBarCommand::OpenPageInCommandBar,
            ),
            (super_combo(KeyCode::Slash), BrowserBarCommand::OpenPathBar),
        ];

        for (pressed, expected) in shortcuts {
            let action = decide(&map, &mut pending, pressed, now);
            match action {
                KeyAction::Consume(Some(AppCommand::Browser(BrowserCommand::Bar(cmd)))) => {
                    assert_eq!(cmd, expected);
                }
                _ => panic!("expected command bar shortcut"),
            }
        }
    }
}
