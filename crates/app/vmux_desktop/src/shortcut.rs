use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use std::time::Instant;
use vmux_command::WriteAppCommands;
pub(crate) use vmux_command::shortcut::{ChordState, KeyCombo, Keymap, Modifiers};
use vmux_setting::{AppSettings, load_settings};

/// Turns key input into app commands: builds the keymap from settings, then matches
/// combos and chords against it.
pub struct ShortcutPlugin;

impl Plugin for ShortcutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::key_claim::KeyClaimPlugin)
            .add_systems(Startup, init_shortcuts.after(load_settings))
            .add_systems(Update, process_key_input.in_set(WriteAppCommands));

        #[cfg(target_os = "macos")]
        app.add_plugins(crate::native_keyboard::NativeKeyboardPlugin);
    }
}

pub(crate) fn init_shortcuts(mut commands: Commands, settings: Option<Res<AppSettings>>) {
    let map = match settings {
        Some(settings) => settings.shortcuts.keymap(),
        None => Keymap::defaults(),
    };

    #[cfg(target_os = "macos")]
    crate::native_keyboard::set_shortcut_map(map.clone());

    commands.insert_resource(map);
    commands.insert_resource(ChordState::default());
}

fn process_key_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keymap>,
    mut chord_state: ResMut<ChordState>,
    mut issuer: vmux_command::CommandIssuer,
    user: Query<Entity, With<vmux_core::team::User>>,
) {
    let caller = user.single().unwrap_or(Entity::PLACEHOLDER);
    let current_modifiers = read_current_modifiers(&keyboard);

    if let Some((_, instant)) = &chord_state.pending_prefix {
        let timeout = std::time::Duration::from_millis(bindings.chord_timeout_ms);
        if instant.elapsed() > timeout {
            chord_state.pending_prefix = None;
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
                .find_map(|pressed| bindings.chord(&prefix, pressed))
        {
            issuer.issue(caller, cmd);
            chord_state.pending_prefix = None;
            return;
        }
        if just_pressed.is_empty() {
            return;
        }
        chord_state.pending_prefix = None;
    }

    for (index, pressed) in just_pressed.iter().enumerate() {
        if let Some(cmd) = bindings.direct(pressed) {
            issuer.issue(caller, cmd);
            return;
        }
        if bindings.has_chord_prefix(pressed) {
            chord_state.pending_prefix = Some((pressed.clone(), Instant::now()));
            for (second_index, second) in just_pressed.iter().enumerate() {
                if second_index == index {
                    continue;
                }
                if let Some(cmd) = bindings.chord(pressed, second) {
                    issuer.issue(caller, cmd);
                    chord_state.pending_prefix = None;
                    return;
                }
            }
            return;
        }
    }
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
    use bevy::ecs::message::Messages;
    use vmux_command::{
        AppCommand, BrowserCommand, CommandPlugin, LayoutCommand, OpenCommand, PaneDirection,
        PaneOpenMode, PaneTarget, SpaceCommand, TabCommand,
    };
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{
        AppSettings, BrowserSettings, KeyComboDef, ShortcutDef, ShortcutEntry, ShortcutSettings,
    };

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(ShortcutPlugin)
            .insert_resource(ButtonInput::<KeyCode>::default());
        app.update();
        app
    }

    fn test_app_with_settings(settings: AppSettings) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(ShortcutPlugin)
            .insert_resource(settings)
            .insert_resource(ButtonInput::<KeyCode>::default());
        app.update();
        app
    }

    fn test_settings_with_leader(key: &str) -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
                ..Default::default()
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings { padding: 0.0 },
                pane: PaneSettings { gap: 0.0 },
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
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            recording: Default::default(),
            editor: Default::default(),
            appearance: Default::default(),
        }
    }

    fn split_settings_with_leader(key: &str) -> AppSettings {
        let mut settings = test_settings_with_leader(key);
        settings.shortcuts.bindings.push(ShortcutEntry {
            command: "split_v".into(),
            binding: ShortcutDef::Leader(KeyComboDef {
                key: "%".into(),
                ctrl: false,
                shift: false,
                alt: false,
                super_key: false,
            }),
            when: None,
        });
        settings.shortcuts.bindings.push(ShortcutEntry {
            command: "split_h".into(),
            binding: ShortcutDef::Leader(KeyComboDef {
                key: "\"".into(),
                ctrl: false,
                shift: false,
                alt: false,
                super_key: false,
            }),
            when: None,
        });
        settings
    }

    fn current_settings_with_leader(key: &str) -> AppSettings {
        let mut settings = split_settings_with_leader(key);
        settings.shortcuts.bindings.push(ShortcutEntry {
            command: "toggle_pane".into(),
            binding: ShortcutDef::Leader(KeyComboDef {
                key: "o".into(),
                ctrl: false,
                shift: false,
                alt: false,
                super_key: false,
            }),
            when: None,
        });
        settings.shortcuts.bindings.push(ShortcutEntry {
            command: "close_pane".into(),
            binding: ShortcutDef::Leader(KeyComboDef {
                key: "x".into(),
                ctrl: false,
                shift: false,
                alt: false,
                super_key: false,
            }),
            when: None,
        });
        settings
    }

    fn tab_settings_with_leader(key: &str) -> AppSettings {
        let mut settings = test_settings_with_leader(key);
        for (command, second) in [
            ("open_in_new_tab", "c"),
            ("next_tab", "n"),
            ("prev_tab", "p"),
        ] {
            settings.shortcuts.bindings.push(ShortcutEntry {
                command: command.into(),
                binding: ShortcutDef::Leader(KeyComboDef {
                    key: second.into(),
                    ctrl: false,
                    shift: false,
                    alt: false,
                    super_key: false,
                }),
                when: None,
            });
        }
        settings
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
    fn leader_h_emits_select_pane_left() {
        use vmux_command::PaneCommand;
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        app.update();

        release(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyH);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectLeft
            ))]
        );
    }

    #[test]
    fn leader_l_emits_select_pane_right() {
        use vmux_command::PaneCommand;
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        app.update();

        release(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyL);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectRight
            ))]
        );
    }

    #[test]
    fn leader_j_emits_select_pane_down() {
        use vmux_command::PaneCommand;
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        app.update();

        release(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyJ);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectDown
            ))]
        );
    }

    #[test]
    fn leader_k_emits_select_pane_up() {
        use vmux_command::PaneCommand;
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyG);
        app.update();

        release(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyK);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Pane(
                PaneCommand::SelectUp
            ))]
        );
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

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open))]
        );
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

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open))]
        );
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

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open))]
        );
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

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Space(SpaceCommand::Open))]
        );
    }

    #[test]
    fn configured_split_v_legacy_binding_emits_right_split() {
        let mut app = test_app_with_settings(split_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::ShiftLeft);
        press(&mut app, KeyCode::Digit5);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Right,
                    target: PaneTarget::NewSplit,
                    mode: PaneOpenMode::NewStack,
                    url: None,
                }
            ))]
        );
    }

    #[test]
    /// The compiled-in default binds this chord to closing the whole stack and the settings file
    /// rebinds it to closing one pane, so it is the only test here where the two disagree.
    fn configured_leader_x_overrides_the_default_stack_close() {
        use vmux_command::PaneCommand;
        let mut app = test_app_with_settings(current_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::KeyX);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Pane(PaneCommand::Close))]
        );
    }

    #[test]
    fn configured_split_h_legacy_binding_emits_bottom_split() {
        let mut app = test_app_with_settings(split_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::ShiftLeft);
        press(&mut app, KeyCode::Quote);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InPane {
                    direction: PaneDirection::Bottom,
                    target: PaneTarget::NewSplit,
                    mode: PaneOpenMode::NewStack,
                    url: None,
                }
            ))]
        );
    }

    #[test]
    fn leader_n_emits_tab_next() {
        let mut app = test_app_with_settings(tab_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::KeyN);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Tab(TabCommand::Next))]
        );
    }

    #[test]
    fn leader_p_emits_tab_previous() {
        let mut app = test_app_with_settings(tab_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::KeyP);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Layout(LayoutCommand::Tab(TabCommand::Previous))]
        );
    }

    #[test]
    fn leader_c_emits_open_in_new_tab() {
        let mut app = test_app_with_settings(tab_settings_with_leader("b"));

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();
        clear_input_frame(&mut app);

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        app.update();
        clear_input_frame(&mut app);

        press(&mut app, KeyCode::KeyC);
        app.update();

        let commands: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .drain()
            .collect();

        assert_eq!(
            commands,
            vec![AppCommand::Browser(BrowserCommand::Open(
                OpenCommand::InNewTab { url: None }
            ))]
        );
    }
}
