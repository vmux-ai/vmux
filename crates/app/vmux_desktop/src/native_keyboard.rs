use std::ptr::NonNull;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::BinHostEmitEvent;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
use parking_lot::Mutex;
use vmux_command::AppCommand;

use crate::shortcut::{KeyCombo, Keymap, Modifiers};

pub(crate) struct NativeKeyboardPlugin;

impl Plugin for NativeKeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            install_native_key_monitor.after(crate::shortcut::init_shortcuts),
        )
        .add_systems(
            Update,
            sync_simulator_shortcuts
                .after(vmux_layout::stack::ComputeFocusSet)
                .before(vmux_simulator::SimulatorFocusSet),
        )
        .add_systems(
            Update,
            process_monitored_keys
                .in_set(vmux_command::WriteAppCommands)
                .before(vmux_simulator::SimulatorInputSet),
        );
    }
}

static SHORTCUT_MAP: LazyLock<Mutex<Option<Keymap>>> = LazyLock::new(|| Mutex::new(None));
static PENDING_PREFIX: LazyLock<Mutex<Option<(KeyCombo, Instant)>>> =
    LazyLock::new(|| Mutex::new(None));
static PENDING_COMMANDS: LazyLock<Mutex<Vec<AppCommand>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_SIMULATOR_BUTTONS: LazyLock<Mutex<Vec<vmux_simulator::event::HardwareButton>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_SIMULATOR_CLIPBOARD: LazyLock<
    Mutex<Vec<vmux_simulator::event::SimulatorClipboardAction>>,
> = LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_SIMULATOR_KEYBOARD: AtomicUsize = AtomicUsize::new(0);
struct PendingShortcutCapture {
    token: vmux_shortcut::ShortcutCaptureToken,
    event: vmux_shortcut::ShortcutPressedEvent,
}

static PENDING_SHORTCUT_CAPTURES: LazyLock<Mutex<Vec<PendingShortcutCapture>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_SHORTCUT_RELEASES: LazyLock<Mutex<Vec<vmux_shortcut::ShortcutCaptureToken>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static WINDOW_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static EXIT_FULLSCREEN_REQUESTED: AtomicBool = AtomicBool::new(false);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static SIMULATOR_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_shortcut_map(map: Keymap) {
    *SHORTCUT_MAP.lock() = Some(map);
}

#[cfg(feature = "native-glass")]
pub(crate) fn set_window_fullscreen(value: bool) {
    WINDOW_FULLSCREEN.store(value, Ordering::Relaxed);
}

#[cfg(feature = "native-glass")]
pub(crate) fn take_exit_fullscreen_request() -> bool {
    EXIT_FULLSCREEN_REQUESTED.swap(false, Ordering::Relaxed)
}

pub(crate) fn take_quit_request() -> bool {
    QUIT_REQUESTED.swap(false, Ordering::Relaxed)
}

fn quits_the_app(combo: &KeyCombo) -> bool {
    combo.key == KeyCode::KeyQ
        && combo.modifiers.super_key
        && !combo.modifiers.ctrl
        && !combo.modifiers.alt
        && !combo.modifiers.shift
}

fn escape_exits_fullscreen(combo: &KeyCombo) -> bool {
    combo.is_bare_escape()
        && WINDOW_FULLSCREEN.load(Ordering::Relaxed)
        && !vmux_browser::native_page_owns_escape()
}

fn simulator_button(combo: &KeyCombo) -> Option<vmux_simulator::event::HardwareButton> {
    if !combo.modifiers.super_key
        || combo.modifiers.ctrl
        || combo.modifiers.alt
        || combo.modifiers.shift
    {
        return None;
    }
    match combo.key {
        KeyCode::KeyH => Some(vmux_simulator::event::HardwareButton::Home),
        KeyCode::KeyL => Some(vmux_simulator::event::HardwareButton::Lock),
        KeyCode::KeyS => Some(vmux_simulator::event::HardwareButton::Siri),
        _ => None,
    }
}

fn simulator_clipboard(
    combo: &KeyCombo,
) -> Option<vmux_simulator::event::SimulatorClipboardAction> {
    if !combo.modifiers.super_key
        || combo.modifiers.ctrl
        || combo.modifiers.alt
        || combo.modifiers.shift
    {
        return None;
    }
    match combo.key {
        KeyCode::KeyC => Some(vmux_simulator::event::SimulatorClipboardAction::Copy),
        KeyCode::KeyV => Some(vmux_simulator::event::SimulatorClipboardAction::Paste),
        _ => None,
    }
}

