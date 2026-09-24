use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use std::time::Instant;
use vmux_command::WriteCommandRequests;
use vmux_command::shortcut::{Binding, Source, When};
pub(crate) use vmux_command::shortcut::{ChordState, KeyCombo, Keymap, Modifiers};
use vmux_setting::{AppSettings, SettingsLoadSet};

pub struct ShortcutPlugin;

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ShortcutInit;

impl Plugin for ShortcutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChordState>()
            .add_plugins(crate::key_claim::KeyClaimPlugin)
            .add_systems(
                Startup,
                sync_keymap
                    .in_set(ShortcutInit)
                    .after(SettingsLoadSet)
                    .after(vmux_command::RegisterCommandDefinitions),
            )
            .add_systems(Update, sync_keymap)
            .add_systems(Update, process_key_input.in_set(WriteCommandRequests));

        #[cfg(target_os = "macos")]
        app.add_plugins(crate::keyboard::KeyboardPlugin);
    }
}

fn sync_keymap(
    settings: Option<Res<AppSettings>>,
    definitions: Query<Ref<vmux_command::CommandDefinition>>,
    mut keymap: ResMut<Keymap>,
) {
    let definitions_changed = definitions
        .iter()
        .any(|definition| definition.is_added() || definition.is_changed());
    let settings_changed = settings
        .as_ref()
        .is_some_and(|settings| settings.is_changed());
    if !keymap.is_added() && !definitions_changed && !settings_changed {
        return;
    }

    let definitions = definitions
        .iter()
        .map(|definition| (*definition).clone())
        .collect::<Vec<_>>();
    let mut next = Keymap::defaults_with(&definitions);
    if let Some(settings) = settings {
        let leader = settings.shortcuts.leader.to_key_combo();
        next.chord_timeout_ms = settings.shortcuts.chord_timeout_ms;
        if let Some(leader) = &leader {
            next.set_leader(leader);
        }

        let mut configured = Vec::new();
        for entry in &settings.shortcuts.bindings {
            let shortcut = match leader.as_ref() {
                Some(leader) => entry.binding.to_shortcut_with_leader(leader),
                None => entry.binding.to_shortcut(),
            };
            let Some(shortcut) = shortcut else { continue };
            configured.push(Binding {
                shortcut,
                command: entry.command.clone(),
                when: entry.when.as_deref().and_then(When::parse),
            });
        }
        next.extend(Source::Settings, configured);
    }

    *keymap = next;
}

