use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::WindowCloseRequested;
use muda::{Menu, MenuEvent, MenuItem, MenuItemKind};
use parking_lot::Mutex;
use std::sync::LazyLock;
use vmux_command::{
    AppCommand, BrowserCommand, LayoutCommand, ReadAppCommands, StackCommand, WriteAppCommands,
    build_native_root_menu, open::OpenCommand,
};
use vmux_layout::scene::InteractionMode;

/// When a menu key-equivalent last fired. ⌘W triggers the `stack_close` menu item *and* Chromium's
/// built-in ⌘W (`performClose:` → `WindowCloseRequested`). The red traffic-light button is the only
/// legitimate window close and never fires a menu event first, so we suppress a `CloseRequested`
/// that lands right after a menu command.
#[derive(Resource, Default)]
pub(crate) struct LastMenuCommandAt(pub Option<std::time::Instant>);

#[derive(Resource, Default)]
pub(crate) struct LastStackCloseAt(pub Option<std::time::Instant>);

#[derive(Resource, Default)]
pub(crate) struct LastNativePageOpenAt(pub Option<std::time::Instant>);

/// Tracks whether the app menu's Close item is currently enabled, so the sync system only touches the
/// native item when the visible-window state actually flips. Starts `true` to match the menu item's
/// build-time default (the primary window is visible at startup).
#[derive(Resource)]
pub(crate) struct CloseMenuItemEnabled(pub bool);

impl Default for CloseMenuItemEnabled {
    fn default() -> Self {
        Self(true)
    }
}

static PENDING_MENU_EVENTS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
const WINDOW_CLOSE_SUPPRESSION_WINDOW: std::time::Duration = std::time::Duration::from_millis(300);
const NATIVE_PAGE_OPEN_CLOSE_SUPPRESSION_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1500);

struct OsMenuResource {
    _menu: Menu,
    interactive_mode: Option<InteractiveModeMenuItems>,
    close_window: Option<MenuItem>,
}

struct InteractiveModeMenuItems {
    user: MenuItem,
    player: MenuItem,
}

impl InteractiveModeMenuItems {
    fn sync(&self, mode: &InteractionMode) {
        self.user.set_enabled(*mode != InteractionMode::User);
        self.player.set_enabled(*mode != InteractionMode::Player);
    }
}

pub struct OsMenuPlugin;

impl Plugin for OsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastMenuCommandAt>()
            .init_resource::<LastStackCloseAt>()
            .init_resource::<LastNativePageOpenAt>()
            .init_resource::<CloseMenuItemEnabled>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    forward_menu_events.in_set(WriteAppCommands),
                    sync_interactive_mode_menu_items.after(ReadAppCommands),
                    remember_stack_close_commands.after(WriteAppCommands),
                    remember_native_page_open_commands.after(WriteAppCommands),
                    hide_window_on_close_request
                        .after(remember_stack_close_commands)
                        .after(remember_native_page_open_commands),
                    sync_close_menu_item.after(hide_window_on_close_request),
                ),
            );
    }
}

fn setup(world: &mut World) {
    let mut menu = Menu::new();
    build_native_root_menu(&mut menu).unwrap();
    append_standard_edit_menu(&menu);
    let interactive_mode = interactive_mode_menu_items(&menu);
    let close_window = find_menu_item(menu.items(), "app_quit");

    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();

    // Native CEF views hold keyboard focus, so app shortcuts arrive as menu key-equivalents.
    // `forward_menu_events` only drains on a Bevy tick; with the loop idle that's ~1s late. Wake the
    // loop from the menu handler so the command is processed this frame.
    let proxy = world
        .get_resource::<bevy::winit::EventLoopProxyWrapper>()
        .map(|w| (**w).clone());

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        PENDING_MENU_EVENTS.lock().push(event.id.0.clone());
        if let Some(proxy) = &proxy {
            let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
        }
    }));

    world.insert_non_send(OsMenuResource {
        _menu: menu,
        interactive_mode,
        close_window,
    });
}

