use super::*;
use bevy::ecs::message::Messages;
use vmux_command::{CommandPlugin, LayoutCommand, SpaceCommand, StackCommand, TabCommand};
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
        .insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());
    app.update();
    app
}

fn test_app_with_settings(settings: AppSettings) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin))
        .add_plugins(ShortcutPlugin)
        .insert_resource(settings)
        .insert_resource(ButtonInput::<KeyCode>::default())
        .insert_resource(bevy_cef::prelude::CefSuppressKeyboardInput::default());
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
fn configured_legacy_leader_x_emits_stack_close() {
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
        vec![AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close
        ))]
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
