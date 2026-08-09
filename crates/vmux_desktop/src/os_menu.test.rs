use super::*;
use bevy::window::Window;
use vmux_command::CommandPlugin;
use vmux_layout::settings::{
    FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
};
use vmux_setting::{AgentSettings, AppSettings, BrowserSettings, ShortcutSettings};

#[cfg(target_os = "macos")]
#[test]
fn edit_items_disabled_only_for_terminal_focus() {
    assert!(
        !edit_menu_items_enabled(HostFocusIntent::WinitHost),
        "terminal focus must release ⌘C/⌘V to the terminal's own handler"
    );
    assert!(edit_menu_items_enabled(HostFocusIntent::Windowed(
        Entity::PLACEHOLDER
    )));
    assert!(edit_menu_items_enabled(HostFocusIntent::Unmanaged));
}

fn test_settings() -> AppSettings {
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
        shortcuts: ShortcutSettings::default(),
        terminal: None,
        auto_update: false,
        agent: AgentSettings::default(),
        spaces: Default::default(),
        recording: Default::default(),
        editor: Default::default(),
        appearance: Default::default(),
    }
}

#[test]
fn root_menu_titles_use_requested_locale() {
    let titles = [
        "Scene", "Layout", "Terminal", "Browser", "Service", "Bookmark", "Edit",
    ]
    .into_iter()
    .filter_map(|title| localized_submenu_title(title, "en-US", "ja"))
    .collect::<Vec<_>>();
    assert_eq!(
        titles,
        [
            "シーン",
            "レイアウト",
            "ターミナル",
            "ブラウザ",
            "サービス",
            "ブックマーク",
            "編集",
        ]
    );
}

#[test]
fn quit_menu_event_hides_windows_not_exit() {
    let source = include_str!("os_menu.rs");
    let needle = ["AppExit", "::", "Success"].concat();
    assert!(
        !source.contains(&needle),
        "Cmd+Q must hide windows, not exit the app — terminal state must survive"
    );
    assert!(
        source.contains("HideAllWindows") || source.contains("window.visible = false"),
        "handle_quit_request must dispatch a hide action"
    );
}

#[test]
fn window_close_request_hides_window_instead_of_despawning() {
    let source = include_str!("os_menu.rs");
    let despawn_marker = ["Closing", "Window"].concat();
    let inserts = source.matches(&format!("insert({despawn_marker})")).count()
        + source
            .matches(&format!("try_insert({despawn_marker})"))
            .count();
    assert_eq!(
        inserts, 0,
        "WindowCloseRequested must hide the window, not insert ClosingWindow which leads to despawn"
    );
    assert!(
        source.contains("window.visible = false") || source.contains(".visible = false"),
        "expected the close handler to set window.visible = false"
    );
}

#[test]
fn window_close_hides_without_quit_confirmation() {
    let source = include_str!("os_menu.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("production source");

    assert!(
        !source.contains("PendingWindowClose"),
        "window close must not route through a confirmation dialog"
    );
    assert!(!source.contains("process_pending_window_close"));
    assert!(!source.contains("should_confirm"));
    assert!(!source.contains("confirm_quit_dialog"));
}

#[test]
fn unsuppressed_window_close_hides_window() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
        .add_message::<WindowCloseRequested>()
        .insert_resource(test_settings());

    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .resource_mut::<Messages<WindowCloseRequested>>()
        .write(WindowCloseRequested { window });

    app.world_mut().run_schedule(Update);

    assert!(!app.world().get::<Window>(window).unwrap().visible);
}

#[test]
fn window_close_request_after_tab_close_is_suppressed() {
    let source = include_str!("os_menu.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("production source");

    assert!(source.contains("LastTabCloseAt"));
    assert!(source.contains("from_tab_close"));
    assert!(source.contains(
        "from_menu_key_equivalent || from_stack_close || from_tab_close || from_native_page_open"
    ));
}

#[test]
fn window_close_request_after_stack_close_command_is_suppressed() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
        .add_message::<WindowCloseRequested>()
        .insert_resource(test_settings());

    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Layout(LayoutCommand::Stack(
            StackCommand::Close,
        )));
    app.world_mut()
        .resource_mut::<Messages<WindowCloseRequested>>()
        .write(WindowCloseRequested { window });

    app.world_mut().run_schedule(Update);

    assert!(app.world().get::<Window>(window).unwrap().visible);
}

#[test]
fn window_close_request_after_native_page_open_is_suppressed() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
        .add_message::<WindowCloseRequested>()
        .insert_resource(test_settings());

    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .resource_mut::<Messages<AppCommand>>()
        .write(AppCommand::Browser(vmux_command::BrowserCommand::Open(
            vmux_command::open::OpenCommand::InPlace {
                url: Some("vmux://terminal".to_string()),
            },
        )));
    app.world_mut()
        .resource_mut::<Messages<WindowCloseRequested>>()
        .write(WindowCloseRequested { window });

    app.world_mut().run_schedule(Update);

    assert!(app.world().get::<Window>(window).unwrap().visible);
}

#[test]
fn delayed_window_close_request_after_native_page_open_is_suppressed() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
        .add_message::<WindowCloseRequested>()
        .insert_resource(test_settings());

    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut().resource_mut::<LastNativePageOpenAt>().0 =
        Some(std::time::Instant::now() - std::time::Duration::from_millis(1000));
    app.world_mut()
        .resource_mut::<Messages<WindowCloseRequested>>()
        .write(WindowCloseRequested { window });

    app.world_mut().run_schedule(Update);

    assert!(app.world().get::<Window>(window).unwrap().visible);
}

#[test]
fn close_menu_item_disabled_when_all_windows_hidden() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CommandPlugin, OsMenuPlugin))
        .add_message::<WindowCloseRequested>()
        .insert_resource(test_settings());

    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut().run_schedule(Update);
    assert!(
        app.world().resource::<CloseMenuItemEnabled>().0,
        "a visible window means Close is enabled"
    );

    app.world_mut().get_mut::<Window>(window).unwrap().visible = false;
    app.world_mut().run_schedule(Update);
    assert!(
        !app.world().resource::<CloseMenuItemEnabled>().0,
        "all windows hidden means Close is disabled"
    );

    app.world_mut().get_mut::<Window>(window).unwrap().visible = true;
    app.world_mut().run_schedule(Update);
    assert!(
        app.world().resource::<CloseMenuItemEnabled>().0,
        "showing a window re-enables Close"
    );
}

#[test]
fn interactive_mode_menu_disables_selected_mode() {
    let source = include_str!("os_menu.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("production source");

    assert!(source.contains("interactive_mode_user"));
    assert!(source.contains("interactive_mode_player"));
    assert!(source.contains("set_enabled(*mode != InteractionMode::User)"));
    assert!(source.contains("set_enabled(*mode != InteractionMode::Player)"));
}
