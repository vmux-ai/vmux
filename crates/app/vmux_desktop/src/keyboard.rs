use std::ptr::NonNull;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
use parking_lot::Mutex;

use crate::shortcut::{KeyCombo, Keymap, Modifiers};

pub(crate) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(KeyboardRuntime::default());
        app.add_message::<ExitFullscreenRequest>()
            .add_systems(
                Startup,
                install_monitor.after(crate::shortcut::ShortcutInit),
            )
            .add_systems(
                Update,
                sync_keyboard_context
                    .after(vmux_layout::stack::ComputeFocusSet)
                    .after(vmux_browser::KeyboardContextSet)
                    .after(vmux_shortcut::ShortcutCaptureSet)
                    .before(vmux_simulator::SimulatorFocusSet),
            )
            .add_systems(
                Update,
                dispatch_keyboard_input
                    .after(sync_keyboard_context)
                    .in_set(vmux_command::WriteCommandRequests)
                    .before(vmux_simulator::SimulatorInputSet),
            );
    }
}

#[derive(Message, Clone, Copy)]
pub(crate) struct ExitFullscreenRequest;

#[derive(Component, Clone, Default)]
pub(crate) struct KeyboardRuntime(Arc<Mutex<KeyboardState>>);

#[derive(Default)]
struct KeyboardState {
    keymap: Option<Keymap>,
    pending_prefix: Option<(KeyCombo, Instant)>,
    capture_target: Option<vmux_shortcut::ShortcutCaptureToken>,
    simulator_active: bool,
    window_fullscreen: bool,
    page_owns_escape: bool,
    text_entry_owns_keys: bool,
    pending: PendingKeyboardInput,
}

#[derive(Default)]
struct PendingKeyboardInput {
    commands: Vec<String>,
    simulator_buttons: Vec<vmux_simulator::event::HardwareButton>,
    simulator_clipboard: Vec<vmux_simulator::event::SimulatorClipboardOperation>,
    simulator_keyboard: usize,
    shortcut_captures: Vec<PendingShortcutCapture>,
    shortcut_releases: Vec<vmux_shortcut::ShortcutCaptureToken>,
    exit_fullscreen: bool,
    quit: bool,
}

struct PendingShortcutCapture {
    token: vmux_shortcut::ShortcutCaptureToken,
    stroke: vmux_shortcut::ShortcutStroke,
    pressed_at_ms: i64,
}

impl KeyboardRuntime {
    #[cfg(feature = "native-glass")]
    pub(crate) fn set_window_fullscreen(&self, value: bool) {
        self.0.lock().window_fullscreen = value;
    }

    fn drain(&self) -> PendingKeyboardInput {
        std::mem::take(&mut self.0.lock().pending)
    }
}

fn quits_the_app(combo: &KeyCombo) -> bool {
    combo.key == KeyCode::KeyQ
        && combo.modifiers.super_key
        && !combo.modifiers.ctrl
        && !combo.modifiers.alt
        && !combo.modifiers.shift
}

