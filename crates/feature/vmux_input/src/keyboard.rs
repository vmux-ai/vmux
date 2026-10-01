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
use vmux_command::shortcut::{KeyCombo, Keymap, Modifiers};
use vmux_command::{CommandInvocation, WriteCommandRequests};
use vmux_core::input::{
    ConsumesNativeKey, NativeKey, NativeKeyCapture, NativeKeyClaimSet, NativeKeyInput,
    NativeKeyInputSet, PassesNativeKey,
};
use vmux_core::team::User;
use vmux_core::{Active, KeyModifiers, WindowFullscreen, WindowFullscreenSet, now_millis};

use crate::{ExitFullscreenShortcut, HideWindowsShortcut, KeyboardContext, KeyboardContextSet};

pub struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<NativeKeyInput>()
            .add_message::<ExitFullscreenShortcut>()
            .add_message::<HideWindowsShortcut>()
            .add_systems(Startup, (spawn_bridge, spawn_application_key_bindings))
            .add_systems(Update, install_monitor)
            .add_systems(
                Update,
                sync_context
                    .after(KeyboardContextSet)
                    .after(NativeKeyClaimSet)
                    .before(NativeKeyInputSet),
            )
            .add_systems(
                Update,
                dispatch
                    .after(sync_context)
                    .in_set(NativeKeyInputSet)
                    .in_set(WriteCommandRequests),
            )
            .add_systems(
                Update,
                sync_application_key_bindings
                    .in_set(NativeKeyClaimSet)
                    .after(KeyboardContextSet)
                    .after(WindowFullscreenSet),
            )
            .add_systems(Update, publish_shortcuts.after(NativeKeyInputSet));
    }
}

#[derive(Component)]
struct ExitFullscreenKey;

#[derive(Component)]
struct HideWindowsKey;

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
    text_entry_owns_keys: bool,
}

#[derive(Clone)]
struct NativeKeyboardOutput {
    commands: Sender<String>,
    inputs: Sender<NativeKeyInput>,
}

#[derive(Component)]
struct KeyboardInbox {
    commands: Receiver<String>,
    inputs: Receiver<NativeKeyInput>,
}

#[derive(Component)]
struct KeyboardMonitorPending;

fn spawn_bridge(mut commands: Commands) {
    let (bridge, inbox) = KeyboardBridge::channel();
    commands.spawn((Name::new("Keyboard"), bridge, inbox, KeyboardMonitorPending));
}

fn spawn_application_key_bindings(mut commands: Commands) {
    commands.spawn((
        Name::new("Hide windows key"),
        HideWindowsKey,
        NativeKey {
            key: KeyCode::KeyQ,
            modifiers: KeyModifiers {
                super_key: true,
                ..default()
            },
        },
        ConsumesNativeKey,
        Active,
    ));
    for shift in [false, true] {
        commands.spawn((
            Name::new("Exit fullscreen key"),
            ExitFullscreenKey,
            NativeKey {
                key: KeyCode::Escape,
                modifiers: KeyModifiers { shift, ..default() },
            },
            ConsumesNativeKey,
        ));
    }
}