fn toggles_simulator_software_keyboard(combo: &KeyCombo) -> bool {
    combo.key == KeyCode::KeyK
        && combo.modifiers.super_key
        && !combo.modifiers.ctrl
        && !combo.modifiers.alt
        && !combo.modifiers.shift
}

enum KeyAction {
    Consume(Option<AppCommand>),
    PassThrough,
}

fn decide(
    map: &Keymap,
    pending: &mut Option<(KeyCombo, Instant)>,
    combo: KeyCombo,
    now: Instant,
    text_entry_owns_keys: bool,
) -> KeyAction {
    if let Some((_, started)) = pending.as_ref()
        && now.duration_since(*started) > Duration::from_millis(map.chord_timeout_ms)
    {
        *pending = None;
    }

    if let Some((prefix, _)) = pending.clone() {
        if let Some(cmd) = map.chord(&prefix, &combo) {
            *pending = None;
            return KeyAction::Consume(Some(cmd));
        }
        *pending = None;
    }

    if let Some(cmd) = map.direct(&combo) {
        if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key {
            return KeyAction::Consume(Some(cmd));
        }
        return KeyAction::PassThrough;
    }

    if !text_entry_owns_keys && map.has_chord_prefix(&combo) {
        *pending = Some((combo, now));
        return KeyAction::Consume(None);
    }

    KeyAction::PassThrough
}

fn classify(combo: KeyCombo) -> KeyAction {
    if SIMULATOR_ACTIVE.load(Ordering::Relaxed) && toggles_simulator_software_keyboard(&combo) {
        PENDING_SIMULATOR_KEYBOARD.fetch_add(1, Ordering::Relaxed);
        return KeyAction::Consume(None);
    }
    if SIMULATOR_ACTIVE.load(Ordering::Relaxed)
        && let Some(action) = simulator_clipboard(&combo)
    {
        PENDING_SIMULATOR_CLIPBOARD.lock().push(action);
        return KeyAction::Consume(None);
    }
    if SIMULATOR_ACTIVE.load(Ordering::Relaxed)
        && let Some(button) = simulator_button(&combo)
    {
        PENDING_SIMULATOR_BUTTONS.lock().push(button);
        return KeyAction::Consume(None);
    }
    if escape_exits_fullscreen(&combo) {
        EXIT_FULLSCREEN_REQUESTED.store(true, Ordering::Relaxed);
        return KeyAction::Consume(None);
    }
    if quits_the_app(&combo) {
        QUIT_REQUESTED.store(true, Ordering::Relaxed);
        return KeyAction::Consume(None);
    }
    let guard = SHORTCUT_MAP.lock();
    let Some(map) = guard.as_ref() else {
        return KeyAction::PassThrough;
    };
    let mut pending = PENDING_PREFIX.lock();
    decide(
        map,
        &mut pending,
        combo,
        Instant::now(),
        vmux_browser::native_text_entry_owns_keys(),
    )
}

fn handle_key_action(
    action: KeyAction,
    wake: impl FnOnce(),
    mut queue: impl FnMut(AppCommand),
) -> bool {
    match action {
        KeyAction::Consume(cmd) => {
            wake();
            if let Some(cmd) = cmd {
                queue(cmd);
            }
            true
        }
        KeyAction::PassThrough => false,
    }
}

fn translate(key_code: u16, flags: NSEventModifierFlags) -> Option<KeyCombo> {
    let key = key_code_from_vk(key_code)?;
    Some(KeyCombo {
        key,
        modifiers: modifiers(flags),
    })
}

fn modifiers(flags: NSEventModifierFlags) -> Modifiers {
    Modifiers {
        ctrl: flags.contains(NSEventModifierFlags::Control),
        shift: flags.contains(NSEventModifierFlags::Shift),
        alt: flags.contains(NSEventModifierFlags::Option),
        super_key: flags.contains(NSEventModifierFlags::Command),
    }
}