fn escape_exits_fullscreen(combo: &KeyCombo, fullscreen: bool, page_owns_escape: bool) -> bool {
    combo.is_bare_escape() && fullscreen && !page_owns_escape
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
) -> Option<vmux_simulator::event::SimulatorClipboardOperation> {
    if !combo.modifiers.super_key
        || combo.modifiers.ctrl
        || combo.modifiers.alt
        || combo.modifiers.shift
    {
        return None;
    }
    match combo.key {
        KeyCode::KeyA => Some(vmux_simulator::event::SimulatorClipboardOperation::SelectAll),
        KeyCode::KeyC => Some(vmux_simulator::event::SimulatorClipboardOperation::Copy),
        KeyCode::KeyX => Some(vmux_simulator::event::SimulatorClipboardOperation::Cut),
        KeyCode::KeyV => Some(vmux_simulator::event::SimulatorClipboardOperation::Paste),
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

fn simulator_text_shortcut(combo: &KeyCombo) -> bool {
    let modifiers = combo.modifiers;
    if modifiers.ctrl {
        return false;
    }
    if modifiers.super_key && !modifiers.alt {
        return matches!(
            combo.key,
            KeyCode::KeyZ
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
                | KeyCode::Backspace
                | KeyCode::Delete
        );
    }
    if modifiers.alt && !modifiers.super_key {
        return matches!(
            combo.key,
            KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
                | KeyCode::Backspace
                | KeyCode::Delete
        );
    }
    false
}

enum KeyDisposition {
    Consume(Option<String>),
    PassThrough,
}

fn decide(
    map: &Keymap,
    pending: &mut Option<(KeyCombo, Instant)>,
    combo: KeyCombo,
    now: Instant,
    text_entry_owns_keys: bool,
) -> KeyDisposition {
    if let Some((_, started)) = pending.as_ref()
        && now.duration_since(*started) > Duration::from_millis(map.chord_timeout_ms)
    {
        *pending = None;
    }

    if let Some((prefix, _)) = pending.clone() {
        if let Some(cmd) = map.chord(&prefix, &combo) {
            *pending = None;
            return KeyDisposition::Consume(Some(cmd));
        }
        *pending = None;
    }

    if let Some(cmd) = map.direct(&combo) {
        if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key {
            return KeyDisposition::Consume(Some(cmd));
        }
        return KeyDisposition::PassThrough;
    }

    if !text_entry_owns_keys && map.has_chord_prefix(&combo) {
        *pending = Some((combo, now));
        return KeyDisposition::Consume(None);
    }

    KeyDisposition::PassThrough
}

impl KeyboardState {
    fn classify(&mut self, combo: KeyCombo) -> KeyDisposition {
        if self.simulator_active && toggles_simulator_software_keyboard(&combo) {
            self.pending.simulator_keyboard += 1;
            return KeyDisposition::Consume(None);
        }
        if self.simulator_active
            && let Some(operation) = simulator_clipboard(&combo)
        {
            self.pending.simulator_clipboard.push(operation);
            return KeyDisposition::Consume(None);
        }
        if self.simulator_active
            && let Some(button) = simulator_button(&combo)
        {
            self.pending.simulator_buttons.push(button);
            return KeyDisposition::Consume(None);
        }
        if self.simulator_active && simulator_text_shortcut(&combo) {
            return KeyDisposition::PassThrough;
        }
        if escape_exits_fullscreen(&combo, self.window_fullscreen, self.page_owns_escape) {
            self.pending.exit_fullscreen = true;
            return KeyDisposition::Consume(None);
        }
        if quits_the_app(&combo) {
            self.pending.quit = true;
            return KeyDisposition::Consume(None);
        }
        let Some(map) = self.keymap.as_ref() else {
            return KeyDisposition::PassThrough;
        };
        decide(
            map,
            &mut self.pending_prefix,
            combo,
            Instant::now(),
            self.text_entry_owns_keys,
        )
    }

    fn consume(&mut self, disposition: KeyDisposition) -> bool {
        match disposition {
            KeyDisposition::Consume(command) => {
                if let Some(command) = command {
                    self.pending.commands.push(command);
                }
                true
            }
            KeyDisposition::PassThrough => false,
        }
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

fn install(state: Arc<Mutex<KeyboardState>>, wake: impl Fn() + Send + Sync + 'static) {
    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { event.as_ref() };
        wake();
        if ev.r#type() != NSEventType::KeyDown {
            return event.as_ptr();
        }
        let key_code = ev.keyCode();
        let flags = ev.modifierFlags();
        let mut state = state.lock();
        if let Some(token) = state.capture_target {
            if ev.isARepeat() {
                return std::ptr::null_mut();
            }
            let stroke = capture_stroke(ev, key_code, flags);
            if releases_shortcut_capture(&stroke) {
                state.capture_target = None;
                state.pending.shortcut_releases.push(token);
                return event.as_ptr();
            }
            state.pending_prefix = None;
            state
                .pending
                .shortcut_captures
                .push(PendingShortcutCapture {
                    token,
                    stroke,
                    pressed_at_ms: vmux_core::now_millis(),
                });
            return std::ptr::null_mut();
        }
        let Some(combo) = translate(key_code, flags) else {
            return event.as_ptr();
        };
        let action = state.classify(combo);
        if state.consume(action) {
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

fn install_monitor(
    runtime: Single<&KeyboardRuntime>,
    keymap: Res<Keymap>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let Some(proxy) = proxy else {
        return;
    };
    runtime.0.lock().keymap = Some(keymap.clone());
    let state = runtime.0.clone();
    let proxy = (**proxy).clone();
    install(state, move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
}

fn sync_keyboard_context(
    runtime: Single<&KeyboardRuntime>,
    keymap: Res<Keymap>,
    browser: Option<Res<vmux_browser::KeyboardContext>>,
    capture: Option<Res<vmux_shortcut::ShortcutCaptureTarget>>,
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
                    vmux_simulator::url::SimulatorRoute::try_from(metadata.url.as_str()).is_ok()
                })
            })
        });
    {
        let mut state = runtime.0.lock();
        if keymap.is_changed() || state.keymap.is_none() {
            state.keymap = Some(keymap.clone());
        }
        state.capture_target = capture.as_deref().and_then(|capture| capture.token());
        state.simulator_active = active.is_some();
        state.page_owns_escape = browser
            .as_deref()
            .is_some_and(|context| context.page_owns_escape);
        state.text_entry_owns_keys = browser
            .as_deref()
            .is_some_and(|context| context.text_entry_owns_keys);
    }
    if let Some(requests) = requests.as_mut() {
        requests.write(vmux_simulator::SimulatorFocusRequest(active));
    }
}

