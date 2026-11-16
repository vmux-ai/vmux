use crate::{
    ConsumesNativeKey, ExitFullscreenShortcut, HideWindowsShortcut, KeyboardContext,
    KeyboardContextSet, NativeKey, NativeKeyCapture, NativeKeyClaimSet, NativeKeyInput,
    NativeKeyInputSet, PassesNativeKey,
};
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::prelude::{
    App, ChildOf, Commands, Entity, Has, MessageReader, MessageWriter, Plugin, Query, Ref, Window,
    With,
};
use bevy::prelude::{On, Remove};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use vmux_api::input::KeyModifiers;
use vmux_command::{
    CommandInvocation, KeyCombo, Keymap, Modifiers, Shortcut, WriteCommandRequests,
};
use vmux_ecs::team::User;
use vmux_ecs::{Active, WindowFullscreen, WindowFullscreenSet};

pub(crate) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<KeyboardPress>()
            .add_message::<KeyboardEffect>()
            .add_message::<KeyboardSuppression>()
            .add_message::<NativeKeyInput>()
            .add_message::<ExitFullscreenShortcut>()
            .add_message::<HideWindowsShortcut>()
            .add_observer(retire_streams)
            .add_systems(Startup, spawn_application_key_bindings)
            .add_systems(
                PreUpdate,
                (initialize, ApplyDeferred)
                    .chain()
                    .in_set(KeyboardInitializeSet),
            )
            .add_systems(
                Update,
                (sync_application_key_bindings, ApplyDeferred)
                    .chain()
                    .in_set(NativeKeyClaimSet)
                    .after(KeyboardContextSet)
                    .after(WindowFullscreenSet),
            )
            .add_systems(
                Update,
                route
                    .in_set(KeyboardRouteSet)
                    .in_set(WriteCommandRequests)
                    .after(KeyboardContextSet)
                    .after(NativeKeyClaimSet)
                    .before(NativeKeyInputSet),
            )
            .add_systems(Update, publish_shortcuts.after(NativeKeyInputSet))
            .add_systems(PostUpdate, release_capture)
            .add_systems(Last, publish_suppression.in_set(KeyboardPublishSet));
        #[cfg(target_os = "macos")]
        app.add_plugins(crate::keyboard_native::KeyboardNativePlugin);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KeyboardInitializeSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KeyboardRouteSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KeyboardPublishSet;

#[derive(Component)]
pub(crate) struct KeyboardStream;

#[derive(Component, Default)]
struct NativeChord {
    prefix: Option<NativePrefix>,
    focused_window: Option<Entity>,
}

struct NativePrefix {
    key: NativeKey,
    accepted_at: Instant,
}

#[derive(Message, Clone, Debug)]
pub(crate) struct KeyboardPress {
    pub(crate) stream: Entity,
    pub(crate) token: u64,
    pub(crate) window: Option<Entity>,
    pub(crate) input: NativeKeyInput,
    pub(crate) observed_at: Instant,
    pub(crate) overflowed: bool,
}