fn process_key_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    bindings: Res<Keymap>,
    mut chord_state: ResMut<ChordState>,
    mut invocations: MessageWriter<vmux_command::CommandInvocation>,
    user: Query<Entity, With<vmux_core::team::User>>,
    capture: Option<Res<vmux_shortcut::ShortcutCaptureTarget>>,
) {
    if capture
        .as_deref()
        .is_some_and(|capture| capture.is_active())
    {
        chord_state.pending_prefix = None;
        return;
    }
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
            invocations.write(vmux_command::CommandInvocation::new(caller, cmd));
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
            invocations.write(vmux_command::CommandInvocation::new(caller, cmd));
            return;
        }
        if bindings.has_chord_prefix(pressed) {
            chord_state.pending_prefix = Some((pressed.clone(), Instant::now()));
            for (second_index, second) in just_pressed.iter().enumerate() {
                if second_index == index {
                    continue;
                }
                if let Some(cmd) = bindings.chord(pressed, second) {
                    invocations.write(vmux_command::CommandInvocation::new(caller, cmd));
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
    use vmux_api::open_target::{PaneDirection, PaneOpenMode, PaneTarget};
    use vmux_command::{CommandInvocation, CommandPlugin};
    use vmux_layout::pane::{PaneArrangement, PaneFocus, PaneOpenRequest, PaneRequest};
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_layout::tab::{
        FocusRequest as TabFocusRequest, OpenRequest as TabOpenRequest, TabFocus,
    };
    use vmux_layout::target::SiblingDirection;
    use vmux_setting::{
        AppSettings, BrowserSettings, KeyComboDef, ShortcutDef, ShortcutEntry, ShortcutSettings,
    };

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(ShortcutPlugin)
            .add_plugins((
                vmux_command::CommandTypePlugin::<PaneRequest>::default(),
                vmux_command::CommandTypePlugin::<TabFocusRequest>::default(),
                vmux_command::CommandTypePlugin::<TabOpenRequest>::default(),
            ))
            .insert_resource(ButtonInput::<KeyCode>::default());
        app.world_mut().spawn(
            vmux_command::CommandDefinition::new("space_open", "Spaces", "Layout > Space")
                .chord("Ctrl+b, s"),
        );
        app.update();
        app
    }

    fn test_app_with_settings(settings: AppSettings) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandPlugin))
            .add_plugins(ShortcutPlugin)
            .add_plugins((
                vmux_command::CommandTypePlugin::<PaneRequest>::default(),
                vmux_command::CommandTypePlugin::<TabFocusRequest>::default(),
                vmux_command::CommandTypePlugin::<TabOpenRequest>::default(),
            ))
            .insert_resource(settings)
            .insert_resource(ButtonInput::<KeyCode>::default());
        app.world_mut().spawn(
            vmux_command::CommandDefinition::new("space_open", "Spaces", "Layout > Space")
                .chord("Ctrl+b, s"),
        );
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
            update_channel: Default::default(),
            agent: vmux_setting::AgentSettings::default(),
            spaces: Default::default(),
            projects: Default::default(),
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
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyH);
        app.update();

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![PaneRequest::Focus(PaneFocus::Direction(
                PaneDirection::Left
            ))]
        );
    }

    #[test]
    fn leader_l_emits_select_pane_right() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyL);
        app.update();

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![PaneRequest::Focus(PaneFocus::Direction(
                PaneDirection::Right
            ))]
        );
    }

    #[test]
    fn leader_j_emits_select_pane_down() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyJ);
        app.update();

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![PaneRequest::Focus(PaneFocus::Direction(
                PaneDirection::Bottom
            ))]
        );
    }

    #[test]
    fn leader_k_emits_select_pane_up() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyK);
        app.update();

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![PaneRequest::Focus(PaneFocus::Direction(PaneDirection::Top))]
        );
    }

    #[test]
    fn leader_s_emits_space_open_command() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        app.update();

        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        clear_input_frame(&mut app);
        press(&mut app, KeyCode::KeyS);
        app.update();

        let invocations: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .drain()
            .collect();

        assert_eq!(invocations[0].id, "space_open");
    }

    #[test]
    fn leader_chord_emits_when_prefix_and_key_arrive_in_same_frame() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        press(&mut app, KeyCode::KeyS);
        app.update();

        let invocations: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .drain()
            .collect();

        assert_eq!(invocations[0].id, "space_open");
    }

    #[test]
    fn leader_chord_emits_when_prefix_is_released_before_same_frame_update() {
        let mut app = test_app();

        press(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::KeyB);
        release(&mut app, KeyCode::ControlLeft);
        press(&mut app, KeyCode::KeyS);
        app.update();

        let invocations: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .drain()
            .collect();

        assert_eq!(invocations[0].id, "space_open");
    }

    #[test]
    fn default_tmux_rotate_and_mirror_chords_emit_commands() {
        for (key, expected) in [
            (
                KeyCode::KeyR,
                PaneRequest::Arrange(PaneArrangement::Rotate(SiblingDirection::Next)),
            ),
            (
                KeyCode::KeyM,
                PaneRequest::Arrange(PaneArrangement::Mirror(None)),
            ),
        ] {
            let mut app = test_app();
            press(&mut app, KeyCode::ControlLeft);
            press(&mut app, KeyCode::KeyB);
            app.update();

            release(&mut app, KeyCode::KeyB);
            release(&mut app, KeyCode::ControlLeft);
            clear_input_frame(&mut app);
            press(&mut app, key);
            app.update();

            let requests: Vec<_> = app
                .world_mut()
                .resource_mut::<Messages<PaneRequest>>()
                .drain()
                .collect();

            assert_eq!(requests, vec![expected]);
        }
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

        let invocations: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CommandInvocation>>()
            .drain()
            .collect();

        assert_eq!(invocations[0].id, "space_open");
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

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![PaneRequest::Open(PaneOpenRequest {
                direction: PaneDirection::Right,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            })]
        );
    }

    #[test]
    fn configured_leader_x_overrides_the_default_stack_close() {
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

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(requests, vec![PaneRequest::Close]);
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

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PaneRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![PaneRequest::Open(PaneOpenRequest {
                direction: PaneDirection::Bottom,
                target: PaneTarget::NewSplit,
                mode: PaneOpenMode::NewStack,
                url: None,
            })]
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

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<TabFocusRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![TabFocusRequest(TabFocus::Sibling(SiblingDirection::Next))]
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

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<TabFocusRequest>>()
            .drain()
            .collect();

        assert_eq!(
            requests,
            vec![TabFocusRequest(TabFocus::Sibling(
                SiblingDirection::Previous
            ))]
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

        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<TabOpenRequest>>()
            .drain()
            .collect();

        assert_eq!(requests, vec![TabOpenRequest { url: None }]);
    }
}