fn dispatch_keyboard_input(
    runtime: Single<&KeyboardRuntime>,
    mut invocations: MessageWriter<vmux_command::CommandInvocation>,
    mut simulator_buttons: Option<ResMut<Messages<vmux_simulator::HardwareButtonRequest>>>,
    mut simulator_clipboard: Option<ResMut<Messages<vmux_simulator::SimulatorClipboardRequest>>>,
    mut simulator_keyboard: Option<
        ResMut<Messages<vmux_simulator::SimulatorSoftwareKeyboardRequest>>,
    >,
    mut fullscreen: MessageWriter<ExitFullscreenRequest>,
    mut hide_windows: Option<MessageWriter<crate::runtime::HideAllWindowsRequest>>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut shortcut_capture: Option<ResMut<vmux_shortcut::ShortcutCaptureTarget>>,
    mut commands: Commands,
) {
    let pending = runtime.drain();
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    for command in pending.commands {
        invocations.write(vmux_command::CommandInvocation::new(caller, command));
    }
    if let Some(target) = shortcut_capture.as_deref_mut() {
        for token in pending.shortcut_releases {
            target.release(token);
        }
    }
    if let Some(token) = shortcut_capture
        .as_deref()
        .and_then(|target| target.token())
    {
        for capture in pending.shortcut_captures {
            if capture.token == token {
                commands.trigger(vmux_shortcut::ShortcutProbePress::new(
                    token.target,
                    capture.stroke,
                    capture.pressed_at_ms,
                ));
            }
        }
    }
    if let Some(simulator_buttons) = simulator_buttons.as_mut() {
        for button in pending.simulator_buttons {
            simulator_buttons.write(vmux_simulator::HardwareButtonRequest { view: None, button });
        }
    }
    if let Some(simulator_clipboard) = simulator_clipboard.as_mut() {
        for operation in pending.simulator_clipboard {
            simulator_clipboard.write(vmux_simulator::SimulatorClipboardRequest {
                view: None,
                operation,
            });
        }
    }
    if let Some(simulator_keyboard) = simulator_keyboard.as_mut() {
        for _ in 0..pending.simulator_keyboard {
            simulator_keyboard
                .write(vmux_simulator::SimulatorSoftwareKeyboardRequest { view: None });
        }
    }
    if pending.exit_fullscreen {
        fullscreen.write(ExitFullscreenRequest);
    }
    if pending.quit
        && let Some(hide_windows) = hide_windows.as_mut()
    {
        hide_windows.write(crate::runtime::HideAllWindowsRequest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Keymap {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_command::CommandPlugin))
            .add_plugins(vmux_command::command_bar::CommandBarPlugin)
            .add_plugins((
                vmux_command::CommandTypePlugin::<vmux_layout::pane::OpenRequest>::default(),
                vmux_command::CommandTypePlugin::<vmux_layout::pane::CloseRequest>::default(),
                vmux_command::CommandTypePlugin::<vmux_layout::pane::FocusRequest>::default(),
                vmux_command::CommandTypePlugin::<vmux_layout::pane::ArrangeRequest>::default(),
                vmux_command::CommandTypePlugin::<vmux_layout::pane::ResizeRequest>::default(),
                vmux_command::CommandTypePlugin::<vmux_layout::pane::ToggleZoomRequest>::default(),
            ));
        app.world_mut().run_schedule(Startup);
        let mut query = app.world_mut().query::<&vmux_command::CommandDefinition>();
        let definitions = query.iter(app.world()).cloned().collect::<Vec<_>>();
        Keymap::defaults_with(&definitions)
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

        let disposition = decide(
            &map,
            &mut pending,
            combo(KeyCode::KeyB, true),
            Instant::now(),
            true,
        );

        assert!(matches!(disposition, KeyDisposition::PassThrough));
        assert!(pending.is_none(), "no chord may be left open");
    }

    #[test]
    fn leader_then_h_consumes_and_emits_select_left() {
        let map = map();
        let mut pending = None;
        let now = Instant::now();

        let prefix = decide(&map, &mut pending, combo(KeyCode::KeyB, true), now, false);
        assert!(matches!(prefix, KeyDisposition::Consume(None)));
        assert!(pending.is_some());

        let second = decide(&map, &mut pending, combo(KeyCode::KeyH, false), now, false);
        assert!(matches!(
            second,
            KeyDisposition::Consume(Some(id)) if id == "select_pane_left"
        ));
        assert!(pending.is_none());
    }

    #[test]
    fn bare_key_without_pending_passes_through() {
        let map = map();
        let mut pending = None;
        let disposition = decide(
            &map,
            &mut pending,
            combo(KeyCode::KeyH, false),
            Instant::now(),
            false,
        );
        assert!(matches!(disposition, KeyDisposition::PassThrough));
    }

    #[test]
    fn fullscreen_escape_becomes_an_ecs_request_unless_the_page_owns_it() {
        let mut state = KeyboardState {
            window_fullscreen: true,
            ..Default::default()
        };

        let disposition = state.classify(combo(KeyCode::Escape, false));

        assert!(state.consume(disposition));
        assert!(state.pending.exit_fullscreen);

        state.pending.exit_fullscreen = false;
        state.page_owns_escape = true;
        let disposition = state.classify(combo(KeyCode::Escape, false));

        assert!(matches!(disposition, KeyDisposition::PassThrough));
        assert!(!state.pending.exit_fullscreen);
    }

    #[test]
    fn consumed_shortcut_queues_command() {
        let mut state = KeyboardState::default();
        let consumed = state.consume(KeyDisposition::Consume(Some(
            "select_pane_left".to_string(),
        )));
        assert!(consumed);
        assert_eq!(state.pending.commands, ["select_pane_left"]);
    }

    #[test]
    fn expired_prefix_does_not_consume_second_key() {
        let map = map();
        let mut pending = Some((combo(KeyCode::KeyB, true), Instant::now()));
        let later = Instant::now() + Duration::from_millis(2000);
        let disposition = decide(
            &map,
            &mut pending,
            combo(KeyCode::KeyH, false),
            later,
            false,
        );
        assert!(matches!(disposition, KeyDisposition::PassThrough));
        assert!(pending.is_none());
    }

    #[test]
    fn native_command_bar_shortcuts_are_consumed_before_cef() {
        let map = map();
        let mut pending = None;
        let now = Instant::now();
        let shortcuts = [
            (super_combo(KeyCode::KeyK), "browser_open_command_bar"),
            (
                super_combo(KeyCode::KeyL),
                "browser_open_page_in_command_bar",
            ),
            (super_combo(KeyCode::Slash), "browser_open_path_bar"),
        ];

        for (pressed, expected) in shortcuts {
            let disposition = decide(&map, &mut pending, pressed, now, false);
            assert!(matches!(
                disposition,
                KeyDisposition::Consume(Some(id)) if id == expected
            ));
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
    fn simulator_clipboard_shortcuts_map_command_edit_actions() {
        use vmux_simulator::event::SimulatorClipboardOperation;

        assert_eq!(
            simulator_clipboard(&super_combo(KeyCode::KeyA)),
            Some(SimulatorClipboardOperation::SelectAll)
        );
        assert_eq!(
            simulator_clipboard(&super_combo(KeyCode::KeyC)),
            Some(SimulatorClipboardOperation::Copy)
        );
        assert_eq!(
            simulator_clipboard(&super_combo(KeyCode::KeyX)),
            Some(SimulatorClipboardOperation::Cut)
        );
        assert_eq!(
            simulator_clipboard(&super_combo(KeyCode::KeyV)),
            Some(SimulatorClipboardOperation::Paste)
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
    fn simulator_text_shortcuts_reach_the_phone() {
        for key in [
            KeyCode::KeyZ,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Backspace,
        ] {
            assert!(simulator_text_shortcut(&super_combo(key)));
        }
        let mut redo = super_combo(KeyCode::KeyZ);
        redo.modifiers.shift = true;
        assert!(simulator_text_shortcut(&redo));
        assert!(!simulator_text_shortcut(&super_combo(KeyCode::KeyA)));
        assert!(!simulator_text_shortcut(&super_combo(KeyCode::KeyX)));
    }

    #[test]
    fn simulator_text_shortcuts_do_not_take_app_commands() {
        assert!(!simulator_text_shortcut(&super_combo(KeyCode::KeyW)));
        assert!(!simulator_text_shortcut(&super_combo(KeyCode::KeyT)));
        assert!(!simulator_text_shortcut(&super_combo(KeyCode::KeyQ)));
        let mut shifted_select_all = super_combo(KeyCode::KeyA);
        shifted_select_all.modifiers.shift = true;
        assert!(!simulator_text_shortcut(&shifted_select_all));
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
