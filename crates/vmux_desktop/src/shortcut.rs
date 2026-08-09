use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use std::time::Instant;
pub(crate) use vmux_command::shortcut::{ChordState, KeyCombo, Modifiers, Shortcut};
use vmux_command::{
    AppCommand, BrowserCommand, OpenCommand, PaneDirection, PaneOpenMode, PaneTarget,
    WriteAppCommands,
};
use vmux_setting::{AppSettings, load_settings};

impl Plugin for ShortcutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_shortcuts.after(load_settings))
            .add_systems(Update, process_key_input.in_set(WriteAppCommands));

        #[cfg(target_os = "macos")]
        app.add_systems(
            Startup,
            crate::native_keyboard::install_native_key_monitor.after(init_shortcuts),
        )
        .add_systems(
            Startup,
            crate::event_tap::install_event_tap.after(init_shortcuts),
        )
        .add_systems(
            Update,
            crate::native_keyboard::process_monitored_keys.in_set(WriteAppCommands),
        );
    }
}

pub struct ShortcutPlugin;

#[derive(Resource, Debug, Clone)]
pub struct ShortcutMap {
    pub bindings: Vec<(Shortcut, String)>,
    pub chord_timeout_ms: u64,
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

    #[cfg(target_os = "macos")]
    crate::native_keyboard::set_shortcut_map(map.clone());

    commands.insert_resource(map);
    commands.insert_resource(ChordState::default());
}

fn process_key_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    bindings: Res<ShortcutMap>,
    mut chord_state: ResMut<ChordState>,
    mut issuer: vmux_command::CommandIssuer,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut suppress: ResMut<bevy_cef::prelude::CefSuppressKeyboardInput>,
) {
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
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
            issuer.issue(caller, cmd);
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
            issuer.issue(caller, cmd);
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
                    issuer.issue(caller, cmd);
                    chord_state.pending_prefix = None;
                    suppress.0 = false;
                    return;
                }
            }
            return;
        }
    }
}

pub(crate) fn direct_command(bindings: &ShortcutMap, pressed: &KeyCombo) -> Option<AppCommand> {
    bindings
        .bindings
        .iter()
        .find_map(|(binding, cmd_id)| match binding {
            Shortcut::Direct(combo) if combo == pressed => command_from_shortcut_id(cmd_id),
            _ => None,
        })
}

pub(crate) fn has_chord_prefix(bindings: &ShortcutMap, pressed: &KeyCombo) -> bool {
    bindings
        .bindings
        .iter()
        .any(|(binding, _)| matches!(binding, Shortcut::Chord(prefix, _) if prefix == pressed))
}

pub(crate) fn chord_command(
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
                command_from_shortcut_id(cmd_id)
            }
            _ => None,
        })
}

fn command_from_shortcut_id(cmd_id: &str) -> Option<AppCommand> {
    match cmd_id {
        "split_v" => Some(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            },
        ))),
        "split_h" => Some(AppCommand::Browser(BrowserCommand::Open(
            OpenCommand::InPane {
                direction: PaneDirection::Bottom,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            },
        ))),
        _ => AppCommand::from_menu_id(cmd_id),
    }
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
#[path = "shortcut.test.rs"]
mod tests;