fn capture_stroke(
    event: &NSEvent,
    key_code: u16,
    flags: NSEventModifierFlags,
) -> vmux_shortcut::ShortcutStroke {
    if let Some(combo) = translate(key_code, flags) {
        return vmux_shortcut::ShortcutStroke {
            code: combo.code(),
            label: combo.key_label(),
            ctrl: combo.modifiers.ctrl,
            shift: combo.modifiers.shift,
            alt: combo.modifiers.alt,
            super_key: combo.modifiers.super_key,
        };
    }
    let modifiers = modifiers(flags);
    let raw = event
        .charactersIgnoringModifiers()
        .map(|characters| characters.to_string())
        .unwrap_or_default();
    let resolved = vmux_command::shortcut::resolve_key(&raw).map(|resolved| KeyCombo {
        key: resolved.key,
        modifiers,
    });
    let code = resolved
        .as_ref()
        .map(KeyCombo::code)
        .unwrap_or_else(|| format!("NativeKeyCode{key_code}"));
    let label = resolved
        .as_ref()
        .map(KeyCombo::key_label)
        .filter(|label| !label.is_empty())
        .or_else(|| (!raw.is_empty()).then(|| raw.to_uppercase()))
        .unwrap_or_else(|| format!("0x{key_code:02X}"));
    vmux_shortcut::ShortcutStroke {
        code,
        label,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        super_key: modifiers.super_key,
    }
}

fn releases_shortcut_capture(stroke: &vmux_shortcut::ShortcutStroke) -> bool {
    stroke.code == "Tab" && !stroke.ctrl && !stroke.alt && !stroke.super_key
}

fn key_code_from_vk(vk: u16) -> Option<KeyCode> {
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
        0x0A => KeyCode::IntlBackslash,
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
        0x40 => KeyCode::F17,
        0x41 => KeyCode::NumpadDecimal,
        0x43 => KeyCode::NumpadMultiply,
        0x45 => KeyCode::NumpadAdd,
        0x47 => KeyCode::NumpadClear,
        0x4B => KeyCode::NumpadDivide,
        0x4C => KeyCode::NumpadEnter,
        0x4E => KeyCode::NumpadSubtract,
        0x4F => KeyCode::F18,
        0x50 => KeyCode::F19,
        0x51 => KeyCode::NumpadEqual,
        0x52 => KeyCode::Numpad0,
        0x53 => KeyCode::Numpad1,
        0x54 => KeyCode::Numpad2,
        0x55 => KeyCode::Numpad3,
        0x56 => KeyCode::Numpad4,
        0x57 => KeyCode::Numpad5,
        0x58 => KeyCode::Numpad6,
        0x59 => KeyCode::Numpad7,
        0x5A => KeyCode::F20,
        0x5B => KeyCode::Numpad8,
        0x5C => KeyCode::Numpad9,
        0x5D => KeyCode::IntlYen,
        0x5E => KeyCode::IntlRo,
        0x5F => KeyCode::NumpadComma,
        0x60 => KeyCode::F5,
        0x61 => KeyCode::F6,
        0x62 => KeyCode::F7,
        0x63 => KeyCode::F3,
        0x64 => KeyCode::F8,
        0x65 => KeyCode::F9,
        0x66 => KeyCode::Lang2,
        0x67 => KeyCode::F11,
        0x68 => KeyCode::Lang1,
        0x69 => KeyCode::F13,
        0x6A => KeyCode::F16,
        0x6B => KeyCode::F14,
        0x6D => KeyCode::F10,
        0x6F => KeyCode::F12,
        0x71 => KeyCode::F15,
        0x72 => KeyCode::Insert,
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
        wake();
        if ev.r#type() != NSEventType::KeyDown {
            return event.as_ptr();
        }
        let key_code = ev.keyCode();
        let flags = ev.modifierFlags();
        if let Some(token) = vmux_shortcut::capture_target() {
            if ev.isARepeat() {
                return std::ptr::null_mut();
            }
            let stroke = capture_stroke(ev, key_code, flags);
            if releases_shortcut_capture(&stroke) {
                if vmux_shortcut::release_capture(token) {
                    PENDING_SHORTCUT_RELEASES.lock().push(token);
                }
                return event.as_ptr();
            }
            *PENDING_PREFIX.lock() = None;
            PENDING_SHORTCUT_CAPTURES
                .lock()
                .push(PendingShortcutCapture {
                    token,
                    event: vmux_shortcut::ShortcutPressedEvent {
                        stroke,
                        pressed_at_ms: vmux_core::now_millis(),
                    },
                });
            return std::ptr::null_mut();
        }
        let Some(combo) = translate(key_code, flags) else {
            return event.as_ptr();
        };
        if handle_key_action(classify(combo), &wake, |cmd| {
            PENDING_COMMANDS.lock().push(cmd);
        }) {
            std::ptr::null_mut()
        } else {
            event.as_ptr()
        }
    });
    let mask = NSEventMask::KeyDown | NSEventMask::KeyUp | NSEventMask::FlagsChanged;
    let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    if let Some(token) = token {
        std::mem::forget(token);
    }
}