/// CEF/Chromium on macOS routes web-content editing shortcuts (Select All, Cut, Copy, Paste, Undo,
/// Redo) through the application's standard Edit menu — without it, cmd+A / cmd+C / cmd+V etc. do
/// nothing in web text inputs. The predefined items use the standard responder-chain selectors and
/// auto-disable when a non-text view (winit content view for terminals) holds first responder, so
/// terminal clipboard handling is unaffected.
fn append_standard_edit_menu(menu: &Menu) {
    use muda::{PredefinedMenuItem, Submenu};

    let undo = PredefinedMenuItem::undo(None);
    let redo = PredefinedMenuItem::redo(None);
    let sep = PredefinedMenuItem::separator();
    let cut = PredefinedMenuItem::cut(None);
    let copy = PredefinedMenuItem::copy(None);
    let paste = PredefinedMenuItem::paste(None);
    let select_all = PredefinedMenuItem::select_all(None);
    let Ok(edit) = Submenu::with_items(
        "Edit",
        true,
        &[&undo, &redo, &sep, &cut, &copy, &paste, &select_all],
    ) else {
        return;
    };
    let _ = menu.append(&edit);
}

fn interactive_mode_menu_items(menu: &Menu) -> Option<InteractiveModeMenuItems> {
    Some(InteractiveModeMenuItems {
        user: find_menu_item(menu.items(), "interactive_mode_user")?,
        player: find_menu_item(menu.items(), "interactive_mode_player")?,
    })
}

fn find_menu_item(items: Vec<MenuItemKind>, id: &str) -> Option<MenuItem> {
    for item in items {
        if item.id().0 == id
            && let Some(menu_item) = item.as_menuitem()
        {
            return Some(menu_item.clone());
        }
        if let Some(submenu) = item.as_submenu()
            && let Some(menu_item) = find_menu_item(submenu.items(), id)
        {
            return Some(menu_item);
        }
    }
    None
}

fn sync_interactive_mode_menu_items(
    menu: Option<NonSend<OsMenuResource>>,
    mode: Option<Res<InteractionMode>>,
) {
    let Some(mode) = mode else {
        return;
    };
    if !mode.is_changed() {
        return;
    }
    let Some(menu) = menu else {
        return;
    };
    if let Some(items) = &menu.interactive_mode {
        items.sync(&mode);
    }
}

fn sync_close_menu_item(
    menu: Option<NonSend<OsMenuResource>>,
    windows: Query<&Window>,
    mut enabled: ResMut<CloseMenuItemEnabled>,
) {
    let any_visible = windows.iter().any(|w| w.visible);
    if enabled.0 == any_visible {
        return;
    }
    enabled.0 = any_visible;
    if let Some(menu) = menu
        && let Some(item) = &menu.close_window
    {
        item.set_enabled(any_visible);
    }
}

fn forward_menu_events(world: &mut World) {
    let drained = {
        let mut events = PENDING_MENU_EVENTS.lock();
        if events.is_empty() {
            return;
        }
        std::mem::take(&mut *events)
    };

    if !drained.is_empty() {
        world.resource_mut::<LastMenuCommandAt>().0 = Some(std::time::Instant::now());
    }
    for event_id in drained {
        if event_id == "app_quit" {
            handle_quit_request(world);
        } else if let Some(cmd) = AppCommand::from_menu_id(event_id.as_str()) {
            let caller = {
                let mut q = world.query_filtered::<Entity, With<vmux_core::team::User>>();
                q.iter(world).next().unwrap_or(Entity::PLACEHOLDER)
            };
            world
                .resource_mut::<Messages<vmux_command::CommandIssued>>()
                .write(vmux_command::CommandIssued {
                    caller,
                    command: cmd.clone(),
                });
            world.resource_mut::<Messages<AppCommand>>().write(cmd);
        } else {
            crate::tray::PENDING_TRAY_EVENTS.lock().push(event_id);
        }
    }
}

