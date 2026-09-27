use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use crossbeam_channel::{Receiver, Sender};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
use parking_lot::Mutex;
use vmux_core::input::{
    ConsumesNativeKey, NativeKey, NativeKeyCapture, NativeKeyClaimSet, NativeKeyInput,
    NativeKeyInputSet, PassesNativeKey,
};

use crate::shortcut::{KeyCombo, Keymap, Modifiers};

pub(crate) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ExitFullscreenRequest>()
            .add_message::<NativeKeyInput>()
            .add_systems(
                Startup,
                (spawn_keyboard_bridge, install_monitor)
                    .chain()
                    .after(crate::shortcut::ShortcutInit),
            )
            .add_systems(
                Update,
                sync_keyboard_context
                    .after(vmux_layout::stack::ComputeFocusSet)
                    .after(vmux_browser::KeyboardContextSet)
                    .after(vmux_shortcut::ShortcutCaptureSet)
                    .after(crate::window_state::SyncWindowFullscreen)
                    .after(NativeKeyClaimSet)
                    .before(NativeKeyInputSet),
            )
            .add_systems(
                Update,
                dispatch_keyboard_input
                    .after(sync_keyboard_context)
                    .in_set(NativeKeyInputSet)
                    .in_set(vmux_command::WriteCommandRequests)
                    .before(vmux_simulator::SimulatorInputSet),
            );
    }
}

#[derive(Message, Clone, Copy)]
pub(crate) struct ExitFullscreenRequest;

#[derive(Component, Clone)]
struct KeyboardBridge {
    context: Arc<Mutex<NativeKeyboardContext>>,
    output: NativeKeyboardOutput,
}

#[derive(Default)]
struct NativeKeyboardContext {
    keymap: Option<Keymap>,
    pending_prefix: Option<(KeyCombo, Instant)>,
    capture_active: bool,
    consumed: HashMap<KeyCombo, Entity>,
    passed: HashSet<KeyCombo>,
    window_fullscreen: bool,
    page_owns_escape: bool,
    text_entry_owns_keys: bool,
}

#[derive(Clone)]
struct NativeKeyboardOutput {
    commands: Sender<String>,
    inputs: Sender<NativeKeyInput>,
    exit_fullscreen: Sender<()>,
    quit: Sender<()>,
}

#[derive(Component)]
struct KeyboardInbox {
    commands: Receiver<String>,
    inputs: Receiver<NativeKeyInput>,
    exit_fullscreen: Receiver<()>,
    quit: Receiver<()>,
}

fn spawn_keyboard_bridge(mut commands: Commands) {
    let (bridge, inbox) = KeyboardBridge::channel();
    commands.spawn((Name::new("Keyboard"), bridge, inbox));
}

impl KeyboardBridge {
    fn channel() -> (Self, KeyboardInbox) {
        let (commands, command_inbox) = crossbeam_channel::unbounded();
        let (inputs, input_inbox) = crossbeam_channel::unbounded();
        let (exit_fullscreen, exit_fullscreen_inbox) = crossbeam_channel::unbounded();
        let (quit, quit_inbox) = crossbeam_channel::unbounded();
        (
            Self {
                context: Arc::new(Mutex::new(NativeKeyboardContext::default())),
                output: NativeKeyboardOutput {
                    commands,
                    inputs,
                    exit_fullscreen,
                    quit,
                },
            },
            KeyboardInbox {
                commands: command_inbox,
                inputs: input_inbox,
                exit_fullscreen: exit_fullscreen_inbox,
                quit: quit_inbox,
            },
        )
    }