fn install_native_key_monitor(proxy: Option<Res<EventLoopProxyWrapper>>) {
    let Some(proxy) = proxy else {
        return;
    };
    let proxy = (**proxy).clone();
    install(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
}

fn sync_simulator_shortcuts(
    focus: Option<Res<vmux_layout::stack::FocusedStack>>,
    children: Query<&Children>,
    pages: Query<&vmux_core::PageMetadata>,
    mut requests: Option<ResMut<Messages<vmux_simulator::SimulatorFocusRequest>>>,
) {
    let active = focus
        .as_deref()
        .and_then(|focus| focus.stack)
        .and_then(|stack| children.get(stack).ok())
        .and_then(|children| {
            children.iter().find(|entity| {
                pages.get(*entity).is_ok_and(|metadata| {
                    vmux_simulator::url::SimulatorRoute::of_url(&metadata.url).is_some()
                })
            })
        });
    SIMULATOR_ACTIVE.store(active.is_some(), Ordering::Relaxed);
    if let Some(requests) = requests.as_mut() {
        requests.write(vmux_simulator::SimulatorFocusRequest(active));
    }
}

fn process_monitored_keys(
    mut issuer: vmux_command::CommandIssuer,
    mut simulator_buttons: Option<ResMut<Messages<vmux_simulator::HardwareButtonRequest>>>,
    mut simulator_clipboard: Option<ResMut<Messages<vmux_simulator::SimulatorClipboardRequest>>>,
    mut simulator_keyboard: Option<
        ResMut<Messages<vmux_simulator::SimulatorSoftwareKeyboardRequest>>,
    >,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut shortcut_capture: Option<ResMut<vmux_shortcut::ShortcutCaptureTarget>>,
    mut ecs: Commands,
) {
    let commands = {
        let mut queue = PENDING_COMMANDS.lock();
        std::mem::take(&mut *queue)
    };
    let buttons = {
        let mut queue = PENDING_SIMULATOR_BUTTONS.lock();
        std::mem::take(&mut *queue)
    };
    let clipboard = {
        let mut queue = PENDING_SIMULATOR_CLIPBOARD.lock();
        std::mem::take(&mut *queue)
    };
    let toggle_keyboard = PENDING_SIMULATOR_KEYBOARD.swap(0, Ordering::Relaxed);
    let shortcut_captures = {
        let mut queue = PENDING_SHORTCUT_CAPTURES.lock();
        std::mem::take(&mut *queue)
    };
    let shortcut_releases = {
        let mut queue = PENDING_SHORTCUT_RELEASES.lock();
        std::mem::take(&mut *queue)
    };
    if commands.is_empty()
        && buttons.is_empty()
        && clipboard.is_empty()
        && toggle_keyboard == 0
        && shortcut_captures.is_empty()
        && shortcut_releases.is_empty()
    {
        return;
    }
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    for cmd in commands {
        issuer.issue(caller, cmd);
    }
    if let Some(target) = shortcut_capture.as_deref_mut() {
        for token in shortcut_releases {
            target.release(token, &mut ecs);
        }
    }
    if let Some(token) = shortcut_capture
        .as_deref()
        .and_then(|target| target.token())
    {
        for capture in shortcut_captures {
            if capture.token != token {
                continue;
            }
            ecs.trigger(BinHostEmitEvent::from_rkyv(
                token.target,
                vmux_shortcut::PRESSED_EVENT,
                &capture.event,
            ));
        }
    }
    if let Some(simulator_buttons) = simulator_buttons.as_mut() {
        for button in buttons {
            simulator_buttons.write(vmux_simulator::HardwareButtonRequest { view: None, button });
        }
    }
    if let Some(simulator_clipboard) = simulator_clipboard.as_mut() {
        for action in clipboard {
            simulator_clipboard
                .write(vmux_simulator::SimulatorClipboardRequest { view: None, action });
        }
    }
    if let Some(simulator_keyboard) = simulator_keyboard.as_mut() {
        for _ in 0..toggle_keyboard {
            simulator_keyboard
                .write(vmux_simulator::SimulatorSoftwareKeyboardRequest { view: None });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_command::{AppCommand, LayoutCommand, PaneCommand};

    fn map() -> Keymap {
        Keymap::defaults()
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
    fn a_field_being_typed_into_keeps_the_leader_key() {
        let map = map();
        let mut pending = None;

        let action = decide(
            &map,
            &mut pending,
            combo(KeyCode::KeyG, true),
            Instant::now(),
            true,
        );

        assert!(matches!(action, KeyAction::PassThrough));
        assert!(pending.is_none(), "no chord may be left open");
    }

    #[test]
    fn leader_then_h_consumes_and_emits_select_left() {
        let map = map();
        let mut pending = None;
        let now = Instant::now();

        let prefix = decide(&map, &mut pending, combo(KeyCode::KeyG, true), now, false);
        assert!(matches!(prefix, KeyAction::Consume(None)));
        assert!(pending.is_some());

        let second = decide(&map, &mut pending, combo(KeyCode::KeyH, false), now, false);
        match second {
            KeyAction::Consume(Some(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft,
            )))) => {}
            _ => panic!("expected SelectLeft"),
        }
        assert!(pending.is_none());
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
            false,
        );
        assert!(matches!(action, KeyAction::PassThrough));
    }

    #[test]
    fn consumed_shortcut_wakes_and_queues_command() {
        let mut woke = false;
        let mut queued = Vec::new();

        let consumed = handle_key_action(
            KeyAction::Consume(Some(AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft,
            )))),
            || woke = true,
            |command| queued.push(command),
        );

        assert!(consumed);
        assert!(woke);
        assert!(matches!(
            queued.as_slice(),
            [AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft
            ))]
        ));
    }

    #[test]
    fn expired_prefix_does_not_consume_second_key() {
        let map = map();
        let mut pending = Some((combo(KeyCode::KeyG, true), Instant::now()));
        let later = Instant::now() + Duration::from_millis(2000);
        let action = decide(
            &map,
            &mut pending,
            combo(KeyCode::KeyH, false),
            later,
            false,
        );
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
            let action = decide(&map, &mut pending, pressed, now, false);
            match action {
                KeyAction::Consume(Some(AppCommand::Browser(BrowserCommand::Bar(cmd)))) => {
                    assert_eq!(cmd, expected);
                }
                _ => panic!("expected command bar shortcut"),
            }
        }
    }

    #[test]
    fn simulator_shortcuts_map_command_h_l_and_s_to_hardware_buttons() {
        use vmux_simulator::event::HardwareButton;

        assert_eq!(
            simulator_button(&super_combo(KeyCode::KeyH)),
            Some(HardwareButton::Home)
        );
        assert_eq!(
            simulator_button(&super_combo(KeyCode::KeyL)),
            Some(HardwareButton::Lock)
        );
        assert_eq!(
            simulator_button(&super_combo(KeyCode::KeyS)),
            Some(HardwareButton::Siri)
        );
    }

    #[test]
    fn simulator_shortcuts_require_command_without_other_modifiers() {
        let mut shifted = super_combo(KeyCode::KeyH);
        shifted.modifiers.shift = true;

        assert_eq!(simulator_button(&shifted), None);
        assert_eq!(simulator_button(&combo(KeyCode::KeyH, false)), None);
    }

    #[test]
    fn simulator_clipboard_shortcuts_map_command_c_and_v() {
        use vmux_simulator::event::SimulatorClipboardAction;

        assert_eq!(
            simulator_clipboard(&super_combo(KeyCode::KeyC)),
            Some(SimulatorClipboardAction::Copy)
        );
        assert_eq!(
            simulator_clipboard(&super_combo(KeyCode::KeyV)),
            Some(SimulatorClipboardAction::Paste)
        );
    }

    #[test]
    fn simulator_software_keyboard_shortcut_is_command_k() {
        assert!(toggles_simulator_software_keyboard(&super_combo(
            KeyCode::KeyK
        )));
        assert!(!toggles_simulator_software_keyboard(&combo(
            KeyCode::KeyK,
            false
        )));
        assert!(!toggles_simulator_software_keyboard(&super_combo(
            KeyCode::KeyH
        )));
    }

    #[test]
    fn native_key_map_covers_insert_and_international_keys() {
        assert_eq!(key_code_from_vk(0x72), Some(KeyCode::Insert));
        assert_eq!(key_code_from_vk(0x0A), Some(KeyCode::IntlBackslash));
        assert_eq!(key_code_from_vk(0x5D), Some(KeyCode::IntlYen));
        assert_eq!(key_code_from_vk(0x5E), Some(KeyCode::IntlRo));
    }

    #[test]
    fn bare_tab_releases_shortcut_capture() {
        let stroke = vmux_shortcut::ShortcutStroke {
            code: "Tab".into(),
            label: "⇥".into(),
            ..default()
        };
        let mut modified = stroke.clone();
        modified.ctrl = true;

        assert!(releases_shortcut_capture(&stroke));
        assert!(!releases_shortcut_capture(&modified));
    }
}