impl KeyboardPress {
    fn combo(&self, key: KeyCode) -> KeyCombo {
        KeyCombo {
            key,
            modifiers: Modifiers {
                ctrl: self.input.modifiers.ctrl,
                alt: self.input.modifiers.alt,
                shift: self.input.modifiers.shift,
                super_key: self.input.modifiers.super_key,
            },
        }
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct KeyboardEffect {
    pub(crate) stream: Entity,
    pub(crate) token: u64,
    pub(crate) forward: bool,
}

#[derive(Message, Clone)]
pub(crate) struct KeyboardSuppression {
    pub(crate) stream: Entity,
    pub(crate) keys: HashSet<NativeKey>,
    pub(crate) all_keys: bool,
}

#[derive(Component)]
struct ExitFullscreenKey;

#[derive(Component)]
struct HideWindowsKey;

#[derive(Component, Default)]
struct HideWindowsGesture {
    started_at_ms: Option<i64>,
    triggered: bool,
}

impl HideWindowsGesture {
    const DOUBLE_TAP_MS: i64 = 450;
    const HOLD_MS: i64 = 900;

    fn register(&mut self, input: &NativeKeyInput) -> bool {
        if input.repeat {
            let Some(started_at_ms) = self.started_at_ms else {
                return false;
            };
            if self.triggered || input.pressed_at_ms - started_at_ms < Self::HOLD_MS {
                return false;
            }
            self.triggered = true;
            return true;
        }

        let double_tap = !self.triggered
            && self.started_at_ms.is_some_and(|started_at_ms| {
                input.pressed_at_ms - started_at_ms <= Self::DOUBLE_TAP_MS
            });
        if double_tap {
            self.triggered = true;
            return true;
        }
        self.started_at_ms = Some(input.pressed_at_ms);
        self.triggered = false;
        false
    }
}

fn initialize(
    keymaps: Query<(Entity, Ref<Keymap>)>,
    streams: Query<(Entity, &ChildOf), With<KeyboardStream>>,
    mut commands: Commands,
) {
    for (owner, keymap) in &keymaps {
        if !keymap.is_added() && streams.iter().any(|(_, parent)| parent.parent() == owner) {
            continue;
        }
        for (entity, parent) in &streams {
            if parent.parent() == owner {
                commands.entity(entity).despawn();
            }
        }
        commands.spawn((
            Name::new("Native keyboard stream"),
            KeyboardStream,
            NativeChord::default(),
            ChildOf(owner),
        ));
    }
}

fn retire_streams(
    removed: On<Remove, Keymap>,
    streams: Query<(Entity, &ChildOf), With<KeyboardStream>>,
    mut commands: Commands,
) {
    for (entity, parent) in &streams {
        if parent.parent() == removed.event_target() {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_application_key_bindings(mut commands: Commands) {
    commands.spawn((
        Name::new("Hide windows key"),
        HideWindowsKey,
        HideWindowsGesture::default(),
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

impl NativeChord {
    fn command(
        &mut self,
        map: &Keymap,
        combo: KeyCombo,
        now: Instant,
        text_entry: bool,
    ) -> (bool, Option<String>) {
        if self.prefix.as_ref().is_some_and(|prefix| {
            now.saturating_duration_since(prefix.accepted_at)
                > Duration::from_millis(map.chord_timeout_ms)
        }) {
            self.prefix = None;
        }
        if let Some(prefix) = self.prefix.take()
            && let Some(command) = map.chord(&prefix.key.combo(), &combo)
        {
            return (true, Some(command));
        }
        if let Some(command) = map.direct(&combo) {
            if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key {
                return (true, Some(command));
            }
            return (false, None);
        }
        if !text_entry && map.has_chord_prefix(&combo) {
            self.prefix = Some(NativePrefix {
                key: NativeKey {
                    key: combo.key,
                    modifiers: KeyModifiers {
                        ctrl: combo.modifiers.ctrl,
                        alt: combo.modifiers.alt,
                        shift: combo.modifiers.shift,
                        super_key: combo.modifiers.super_key,
                    },
                },
                accepted_at: now,
            });
            return (true, None);
        }
        (false, None)
    }
}

type NativeStreams<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf, &'static mut NativeChord), With<KeyboardStream>>;
type NativeClaims<'w, 's> = Query<
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

#[derive(bevy::ecs::system::SystemParam)]
struct Sequences<'w, 's> {
    streams: NativeStreams<'w, 's>,
    maps: Query<'w, 's, Ref<'static, Keymap>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct RoutingOutput<'w, 's> {
    inputs: MessageWriter<'w, NativeKeyInput>,
    invocations: MessageWriter<'w, CommandInvocation>,
    effects: MessageWriter<'w, KeyboardEffect>,
    commands: Commands<'w, 's>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Routing<'w, 's> {
    windows: Query<'w, 's, Entity, (With<Window>, With<Active>)>,
    contexts: Query<'w, 's, &'static KeyboardContext>,
    captures: Query<'w, 's, Entity, With<NativeKeyCapture>>,
    claims: NativeClaims<'w, 's>,
    user: Query<'w, 's, Entity, With<User>>,
}

impl Routing<'_, '_> {
    fn claim(&self, input: &NativeKeyInput) -> (bool, Option<Entity>) {
        let mut passed = false;
        let mut feature = None;
        for (entity, key, consumes, passes) in &self.claims {
            if Some(key.key) != input.key || key.modifiers != input.modifiers {
                continue;
            }
            passed |= passes;
            if consumes {
                feature = Some(entity);
            }
        }
        (passed, feature)
    }
}

fn route(
    mut packets: MessageReader<KeyboardPress>,
    mut sequences: Sequences,
    routing: Routing,
    mut output: RoutingOutput,
) {
    let focused = routing.windows.iter().next();
    let text_entry = routing
        .contexts
        .single()
        .is_ok_and(|context| context.text_entry_owns_keys);
    let caller = routing.user.single().unwrap_or(Entity::PLACEHOLDER);
    let mut capture_active = !routing.captures.is_empty();
    for (_, parent, mut chord) in &mut sequences.streams {
        let Ok(map) = sequences.maps.get(parent.parent()) else {
            continue;
        };
        if map.is_changed() || chord.focused_window != focused || text_entry || capture_active {
            chord.prefix = None;
        }
        chord.focused_window = focused;
    }
    let Sequences { streams, maps } = &mut sequences;
    for packet in packets.read() {
        let mut forward = false;
        let Ok((stream, parent, mut chord)) = streams.get_mut(packet.stream) else {
            output.effects.write(KeyboardEffect {
                stream: packet.stream,
                token: packet.token,
                forward,
            });
            continue;
        };
        let Ok(map) = maps.get(parent.parent()) else {
            output.effects.write(KeyboardEffect {
                stream,
                token: packet.token,
                forward: false,
            });
            continue;
        };
        if packet.overflowed {
            chord.prefix = None;
            capture_active = false;
            for capture in &routing.captures {
                output.commands.entity(capture).remove::<NativeKeyCapture>();
            }
            continue;
        }
        if packet.window.is_some() && packet.window != focused {
            output.effects.write(KeyboardEffect {
                stream,
                token: packet.token,
                forward,
            });
            continue;
        }
        if capture_active {
            chord.prefix = None;
            if !packet.input.repeat {
                let mut input = packet.input.clone();
                input.captured = true;
                let release = input.releases_capture();
                output.inputs.write(input);
                if release {
                    capture_active = false;
                }
                forward = release;
            }
        } else {
            forward = true;
            if let Some(key) = packet.input.key {
                let (passed, feature) = routing.claim(&packet.input);
                if !passed {
                    if let Some(claim) = feature {
                        let mut input = packet.input.clone();
                        input.claim = Some(claim);
                        output.inputs.write(input);
                        forward = false;
                    } else {
                        let combo = packet.combo(key);
                        let (consumed, command) =
                            chord.command(&map, combo, packet.observed_at, text_entry);
                        if let Some(command) = command {
                            output
                                .invocations
                                .write(CommandInvocation::new(caller, command));
                        }
                        forward = !consumed;
                    }
                }
            }
        }
        output.effects.write(KeyboardEffect {
            stream,
            token: packet.token,
            forward,
        });
    }
}

fn publish_suppression(
    streams: Query<(Entity, &ChildOf, &NativeChord), With<KeyboardStream>>,
    maps: Query<&Keymap>,
    contexts: Query<&KeyboardContext>,
    captures: Query<(), With<NativeKeyCapture>>,
    claims: NativeClaims,
    mut masks: MessageWriter<KeyboardSuppression>,
) {
    let text_entry = contexts
        .single()
        .is_ok_and(|context| context.text_entry_owns_keys);
    for (stream, parent, chord) in &streams {
        let Ok(map) = maps.get(parent.parent()) else {
            continue;
        };
        let mut keys = HashSet::new();
        for binding in map.bindings() {
            match &binding.shortcut {
                Shortcut::Direct(combo) => {
                    if combo.modifiers.ctrl || combo.modifiers.alt || combo.modifiers.super_key {
                        keys.insert(NativeKey {
                            key: combo.key,
                            modifiers: KeyModifiers {
                                ctrl: combo.modifiers.ctrl,
                                alt: combo.modifiers.alt,
                                shift: combo.modifiers.shift,
                                super_key: combo.modifiers.super_key,
                            },
                        });
                    }
                }
                Shortcut::Chord(prefix, _) => {
                    if !text_entry {
                        keys.insert(NativeKey {
                            key: prefix.key,
                            modifiers: KeyModifiers {
                                ctrl: prefix.modifiers.ctrl,
                                alt: prefix.modifiers.alt,
                                shift: prefix.modifiers.shift,
                                super_key: prefix.modifiers.super_key,
                            },
                        });
                    }
                }
            }
        }
        for (_, key, consumes, _) in &claims {
            if consumes {
                keys.insert(*key);
            }
        }
        for (_, key, _, passes) in &claims {
            if passes {
                keys.remove(key);
            }
        }
        masks.write(KeyboardSuppression {
            stream,
            keys,
            all_keys: chord.prefix.is_some() || !captures.is_empty(),
        });
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

fn release_capture(
    mut inputs: MessageReader<NativeKeyInput>,
    captures: Query<Entity, With<NativeKeyCapture>>,
    mut commands: Commands,
) {
    if !inputs
        .read()
        .any(|input| input.captured && input.releases_capture())
    {
        return;
    }
    for capture in &captures {
        commands.entity(capture).remove::<NativeKeyCapture>();
    }
}

fn publish_shortcuts(
    mut inputs: MessageReader<NativeKeyInput>,
    mut bindings: Query<(Has<ExitFullscreenKey>, Option<&mut HideWindowsGesture>), With<Active>>,
    mut exit_fullscreen: MessageWriter<ExitFullscreenShortcut>,
    mut hide_windows: MessageWriter<HideWindowsShortcut>,
) {
    for input in inputs.read() {
        let Some(claim) = input.claim else {
            continue;
        };
        let Ok((exits, gesture)) = bindings.get_mut(claim) else {
            continue;
        };
        if exits {
            exit_fullscreen.write(ExitFullscreenShortcut);
        }
        if let Some(mut gesture) = gesture
            && gesture.register(input)
        {
            hide_windows.write(HideWindowsShortcut);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use vmux_command::{Binding, Source};

    struct Keyboard {
        app: App,
        keymap: Entity,
        window: Entity,
        stream: Entity,
        clock: Instant,
        next: u64,
    }

    impl Keyboard {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, KeyboardPlugin))
                .add_message::<CommandInvocation>();
            let window = app.world_mut().spawn((Window::default(), Active)).id();
            let mut map = Keymap::defaults();
            map.register(["left", "bar"]);
            map.extend(
                Source::Default,
                [
                    Binding {
                        shortcut: Shortcut::Chord(
                            Self::combo(KeyCode::KeyB, true, false),
                            Self::combo(KeyCode::KeyH, false, false),
                        ),
                        command: "left".into(),
                        when: None,
                    },
                    Binding {
                        shortcut: Shortcut::Direct(Self::combo(KeyCode::KeyK, false, true)),
                        command: "bar".into(),
                        when: None,
                    },
                ],
            );
            let keymap = app.world_mut().spawn(map).id();
            app.update();
            let stream = {
                let world = app.world_mut();
                let mut streams = world.query_filtered::<Entity, With<KeyboardStream>>();
                streams.single(world).unwrap()
            };
            Self {
                app,
                keymap,
                window,
                stream,
                clock: Instant::now(),
                next: 0,
            }
        }

        fn combo(key: KeyCode, ctrl: bool, super_key: bool) -> KeyCombo {
            KeyCombo {
                key,
                modifiers: Modifiers {
                    ctrl,
                    super_key,
                    ..Default::default()
                },
            }
        }

        fn press(&mut self, combo: KeyCombo, at_ms: u64) {
            self.next += 1;
            self.app
                .world_mut()
                .resource_mut::<Messages<KeyboardPress>>()
                .write(KeyboardPress {
                    stream: self.stream,
                    token: self.next,
                    window: Some(self.window),
                    observed_at: self.clock + Duration::from_millis(at_ms),
                    overflowed: false,
                    input: NativeKeyInput {
                        key: Some(combo.key),
                        native_code: 0,
                        text: String::new(),
                        modifiers: KeyModifiers {
                            ctrl: combo.modifiers.ctrl,
                            alt: combo.modifiers.alt,
                            shift: combo.modifiers.shift,
                            super_key: combo.modifiers.super_key,
                        },
                        repeat: false,
                        captured: false,
                        claim: None,
                        pressed_at_ms: at_ms as i64,
                    },
                });
        }

        fn update(&mut self) -> (Vec<String>, Vec<bool>, Vec<NativeKeyInput>) {
            self.app.update();
            let world = self.app.world_mut();
            let commands = world
                .resource_mut::<Messages<CommandInvocation>>()
                .drain()
                .map(|input| input.id)
                .collect();
            let effects = world
                .resource_mut::<Messages<KeyboardEffect>>()
                .drain()
                .map(|effect| effect.forward)
                .collect();
            let inputs = world
                .resource_mut::<Messages<NativeKeyInput>>()
                .drain()
                .collect();
            (commands, effects, inputs)
        }
    }

    #[test]
    fn native_chord_consumes_a_prefix_and_routes_one_command() {
        let mut keyboard = Keyboard::new();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        assert_eq!(keyboard.update().1, [false]);
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        let (commands, effects, _) = keyboard.update();
        assert_eq!(commands, ["left"]);
        assert_eq!(effects, [false]);
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 30);
        let (commands, effects, _) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [true]);
    }

    #[test]
    fn native_chord_preserves_order_when_both_keys_arrive_in_one_update() {
        let mut keyboard = Keyboard::new();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        let (commands, effects, _) = keyboard.update();
        assert_eq!(commands, ["left"]);
        assert_eq!(effects, [false, false]);
    }

    #[test]
    fn native_chord_expiry_returns_the_second_key_to_its_native_window() {
        let mut keyboard = Keyboard::new();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        keyboard.update();
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 2_000);
        let (commands, effects, _) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [true]);
    }

    #[test]
    fn capture_releases_on_tab_before_the_next_input_in_the_batch() {
        let mut keyboard = Keyboard::new();
        let capture = keyboard.app.world_mut().spawn(NativeKeyCapture).id();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        let (commands, effects, inputs) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [false]);
        assert!(inputs[0].captured);
        let hint = keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardSuppression>>()
            .drain()
            .last()
            .unwrap();
        assert!(hint.all_keys);
        keyboard.press(Keyboard::combo(KeyCode::Tab, false, false), 10);
        keyboard.press(Keyboard::combo(KeyCode::KeyK, false, true), 20);
        let (commands, effects, inputs) = keyboard.update();
        assert_eq!(commands, ["bar"]);
        assert_eq!(effects, [true, false]);
        assert_eq!(inputs.len(), 1);
        assert!(inputs[0].releases_capture());
        assert!(
            keyboard
                .app
                .world()
                .get::<NativeKeyCapture>(capture)
                .is_none()
        );
        let hint = keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardSuppression>>()
            .drain()
            .last()
            .unwrap();
        assert!(!hint.all_keys);
    }

    #[test]
    fn native_chord_does_not_cross_window_focus_changes() {
        let mut keyboard = Keyboard::new();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        keyboard.update();
        keyboard
            .app
            .world_mut()
            .entity_mut(keyboard.window)
            .remove::<Active>();
        keyboard.window = keyboard
            .app
            .world_mut()
            .spawn((Window::default(), Active))
            .id();
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        let (commands, effects, _) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [true]);
    }

    #[test]
    fn native_input_from_a_retired_window_cannot_target_the_new_focus() {
        let mut keyboard = Keyboard::new();
        keyboard
            .app
            .world_mut()
            .entity_mut(keyboard.window)
            .remove::<Active>();
        keyboard.app.world_mut().spawn((Window::default(), Active));
        keyboard.press(Keyboard::combo(KeyCode::KeyK, false, true), 0);
        let (commands, effects, _) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [false]);
    }

    #[test]
    fn native_input_cannot_reuse_a_removed_keymap_stream() {
        let mut keyboard = Keyboard::new();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        keyboard.update();
        let map = keyboard
            .app
            .world()
            .get::<Keymap>(keyboard.keymap)
            .unwrap()
            .clone();
        keyboard
            .app
            .world_mut()
            .entity_mut(keyboard.keymap)
            .remove::<Keymap>();
        keyboard
            .app
            .world_mut()
            .entity_mut(keyboard.keymap)
            .insert(map);
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        keyboard.app.update();
        let world = keyboard.app.world_mut();
        assert!(
            world
                .resource_mut::<Messages<CommandInvocation>>()
                .drain()
                .next()
                .is_none()
        );
        let effects: Vec<_> = world
            .resource_mut::<Messages<KeyboardEffect>>()
            .drain()
            .collect();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].stream, keyboard.stream);
        assert_eq!(effects[0].token, keyboard.next);
        assert!(!effects[0].forward);
    }

    #[test]
    fn text_entry_keeps_leaders_but_native_command_shortcuts_still_route() {
        let mut keyboard = Keyboard::new();
        keyboard.app.world_mut().spawn(KeyboardContext {
            text_entry_owns_keys: true,
            ..Default::default()
        });
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        keyboard.press(Keyboard::combo(KeyCode::KeyK, false, true), 30);
        let (commands, effects, _) = keyboard.update();
        assert_eq!(commands, ["bar"]);
        assert_eq!(effects, [true, true, false]);
    }

    #[test]
    fn native_pass_claims_win_without_publishing_a_feature_key() {
        let mut keyboard = Keyboard::new();
        let key = NativeKey {
            key: KeyCode::KeyH,
            modifiers: KeyModifiers::default(),
        };
        keyboard
            .app
            .world_mut()
            .spawn((key, ConsumesNativeKey, Active));
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 0);
        let (commands, effects, inputs) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [false]);
        assert!(inputs[0].claim.is_some());
        let hint = keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardSuppression>>()
            .drain()
            .last()
            .unwrap();
        assert_eq!(hint.stream, keyboard.stream);
        assert!(hint.keys.contains(&key));
        keyboard
            .app
            .world_mut()
            .spawn((key, PassesNativeKey, Active));
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        let (commands, effects, inputs) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [true]);
        assert!(inputs.is_empty());
        let hint = keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardSuppression>>()
            .drain()
            .last()
            .unwrap();
        assert!(!hint.keys.contains(&key));
    }

    #[test]
    fn native_capture_repeat_is_consumed_without_publishing_an_input() {
        let mut keyboard = Keyboard::new();
        keyboard.app.world_mut().spawn(NativeKeyCapture);
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        let mut packet = keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardPress>>()
            .drain()
            .next()
            .unwrap();
        packet.input.repeat = true;
        keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardPress>>()
            .write(packet);
        let (commands, effects, inputs) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [false]);
        assert!(inputs.is_empty());
    }

    #[test]
    fn native_overflow_cancels_the_prefix_before_a_later_suffix() {
        let mut keyboard = Keyboard::new();
        keyboard.press(Keyboard::combo(KeyCode::KeyB, true, false), 0);
        keyboard.update();
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 20);
        let mut packet = keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardPress>>()
            .drain()
            .next()
            .unwrap();
        packet.overflowed = true;
        keyboard
            .app
            .world_mut()
            .resource_mut::<Messages<KeyboardPress>>()
            .write(packet);
        keyboard.press(Keyboard::combo(KeyCode::KeyH, false, false), 30);
        let (commands, effects, _) = keyboard.update();
        assert!(commands.is_empty());
        assert_eq!(effects, [true]);
    }

    #[test]
    fn fullscreen_escape_and_hide_double_tap_use_registered_feature_receivers() {
        let mut keyboard = Keyboard::new();
        keyboard
            .app
            .world_mut()
            .entity_mut(keyboard.window)
            .insert(WindowFullscreen(true));
        keyboard.press(Keyboard::combo(KeyCode::Escape, false, false), 0);
        keyboard.update();
        assert_eq!(
            keyboard
                .app
                .world_mut()
                .resource_mut::<Messages<ExitFullscreenShortcut>>()
                .drain()
                .count(),
            1
        );
        keyboard.press(Keyboard::combo(KeyCode::KeyQ, false, true), 1000);
        keyboard.update();
        assert_eq!(
            keyboard
                .app
                .world_mut()
                .resource_mut::<Messages<HideWindowsShortcut>>()
                .drain()
                .count(),
            0
        );
        keyboard.press(Keyboard::combo(KeyCode::KeyQ, false, true), 1200);
        keyboard.update();
        assert_eq!(
            keyboard
                .app
                .world_mut()
                .resource_mut::<Messages<HideWindowsShortcut>>()
                .drain()
                .count(),
            1
        );
    }
}
