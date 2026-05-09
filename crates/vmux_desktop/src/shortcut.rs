use crate::command::{AppCommand, WriteAppCommands};
use crate::settings::{AppSettings, load_settings};
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use std::time::Instant;
pub(crate) use vmux_command::shortcut::{KeyCombo, Modifiers, Shortcut, resolve_key};

pub struct ShortcutPlugin;

impl Plugin for ShortcutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_shortcuts.after(load_settings))
            .add_systems(Update, process_key_input.in_set(WriteAppCommands));
    }
}

#[derive(Resource, Debug, Clone)]
pub struct ShortcutMap {
    pub bindings: Vec<(Shortcut, String)>,
    pub chord_timeout_ms: u64,
}

#[derive(Resource, Default)]
pub struct ChordState {
    pub pending_prefix: Option<(KeyCombo, Instant)>,
}

fn init_shortcuts(mut commands: Commands, settings: Option<Res<AppSettings>>) {
    let mut map = ShortcutMap {
        bindings: AppCommand::default_shortcuts(),
        chord_timeout_ms: 1000,
    };

    if let Some(settings) = settings {
        map.chord_timeout_ms = settings.shortcuts.chord_timeout_ms;

        // Parse the configured leader key
        if let Some(leader) = settings.shortcuts.leader.to_key_combo() {
            // Replace chord prefixes in default bindings with the configured leader
            for (binding, _) in &mut map.bindings {
                if let Shortcut::Chord(prefix, _) = binding {
                    *prefix = leader.clone();
                }
            }

            // Add user-specified bindings, resolving Leader(...) with the leader key
            for entry in &settings.shortcuts.bindings {
                if let Some(binding) = entry.binding.to_shortcut_with_leader(&leader) {
                    map.bindings.push((binding, entry.command.clone()));
                }
            }
        } else {
            // Leader parse failed, fall through with defaults
            for entry in &settings.shortcuts.bindings {
                if let Some(binding) = entry.binding.to_shortcut() {
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
    bindings: Res<ShortcutMap>,
    mut chord_state: ResMut<ChordState>,
    mut writer: MessageWriter<AppCommand>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
) {
    let current_modifiers = read_current_modifiers(&keyboard);

    if let Some((_, instant)) = &chord_state.pending_prefix {
        let timeout = std::time::Duration::from_millis(bindings.chord_timeout_ms);
        if instant.elapsed() > timeout {
            chord_state.pending_prefix = None;
            suppress.0 = false;
        }
    }

    let just_pressed: Vec<KeyCombo> = keyboard
        .get_just_pressed()
        .filter(|key| !is_modifier_key(**key))
        .map(|key| KeyCombo {
            key: *key,
            modifiers: current_modifiers,
        })
        .collect();

    if let Some((prefix, instant)) = chord_state.pending_prefix.clone() {
        let timeout = std::time::Duration::from_millis(bindings.chord_timeout_ms);
        if instant.elapsed() <= timeout
            && let Some(cmd) = just_pressed
                .iter()
                .find_map(|pressed| chord_command(&bindings, &prefix, pressed))
        {
            writer.write(cmd);
            chord_state.pending_prefix = None;
            suppress.0 = false;
            return;
        }
        if just_pressed.is_empty() {
            return;
        }
        chord_state.pending_prefix = None;
        suppress.0 = false;
    }

    for (index, pressed) in just_pressed.iter().enumerate() {
        if let Some(cmd) = direct_command(&bindings, pressed) {
            writer.write(cmd);
            return;
        }
        if has_chord_prefix(&bindings, pressed) {
            chord_state.pending_prefix = Some((pressed.clone(), Instant::now()));
            suppress.0 = true;
            for (second_index, second) in just_pressed.iter().enumerate() {
                if second_index == index {
                    continue;
                }
                if let Some(cmd) = chord_command(&bindings, pressed, second) {
                    writer.write(cmd);
                    chord_state.pending_prefix = None;
                    suppress.0 = false;
                    return;
                }
            }
            return;
        }
    }
}

fn direct_command(bindings: &ShortcutMap, pressed: &KeyCombo) -> Option<AppCommand> {
    bindings
        .bindings
        .iter()
        .find_map(|(binding, cmd_id)| match binding {
            Shortcut::Direct(combo) if combo == pressed => AppCommand::from_menu_id(cmd_id),
            _ => None,
        })
}

fn has_chord_prefix(bindings: &ShortcutMap, pressed: &KeyCombo) -> bool {
    bindings
        .bindings
        .iter()
        .any(|(binding, _)| matches!(binding, Shortcut::Chord(prefix, _) if prefix == pressed))
}

fn chord_command(
    bindings: &ShortcutMap,
    prefix: &KeyCombo,
    pressed: &KeyCombo,
) -> Option<AppCommand> {
    let effective = effective_chord_second(prefix, pressed);
    bindings
        .bindings
        .iter()
        .find_map(|(binding, cmd_id)| match binding {
            Shortcut::Chord(binding_prefix, second)
                if binding_prefix == prefix && second == &effective =>
            {
                AppCommand::from_menu_id(cmd_id)
            }
            _ => None,
        })
}

fn effective_chord_second(prefix: &KeyCombo, pressed: &KeyCombo) -> KeyCombo {
    let mut effective = pressed.clone();
    if prefix.modifiers.ctrl {
        effective.modifiers.ctrl = false;
    }
    if prefix.modifiers.alt {
        effective.modifiers.alt = false;
    }
    if prefix.modifiers.super_key {
        effective.modifiers.super_key = false;
    }
    effective
}

fn read_current_modifiers(keyboard: &ButtonInput<KeyCode>) -> Modifiers {
    Modifiers {
        ctrl: keyboard.pressed(KeyCode::ControlLeft)
            || keyboard.pressed(KeyCode::ControlRight)
            || keyboard.just_pressed(KeyCode::ControlLeft)
            || keyboard.just_pressed(KeyCode::ControlRight),
        shift: keyboard.pressed(KeyCode::ShiftLeft)
            || keyboard.pressed(KeyCode::ShiftRight)
            || keyboard.just_pressed(KeyCode::ShiftLeft)
            || keyboard.just_pressed(KeyCode::ShiftRight),
        alt: keyboard.pressed(KeyCode::AltLeft)
            || keyboard.pressed(KeyCode::AltRight)
            || keyboard.just_pressed(KeyCode::AltLeft)
            || keyboard.just_pressed(KeyCode::AltRight),
        super_key: keyboard.pressed(KeyCode::SuperLeft)
            || keyboard.pressed(KeyCode::SuperRight)
            || keyboard.just_pressed(KeyCode::SuperLeft)
            || keyboard.just_pressed(KeyCode::SuperRight),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CommandPlugin, SpaceCommand};
    use crate::settings::{
        AppSettings, BrowserSettings, FocusRingSettings, KeyComboDef, LayoutSettings, PaneSettings,
        ShortcutSettings, SideSheetSettings, WindowSettings,
    };
    use bevy::ecs::message::Messages;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_plugins(ShortcutPlugin);
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());
        app.update();
        app
    }

    fn test_app_with_settings(settings: AppSettings) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_plugins(ShortcutPlugin);
        app.insert_resource(settings);
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());
        app.update();
        app
    }

    fn test_settings_with_leader(key: &str) -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                window: WindowSettings {
                    padding: 0.0,
                    padding_top: None,
                    padding_right: None,
                    padding_bottom: None,
                    padding_left: None,
                },
                pane: PaneSettings {
                    gap: 0.0,
                    radius: 0.0,
                },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings {
                leader: KeyComboDef {
                    key: key.to_string(),
                    ctrl: true,
                    shift: false,
                    alt: false,
                    super_key: false,
                },
                ..Default::default()
            },
            terminal: None,
            auto_update: false,
        }
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
    }

    fn release(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
    }

    fn clear_input_frame(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    #[test]
    fn leader_s_emits_space_open_command() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        app.update();

        release(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyS);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(commands, vec![AppCommand::Space(SpaceCommand::Open)]);
    }

    #[test]
    fn leader_chord_emits_when_prefix_and_key_arrive_in_same_frame() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        press(&mut app, KeyCode::KeyS);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(commands, vec![AppCommand::Space(SpaceCommand::Open)]);
    }

    #[test]
    fn leader_chord_emits_when_prefix_is_released_before_same_frame_update() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyS);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(commands, vec![AppCommand::Space(SpaceCommand::Open)]);
    }

    #[test]
    fn configured_leader_s_survives_prefix_release_frame() {
        let mut app = test_app_with_settings(test_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::KeyS);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(commands, vec![AppCommand::Space(SpaceCommand::Open)]);
    }
}