fn handle_quit_request(world: &mut World) {
    world
        .resource_mut::<Messages<crate::background_lifecycle::LifecycleEvent>>()
        .write(crate::background_lifecycle::LifecycleEvent::HideAllWindows);
}

fn remember_stack_close_commands(
    mut reader: MessageReader<AppCommand>,
    mut last_stack_close: ResMut<LastStackCloseAt>,
) {
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close))
        ) {
            last_stack_close.0 = Some(std::time::Instant::now());
        }
    }
}

fn remember_native_page_open_commands(
    mut reader: MessageReader<AppCommand>,
    mut last_native_page_open: ResMut<LastNativePageOpenAt>,
) {
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Browser(BrowserCommand::Open(OpenCommand::InPlace {
                url: Some(url)
            })) if url.starts_with("vmux://")
        ) {
            last_native_page_open.0 = Some(std::time::Instant::now());
        }
    }
}

fn hide_window_on_close_request(
    mut closed: MessageReader<WindowCloseRequested>,
    mut windows: Query<&mut Window>,
    last_menu_command: Res<LastMenuCommandAt>,
    last_stack_close: Res<LastStackCloseAt>,
    last_native_page_open: Res<LastNativePageOpenAt>,
    last_tab_close: Option<Res<vmux_layout::tab::LastTabCloseAt>>,
) {
    // ⌘W fires the `stack_close` menu item but Chromium's built-in ⌘W also requests a window close.
    // Suppress a close that lands right after a menu command — the red button never does.
    let from_menu_key_equivalent = last_menu_command
        .0
        .is_some_and(|t| t.elapsed() < WINDOW_CLOSE_SUPPRESSION_WINDOW);
    let from_tab_close = last_tab_close
        .as_deref()
        .and_then(|last| last.0)
        .is_some_and(|t| t.elapsed() < WINDOW_CLOSE_SUPPRESSION_WINDOW);
    let from_stack_close = last_stack_close
        .0
        .is_some_and(|t| t.elapsed() < WINDOW_CLOSE_SUPPRESSION_WINDOW);
    let from_native_page_open = last_native_page_open
        .0
        .is_some_and(|t| t.elapsed() < NATIVE_PAGE_OPEN_CLOSE_SUPPRESSION_WINDOW);
    for event in closed.read() {
        if from_menu_key_equivalent || from_stack_close || from_tab_close || from_native_page_open {
            info!(
                target: "vmux_desktop::window_close",
                window = ?event.window,
                from_menu_key_equivalent,
                from_stack_close,
                from_tab_close,
                from_native_page_open,
                "suppressed WindowCloseRequested"
            );
            continue;
        }
        if let Ok(mut window) = windows.get_mut(event.window) {
            window.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::Window;
    use vmux_command::CommandPlugin;
    use vmux_layout::settings::{
        FocusRingSettings, LayoutSettings, PaneSettings, SideSheetSettings, WindowSettings,
    };
    use vmux_setting::{AgentSettings, AppSettings, BrowserSettings, ShortcutSettings};

    fn test_settings() -> AppSettings {
        AppSettings {
            browser: BrowserSettings {
                startup_url: "about:blank".to_string(),
            },
            layout: LayoutSettings {
                radius: 0.0,
                window: WindowSettings {
                    padding: 0.0,
                },
                pane: PaneSettings { gap: 0.0 },
                side_sheet: SideSheetSettings::default(),
                focus_ring: FocusRingSettings::default(),
            },
            shortcuts: ShortcutSettings::default(),
            terminal: None,
            auto_update: false,
            agent: AgentSettings::default(),
            spaces: Default::default(),
        }
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
        assert!(
            source.contains("from_menu_key_equivalent || from_stack_close || from_tab_close || from_native_page_open")
        );
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
}