    fn install(&self, wake: impl Fn() + Send + Sync + 'static) {
        let context = self.context.clone();
        let output = self.output.clone();
        let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let ev = unsafe { event.as_ref() };
            wake();
            if ev.r#type() != NSEventType::KeyDown {
                return event.as_ptr();
            }
            let key_code = ev.keyCode();
            let flags = ev.modifierFlags();
            let input = native_input(ev, key_code, flags);
            let mut context = context.lock();
            if context.capture_active {
                if ev.isARepeat() {
                    return std::ptr::null_mut();
                }
                let releases = input.releases_capture();
                if releases {
                    context.capture_active = false;
                }
                context.pending_prefix = None;
                let _ = output.inputs.send(NativeKeyInput {
                    captured: true,
                    ..input
                });
                if releases {
                    return event.as_ptr();
                }
                return std::ptr::null_mut();
            }
            let Some(combo) = translate(key_code, flags) else {
                return event.as_ptr();
            };
            let disposition = context.classify(combo);
            if output.publish(disposition, input) {
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

enum KeyDisposition {
    Consume(Option<String>),
    Feature(Entity),
    ExitFullscreen,
    Quit,
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

impl NativeKeyboardContext {
    fn classify(&mut self, combo: KeyCombo) -> KeyDisposition {
        if self.passed.contains(&combo) {
            return KeyDisposition::PassThrough;
        }
        if let Some(entity) = self.consumed.get(&combo) {
            return KeyDisposition::Feature(*entity);
        }
        if escape_exits_fullscreen(&combo, self.window_fullscreen, self.page_owns_escape) {
            return KeyDisposition::ExitFullscreen;
        }
        if quits_the_app(&combo) {
            return KeyDisposition::Quit;
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
}

impl NativeKeyboardOutput {
    fn publish(&self, disposition: KeyDisposition, mut input: NativeKeyInput) -> bool {
        match disposition {
            KeyDisposition::Consume(command) => {
                if let Some(command) = command {
                    let _ = self.commands.send(command);
                }
                true
            }
            KeyDisposition::Feature(claim) => {
                input.claim = Some(claim);
                let _ = self.inputs.send(input);
                true
            }
            KeyDisposition::ExitFullscreen => {
                let _ = self.exit_fullscreen.send(());
                true
            }
            KeyDisposition::Quit => {
                let _ = self.quit.send(());
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

fn native_input(event: &NSEvent, native_code: u16, flags: NSEventModifierFlags) -> NativeKeyInput {
    let text = event
        .charactersIgnoringModifiers()
        .map(|characters| characters.to_string())
        .unwrap_or_default();
    NativeKeyInput {
        key: key_code_from_vk(native_code),
        native_code,
        text,
        modifiers: modifiers(flags).into(),
        repeat: event.isARepeat(),
        captured: false,
        claim: None,
        pressed_at_ms: vmux_core::now_millis(),
    }
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

fn install_monitor(
    keyboard: Single<&KeyboardBridge>,
    keymap: Res<Keymap>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    let Some(proxy) = proxy else {
        return;
    };
    keyboard.context.lock().keymap = Some(keymap.clone());
    let proxy = (**proxy).clone();
    keyboard.install(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
}

fn sync_keyboard_context(
    keyboard: Single<&KeyboardBridge>,
    keymap: Res<Keymap>,
    browser: Option<Res<vmux_browser::KeyboardContext>>,
    capture: Query<(), With<NativeKeyCapture>>,
    claims: Query<
        (
            Entity,
            &NativeKey,
            Has<ConsumesNativeKey>,
            Has<PassesNativeKey>,
        ),
        With<vmux_core::Active>,
    >,
    focused_window: Option<Res<vmux_layout::window::FocusedWindow>>,
    fullscreen: Query<&crate::window_state::WindowFullscreen>,
) {
    let mut context = keyboard.context.lock();
    if keymap.is_changed() || context.keymap.is_none() {
        context.keymap = Some(keymap.clone());
    }
    context.capture_active = !capture.is_empty();
    context.consumed.clear();
    context.passed.clear();
    for (entity, key, consumes, passes) in &claims {
        let combo = KeyCombo {
            key: key.key,
            modifiers: Modifiers {
                ctrl: key.modifiers.ctrl,
                shift: key.modifiers.shift,
                alt: key.modifiers.alt,
                super_key: key.modifiers.super_key,
            },
        };
        if passes {
            context.passed.insert(combo.clone());
        }
        if consumes {
            context.consumed.insert(combo, entity);
        }
    }
    context.window_fullscreen = focused_window
        .as_deref()
        .and_then(|focused_window| focused_window.0)
        .and_then(|window| fullscreen.get(window).ok())
        .is_some_and(|fullscreen| fullscreen.0);
    context.page_owns_escape = browser
        .as_deref()
        .is_some_and(|context| context.page_owns_escape);
    context.text_entry_owns_keys = browser
        .as_deref()
        .is_some_and(|context| context.text_entry_owns_keys);
}

fn dispatch_keyboard_input(
    inbox: Single<&KeyboardInbox>,
    mut invocations: MessageWriter<vmux_command::CommandInvocation>,
    mut inputs: MessageWriter<NativeKeyInput>,
    mut fullscreen: MessageWriter<ExitFullscreenRequest>,
    mut hide_windows: Option<MessageWriter<crate::runtime::HideAllWindowsRequest>>,
    user: Query<Entity, With<vmux_core::team::User>>,
) {
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    for command in inbox.commands.try_iter() {
        invocations.write(vmux_command::CommandInvocation::new(caller, command));
    }
    for input in inbox.inputs.try_iter() {
        inputs.write(input);
    }
    if inbox.exit_fullscreen.try_iter().next().is_some() {
        fullscreen.write(ExitFullscreenRequest);
    }
    if inbox.quit.try_iter().next().is_some()
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

    fn input(key: KeyCode) -> NativeKeyInput {
        NativeKeyInput {
            key: Some(key),
            native_code: 0,
            text: String::new(),
            modifiers: Default::default(),
            repeat: false,
            captured: false,
            claim: None,
            pressed_at_ms: 0,
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
        let mut context = NativeKeyboardContext {
            window_fullscreen: true,
            ..Default::default()
        };

        let disposition = context.classify(combo(KeyCode::Escape, false));

        assert!(matches!(disposition, KeyDisposition::ExitFullscreen));

        context.page_owns_escape = true;
        let disposition = context.classify(combo(KeyCode::Escape, false));

        assert!(matches!(disposition, KeyDisposition::PassThrough));
    }

    #[test]
    fn consumed_shortcut_queues_command() {
        let (bridge, inbox) = KeyboardBridge::channel();
        let consumed = bridge.output.publish(
            KeyDisposition::Consume(Some("select_pane_left".to_string())),
            input(KeyCode::KeyH),
        );
        assert!(consumed);
        assert_eq!(inbox.commands.try_recv().unwrap(), "select_pane_left");
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
    fn native_key_map_covers_insert_and_international_keys() {
        assert_eq!(key_code_from_vk(0x72), Some(KeyCode::Insert));
        assert_eq!(key_code_from_vk(0x0A), Some(KeyCode::IntlBackslash));
        assert_eq!(key_code_from_vk(0x5D), Some(KeyCode::IntlYen));
        assert_eq!(key_code_from_vk(0x5E), Some(KeyCode::IntlRo));
    }

    #[test]
    fn bare_tab_releases_shortcut_capture() {
        let stroke = input(KeyCode::Tab);
        let mut modified = stroke.clone();
        modified.modifiers.ctrl = true;

        assert!(stroke.releases_capture());
        assert!(!modified.releases_capture());
    }
}
