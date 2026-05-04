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

    // Extra default chord bindings that the macro can't express
    // (multiple shortcuts per variant). Prefix is replaced with the
    // configured leader below.
    let placeholder_prefix = KeyCombo {
        key: KeyCode::KeyG,
        modifiers: Modifiers {
            ctrl: true,
            ..Default::default()
        },
    };
    for (key, menu_id) in [
        (KeyCode::ArrowLeft, "prev_space"),
        (KeyCode::ArrowRight, "next_space"),
    ] {
        map.bindings.push((
            Shortcut::Chord(
                placeholder_prefix.clone(),
                KeyCombo {
                    key,
                    modifiers: Modifiers::default(),
                },
            ),
            menu_id.to_string(),
        ));
    }

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
                // Strip prefix modifiers from the second key so that holding
                // Ctrl through the whole chord still works (tmux-style).
                // e.g. Ctrl+G → Ctrl+x matches chord "Ctrl+G, x".
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
                for (binding, cmd_id) in &bindings.bindings {
                    if let Shortcut::Chord(b_prefix, b_second) = binding
                        && b_prefix == prefix
                        && *b_second == effective
                    {
                        if let Some(cmd) = AppCommand::from_menu_id(cmd_id.as_str()) {
                            writer.write(cmd);
                        }
                        chord_state.pending_prefix = None;
                        suppress.0 = false;
                        return;
                    }
                }
            }
            // No chord matched — clear pending state and suppress
            chord_state.pending_prefix = None;
            suppress.0 = false;
        }

        for (binding, cmd_id) in &bindings.bindings {
            match binding {
                Shortcut::Direct(combo) if *combo == pressed => {
                    if let Some(cmd) = AppCommand::from_menu_id(cmd_id.as_str()) {
                        writer.write(cmd);
                    }
                    return;
                }
                Shortcut::Chord(prefix, _) if *prefix == pressed => {
                    chord_state.pending_prefix = Some((pressed, Instant::now()));
                    // Suppress keyboard forwarding to CEF while waiting for
                    // the second key of the chord.
                    suppress.0 = true;
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