impl KeyboardBridge {
    fn channel() -> (Self, KeyboardInbox) {
        let (commands, command_inbox) = crossbeam_channel::unbounded();
        let (inputs, input_inbox) = crossbeam_channel::unbounded();
        (
            Self {
                context: Arc::new(Mutex::new(NativeKeyboardContext::default())),
                output: NativeKeyboardOutput { commands, inputs },
            },
            KeyboardInbox {
                commands: command_inbox,
                inputs: input_inbox,
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

enum KeyDisposition {
    Consume(Option<String>),
    Feature(Entity),
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
        pressed_at_ms: now_millis(),
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
    keyboard: Query<(Entity, &KeyboardBridge), With<KeyboardMonitorPending>>,
    keymaps: Query<&Keymap>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
    mut commands: Commands,
) {
    let Some(proxy) = proxy else {
        return;
    };
    let Ok((entity, keyboard)) = keyboard.single() else {
        return;
    };
    let Ok(keymap) = keymaps.single() else {
        return;
    };
    keyboard.context.lock().keymap = Some((*keymap).clone());
    let proxy = (**proxy).clone();
    keyboard.install(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    });
    commands.entity(entity).remove::<KeyboardMonitorPending>();
}

type ActiveNativeKeyClaims<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static NativeKey,
        Has<ConsumesNativeKey>,
        Has<PassesNativeKey>,
    ),
    With<Active>,
>;

fn sync_context(
    keyboard: Single<&KeyboardBridge>,
    keymap: Single<Ref<Keymap>>,
    contexts: Query<&KeyboardContext>,
    capture: Query<(), With<NativeKeyCapture>>,
    claims: ActiveNativeKeyClaims,
) {
    let mut context = keyboard.context.lock();
    if keymap.is_changed() || context.keymap.is_none() {
        context.keymap = Some((**keymap).clone());
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
    context.text_entry_owns_keys = contexts
        .single()
        .ok()
        .is_some_and(|context| context.text_entry_owns_keys);
}

fn dispatch(
    inbox: Single<&KeyboardInbox>,
    mut invocations: MessageWriter<CommandInvocation>,
    mut inputs: MessageWriter<NativeKeyInput>,
    user: Query<Entity, With<User>>,
) {
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    for command in inbox.commands.try_iter() {
        invocations.write(CommandInvocation::new(caller, command));
    }
    for input in inbox.inputs.try_iter() {
        inputs.write(input);
    }
}

fn sync_application_key_bindings(
    context: Query<&KeyboardContext>,
    fullscreen: Query<&WindowFullscreen, (With<Window>, With<Active>)>,
    bindings: Query<(Entity, Has<Active>), With<ExitFullscreenKey>>,
    mut commands: Commands,
) {
    let page_owns_escape = context
        .single()
        .ok()
        .is_some_and(|context| context.page_owns_escape);
    let enabled = fullscreen
        .single()
        .ok()
        .is_some_and(|fullscreen| fullscreen.0)
        && !page_owns_escape;
    for (entity, active) in &bindings {
        if active == enabled {
            continue;
        }
        if enabled {
            commands.entity(entity).insert(Active);
        } else {
            commands.entity(entity).remove::<Active>();
        }
    }
}

fn publish_shortcuts(
    mut inputs: MessageReader<NativeKeyInput>,
    bindings: Query<(Has<ExitFullscreenKey>, Has<HideWindowsKey>), With<Active>>,
    mut exit_fullscreen: MessageWriter<ExitFullscreenShortcut>,
    mut hide_windows: MessageWriter<HideWindowsShortcut>,
) {
    for input in inputs.read() {
        let Some(claim) = input.claim else { continue };
        let Ok((exits_fullscreen, hides_windows)) = bindings.get(claim) else {
            continue;
        };
        if exits_fullscreen {
            exit_fullscreen.write(ExitFullscreenShortcut);
        }
        if hides_windows {
            hide_windows.write(HideWindowsShortcut);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use vmux_command::shortcut::{Binding, Shortcut, Source};

    fn map() -> Keymap {
        let mut map = Keymap::defaults();
        map.register([
            "select_pane_left",
            "browser_open_command_bar",
            "browser_open_page_in_command_bar",
            "browser_open_path_bar",
        ]);
        map.extend(
            Source::Default,
            [
                Binding {
                    shortcut: Shortcut::Chord(
                        combo(KeyCode::KeyB, true),
                        combo(KeyCode::KeyH, false),
                    ),
                    command: "select_pane_left".to_string(),
                    when: None,
                },
                Binding {
                    shortcut: Shortcut::Direct(super_combo(KeyCode::KeyK)),
                    command: "browser_open_command_bar".to_string(),
                    when: None,
                },
                Binding {
                    shortcut: Shortcut::Direct(super_combo(KeyCode::KeyL)),
                    command: "browser_open_page_in_command_bar".to_string(),
                    when: None,
                },
                Binding {
                    shortcut: Shortcut::Direct(super_combo(KeyCode::Slash)),
                    command: "browser_open_path_bar".to_string(),
                    when: None,
                },
            ],
        );
        map
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

    #[test]
    fn fullscreen_escape_publishes_a_shortcut_unless_the_page_owns_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<NativeKeyInput>()
            .add_message::<ExitFullscreenShortcut>()
            .add_message::<HideWindowsShortcut>()
            .add_systems(Startup, spawn_application_key_bindings)
            .add_systems(
                Update,
                (sync_application_key_bindings, publish_shortcuts).chain(),
            );
        app.world_mut().spawn((
            Window::default(),
            vmux_core::WindowFullscreen(true),
            vmux_core::Active,
        ));
        app.update();
        let mut claims = app.world_mut().query_filtered::<
            (Entity, &NativeKey),
            (With<ExitFullscreenKey>, With<vmux_core::Active>),
        >();
        let claim = claims
            .iter(app.world())
            .find_map(|(entity, key)| (!key.modifiers.shift).then_some(entity))
            .unwrap();
        app.world_mut()
            .resource_mut::<Messages<NativeKeyInput>>()
            .write(NativeKeyInput {
                key: Some(KeyCode::Escape),
                native_code: 0,
                text: String::new(),
                modifiers: Default::default(),
                repeat: false,
                captured: false,
                claim: Some(claim),
                pressed_at_ms: 0,
            });
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<ExitFullscreenShortcut>>()
                .drain()
                .count(),
            1
        );

        app.world_mut().spawn(KeyboardContext {
            page_owns_escape: true,
            text_entry_owns_keys: false,
        });
        app.update();

        assert!(
            app.world_mut()
                .query_filtered::<Entity, (With<ExitFullscreenKey>, With<vmux_core::Active>)>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
