use super::*;
use vmux_layout::bookmark::{BookmarkContextMenuActive, BookmarkTextInputActive};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<HostFocusIntent>()
        .insert_resource(InteractionMode::User)
        .insert_resource(FocusedStack::default())
        .add_systems(Update, compute_host_focus_intent);
    app
}

fn intent(app: &App) -> HostFocusIntent {
    *app.world().resource::<HostFocusIntent>()
}

#[test]
fn web_child_of_active_stack_intends_windowed() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    let page = app.world_mut().spawn((Browser, ChildOf(stack))).id();
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::Windowed(page));
}

#[test]
fn terminal_child_of_active_stack_intends_winit_host() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, Terminal, ChildOf(stack)));
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::WinitHost);
}

#[test]
fn no_active_stack_intends_winit_host() {
    let mut app = app();
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::WinitHost);
}

#[test]
fn open_osr_command_bar_reclaims_winit_host_focus() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    app.world_mut().spawn((
        Modal,
        Node {
            display: Display::Flex,
            ..default()
        },
        CefKeyboardTarget,
    ));
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::WinitHost);
}

/// A windowed modal hosts a real DOM text field, so Chromium has to receive the keystrokes
/// itself — `send_key_event` forwarding produces no DOM key events for a windowed browser.
#[test]
fn open_windowed_command_bar_takes_native_focus() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    let modal = app
        .world_mut()
        .spawn((
            Modal,
            Node {
                display: Display::Flex,
                ..default()
            },
            CefKeyboardTarget,
            WebviewWindowed,
        ))
        .id();
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });

    app.update();

    assert_eq!(intent(&app), HostFocusIntent::Windowed(modal));
}

/// Focus must move to the bar the moment it owns input, not when its surface is revealed —
/// otherwise keys typed during the reveal frames land in the page behind it.
#[test]
fn revealing_windowed_command_bar_keeps_focus_off_the_page() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    let modal = app
        .world_mut()
        .spawn((
            Modal,
            Node {
                display: Display::Flex,
                ..default()
            },
            Visibility::Hidden,
            CefKeyboardTarget,
            WebviewWindowed,
        ))
        .id();
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });

    app.update();

    assert_eq!(intent(&app), HostFocusIntent::Windowed(modal));
}

#[test]
fn revealing_osr_command_bar_keeps_focus_off_the_page() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    app.world_mut().spawn((
        Modal,
        Node {
            display: Display::Flex,
            ..default()
        },
        Visibility::Hidden,
        CefKeyboardTarget,
    ));
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });

    app.update();

    assert_eq!(intent(&app), HostFocusIntent::WinitHost);
}

#[test]
fn bookmark_text_input_reclaims_winit_host_focus() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    app.world_mut().spawn((Browser, BookmarkTextInputActive));
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::WinitHost);
}

#[test]
fn bookmark_context_menu_reclaims_winit_host_focus() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    app.world_mut().spawn((Browser, BookmarkContextMenuActive));
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::WinitHost);
}

#[test]
fn windowed_focus_action_focuses_available_target_once() {
    let webview = Entity::from_bits(1);
    let mut focused = None;

    assert_eq!(
        windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
        Some(webview)
    );
    assert_eq!(focused, Some(webview));
    assert_eq!(
        windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
        None
    );
    assert_eq!(focused, Some(webview));
}

#[test]
fn windowed_focus_action_refocuses_after_browser_reappears() {
    let webview = Entity::from_bits(1);
    let mut focused = None;

    assert_eq!(
        windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
        Some(webview)
    );
    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(webview),
            false,
            None,
            &mut focused,
        ),
        None
    );
    assert_eq!(focused, None);
    assert_eq!(
        windowed_focus_action(HostFocusIntent::Windowed(webview), true, None, &mut focused,),
        Some(webview)
    );
}

#[test]
fn windowed_focus_action_recovers_lost_native_focus() {
    let webview = Entity::from_bits(1);
    let mut focused = Some(webview);

    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(webview),
            true,
            Some(false),
            &mut focused,
        ),
        Some(webview)
    );
}

#[test]
fn windowed_focus_action_preserves_held_native_focus() {
    let webview = Entity::from_bits(1);
    let mut focused = Some(webview);

    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(webview),
            true,
            Some(true),
            &mut focused,
        ),
        None
    );
}

#[test]
fn windowed_focus_action_focuses_changed_target() {
    let previous = Entity::from_bits(1);
    let next = Entity::from_bits(2);
    let mut focused = Some(previous);

    assert_eq!(
        windowed_focus_action(
            HostFocusIntent::Windowed(next),
            true,
            Some(false),
            &mut focused,
        ),
        Some(next)
    );
    assert_eq!(focused, Some(next));
}

#[test]
fn windowed_focus_action_clears_cache_for_winit_host() {
    let mut focused = Some(Entity::from_bits(1));

    assert_eq!(
        windowed_focus_action(HostFocusIntent::WinitHost, false, None, &mut focused),
        None
    );
    assert_eq!(focused, None);
}

#[test]
fn windowed_focus_action_clears_cache_when_unmanaged() {
    let mut focused = Some(Entity::from_bits(1));

    assert_eq!(
        windowed_focus_action(HostFocusIntent::Unmanaged, false, None, &mut focused),
        None
    );
    assert_eq!(focused, None);
}

#[test]
fn player_mode_is_unmanaged() {
    let mut app = app();
    let stack = app.world_mut().spawn_empty().id();
    app.world_mut().spawn((Browser, ChildOf(stack)));
    app.insert_resource(InteractionMode::Player);
    app.insert_resource(FocusedStack {
        stack: Some(stack),
        ..default()
    });
    app.update();
    assert_eq!(intent(&app), HostFocusIntent::Unmanaged);
}
